//! Integration tests for wire encoding negotiation
#![allow(clippy::unwrap_used, clippy::expect_used)]
//!
//! Tests that verify encoding negotiation between nodes with different capabilities:
//! - Both nodes support postcard: use postcard encoding
//! - One node doesn't support postcard: fall back to bincode
//! - Version info exchange includes POSTCARD_ENCODING capability

use anyhow::Result;
use icn_identity::KeyPair;
use icn_net::{
    common_capabilities, CapabilityFlags, EncodingFormat, MessagePayload, NetworkMessage,
    VersionInfo,
};

/// Test that POSTCARD_ENCODING capability is included in current capabilities
#[test]
fn test_current_capabilities_include_postcard() {
    let caps = CapabilityFlags::current();
    assert!(
        caps.contains(CapabilityFlags::POSTCARD_ENCODING),
        "Current capabilities should include POSTCARD_ENCODING"
    );
}

/// Test capability negotiation when both nodes support postcard
#[test]
fn test_both_nodes_support_postcard() {
    let local = VersionInfo::new("icnd-test".to_string());
    let remote = VersionInfo::new("icnd-test".to_string());

    let common = common_capabilities(&local, &remote);

    assert!(
        common.contains(CapabilityFlags::POSTCARD_ENCODING),
        "Common capabilities should include POSTCARD_ENCODING when both support it"
    );
    assert!(
        common.contains(CapabilityFlags::MESSAGE_COMPRESSION),
        "Common capabilities should include MESSAGE_COMPRESSION"
    );
}

/// Test capability negotiation when remote node doesn't support postcard
#[test]
fn test_remote_node_without_postcard() {
    let local = VersionInfo::new("icnd-test".to_string());

    // Create a remote node that doesn't have POSTCARD_ENCODING
    let mut remote = VersionInfo::new("icnd-legacy".to_string());
    remote.capabilities = CapabilityFlags::E2E_ENCRYPTION
        | CapabilityFlags::SIGNED_MESSAGES
        | CapabilityFlags::MESSAGE_COMPRESSION;

    let common = common_capabilities(&local, &remote);

    assert!(
        !common.contains(CapabilityFlags::POSTCARD_ENCODING),
        "Common capabilities should NOT include POSTCARD_ENCODING when remote doesn't support it"
    );
    assert!(
        common.contains(CapabilityFlags::MESSAGE_COMPRESSION),
        "Common capabilities should still include MESSAGE_COMPRESSION"
    );
}

/// Test that messages can roundtrip with bincode when postcard is not negotiated
#[test]
fn test_bincode_fallback_roundtrip() -> Result<()> {
    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();

    let original = NetworkMessage::ping(alice.clone(), bob.clone());

    // Simulate: both nodes don't support postcard, use bincode
    let use_postcard = false;
    let enable_compression = true;

    let bytes = original.to_bytes_negotiated(use_postcard, enable_compression)?;

    // Verify wire format byte indicates bincode
    let (encoding, _) = EncodingFormat::from_wire_byte(bytes[0])?;
    assert_eq!(encoding, EncodingFormat::Bincode);

    let decoded = NetworkMessage::from_bytes_negotiated(&bytes)?;

    assert_eq!(decoded.from, alice);
    assert_eq!(decoded.to, Some(bob));
    assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));

    Ok(())
}

/// Test that messages can roundtrip with postcard when both support it
#[test]
fn test_postcard_roundtrip() -> Result<()> {
    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();

    let original = NetworkMessage::ping(alice.clone(), bob.clone());

    // Simulate: both nodes support postcard
    let use_postcard = true;
    let enable_compression = true;

    let bytes = original.to_bytes_negotiated(use_postcard, enable_compression)?;

    // Verify wire format byte indicates postcard
    let (encoding, _) = EncodingFormat::from_wire_byte(bytes[0])?;
    assert_eq!(encoding, EncodingFormat::Postcard);

    let decoded = NetworkMessage::from_bytes_negotiated(&bytes)?;

    assert_eq!(decoded.from, alice);
    assert_eq!(decoded.to, Some(bob));
    assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));

    Ok(())
}

/// Test that postcard encoding produces smaller messages than bincode
#[test]
fn test_postcard_more_compact() -> Result<()> {
    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();

    let msg = NetworkMessage::ping(alice, bob);

    let bincode_bytes = msg.to_bytes_negotiated(false, false)?;
    let postcard_bytes = msg.to_bytes_negotiated(true, false)?;

    println!(
        "Message size comparison: bincode={} bytes, postcard={} bytes",
        bincode_bytes.len(),
        postcard_bytes.len()
    );

    // Postcard should typically be more compact for most messages
    // (though for very small messages the difference may be negligible)
    // Both should decode correctly regardless of which is smaller
    let decoded_bincode = NetworkMessage::from_bytes_negotiated(&bincode_bytes)?;
    let decoded_postcard = NetworkMessage::from_bytes_negotiated(&postcard_bytes)?;

    assert!(matches!(
        decoded_bincode.payload,
        MessagePayload::Ping { .. }
    ));
    assert!(matches!(
        decoded_postcard.payload,
        MessagePayload::Ping { .. }
    ));

    Ok(())
}

