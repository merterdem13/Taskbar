use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

use core::{
    is_newer_version, parse_github_release_json, swap_executable_preserving_config, ReleaseInfo,
    UpdateError, GITHUB_REPO,
};

pub const TASKBAR_WINDOW_CLASS: PCWSTR = w!("RustTaskbarWindow");
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Check,
    Apply,
}

pub fn parse_mode(args: &[String]) -> Result<Mode, String> {
    match args {
        [] => Ok(Mode::Check),
        [flag] if flag == "--check" => Ok(Mode::Check),
        [flag] if flag == "--apply" => Ok(Mode::Apply),
        other => Err(format!(
            "unsupported arguments: {:?} (expected --check or --apply)",
            other
        )),
    }
}

pub fn split_http_url(url: &str) -> Result<(bool, String, u16, String), String> {
    let (is_https, default_port, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, 443u16, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, 80u16, r)
    } else {
        return Err(format!("unsupported URL scheme: {url}"));
    };

    let (authority, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], rest[idx..].to_string()),
        None => (rest, "/".to_string()),
    };

    if authority.is_empty() {
        return Err(format!("missing host in URL: {url}"));
    }

    let (host, port) = match authority.split_once(':') {
        Some((h, p)) => {
            let parsed_port: u16 = p
                .parse()
                .map_err(|_| format!("invalid port in URL: {url}"))?;
            (h.to_string(), parsed_port)
        }
        None => (authority.to_string(), default_port),
    };

    Ok((is_https, host, port, path))
}

struct WinHttpHandle(*mut std::ffi::c_void);

impl Drop for WinHttpHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

fn to_wide_null(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub fn http_get_bytes(url: &str) -> Result<(u16, Vec<u8>), String> {
    let (is_https, host, port, path) = split_http_url(url)?;
    let host_wide = to_wide_null(&host);
    let path_wide = to_wide_null(&path);

    unsafe {
        let session = WinHttpHandle(WinHttpOpen(
            w!("Taskbar-Updater"),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return Err("WinHttpOpen failed".to_string());
        }

        let connect = WinHttpHandle(WinHttpConnect(
            session.0,
            PCWSTR(host_wide.as_ptr()),
            port,
            0,
        ));
        if connect.0.is_null() {
            return Err(format!("WinHttpConnect failed for {host}:{port}"));
        }

        let flags = if is_https {
            WINHTTP_FLAG_SECURE
        } else {
            WINHTTP_OPEN_REQUEST_FLAGS(0)
        };
        let request = WinHttpHandle(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            PCWSTR(path_wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        ));
        if request.0.is_null() {
            return Err("WinHttpOpenRequest failed".to_string());
        }

        let headers = w!("Accept: application/vnd.github+json\r\nUser-Agent: Taskbar-Updater\r\n");
        WinHttpSendRequest(request.0, Some(headers.as_wide()), None, 0, 0, 0)
            .map_err(|e| format!("WinHttpSendRequest failed: {e}"))?;

        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|e| format!("WinHttpReceiveResponse failed: {e}"))?;

        let mut status_code: u32 = 0;
        let mut status_size = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status_code as *mut u32 as *mut std::ffi::c_void),
            &mut status_size,
            std::ptr::null_mut(),
        )
        .map_err(|e| format!("WinHttpQueryHeaders failed: {e}"))?;

        let mut body = Vec::new();
        loop {
            let mut available: u32 = 0;
            WinHttpQueryDataAvailable(request.0, &mut available)
                .map_err(|e| format!("WinHttpQueryDataAvailable failed: {e}"))?;
            if available == 0 {
                break;
            }
            let mut chunk = vec![0u8; available as usize];
            let mut read: u32 = 0;
            WinHttpReadData(
                request.0,
                chunk.as_mut_ptr() as *mut std::ffi::c_void,
                available,
                &mut read,
            )
            .map_err(|e| format!("WinHttpReadData failed: {e}"))?;
            chunk.truncate(read as usize);
            body.extend_from_slice(&chunk);
        }

        Ok((status_code as u16, body))
    }
}

pub fn resolve_release_from_json(json: &str) -> Result<ReleaseInfo, UpdateError> {
    for candidate_asset in ["bar.exe", "taskbar.exe"] {
        match parse_github_release_json(json, candidate_asset) {
            Ok(info) => return Ok(info),
            Err(UpdateError::NoMatchingAsset(_)) => continue,
            Err(other) => return Err(other),
        }
    }
    Err(UpdateError::NoMatchingAsset(
        "bar.exe or taskbar.exe".to_string(),
    ))
}

pub fn resolve_install_exe_in_dir(dir: &Path) -> PathBuf {
    let bar_exe = dir.join("bar.exe");
    if bar_exe.exists() {
        return bar_exe;
    }
    let taskbar_exe = dir.join("taskbar.exe");
    if taskbar_exe.exists() {
        return taskbar_exe;
    }
    bar_exe
}

pub fn default_config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("Taskbar").join("config.toml")
}

pub fn close_window_by_class_and_wait(window_class: PCWSTR, timeout: Duration) -> bool {
    unsafe {
        let Ok(hwnd) = FindWindowW(window_class, PCWSTR::null()) else {
            return true;
        };
        if hwnd.0.is_null() {
            return true;
        }
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));

        let start = Instant::now();
        while start.elapsed() < timeout {
            match FindWindowW(window_class, PCWSTR::null()) {
                Ok(h) if !h.0.is_null() => thread::sleep(Duration::from_millis(100)),
                _ => return true,
            }
        }
        match FindWindowW(window_class, PCWSTR::null()) {
            Ok(h) => h.0.is_null(),
            Err(_) => true,
        }
    }
}

