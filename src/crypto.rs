// src/crypto.rs

use ed25519_dalek::{Signer, Verifier, SigningKey, VerifyingKey, Signature};
use sha3::{Digest, Sha3_256};
use std::path::Path;
use std::fs;
use rand::{rngs::OsRng, RngCore};
use anyhow::{Result, Context};
use thiserror::Error;

/// Cryptographic errors
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid keypair format: {0}")]
    InvalidKeypairFormat(String),
    #[error("Key file error: {0}")]
    KeyFileError(String),
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}

/// Represents the engine's unique identity with enhanced functionality
#[derive(Clone)]
pub struct NodeKeypair(SigningKey);

impl NodeKeypair {
    /// Get the signing key (private key)
    pub fn signing_key(&self) -> &SigningKey {
        &self.0
    }

    /// Get the verifying key (public key)
    pub fn verifying_key(&self) -> VerifyingKey {
        self.0.verifying_key()
    }

    /// Get the public key for this node (alias for verifying_key)
    pub fn public_key(&self) -> VerifyingKey {
        self.0.verifying_key()
    }

    /// Get the node ID as a hex string (derived from public key)
    pub fn node_id(&self) -> String {
        hex::encode(self.public_key().to_bytes())
    }

    /// Sign data with this node's private key
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        self.0.sign(data).to_bytes().to_vec()
    }

    /// Sign data and return a Signature object
    pub fn sign_message(&self, data: &[u8]) -> Signature {
        self.0.sign(data)
    }

    /// Verify a signature against data using this node's public key
    pub fn verify(&self, data: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
        let signature = Signature::from_bytes(signature.try_into()
            .map_err(|_| CryptoError::InvalidSignature)?);

        self.verifying_key()
            .verify(data, &signature)
            .map_err(|_| CryptoError::SignatureVerificationFailed)
    }

    /// Create a new keypair from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeypairFormat(
                format!("Expected 32 bytes, got {}", bytes.len())
            ));
        }

        let signing_key = SigningKey::from_bytes(bytes.try_into()
            .map_err(|_| CryptoError::InvalidKeypairFormat("Invalid key format".to_string()))?);

        Ok(NodeKeypair(signing_key))
    }

    /// Export the private key as bytes (use with caution)
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

/// Signs a block of data with the node's private key.
pub fn sign_block(block: &super::validator::BlockToValidate) -> Vec<u8> {
    // Create a hash of the block data for signing
    let mut hasher = Sha3_256::new();
    hasher.update(&block.data);
    hasher.update(block.id.as_bytes());
    let hash = hasher.finalize();

    // For now, return a placeholder signature
    // In a real implementation, this would use the actual node keypair
    hash.to_vec()
}

/// Loads a keypair from a file or creates a new one if it doesn't exist.
/// Enhanced with better error handling and security considerations.
pub fn load_or_create_keypair(path: &Path) -> Result<NodeKeypair> {
    if path.exists() {
        load_keypair_from_file(path)
    } else {
        create_and_save_keypair(path)
    }
}

/// Load an existing keypair from file
fn load_keypair_from_file(path: &Path) -> Result<NodeKeypair> {
    tracing::info!("Loading existing keypair from: {:?}", path);

    let key_data = fs::read(path)
        .with_context(|| format!("Failed to read keypair file: {:?}", path))?;

    // Validate file format - we store 64 bytes (32 private + 32 public)
    if key_data.len() != 64 {
        return Err(CryptoError::InvalidKeypairFormat(
            format!("Expected 64 bytes, got {}", key_data.len())
        ).into());
    }

    // Extract the private key (first 32 bytes)
    let private_key_bytes: [u8; 32] = key_data[..32].try_into()
        .map_err(|_| CryptoError::InvalidKeypairFormat("Invalid private key length".to_string()))?;

    let signing_key = SigningKey::from_bytes(&private_key_bytes);
    let keypair = NodeKeypair(signing_key);

    // Verify the stored public key matches the derived public key
    let stored_public_key = &key_data[32..64];
    let derived_public_key = keypair.verifying_key().to_bytes();

    if stored_public_key != derived_public_key {
        return Err(CryptoError::InvalidKeypairFormat(
            "Stored public key doesn't match derived public key".to_string()
        ).into());
    }

    tracing::info!("Keypair loaded successfully with node ID: {}", keypair.node_id());
    Ok(keypair)
}

