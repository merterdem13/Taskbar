#![windows_subsystem = "windows"]

pub mod app;
pub mod appbar;

use core::Config;
use std::sync::atomic::Ordering;
use windows::core::Result;
use windows::Win32::Foundation::HWND;

fn main() -> Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let raw = appbar::REGISTERED_HWND.swap(0, Ordering::SeqCst);
        if raw != 0 {
            appbar::remove_appbar(HWND(raw as *mut std::ffi::c_void));
        }
        default_hook(info);
    }));

    app::run(Config::default())
}
