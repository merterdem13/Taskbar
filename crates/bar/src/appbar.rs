use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE,
    ABM_SETPOS, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, GetWindowRect, MoveWindow, SM_CXSCREEN, SM_CYSCREEN, WM_USER,
};

use core::{bar_rect, BarConfig, Edge, Rect};

pub const WINDOW_CLASS: PCWSTR = w!("RustTaskbarWindow");
pub const WM_APPBAR: u32 = WM_USER + 1;

pub static REGISTERED_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn edge_to_abe(edge: Edge) -> u32 {
    match edge {
        Edge::Top => ABE_TOP,
        Edge::Bottom => ABE_BOTTOM,
        Edge::Left => ABE_LEFT,
        Edge::Right => ABE_RIGHT,
    }
}

pub fn apply_querypos_thickness(edge: Edge, queried: Rect, thickness_px: i32) -> Rect {
    match edge {
        Edge::Top => Rect {
            x: queried.x,
            y: queried.y,
            w: queried.w,
            h: thickness_px,
        },
        Edge::Bottom => Rect {
            x: queried.x,
            y: (queried.y + queried.h) - thickness_px,
            w: queried.w,
            h: thickness_px,
        },
        Edge::Left => Rect {
            x: queried.x,
            y: queried.y,
            w: thickness_px,
            h: queried.h,
        },
        Edge::Right => Rect {
            x: (queried.x + queried.w) - thickness_px,
            y: queried.y,
            w: thickness_px,
            h: queried.h,
        },
    }
}

fn rect_to_win32(r: Rect) -> RECT {
    RECT {
        left: r.x,
        top: r.y,
        right: r.x + r.w,
        bottom: r.y + r.h,
    }
}

fn win32_to_rect(rc: RECT) -> Rect {
    Rect {
        x: rc.left,
        y: rc.top,
        w: rc.right - rc.left,
        h: rc.bottom - rc.top,
    }
}

pub fn register_appbar(hwnd: HWND) {
    let raw = hwnd.0 as isize;
    if REGISTERED_HWND.swap(raw, Ordering::SeqCst) == raw && raw != 0 {
        return;
    }
    let mut abd = APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: WM_APPBAR,
        ..Default::default()
    };
    unsafe {
        SHAppBarMessage(ABM_NEW, &mut abd);
    }
}

pub fn place_appbar(hwnd: HWND, cfg: &BarConfig) -> Rect {
    unsafe {
        let screen = Rect {
            x: 0,
            y: 0,
            w: GetSystemMetrics(SM_CXSCREEN),
            h: GetSystemMetrics(SM_CYSCREEN),
        };
        let dpi = GetDpiForWindow(hwnd) as i32;
        let dpi = if dpi > 0 { dpi } else { 96 };
        let thickness_px = (cfg.thickness as i32 * dpi / 96).max(1);
        let initial = bar_rect(cfg.edge, screen, thickness_px);

        let mut abd = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: WM_APPBAR,
            uEdge: edge_to_abe(cfg.edge),
            rc: rect_to_win32(initial),
            ..Default::default()
        };
        SHAppBarMessage(ABM_QUERYPOS, &mut abd);
        let adjusted = apply_querypos_thickness(cfg.edge, win32_to_rect(abd.rc), thickness_px);
        abd.rc = rect_to_win32(adjusted);
        SHAppBarMessage(ABM_SETPOS, &mut abd);

        let final_rect = win32_to_rect(abd.rc);
        let mut current = RECT::default();
        let needs_move = GetWindowRect(hwnd, &mut current).is_err() || current != abd.rc;
        if needs_move {
            let _ = MoveWindow(
                hwnd,
                final_rect.x,
                final_rect.y,
                final_rect.w,
                final_rect.h,
                true,
            );
        }
        final_rect
    }
}

