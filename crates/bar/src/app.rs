use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Shell::ABN_POSCHANGED;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, LoadCursorW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetForegroundWindow, SetTimer, ShowWindow, TrackPopupMenu,
    TranslateMessage, IDC_ARROW, MA_NOACTIVATE, MF_STRING, MSG, SW_SHOWNOACTIVATE, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NULL, WM_PAINT, WM_POWERBROADCAST,
    WM_RBUTTONUP, WM_SIZE, WM_TIMECHANGE, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_POPUP,
};

use core::{
    build_modules_with_config, Config, Module, PreviewSessionState, Size, DEFAULT_PIPE_NAME,
};

use crate::appbar::{place_appbar, register_appbar, remove_appbar, WINDOW_CLASS, WM_APPBAR};
use crate::config_watch::{
    default_config_path, reload_keeping_previous, watch, WM_CONFIG_CHANGED,
};
use crate::ipc_server::{start_ipc_server, IpcEvent, SharedIpcQueue, WM_IPC_EVENT};
use crate::render::{apply_backdrop, paint_bar, ActiveCanvas};
use crate::sys::{ms_until_next_minute, SystemClock, SystemPower};

const MENU_OPEN_SETTINGS: usize = 1;
const MENU_CHECK_UPDATES: usize = 2;
const MENU_EXIT: usize = 3;

thread_local! {
    static CANVAS: RefCell<Option<ActiveCanvas>> = const { RefCell::new(None) };
    static SESSION: RefCell<PreviewSessionState> = RefCell::new(PreviewSessionState::new(Config::default()));
    static CFG_PATH: RefCell<PathBuf> = RefCell::new(PathBuf::new());
    static MODULES: RefCell<Vec<Box<dyn Module>>> = const { RefCell::new(Vec::new()) };
    static IN_PLACE_APPBAR: Cell<bool> = const { Cell::new(false) };
    static IPC_QUEUE: RefCell<SharedIpcQueue> = RefCell::new(Arc::new(Mutex::new(VecDeque::new())));
    static SHARED_ACTIVE_CFG: RefCell<Arc<Mutex<Config>>> = RefCell::new(Arc::new(Mutex::new(Config::default())));
}

fn rebuild_modules(cfg: &Config) {
    let built = build_modules_with_config(cfg, Box::new(SystemClock), Box::new(SystemPower));
    MODULES.with(|m| *m.borrow_mut() = built);
}

fn refresh_modules() -> bool {
    MODULES.with(|m| {
        let mut mods = m.borrow_mut();
        let mut changed = false;
        for module in mods.iter_mut() {
            if module.refresh() {
                changed = true;
            }
        }
        changed
    })
}

fn rearm_minute_timer(hwnd: HWND) {
    unsafe {
        SetTimer(Some(hwnd), 1, ms_until_next_minute(), None);
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let _ = ValidateRect(Some(hwnd), None);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let bar_size = Size {
            w: (rc.right - rc.left).max(1),
            h: (rc.bottom - rc.top).max(1),
        };

        CANVAS.with(|c| {
            let mut c = c.borrow_mut();
            let Some(canvas) = c.as_mut() else { return };
            let cfg = SESSION.with(|s| s.borrow().active_config().clone());
            MODULES.with(|m| {
                let modules = m.borrow();
                canvas.begin_draw();
                paint_bar(canvas, bar_size, &cfg, &modules);
                let _ = canvas.end_draw();
            });
        });
    }
}

fn guarded_place_appbar(hwnd: HWND) {
    if IN_PLACE_APPBAR.replace(true) {
        return;
    }
    let bar_cfg = SESSION.with(|s| s.borrow().active_config().bar.clone());
    place_appbar(hwnd, &bar_cfg);
    IN_PLACE_APPBAR.set(false);
}

fn apply_active_config(hwnd: HWND) {
    let active = SESSION.with(|s| s.borrow().active_config().clone());
    SHARED_ACTIVE_CFG.with(|sc| {
        if let Ok(mut guard) = sc.borrow().lock() {
            *guard = active.clone();
        }
    });
    rebuild_modules(&active);
    apply_backdrop(hwnd, active.bar.backdrop);
    guarded_place_appbar(hwnd);
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn write_config_file(path: &std::path::Path, cfg: &Config) {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("failed to create config dir {}: {e}", parent.display());
                return;
            }
        }
    }
    match toml::to_string_pretty(cfg) {
        Ok(text) => {
            if let Err(e) = std::fs::write(path, text) {
                eprintln!("failed to write config {}: {e}", path.display());
            }
        }
        Err(e) => eprintln!("failed to serialize config: {e}"),
    }
}

fn build_updater_command(updater: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(updater);
    cmd.arg("--apply");
    cmd
}

fn build_settings_command(settings: &std::path::Path) -> std::process::Command {
    std::process::Command::new(settings)
}

fn spawn_sibling_exe(exe_name: &str, build_cmd: fn(&std::path::Path) -> std::process::Command) {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let target = dir.join(exe_name);
            if target.exists() {
                let _ = build_cmd(&target).spawn();
            }
        }
    }
}

