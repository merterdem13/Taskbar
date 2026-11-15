use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows::core::{Result, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadDirectoryChangesW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::IO::CancelIoEx;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_USER};

use core::Config;

pub const WM_CONFIG_CHANGED: u32 = WM_USER + 2;

pub struct WatchHandle {
    dir_handle: isize,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let h = HANDLE(self.dir_handle as *mut std::ffi::c_void);
        unsafe {
            let _ = CancelIoEx(h, None);
        }
        if let Some(join) = self.thread.take() {
            let _ = join.join();
        }
        unsafe {
            let _ = CloseHandle(h);
        }
    }
}

pub fn default_config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Taskbar").join("config.toml")
}

pub fn load_initial(path: &Path) -> Config {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("failed to create config directory {}: {e}", parent.display());
                    return Config::default();
                }
            }
        }
        let default_cfg = Config::default();
        if let Ok(serialized) = toml::to_string_pretty(&default_cfg) {
            if let Err(e) = fs::write(path, serialized) {
                eprintln!("failed to write default config {}: {e}", path.display());
            }
        }
        return default_cfg;
    }

    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to read config {}: {e}", path.display());
            return Config::default();
        }
    };

    match Config::parse(&text) {
        Ok(cfg) => cfg.clamped(),
        Err(e) => {
            eprintln!("invalid config {}: {e}", path.display());
            Config::default()
        }
    }
}

pub fn reload_keeping_previous(path: &Path, prev: &Config) -> Config {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to read config {} on reload: {e}", path.display());
            return prev.clone();
        }
    };

    if text.trim().is_empty() {
        return prev.clone();
    }

    match Config::parse(&text) {
        Ok(cfg) => cfg.clamped(),
        Err(e) => {
            eprintln!("ignoring invalid config {} on reload: {e}", path.display());
            prev.clone()
        }
    }
}

