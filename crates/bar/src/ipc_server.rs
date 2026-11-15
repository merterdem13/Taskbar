use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::core::{Result, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE, HWND, LPARAM, WAIT_OBJECT_0,
    WPARAM,
};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::Threading::{
    CreateEventW, ResetEvent, SetEvent, WaitForMultipleObjects, INFINITE,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_USER};

use core::{decode_request, decode_response, encode_request, encode_response, Config, IpcRequest, IpcResponse};

pub const WM_IPC_EVENT: u32 = WM_USER + 3;

#[derive(Debug, Clone, PartialEq)]
pub enum IpcEvent {
    PreviewConfig(Config),
    RevertPreview,
    SaveConfig(Config),
    ShutdownForUpdate,
    ClientDisconnected,
}

pub type SharedIpcQueue = Arc<Mutex<VecDeque<IpcEvent>>>;

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() && !self.0.0.is_null() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

pub struct IpcServerHandle {
    stop: Arc<AtomicBool>,
    stop_event_raw: isize,
    thread: Option<JoinHandle<()>>,
}

impl Drop for IpcServerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let stop_ev = HANDLE(self.stop_event_raw as *mut std::ffi::c_void);
        unsafe {
            let _ = SetEvent(stop_ev);
        }
        if let Some(join) = self.thread.take() {
            let _ = join.join();
        }
        unsafe {
            let _ = CloseHandle(stop_ev);
        }
    }
}

enum WaitOutcome {
    Completed(u32),
    Stopped,
    Failed,
}

unsafe fn wait_overlapped(
    pipe: HANDLE,
    ov: &OVERLAPPED,
    io_event: HANDLE,
    stop_event: HANDLE,
) -> WaitOutcome {
    unsafe {
        let wait = WaitForMultipleObjects(&[io_event, stop_event], false, INFINITE);
        if wait == WAIT_OBJECT_0 {
            let mut bytes = 0u32;
            if GetOverlappedResult(pipe, ov, &mut bytes, false).is_ok() {
                WaitOutcome::Completed(bytes)
            } else {
                WaitOutcome::Failed
            }
        } else {
            let _ = CancelIoEx(pipe, Some(ov));
            let mut bytes = 0u32;
            let _ = GetOverlappedResult(pipe, ov, &mut bytes, true);
            WaitOutcome::Stopped
        }
    }
}

unsafe fn connect_pipe_overlapped(
    pipe: HANDLE,
    io_event: HANDLE,
    stop_event: HANDLE,
) -> Option<bool> {
    unsafe {
        let _ = ResetEvent(io_event);
        let mut ov = OVERLAPPED {
            hEvent: io_event,
            ..Default::default()
        };
        match ConnectNamedPipe(pipe, Some(&mut ov)) {
            Ok(()) => Some(true),
            Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => Some(true),
            Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
                match wait_overlapped(pipe, &ov, io_event, stop_event) {
                    WaitOutcome::Completed(_) => Some(true),
                    WaitOutcome::Stopped => None,
                    WaitOutcome::Failed => Some(false),
                }
            }
            Err(_) => Some(false),
        }
    }
}

unsafe fn read_pipe_overlapped(
    pipe: HANDLE,
    buf: &mut [u8],
    io_event: HANDLE,
    stop_event: HANDLE,
) -> Option<usize> {
    unsafe {
        let _ = ResetEvent(io_event);
        let mut ov = OVERLAPPED {
            hEvent: io_event,
            ..Default::default()
        };
        let mut read = 0u32;
        match ReadFile(pipe, Some(buf), Some(&mut read), Some(&mut ov)) {
            Ok(()) => {
                if read > 0 {
                    Some(read as usize)
                } else {
                    None
                }
            }
            Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
                match wait_overlapped(pipe, &ov, io_event, stop_event) {
                    WaitOutcome::Completed(n) if n > 0 => Some(n as usize),
                    _ => None,
                }
            }
            Err(_) => None,
        }
    }
}

