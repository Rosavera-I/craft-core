//! Cryptographic primitives for distributed memory sync
//!
//! Provides AES-256-GCM encryption with X25519 key exchange.
//! WireGuard-inspired noise protocol for secure handshake.

use aes_gcm::{
    Aes256Gcm, Key, Nonce as AesNonce,
    aead::{Aead, KeyInit},
};
use sha2::{Digest, Sha256};
use std::fmt;

/// Size of AES-256 key in bytes
pub const KEY_SIZE: usize = 32;
/// Size of nonce for AES-GCM
pub const NONCE_SIZE: usize = 12;
/// Size of authentication tag
pub const TAG_SIZE: usize = 16;

/// Cryptographic error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    Encryption(String),
    Decryption(String),
    KeyExchange(String),
    InvalidKey,
    InvalidNonce,
    InvalidCiphertext,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::Encryption(msg) => write!(f, "encryption failed: {}", msg),
            CryptoError::Decryption(msg) => write!(f, "decryption failed: {}", msg),
            CryptoError::KeyExchange(msg) => write!(f, "key exchange failed: {}", msg),
            CryptoError::InvalidKey => write!(f, "invalid encryption key"),
            CryptoError::InvalidNonce => write!(f, "invalid nonce"),
            CryptoError::InvalidCiphertext => write!(f, "invalid ciphertext"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// X25519 public key wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X25519PublicKey([u8; 32]);

impl X25519PublicKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for X25519PublicKey {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<x25519_dalek::PublicKey> for X25519PublicKey {
    fn from(key: x25519_dalek::PublicKey) -> Self {
        Self(key.to_bytes())
    }
}

impl serde::Serialize for X25519PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for X25519PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("X25519PublicKey must be 32 bytes"));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(Self(key))
    }
}

/// X25519 secret key for long-term identity.
#[derive(Clone)]
pub struct X25519Secret(x25519_dalek::StaticSecret);

impl std::fmt::Debug for X25519Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("X25519Secret")
            .field("public_key", &self.public_key())
            .finish()
    }
}

impl X25519Secret {
    /// Generate a new random secret key
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut bytes);
        Self(x25519_dalek::StaticSecret::from(bytes))
    }

    /// Get the corresponding public key
    pub fn public_key(&self) -> X25519PublicKey {
        x25519_dalek::PublicKey::from(&self.0).into()
    }

    /// Perform X25519 key exchange to derive shared secret
    pub fn diffie_hellman(&self, other_public: &X25519PublicKey) -> [u8; 32] {
        let peer = x25519_dalek::PublicKey::from(*other_public.as_bytes());
        self.0.diffie_hellman(&peer).to_bytes()
    }
}

/// Encrypted payload with nonce and authentication tag
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedPayload {
    /// 12-byte nonce (96 bits for AES-GCM)
    pub nonce: [u8; NONCE_SIZE],
    /// Ciphertext with 16-byte authentication tag appended
    pub ciphertext: Vec<u8>,
}

impl EncryptedPayload {
    /// Serialize to bytes: [nonce || ciphertext]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(NONCE_SIZE + self.ciphertext.len());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&self.ciphertext);
        bytes
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() < NONCE_SIZE + TAG_SIZE {
            return Err(CryptoError::InvalidCiphertext);
        }
        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&bytes[..NONCE_SIZE]);
        let ciphertext = bytes[NONCE_SIZE..].to_vec();
        Ok(Self { nonce, ciphertext })
    }
}

/// Symmetric encryption engine using AES-256-GCM
#[derive(Clone)]
pub struct SymmetricCipher {
    key: [u8; KEY_SIZE],
}