pub fn watch(path: &Path, hwnd: HWND) -> Result<WatchHandle> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let _ = fs::create_dir_all(dir);

    let wide: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )?
    };

    let raw_handle = handle.0 as isize;
    let raw_hwnd = hwnd.0 as isize;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);

    let thread = thread::Builder::new()
        .name("config-watch".to_string())
        .spawn(move || {
            let mut buf = [0u32; 1024];
            let filter = FILE_NOTIFY_CHANGE_FILE_NAME
                | FILE_NOTIFY_CHANGE_LAST_WRITE
                | FILE_NOTIFY_CHANGE_SIZE;

            while !stop_thread.load(Ordering::SeqCst) {
                let mut bytes_returned = 0u32;
                let h = HANDLE(raw_handle as *mut std::ffi::c_void);
                let ok = unsafe {
                    ReadDirectoryChangesW(
                        h,
                        buf.as_mut_ptr() as *mut std::ffi::c_void,
                        (buf.len() * std::mem::size_of::<u32>()) as u32,
                        false,
                        filter,
                        Some(&mut bytes_returned),
                        None,
                        None,
                    )
                };

                if stop_thread.load(Ordering::SeqCst) || ok.is_err() || bytes_returned == 0 {
                    break;
                }

                thread::sleep(Duration::from_millis(150));
                if stop_thread.load(Ordering::SeqCst) {
                    break;
                }

                let target_hwnd = HWND(raw_hwnd as *mut std::ffi::c_void);
                unsafe {
                    let _ = PostMessageW(Some(target_hwnd), WM_CONFIG_CHANGED, WPARAM(0), LPARAM(0));
                }
            }
        })
        .expect("spawn config-watch thread");

    Ok(WatchHandle {
        dir_handle: raw_handle,
        stop,
        thread: Some(thread),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::Edge;
    use std::sync::atomic::AtomicUsize;

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_config_path(test_name: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "taskbar_config_watch_test_{}_{}_{}",
            std::process::id(),
            test_name,
            id
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("Taskbar").join("config.toml")
    }

    #[test]
    fn load_missing_file_gives_default_and_creates_file() {
        let path = temp_config_path("missing");
        assert!(!path.exists());

        let cfg = load_initial(&path);
        assert_eq!(cfg, Config::default());
        assert!(
            path.exists(),
            "expected load_initial to create default config.toml"
        );

        let written = fs::read_to_string(&path).expect("read created config.toml");
        let reparsed = Config::parse(&written).expect("created config.toml must be valid");
        assert_eq!(reparsed, Config::default());

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn load_broken_file_gives_default_without_overwriting_user_file() {
        let path = temp_config_path("broken");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let broken_content = "version = [not valid toml # user comment";
        fs::write(&path, broken_content).unwrap();

        let cfg = load_initial(&path);
        assert_eq!(cfg, Config::default());

        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(
            after, broken_content,
            "broken user config file must not be overwritten"
        );

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn load_future_version_gives_default() {
        let path = temp_config_path("future_version");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let future_content = "version = 99\n[bar]\nedge = \"bottom\"\n";
        fs::write(&path, future_content).unwrap();

        let cfg = load_initial(&path);
        assert_eq!(cfg, Config::default());

        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(
            after, future_content,
            "future version config file must not be overwritten"
        );

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn load_valid_file_applies_and_clamps() {
        let path = temp_config_path("valid_clamps");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let content = r#"
version = 1
[bar]
edge = "left"
thickness = 500
gap = 200
"#;
        fs::write(&path, content).unwrap();

        let cfg = load_initial(&path);
        assert_eq!(cfg.bar.edge, Edge::Left);
        assert_eq!(cfg.bar.thickness, 120);
        assert_eq!(cfg.bar.gap, 64);

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn reload_broken_file_keeps_previous_custom_config() {
        let path = temp_config_path("reload_broken");
        fs::create_dir_all(path.parent().unwrap()).unwrap();

        let mut prev = Config::default();
        prev.bar.edge = Edge::Right;
        prev.bar.thickness = 56;

        // Syntax error during edit
        fs::write(&path, "version = [broken").unwrap();
        let reloaded = reload_keeping_previous(&path, &prev);
        assert_eq!(reloaded.bar.edge, Edge::Right);
        assert_eq!(reloaded.bar.thickness, 56);

        // Temporarily truncated/empty file during editor save
        fs::write(&path, "   \n ").unwrap();
        let reloaded_empty = reload_keeping_previous(&path, &prev);
        assert_eq!(reloaded_empty.bar.edge, Edge::Right);
        assert_eq!(reloaded_empty.bar.thickness, 56);

        // Valid update applies and clamps
        fs::write(
            &path,
            "version = 1\n[bar]\nedge = \"bottom\"\nthickness = 10\n",
        )
        .unwrap();
        let reloaded_valid = reload_keeping_previous(&path, &prev);
        assert_eq!(reloaded_valid.bar.edge, Edge::Bottom);
        assert_eq!(reloaded_valid.bar.thickness, 24);

        let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn watch_posts_wm_config_changed_on_modification() {
        use std::time::Instant;
        use windows::core::w;
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
        ) -> windows::Win32::Foundation::LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }

        let path = temp_config_path("watch_notify");
        let _ = load_initial(&path);

        unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).expect("module handle");
            let class = w!("RustTaskbarConfigWatchTestWindow");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(dummy_wndproc),
                hInstance: hi.into(),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                Default::default(),
                class,
                w!("watch_test"),
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
            .expect("create window");

            let handle = watch(&path, hwnd).expect("start watcher");
            thread::sleep(Duration::from_millis(50));
            fs::write(&path, "version = 1\n[bar]\nedge = \"bottom\"\n").unwrap();

            let start = Instant::now();
            let mut got_msg = false;
            while start.elapsed() < Duration::from_secs(2) {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_CONFIG_CHANGED {
                        got_msg = true;
                        break;
                    }
                }
                if got_msg {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }

            drop(handle);
            let _ = DestroyWindow(hwnd);
            let _ = fs::remove_dir_all(path.parent().unwrap().parent().unwrap());

            assert!(got_msg, "expected WM_CONFIG_CHANGED after modifying config.toml");
        }
    }
}
