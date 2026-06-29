//! Sync protocol implementation
//!
//! Implements the full sync flow:
//! 1. Establish encrypted connection
//! 2. Exchange Merkle roots
//! 3. Request missing facts
//! 4. Apply with conflict resolution

use super::{EncryptedSyncMessage, SyncError, SyncFact, SyncStats};
use crate::crdt::NodeId;
use crate::crdt::lww::MemoryFactCrdt;
use crate::crdt::merkle::MerkleTree;
use crate::crypto::{CryptoError, SymmetricCipher, X25519Secret};
use std::time::{Duration, Instant};

/// Sync protocol message types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SyncMessage {
    /// Request to initiate sync
    SyncRequest {
        /// Our node ID
        node_id: String,
        /// Our root hash
        root_hash: Option<[u8; 32]>,
    },
    /// Response with differences
    SyncResponse {
        /// Facts we have that they need
        facts: Vec<SyncFact>,
        /// Their root hash
        root_hash: Option<[u8; 32]>,
    },
    /// Acknowledge receipt
    SyncAck {
        /// Number of facts received
        count: usize,
    },
    /// Error during sync
    SyncError {
        /// Error message
        message: String,
    },
}

/// Active sync session
#[derive(Debug)]
pub struct SyncSession {
    /// Session ID
    pub id: String,
    /// Peer node ID
    pub peer_id: String,
    /// When session started
    pub started_at: Instant,
    /// Cipher for encryption/decryption
    cipher: Option<SymmetricCipher>,
    /// Current state
    state: SessionState,
    /// Accumulated stats
    stats: SyncStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Initializing,
    Handshaking,
    ExchangingHashes,
    Transferring,
    Complete,
    Failed,
}

impl SyncSession {
    /// Create a new sync session
    pub fn new(id: impl Into<String>, peer_id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            peer_id: peer_id.into(),
            started_at: Instant::now(),
            cipher: None,
            state: SessionState::Initializing,
            stats: SyncStats::default(),
        }
    }

    /// Perform handshake and return shared secret
    pub fn handshake(
        &mut self,
        our_secret: &X25519Secret,
        peer_public: &[u8; 32],
    ) -> Result<(), CryptoError> {
        self.state = SessionState::Handshaking;

        let peer_key = crate::crypto::X25519PublicKey::from(*peer_public);

        // The current sync session derives a peer-specific cipher from configured static keys.
        let shared_secret = our_secret.diffie_hellman(&peer_key);

        self.cipher = Some(SymmetricCipher::from_shared_secret(&shared_secret));
        self.state = SessionState::ExchangingHashes;

        Ok(())
    }

    /// Encrypt a message
    pub fn encrypt(&self, message: &SyncMessage) -> Result<Vec<u8>, SyncError> {
        let cipher = self
            .cipher
            .as_ref()
            .ok_or_else(|| SyncError::Crypto(CryptoError::InvalidKey))?;

        let plaintext =
            serde_json::to_vec(message).map_err(|e| SyncError::Serialization(e.to_string()))?;

        let encrypted = cipher.encrypt(&plaintext)?;
        let payload = EncryptedSyncMessage::new(encrypted);

        serde_json::to_vec(&payload).map_err(|e| SyncError::Serialization(e.to_string()))
    }

    /// Decrypt a message
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<SyncMessage, SyncError> {
        let cipher = self
            .cipher
            .as_ref()
            .ok_or_else(|| SyncError::Crypto(CryptoError::InvalidKey))?;

        let payload: EncryptedSyncMessage = serde_json::from_slice(ciphertext)
            .map_err(|e| SyncError::Serialization(e.to_string()))?;

        let plaintext = cipher.decrypt(&payload.into_payload())?;

        serde_json::from_slice(&plaintext).map_err(|e| SyncError::Serialization(e.to_string()))
    }

    /// Process incoming sync request
    pub fn handle_request(&mut self, request: &SyncMessage) -> Result<SyncMessage, SyncError> {
        match request {
            SyncMessage::SyncRequest { root_hash, .. } => {
                self.state = SessionState::Transferring;

                Ok(SyncMessage::SyncResponse {
                    facts: Vec::new(),
                    root_hash: *root_hash,
                })
            }
            _ => Err(SyncError::Protocol("unexpected message type".to_string())),
        }
    }

    /// Record sent facts
    pub fn record_sent(&mut self, count: usize) {
        self.stats.sent += count;
    }

    /// Record received facts
    pub fn record_received(&mut self, count: usize, merged: usize) {
        self.stats.received += count;
        self.stats.merged += merged;
    }

    /// Complete the session
    pub fn complete(mut self) -> Result<SyncStats, SyncError> {
        self.state = SessionState::Complete;
        Ok(self.stats)
    }

    /// Mark session as failed
    pub fn fail(&mut self, _error: &str) {
        self.state = SessionState::Failed;
    }

    /// Get current state
    pub fn state(&self) -> &str {
        match self.state {
            SessionState::Initializing => "initializing",
            SessionState::Handshaking => "handshaking",
            SessionState::ExchangingHashes => "exchanging_hashes",
            SessionState::Transferring => "transferring",
            SessionState::Complete => "complete",
            SessionState::Failed => "failed",
        }
    }

    /// Get session duration
    pub fn duration(&self) -> Duration {
        self.started_at.elapsed()
    }
}

/// High-level sync protocol handler
#[derive(Debug)]
pub struct SyncProtocol {
    /// Our node ID
    pub node_id: NodeId,
    /// Our static secret
    pub secret: X25519Secret,
}

