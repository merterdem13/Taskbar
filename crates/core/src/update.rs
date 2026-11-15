use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const GITHUB_REPO: &str = "merterdem13/Taskbar";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub version: String,
    pub asset_name: String,
    pub download_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    Json(String),
    NoMatchingAsset(String),
    Io(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::Json(msg) => write!(f, "release json error: {msg}"),
            UpdateError::NoMatchingAsset(asset) => write!(f, "no matching asset: {asset}"),
            UpdateError::Io(msg) => write!(f, "update io error: {msg}"),
        }
    }
}

impl std::error::Error for UpdateError {}

fn parse_semver_tuple(raw: &str) -> Option<(u32, u32, u32)> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    if trimmed.contains('-') || trimmed.contains('+') {
        return None;
    }
    let mut parts = trimmed.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_newer_version(current: &str, candidate_tag: &str) -> bool {
    let Some(curr_tuple) = parse_semver_tuple(current) else {
        return false;
    };
    let Some(cand_tuple) = parse_semver_tuple(candidate_tag) else {
        return false;
    };
    cand_tuple > curr_tuple
}

#[derive(Deserialize)]
struct GithubAssetPayload {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GithubReleasePayload {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubAssetPayload>,
}

pub fn parse_github_release_json(
    json: &str,
    expected_asset: &str,
) -> Result<ReleaseInfo, UpdateError> {
    let payload: GithubReleasePayload =
        serde_json::from_str(json).map_err(|e| UpdateError::Json(e.to_string()))?;

    let asset = payload
        .assets
        .into_iter()
        .find(|a| a.name.eq_ignore_ascii_case(expected_asset))
        .ok_or_else(|| UpdateError::NoMatchingAsset(expected_asset.to_string()))?;

    let version = payload.tag_name.trim().trim_start_matches(['v', 'V']).to_string();

    Ok(ReleaseInfo {
        version,
        asset_name: asset.name,
        download_url: asset.browser_download_url,
    })
}

pub fn swap_executable_preserving_config(
    install_exe: &Path,
    staged_exe: &Path,
    user_config: &Path,
) -> Result<PathBuf, UpdateError> {
    let config_snapshot = if user_config.exists() {
        Some(fs::read(user_config).map_err(|e| UpdateError::Io(e.to_string()))?)
    } else {
        None
    };

    fs::metadata(staged_exe).map_err(|e| UpdateError::Io(e.to_string()))?;

    let backup_path = install_exe.with_extension("exe.old");
    if install_exe.exists() {
        if backup_path.exists() {
            let _ = fs::remove_file(&backup_path);
        }
        fs::rename(install_exe, &backup_path).map_err(|e| UpdateError::Io(e.to_string()))?;
    }

    fs::copy(staged_exe, install_exe).map_err(|e| UpdateError::Io(e.to_string()))?;

    if let Some(expected_bytes) = config_snapshot {
        let actual_bytes = fs::read(user_config).map_err(|e| UpdateError::Io(e.to_string()))?;
        if actual_bytes != expected_bytes {
            return Err(UpdateError::Io(
                "user config was unexpectedly modified during swap".to_string(),
            ));
        }
    }

    Ok(backup_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_test_dir(name: &str) -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "taskbar_update_test_{}_{}_{}",
            std::process::id(),
            name,
            id
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn semver_comparison_handles_v_prefix_and_equal_or_older() {
        assert!(is_newer_version("0.1.0", "v0.2.0"));
        assert!(!is_newer_version("0.2.0", "v0.2.0"));
        assert!(!is_newer_version("0.2.0", "0.1.9"));
        assert!(is_newer_version("v0.1.0", "V0.1.1"));
        assert!(is_newer_version("0.9.9", "1.0.0"));
        assert!(!is_newer_version("0.1.0", "v0.2.0-beta.1"));
        assert!(!is_newer_version("0.1.0", "not-a-version"));
        assert!(!is_newer_version("invalid", "v0.2.0"));
        assert!(!is_newer_version("0.1", "0.2.0"));
    }

    #[test]
    fn parses_release_json_and_extracts_asset_url() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "assets": [
                {
                    "name": "checksums.txt",
                    "browser_download_url": "https://github.com/merterdem13/Taskbar/releases/download/v0.2.0/checksums.txt"
                },
                {
                    "name": "bar.exe",
                    "browser_download_url": "https://github.com/merterdem13/Taskbar/releases/download/v0.2.0/bar.exe"
                }
            ]
        }"#;

        let info = parse_github_release_json(json, "bar.exe").expect("should parse release");
        assert_eq!(
            info,
            ReleaseInfo {
                version: "0.2.0".to_string(),
                asset_name: "bar.exe".to_string(),
                download_url:
                    "https://github.com/merterdem13/Taskbar/releases/download/v0.2.0/bar.exe"
                        .to_string(),
            }
        );

        let missing = parse_github_release_json(json, "missing.exe").unwrap_err();
        assert_eq!(
            missing,
            UpdateError::NoMatchingAsset("missing.exe".to_string())
        );

        let bad_json = parse_github_release_json("{not valid json", "bar.exe").unwrap_err();
        assert!(matches!(bad_json, UpdateError::Json(_)));
    }

    #[test]
    fn swap_executable_replaces_binary_and_leaves_customized_config_untouched() {
        let dir = temp_test_dir("swap_preserves_config");
        let install_exe = dir.join("taskbar.exe");
        let staged_exe = dir.join("taskbar_new.exe");
        let config_dir = dir.join("Taskbar");
        fs::create_dir_all(&config_dir).unwrap();
        let user_config = config_dir.join("config.toml");

        let v1_bytes = b"MZ-taskbar-binary-v1";
        let v2_bytes = b"MZ-taskbar-binary-v2-updated";
        let custom_config = "version = 1\n# user custom comment\n[bar]\nedge = \"right\"\nthickness = 52\n";

        fs::write(&install_exe, v1_bytes).unwrap();
        fs::write(&staged_exe, v2_bytes).unwrap();
        fs::write(&user_config, custom_config).unwrap();

        let backup_path =
            swap_executable_preserving_config(&install_exe, &staged_exe, &user_config)
                .expect("swap should succeed");

        assert_eq!(backup_path, dir.join("taskbar.exe.old"));
        assert_eq!(fs::read(&install_exe).unwrap(), v2_bytes);
        assert_eq!(fs::read(&backup_path).unwrap(), v1_bytes);
        assert_eq!(
            fs::read_to_string(&user_config).unwrap(),
            custom_config,
            "customized config.toml must be preserved byte-for-byte"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
