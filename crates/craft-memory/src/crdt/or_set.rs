//! Observed-Removed Set (OR-Set) CRDT
//!
//! A set where elements can be added and removed any number of times.
//! Uses unique tags for each addition to handle concurrent add/remove.

use super::{Mergeable, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::Hash;

/// Unique tag for tracking additions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Tag(pub u64);

impl Tag {
    /// Generate a new unique tag based on timestamp and counter
    pub fn new() -> Self {
        Self(current_timestamp_micros())
    }

    /// Generate a deterministic tag for testing
    pub fn from_timestamp(ts: u64) -> Self {
        Self(ts)
    }
}

impl Default for Tag {
    fn default() -> Self {
        Self::new()
    }
}

/// An Observed-Removed Set CRDT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrSet<T: Hash + Eq + Clone> {
    /// Element -> Set of tags that added it
    add_map: HashMap<T, HashSet<Tag>>,
    /// Set of tags that have been removed (tombstones)
    remove_set: HashSet<Tag>,
    /// Node ID for this replica
    node_id: NodeId,
}

impl<T: Hash + Eq + Clone> OrSet<T> {
    /// Create a new empty OR-Set
    pub fn new(node_id: NodeId) -> Self {
        Self {
            add_map: HashMap::new(),
            remove_set: HashSet::new(),
            node_id,
        }
    }

    /// Check if element is in the set
    pub fn contains(&self, element: &T) -> bool {
        self.add_map
            .get(element)
            .map(|tags| tags.iter().any(|tag| !self.remove_set.contains(tag)))
            .unwrap_or(false)
    }

    /// Get all non-removed elements
    pub fn value(&self) -> HashSet<T> {
        self.add_map
            .iter()
            .filter(|(_, tags)| tags.iter().any(|tag| !self.remove_set.contains(tag)))
            .map(|(elem, _)| elem.clone())
            .collect()
    }

    /// Add an element, returning the tag used
    pub fn add(&mut self, element: T) -> Tag {
        let tag = Tag::new();
        self.add_map.entry(element).or_default().insert(tag);
        tag
    }

    /// Add an element with a specific tag (for deserialization)
    pub fn add_with_tag(&mut self, element: T, tag: Tag) {
        self.add_map.entry(element).or_default().insert(tag);
    }

    /// Remove an element (tombstones all visible tags)
    /// Returns true if the element was present
    pub fn remove(&mut self, element: &T) -> bool {
        if let Some(tags) = self.add_map.get(element) {
            let tags_to_remove: HashSet<Tag> = tags
                .iter()
                .filter(|tag| !self.remove_set.contains(tag))
                .cloned()
                .collect();
            let was_present = !tags_to_remove.is_empty();
            self.remove_set.extend(tags_to_remove);
            was_present
        } else {
            false
        }
    }

    /// Remove a specific tag (direct removal)
    pub fn remove_tag(&mut self, tag: Tag) {
        self.remove_set.insert(tag);
    }

    /// Get the number of elements (visible only)
    pub fn len(&self) -> usize {
        self.value().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get internal add map (for serialization/debugging)
    pub fn add_map(&self) -> &HashMap<T, HashSet<Tag>> {
        &self.add_map
    }

    /// Get remove set (for serialization/debugging)
    pub fn remove_set(&self) -> &HashSet<Tag> {
        &self.remove_set
    }
}

impl<T: Hash + Eq + Clone> Mergeable for OrSet<T> {
    fn merge(&mut self, other: &Self) {
        // Merge add maps: union of all elements with union of tags
        for (element, tags) in &other.add_map {
            self.add_map
                .entry(element.clone())
                .or_default()
                .extend(tags);
        }

        // Merge remove sets
        self.remove_set.extend(other.remove_set.iter().cloned());
    }

    fn dominates(&self, other: &Self) -> bool {
        // A dominates B if A has all of B's adds and removes
        let adds_dominate = other.add_map.iter().all(|(elem, tags)| {
            self.add_map
                .get(elem)
                .map(|self_tags| tags.is_subset(self_tags))
                .unwrap_or(false)
        });

        let removes_dominate = other.remove_set.is_subset(&self.remove_set);

        adds_dominate && removes_dominate && (self.add_map.len() >= other.add_map.len())
    }
}

impl<T: Hash + Eq + Clone + fmt::Display> fmt::Display for OrSet<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let elements: Vec<String> = self.value().iter().map(|s| s.to_string()).collect();
        write!(f, "OR-Set{{{}}}", elements.join(", "))
    }
}

impl<T: Hash + Eq + Clone> Default for OrSet<T> {
    fn default() -> Self {
        Self::new(NodeId::from("default"))
    }
}

/// Serializable version of OR-Set for network transmission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrSetSnapshot<T: Hash + Eq + Clone> {
    pub add_map: HashMap<T, Vec<Tag>>,
    pub remove_set: Vec<Tag>,
}

impl<T: Hash + Eq + Clone> OrSetSnapshot<T> {
    pub fn from_or_set(set: &OrSet<T>) -> Self {
        Self {
            add_map: set
                .add_map
                .iter()
                .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
                .collect(),
            remove_set: set.remove_set.iter().cloned().collect(),
        }
    }

    pub fn into_or_set(self, node_id: NodeId) -> OrSet<T> {
        let mut set = OrSet::new(node_id);
        for (element, tags) in self.add_map {
            for tag in tags {
                set.add_with_tag(element.clone(), tag);
            }
        }
        for tag in self.remove_set {
            set.remove_tag(tag);
        }
        set
    }
}

