#![windows_subsystem = "windows"]

pub mod app;
pub mod appbar;
pub mod config_watch;
pub mod ipc_server;
pub mod render;
pub mod sys;

use windows::core::Result;

fn main() -> Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        appbar::remove_registered_appbar();
        default_hook(info);
    }));

    let config_path = config_watch::default_config_path();
    let cfg = config_watch::load_initial(&config_path);
    app::run_with_path(cfg, config_path)
}
