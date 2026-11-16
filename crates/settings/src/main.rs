#![windows_subsystem = "windows"]

pub mod anim;
pub mod canvas;
pub mod d2d;
pub mod ipc_client;
pub mod state;
pub mod ui;

use std::cell::RefCell;
use std::time::Instant;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MonitorFromWindow, ScreenToClient, ValidateRect, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, GetClientRect, GetMessageW,
    IsZoomed, LoadCursorW, PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, SetWindowPos, ShowWindow, TranslateMessage, CW_USEDEFAULT, HTBOTTOM,
    HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT,
    HTTOPRIGHT, IDC_ARROW, MINMAXINFO, MSG, NCCALCSIZE_PARAMS, PM_REMOVE, SIZE_MAXIMIZED,
    SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCALCSIZE, WM_NCHITTEST, WM_PAINT, WM_QUIT, WM_SIZE,
    WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

use core::{Rect, Size, DEFAULT_PIPE_NAME};

use crate::anim::{AppPhase, UiAnimState};
use crate::d2d::{apply_integrated_frame, D2dCanvas};
use crate::ipc_client::PipeClient;
use crate::state::{
    default_config_path, load_config_from_disk, ActionEffect, SettingsAppState, SettingsTab,
    UiAction,
};
use crate::ui::{hit_test_region, render_settings_ui_animated, HitRegion, TITLEBAR_DRAG_HEIGHT};

const WINDOW_CLASS: PCWSTR = w!("RustTaskbarSettingsWindow");

thread_local! {
    static CANVAS: RefCell<Option<D2dCanvas>> = const { RefCell::new(None) };
    static STATE: RefCell<Option<SettingsAppState>> = const { RefCell::new(None) };
    static ANIM: RefCell<Option<UiAnimState>> = const { RefCell::new(None) };
    static PIPE: RefCell<Option<PipeClient>> = const { RefCell::new(None) };
    static HIT_REGIONS: RefCell<Vec<HitRegion>> = const { RefCell::new(Vec::new()) };
    static LAST_TICK: RefCell<Option<Instant>> = const { RefCell::new(None) };
    static DRAGGING_SLIDER: RefCell<Option<Rect>> = const { RefCell::new(None) };
    static TRACKING_MOUSE: RefCell<bool> = const { RefCell::new(false) };
}

fn tab_index(tab: SettingsTab) -> usize {
    match tab {
        SettingsTab::Appearance => 0,
        SettingsTab::Widgets => 1,
        SettingsTab::About => 2,
    }
}

pub fn slider_x_to_thickness(rect: Rect, x: i32) -> u32 {
    let w = rect.w.max(1) as f32;
    let ratio = ((x - rect.x) as f32 / w).clamp(0.0, 1.0);
    (24.0 + ratio * (120.0 - 24.0)).round() as u32
}

pub fn is_window_control_action(action: &UiAction) -> bool {
    matches!(
        action,
        UiAction::WindowMinimize | UiAction::WindowMaximize | UiAction::WindowClose
    )
}

pub fn clamp_maximized_rect_to_work_area(rect: &mut RECT, work: RECT) {
    *rect = work;
}

pub fn compute_nc_hit_test(
    client_x: i32,
    client_y: i32,
    client_w: i32,
    client_h: i32,
    is_maximized: bool,
    regions: &[HitRegion],
) -> u32 {
    const RESIZE_BORDER: i32 = 6;
    if !is_maximized {
        let on_left = client_x >= 0 && client_x < RESIZE_BORDER;
        let on_right = client_x >= client_w - RESIZE_BORDER && client_x < client_w;
        let on_top = client_y >= 0 && client_y < RESIZE_BORDER;
        let on_bottom = client_y >= client_h - RESIZE_BORDER && client_y < client_h;
        match (on_left, on_right, on_top, on_bottom) {
            (true, false, true, false) => return HTTOPLEFT,
            (false, true, true, false) => return HTTOPRIGHT,
            (true, false, false, true) => return HTBOTTOMLEFT,
            (false, true, false, true) => return HTBOTTOMRIGHT,
            (true, false, false, false) => return HTLEFT,
            (false, true, false, false) => return HTRIGHT,
            (false, false, true, false) => return HTTOP,
            (false, false, false, true) => return HTBOTTOM,
            _ => {}
        }
    }

    if hit_test_region(regions, client_x, client_y).is_some() {
        return HTCLIENT;
    }

    if client_y >= 0 && client_y < TITLEBAR_DRAG_HEIGHT {
        return HTCAPTION;
    }

    HTCLIENT
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

        let mut recreate = false;
        CANVAS.with(|c| {
            let mut c = c.borrow_mut();
            let Some(canvas) = c.as_mut() else { return };
            STATE.with(|s| {
                let s = s.borrow();
                let Some(state) = s.as_ref() else { return };
                ANIM.with(|a| {
                    let a = a.borrow();
                    let Some(anim) = a.as_ref() else { return };
                    canvas.begin_draw();
                    let regions = render_settings_ui_animated(canvas, size, state, anim);
                    HIT_REGIONS.with(|hr| *hr.borrow_mut() = regions);
                    if canvas.end_draw().is_err() {
                        recreate = true;
                    }
                });
            });
        });

        if recreate {
            if let Ok(new_canvas) = D2dCanvas::new(hwnd) {
                CANVAS.with(|c| *c.borrow_mut() = Some(new_canvas));
            }
        }
    }
}