unsafe fn write_all_overlapped(pipe: HANDLE, data: &[u8], io_event: HANDLE) -> bool {
    unsafe {
        let mut offset = 0usize;
        while offset < data.len() {
            let _ = ResetEvent(io_event);
            let mut ov = OVERLAPPED {
                hEvent: io_event,
                ..Default::default()
            };
            let mut written = 0u32;
            let ok = match WriteFile(
                pipe,
                Some(&data[offset..]),
                Some(&mut written),
                Some(&mut ov),
            ) {
                Ok(()) => true,
                Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {
                    GetOverlappedResult(pipe, &ov, &mut written, true).is_ok()
                }
                Err(_) => false,
            };
            if !ok || written == 0 {
                return false;
            }
            offset += written as usize;
        }
        true
    }
}

fn push_ipc_event(queue: &SharedIpcQueue, target_hwnd: HWND, event: IpcEvent) {
    if let Ok(mut guard) = queue.lock() {
        guard.push_back(event);
    }
    unsafe {
        let _ = PostMessageW(Some(target_hwnd), WM_IPC_EVENT, WPARAM(0), LPARAM(0));
    }
}

fn handle_request_line(
    line: &str,
    queue: &SharedIpcQueue,
    target_hwnd: HWND,
    current_cfg: &Arc<Mutex<Config>>,
    client_had_active_preview: &mut bool,
) -> IpcResponse {
    match decode_request(line) {
        Ok(IpcRequest::PreviewConfig { config, .. }) => {
            *client_had_active_preview = true;
            if let Ok(mut guard) = current_cfg.lock() {
                *guard = config.clone().clamped();
            }
            push_ipc_event(queue, target_hwnd, IpcEvent::PreviewConfig(config));
            IpcResponse::Ok { config: None }
        }
        Ok(IpcRequest::RevertPreview { .. }) => {
            *client_had_active_preview = false;
            push_ipc_event(queue, target_hwnd, IpcEvent::RevertPreview);
            IpcResponse::Ok { config: None }
        }
        Ok(IpcRequest::SaveConfig { config, .. }) => {
            *client_had_active_preview = false;
            if let Ok(mut guard) = current_cfg.lock() {
                *guard = config.clone().clamped();
            }
            push_ipc_event(queue, target_hwnd, IpcEvent::SaveConfig(config));
            IpcResponse::Ok { config: None }
        }
        Ok(IpcRequest::GetActiveConfig { .. }) => {
            let cfg = current_cfg
                .lock()
                .map(|g| g.clone())
                .unwrap_or_else(|_| Config::default());
            IpcResponse::Ok { config: Some(cfg) }
        }
        Ok(IpcRequest::ShutdownForUpdate { .. }) => {
            *client_had_active_preview = false;
            push_ipc_event(queue, target_hwnd, IpcEvent::ShutdownForUpdate);
            IpcResponse::Ok { config: None }
        }
        Err(msg) => IpcResponse::Error { message: msg },
    }
}

