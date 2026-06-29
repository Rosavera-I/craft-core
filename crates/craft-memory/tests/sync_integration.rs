//! Integration tests for distributed memory sync
//!
//! Tests the full sync pipeline: encryption, CRDT merging, and protocol handling.

#![cfg(feature = "crypto")]
#![allow(clippy::unwrap_used)]

use craft_memory::{SyncFact, SyncMessage, SyncProtocol, crdt::NodeId, sync::DistributedConfig};
use std::time::Duration;

/// Test a full encrypted sync handshake and message exchange
#[test]
fn encrypted_sync_handshake_completes() {
    use craft_memory::crypto::SymmetricCipher;

    // Create a shared secret for both sessions (simulating successful DH)
    let shared_secret: [u8; 32] = rand::random();

    // For this test, we use a shared cipher directly to verify the encryption layer
    // In production, the handshake would derive this shared_secret
    let cipher = SymmetricCipher::from_shared_secret(&shared_secret);

    // Create and encrypt a sync request using the cipher directly
    let request = SyncMessage::SyncRequest {
        node_id: "node-1".to_string(),
        root_hash: Some([0u8; 32]),
    };

    let request_json = serde_json::to_string(&request).unwrap();
    let encrypted = cipher.encrypt(request_json.as_bytes()).unwrap();

    // Decrypt on node-2
    let decrypted = cipher.decrypt(&encrypted).unwrap();
    let decrypted_msg: SyncMessage = serde_json::from_slice(&decrypted).unwrap();

    match decrypted_msg {
        SyncMessage::SyncRequest { node_id, .. } => {
            assert_eq!(node_id, "node-1");
        }
        _ => panic!("Expected SyncRequest"),
    }
}

/// Test sync fact merging with LWW crdt
#[test]
fn sync_fact_lww_merge() {
    let local_facts = vec![SyncFact {
        scope: "global".to_string(),
        key: "k1".to_string(),
        value: "v1".to_string(),
        timestamp: 100,
        source_node: "node-1".to_string(),
    }];

    let remote_facts = vec![SyncFact {
        scope: "global".to_string(),
        key: "k1".to_string(),
        value: "v2".to_string(),
        timestamp: 200, // Later timestamp
        source_node: "node-2".to_string(),
    }];

    let protocol = SyncProtocol::new("node-1");

    // Convert local facts to CRDTs
    let mut local_crdts: Vec<craft_memory::crdt::lww::MemoryFactCrdt> = local_facts
        .into_iter()
        .map(|f| f.to_crdt(NodeId::from("local")))
        .collect();

    // Merge remote facts
    let merged = protocol.merge_facts(&remote_facts, &mut local_crdts);

    assert_eq!(merged, 1);
    assert_eq!(local_crdts[0].value, "v2"); // Higher timestamp wins
}

/// Test protocol computes correct diff based on Merkle roots
#[test]
fn sync_protocol_diff_computation() {
    use craft_memory::crdt::merkle::MerkleTree;

    let protocol = SyncProtocol::new("node-1");

    // Create local facts
    let facts = vec![
        ("a".to_string(), b"1".to_vec()),
        ("b".to_string(), b"2".to_vec()),
    ];
    let tree = MerkleTree::from_entries(&facts);

    // Create sync facts
    let sync_facts = vec![
        SyncFact {
            scope: "global".to_string(),
            key: "a".to_string(),
            value: "1".to_string(),
            timestamp: 1,
            source_node: "node-1".to_string(),
        },
        SyncFact {
            scope: "global".to_string(),
            key: "b".to_string(),
            value: "2".to_string(),
            timestamp: 2,
            source_node: "node-1".to_string(),
        },
    ];

    // Different root hash - should return all facts
    let diff = protocol.compute_diff(&tree, Some([99u8; 32]), &sync_facts);
    assert_eq!(diff.len(), 2);

    // Same root hash - should return empty
    let same_root = protocol.compute_diff(&tree, tree.root_hash(), &sync_facts);
    assert!(same_root.is_empty());
}

/// Test peer configuration and connection lifecycle
#[test]
fn peer_connection_lifecycle() {
    use craft_memory::sync::{PeerConfig, PeerConnection, PeerState};
    use std::net::SocketAddr;

    let addr: SocketAddr = "127.0.0.1:9090".parse().unwrap();
    let config = PeerConfig::new("peer-1")
        .with_address(addr)
        .with_alias("Test Peer");

    let mut conn = PeerConnection::new(config);

    assert_eq!(conn.state, PeerState::Discovered);
    assert_eq!(conn.config.display_name(), "Test Peer");

    // Transition through states
    conn.transition(PeerState::Connecting);
    assert_eq!(conn.connect_attempts, 1);

    conn.transition(PeerState::Connected);
    assert!(conn.connected_at.is_some());

    conn.record_sync(100, 50);
    assert_eq!(conn.successful_syncs, 1);
    assert_eq!(conn.bytes_sent, 100);
    assert_eq!(conn.bytes_received, 50);
}

