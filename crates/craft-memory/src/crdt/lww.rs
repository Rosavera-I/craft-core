//! Last-Write-Wins (LWW) Register CRDT
//!
//! For scalar values where the most recent write wins based on
//! a monotonic timestamp and node ID tiebreaker.

use super::{Mergeable, NodeId};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// A Last-Write-Wins register containing a value
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LwwRegister<T> {
    /// The stored value
    value: T,
    /// Timestamp of when the value was written
    timestamp: u64,
    /// Node ID that performed the write
    node_id: NodeId,
}

impl<T: Clone + PartialEq> LwwRegister<T> {
    /// Create a new LWW register with initial value
    pub fn new(value: T, node_id: NodeId) -> Self {
        Self {
            value,
            timestamp: current_timestamp_millis(),
            node_id,
        }
    }

    /// Create a new LWW register with explicit timestamp
    pub fn with_timestamp(value: T, node_id: NodeId, timestamp: u64) -> Self {
        Self {
            value,
            timestamp,
            node_id,
        }
    }

    /// Get the current value
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Get the timestamp
    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Get the node ID
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Update the value (creates a new logical timestamp)
    pub fn set(&mut self, value: T, node_id: NodeId) {
        let now = current_timestamp_millis();
        // Ensure timestamp is monotonic by advancing if needed
        self.timestamp = now.max(self.timestamp + 1);
        self.value = value;
        self.node_id = node_id;
    }

    /// Update with explicit timestamp (for merging)
    fn set_with_timestamp(&mut self, value: T, node_id: NodeId, timestamp: u64) {
        self.value = value;
        self.timestamp = timestamp;
        self.node_id = node_id;
    }

    /// Compare two LWW registers for ordering
    /// Returns Ordering::Greater if self should win (higher timestamp, or same timestamp with smaller node_id)
    fn compare(&self, other: &Self) -> Ordering {
        // First compare timestamps
        let time_cmp = self.timestamp.cmp(&other.timestamp);
        if time_cmp != Ordering::Equal {
            return time_cmp;
        }
        // Tiebreaker: smaller node ID wins (reverse comparison)
        // "alpha" < "zebra", so "alpha" should win
        other.node_id.0.cmp(&self.node_id.0)
    }
}

impl<T: Clone + PartialEq> Mergeable for LwwRegister<T> {
    fn merge(&mut self, other: &Self) {
        // LWW: keep the value with higher timestamp
        // If timestamps equal, use node ID as tiebreaker
        match self.compare(other) {
            Ordering::Less => {
                // Other wins
                self.set_with_timestamp(
                    other.value.clone(),
                    other.node_id.clone(),
                    other.timestamp,
                );
            }
            Ordering::Equal => {
                // Already equal (shouldn't normally happen with different node IDs at same timestamp)
            }
            Ordering::Greater => {
                // We win, keep our value
            }
        }
    }

    fn dominates(&self, other: &Self) -> bool {
        // We dominate if we won the comparison
        matches!(self.compare(other), Ordering::Greater)
    }
}

impl<T: fmt::Display + Clone + PartialEq> fmt::Display for LwwRegister<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LWW({} @ {} by {})",
            self.value, self.timestamp, self.node_id.0
        )
    }
}

fn current_timestamp_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Serializable fact representation for memory sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFactCrdt {
    pub scope: String,
    pub key: String,
    pub value: String,
    pub register: LwwRegister<()>,
}

impl MemoryFactCrdt {
    pub fn new(
        scope: impl Into<String>,
        key: impl Into<String>,
        value: impl Into<String>,
        node_id: NodeId,
    ) -> Self {
        let scope = scope.into();
        let key = key.into();
        let value_str = value.into();

        Self {
            scope,
            key,
            value: value_str,
            register: LwwRegister::new((), node_id),
        }
    }

    pub fn timestamp(&self) -> u64 {
        self.register.timestamp()
    }

