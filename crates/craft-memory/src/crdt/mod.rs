//! Conflict-free Replicated Data Types (CRDTs) for distributed memory
//!
//! Provides:
//! - LWW (Last-Write-Wins) Register for scalar facts
//! - OR-Set (Observed-Removed Set) for collections and tags
//! - Vector clocks for causality tracking
//! - Merkle tree for sync verification

pub mod lww;
pub mod merkle;
pub mod or_set;
pub mod vector_clock;

pub use lww::LwwRegister;
pub use merkle::MerkleTree;
pub use or_set::OrSet;
pub use vector_clock::VectorClock;

use serde::{Deserialize, Serialize};
use std::fmt;

/// CRDT error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrdtError {
    MergeConflict(String),
    InvalidState(String),
    Serialization(String),
}

impl fmt::Display for CrdtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CrdtError::MergeConflict(msg) => write!(f, "merge conflict: {}", msg),
            CrdtError::InvalidState(msg) => write!(f, "invalid state: {}", msg),
            CrdtError::Serialization(msg) => write!(f, "serialization error: {}", msg),
        }
    }
}

impl std::error::Error for CrdtError {}

/// Trait for CRDT merge operations
pub trait Mergeable {
    /// Merge another CRDT into this one
    fn merge(&mut self, other: &Self);

    /// Check if this CRDT dominates another (happened-after)
    fn dominates(&self, other: &Self) -> bool;

    /// Check if concurrent with another CRDT
    fn concurrent_with(&self, other: &Self) -> bool {
        !self.dominates(other) && !other.dominates(self)
    }
}

/// Serializable CRDT wrapper for network transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtEnvelope<T> {
    pub peer_id: String,
    pub timestamp: u64,
    pub data: T,
}

impl<T> CrdtEnvelope<T> {
    pub fn new(peer_id: impl Into<String>, data: T) -> Self {
        Self {
            peer_id: peer_id.into(),
            timestamp: current_timestamp_secs(),
            data,
        }
    }
}

fn current_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Node identifier for distributed systems
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_display() {
        let id = NodeId::from("peer-1");
        assert_eq!(id.to_string(), "peer-1");
    }

    #[test]
    fn crdt_envelope_creates_timestamp() {
        let envelope: CrdtEnvelope<i32> = CrdtEnvelope::new("peer-1", 42);
        assert_eq!(envelope.peer_id, "peer-1");
        assert_eq!(envelope.data, 42);
        assert!(envelope.timestamp > 0);
    }
}
