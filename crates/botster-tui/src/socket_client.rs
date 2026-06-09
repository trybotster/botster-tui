use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

use botster_core::{EntityFrame, EntityStores};
use serde_json::{Value, json};

pub const SOCKET_PROTOCOL_VERSION: u32 = 2;
pub const SOCKET_PROTOCOL_MIN_SUPPORTED: u32 = 1;
pub const TUI_HUB_SUBSCRIPTION_ID: &str = "tui_hub";
pub const TUI_CORE_ENTITY_TYPES: &[&str] = &[
    "hub",
    "connection_code",
    "session",
    "session_action",
    "workspace",
    "spawn_target",
    "worktree",
];

const FRAME_JSON: u8 = 0x01;
const FRAME_PTY_OUTPUT: u8 = 0x02;
const FRAME_PTY_INPUT: u8 = 0x03;
const FRAME_SCROLLBACK: u8 = 0x04;
const FRAME_PROCESS_EXITED: u8 = 0x05;
const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum HubClientError {
    Io(io::Error),
    Json(serde_json::Error),
    Protocol(String),
}

impl std::fmt::Display for HubClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::Protocol(error) => f.write_str(error),
        }
    }
}

impl std::error::Error for HubClientError {}

impl From<io::Error> for HubClientError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for HubClientError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SocketFrame {
    Json(Value),
    PtyOutput {
        session_uuid: String,
        data: Vec<u8>,
    },
    PtyInput {
        session_uuid: String,
        data: Vec<u8>,
    },
    Scrollback {
        session_uuid: String,
        rows: u16,
        cols: u16,
        data: Vec<u8>,
        kitty_enabled: bool,
    },
    ProcessExited {
        session_uuid: String,
        exit_code: Option<i32>,
    },
}

impl SocketFrame {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Json(value) => {
                let payload = serde_json::to_vec(value).expect("json values serialize");
                encode_raw(FRAME_JSON, &payload)
            }
            Self::PtyOutput { session_uuid, data } => {
                encode_session_frame(FRAME_PTY_OUTPUT, session_uuid, data)
            }
            Self::PtyInput { session_uuid, data } => {
                encode_session_frame(FRAME_PTY_INPUT, session_uuid, data)
            }
            Self::Scrollback {
                session_uuid,
                rows,
                cols,
                data,
                kitty_enabled,
            } => {
                let mut payload = Vec::with_capacity(2 + session_uuid.len() + 5 + data.len());
                encode_session_uuid(&mut payload, session_uuid);
                payload.extend_from_slice(&rows.to_le_bytes());
                payload.extend_from_slice(&cols.to_le_bytes());
                payload.push(u8::from(*kitty_enabled));
                payload.extend_from_slice(data);
                encode_raw(FRAME_SCROLLBACK, &payload)
            }
            Self::ProcessExited {
                session_uuid,
                exit_code,
            } => {
                let mut payload = Vec::with_capacity(2 + session_uuid.len() + 4);
                encode_session_uuid(&mut payload, session_uuid);
                payload.extend_from_slice(&exit_code.unwrap_or(-1).to_le_bytes());
                encode_raw(FRAME_PROCESS_EXITED, &payload)
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<SocketFrame>, HubClientError> {
        self.buf.extend_from_slice(bytes);
        let mut frames = Vec::new();

        loop {
            if self.buf.len() < 4 {
                break;
            }

            let length = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]);
            if length == 0 {
                return Err(HubClientError::Protocol(
                    "hub socket frame length was zero".to_string(),
                ));
            }
            if length > MAX_FRAME_SIZE {
                return Err(HubClientError::Protocol(format!(
                    "hub socket frame length {length} exceeds {MAX_FRAME_SIZE}"
                )));
            }

            let total = 4 + length as usize;
            if self.buf.len() < total {
                break;
            }

            let frame_type = self.buf[4];
            let payload = self.buf[5..total].to_vec();
            self.buf.drain(..total);
            frames.push(decode_frame(frame_type, &payload)?);
        }