pub fn start_ipc_server(
    pipe_name: &str,
    hwnd: HWND,
    queue: SharedIpcQueue,
    current_cfg: Arc<Mutex<Config>>,
) -> Result<IpcServerHandle> {
    let stop_event = unsafe { CreateEventW(None, true, false, PCWSTR::null())? };
    let io_event = match unsafe { CreateEventW(None, true, false, PCWSTR::null()) } {
        Ok(ev) => ev,
        Err(e) => {
            unsafe {
                let _ = CloseHandle(stop_event);
            }
            return Err(e);
        }
    };

    let stop_event_raw = stop_event.0 as isize;
    let io_event_raw = io_event.0 as isize;
    let raw_hwnd = hwnd.0 as isize;
    let wide_name: Vec<u16> = std::ffi::OsStr::new(pipe_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);

    let thread = thread::Builder::new()
        .name("ipc-server".to_string())
        .spawn(move || {
            let _io_guard = OwnedHandle(HANDLE(io_event_raw as *mut std::ffi::c_void));
            let io_ev = _io_guard.0;
            let stop_ev = HANDLE(stop_event_raw as *mut std::ffi::c_void);
            let target_hwnd = HWND(raw_hwnd as *mut std::ffi::c_void);

            while !stop_thread.load(Ordering::SeqCst) {
                let pipe_handle = unsafe {
                    CreateNamedPipeW(
                        PCWSTR(wide_name.as_ptr()),
                        PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
                        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                        PIPE_UNLIMITED_INSTANCES,
                        65536,
                        65536,
                        0,
                        None,
                    )
                };
                if pipe_handle.is_invalid() {
                    thread::sleep(Duration::from_millis(50));
                    continue;
                }
                let _pipe_guard = OwnedHandle(pipe_handle);

                match unsafe { connect_pipe_overlapped(pipe_handle, io_ev, stop_ev) } {
                    None => break,
                    Some(false) => continue,
                    Some(true) => {}
                }

                if stop_thread.load(Ordering::SeqCst) {
                    unsafe {
                        let _ = DisconnectNamedPipe(pipe_handle);
                    }
                    break;
                }

                let mut client_had_active_preview = false;
                let mut line_buf = Vec::<u8>::new();
                let mut chunk = [0u8; 4096];

                while !stop_thread.load(Ordering::SeqCst) {
                    let Some(n) =
                        (unsafe { read_pipe_overlapped(pipe_handle, &mut chunk, io_ev, stop_ev) })
                    else {
                        break;
                    };
                    line_buf.extend_from_slice(&chunk[..n]);

                    while let Some(pos) = line_buf.iter().position(|&b| b == b'\n') {
                        let raw_line: Vec<u8> = line_buf.drain(..=pos).collect();
                        let resp = match std::str::from_utf8(&raw_line) {
                            Ok(text) => handle_request_line(
                                text,
                                &queue,
                                target_hwnd,
                                &current_cfg,
                                &mut client_had_active_preview,
                            ),
                            Err(e) => IpcResponse::Error {
                                message: format!("invalid UTF-8 IPC payload: {e}"),
                            },
                        };
                        if let Ok(encoded) = encode_response(&resp) {
                            if !unsafe { write_all_overlapped(pipe_handle, &encoded, io_ev) } {
                                break;
                            }
                        }
                    }
                }

                if client_had_active_preview && !stop_thread.load(Ordering::SeqCst) {
                    push_ipc_event(&queue, target_hwnd, IpcEvent::ClientDisconnected);
                }

                unsafe {
                    let _ = DisconnectNamedPipe(pipe_handle);
                }
            }
        })
        .expect("spawn ipc-server thread");

    Ok(IpcServerHandle {
        stop,
        stop_event_raw,
        thread: Some(thread),
    })
}

