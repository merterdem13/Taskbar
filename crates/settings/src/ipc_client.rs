use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};

use core::{
    decode_response, encode_request, Config, IpcRequest, IpcResponse, IPC_PROTOCOL_VERSION,
};

pub struct PipeClient {
    stream: Option<File>,
    pipe_name: String,
}

impl PipeClient {
    pub fn new(pipe_name: impl Into<String>) -> Self {
        let pipe_name = pipe_name.into();
        let stream = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&pipe_name)
            .ok();
        Self { stream, pipe_name }
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    fn ensure_connected(&mut self) -> bool {
        if self.stream.is_none() {
            self.stream = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.pipe_name)
                .ok();
        }
        self.stream.is_some()
    }

    fn send_once_on_stream(&mut self, bytes: &[u8]) -> Option<IpcResponse> {
        let file = self.stream.as_mut()?;
        if file.write_all(bytes).is_err() || file.flush().is_err() {
            return None;
        }
        let reader_file = file.try_clone().ok()?;
        let mut reader = BufReader::new(reader_file);
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        decode_response(&line).ok()
    }

    fn send_request(&mut self, req: &IpcRequest) -> Option<IpcResponse> {
        let bytes = encode_request(req).ok()?;
        if !self.ensure_connected() {
            return None;
        }
        if let Some(resp) = self.send_once_on_stream(&bytes) {
            return Some(resp);
        }
        // Pipe may have broken if bar restarted; reconnect once and retry.
        self.stream = None;
        if !self.ensure_connected() {
            return None;
        }
        let resp = self.send_once_on_stream(&bytes);
        if resp.is_none() {
            self.stream = None;
        }
        resp
    }

    pub fn fetch_active_config(&mut self) -> Option<Config> {
        match self.send_request(&IpcRequest::GetActiveConfig {
            version: IPC_PROTOCOL_VERSION,
        }) {
            Some(IpcResponse::Ok { config }) => config,
            _ => None,
        }
    }

    pub fn send_preview(&mut self, cfg: &Config) -> bool {
        matches!(
            self.send_request(&IpcRequest::PreviewConfig {
                version: IPC_PROTOCOL_VERSION,
                config: cfg.clone(),
            }),
            Some(IpcResponse::Ok { .. })
        )
    }

    pub fn send_save(&mut self, cfg: &Config) -> bool {
        matches!(
            self.send_request(&IpcRequest::SaveConfig {
                version: IPC_PROTOCOL_VERSION,
                config: cfg.clone(),
            }),
            Some(IpcResponse::Ok { .. })
        )
    }

    pub fn send_revert(&mut self) -> bool {
        matches!(
            self.send_request(&IpcRequest::RevertPreview {
                version: IPC_PROTOCOL_VERSION,
            }),
            Some(IpcResponse::Ok { .. })
        )
    }
}
