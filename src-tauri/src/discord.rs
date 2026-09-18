//! Discord Rich Presence (RPC) IPC integration (#958).
//!
//! Connects asynchronously to the local Discord desktop application via named pipes
//! on Windows or Unix domain sockets on Linux/macOS. Formats and dispatches
//! `SET_ACTIVITY` frames when playback changes, displaying track title, artist,
//! album, and progress timestamps.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
#[cfg(unix)]
use tokio::net::UnixStream;

pub const DEFAULT_DISCORD_CLIENT_ID: &str = "1548913001715990610";

/// Cross-platform IPC stream abstraction for Discord RPC.
pub enum IpcStream {
    #[cfg(windows)]
    Windows(NamedPipeClient),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl IpcStream {
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        match self {
            #[cfg(windows)]
            IpcStream::Windows(ref mut s) => s.read_exact(buf).await.map(|_| ()),
            #[cfg(unix)]
            IpcStream::Unix(ref mut s) => s.read_exact(buf).await.map(|_| ()),
        }
    }

    pub async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            #[cfg(windows)]
            IpcStream::Windows(ref mut s) => s.write_all(buf).await,
            #[cfg(unix)]
            IpcStream::Unix(ref mut s) => s.write_all(buf).await,
        }
    }

    pub async fn flush(&mut self) -> std::io::Result<()> {
        match self {
            #[cfg(windows)]
            IpcStream::Windows(ref mut s) => s.flush().await,
            #[cfg(unix)]
            IpcStream::Unix(ref mut s) => s.flush().await,
        }
    }
}

/// Discord RPC packet opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Opcode {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl Opcode {
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            0 => Some(Opcode::Handshake),
            1 => Some(Opcode::Frame),
            2 => Some(Opcode::Close),
            3 => Some(Opcode::Ping),
            4 => Some(Opcode::Pong),
            _ => None,
        }
    }
}

/// Encode a frame with 4-byte LE opcode and 4-byte LE payload length.
pub fn encode_frame(opcode: Opcode, payload: &str) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut buf = Vec::with_capacity(8 + payload.len());
    buf.extend_from_slice(&(opcode as u32).to_le_bytes());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(payload.as_bytes());
    buf
}

/// Decode an 8-byte header into (opcode, payload_length).
pub fn decode_header(header: &[u8; 8]) -> (u32, u32) {
    let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    (opcode, len)
}

/// Connect to the local Discord IPC endpoint.
pub async fn connect_discord_ipc() -> std::io::Result<IpcStream> {
    #[cfg(windows)]
    {
        for i in 0..10 {
            let path = format!(r"\\.\pipe\discord-ipc-{}", i);
            if let Ok(client) = ClientOptions::new().open(&path) {
                log::debug!("Connected to Discord IPC named pipe: {}", path);
                return Ok(IpcStream::Windows(client));
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Discord IPC named pipe not found (Discord might not be running)",
        ))
    }
    #[cfg(unix)]
    {
        let xdg_runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();

        // A bare `$XDG_RUNTIME_DIR/discord-ipc-*` only covers a natively
        // (deb/rpm/tarball) installed Discord. Flatpak and Snap sandbox each
        // app into its own private subdirectory of the runtime dir instead
        // of sharing the top-level socket namespace, so a Flatpak- or
        // Snap-installed Discord (both common on Linux, and Flatpak is the
        // distro-agnostic install path Discord's own download page
        // recommends) never shows up there.
        let mut dirs: Vec<String> = Vec::new();
        if let Some(ref runtime_dir) = xdg_runtime_dir {
            dirs.push(runtime_dir.clone());
            dirs.push(format!("{runtime_dir}/app/com.discordapp.Discord"));
            dirs.push(format!("{runtime_dir}/snap.discord"));
        }
        if let Ok(tmpdir) = std::env::var("TMPDIR") {
            dirs.push(tmpdir);
        }
        dirs.push("/tmp".to_string());

        for dir in dirs {
            for i in 0..10 {
                let path = format!("{}/discord-ipc-{}", dir, i);
                if let Ok(stream) = UnixStream::connect(&path).await {
                    log::debug!("Connected to Discord IPC Unix socket: {}", path);
                    return Ok(IpcStream::Unix(stream));
                }
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Discord IPC socket not found (Discord might not be running)",
        ))
    }
}