pub fn send_ipc_request_once(
    pipe_name: &str,
    req: &IpcRequest,
) -> std::result::Result<IpcResponse, String> {
    let mut attempts = 0;
    let mut file = loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(pipe_name)
        {
            Ok(f) => break f,
            Err(e) if e.raw_os_error() == Some(231) && attempts < 10 => {
                attempts += 1;
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(format!("open pipe {pipe_name}: {e}")),
        }
    };

    let bytes = encode_request(req)?;
    file.write_all(&bytes)
        .map_err(|e| format!("write pipe {pipe_name}: {e}"))?;
    file.flush()
        .map_err(|e| format!("flush pipe {pipe_name}: {e}"))?;

    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read pipe {pipe_name}: {e}"))?;
    decode_response(&line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{decode_response, encode_request, Edge, IPC_PROTOCOL_VERSION};
    use std::io::{BufRead, BufReader, Write};
    use std::time::{Duration, Instant};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, PeekMessageW, RegisterClassW, MSG,
        PM_REMOVE, WNDCLASSW, WS_POPUP,
    };

    unsafe extern "system" fn dummy_wndproc(
        hwnd: HWND,
        msg: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }

    fn pump_until_event(
        hwnd: HWND,
        queue: &SharedIpcQueue,
        timeout: Duration,
        predicate: impl Fn(&[IpcEvent]) -> bool,
    ) -> Vec<IpcEvent> {
        let mut collected = Vec::new();
        let start = Instant::now();
        while start.elapsed() < timeout {
            let mut msg = MSG::default();
            unsafe {
                while PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_IPC_EVENT {
                        let mut q = queue.lock().unwrap();
                        collected.extend(q.drain(..));
                    }
                }
            }
            {
                let mut q = queue.lock().unwrap();
                collected.extend(q.drain(..));
            }
            if predicate(&collected) {
                return collected;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        collected
    }

    #[test]
    fn ipc_server_handles_preview_save_and_auto_reverts_on_client_drop() {
        let pipe_name = format!(r"\\.\pipe\TaskbarIpcTest_{}", std::process::id());
        let queue: SharedIpcQueue = Arc::new(Mutex::new(VecDeque::new()));
        let current_cfg = Arc::new(Mutex::new(Config::default()));

        let hwnd = unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).expect("module handle");
            let class = w!("RustTaskbarIpcServerTestWindow");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(dummy_wndproc),
                hInstance: hi.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            CreateWindowExW(
                Default::default(),
                class,
                w!("ipc_test"),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(hi.into()),
                None,
            )
            .expect("create window")
        };

        let server = start_ipc_server(&pipe_name, hwnd, Arc::clone(&queue), Arc::clone(&current_cfg))
            .expect("start ipc server");

        // Give pipe thread a moment to create the pipe instance
        std::thread::sleep(Duration::from_millis(40));

        // 1. Connect persistent client, send PreviewConfig, then drop without saving
        {
            let mut client = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&pipe_name)
                .expect("open pipe client");
            let mut preview_cfg = Config::default();
            preview_cfg.bar.edge = Edge::Right;
            let req = IpcRequest::PreviewConfig {
                version: IPC_PROTOCOL_VERSION,
                config: preview_cfg,
            };
            let bytes = encode_request(&req).unwrap();
            client.write_all(&bytes).unwrap();
            client.flush().unwrap();

            let mut reader = BufReader::new(client.try_clone().unwrap());
            let mut resp_line = String::new();
            reader.read_line(&mut resp_line).unwrap();
            let resp = decode_response(&resp_line).unwrap();
            assert!(matches!(resp, IpcResponse::Ok { .. }));

            let events = pump_until_event(hwnd, &queue, Duration::from_secs(2), |evs| {
                evs.iter()
                    .any(|e| matches!(e, IpcEvent::PreviewConfig(c) if c.bar.edge == Edge::Right))
            });
            assert!(
                events
                    .iter()
                    .any(|e| matches!(e, IpcEvent::PreviewConfig(c) if c.bar.edge == Edge::Right)),
                "expected PreviewConfig event, got {events:?}"
            );
        }

        // Dropping client handle without SaveConfig must trigger ClientDisconnected
        let disconnect_events = pump_until_event(hwnd, &queue, Duration::from_secs(2), |evs| {
            evs.iter()
                .any(|e| matches!(e, IpcEvent::ClientDisconnected))
        });
        assert!(
            disconnect_events
                .iter()
                .any(|e| matches!(e, IpcEvent::ClientDisconnected)),
            "expected ClientDisconnected on drop, got {disconnect_events:?}"
        );

        // 2. One-shot SaveConfig request works and does NOT emit ClientDisconnected
        let mut saved_cfg = Config::default();
        saved_cfg.bar.edge = Edge::Bottom;
        let resp = send_ipc_request_once(
            &pipe_name,
            &IpcRequest::SaveConfig {
                version: IPC_PROTOCOL_VERSION,
                config: saved_cfg,
            },
        )
        .expect("send SaveConfig");
        assert!(matches!(resp, IpcResponse::Ok { .. }));

        let save_events = pump_until_event(hwnd, &queue, Duration::from_secs(2), |evs| {
            evs.iter()
                .any(|e| matches!(e, IpcEvent::SaveConfig(c) if c.bar.edge == Edge::Bottom))
        });
        assert!(save_events
            .iter()
            .any(|e| matches!(e, IpcEvent::SaveConfig(c) if c.bar.edge == Edge::Bottom)));

        drop(server);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}