pub fn remove_appbar(hwnd: HWND) {
    REGISTERED_HWND.store(0, Ordering::SeqCst);
    let mut abd = APPBARDATA {
        cbSize: size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        ..Default::default()
    };
    unsafe {
        SHAppBarMessage(ABM_REMOVE, &mut abd);
    }
}

pub fn remove_registered_appbar() {
    let raw = REGISTERED_HWND.swap(0, Ordering::SeqCst);
    if raw != 0 {
        remove_appbar(HWND(raw as *mut std::ffi::c_void));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_to_abe_maps_all_four_edges() {
        assert_eq!(edge_to_abe(Edge::Top), ABE_TOP);
        assert_eq!(edge_to_abe(Edge::Bottom), ABE_BOTTOM);
        assert_eq!(edge_to_abe(Edge::Left), ABE_LEFT);
        assert_eq!(edge_to_abe(Edge::Right), ABE_RIGHT);
    }

    #[test]
    fn apply_querypos_thickness_preserves_strip_thickness_on_all_edges() {
        let top = apply_querypos_thickness(
            Edge::Top,
            Rect {
                x: 0,
                y: 12,
                w: 1920,
                h: 60,
            },
            40,
        );
        assert_eq!(
            top,
            Rect {
                x: 0,
                y: 12,
                w: 1920,
                h: 40
            }
        );

        let bottom = apply_querypos_thickness(
            Edge::Bottom,
            Rect {
                x: 0,
                y: 980,
                w: 1920,
                h: 60,
            },
            40,
        );
        assert_eq!(
            bottom,
            Rect {
                x: 0,
                y: 1000,
                w: 1920,
                h: 40
            }
        );

        let left = apply_querypos_thickness(
            Edge::Left,
            Rect {
                x: 8,
                y: 0,
                w: 60,
                h: 1080,
            },
            48,
        );
        assert_eq!(
            left,
            Rect {
                x: 8,
                y: 0,
                w: 48,
                h: 1080
            }
        );

        let right = apply_querypos_thickness(
            Edge::Right,
            Rect {
                x: 1850,
                y: 0,
                w: 60,
                h: 1080,
            },
            48,
        );
        assert_eq!(
            right,
            Rect {
                x: 1862,
                y: 0,
                w: 48,
                h: 1080
            }
        );
    }

    #[test]
    fn win32_rect_conversion_roundtrips() {
        let original = Rect {
            x: 10,
            y: 20,
            w: 300,
            h: 40,
        };
        let win_rc = rect_to_win32(original);
        assert_eq!(
            win_rc,
            RECT {
                left: 10,
                top: 20,
                right: 310,
                bottom: 60
            }
        );
        assert_eq!(win32_to_rect(win_rc), original);
    }

    #[test]
    fn register_place_four_edges_and_remove_restores_state() {
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
        };

        unsafe extern "system" fn dummy_wndproc(
            hwnd: HWND,
            msg: u32,
            wp: windows::Win32::Foundation::WPARAM,
            lp: windows::Win32::Foundation::LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }

        unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).expect("module handle");
            let class = w!("RustTaskbarAppBarTestWindow");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(dummy_wndproc),
                hInstance: hi.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("test"),
                WS_POPUP,
                0,
                0,
                100,
                40,
                None,
                None,
                Some(hi.into()),
                None,
            )
            .expect("create test window");

            register_appbar(hwnd);
            assert_eq!(REGISTERED_HWND.load(Ordering::SeqCst), hwnd.0 as isize);

            for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
                let cfg = BarConfig {
                    edge,
                    thickness: 40,
                    ..BarConfig::default()
                };
                let r = place_appbar(hwnd, &cfg);
                if edge.is_vertical() {
                    assert!(r.w > 0);
                    assert!(r.h >= r.w);
                } else {
                    assert!(r.h > 0);
                    assert!(r.w >= r.h);
                }
            }

            remove_appbar(hwnd);
            assert_eq!(REGISTERED_HWND.load(Ordering::SeqCst), 0);
            let _ = DestroyWindow(hwnd);
        }
    }
}
