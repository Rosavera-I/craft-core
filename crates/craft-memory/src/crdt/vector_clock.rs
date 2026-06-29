//! Vector Clock for causality tracking in distributed memory
//!
//! Tracks the happens-before relationship between events across nodes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// A vector clock maps node IDs to logical timestamps
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VectorClock {
    /// Node ID -> Logical clock value
    clock: HashMap<String, u64>,
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        Self {
            clock: HashMap::new(),
        }
    }

    /// Increment the clock for a given node
    pub fn increment(&mut self, node_id: &str) -> u64 {
        let entry = self.clock.entry(node_id.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Get the clock value for a node (0 if not present)
    pub fn get(&self, node_id: &str) -> u64 {
        self.clock.get(node_id).copied().unwrap_or(0)
    }

    /// Merge another vector clock into this one (taking max of each entry)
    pub fn merge(&mut self, other: &Self) {
        for (node, time) in &other.clock {
            let entry = self.clock.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*time);
        }
    }

    /// Check if this clock happened before another (strictly)
    pub fn happened_before(&self, other: &Self) -> bool {
        let mut all_less_or_equal = true;
        let mut at_least_one_less = false;

        // Check all nodes in self
        for (node, time) in &self.clock {
            let other_time = other.get(node);
            if *time > other_time {
                all_less_or_equal = false;
                break;
            }
            if *time < other_time {
                at_least_one_less = true;
            }
        }

        // Check for nodes in other but not in self
        if all_less_or_equal {
            for (node, time) in &other.clock {
                if self.get(node) < *time {
                    at_least_one_less = true;
                    break;
                }
            }
        }

        all_less_or_equal && at_least_one_less
    }

    /// Check if this clock is concurrent with another
    /// (neither happened before the other)
    pub fn concurrent_with(&self, other: &Self) -> bool {
        !self.happened_before(other) && !other.happened_before(self)
    }

    /// Check if this clock dominates another (happened at or after)
    pub fn dominates(&self, other: &Self) -> bool {
        other.happened_before(self) || self == other
    }

    /// Get all nodes in this clock
    pub fn nodes(&self) -> impl Iterator<Item = &String> {
        self.clock.keys()
    }

    /// Returns the maximum clock value across all nodes
    pub fn max_clock(&self) -> u64 {
        self.clock.values().copied().max().unwrap_or(0)
    }

    /// Returns the minimum clock value across all nodes
    pub fn min_clock(&self) -> u64 {
        self.clock.values().copied().min().unwrap_or(0)
    }

    /// Check if this clock is empty
    pub fn is_empty(&self) -> bool {
        self.clock.is_empty()
    }

    /// Number of nodes tracked
    pub fn len(&self) -> usize {
        self.clock.len()
    }

    /// Compare two vector clocks
    /// Returns:
    /// - Ordering::Less if self happened before other
    /// - Ordering::Greater if other happened before self  
    /// - Ordering::Equal if equal
    /// - None if concurrent
    pub fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self == other {
            return Some(std::cmp::Ordering::Equal);
        }
        if self.happened_before(other) {
            return Some(std::cmp::Ordering::Less);
        }
        if other.happened_before(self) {
            return Some(std::cmp::Ordering::Greater);
        }
        None // Concurrent
    }

    /// Get facts that are newer than a given vector clock threshold
    /// Used for incremental sync
    pub fn filter_newer_than<'a, T>(
        &self,
        facts: &'a [T],
        clock_extractor: impl Fn(&T) -> &Self,
    ) -> Vec<&'a T> {
        facts
            .iter()
            .filter(|fact| {
                let fact_clock = clock_extractor(fact);
                // Include if fact clock dominates our clock (has updates we haven't seen)
                fact_clock.dominates(self) && fact_clock != self
            })
            .collect()
    }
}

impl fmt::Display for VectorClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entries: Vec<String> = self
            .clock
            .iter()
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect();
        write!(f, "VC{{{}}}", entries.join(", "))
    }
}

/// VClock version tag for facts
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionTag {
    pub node_id: String,
    pub timestamp: u64,
    pub vector_clock: VectorClock,
}

impl VersionTag {
    pub fn new(node_id: impl Into<String>, timestamp: u64) -> Self {
        Self {
            node_id: node_id.into(),
            timestamp,
            vector_clock: VectorClock::new(),
        }
    }

    pub fn with_clock(mut self, clock: VectorClock) -> Self {
        self.vector_clock = clock;
        self
    }

    pub fn increment(&mut self) -> u64 {
        self.vector_clock.increment(&self.node_id)
    }
}

/// Represents a versioned fact for sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedFact<T> {
    pub data: T,
    pub version: VersionTag,
}

impl<T> VersionedFact<T> {
    pub fn new(data: T, node_id: impl Into<String>) -> Self {
        let timestamp = current_timestamp_millis();
        let node_id = node_id.into();
        let mut version = VersionTag::new(&node_id, timestamp);
        version.increment();

        Self { data, version }
    }

    pub fn with_version(mut self, version: VersionTag) -> Self {
        self.version = version;
        self
    }
}

fn current_timestamp_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Sync checkpoint for tracking what has been synced
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncCheckpoint {
    /// Vector clock marking the last sync point with each peer
    pub clock: VectorClock,
    /// Timestamp of last successful sync
    pub last_sync: u64,
}

