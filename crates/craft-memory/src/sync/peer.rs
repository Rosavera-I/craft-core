//! Peer management for distributed memory sync
//!
//! Handles peer discovery, connection state, and peer configuration.

use crate::crypto::X25519PublicKey;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Unique peer identifier
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerId(pub String);

impl From<String> for PeerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for PeerId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Peer connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerState {
    /// Never connected or known
    Unknown,
    /// Discovered but not yet connecting
    Discovered,
    /// Currently attempting connection
    Connecting,
    /// Handshake in progress
    Handshaking,
    /// Connected and ready for sync
    Connected,
    /// Sync in progress
    Syncing,
    /// Connection failed, will retry
    Failed,
    /// Disconnected by peer or timeout
    Disconnected,
}

impl PeerState {
    /// Check if peer can accept sync requests
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Connected | Self::Syncing)
    }

    /// Check if peer is in terminal/failed state
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed | Self::Disconnected)
    }

    /// Check if peer is actively connecting
    pub fn is_connecting(&self) -> bool {
        matches!(self, Self::Connecting | Self::Handshaking)
    }
}

impl fmt::Display for PeerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerState::Unknown => write!(f, "unknown"),
            PeerState::Discovered => write!(f, "discovered"),
            PeerState::Connecting => write!(f, "connecting"),
            PeerState::Handshaking => write!(f, "handshaking"),
            PeerState::Connected => write!(f, "connected"),
            PeerState::Syncing => write!(f, "syncing"),
            PeerState::Failed => write!(f, "failed"),
            PeerState::Disconnected => write!(f, "disconnected"),
        }
    }
}

/// Static peer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Peer ID
    pub id: PeerId,
    /// Peer address for direct connection
    pub address: Option<SocketAddr>,
    /// Peer public key for encryption
    pub public_key: Option<X25519PublicKey>,
    /// Friendly name/alias
    pub alias: Option<String>,
    /// Connection timeout
    #[serde(with = "humankind_serde")]
    pub timeout: Duration,
    /// Retry interval on failure
    #[serde(with = "humankind_serde")]
    pub retry_interval: Duration,
    /// Maximum retry attempts (0 = infinite)
    pub max_retries: u32,
}

impl PeerConfig {
    /// Create a new peer configuration with just an ID
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: PeerId::from(id.into()),
            address: None,
            public_key: None,
            alias: None,
            timeout: Duration::from_secs(30),
            retry_interval: Duration::from_secs(60),
            max_retries: 3,
        }
    }

    /// Set the peer address
    pub fn with_address(mut self, addr: SocketAddr) -> Self {
        self.address = Some(addr);
        self
    }

    /// Set the peer public key
    pub fn with_public_key(mut self, key: X25519PublicKey) -> Self {
        self.public_key = Some(key);
        self
    }

    /// Set a friendly alias
    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// Set connection timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Get display name (alias or ID)
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.id.0)
    }
}

/// Active peer connection
#[derive(Debug)]
pub struct PeerConnection {
    /// Peer configuration
    pub config: PeerConfig,
    /// Current connection state
    pub state: PeerState,
    /// When connection was established
    pub connected_at: Option<Instant>,
    /// Last activity time
    pub last_activity: Option<Instant>,
    /// Number of connection attempts
    pub connect_attempts: u32,
    /// Number of successful syncs
    pub successful_syncs: u32,
    /// Total bytes sent to this peer
    pub bytes_sent: u64,
    /// Total bytes received from this peer
    pub bytes_received: u64,
}