    /// Merge another fact into this one, keeping LWW
    pub fn merge(&mut self, other: &Self) {
        // Only merge if same scope and key
        if self.scope == other.scope && self.key == other.key {
            let old_timestamp = self.register.timestamp();
            self.register.merge(&other.register);
            // If the register was updated, also update the value
            if self.register.timestamp() != old_timestamp {
                self.value = other.value.clone();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lww_new_has_timestamp() {
        let reg: LwwRegister<String> =
            LwwRegister::new("hello".to_string(), NodeId::from("node-1"));
        assert_eq!(reg.value(), "hello");
        assert_eq!(reg.node_id().0, "node-1");
        assert!(reg.timestamp() > 0);
    }

    #[test]
    fn lww_set_updates_value_and_timestamp() {
        let mut reg: LwwRegister<i32> = LwwRegister::new(1, NodeId::from("node-1"));
        let old_ts = reg.timestamp();

        std::thread::sleep(std::time::Duration::from_millis(10));
        reg.set(2, NodeId::from("node-2"));

        assert_eq!(*reg.value(), 2);
        assert_eq!(reg.node_id().0, "node-2");
        assert!(reg.timestamp() > old_ts);
    }

    #[test]
    fn lww_merge_keeps_higher_timestamp() {
        let mut reg1: LwwRegister<String> =
            LwwRegister::with_timestamp("value-1".to_string(), NodeId::from("node-1"), 100);
        let reg2: LwwRegister<String> =
            LwwRegister::with_timestamp("value-2".to_string(), NodeId::from("node-2"), 200);

        reg1.merge(&reg2);

        assert_eq!(*reg1.value(), "value-2");
        assert_eq!(reg1.node_id().0, "node-2");
        assert_eq!(reg1.timestamp(), 200);
    }

    #[test]
    fn lww_merge_keeps_ours_if_lower_timestamp() {
        let mut reg1: LwwRegister<String> =
            LwwRegister::with_timestamp("value-1".to_string(), NodeId::from("node-1"), 200);
        let reg2: LwwRegister<String> =
            LwwRegister::with_timestamp("value-2".to_string(), NodeId::from("node-2"), 100);

        reg1.merge(&reg2);

        assert_eq!(*reg1.value(), "value-1");
        assert_eq!(reg1.node_id().0, "node-1");
    }

    #[test]
    fn lww_tiebreaker_uses_node_id() {
        let mut reg1: LwwRegister<String> =
            LwwRegister::with_timestamp("value-1".to_string(), NodeId::from("zebra"), 100);
        let reg2: LwwRegister<String> =
            LwwRegister::with_timestamp("value-2".to_string(), NodeId::from("alpha"), 100);

        reg1.merge(&reg2);

        // alpha < zebra lexicographically, so alpha's value wins on tie
        assert_eq!(*reg1.value(), "value-2");
        assert_eq!(reg1.node_id().0, "alpha");
    }

    #[test]
    fn lww_dominates_detects_ordering() {
        let reg1: LwwRegister<i32> = LwwRegister::with_timestamp(1, NodeId::from("a"), 100);
        let reg2: LwwRegister<i32> = LwwRegister::with_timestamp(2, NodeId::from("a"), 200);
        let reg3: LwwRegister<i32> = LwwRegister::with_timestamp(3, NodeId::from("a"), 100);

        assert!(!reg1.dominates(&reg2)); // reg1 has lower timestamp
        assert!(reg2.dominates(&reg1)); // reg2 has higher timestamp
        assert!(!reg1.dominates(&reg3)); // same timestamp, equal node IDs
        assert!(!reg3.dominates(&reg1));
    }

    #[test]
    fn memory_fact_crdt_merge() {
        let mut fact1 = MemoryFactCrdt::new("global", "key1", "value1", NodeId::from("node-1"));
        let mut fact2 = MemoryFactCrdt::new("global", "key1", "value2", NodeId::from("node-2"));

        // Manually set timestamps for predictable test
        fact1.register = LwwRegister::with_timestamp((), NodeId::from("node-1"), 100);
        fact2.register = LwwRegister::with_timestamp((), NodeId::from("node-2"), 200);

        fact1.merge(&fact2);

        assert_eq!(fact1.value, "value2");
        assert_eq!(fact1.timestamp(), 200);
    }

    #[test]
    fn memory_fact_different_keys_no_merge() {
        let mut fact1 = MemoryFactCrdt::new("global", "key1", "value1", NodeId::from("node-1"));
        let fact2 = MemoryFactCrdt::new("global", "key2", "value2", NodeId::from("node-2"));

        // Merge should have no effect on different keys
        fact1.merge(&fact2);

        assert_eq!(fact1.key, "key1");
        assert_eq!(fact1.value, "value1");
    }

    #[test]
    fn lww_display_format() {
        let reg: LwwRegister<i32> = LwwRegister::with_timestamp(42, NodeId::from("node-a"), 123456);
        let display = format!("{}", reg);
        assert!(display.contains("42"));
        assert!(display.contains("123456"));
        assert!(display.contains("node-a"));
    }
}
