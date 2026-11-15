use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER,
};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE,
    DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{InvalidateRect, ValidateRect};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Shell::ABN_POSCHANGED;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, LoadCursorW, PostMessageW,
    PostQuitMessage, RegisterClassW, SetForegroundWindow, SetTimer, ShowWindow, TrackPopupMenu,
    TranslateMessage, IDC_ARROW, MA_NOACTIVATE, MF_STRING, MSG, SW_SHOWNOACTIVATE, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NULL, WM_PAINT, WM_POWERBROADCAST,
    WM_RBUTTONUP, WM_SIZE, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

use core::{build_modules, Clock, Config, Module, PowerSource, PowerState};

use crate::appbar::{place_appbar, register_appbar, remove_appbar, WINDOW_CLASS, WM_APPBAR};
use crate::config_watch::{
    default_config_path, reload_keeping_previous, watch, WM_CONFIG_CHANGED,
};

const MENU_CHECK_UPDATES: usize = 1;
const MENU_EXIT: usize = 2;

struct WinClock;

impl Clock for WinClock {
    fn now(&self) -> (u8, u8) {
        let t = unsafe { GetLocalTime() };
        (t.wHour as u8, t.wMinute as u8)
    }
}

struct WinPower;

impl PowerSource for WinPower {
    fn read(&self) -> Option<PowerState> {
        let mut sps = SYSTEM_POWER_STATUS::default();
        let ok = unsafe { GetSystemPowerStatus(&mut sps) }.is_ok() && sps.BatteryLifePercent <= 100;
        if ok {
            Some(PowerState {
                percent: sps.BatteryLifePercent,
                on_ac: sps.ACLineStatus == 1,
            })
        } else {
            None
        }
    }
}

struct Gfx {
    target: ID2D1HwndRenderTarget,
    text: IDWriteTextFormat,
    brush: ID2D1SolidColorBrush,
    pill: ID2D1SolidColorBrush,
}

thread_local! {
    static GFX: RefCell<Option<Gfx>> = const { RefCell::new(None) };
    static CFG: RefCell<Config> = RefCell::new(Config::default());
    static CFG_PATH: RefCell<PathBuf> = RefCell::new(PathBuf::new());
    static MODULES: RefCell<Vec<Box<dyn Module>>> = const { RefCell::new(Vec::new()) };
    static IN_PLACE_APPBAR: Cell<bool> = const { Cell::new(false) };
}

fn rebuild_modules(cfg: &Config) {
    let built = build_modules(&cfg.modules, Box::new(WinClock), Box::new(WinPower));
    MODULES.with(|m| *m.borrow_mut() = built);
}

fn status_text() -> String {
    let edge = CFG.with(|c| c.borrow().bar.edge);
    MODULES.with(|m| {
        let mut mods = m.borrow_mut();
        for module in mods.iter_mut() {
            module.refresh();
        }
        mods.iter()
            .map(|module| module.text(edge))
            .collect::<Vec<_>>()
            .join("    ")
    })
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
                width: (rc.right - rc.left).max(1) as u32,
                height: (rc.bottom - rc.top).max(1) as u32,
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
        let white = D2D1_COLOR_F {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 1.0,
        };
        let pillc = D2D1_COLOR_F {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: 0.10,
        };
        let brush = target.CreateSolidColorBrush(&white, None)?;
        let pill = target.CreateSolidColorBrush(&pillc, None)?;
        Ok(Gfx {
            target,
            text,
            brush,
            pill,
        })
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
            g.target.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));
            let w = 240.0f32.min(size.width);
            let r = D2D_RECT_F {
                left: (size.width - w) / 2.0,
                top: 6.0,
                right: (size.width + w) / 2.0,
                bottom: (size.height - 6.0).max(6.0),
            };
            g.target.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: r,
                    radiusX: 12.0,
                    radiusY: 12.0,
                },
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

fn guarded_place_appbar(hwnd: HWND) {
    if IN_PLACE_APPBAR.replace(true) {
        return;
    }
    let bar_cfg = CFG.with(|c| c.borrow().bar.clone());
    place_appbar(hwnd, &bar_cfg);
    IN_PLACE_APPBAR.set(false);
}

fn spawn_updater() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let updater = dir.join("taskbar-updater.exe");
            if updater.exists() {
                let _ = std::process::Command::new(updater).spawn();
            }
        }
    }
}

fn show_context_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
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
            MENU_CHECK_UPDATES => spawn_updater(),
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
                let backdrop: i32 = 3; // transient window = acrylic; 2 = Mica
                let dark: i32 = 1;
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_SYSTEMBACKDROP_TYPE,
                    &backdrop as *const _ as _,
                    4,
                );
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_USE_IMMERSIVE_DARK_MODE,
                    &dark as *const _ as _,
                    4,
                );
                let m = MARGINS {
                    cxLeftWidth: -1,
                    cxRightWidth: -1,
                    cyTopHeight: -1,
                    cyBottomHeight: -1,
                };
                let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
                SetTimer(Some(hwnd), 1, 60_000, None);
                LRESULT(0)
            }
            WM_SIZE => {
                GFX.with(|g| {
                    if let Some(g) = g.borrow().as_ref() {
                        let _ = g.target.Resize(&D2D_SIZE_U {
                            width: ((lp.0 & 0xffff) as u32).max(1),
                            height: (((lp.0 >> 16) & 0xffff) as u32).max(1),
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
                let next_cfg = CFG.with(|c| {
                    let prev = c.borrow();
                    reload_keeping_previous(&path, &prev)
                });
                rebuild_modules(&next_cfg);
                CFG.with(|c| *c.borrow_mut() = next_cfg);
                guarded_place_appbar(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                GFX.with(|g| *g.borrow_mut() = None);
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
    CFG.with(|c| *c.borrow_mut() = cfg);
    CFG_PATH.with(|p| *p.borrow_mut() = config_path.clone());

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
        register_appbar(hwnd);
        guarded_place_appbar(hwnd);
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