impl PeerConnection {
    /// Create a new peer connection from config
    pub fn new(config: PeerConfig) -> Self {
        Self {
            config,
            state: PeerState::Discovered,
            connected_at: None,
            last_activity: None,
            connect_attempts: 0,
            successful_syncs: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }

    /// Transition to a new state
    pub fn transition(&mut self, new_state: PeerState) {
        self.state = new_state;

        match new_state {
            PeerState::Connecting => {
                self.connect_attempts += 1;
            }
            PeerState::Connected => {
                self.connected_at = Some(Instant::now());
            }
            PeerState::Syncing => {
                self.last_activity = Some(Instant::now());
            }
            PeerState::Failed | PeerState::Disconnected => {
                self.connected_at = None;
            }
            _ => {}
        }
    }

    /// Record successful sync
    pub fn record_sync(&mut self, sent: u64, received: u64) {
        self.successful_syncs += 1;
        self.bytes_sent += sent;
        self.bytes_received += received;
        self.last_activity = Some(Instant::now());
        self.state = PeerState::Connected;
    }

    /// Check if we should retry connection
    pub fn should_retry(&self) -> bool {
        if self.state != PeerState::Failed {
            return false;
        }

        if self.config.max_retries > 0 && self.connect_attempts >= self.config.max_retries {
            return false;
        }

        // Check if enough time has passed since last attempt
        if let Some(last) = self.last_activity {
            return last.elapsed() >= self.config.retry_interval;
        }

        true
    }

    /// Get connection duration if connected
    pub fn connection_duration(&self) -> Option<Duration> {
        self.connected_at.map(|t| t.elapsed())
    }

    /// Get time since last activity
    pub fn idle_duration(&self) -> Option<Duration> {
        self.last_activity.map(|t| t.elapsed())
    }

    /// Check if connection is stale
    pub fn is_stale(&self, max_idle: Duration) -> bool {
        self.idle_duration().map(|d| d > max_idle).unwrap_or(true)
    }
}

/// Helper module for Duration serialization with humankind
mod humankind_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_state_is_ready() {
        assert!(PeerState::Connected.is_ready());
        assert!(PeerState::Syncing.is_ready());
        assert!(!PeerState::Connecting.is_ready());
        assert!(!PeerState::Failed.is_ready());
    }

    #[test]
    fn peer_state_is_failed() {
        assert!(PeerState::Failed.is_failed());
        assert!(PeerState::Disconnected.is_failed());
        assert!(!PeerState::Connected.is_failed());
    }

    #[test]
    fn peer_config_display_name() {
        let config = PeerConfig::new("peer-1").with_alias("My Friend");
        assert_eq!(config.display_name(), "My Friend");

        let config_no_alias = PeerConfig::new("peer-2");
        assert_eq!(config_no_alias.display_name(), "peer-2");
    }

    #[test]
    fn peer_connection_transitions() {
        let config = PeerConfig::new("peer-1");
        let mut conn = PeerConnection::new(config);

        assert_eq!(conn.state, PeerState::Discovered);

        conn.transition(PeerState::Connecting);
        assert_eq!(conn.state, PeerState::Connecting);
        assert_eq!(conn.connect_attempts, 1);

        conn.transition(PeerState::Connected);
        assert_eq!(conn.state, PeerState::Connected);
        assert!(conn.connected_at.is_some());

        conn.transition(PeerState::Syncing);
        assert_eq!(conn.state, PeerState::Syncing);
        assert!(conn.last_activity.is_some());

        conn.record_sync(100, 200);
        assert_eq!(conn.bytes_sent, 100);
        assert_eq!(conn.bytes_received, 200);
        assert_eq!(conn.successful_syncs, 1);
        assert_eq!(conn.state, PeerState::Connected);
    }

    #[test]
    fn peer_connection_should_retry() {
        let config = PeerConfig::new("peer-1").with_timeout(Duration::from_secs(1));
        let mut conn = PeerConnection::new(config);

        // Not failed - shouldn't retry
        conn.state = PeerState::Connected;
        assert!(!conn.should_retry());

        // Failed but no activity recorded yet
        conn.state = PeerState::Failed;
        assert!(conn.should_retry());

        // Max retries exceeded
        conn.connect_attempts = 100;
        conn.config.max_retries = 3;
        assert!(!conn.should_retry());
    }

    #[test]
    fn peer_id_display() {
        let id = PeerId::from("test-peer");
        assert_eq!(format!("{}", id), "test-peer");
    }

    #[test]
    fn peer_state_display() {
        assert_eq!(format!("{}", PeerState::Connected), "connected");
        assert_eq!(format!("{}", PeerState::Failed), "failed");
        assert_eq!(format!("{}", PeerState::Syncing), "syncing");
    }

    #[test]
    fn peer_connection_is_stale() {
        let config = PeerConfig::new("peer-1");
        let mut conn = PeerConnection::new(config);

        // No activity - considered stale
        assert!(conn.is_stale(Duration::from_secs(1)));

        // Activity just happened
        conn.last_activity = Some(Instant::now());
        assert!(!conn.is_stale(Duration::from_secs(1)));
    }

    #[test]
    fn peer_config_with_address() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let config = PeerConfig::new("peer-1").with_address(addr);

        assert_eq!(config.address, Some(addr));
    }
}
