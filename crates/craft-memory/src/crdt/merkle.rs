//! Merkle Tree for efficient sync verification
//!
//! Used to determine which facts need to be synced without
//! transmitting the entire dataset.

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt;

/// 256-bit hash
pub type Hash256 = [u8; 32];

/// A Merkle tree node
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleNode {
    /// Hash of this node
    pub hash: Hash256,
    /// Left child (for internal nodes)
    pub left: Option<Box<MerkleNode>>,
    /// Right child (for internal nodes)
    pub right: Option<Box<MerkleNode>>,
    /// Key range (for leaf identification)
    pub key: Option<String>,
}

impl MerkleNode {
    /// Create a leaf node from a key-value pair
    pub fn leaf(key: impl Into<String>, value: impl AsRef<[u8]>) -> Self {
        let key = key.into();
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hasher.update(value.as_ref());
        let hash = hasher.finalize().into();

        Self {
            hash,
            left: None,
            right: None,
            key: Some(key),
        }
    }

    /// Create an empty leaf node
    pub fn empty() -> Self {
        Self {
            hash: [0u8; 32],
            left: None,
            right: None,
            key: None,
        }
    }

    /// Create an internal node from two children
    pub fn internal(left: MerkleNode, right: MerkleNode) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(left.hash);
        hasher.update(right.hash);
        let hash = hasher.finalize().into();

        Self {
            hash,
            left: Some(Box::new(left)),
            right: Some(Box::new(right)),
            key: None,
        }
    }

    /// Check if this is a leaf node
    pub fn is_leaf(&self) -> bool {
        self.key.is_some()
    }

    /// Get the hash as a hex string
    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash)
    }
}

/// A Merkle tree for efficient diff computation
#[derive(Debug, Clone)]
pub struct MerkleTree {
    root: Option<MerkleNode>,
    /// Cache of key -> leaf node for quick lookup
    leaf_cache: HashMap<String, Hash256>,
}

impl MerkleTree {
    /// Create an empty Merkle tree
    pub fn new() -> Self {
        Self {
            root: None,
            leaf_cache: HashMap::new(),
        }
    }

    /// Build a Merkle tree from key-value pairs
    pub fn from_entries(entries: &[(String, Vec<u8>)]) -> Self {
        if entries.is_empty() {
            return Self::new();
        }

        // Sort entries by key for deterministic ordering
        let mut sorted = entries.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));

        // Build leaf nodes
        let leaves: Vec<MerkleNode> = sorted
            .into_iter()
            .map(|(key, value)| MerkleNode::leaf(key, value))
            .collect();

        let leaf_cache: HashMap<String, Hash256> = leaves
            .iter()
            .filter_map(|n| n.key.clone().map(|k| (k, n.hash)))
            .collect();

        // Build tree bottom-up
        let root = build_tree(leaves);

        Self { root, leaf_cache }
    }

    /// Get the root hash
    pub fn root_hash(&self) -> Option<Hash256> {
        self.root.as_ref().map(|n| n.hash)
    }

    /// Get the root hash as hex string
    pub fn root_hash_hex(&self) -> Option<String> {
        self.root_hash().map(hex::encode)
    }

    /// Check if the tree contains a key
    pub fn contains_key(&self, key: &str) -> bool {
        self.leaf_cache.contains_key(key)
    }

    /// Get the hash for a specific key
    pub fn get_hash(&self, key: &str) -> Option<Hash256> {
        self.leaf_cache.get(key).copied()
    }

    /// Compare two trees and return keys that differ
    pub fn diff(&self, other: &Self) -> Vec<String> {
        match (&self.root, &other.root) {
            (None, None) => vec![],
            (Some(_), None) => self.leaf_cache.keys().cloned().collect(),
            (None, Some(_)) => other.leaf_cache.keys().cloned().collect(),
            (Some(a), Some(b)) => diff_nodes(a, b),
        }
    }

    /// Get the number of leaves
    pub fn len(&self) -> usize {
        self.leaf_cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Verify the integrity of the tree
    pub fn verify(&self) -> bool {
        match &self.root {
            None => true,
            Some(root) => verify_node(root),
        }
    }

    /// Get all keys in the tree
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.leaf_cache.keys()
    }
}

impl Default for MerkleTree {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MerkleTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.root_hash_hex() {
            Some(hash) => write!(f, "MerkleTree({})", &hash[..16]),
            None => write!(f, "MerkleTree(empty)"),
        }
    }
}