impl std::fmt::Debug for SymmetricCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymmetricCipher")
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl SymmetricCipher {
    /// Create cipher from derived key material
    pub fn from_shared_secret(shared_secret: &[u8; 32]) -> Self {
        // Derive AES-256 key using SHA-256
        let hash = Sha256::digest(shared_secret);
        let mut key = [0u8; KEY_SIZE];
        key.copy_from_slice(&hash[..KEY_SIZE]);
        Self { key }
    }

    /// Encrypt plaintext
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedPayload, CryptoError> {
        let key = Key::<Aes256Gcm>::from_slice(&self.key);
        let cipher = Aes256Gcm::new(key);

        // Generate random nonce
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
        let nonce = AesNonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| CryptoError::Encryption(e.to_string()))?;

        Ok(EncryptedPayload {
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    /// Decrypt ciphertext
    pub fn decrypt(&self, payload: &EncryptedPayload) -> Result<Vec<u8>, CryptoError> {
        let key = Key::<Aes256Gcm>::from_slice(&self.key);
        let cipher = Aes256Gcm::new(key);
        let nonce = AesNonce::from_slice(&payload.nonce);

        cipher
            .decrypt(nonce, payload.ciphertext.as_ref())
            .map_err(|e| CryptoError::Decryption(e.to_string()))
    }
}

/// Noise protocol-like handshake state machine
#[derive(Debug)]
pub struct NoiseHandshake {
    static_secret: X25519Secret,
    ephemeral_secret: Option<X25519Secret>,
    state: HandshakeState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandshakeState {
    Init,
    EphemeralSent,
    Completed,
}

impl NoiseHandshake {
    /// Initialize handshake with our static key
    pub fn new(static_secret: X25519Secret) -> Self {
        Self {
            static_secret,
            ephemeral_secret: None,
            state: HandshakeState::Init,
        }
    }

    /// Get our static public key
    pub fn static_public(&self) -> X25519PublicKey {
        self.static_secret.public_key()
    }

    /// Generate ephemeral key and return public key for transmission
    pub fn send_ephemeral(&mut self) -> X25519PublicKey {
        let ephemeral = X25519Secret::generate();
        let public = ephemeral.public_key();
        self.ephemeral_secret = Some(ephemeral);
        self.state = HandshakeState::EphemeralSent;
        public
    }

    /// Complete handshake with peer's ephemeral and static public keys
    /// Returns the derived shared secret for symmetric encryption
    pub fn complete_handshake(
        &mut self,
        peer_ephemeral: &X25519PublicKey,
        peer_static: &X25519PublicKey,
    ) -> Result<[u8; 32], CryptoError> {
        let ephemeral = self
            .ephemeral_secret
            .as_ref()
            .ok_or(CryptoError::KeyExchange(
                "ephemeral key not generated".to_string(),
            ))?;

        // Derive keys through multiple DH exchanges (Noise pattern)
        // Shared secret = HMAC( DH(ephemeral, peer_ephemeral) || DH(static, peer_static) )
        let dh1 = ephemeral.diffie_hellman(peer_ephemeral);
        let dh2 = self.static_secret.diffie_hellman(peer_static);

        // Combine and hash
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(&dh1);
        combined.extend_from_slice(&dh2);

        let hash = Sha256::digest(&combined);
        let mut shared_secret = [0u8; 32];
        shared_secret.copy_from_slice(&hash);

        self.state = HandshakeState::Completed;
        Ok(shared_secret)
    }

    /// Check if handshake is completed
    pub fn is_completed(&self) -> bool {
        self.state == HandshakeState::Completed
    }
}

/// Peer identity with public keys
#[derive(Debug, Clone)]
pub struct PeerIdentity {
    pub public_key: X25519PublicKey,
    pub name: String,
}

/// Compute a keyed integrity digest for sync payloads.
pub fn payload_integrity_digest(payload: &[u8], shared_secret: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(shared_secret);
    hasher.update(payload);
    hasher.finalize().into()
}

#[deprecated(since = "0.3.0", note = "use payload_integrity_digest")]
pub fn verify_payload_integrity(payload: &[u8], shared_secret: &[u8; 32]) -> [u8; 32] {
    payload_integrity_digest(payload, shared_secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_encryption_roundtrip() {
        let key: [u8; 32] = rand::random();
        let cipher = SymmetricCipher::from_shared_secret(&key);
        let plaintext = b"Hello, distributed memory!";

        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypted_payload_serialization() {
        let payload = EncryptedPayload {
            nonce: [1u8; NONCE_SIZE],
            ciphertext: vec![2u8; 64],
        };

        let bytes = payload.to_bytes();
        let restored = EncryptedPayload::from_bytes(&bytes).unwrap();

        assert_eq!(payload, restored);
    }

    #[test]
    fn x25519_key_exchange() {
        let alice = X25519Secret::generate();
        let bob = X25519Secret::generate();

        let alice_shared = alice.diffie_hellman(&bob.public_key());
        let bob_shared = bob.diffie_hellman(&alice.public_key());

        assert_eq!(alice_shared, bob_shared);
        assert_ne!(alice_shared, [0u8; 32]);
    }

    #[test]
    fn noise_handshake_completes() {
        let alice_static = X25519Secret::generate();
        let bob_static = X25519Secret::generate();

        let mut alice_handshake = NoiseHandshake::new(alice_static);
        let mut bob_handshake = NoiseHandshake::new(bob_static);

        // Alice generates ephemeral key
        let alice_ephemeral_pub = alice_handshake.send_ephemeral();

        // Bob generates ephemeral key
        let bob_ephemeral_pub = bob_handshake.send_ephemeral();

        let alice_secret = alice_handshake
            .complete_handshake(&bob_ephemeral_pub, &bob_handshake.static_public())
            .unwrap();
        let bob_secret = bob_handshake
            .complete_handshake(&alice_ephemeral_pub, &alice_handshake.static_public())
            .unwrap();

        assert!(alice_handshake.is_completed());
        assert!(bob_handshake.is_completed());
        assert_eq!(alice_secret, bob_secret);
    }

    #[test]
    fn different_plaintexts_produce_different_ciphertexts() {
        let key: [u8; 32] = rand::random();
        let cipher = SymmetricCipher::from_shared_secret(&key);

        let encrypted1 = cipher.encrypt(b"message one").unwrap();
        let encrypted2 = cipher.encrypt(b"message two").unwrap();

        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
    }

    #[test]
    fn tampered_ciphertext_fails_decryption() {
        let key: [u8; 32] = rand::random();
        let cipher = SymmetricCipher::from_shared_secret(&key);
        let plaintext = b"sensitive data";

        let mut encrypted = cipher.encrypt(plaintext).unwrap();
        encrypted.ciphertext[0] ^= 0xFF; // Tamper with first byte

        let result = cipher.decrypt(&encrypted);
        assert!(result.is_err());
    }
}
