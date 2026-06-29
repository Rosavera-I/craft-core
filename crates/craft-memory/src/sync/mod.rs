//! Synchronization protocol for distributed memory
//!
//! Provides encrypted peer-to-peer synchronization with:
//! - Peer discovery and management
//! - Incremental sync using Merkle trees
//! - Conflict resolution via CRDTs
//! - WireGuard-inspired handshake for security

pub mod peer;
pub mod protocol;

use crate::crdt::{CrdtError, NodeId, lww::MemoryFactCrdt, vector_clock::SyncCheckpoint};
use crate::crypto::{CryptoError, EncryptedPayload, X25519PublicKey, X25519Secret};
use crate::{Memory, MemoryError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

pub use peer::{PeerConfig, PeerConnection, PeerId, PeerState};
pub use protocol::{SyncProtocol, SyncReport, SyncSession};

/// Errors specific to sync operations
#[derive(Debug)]
pub enum SyncError {
    Crypto(CryptoError),
    Crdt(CrdtError),
    Storage(MemoryError),
    Io(std::io::Error),
    Connection(String),
    HandshakeFailed(String),
    Protocol(String),
    Serialization(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Crypto(e) => write!(f, "crypto error: {}", e),
            SyncError::Crdt(e) => write!(f, "crdt error: {}", e),
            SyncError::Storage(e) => write!(f, "storage error: {}", e),
            SyncError::Io(e) => write!(f, "io error: {}", e),
            SyncError::Connection(s) => write!(f, "connection error: {}", s),
            SyncError::HandshakeFailed(s) => write!(f, "handshake failed: {}", s),
            SyncError::Protocol(s) => write!(f, "protocol error: {}", s),
            SyncError::Serialization(s) => write!(f, "serialization error: {}", s),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<CryptoError> for SyncError {
    fn from(e: CryptoError) -> Self {
        SyncError::Crypto(e)
    }
}

impl From<CrdtError> for SyncError {
    fn from(e: CrdtError) -> Self {
        SyncError::Crdt(e)
    }
}

impl From<MemoryError> for SyncError {
    fn from(e: MemoryError) -> Self {
        SyncError::Storage(e)
    }
}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::Io(e)
    }
}

/// Distributed memory configuration
#[derive(Debug, Clone)]
pub struct DistributedConfig {
    /// Our node ID
    pub node_id: NodeId,
    /// Our encryption secret
    pub secret: X25519Secret,
    /// Sync interval
    pub sync_interval: Duration,
    /// Connection timeout
    pub connection_timeout: Duration,
}

impl DistributedConfig {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: NodeId::from(node_id.into()),
            secret: X25519Secret::generate(),
            sync_interval: Duration::from_secs(300), // 5 minutes default
            connection_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.sync_interval = interval;
        self
    }

    pub fn public_key(&self) -> X25519PublicKey {
        self.secret.public_key()
    }
}

/// A fact to be synchronized with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFact {
    /// The fact data
    pub scope: String,
    pub key: String,
    pub value: String,
    /// CRDT timestamp for LWW
    pub timestamp: u64,
    /// Source node
    pub source_node: String,
}

impl SyncFact {
    pub fn from_memory_fact(fact: &crate::MemoryFact, source_node: &str) -> Self {
        Self {
            scope: fact.scope.storage_key(),
            key: fact.key.clone(),
            value: fact.value.clone(),
            timestamp: fact.created_at as u64,
            source_node: source_node.to_string(),
        }
    }

    pub fn to_crdt(&self, node_id: NodeId) -> MemoryFactCrdt {
        let mut crdt = MemoryFactCrdt::new(
            self.scope.clone(),
            self.key.clone(),
            self.value.clone(),
            node_id.clone(),
        );
        // Set the timestamp to match the original
        crdt.register = crate::crdt::lww::LwwRegister::with_timestamp(
            (),
            NodeId::from(self.source_node.clone()),
            self.timestamp,
        );
        crdt
    }
}

/// Sync batch for efficient transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBatch {
    /// Facts to sync
    pub facts: Vec<SyncFact>,
    /// Checkpoint for this sync
    pub checkpoint: SyncCheckpoint,
    /// Merkle root hash for verification
    pub merkle_root: Option<[u8; 32]>,
}

impl SyncBatch {
    pub fn new(facts: Vec<SyncFact>) -> Self {
        Self {
            facts,
            checkpoint: SyncCheckpoint::new(),
            merkle_root: None,
        }
    }

    pub fn with_checkpoint(mut self, checkpoint: SyncCheckpoint) -> Self {
        self.checkpoint = checkpoint;
        self
    }

    pub fn with_merkle_root(mut self, root: [u8; 32]) -> Self {
        self.merkle_root = Some(root);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.facts.len()
    }
}

/// Encrypted sync message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSyncMessage {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

impl EncryptedSyncMessage {
    pub fn new(payload: EncryptedPayload) -> Self {
        Self {
            nonce: payload.nonce,
            ciphertext: payload.ciphertext,
        }
    }

    pub fn into_payload(self) -> EncryptedPayload {
        EncryptedPayload {
            nonce: self.nonce,
            ciphertext: self.ciphertext,
        }
    }
}