/// Connection status of the Discord Rich Presence integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscordStatus {
    Connected,
    Disconnected,
    NotRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordTimestamps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordAssets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordActivity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<DiscordTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<DiscordAssets>,
}

/// Truncate string to Discord's maximum 128 character activity string limit.
pub fn truncate_activity_str(s: &str) -> String {
    if s.chars().count() > 128 {
        s.chars().take(125).collect::<String>() + "..."
    } else {
        s.to_string()
    }
}

/// Manages the Discord IPC connection lifecycle and activity state.
pub struct DiscordManager {
    stream: Option<IpcStream>,
    active_client_id: String,
    status: DiscordStatus,
}

impl Default for DiscordManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscordManager {
    pub fn new() -> Self {
        Self {
            stream: None,
            active_client_id: String::new(),
            status: DiscordStatus::Disconnected,
        }
    }

    pub fn status(&self) -> DiscordStatus {
        self.status
    }

    /// Disconnect from Discord IPC and reset internal state.
    pub fn disconnect(&mut self) {
        self.stream = None;
        self.active_client_id.clear();
        self.status = DiscordStatus::Disconnected;
    }

    /// Ensure an active handshake with the Discord IPC endpoint.
    pub async fn connect(&mut self, client_id: &str) -> Result<(), String> {
        self.ensure_connected(client_id).await
    }

