#![windows_subsystem = "windows"]

pub mod canvas;
pub mod ipc_client;
pub mod state;
pub mod ui;

use std::cell::RefCell;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, GetClientRect, GetMessageW,
    LoadCursorW, PostQuitMessage, RegisterClassW, SetForegroundWindow, ShowWindow,
    TranslateMessage, CW_USEDEFAULT, IDC_ARROW, MSG, SW_RESTORE, SW_SHOW, WM_CREATE, WM_DESTROY,
    WM_ERASEBKGND, WM_LBUTTONDOWN, WM_PAINT, WM_SIZE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

use core::{Size, DEFAULT_PIPE_NAME};

use crate::canvas::{apply_settings_backdrop, SkiaCanvas};
use crate::ipc_client::PipeClient;
use crate::state::{
    default_config_path, load_config_from_disk, ActionEffect, SettingsAppState,
};
use crate::ui::{hit_test, render_settings_ui, HitRegion};

const WINDOW_CLASS: PCWSTR = w!("RustTaskbarSettingsWindow");

thread_local! {
    static CANVAS: RefCell<Option<SkiaCanvas>> = const { RefCell::new(None) };
    static STATE: RefCell<Option<SettingsAppState>> = const { RefCell::new(None) };
    static PIPE: RefCell<Option<PipeClient>> = const { RefCell::new(None) };
    static HIT_REGIONS: RefCell<Vec<HitRegion>> = const { RefCell::new(Vec::new()) };
}

fn spawn_updater() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let updater = dir.join("taskbar-updater.exe");
            if updater.exists() {
                let mut cmd = std::process::Command::new(updater);
                cmd.arg("--apply");
                let _ = cmd.spawn();
            }
        }
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let _ = ValidateRect(Some(hwnd), None);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let size = Size {
            w: (rc.right - rc.left).max(1),
            h: (rc.bottom - rc.top).max(1),
        };

        CANVAS.with(|c| {
            let mut c = c.borrow_mut();
            let Some(canvas) = c.as_mut() else { return };
            STATE.with(|s| {
                let s = s.borrow();
                let Some(state) = s.as_ref() else { return };
                canvas.begin_draw();
                let regions = render_settings_ui(canvas, size, state);
                HIT_REGIONS.with(|hr| *hr.borrow_mut() = regions);
                let _ = canvas.end_draw();
            });
        });
    }
}

fn handle_click(hwnd: HWND, x: i32, y: i32) {
    let maybe_action = HIT_REGIONS.with(|hr| hit_test(&hr.borrow(), x, y));
    let Some(action) = maybe_action else {
        return;
    };

    let effect = STATE.with(|s| {
        let mut guard = s.borrow_mut();
        guard
            .as_mut()
            .map(|st| st.apply_action(action))
            .unwrap_or(ActionEffect::None)
    });

    match effect {
        ActionEffect::None => {}
        ActionEffect::PreviewChanged => {
            PIPE.with(|p| {
                let mut pipe_guard = p.borrow_mut();
                if let Some(pipe) = pipe_guard.as_mut() {
                    STATE.with(|s| {
                        let mut st_guard = s.borrow_mut();
                        if let Some(st) = st_guard.as_mut() {
                            let ok = pipe.send_preview(&st.working_config);
                            st.live_connected = ok;
                        }
                    });
                }
            });
        }
        ActionEffect::RevertRequested => {
            PIPE.with(|p| {
                let mut pipe_guard = p.borrow_mut();
                if let Some(pipe) = pipe_guard.as_mut() {
                    let ok = pipe.send_revert();
                    STATE.with(|s| {
                        if let Some(st) = s.borrow_mut().as_mut() {
                            st.live_connected = ok;
                        }
                    });
                }
            });
        }
        ActionEffect::SaveRequested => {
            STATE.with(|s| {
                let mut st_guard = s.borrow_mut();
                if let Some(st) = st_guard.as_mut() {
                    if let Err(err) = st.save_to_disk() {
                        st.status_text = format!("Kaydetme hatası: {err}");
                        return;
                    }
                    PIPE.with(|p| {
                        if let Some(pipe) = p.borrow_mut().as_mut() {
                            let ok = pipe.send_save(&st.working_config);
                            st.live_connected = ok;
                        }
                    });
                }
            });
        }
        ActionEffect::LaunchUpdater => {
            spawn_updater();
            STATE.with(|s| {
                if let Some(st) = s.borrow_mut().as_mut() {
                    st.status_text = "Güncelleme denetleyicisi başlatıldı".to_string();
                }
            });
        }
    }

    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                apply_settings_backdrop(hwnd);
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
            WM_LBUTTONDOWN => {
                let x = (lp.0 & 0xffff) as i16 as i32;
                let y = ((lp.0 >> 16) & 0xffff) as i16 as i32;
                handle_click(hwnd, x, y);
                LRESULT(0)
            }
            WM_DESTROY => {
                CANVAS.with(|c| *c.borrow_mut() = None);
                // Dropping PipeClient closes the named pipe so bar reverts any unsaved preview.
                PIPE.with(|p| *p.borrow_mut() = None);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn main() -> Result<()> {
    unsafe {
        if let Ok(existing) = FindWindowW(WINDOW_CLASS, PCWSTR::null()) {
            if !existing.0.is_null() {
                let _ = ShowWindow(existing, SW_RESTORE);
                let _ = SetForegroundWindow(existing);
                return Ok(());
            }
        }

        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let config_path = default_config_path();
        let disk_cfg = load_config_from_disk(&config_path);
        let mut pipe = PipeClient::new(DEFAULT_PIPE_NAME);
        let connected = pipe.is_connected();
        let initial_cfg = pipe.fetch_active_config().unwrap_or(disk_cfg);

        STATE.with(|s| {
            *s.borrow_mut() = Some(SettingsAppState::new(
                initial_cfg,
                config_path,
                connected,
            ));
        });
        PIPE.with(|p| *p.borrow_mut() = Some(pipe));

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
            Default::default(),
            WINDOW_CLASS,
            w!("Taskbar Ayarları"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            780,
            600,
            None,
            None,
            Some(hi.into()),
            None,
        )?;
        let canvas = SkiaCanvas::new(hwnd)?;
        CANVAS.with(|c| *c.borrow_mut() = Some(canvas));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = InvalidateRect(Some(hwnd), None, false);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}