/// Stats from a sync operation
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub sent: usize,
    pub received: usize,
    pub merged: usize,
    pub conflicts: usize,
}

/// The distributed memory sync engine
#[cfg(feature = "crypto")]
pub struct DistributedMemory {
    config: DistributedConfig,
    local: Memory,
    peers: HashMap<String, PeerConfig>,
    checkpoints: HashMap<String, SyncCheckpoint>,
}

#[cfg(feature = "crypto")]
impl DistributedMemory {
    /// Create a new distributed memory instance
    pub fn new(memory: Memory, config: DistributedConfig) -> Self {
        Self {
            config,
            local: memory,
            peers: HashMap::new(),
            checkpoints: HashMap::new(),
        }
    }

    /// Add a peer configuration
    pub fn add_peer(&mut self, config: PeerConfig) {
        self.peers.insert(config.id.0.clone(), config);
    }

    /// Remove a peer
    pub fn remove_peer(&mut self, peer_id: &str) {
        self.peers.remove(peer_id);
        self.checkpoints.remove(peer_id);
    }

    /// Get our public key.
    pub fn encryption_key(&self) -> X25519PublicKey {
        self.config.public_key()
    }

    /// Get encryption key as value
    pub fn public_key(&self) -> X25519PublicKey {
        self.encryption_key()
    }

    /// Get all configured peers
    pub fn peers(&self) -> &HashMap<String, PeerConfig> {
        &self.peers
    }

    /// Get the sync interval
    pub fn sync_interval(&self) -> Duration {
        self.config.sync_interval
    }

    /// Perform sync with all peers
    pub async fn sync(&mut self) -> Result<crate::sync::protocol::SyncReport, SyncError> {
        let mut report = crate::sync::protocol::SyncReport::new();
        self.sync_into(&mut report).await?;
        Ok(report)
    }

    /// Sync with a specific peer
    async fn sync_with_peer(&mut self, peer_id: &str) -> Result<SyncStats, SyncError> {
        let _peer = self
            .peers
            .get(peer_id)
            .ok_or_else(|| SyncError::Connection(format!("peer {} not found", peer_id)))?;

        let stats = SyncStats::default();

        // Update checkpoint
        let checkpoint = self.checkpoints.entry(peer_id.to_string()).or_default();
        checkpoint.last_sync = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(stats)
    }

    /// Get local memory store
    pub fn local(&self) -> &Memory {
        &self.local
    }

    /// Get local memory store mutably
    pub fn local_mut(&mut self) -> &mut Memory {
        &mut self.local
    }

    /// Perform sync with all peers, collecting into a pre-existing report
    pub async fn sync_into(
        &mut self,
        report: &mut crate::sync::protocol::SyncReport,
    ) -> Result<(), SyncError> {
        let peer_ids: Vec<String> = self.peers.keys().cloned().collect();

        for peer_id in peer_ids {
            match self.sync_with_peer(&peer_id).await {
                Ok(stats) => {
                    report.add_success(peer_id, stats);
                }
                Err(e) => {
                    report.add_failure(peer_id, e.to_string());
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distributed_config_creates_with_defaults() {
        let config = DistributedConfig::new("test-node");
        assert_eq!(config.node_id.0, "test-node");
        assert_eq!(config.sync_interval, Duration::from_secs(300));
    }

    #[test]
    fn distributed_config_with_interval() {
        let config = DistributedConfig::new("test-node").with_interval(Duration::from_secs(60));
        assert_eq!(config.sync_interval, Duration::from_secs(60));
    }

    #[test]
    fn sync_fact_from_memory() {
        use crate::MemoryScope;

        let fact = crate::MemoryFact {
            scope: MemoryScope::Global,
            key: "key1".to_string(),
            value: "value1".to_string(),
            created_at: 1234567890,
        };

        let sync_fact = SyncFact::from_memory_fact(&fact, "node-1");
        assert_eq!(sync_fact.key, "key1");
        assert_eq!(sync_fact.value, "value1");
        assert_eq!(sync_fact.source_node, "node-1");
    }

    #[test]
    fn sync_batch_empty() {
        let batch = SyncBatch::new(vec![]);
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn sync_report_tracking() {
        let mut report = SyncReport::new();

        let stats = SyncStats {
            sent: 10,
            received: 5,
            merged: 3,
            conflicts: 0,
        };

        report.add_success("peer-1".to_string(), stats);
        report.add_failure("peer-2".to_string(), "timeout".to_string());

        assert!(!report.was_successful());
        assert_eq!(report.total_sent(), 10);
        assert_eq!(report.total_received(), 5);
    }

    #[test]
    fn sync_stats_default() {
        let stats = SyncStats::default();
        assert_eq!(stats.sent, 0);
        assert_eq!(stats.received, 0);
        assert_eq!(stats.merged, 0);
    }

    #[test]
    fn encrypted_sync_message_roundtrip() {
        use crate::crypto::SymmetricCipher;

        let key: [u8; 32] = rand::random();
        let cipher = SymmetricCipher::from_shared_secret(&key);

        let plaintext = b"test sync data";
        let encrypted = cipher.encrypt(plaintext).unwrap();

        let message = EncryptedSyncMessage::new(encrypted);
        let payload_back = message.into_payload();

        let decrypted = cipher.decrypt(&payload_back).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