/// Test distributed memory configuration
#[test]
fn distributed_config_creation() {
    let config = DistributedConfig::new("test-node").with_interval(Duration::from_secs(60));

    assert_eq!(config.node_id.0, "test-node");
    assert_eq!(config.sync_interval, Duration::from_secs(60));

    let _public_key = config.public_key();
}

/// Test full sync report aggregation
#[test]
fn sync_report_aggregation() {
    use craft_memory::sync::SyncReport;

    let mut report = SyncReport::new();

    // Add successful syncs
    report.add_success("peer-1".to_string(), craft_memory::SyncStats::default());
    report.add_success("peer-2".to_string(), craft_memory::SyncStats::default());

    // Add failure
    report.add_failure("peer-3".to_string(), "timeout".to_string());

    assert_eq!(report.success_count(), 2);
    assert_eq!(report.failure_count(), 1);
    assert!(!report.was_successful());
}

/// Test encrypted payload roundtrip with different data sizes
#[test]
fn encrypted_payload_various_sizes() {
    use craft_memory::crypto::SymmetricCipher;

    let key: [u8; 32] = rand::random();
    let cipher = SymmetricCipher::from_shared_secret(&key);

    // Test empty payload
    let encrypted = cipher.encrypt(b"").unwrap();
    let decrypted = cipher.decrypt(&encrypted).unwrap();
    assert_eq!(decrypted, b"");

    // Test small payload
    let small = b"tiny";
    let enc = cipher.encrypt(small).unwrap();
    let dec = cipher.decrypt(&enc).unwrap();
    assert_eq!(dec, small);

    // Test larger payload
    let large = vec![b'x'; 1024];
    let enc = cipher.encrypt(&large).unwrap();
    let dec = cipher.decrypt(&enc).unwrap();
    assert_eq!(dec, large);
}

/// Test node ID comparison and hashing
#[test]
fn node_id_operations() {
    use craft_memory::crdt::NodeId;

    let n1 = NodeId::from("node-a");
    let n2 = NodeId::from("node-b");
    let n3 = NodeId::from("node-a");

    assert_ne!(n1, n2);
    assert_eq!(n1, n3);

    // Hash check (for HashMap usage)
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(n1.clone());
    set.insert(n2.clone());
    assert_eq!(set.len(), 2);
    set.insert(n3);
    assert_eq!(set.len(), 2); // n3 == n1
}

/// Test OR-Set concurrent add/remove behavior
#[test]
fn or_set_concurrent_add_remove() {
    use craft_memory::crdt::Mergeable;
    use craft_memory::crdt::or_set::OrSet;

    // Simulate concurrent add and remove on different replicas
    let node1 = NodeId::from("node-1");
    let mut replica1: OrSet<String> = OrSet::new(node1);

    // Both start empty
    let _tag = replica1.add("item".to_string());
    let mut replica2 = replica1.clone(); // sync state

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

/// Test vector clock causality tracking
#[test]
fn vector_clock_causality() {
    use craft_memory::crdt::vector_clock::VectorClock;

    let mut vc1 = VectorClock::new();
    let mut vc2 = VectorClock::new();

    // Node-1 does some work
    vc1.increment("node-1");
    vc1.increment("node-1");

    // Node-2 does work
    vc2.increment("node-2");

    // vc1 and vc2 are concurrent (neither happened before the other)
    assert!(!vc1.happened_before(&vc2));
    assert!(!vc2.happened_before(&vc1));
    assert!(vc1.concurrent_with(&vc2));

    // Node-2 learns about node-1's work
    vc2.merge(&vc1);

    // Now vc2 dominates vc1
    assert!(vc2.dominates(&vc1));
    assert!(!vc1.dominates(&vc2));
}

/// Test Merkle tree integrity verification
#[test]
fn merkle_tree_integrity() {
    use craft_memory::crdt::merkle::MerkleTree;

    let entries = vec![
        ("a".to_string(), b"1".to_vec()),
        ("b".to_string(), b"2".to_vec()),
        ("c".to_string(), b"3".to_vec()),
    ];
    let tree = MerkleTree::from_entries(&entries);

    // Tree should verify
    assert!(tree.verify());

    // Root hash should be present
    assert!(tree.root_hash().is_some());

    // All keys should be present
    assert!(tree.contains_key("a"));
    assert!(tree.contains_key("b"));
    assert!(tree.contains_key("c"));
}