fn apply_ui_action(hwnd: HWND, action: UiAction) {
    match action {
        UiAction::WindowMinimize => {
            unsafe {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            return;
        }
        UiAction::WindowMaximize => {
            unsafe {
                let cmd = if IsZoomed(hwnd).as_bool() {
                    SW_RESTORE
                } else {
                    SW_MAXIMIZE
                };
                let _ = ShowWindow(hwnd, cmd);
            }
            return;
        }
        UiAction::WindowClose => {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        UiAction::SelectTab(tab) => {
            ANIM.with(|a| {
                if let Some(anim) = a.borrow_mut().as_mut() {
                    anim.on_tab_changed(tab_index(tab));
                }
            });
        }
        _ => {}
    }

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

fn handle_lbutton_down(hwnd: HWND, x: i32, y: i32) {
    let in_splash = ANIM.with(|a| {
        a.borrow()
            .as_ref()
            .is_some_and(|anim| anim.phase() == AppPhase::Splash)
    });

    let hit = HIT_REGIONS.with(|hr| hit_test_region(&hr.borrow(), x, y).cloned());
    let Some(region) = hit else {
        return;
    };
    if in_splash && !is_window_control_action(&region.action) {
        return;
    }

    ANIM.with(|a| {
        if let Some(anim) = a.borrow_mut().as_mut() {
            anim.set_pressed(Some(region.id.clone()));
        }
    });

    if region.is_thickness_slider {
        DRAGGING_SLIDER.with(|d| *d.borrow_mut() = Some(region.rect));
        unsafe {
            SetCapture(hwnd);
        }
        let thickness = slider_x_to_thickness(region.rect, x);
        apply_ui_action(hwnd, UiAction::SetThickness(thickness));
    } else {
        apply_ui_action(hwnd, region.action);
    }
}

fn handle_mouse_move(hwnd: HWND, x: i32, y: i32) {
    TRACKING_MOUSE.with(|t| {
        if !*t.borrow() {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            unsafe {
                let _ = TrackMouseEvent(&mut tme);
            }
            *t.borrow_mut() = true;
        }
    });

    let active_slider = DRAGGING_SLIDER.with(|d| *d.borrow());
    if let Some(slider_rect) = active_slider {
        let thickness = slider_x_to_thickness(slider_rect, x);
        let changed = STATE.with(|s| {
            s.borrow()
                .as_ref()
                .is_some_and(|st| st.working_config.bar.thickness != thickness)
        });
        if changed {
            apply_ui_action(hwnd, UiAction::SetThickness(thickness));
        }
        return;
    }

    let hovered = HIT_REGIONS.with(|hr| hit_test_region(&hr.borrow(), x, y).map(|r| r.id.clone()));
    ANIM.with(|a| {
        if let Some(anim) = a.borrow_mut().as_mut() {
            anim.set_hovered(hovered);
        }
    });
}

fn current_targets() -> (usize, u32) {
    STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|st| (tab_index(st.tab), st.working_config.bar.thickness))
            .unwrap_or((0, 40))
    })
}

fn is_currently_animating() -> bool {
    let (tab_idx, thickness) = current_targets();
    ANIM.with(|a| {
        a.borrow()
            .as_ref()
            .is_some_and(|anim| anim.is_animating(tab_idx, thickness))
    })
}

