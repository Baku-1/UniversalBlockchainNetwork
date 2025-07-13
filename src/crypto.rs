// src/crypto.rs

use ed25519_dalek::{Signer, SigningKey, VerifyingKey, Signature, SecretKey};
use sha3::{Digest, Sha3_256};
use std::path::Path;
use std::fs;
use rand::{rngs::OsRng, RngCore};
// Serde traits removed as they're not used in this module

// Represents the engine's unique identity.
#[derive(Clone)]
pub struct NodeKeypair(SigningKey);

impl NodeKeypair {
    pub fn signing_key(&self) -> &SigningKey {
        &self.0
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.0.verifying_key()
    }
}

impl NodeKeypair {
    /// Get the public key for this node
    pub fn public_key(&self) -> VerifyingKey {
        self.0.verifying_key()
    }

    /// Get the node ID as a hex string
    pub fn node_id(&self) -> String {
        hex::encode(self.public_key().to_bytes())
    }

    /// Sign data with this node's private key
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        self.0.sign(data).to_bytes().to_vec()
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
pub fn load_or_create_keypair(path: &Path) -> Result<NodeKeypair, Box<dyn std::error::Error>> {
    if path.exists() {
        // Load existing keypair
        tracing::info!("Loading existing keypair from: {:?}", path);
        let key_data = fs::read(path)?;

        // For now, we'll store the raw keypair bytes (in production, this should be encrypted)
        if key_data.len() != 64 {
            return Err("Invalid keypair file format".into());
        }

        let signing_key = SigningKey::from_bytes(
            key_data[..32].try_into()
                .map_err(|_| "Invalid signing key length")?
        );
        Ok(NodeKeypair(signing_key))
    } else {
        // Generate a new cryptographic keypair
        tracing::info!("Generating new keypair and saving to: {:?}", path);
        let mut csprng = OsRng;
        let mut secret_bytes = [0u8; 32];
        csprng.fill_bytes(&mut secret_bytes);
        let secret_key: SecretKey = secret_bytes;
        let signing_key = SigningKey::from(&secret_key);
        let keypair = NodeKeypair(signing_key);

        // Save the keypair to file (in production, this should be encrypted)
        let key_bytes = [
            keypair.signing_key().to_bytes().as_slice(),
            keypair.verifying_key().to_bytes().as_slice()
        ].concat();

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, key_bytes)?;

        // Set file permissions to be readable only by the current user (Unix-like systems)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o600); // Read/write for owner only
            fs::set_permissions(path, perms)?;
        }

        tracing::info!("New keypair generated with node ID: {}",
                      hex::encode(keypair.verifying_key().to_bytes()));

        Ok(keypair)
    }
}

/// Hash data using SHA-3
pub fn hash_data(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha3_256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// Verify a signature against data and public key
pub fn verify_signature(data: &[u8], signature: &[u8], public_key: &VerifyingKey) -> bool {
    use ed25519_dalek::Verifier;

    if signature.len() == 64 {
        if let Ok(sig_array) = signature.try_into() {
            let sig = Signature::from_bytes(&sig_array);
            return public_key.verify(data, &sig).is_ok();
        }
    }
    false
}