fn fetch_latest_release() -> Result<Option<ReleaseInfo>, String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let (status, body) = http_get_bytes(&url)?;
    if status == 404 {
        return Ok(None);
    }
    if !(200..300).contains(&status) {
        return Err(format!("GitHub API returned HTTP {status}"));
    }
    let text = String::from_utf8(body).map_err(|e| format!("invalid UTF-8 response: {e}"))?;
    let info = resolve_release_from_json(&text).map_err(|e| e.to_string())?;
    Ok(Some(info))
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match parse_mode(&raw_args) {
        Ok(m) => m,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };

    let release = match fetch_latest_release() {
        Ok(Some(r)) => r,
        Ok(None) => {
            println!("No releases published yet (up to date)");
            return ExitCode::from(0);
        }
        Err(e) => {
            eprintln!("update check failed: {e}");
            return ExitCode::from(1);
        }
    };

    if !is_newer_version(CURRENT_VERSION, &release.version) {
        println!("Up to date (current: {CURRENT_VERSION}, latest: {})", release.version);
        return ExitCode::from(0);
    }

    match mode {
        Mode::Check => {
            println!(
                "Update available: {} -> {} ({})",
                CURRENT_VERSION, release.version, release.download_url
            );
            ExitCode::from(2)
        }
        Mode::Apply => {
            println!("Downloading update {}...", release.version);
            let (status, bytes) = match http_get_bytes(&release.download_url) {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("failed to download asset: {e}");
                    return ExitCode::from(1);
                }
            };
            if !(200..300).contains(&status) {
                eprintln!("asset download returned HTTP {status}");
                return ExitCode::from(1);
            }

            let staged_exe = std::env::temp_dir().join("taskbar_staged_update.exe");
            if let Err(e) = fs::write(&staged_exe, &bytes) {
                eprintln!("failed to write staged update {}: {e}", staged_exe.display());
                return ExitCode::from(1);
            }

            let _ = close_window_by_class_and_wait(TASKBAR_WINDOW_CLASS, Duration::from_secs(5));

            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("."));
            let install_exe = resolve_install_exe_in_dir(&exe_dir);
            let user_config = default_config_path();

            if let Err(e) =
                swap_executable_preserving_config(&install_exe, &staged_exe, &user_config)
            {
                eprintln!("failed to swap executable: {e}");
                return ExitCode::from(1);
            }

            let _ = fs::remove_file(&staged_exe);
            let _ = Command::new(&install_exe).spawn();
            println!("Updated to {}", release.version);
            ExitCode::from(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_mode_defaults_to_check_and_parses_flags() {
        assert_eq!(parse_mode(&[]).unwrap(), Mode::Check);
        assert_eq!(parse_mode(&["--check".to_string()]).unwrap(), Mode::Check);
        assert_eq!(parse_mode(&["--apply".to_string()]).unwrap(), Mode::Apply);
        assert!(parse_mode(&["--unknown".to_string()]).is_err());
    }

    #[test]
    fn split_http_url_extracts_scheme_host_port_and_path() {
        let (https, host, port, path) =
            split_http_url("https://api.github.com/repos/merterdem13/Taskbar/releases/latest")
                .unwrap();
        assert!(https);
        assert_eq!(host, "api.github.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/repos/merterdem13/Taskbar/releases/latest");

        let (https2, host2, port2, path2) =
            split_http_url("http://localhost:8080/update.exe").unwrap();
        assert!(!https2);
        assert_eq!(host2, "localhost");
        assert_eq!(port2, 8080);
        assert_eq!(path2, "/update.exe");
    }

    #[test]
    fn resolve_release_matches_bar_exe_or_taskbar_exe() {
        let json_bar = r#"{
            "tag_name": "v0.3.0",
            "assets": [
                {
                    "name": "bar.exe",
                    "browser_download_url": "https://github.com/merterdem13/Taskbar/releases/download/v0.3.0/bar.exe"
                }
            ]
        }"#;
        let info_bar = resolve_release_from_json(json_bar).expect("bar.exe asset");
        assert_eq!(info_bar.version, "0.3.0");
        assert_eq!(info_bar.asset_name, "bar.exe");

        let json_taskbar = r#"{
            "tag_name": "v0.3.0",
            "assets": [
                {
                    "name": "taskbar.exe",
                    "browser_download_url": "https://github.com/merterdem13/Taskbar/releases/download/v0.3.0/taskbar.exe"
                }
            ]
        }"#;
        let info_taskbar = resolve_release_from_json(json_taskbar).expect("taskbar.exe asset");
        assert_eq!(info_taskbar.version, "0.3.0");
        assert_eq!(info_taskbar.asset_name, "taskbar.exe");
    }

    #[test]
    fn resolve_install_exe_prefers_existing_binary_name() {
        let temp = std::env::temp_dir().join(format!(
            "taskbar_updater_resolve_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();

        // Default when neither exists is bar.exe
        assert_eq!(resolve_install_exe_in_dir(&temp), temp.join("bar.exe"));

        // When taskbar.exe exists, chooses taskbar.exe
        let tb = temp.join("taskbar.exe");
        std::fs::write(&tb, b"dummy").unwrap();
        assert_eq!(resolve_install_exe_in_dir(&temp), tb);

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn close_window_by_class_returns_true_when_no_window_running() {
        let closed = close_window_by_class_and_wait(
            w!("NonExistentRustTaskbarTestWindowClass"),
            Duration::from_millis(200),
        );
        assert!(closed);
    }
}