impl SyncCheckpoint {
    pub fn new() -> Self {
        Self {
            clock: VectorClock::new(),
            last_sync: 0,
        }
    }

    pub fn update(&mut self, peer_clock: &VectorClock) {
        self.clock.merge(peer_clock);
        self.last_sync = current_timestamp_millis();
    }

    pub fn is_behind(&self, other: &VectorClock) -> bool {
        // We're behind if other is strictly ahead of us (other happened_before us)
        // or if we're concurrent (both have info the other doesn't)
        // If clocks are equal, we're not behind
        if self.clock == *other {
            return false;
        }
        self.clock.happened_before(other) || self.clock.concurrent_with(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_clock_increment() {
        let mut vc = VectorClock::new();
        assert_eq!(vc.increment("node-1"), 1);
        assert_eq!(vc.increment("node-1"), 2);
        assert_eq!(vc.get("node-1"), 2);
        assert_eq!(vc.get("node-2"), 0);
    }

    #[test]
    fn vector_clock_merge_takes_max() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node-1");
        vc1.increment("node-1");

        let mut vc2 = VectorClock::new();
        vc2.increment("node-1");
        vc2.increment("node-2");

        vc1.merge(&vc2);

        assert_eq!(vc1.get("node-1"), 2); // max of 2 and 1
        assert_eq!(vc1.get("node-2"), 1); // from vc2
    }

    #[test]
    fn vector_clock_happened_before() {
        let mut earlier = VectorClock::new();
        earlier.increment("node-1");

        let mut later = VectorClock::new();
        later.increment("node-1");
        later.increment("node-1");

        assert!(earlier.happened_before(&later));
        assert!(!later.happened_before(&earlier));
    }

    #[test]
    fn vector_clock_concurrent() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node-1");

        let mut vc2 = VectorClock::new();
        vc2.increment("node-2");

        assert!(vc1.concurrent_with(&vc2));
        assert!(vc2.concurrent_with(&vc1));
        assert!(!vc1.happened_before(&vc2));
        assert!(!vc2.happened_before(&vc1));
    }

    #[test]
    fn vector_clock_dominates() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node-1");
        vc1.increment("node-1");

        let mut vc2 = VectorClock::new();
        vc2.increment("node-1");

        assert!(vc1.dominates(&vc2));
        assert!(!vc2.dominates(&vc1));

        // Self dominance
        assert!(vc1.dominates(&vc1));
    }

    #[test]
    fn vector_clock_partial_cmp() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node-1");

        let mut vc2 = VectorClock::new();
        vc2.increment("node-1");
        vc2.increment("node-1");

        let mut vc3 = VectorClock::new();
        vc3.increment("node-2");

        assert_eq!(vc1.partial_cmp(&vc2), Some(std::cmp::Ordering::Less));
        assert_eq!(vc2.partial_cmp(&vc1), Some(std::cmp::Ordering::Greater));
        assert_eq!(vc1.partial_cmp(&vc1), Some(std::cmp::Ordering::Equal));
        assert_eq!(vc1.partial_cmp(&vc3), None); // Concurrent
    }

    #[test]
    fn version_tag_increments() {
        let mut tag = VersionTag::new("node-1", 1000);
        assert_eq!(tag.increment(), 1);
        assert_eq!(tag.increment(), 2);
        assert_eq!(tag.vector_clock.get("node-1"), 2);
    }

    #[test]
    fn sync_checkpoint_update() {
        let mut checkpoint = SyncCheckpoint::new();

        let mut peer_clock = VectorClock::new();
        peer_clock.increment("peer-1");
        peer_clock.increment("peer-1");

        checkpoint.update(&peer_clock);

        assert_eq!(checkpoint.clock.get("peer-1"), 2);
        assert!(checkpoint.last_sync > 0);
    }

    #[test]
    fn sync_checkpoint_is_behind() {
        let mut checkpoint = SyncCheckpoint::new();
        checkpoint.clock.increment("peer-1");

        let mut ahead = VectorClock::new();
        ahead.increment("peer-1");
        ahead.increment("peer-1");

        assert!(checkpoint.is_behind(&ahead));

        // After updating
        checkpoint.update(&ahead);
        assert!(!checkpoint.is_behind(&ahead));
    }

    #[test]
    fn vector_clock_display() {
        let mut vc = VectorClock::new();
        vc.increment("node-a");
        vc.increment("node-b");
        vc.increment("node-a");

        let display = format!("{}", vc);
        assert!(display.contains("node-a:2"));
        assert!(display.contains("node-b:1"));
    }

    #[test]
    fn vector_clock_not_happened_before_if_only_different_nodes() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node-1");

        let mut vc2 = VectorClock::new();
        vc2.increment("node-2");

        // Neither happened before the other
        assert!(!vc1.happened_before(&vc2));
        assert!(!vc2.happened_before(&vc1));
    }

    #[test]
    fn versioned_fact_creates_with_timestamp() {
        let fact: VersionedFact<String> = VersionedFact::new("data".to_string(), "node-1");
        assert_eq!(fact.data, "data");
        assert_eq!(fact.version.node_id, "node-1");
        assert!(fact.version.timestamp > 0);
    }
}