fn show_context_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN_SETTINGS, w!("Ayarlar"));
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            MENU_CHECK_UPDATES,
            w!("Güncellemeleri Denetle"),
        );
        let _ = AppendMenuW(menu, MF_STRING, MENU_EXIT, w!("Çıkış"));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        match cmd.0 as usize {
            MENU_OPEN_SETTINGS => spawn_sibling_exe("taskbar-settings.exe", build_settings_command),
            MENU_CHECK_UPDATES => spawn_sibling_exe("taskbar-updater.exe", build_updater_command),
            MENU_EXIT => {
                let _ = DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

pub unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                let backdrop = SESSION.with(|s| s.borrow().active_config().bar.backdrop);
                apply_backdrop(hwnd, backdrop);
                rearm_minute_timer(hwnd);
                LRESULT(0)
            }
            WM_SIZE => {
                CANVAS.with(|c| {
                    if let Some(canvas) = c.borrow_mut().as_mut() {
                        canvas.resize(
                            ((lp.0 & 0xffff) as u32).max(1),
                            (((lp.0 >> 16) & 0xffff) as u32).max(1),
                        );
                    }
                });
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_PAINT => {
                paint(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_TIMER => {
                rearm_minute_timer(hwnd);
                if refresh_modules() {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_TIMECHANGE => {
                rearm_minute_timer(hwnd);
                let _ = refresh_modules();
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_POWERBROADCAST => {
                rearm_minute_timer(hwnd);
                if refresh_modules() {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(1)
            }
            WM_RBUTTONUP => {
                show_context_menu(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                guarded_place_appbar(hwnd);
                LRESULT(0)
            }
            WM_APPBAR => {
                if wp.0 as u32 == ABN_POSCHANGED {
                    guarded_place_appbar(hwnd);
                }
                LRESULT(0)
            }
            WM_CONFIG_CHANGED => {
                let path = CFG_PATH.with(|p| p.borrow().clone());
                let changed = SESSION.with(|s| {
                    let mut session = s.borrow_mut();
                    let prev_saved = session.saved_config().clone();
                    let prev_active = session.active_config().clone();
                    let next_disk = reload_keeping_previous(&path, &prev_saved);
                    session.on_disk_reload(next_disk);
                    *session.active_config() != prev_active
                });
                if changed {
                    apply_active_config(hwnd);
                }
                LRESULT(0)
            }
            WM_IPC_EVENT => {
                let events: Vec<IpcEvent> = IPC_QUEUE.with(|q| {
                    let queue = q.borrow();
                    let mut guard = queue.lock().expect("ipc queue lock");
                    guard.drain(..).collect()
                });
                let mut needs_apply = false;
                for ev in events {
                    match ev {
                        IpcEvent::PreviewConfig(cfg) => {
                            SESSION.with(|s| {
                                s.borrow_mut().apply_preview(cfg);
                            });
                            needs_apply = true;
                        }
                        IpcEvent::RevertPreview => {
                            if SESSION.with(|s| s.borrow_mut().revert_preview()) {
                                needs_apply = true;
                            }
                        }
                        IpcEvent::ClientDisconnected => {
                            if SESSION.with(|s| s.borrow_mut().on_client_disconnect()) {
                                needs_apply = true;
                            }
                        }
                        IpcEvent::SaveConfig(cfg) => {
                            let clamped = cfg.clamped();
                            let path = CFG_PATH.with(|p| p.borrow().clone());
                            write_config_file(&path, &clamped);
                            SESSION.with(|s| {
                                s.borrow_mut().commit_saved(clamped);
                            });
                            needs_apply = true;
                        }
                        IpcEvent::ShutdownForUpdate => {
                            let _ = DestroyWindow(hwnd);
                            return LRESULT(0);
                        }
                    }
                }
                if needs_apply {
                    apply_active_config(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                CANVAS.with(|c| *c.borrow_mut() = None);
                remove_appbar(hwnd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

pub fn run(cfg: Config) -> Result<()> {
    run_with_path(cfg, default_config_path())
}

pub fn run_with_path(cfg: Config, config_path: PathBuf) -> Result<()> {
    let cfg = cfg.clamped();
    rebuild_modules(&cfg);
    SESSION.with(|s| *s.borrow_mut() = PreviewSessionState::new(cfg.clone()));
    CFG_PATH.with(|p| *p.borrow_mut() = config_path.clone());
    let queue = Arc::new(Mutex::new(VecDeque::new()));
    IPC_QUEUE.with(|q| *q.borrow_mut() = Arc::clone(&queue));
    let shared_cfg = Arc::new(Mutex::new(cfg));
    SHARED_ACTIVE_CFG.with(|sc| *sc.borrow_mut() = Arc::clone(&shared_cfg));

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hi = GetModuleHandleW(PCWSTR::null())?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hi.into(),
            lpszClassName: WINDOW_CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            WINDOW_CLASS,
            w!("taskbar"),
            WS_POPUP,
            0,
            0,
            800,
            40,
            None,
            None,
            Some(hi.into()),
            None,
        )?;
        let _watch_handle = watch(&config_path, hwnd).ok();
        let _ipc_handle = start_ipc_server(DEFAULT_PIPE_NAME, hwnd, queue, shared_cfg).ok();
        register_appbar(hwnd);
        guarded_place_appbar(hwnd);
        let canvas = ActiveCanvas::new(hwnd)?;
        CANVAS.with(|c| *c.borrow_mut() = Some(canvas));
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = InvalidateRect(Some(hwnd), None, false);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn updater_command_passes_apply_flag() {
        let updater_path = Path::new(r"C:\Taskbar\taskbar-updater.exe");
        let cmd = build_updater_command(updater_path);
        assert_eq!(cmd.get_program(), updater_path.as_os_str());
        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(args, vec![OsStr::new("--apply")]);
    }

    #[test]
    fn settings_command_targets_taskbar_settings_exe() {
        let settings_path = Path::new(r"C:\Taskbar\taskbar-settings.exe");
        let cmd = build_settings_command(settings_path);
        assert_eq!(cmd.get_program(), settings_path.as_os_str());
        assert_eq!(cmd.get_args().count(), 0);
    }
}
