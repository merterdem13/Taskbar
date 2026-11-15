#![windows_subsystem = "windows"]
//! SPIKE (throwaway): top appbar with acrylic backdrop, Direct2D + DirectWrite,
//! battery + clock. Event driven: redraws only on power change / minute tick.
//! Right-click the bar to quit.

use std::cell::RefCell;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_APPBAR: u32 = WM_USER + 1;
const BAR_HEIGHT_DIP: i32 = 40;

struct Gfx {
    target: ID2D1HwndRenderTarget,
    text: IDWriteTextFormat,
    brush: ID2D1SolidColorBrush,
    pill: ID2D1SolidColorBrush,
}

thread_local! { static GFX: RefCell<Option<Gfx>> = const { RefCell::new(None) }; }

fn status_text() -> String {
    let mut sps = SYSTEM_POWER_STATUS::default();
    let ok = unsafe { GetSystemPowerStatus(&mut sps) }.is_ok() && sps.BatteryLifePercent <= 100;
    let bat = if ok {
        let ac = if sps.ACLineStatus == 1 { " \u{26A1}" } else { "" };
        format!("\u{1F50B} {}%{}", sps.BatteryLifePercent, ac)
    } else {
        "\u{1F50C} AC".to_string()
    };
    let t = unsafe { GetLocalTime() };
    format!("{}    {:02}:{:02}", bat, t.wHour, t.wMinute)
}

fn create_gfx(hwnd: HWND) -> Result<Gfx> {
    unsafe {
        let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc)?;
        let dpi = GetDpiForWindow(hwnd) as f32;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi,
            dpiY: dpi,
            ..Default::default()
        };
        let hprops = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: (rc.right - rc.left) as u32,
                height: (rc.bottom - rc.top) as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        let target = factory.CreateHwndRenderTarget(&props, &hprops)?;
        let text = dwrite.CreateTextFormat(
            w!("Segoe UI Variable Text"),
            None,
            DWRITE_FONT_WEIGHT_SEMI_BOLD,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            14.0,
            w!("en-us"),
        )?;
        text.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        text.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        let white = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
        let pillc = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 0.10 };
        let brush = target.CreateSolidColorBrush(&white, None)?;
        let pill = target.CreateSolidColorBrush(&pillc, None)?;
        Ok(Gfx { target, text, brush, pill })
    }
}

fn paint(hwnd: HWND) {
    unsafe {
        let _ = ValidateRect(Some(hwnd), None);
        GFX.with(|g| {
            let g = g.borrow();
            let Some(g) = g.as_ref() else { return };
            let s: Vec<u16> = status_text().encode_utf16().collect();
            let size = g.target.GetSize();
            g.target.BeginDraw();
            g.target.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let w = 240.0f32.min(size.width);
            let r = D2D_RECT_F {
                left: (size.width - w) / 2.0,
                top: 6.0,
                right: (size.width + w) / 2.0,
                bottom: size.height - 6.0,
            };
            g.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT { rect: r, radiusX: 12.0, radiusY: 12.0 },
                &g.pill,
            );
            g.target.DrawText(
                &s,
                &g.text,
                &r,
                &g.brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let _ = g.target.EndDraw(None, None);
        });
    }
}

fn register_appbar(hwnd: HWND) {
    unsafe {
        let mut abd = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: WM_APPBAR,
            ..Default::default()
        };
        SHAppBarMessage(ABM_NEW, &mut abd);
        let dpi = GetDpiForWindow(hwnd) as i32;
        let h = BAR_HEIGHT_DIP * dpi / 96;
        abd.uEdge = ABE_TOP;
        abd.rc = RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: h };
        SHAppBarMessage(ABM_QUERYPOS, &mut abd);
        abd.rc.bottom = abd.rc.top + h;
        SHAppBarMessage(ABM_SETPOS, &mut abd);
        let _ = MoveWindow(hwnd, abd.rc.left, abd.rc.top, abd.rc.right - abd.rc.left, h, true);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CREATE => {
                let backdrop: i32 = 3; // transient window = acrylic; 2 = Mica
                let dark: i32 = 1;
                let _ = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop as *const _ as _, 4);
                let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark as *const _ as _, 4);
                let m = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
                let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
                SetTimer(Some(hwnd), 1, 60_000, None);
                LRESULT(0)
            }
            WM_SIZE => {
                GFX.with(|g| {
                    if let Some(g) = g.borrow().as_ref() {
                        let _ = g.target.Resize(&D2D_SIZE_U {
                            width: (lp.0 & 0xffff) as u32,
                            height: ((lp.0 >> 16) & 0xffff) as u32,
                        });
                    }
                });
                LRESULT(0)
            }
            WM_PAINT => {
                paint(hwnd);
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_TIMER => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_POWERBROADCAST => {
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(1)
            }
            WM_RBUTTONUP => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                register_appbar(hwnd);
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_DESTROY => {
                let mut abd = APPBARDATA {
                    cbSize: size_of::<APPBARDATA>() as u32,
                    hWnd: hwnd,
                    ..Default::default()
                };
                SHAppBarMessage(ABM_REMOVE, &mut abd);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn main() -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let hi = GetModuleHandleW(PCWSTR::null())?;
        let class = w!("RustTaskbarSpike");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hi.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
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
        register_appbar(hwnd);
        let gfx = create_gfx(hwnd)?;
        GFX.with(|g| *g.borrow_mut() = Some(gfx));
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}