    /// Ensure an active handshake with the Discord IPC endpoint.
    async fn ensure_connected(&mut self, client_id: &str) -> Result<(), String> {
        let trimmed_id = client_id.trim();
        let target_id = if trimmed_id.is_empty() {
            DEFAULT_DISCORD_CLIENT_ID
        } else {
            trimmed_id
        };

        if self.stream.is_some() && self.active_client_id == target_id && self.status == DiscordStatus::Connected {
            return Ok(());
        }

        // Reconnecting or new connection
        self.disconnect();

        let mut stream = match connect_discord_ipc().await {
            Ok(s) => s,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    self.status = DiscordStatus::NotRunning;
                } else {
                    self.status = DiscordStatus::Disconnected;
                }
                return Err(format!("Could not connect to Discord: {e}"));
            }
        };

        // Handshake: Opcode 0
        let handshake_payload = serde_json::json!({
            "v": 1,
            "client_id": target_id
        })
        .to_string();

        let frame_bytes = encode_frame(Opcode::Handshake, &handshake_payload);
        if let Err(e) = stream.write_all(&frame_bytes).await {
            self.status = DiscordStatus::Disconnected;
            return Err(format!("Failed to send Discord handshake: {e}"));
        }
        let _ = stream.flush().await;

        // Read handshake response with a 2-second timeout
        let read_result = tokio::time::timeout(Duration::from_secs(2), async {
            let mut header = [0u8; 8];
            stream.read_exact(&mut header).await?;
            let (_opcode, len) = decode_header(&header);
            let mut body = vec![0u8; len as usize];
            stream.read_exact(&mut body).await?;
            Ok::<Vec<u8>, std::io::Error>(body)
        })
        .await;

        match read_result {
            Ok(Ok(_body)) => {
                self.stream = Some(stream);
                self.active_client_id = target_id.to_string();
                self.status = DiscordStatus::Connected;
                log::info!("Successfully connected and handshook with Discord IPC");
                Ok(())
            }
            Ok(Err(e)) => {
                self.status = DiscordStatus::Disconnected;
                Err(format!("Error reading Discord handshake response: {e}"))
            }
            Err(_) => {
                self.status = DiscordStatus::Disconnected;
                Err("Timed out waiting for Discord handshake response".to_string())
            }
        }
    }

    /// Update the current Discord Rich Presence activity.
    /// If `activity` is `None`, clears the activity.
    pub async fn set_activity(
        &mut self,
        client_id: &str,
        activity: Option<DiscordActivity>,
    ) -> Result<(), String> {
        // If clearing activity while not connected, treat as success
        if activity.is_none() && self.stream.is_none() {
            return Ok(());
        }

        self.ensure_connected(client_id).await?;

        let stream = match self.stream.as_mut() {
            Some(s) => s,
            None => return Err("Discord stream is unavailable".into()),
        };

        let payload = serde_json::json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": std::process::id(),
                "activity": activity,
            },
            "nonce": uuid::Uuid::new_v4().to_string(),
        })
        .to_string();

        let frame_bytes = encode_frame(Opcode::Frame, &payload);
        if let Err(e) = stream.write_all(&frame_bytes).await {
            log::warn!("Discord IPC write error, disconnecting: {e}");
            self.disconnect();
            return Err(format!("Failed to write activity frame: {e}"));
        }
        let _ = stream.flush().await;

        // Drain the response frame with a short timeout to prevent pipe buffering buildup
        let _ = tokio::time::timeout(Duration::from_millis(500), async {
            let mut header = [0u8; 8];
            if stream.read_exact(&mut header).await.is_ok() {
                let (_, len) = decode_header(&header);
                let mut body = vec![0u8; len as usize];
                let _ = stream.read_exact(&mut body).await;
            }
        })
        .await;

        Ok(())
    }

    /// Clear the current Discord activity (`activity: null`).
    pub async fn clear_activity(&mut self, client_id: &str) -> Result<(), String> {
        self.set_activity(client_id, None).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_and_decode_frame() {
        let payload = "{\"test\":true}";
        let encoded = encode_frame(Opcode::Frame, payload);

        assert_eq!(encoded.len(), 8 + payload.len());

        let mut header = [0u8; 8];
        header.copy_from_slice(&encoded[0..8]);
        let (opcode, len) = decode_header(&header);

        assert_eq!(opcode, 1);
        assert_eq!(len, payload.len() as u32);
        assert_eq!(&encoded[8..], payload.as_bytes());
    }

    #[test]
    fn test_activity_serialization_with_presence() {
        let activity = DiscordActivity {
            state: Some("by Radiohead".into()),
            details: Some("Karma Police".into()),
            timestamps: Some(DiscordTimestamps {
                start: Some(1725690000),
                end: Some(1725690264),
            }),
            assets: Some(DiscordAssets {
                large_image: Some("luminous_logo".into()),
                large_text: Some("OK Computer".into()),
                small_image: Some("play".into()),
                small_text: Some("Playing".into()),
            }),
        };

        let json = serde_json::to_string(&activity).unwrap();
        assert!(json.contains("\"details\":\"Karma Police\""));
        assert!(json.contains("\"state\":\"by Radiohead\""));
        assert!(json.contains("\"start\":1725690000"));
        assert!(json.contains("\"end\":1725690264"));
        assert!(json.contains("\"large_image\":\"luminous_logo\""));
        assert!(json.contains("\"small_image\":\"play\""));
    }

    #[test]
    fn test_truncate_activity_str() {
        let short = "Short Title";
        assert_eq!(truncate_activity_str(short), "Short Title");

        let long = "A".repeat(150);
        let truncated = truncate_activity_str(&long);
        assert_eq!(truncated.chars().count(), 128);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_status_serialization() {
        assert_eq!(
            serde_json::to_string(&DiscordStatus::Connected).unwrap(),
            "\"connected\""
        );
        assert_eq!(
            serde_json::to_string(&DiscordStatus::Disconnected).unwrap(),
            "\"disconnected\""
        );
        assert_eq!(
            serde_json::to_string(&DiscordStatus::NotRunning).unwrap(),
            "\"not_running\""
        );
    }
}
