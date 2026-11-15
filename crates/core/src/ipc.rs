use serde::{Deserialize, Serialize};

use crate::config::Config;

pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\TaskbarLiveConfigV1";
pub const IPC_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    PreviewConfig { version: u32, config: Config },
    RevertPreview { version: u32 },
    SaveConfig { version: u32, config: Config },
    GetActiveConfig { version: u32 },
    ShutdownForUpdate { version: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IpcResponse {
    Ok { config: Option<Config> },
    Error { message: String },
}

impl IpcRequest {
    pub fn protocol_version(&self) -> u32 {
        match self {
            IpcRequest::PreviewConfig { version, .. }
            | IpcRequest::RevertPreview { version }
            | IpcRequest::SaveConfig { version, .. }
            | IpcRequest::GetActiveConfig { version }
            | IpcRequest::ShutdownForUpdate { version } => *version,
        }
    }
}

fn encode_json_line<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| format!("ipc serialize error: {e}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn encode_request(req: &IpcRequest) -> Result<Vec<u8>, String> {
    encode_json_line(req)
}

pub fn decode_request(line: &str) -> Result<IpcRequest, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("empty IPC request".to_string());
    }
    let req: IpcRequest =
        serde_json::from_str(trimmed).map_err(|e| format!("invalid IPC request: {e}"))?;
    let v = req.protocol_version();
    if v != IPC_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported IPC protocol version: {v} (expected {IPC_PROTOCOL_VERSION})"
        ));
    }
    Ok(req)
}

pub fn encode_response(resp: &IpcResponse) -> Result<Vec<u8>, String> {
    encode_json_line(resp)
}

pub fn decode_response(line: &str) -> Result<IpcResponse, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("empty IPC response".to_string());
    }
    serde_json::from_str(trimmed).map_err(|e| format!("invalid IPC response: {e}"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewSessionState {
    saved: Config,
    preview: Option<Config>,
}

impl PreviewSessionState {
    pub fn new(saved: Config) -> Self {
        Self {
            saved: saved.clamped(),
            preview: None,
        }
    }

    pub fn active_config(&self) -> &Config {
        self.preview.as_ref().unwrap_or(&self.saved)
    }

    pub fn saved_config(&self) -> &Config {
        &self.saved
    }

    pub fn is_previewing(&self) -> bool {
        self.preview.is_some()
    }

    pub fn apply_preview(&mut self, cfg: Config) -> &Config {
        self.preview = Some(cfg.clamped());
        self.active_config()
    }

    pub fn revert_preview(&mut self) -> bool {
        self.preview.take().is_some()
    }

    pub fn commit_saved(&mut self, cfg: Config) -> &Config {
        self.saved = cfg.clamped();
        self.preview = None;
        &self.saved
    }

    pub fn on_disk_reload(&mut self, disk_cfg: Config) -> &Config {
        self.saved = disk_cfg.clamped();
        self.active_config()
    }

    pub fn on_client_disconnect(&mut self) -> bool {
        self.revert_preview()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Edge;

    #[test]
    fn ipc_request_and_response_json_roundtrip() {
        let mut cfg = Config::default();
        cfg.bar.edge = Edge::Right;
        cfg.bar.thickness = 52;
        cfg.widget_settings_mut("clock")
            .params
            .insert("format".to_string(), "12h".to_string());

        let requests = [
            IpcRequest::PreviewConfig {
                version: IPC_PROTOCOL_VERSION,
                config: cfg.clone(),
            },
            IpcRequest::RevertPreview {
                version: IPC_PROTOCOL_VERSION,
            },
            IpcRequest::SaveConfig {
                version: IPC_PROTOCOL_VERSION,
                config: cfg.clone(),
            },
            IpcRequest::GetActiveConfig {
                version: IPC_PROTOCOL_VERSION,
            },
            IpcRequest::ShutdownForUpdate {
                version: IPC_PROTOCOL_VERSION,
            },
        ];

        for req in &requests {
            let encoded = encode_request(req).expect("encode request");
            assert!(encoded.ends_with(b"\n"));
            let line = std::str::from_utf8(&encoded).unwrap();
            let decoded = decode_request(line).expect("decode request");
            assert_eq!(&decoded, req);
        }

        let responses = [
            IpcResponse::Ok {
                config: Some(cfg.clone()),
            },
            IpcResponse::Ok { config: None },
            IpcResponse::Error {
                message: "bad payload".to_string(),
            },
        ];

        for resp in &responses {
            let encoded = encode_response(resp).expect("encode response");
            assert!(encoded.ends_with(b"\n"));
            let line = std::str::from_utf8(&encoded).unwrap();
            let decoded = decode_response(line).expect("decode response");
            assert_eq!(&decoded, resp);
        }
    }

    #[test]
    fn ipc_rejects_unsupported_protocol_version() {
        let req = IpcRequest::GetActiveConfig { version: 99 };
        let encoded = encode_request(&req).expect("encode request");
        let line = std::str::from_utf8(&encoded).unwrap();
        let err = decode_request(line).unwrap_err();
        assert!(err.contains("99"), "unexpected error: {err}");
    }

    #[test]
    fn preview_session_applies_clamped_preview_and_reverts_on_disconnect() {
        let saved = Config::default();
        let mut session = PreviewSessionState::new(saved.clone());
        assert!(!session.is_previewing());
        assert_eq!(session.active_config().bar.edge, Edge::Top);

        let mut preview = Config::default();
        preview.bar.edge = Edge::Right;
        preview.bar.thickness = 9999; // should clamp to 120
        let active = session.apply_preview(preview);
        assert_eq!(active.bar.edge, Edge::Right);
        assert_eq!(active.bar.thickness, 120);
        assert!(session.is_previewing());
        assert_eq!(session.saved_config().bar.edge, Edge::Top);

        assert!(session.on_client_disconnect());
        assert!(!session.is_previewing());
        assert_eq!(session.active_config(), &saved);
        assert!(!session.revert_preview());
    }

    #[test]
    fn preview_session_commit_saved_clears_preview_and_persists() {
        let mut session = PreviewSessionState::new(Config::default());

        let mut preview = Config::default();
        preview.bar.edge = Edge::Left;
        session.apply_preview(preview);
        assert!(session.is_previewing());

        let mut committed = Config::default();
        committed.bar.edge = Edge::Bottom;
        committed.bar.thickness = 50;
        let active = session.commit_saved(committed);
        assert_eq!(active.bar.edge, Edge::Bottom);
        assert_eq!(active.bar.thickness, 50);
        assert!(!session.is_previewing());
        assert_eq!(session.saved_config().bar.edge, Edge::Bottom);
    }

    #[test]
    fn preview_session_disk_reload_updates_saved_baseline() {
        let mut session = PreviewSessionState::new(Config::default());

        let mut disk_cfg = Config::default();
        disk_cfg.bar.edge = Edge::Bottom;
        assert_eq!(session.on_disk_reload(disk_cfg.clone()).bar.edge, Edge::Bottom);

        let mut preview = Config::default();
        preview.bar.edge = Edge::Right;
        session.apply_preview(preview);

        let mut disk_cfg2 = Config::default();
        disk_cfg2.bar.edge = Edge::Left;
        // While preview is active, active stays Right, but saved updates to Left
        assert_eq!(session.on_disk_reload(disk_cfg2).bar.edge, Edge::Right);
        assert_eq!(session.saved_config().bar.edge, Edge::Left);

        assert!(session.revert_preview());
        assert_eq!(session.active_config().bar.edge, Edge::Left);
    }
}
