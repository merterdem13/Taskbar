use std::cell::{Cell, RefCell};
use std::path::PathBuf;
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

use core::{build_modules, Config, Module, Size};

use crate::appbar::{place_appbar, register_appbar, remove_appbar, WINDOW_CLASS, WM_APPBAR};
use crate::config_watch::{
    default_config_path, reload_keeping_previous, watch, WM_CONFIG_CHANGED,
};
use crate::render::{apply_backdrop, paint_bar, ActiveCanvas};
use crate::sys::{ms_until_next_minute, SystemClock, SystemPower};

const MENU_CHECK_UPDATES: usize = 1;
const MENU_EXIT: usize = 2;

thread_local! {
    static CANVAS: RefCell<Option<ActiveCanvas>> = const { RefCell::new(None) };
    static CFG: RefCell<Config> = RefCell::new(Config::default());
    static CFG_PATH: RefCell<PathBuf> = RefCell::new(PathBuf::new());
    static MODULES: RefCell<Vec<Box<dyn Module>>> = const { RefCell::new(Vec::new()) };
    static IN_PLACE_APPBAR: Cell<bool> = const { Cell::new(false) };
}

fn rebuild_modules(cfg: &Config) {
    let built = build_modules(&cfg.modules, Box::new(SystemClock), Box::new(SystemPower));
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
            let cfg = CFG.with(|cfg| cfg.borrow().clone());
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
    let bar_cfg = CFG.with(|c| c.borrow().bar.clone());
    place_appbar(hwnd, &bar_cfg);
    IN_PLACE_APPBAR.set(false);
}

fn build_updater_command(updater: &std::path::Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(updater);
    cmd.arg("--apply");
    cmd
}

fn spawn_updater() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let updater = dir.join("taskbar-updater.exe");
            if updater.exists() {
                let _ = build_updater_command(&updater).spawn();
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
                let backdrop = CFG.with(|c| c.borrow().bar.backdrop);
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
                let next_cfg = CFG.with(|c| {
                    let prev = c.borrow();
                    reload_keeping_previous(&path, &prev)
                });
                rebuild_modules(&next_cfg);
                apply_backdrop(hwnd, next_cfg.bar.backdrop);
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
}