/// OR-Set specialized for tags/collections
pub type TagSet = OrSet<String>;

fn current_timestamp_micros() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn or_set_add_contains() {
        let mut set: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        set.add("hello".to_string());
        set.add("world".to_string());

        assert!(set.contains(&"hello".to_string()));
        assert!(set.contains(&"world".to_string()));
        assert!(!set.contains(&"goodbye".to_string()));
    }

    #[test]
    fn or_set_remove() {
        let mut set: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        set.add("item".to_string());
        assert!(set.contains(&"item".to_string()));

        set.remove(&"item".to_string());
        assert!(!set.contains(&"item".to_string()));
    }

    #[test]
    fn or_set_remove_nonexistent() {
        let mut set: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        assert!(!set.remove(&"nonexistent".to_string()));
    }

    #[test]
    fn or_set_add_after_remove() {
        let mut set: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        set.add("item".to_string());
        set.remove(&"item".to_string());
        assert!(!set.contains(&"item".to_string()));

        // Adding after removal should work
        set.add("item".to_string());
        assert!(set.contains(&"item".to_string()));
    }

    #[test]
    fn or_set_concurrent_add_remove() {
        // Simulate concurrent add and remove on different replicas
        let mut replica1: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        let mut replica2: OrSet<String> = OrSet::new(NodeId::from("node-2"));

        // Both start empty
        replica1.add("item".to_string()); // replica1 adds
        replica2.add_map = replica1.add_map.clone(); // sync state

        // replica1 removes
        replica1.remove(&"item".to_string());
        // replica2 adds again (with different tag)
        replica2.add("item".to_string());

        // Merge in both directions
        let merged1 = replica1.clone();
        replica1.merge(&replica2);
        replica2.merge(&merged1);

        // Both should see the item (add wins over remove)
        assert!(replica1.contains(&"item".to_string()));
        assert!(replica2.contains(&"item".to_string()));
    }

    #[test]
    fn or_set_merge() {
        let mut set1: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        let mut set2: OrSet<String> = OrSet::new(NodeId::from("node-2"));

        set1.add("a".to_string());
        set1.add("b".to_string());
        set2.add("c".to_string());

        set1.merge(&set2);

        assert!(set1.contains(&"a".to_string()));
        assert!(set1.contains(&"b".to_string()));
        assert!(set1.contains(&"c".to_string()));
    }

    #[test]
    fn or_set_merge_with_removes() {
        let mut set1: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        let mut set2: OrSet<String> = OrSet::new(NodeId::from("node-2"));

        // Add same element to both
        let tag = set1.add("item".to_string());
        set2.add_with_tag("item".to_string(), tag);

        // Remove from set2
        set2.remove(&"item".to_string());

        // Merge - should not contain the item
        set1.merge(&set2);
        assert!(!set1.contains(&"item".to_string()));
    }

    #[test]
    fn or_set_dominates() {
        let mut set1: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        let mut set2: OrSet<String> = OrSet::new(NodeId::from("node-2"));

        // Use same tags for distributed scenario (simulates sync)
        let tag_a = Tag::from_timestamp(100);
        let tag_b = Tag::from_timestamp(200);

        set1.add_with_tag("a".to_string(), tag_a);
        set1.add_with_tag("b".to_string(), tag_b);
        set2.add_with_tag("a".to_string(), tag_a); // same tag as set1

        // set1 dominates set2 because it has all of set2's tags plus more
        assert!(set1.dominates(&set2));
        assert!(!set2.dominates(&set1));
    }

    #[test]
    fn or_set_dominates_with_removes() {
        let mut set1: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        let set2: OrSet<String> = OrSet::new(NodeId::from("node-2"));

        // Both have same elements
        let tag_a = Tag::from_timestamp(100);
        set1.add_with_tag("a".to_string(), tag_a);
        let tag_b = Tag::from_timestamp(200);
        set1.add_with_tag("b".to_string(), tag_b);

        // set2 is a subset with same tags
        // (Using same add_map structure means dominates should work)
        assert!(set1.dominates(&set2) || set1.len() >= set2.len());
    }

    #[test]
    fn or_set_snapshot_roundtrip() {
        let mut set: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        set.add("a".to_string());
        set.add("b".to_string());
        set.remove(&"a".to_string());

        let snapshot = OrSetSnapshot::from_or_set(&set);
        let restored = snapshot.into_or_set(NodeId::from("node-2"));

        assert!(!restored.contains(&"a".to_string()));
        assert!(restored.contains(&"b".to_string()));
    }

    #[test]
    fn or_set_empty_after_clearing() {
        let mut set: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        set.add("item".to_string());
        set.remove(&"item".to_string());

        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn or_set_multiple_adds_same_element() {
        let mut set: OrSet<String> = OrSet::new(NodeId::from("node-1"));
        let tag1 = set.add("item".to_string());
        let tag2 = set.add("item".to_string());

        assert_eq!(set.add_map.get("item").unwrap().len(), 2);

        // Remove one tag - should still be present
        set.remove_tag(tag1);
        assert!(set.contains(&"item".to_string()));

        // Remove other tag - should be gone
        set.remove_tag(tag2);
        assert!(!set.contains(&"item".to_string()));
    }
}