fn step_animation_and_paint(hwnd: HWND) {
    let frame_start = Instant::now();
    let dt = LAST_TICK.with(|lt| {
        let mut guard = lt.borrow_mut();
        let dt = guard
            .map(|prev| frame_start.saturating_duration_since(prev).as_secs_f32())
            .unwrap_or(0.016);
        *guard = Some(frame_start);
        dt
    });

    let (tab_idx, thickness) = current_targets();
    ANIM.with(|a| {
        if let Some(anim) = a.borrow_mut().as_mut() {
            anim.tick(dt, tab_idx, thickness);
        }
    });

    paint(hwnd);

    // D2D's EndDraw waits on DWM VSync when visible, but if minimized or on a virtual display
    // where EndDraw returns immediately, DwmFlush prevents busy-spinning.
    if frame_start.elapsed().as_millis() < 4 {
        unsafe {
            let _ = DwmFlush();
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                apply_integrated_frame(hwnd);
                LAST_TICK.with(|lt| *lt.borrow_mut() = Some(Instant::now()));
                LRESULT(0)
            }
            WM_NCCALCSIZE => {
                if wp.0 != 0 {
                    let params = &mut *(lp.0 as *mut NCCALCSIZE_PARAMS);
                    if IsZoomed(hwnd).as_bool() {
                        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                        let mut mi = MONITORINFO {
                            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                            ..Default::default()
                        };
                        if GetMonitorInfoW(monitor, &mut mi).as_bool() {
                            clamp_maximized_rect_to_work_area(&mut params.rgrc[0], mi.rcWork);
                        }
                    }
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wp, lp)
                }
            }
            WM_NCHITTEST => {
                let mut pt = POINT {
                    x: (lp.0 & 0xffff) as i16 as i32,
                    y: ((lp.0 >> 16) & 0xffff) as i16 as i32,
                };
                let _ = ScreenToClient(hwnd, &mut pt);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let is_max = IsZoomed(hwnd).as_bool();
                let hit = HIT_REGIONS.with(|hr| {
                    compute_nc_hit_test(
                        pt.x,
                        pt.y,
                        rc.right - rc.left,
                        rc.bottom - rc.top,
                        is_max,
                        &hr.borrow(),
                    )
                });
                LRESULT(hit as isize)
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lp.0 as *mut MINMAXINFO);
                info.ptMinTrackSize.x = 520;
                info.ptMinTrackSize.y = 460;
                LRESULT(0)
            }
            WM_SIZE => {
                STATE.with(|s| {
                    if let Some(st) = s.borrow_mut().as_mut() {
                        st.window_maximized = wp.0 == SIZE_MAXIMIZED as usize;
                    }
                });
                CANVAS.with(|c| {
                    if let Some(canvas) = c.borrow_mut().as_mut() {
                        canvas.resize(
                            ((lp.0 & 0xffff) as u32).max(1),
                            (((lp.0 >> 16) & 0xffff) as u32).max(1),
                        );
                    }
                });
                paint(hwnd);
                LRESULT(0)
            }
            WM_PAINT => {
                paint(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_MOUSEMOVE => {
                let x = (lp.0 & 0xffff) as i16 as i32;
                let y = ((lp.0 >> 16) & 0xffff) as i16 as i32;
                handle_mouse_move(hwnd, x, y);
                LRESULT(0)
            }
            WM_MOUSELEAVE => {
                TRACKING_MOUSE.with(|t| *t.borrow_mut() = false);
                ANIM.with(|a| {
                    if let Some(anim) = a.borrow_mut().as_mut() {
                        anim.set_hovered(None);
                        anim.set_pressed(None);
                    }
                });
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let x = (lp.0 & 0xffff) as i16 as i32;
                let y = ((lp.0 >> 16) & 0xffff) as i16 as i32;
                handle_lbutton_down(hwnd, x, y);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                let was_dragging = DRAGGING_SLIDER.with(|d| d.borrow_mut().take().is_some());
                if was_dragging {
                    let _ = ReleaseCapture();
                }
                ANIM.with(|a| {
                    if let Some(anim) = a.borrow_mut().as_mut() {
                        anim.set_pressed(None);
                    }
                });
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
        let initial_thickness = initial_cfg.bar.thickness;

        STATE.with(|s| {
            *s.borrow_mut() = Some(SettingsAppState::new(
                initial_cfg,
                config_path,
                connected,
            ));
        });
        ANIM.with(|a| {
            *a.borrow_mut() = Some(UiAnimState::new(0, initial_thickness));
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
            w!("Taskbar"),
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
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );
        let canvas = D2dCanvas::new(hwnd)?;
        CANVAS.with(|c| *c.borrow_mut() = Some(canvas));
        let _ = ShowWindow(hwnd, SW_SHOW);
        paint(hwnd);

        let mut msg = MSG::default();
        loop {
            if is_currently_animating() {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return Ok(());
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                step_animation_and_paint(hwnd);
            } else {
                if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    break;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
                LAST_TICK.with(|lt| *lt.borrow_mut() = Some(Instant::now()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slider_x_to_thickness_clamps_out_of_bounds() {
        let rect = Rect {
            x: 100,
            y: 20,
            w: 200,
            h: 26,
        };
        assert_eq!(slider_x_to_thickness(rect, 0), 24);
        assert_eq!(slider_x_to_thickness(rect, 100), 24);
        assert_eq!(slider_x_to_thickness(rect, 200), 72);
        assert_eq!(slider_x_to_thickness(rect, 300), 120);
        assert_eq!(slider_x_to_thickness(rect, 500), 120);
    }

    #[test]
    fn is_window_control_action_identifies_caption_actions() {
        assert!(is_window_control_action(&UiAction::WindowMinimize));
        assert!(is_window_control_action(&UiAction::WindowMaximize));
        assert!(is_window_control_action(&UiAction::WindowClose));
        assert!(!is_window_control_action(&UiAction::Save));
        assert!(!is_window_control_action(&UiAction::SelectTab(
            SettingsTab::Appearance
        )));
    }

    #[test]
    fn clamp_maximized_rect_to_work_area_prevents_offscreen_overflow() {
        let mut rect = RECT {
            left: -8,
            top: -8,
            right: 1928,
            bottom: 1048,
        };
        let work = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        clamp_maximized_rect_to_work_area(&mut rect, work);
        assert_eq!(rect, work);
    }

    #[test]
    fn nc_hit_test_borders_disabled_when_maximized() {
        use windows::Win32::UI::WindowsAndMessaging::{
            HTCAPTION, HTCLIENT, HTLEFT, HTTOPLEFT, HTTOPRIGHT,
        };
        let close_region = vec![HitRegion {
            id: "win_close".to_string(),
            rect: Rect {
                x: 738,
                y: 0,
                w: 42,
                h: 34,
            },
            action: UiAction::WindowClose,
            is_thickness_slider: false,
        }];
        assert_eq!(
            compute_nc_hit_test(2, 2, 780, 600, false, &[]),
            HTTOPLEFT
        );
        assert_eq!(
            compute_nc_hit_test(2, 2, 780, 600, true, &[]),
            HTCAPTION
        );
        assert_eq!(
            compute_nc_hit_test(2, 100, 780, 600, false, &[]),
            HTLEFT
        );
        assert_eq!(
            compute_nc_hit_test(2, 100, 780, 600, true, &[]),
            HTCLIENT
        );
        assert_eq!(
            compute_nc_hit_test(778, 2, 780, 600, false, &close_region),
            HTTOPRIGHT
        );
        assert_eq!(
            compute_nc_hit_test(778, 2, 780, 600, true, &close_region),
            HTCLIENT
        );
    }

    #[test]
    fn nc_hit_test_prefers_hit_regions_over_caption() {
        use windows::Win32::UI::WindowsAndMessaging::{HTCAPTION, HTCLIENT};
        let regions = vec![HitRegion {
            id: "win_close".to_string(),
            rect: Rect {
                x: 394,
                y: 0,
                w: 42,
                h: 34,
            },
            action: UiAction::WindowClose,
            is_thickness_slider: false,
        }];
        assert_eq!(
            compute_nc_hit_test(410, 16, 520, 460, false, &regions),
            HTCLIENT
        );
        assert_eq!(
            compute_nc_hit_test(150, 16, 520, 460, false, &regions),
            HTCAPTION
        );
        assert_eq!(
            compute_nc_hit_test(150, 120, 520, 460, false, &regions),
            HTCLIENT
        );
    }
}