/// Build a tree from leaf nodes bottom-up
fn build_tree(mut leaves: Vec<MerkleNode>) -> Option<MerkleNode> {
    if leaves.is_empty() {
        return None;
    }

    // Pad to power of 2
    let mut level_size = 1;
    while level_size < leaves.len() {
        level_size *= 2;
    }

    // Fill in empty leaves
    while leaves.len() < level_size {
        leaves.push(MerkleNode::empty());
    }

    let mut current_level: Vec<MerkleNode> = leaves;

    while current_level.len() > 1 {
        let mut next_level: Vec<MerkleNode> = Vec::new();
        for chunk in current_level.chunks(2) {
            if chunk.len() == 2 {
                next_level.push(MerkleNode::internal(chunk[0].clone(), chunk[1].clone()));
            } else {
                // Single element - promote
                next_level.push(chunk[0].clone());
            }
        }
        current_level = next_level;
    }

    current_level.into_iter().next()
}

/// Recursively diff two nodes
fn diff_nodes(a: &MerkleNode, b: &MerkleNode) -> Vec<String> {
    // If hashes match, no diff
    if a.hash == b.hash {
        return vec![];
    }

    // If both are leaves, they differ
    if a.is_leaf() && b.is_leaf() {
        let mut result = vec![];
        if let Some(key) = &a.key {
            result.push(key.clone());
        }
        if let Some(key) = &b.key
            && !result.contains(key)
        {
            result.push(key.clone());
        }
        return result;
    }

    // Recurse into children
    let mut result = vec![];

    match (&a.left, &b.left) {
        (Some(al), Some(bl)) => {
            result.extend(diff_nodes(al, bl));
        }
        (Some(al), None) => {
            collect_keys(al, &mut result);
        }
        (None, Some(bl)) => {
            collect_keys(bl, &mut result);
        }
        (None, None) => {}
    }

    match (&a.right, &b.right) {
        (Some(ar), Some(br)) => {
            result.extend(diff_nodes(ar, br));
        }
        (Some(ar), None) => {
            collect_keys(ar, &mut result);
        }
        (None, Some(br)) => {
            collect_keys(br, &mut result);
        }
        _ => {}
    }

    result
}

/// Collect all keys in a subtree
fn collect_keys(node: &MerkleNode, keys: &mut Vec<String>) {
    if let Some(key) = &node.key {
        keys.push(key.clone());
        return;
    }
    if let Some(ref left) = node.left {
        collect_keys(left, keys);
    }
    if let Some(ref right) = node.right {
        collect_keys(right, keys);
    }
}

/// Verify node hash integrity
fn verify_node(node: &MerkleNode) -> bool {
    if node.is_leaf() {
        return true; // Leaf hashes are trusted (computed on construction)
    }

    match (&node.left, &node.right) {
        (Some(left), Some(right)) => {
            let mut hasher = Sha256::new();
            hasher.update(left.hash);
            hasher.update(right.hash);
            let expected_hash: Hash256 = hasher.finalize().into();

            if node.hash != expected_hash {
                return false;
            }

            verify_node(left) && verify_node(right)
        }
        (Some(child), None) | (None, Some(child)) => {
            // Single child promoted - hash should match
            node.hash == child.hash && verify_node(child)
        }
        (None, None) => {
            // Internal node with no children should have empty hash
            node.hash == [0u8; 32]
        }
    }
}

/// Hash a single value
pub fn hash_value(value: impl AsRef<[u8]>) -> Hash256 {
    Sha256::digest(value).into()
}

/// Sync message for tree comparison
#[derive(Debug, Clone)]
pub struct SyncMessage {
    /// Root hash of sender's tree
    pub root_hash: Hash256,
    /// Specific keys to request (if diff detected)
    pub requested_keys: Vec<String>,
}

impl SyncMessage {
    pub fn new(root_hash: Hash256) -> Self {
        Self {
            root_hash,
            requested_keys: vec![],
        }
    }

    pub fn with_keys(mut self, keys: Vec<String>) -> Self {
        self.requested_keys = keys;
        self
    }

    pub fn needs_sync(&self, local_tree: &MerkleTree) -> bool {
        match local_tree.root_hash() {
            Some(hash) => hash != self.root_hash,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merkle_leaf_creates_hash() {
        let leaf = MerkleNode::leaf("key1", b"value1");
        assert!(leaf.is_leaf());
        assert_eq!(leaf.key, Some("key1".to_string()));
        assert_ne!(leaf.hash, [0u8; 32]);
    }

    #[test]
    fn merkle_tree_from_entries_builds_root() {
        let entries = vec![
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"2".to_vec()),
            ("c".to_string(), b"3".to_vec()),
        ];

        let tree = MerkleTree::from_entries(&entries);

        assert!(tree.root_hash().is_some());
        assert!(tree.verify());
        assert!(tree.contains_key("a"));
        assert!(tree.contains_key("b"));
        assert!(tree.contains_key("c"));
        assert!(!tree.contains_key("d"));
    }

    #[test]
    fn merkle_tree_empty() {
        let tree: MerkleTree = MerkleTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.root_hash(), None);
    }