/// Create a new keypair and save it to file
fn create_and_save_keypair(path: &Path) -> Result<NodeKeypair> {
    tracing::info!("Generating new keypair and saving to: {:?}", path);

    // Generate cryptographically secure random bytes
    let mut csprng = OsRng;
    let mut secret_bytes = [0u8; 32];
    csprng.fill_bytes(&mut secret_bytes);

    let signing_key = SigningKey::from_bytes(&secret_bytes);
    let keypair = NodeKeypair(signing_key);

    // Prepare key data for storage (private key + public key)
    let key_bytes = [
        keypair.signing_key().to_bytes().as_slice(),
        keypair.verifying_key().to_bytes().as_slice()
    ].concat();

    // Create parent directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {:?}", parent))?;
    }

    // Write the keypair to file
    fs::write(path, &key_bytes)
        .with_context(|| format!("Failed to write keypair to: {:?}", path))?;

    // Set secure file permissions (Unix-like systems only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)
            .with_context(|| format!("Failed to get file metadata: {:?}", path))?
            .permissions();
        perms.set_mode(0o600); // Read/write for owner only
        fs::set_permissions(path, perms)
            .with_context(|| format!("Failed to set file permissions: {:?}", path))?;
        tracing::info!("Set secure file permissions (0o600) for keypair file");
    }

    #[cfg(windows)]
    {
        tracing::warn!("File permissions not set on Windows - consider using encrypted storage");
    }

    tracing::info!("New keypair generated with node ID: {}", keypair.node_id());
    Ok(keypair)
}

/// Hash data using SHA-3
pub fn hash_data(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Verify a signature against data using a public key
pub fn verify_signature(public_key: &VerifyingKey, data: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
    let signature = Signature::from_bytes(signature.try_into()
        .map_err(|_| CryptoError::InvalidSignature)?);

    public_key
        .verify(data, &signature)
        .map_err(|_| CryptoError::SignatureVerificationFailed)
}

/// Create a message hash for signing (includes timestamp for replay protection)
pub fn create_message_hash(data: &[u8], timestamp: u64) -> Vec<u8> {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.update(&timestamp.to_le_bytes());
    hasher.finalize().to_vec()
}

/// Verify a timestamped message signature with replay protection
pub fn verify_timestamped_signature(
    public_key: &VerifyingKey,
    data: &[u8],
    timestamp: u64,
    signature: &[u8],
    max_age_secs: u64,
) -> Result<(), CryptoError> {
    // Check timestamp is not too old
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    if now.saturating_sub(timestamp) > max_age_secs {
        return Err(CryptoError::SignatureVerificationFailed);
    }

    // Create the message hash with timestamp
    let message_hash = create_message_hash(data, timestamp);

    // Verify the signature
    verify_signature(public_key, &message_hash, signature)
}

/// Generate a random nonce for message authentication
pub fn generate_nonce() -> [u8; 32] {
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

/// Derive a public key from a node ID string
pub fn public_key_from_node_id(node_id: &str) -> Result<VerifyingKey, CryptoError> {
    let bytes = hex::decode(node_id)
        .map_err(|_| CryptoError::InvalidKeypairFormat("Invalid hex in node ID".to_string()))?;

    if bytes.len() != 32 {
        return Err(CryptoError::InvalidKeypairFormat(
            format!("Node ID must be 32 bytes, got {}", bytes.len())
        ));
    }

    let key_bytes: [u8; 32] = bytes.try_into()
        .map_err(|_| CryptoError::InvalidKeypairFormat("Invalid key bytes".to_string()))?;

    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| CryptoError::InvalidKeypairFormat("Invalid public key".to_string()))
}