impl SyncProtocol {
    /// Create a new protocol instance
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: NodeId::from(node_id.into()),
            secret: X25519Secret::generate(),
        }
    }

    /// Initiate a sync session
    pub fn initiate(&self, peer_id: &str) -> SyncSession {
        let session_id = format!(
            "{}-{}-{}",
            self.node_id.0,
            peer_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        SyncSession::new(session_id, peer_id)
    }

    /// Accept an incoming sync request
    pub fn accept(&self, session_id: impl Into<String>, peer_id: impl Into<String>) -> SyncSession {
        SyncSession::new(session_id, peer_id)
    }

    /// Create a sync request message
    pub fn create_request(&self, merkle_root: Option<[u8; 32]>) -> SyncMessage {
        SyncMessage::SyncRequest {
            node_id: self.node_id.0.clone(),
            root_hash: merkle_root,
        }
    }

    /// Compute facts to send based on diff
    pub fn compute_diff(
        &self,
        local_tree: &MerkleTree,
        remote_root: Option<[u8; 32]>,
        facts: &[SyncFact],
    ) -> Vec<SyncFact> {
        if let Some(remote_hash) = remote_root {
            if local_tree.root_hash() != Some(remote_hash) {
                facts.to_vec()
            } else {
                vec![]
            }
        } else {
            // Peer has no data, send everything
            facts.to_vec()
        }
    }

    /// Merge remote facts with local state using CRDTs
    pub fn merge_facts(
        &self,
        remote_facts: &[SyncFact],
        local_facts: &mut Vec<MemoryFactCrdt>,
    ) -> usize {
        let mut merged = 0;

        for remote in remote_facts {
            let remote_crdt = remote.to_crdt(self.node_id.clone());

            if let Some(existing) = local_facts
                .iter_mut()
                .find(|f| f.scope == remote.scope && f.key == remote.key)
            {
                existing.merge(&remote_crdt);
                merged += 1;
            } else {
                local_facts.push(remote_crdt);
                merged += 1;
            }
        }

        merged
    }
}

/// Sync report for completed operations
#[derive(Debug, Clone)]
pub struct SyncReport {
    /// Peer ID -> (success, stats or error)
    pub results: Vec<(String, Result<SyncStats, String>)>,
    pub duration: Duration,
}

impl SyncReport {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            duration: Duration::default(),
        }
    }

    pub fn add_success(&mut self, peer_id: String, stats: SyncStats) {
        self.results.push((peer_id, Ok(stats)));
    }

    pub fn add_failure(&mut self, peer_id: String, error: String) {
        self.results.push((peer_id, Err(error)));
    }

    pub fn was_successful(&self) -> bool {
        self.results.iter().all(|(_, r)| r.is_ok())
    }

    pub fn total_sent(&self) -> usize {
        self.results
            .iter()
            .filter_map(|(_, r)| r.as_ref().ok())
            .map(|s| s.sent)
            .sum()
    }

    pub fn total_received(&self) -> usize {
        self.results
            .iter()
            .filter_map(|(_, r)| r.as_ref().ok())
            .map(|s| s.received)
            .sum()
    }

    pub fn success_count(&self) -> usize {
        self.results.iter().filter(|(_, r)| r.is_ok()).count()
    }

    pub fn failure_count(&self) -> usize {
        self.results.iter().filter(|(_, r)| r.is_err()).count()
    }
}

impl Default for SyncReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_protocol_creates_session() {
        let protocol = SyncProtocol::new("node-1");
        let session = protocol.initiate("node-2");

        assert_eq!(session.peer_id, "node-2");
        assert_eq!(session.state(), "initializing");
    }

    #[test]
    fn sync_protocol_creates_request() {
        let protocol = SyncProtocol::new("node-1");
        let request = protocol.create_request(Some([1u8; 32]));

        match request {
            SyncMessage::SyncRequest { node_id, root_hash } => {
                assert_eq!(node_id, "node-1");
                assert_eq!(root_hash, Some([1u8; 32]));
            }
            _ => panic!("expected SyncRequest"),
        }
    }

    #[test]
    fn sync_session_records_stats() {
        let mut session = SyncSession::new("session-1", "peer-1");

        session.record_sent(10);
        assert_eq!(session.stats.sent, 10);

        session.record_received(5, 3);
        assert_eq!(session.stats.received, 5);
        assert_eq!(session.stats.merged, 3);
    }

    #[test]
    fn sync_report_tracks_results() {
        let mut report = SyncReport::new();

        let stats = SyncStats::default();
        report.add_success("peer-1".to_string(), stats);
        report.add_failure("peer-2".to_string(), "timeout".to_string());

        assert!(!report.was_successful());
        assert_eq!(report.success_count(), 1);
        assert_eq!(report.failure_count(), 1);
    }

    #[test]
    fn sync_message_serialization() {
        let msg = SyncMessage::SyncRequest {
            node_id: "test".to_string(),
            root_hash: Some([0u8; 32]),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SyncMessage = serde_json::from_str(&json).unwrap();

        match decoded {
            SyncMessage::SyncRequest { node_id, .. } => {
                assert_eq!(node_id, "test");
            }
            _ => panic!("wrong message type"),
        }
    }

    #[test]
    fn sync_protocol_compute_diff() {
        let protocol = SyncProtocol::new("node-1");
        let facts = vec![
            SyncFact {
                scope: "global".to_string(),
                key: "k1".to_string(),
                value: "v1".to_string(),
                timestamp: 1,
                source_node: "n1".to_string(),
            },
            SyncFact {
                scope: "global".to_string(),
                key: "k2".to_string(),
                value: "v2".to_string(),
                timestamp: 2,
                source_node: "n1".to_string(),
            },
        ];

        // No tree, no remote hash - should return all facts
        let empty_tree = MerkleTree::new();
        let result = protocol.compute_diff(&empty_tree, None, &facts);
        assert_eq!(result.len(), 2);
    }
}