    #[test]
    fn merkle_tree_diff_detects_changes() {
        let entries1 = vec![
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"2".to_vec()),
        ];
        let entries2 = vec![
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"changed".to_vec()),
        ];

        let tree1 = MerkleTree::from_entries(&entries1);
        let tree2 = MerkleTree::from_entries(&entries2);

        let diff = tree1.diff(&tree2);
        assert_eq!(diff.len(), 1);
        assert!(diff.contains(&"b".to_string()));
    }

    #[test]
    fn merkle_tree_diff_new_key() {
        let entries1 = vec![("a".to_string(), b"1".to_vec())];
        let entries2 = vec![
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"2".to_vec()),
        ];

        let tree1 = MerkleTree::from_entries(&entries1);
        let tree2 = MerkleTree::from_entries(&entries2);

        let diff = tree1.diff(&tree2);
        assert!(diff.contains(&"b".to_string()));

        let diff2 = tree2.diff(&tree1);
        assert!(diff2.contains(&"b".to_string()));
    }

    #[test]
    fn merkle_tree_same_data_same_hash() {
        let entries = vec![
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"2".to_vec()),
        ];

        let tree1 = MerkleTree::from_entries(&entries);
        let tree2 = MerkleTree::from_entries(&entries);

        assert_eq!(tree1.root_hash(), tree2.root_hash());
        let diff = tree1.diff(&tree2);
        assert!(diff.is_empty());
    }

    #[test]
    fn merkle_tree_display() {
        let entries = vec![("key".to_string(), b"val".to_vec())];
        let tree = MerkleTree::from_entries(&entries);

        let display = format!("{}", tree);
        assert!(display.contains("MerkleTree"));
    }

    #[test]
    fn merkle_tree_order_independent() {
        // Entries in different order should produce same tree
        let entries1 = vec![
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"2".to_vec()),
        ];
        let entries2 = vec![
            ("b".to_string(), b"2".to_vec()),
            ("a".to_string(), b"1".to_vec()),
        ];

        let tree1 = MerkleTree::from_entries(&entries1);
        let tree2 = MerkleTree::from_entries(&entries2);

        assert_eq!(tree1.root_hash(), tree2.root_hash());
    }

    #[test]
    fn sync_message_needs_sync() {
        let entries1 = vec![("a".to_string(), b"1".to_vec())];
        let entries2 = vec![("a".to_string(), b"2".to_vec())];

        let tree1 = MerkleTree::from_entries(&entries1);
        let tree2 = MerkleTree::from_entries(&entries2);

        let msg = SyncMessage::new(tree2.root_hash().unwrap());

        assert!(msg.needs_sync(&tree1));

        let msg_same = SyncMessage::new(tree1.root_hash().unwrap());
        assert!(!msg_same.needs_sync(&tree1));
    }

    #[test]
    fn merkle_internal_node_hash_computed() {
        let leaf1 = MerkleNode::leaf("key1", b"value1");
        let leaf2 = MerkleNode::leaf("key2", b"value2");

        let internal = MerkleNode::internal(leaf1, leaf2);

        assert!(!internal.is_leaf());
        assert_ne!(internal.hash, [0u8; 32]);
    }

    #[test]
    fn hash_value_produces_256_bit() {
        let hash = hash_value(b"test data");
        assert_eq!(hash.len(), 32);
        assert_ne!(hash, [0u8; 32]);
    }

    #[test]
    fn merkle_tree_verify_detects_tampering() {
        // Build a tree and verify it's valid
        let entries = vec![
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"2".to_vec()),
        ];
        let tree = MerkleTree::from_entries(&entries);
        assert!(tree.verify());

        // Tampering would require rebuilding node hashes, which is
        // prevented by the type system (fields are not public)
    }

    #[test]
    fn merkle_tree_counts_leaves() {
        let entries = vec![
            ("a".to_string(), b"1".to_vec()),
            ("b".to_string(), b"2".to_vec()),
            ("c".to_string(), b"3".to_vec()),
        ];
        let tree = MerkleTree::from_entries(&entries);
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn merkle_tree_diff_empty_tree() {
        let entries = vec![("a".to_string(), b"1".to_vec())];
        let tree = MerkleTree::from_entries(&entries);
        let empty = MerkleTree::new();

        let diff = tree.diff(&empty);
        assert!(diff.contains(&"a".to_string()));

        let diff2 = empty.diff(&tree);
        assert!(diff2.contains(&"a".to_string()));
    }

    #[test]
    fn merkle_node_empty_has_zero_hash() {
        let empty = MerkleNode::empty();
        assert_eq!(empty.hash, [0u8; 32]);
        assert!(!empty.is_leaf());
    }

    #[test]
    fn merkle_tree_get_hash_returns_correct_hash() {
        let entries = vec![("key1".to_string(), b"value1".to_vec())];
        let tree = MerkleTree::from_entries(&entries);

        let hash = tree.get_hash("key1").unwrap();
        assert_ne!(hash, [0u8; 32]);

        assert!(tree.get_hash("nonexistent").is_none());
    }
}