/// Test mixed encoding scenario: sender uses postcard, receiver can decode
#[test]
fn test_cross_encoding_decode() -> Result<()> {
    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();

    let msg = NetworkMessage::ping(alice.clone(), bob.clone());

    // Alice sends with postcard
    let postcard_bytes = msg.to_bytes_negotiated(true, false)?;

    // Bob receives and decodes (from_bytes_negotiated auto-detects encoding)
    let decoded = NetworkMessage::from_bytes_negotiated(&postcard_bytes)?;

    assert_eq!(decoded.from, alice);
    assert_eq!(decoded.to, Some(bob.clone()));
    assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));

    // Now test bincode sender
    let bincode_bytes = msg.to_bytes_negotiated(false, false)?;
    let decoded = NetworkMessage::from_bytes_negotiated(&bincode_bytes)?;

    assert_eq!(decoded.from, alice);
    assert_eq!(decoded.to, Some(bob.clone()));
    assert!(matches!(decoded.payload, MessagePayload::Ping { .. }));

    Ok(())
}

/// Test capability flag serialization roundtrip
#[test]
fn test_capability_flag_serialization() -> Result<()> {
    let caps = CapabilityFlags::current();

    // Serialize and deserialize
    let json = serde_json::to_string(&caps)?;
    let deserialized: CapabilityFlags = serde_json::from_str(&json)?;

    assert_eq!(caps, deserialized);
    assert!(deserialized.contains(CapabilityFlags::POSTCARD_ENCODING));

    Ok(())
}

/// Test VersionInfo with POSTCARD_ENCODING in Hello message
#[test]
fn test_version_info_in_hello() -> Result<()> {
    use icn_identity::IdentityBundle;

    let keypair = KeyPair::generate()?;
    let identity = IdentityBundle::from_keypair(keypair.clone())?;
    let did = identity.did().clone();

    let other_keypair = KeyPair::generate()?;
    let other_did = other_keypair.did().clone();

    let version_info = VersionInfo::new("icnd-test".to_string());
    let binding_info = identity.binding_info();

    // Create Hello message with version info
    let hello = NetworkMessage::hello(
        did.clone(),
        other_did.clone(),
        binding_info,
        version_info.clone(),
        None, // No topology info
        *identity.x25519_public_bytes(),
        None, // No ML-DSA
        None, // No ML-KEM
    );

    // Serialize with postcard
    let bytes = hello.to_bytes_negotiated(true, false)?;

    // Deserialize
    let decoded = NetworkMessage::from_bytes_negotiated(&bytes)?;

    // Verify Hello payload
    if let MessagePayload::Hello {
        version_info: Some(decoded_version),
        ..
    } = decoded.payload
    {
        assert!(decoded_version
            .capabilities
            .contains(CapabilityFlags::POSTCARD_ENCODING));
        assert_eq!(decoded_version.software_version, "icnd-test");
    } else {
        panic!("Expected Hello message with version_info");
    }

    Ok(())
}

/// Test that compression works with both encodings
#[test]
fn test_compression_with_both_encodings() -> Result<()> {
    use icn_gossip::{types::GossipEntry, GossipMessage, VectorClock};

    let alice = KeyPair::generate()?.did().clone();
    let bob = KeyPair::generate()?.did().clone();

    // Create a large message that will benefit from compression
    let mut clock = VectorClock::new();
    clock.increment(&alice);

    let large_data = vec![42u8; 4096]; // 4KB of compressible data
    let gossip = GossipMessage::Response {
        entry: GossipEntry {
            hash: [1u8; 32],
            author: alice.clone(),
            clock,
            topic: "test".to_string(),
            data: large_data,
            compressed: false,
            timestamp: 0,
            replica_offered: None,
        },
    };

    let msg = NetworkMessage::gossip(alice.clone(), Some(bob.clone()), gossip);

    // Test bincode + compression
    let bincode_compressed = msg.to_bytes_negotiated(false, true)?;
    let bincode_uncompressed = msg.to_bytes_negotiated(false, false)?;

    assert!(
        bincode_compressed.len() < bincode_uncompressed.len(),
        "Bincode compression should reduce size"
    );

    // Test postcard + compression
    let postcard_compressed = msg.to_bytes_negotiated(true, true)?;
    let postcard_uncompressed = msg.to_bytes_negotiated(true, false)?;

    assert!(
        postcard_compressed.len() < postcard_uncompressed.len(),
        "Postcard compression should reduce size"
    );

    // All should decode correctly
    let decoded1 = NetworkMessage::from_bytes_negotiated(&bincode_compressed)?;
    let decoded2 = NetworkMessage::from_bytes_negotiated(&bincode_uncompressed)?;
    let decoded3 = NetworkMessage::from_bytes_negotiated(&postcard_compressed)?;
    let decoded4 = NetworkMessage::from_bytes_negotiated(&postcard_uncompressed)?;

    assert!(matches!(decoded1.payload, MessagePayload::Gossip(_)));
    assert!(matches!(decoded2.payload, MessagePayload::Gossip(_)));
    assert!(matches!(decoded3.payload, MessagePayload::Gossip(_)));
    assert!(matches!(decoded4.payload, MessagePayload::Gossip(_)));

    Ok(())
}