        Ok(frames)
    }
}

#[derive(Debug)]
pub struct HubSocketClient {
    stream: UnixStream,
    decoder: FrameDecoder,
}

impl HubSocketClient {
    pub fn connect(path: &Path) -> Result<Self, HubClientError> {
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_millis(250)))?;
        stream.set_write_timeout(Some(Duration::from_secs(1)))?;

        stream.write_all(&hello_frame().encode())?;
        stream.write_all(&subscribe_frame().encode())?;
        stream.write_all(&core_entities_frame().encode())?;
        stream.flush()?;

        let mut client = Self {
            stream,
            decoder: FrameDecoder::default(),
        };
        client.expect_hello_ack(Duration::from_secs(1))?;
        client.stream.set_nonblocking(true)?;
        Ok(client)
    }

    pub fn send_create_agent(
        &mut self,
        request_id: &str,
        issue_or_branch: &str,
        prompt: &str,
    ) -> Result<(), HubClientError> {
        self.write_json(json!({
            "subscriptionId": TUI_HUB_SUBSCRIPTION_ID,
            "data": {
                "type": "create_agent",
                "request_id": request_id,
                "issue_or_branch": issue_or_branch,
                "prompt": prompt,
                "metadata": {
                    "source": "botster-tui-dogfood",
                },
            },
        }))
    }

    pub fn subscribe_terminal(
        &mut self,
        session_uuid: &str,
        rows: u16,
        cols: u16,
    ) -> Result<(), HubClientError> {
        self.write_json(json!({
            "type": "subscribe",
            "channel": "terminal",
            "subscriptionId": terminal_subscription_id(session_uuid),
            "params": {
                "session_uuid": session_uuid,
                "rows": rows,
                "cols": cols,
            },
        }))
    }

    pub fn send_terminal_input(
        &mut self,
        session_uuid: &str,
        data: Vec<u8>,
    ) -> Result<(), HubClientError> {
        self.stream.write_all(
            &SocketFrame::PtyInput {
                session_uuid: session_uuid.to_string(),
                data,
            }
            .encode(),
        )?;
        Ok(())
    }

    pub fn send_resize(
        &mut self,
        session_uuid: &str,
        rows: u16,
        cols: u16,
    ) -> Result<(), HubClientError> {
        self.write_json(json!({
            "subscriptionId": terminal_subscription_id(session_uuid),
            "data": {
                "type": "resize",
                "rows": rows,
                "cols": cols,
            },
        }))
    }

    pub fn read_available(&mut self) -> Result<Vec<SocketFrame>, HubClientError> {
        let mut frames = Vec::new();
        let mut buf = [0u8; 8192];

        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    return Err(HubClientError::Protocol(
                        "hub socket closed by peer".to_string(),
                    ));
                }
                Ok(n) => frames.extend(self.decoder.feed(&buf[..n])?),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::TimedOut => break,
                Err(error) => return Err(HubClientError::Io(error)),
            }
        }

        Ok(frames)
    }

    fn write_json(&mut self, value: Value) -> Result<(), HubClientError> {
        self.stream.write_all(&SocketFrame::Json(value).encode())?;
        Ok(())
    }

    fn expect_hello_ack(&mut self, timeout: Duration) -> Result<(), HubClientError> {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 4096];

        while Instant::now() < deadline {
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    return Err(HubClientError::Protocol(
                        "hub socket closed before hello_ack".to_string(),
                    ));
                }
                Ok(n) => {
                    for frame in self.decoder.feed(&buf[..n])? {
                        if frame_is_hello_ack(&frame) {
                            return Ok(());
                        }
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => return Err(HubClientError::Io(error)),
            }
        }

        Err(HubClientError::Protocol(
            "hub socket did not return hello_ack".to_string(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct ClientReadModel {
    pub entities: EntityStores,
    pub terminal_output: String,
    pub attached_session_uuid: Option<String>,
    pub last_command_error: Option<String>,
}

impl ClientReadModel {
    pub fn apply_frame(&mut self, frame: SocketFrame) -> Result<(), HubClientError> {
        match frame {
            SocketFrame::Json(value) => self.apply_json(value),
            SocketFrame::PtyOutput { session_uuid, data }
            | SocketFrame::Scrollback {
                session_uuid, data, ..
            } => {
                self.attached_session_uuid = Some(session_uuid);
                self.terminal_output
                    .push_str(&String::from_utf8_lossy(&data));
                Ok(())
            }
            SocketFrame::ProcessExited {
                session_uuid,
                exit_code,
            } => {
                self.terminal_output.push_str(&format!(
                    "\n[process exited: {} ({})]\n",
                    session_uuid,
                    exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "signal".to_string())
                ));
                Ok(())
            }
            SocketFrame::PtyInput { .. } => Ok(()),
        }
    }

    pub fn sessions(&self) -> Vec<Value> {
        self.entities
            .get(&botster_core::EntityKind("session".to_string()))
            .map(|store| store.iter().map(|(_, record)| record.clone()).collect())
            .unwrap_or_default()
    }

    fn apply_json(&mut self, value: Value) -> Result<(), HubClientError> {
        if value.get("type").and_then(Value::as_str) == Some("command_response") {
            if value.get("ok").and_then(Value::as_bool) == Some(false) {
                self.last_command_error = value
                    .get("error")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
            if let Some(session_uuid) = value.get("session_uuid").and_then(Value::as_str) {
                self.attached_session_uuid = Some(session_uuid.to_string());
            }
        }

        if let Ok(frame) = serde_json::from_value::<EntityFrame>(value.clone()) {
            self.entities.apply_frame(&frame).map_err(|error| {
                HubClientError::Protocol(format!("entity frame apply failed: {error}"))
            })?;
            return Ok(());
        }

        if let Some(data) = value.get("data")
            && let Ok(frame) = serde_json::from_value::<EntityFrame>(data.clone())
        {
            self.entities.apply_frame(&frame).map_err(|error| {
                HubClientError::Protocol(format!("entity frame apply failed: {error}"))
            })?;
        }

        Ok(())
    }
}

pub fn hello_frame() -> SocketFrame {
    SocketFrame::Json(json!({
        "type": "hello",
        "protocol_version": SOCKET_PROTOCOL_VERSION,
        "min_supported_version": SOCKET_PROTOCOL_MIN_SUPPORTED,
        "client": "botster-tui",
    }))
}

pub fn subscribe_frame() -> SocketFrame {
    SocketFrame::Json(json!({
        "type": "subscribe",
        "channel": "hub",
        "subscriptionId": TUI_HUB_SUBSCRIPTION_ID,
    }))
}

pub fn core_entities_frame() -> SocketFrame {
    SocketFrame::Json(json!({
        "subscriptionId": TUI_HUB_SUBSCRIPTION_ID,
        "data": {
            "type": "hub:entities",
            "entity_types": TUI_CORE_ENTITY_TYPES,
        },
    }))
}

pub fn terminal_subscription_id(session_uuid: &str) -> String {
    format!("tui:{session_uuid}")
}

fn frame_is_hello_ack(frame: &SocketFrame) -> bool {
    matches!(frame, SocketFrame::Json(value) if value.get("type").and_then(Value::as_str) == Some("hello_ack"))
}

fn encode_session_frame(frame_type: u8, session_uuid: &str, data: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(2 + session_uuid.len() + data.len());
    encode_session_uuid(&mut payload, session_uuid);
    payload.extend_from_slice(data);
    encode_raw(frame_type, &payload)
}

fn encode_session_uuid(payload: &mut Vec<u8>, session_uuid: &str) {
    let uuid_bytes = session_uuid.as_bytes();
    payload.extend_from_slice(&(uuid_bytes.len() as u16).to_le_bytes());
    payload.extend_from_slice(uuid_bytes);
}

fn encode_raw(frame_type: u8, payload: &[u8]) -> Vec<u8> {
    let length = (payload.len() + 1) as u32;
    let mut buf = Vec::with_capacity(4 + 1 + payload.len());
    buf.extend_from_slice(&length.to_le_bytes());
    buf.push(frame_type);
    buf.extend_from_slice(payload);
    buf
}

fn decode_frame(frame_type: u8, payload: &[u8]) -> Result<SocketFrame, HubClientError> {
    match frame_type {
        FRAME_JSON => Ok(SocketFrame::Json(serde_json::from_slice(payload)?)),
        FRAME_PTY_OUTPUT => {
            let (session_uuid, consumed) = decode_session_uuid(payload)?;
            Ok(SocketFrame::PtyOutput {
                session_uuid,
                data: payload[consumed..].to_vec(),
            })
        }
        FRAME_PTY_INPUT => {
            let (session_uuid, consumed) = decode_session_uuid(payload)?;
            Ok(SocketFrame::PtyInput {
                session_uuid,
                data: payload[consumed..].to_vec(),
            })
        }
        FRAME_SCROLLBACK => decode_scrollback(payload),
        FRAME_PROCESS_EXITED => decode_process_exited(payload),
        _ => Err(HubClientError::Protocol(format!(
            "unknown hub socket frame type 0x{frame_type:02x}"
        ))),
    }
}

fn decode_scrollback(payload: &[u8]) -> Result<SocketFrame, HubClientError> {
    let (session_uuid, consumed) = decode_session_uuid(payload)?;
    if payload.len() < consumed + 5 {
        return Err(HubClientError::Protocol(
            "scrollback frame too short".to_string(),
        ));
    }

    let rows = u16::from_le_bytes([payload[consumed], payload[consumed + 1]]);
    let cols = u16::from_le_bytes([payload[consumed + 2], payload[consumed + 3]]);
    let kitty_enabled = payload[consumed + 4] != 0;
    Ok(SocketFrame::Scrollback {
        session_uuid,
        rows,
        cols,
        kitty_enabled,
        data: payload[consumed + 5..].to_vec(),
    })
}

fn decode_process_exited(payload: &[u8]) -> Result<SocketFrame, HubClientError> {
    let (session_uuid, consumed) = decode_session_uuid(payload)?;
    if payload.len() < consumed + 4 {
        return Err(HubClientError::Protocol(
            "process exited frame too short".to_string(),
        ));
    }
    let raw_code = i32::from_le_bytes([
        payload[consumed],
        payload[consumed + 1],
        payload[consumed + 2],
        payload[consumed + 3],
    ]);
    Ok(SocketFrame::ProcessExited {
        session_uuid,
        exit_code: (raw_code != -1).then_some(raw_code),
    })
}

fn decode_session_uuid(payload: &[u8]) -> Result<(String, usize), HubClientError> {
    if payload.len() < 2 {
        return Err(HubClientError::Protocol(
            "frame too short for session uuid length".to_string(),
        ));
    }
    let uuid_len = u16::from_le_bytes([payload[0], payload[1]]) as usize;
    let total = 2 + uuid_len;
    if payload.len() < total {
        return Err(HubClientError::Protocol(
            "frame too short for session uuid".to_string(),
        ));
    }
    let uuid = std::str::from_utf8(&payload[2..total])
        .map_err(|error| HubClientError::Protocol(format!("invalid session uuid utf8: {error}")))?
        .to_string();
    Ok((uuid, total))
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::thread;

    use super::*;

    #[test]
    fn framing_round_trips_json_and_terminal_frames() {
        let frames = vec![
            hello_frame(),
            SocketFrame::PtyInput {
                session_uuid: "sess-1".to_string(),
                data: b"ls\n".to_vec(),
            },
            SocketFrame::PtyOutput {
                session_uuid: "sess-1".to_string(),
                data: b"prompt> ".to_vec(),
            },
        ];
        let bytes = frames
            .iter()
            .flat_map(SocketFrame::encode)
            .collect::<Vec<_>>();

        let mut decoder = FrameDecoder::default();
        assert_eq!(decoder.feed(&bytes).unwrap(), frames);
    }

    #[test]
    fn core_entity_request_names_every_required_family() {
        let SocketFrame::Json(value) = core_entities_frame() else {
            panic!("core entities frame should be json");
        };
        let entity_types = value
            .pointer("/data/entity_types")
            .and_then(Value::as_array)
            .expect("entity types");

        for required in TUI_CORE_ENTITY_TYPES {
            assert!(
                entity_types
                    .iter()
                    .any(|actual| actual.as_str() == Some(*required)),
                "missing {required}"
            );
        }
    }

    #[test]
    fn hello_ack_probe_accepts_only_protocol_ack() {
        let (stream, mut peer) = UnixStream::pair().expect("socket pair");
        let handle = thread::spawn(move || {
            peer.write_all(&SocketFrame::Json(json!({ "type": "hello_ack" })).encode())
                .expect("write ack");
        });
        let mut client = HubSocketClient {
            stream,
            decoder: FrameDecoder::default(),
        };

        assert!(client.expect_hello_ack(Duration::from_secs(1)).is_ok());
        handle.join().expect("server thread");
    }

    #[test]
    fn connect_fails_when_socket_path_is_missing() {
        let path = std::path::PathBuf::from(format!(
            "/private/tmp/botster-tui-missing-{}.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let error = HubSocketClient::connect(&path).expect_err("missing socket should fail");

        assert!(matches!(error, HubClientError::Io(_)));
    }

    #[test]
    fn hello_ack_probe_rejects_non_ack_json() {
        let (stream, mut peer) = UnixStream::pair().expect("socket pair");
        let handle = thread::spawn(move || {
            peer.write_all(&SocketFrame::Json(json!({ "type": "not_ack" })).encode())
                .expect("write non ack");
        });
        let mut client = HubSocketClient {
            stream,
            decoder: FrameDecoder::default(),
        };

        let error = client
            .expect_hello_ack(Duration::from_millis(25))
            .expect_err("non ack should fail");
        handle.join().expect("server thread");
        assert!(matches!(error, HubClientError::Protocol(_)));
    }

    #[test]
    #[ignore = "sandboxed CI can forbid binding Unix socket paths; run manually for full connect proof"]
    fn connect_requires_hello_ack_before_success() {
        let temp_dir = std::path::PathBuf::from(format!(
            "/private/tmp/botster-tui-socket-test-{}.sock",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&temp_dir);
        let listener = UnixListener::bind(&temp_dir).expect("bind socket");
        let path = temp_dir.clone();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .write_all(&SocketFrame::Json(json!({ "type": "hello_ack" })).encode())
                .expect("write ack");
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
        });

        let client = HubSocketClient::connect(&path);
        let _ = std::fs::remove_file(&path);
        handle.join().expect("server thread");
        assert!(client.is_ok());
    }

    #[test]
    fn read_model_applies_entity_frames_and_terminal_output() {
        let mut model = ClientReadModel::default();
        model
            .apply_frame(SocketFrame::Json(json!({
                "type": "entity_snapshot",
                "entity_type": "session",
                "snapshot_seq": 1,
                "items": [{
                    "session_uuid": "sess-1",
                    "display_name": "Dogfood",
                    "status": "running"
                }]
            })))
            .unwrap();
        model
            .apply_frame(SocketFrame::PtyOutput {
                session_uuid: "sess-1".to_string(),
                data: b"hello\r\n".to_vec(),
            })
            .unwrap();

        assert_eq!(model.sessions().len(), 1);
        assert_eq!(model.attached_session_uuid.as_deref(), Some("sess-1"));
        assert!(model.terminal_output.contains("hello"));
    }
}
