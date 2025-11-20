use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{Context, Result};
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2,
};
use chacha20poly1305::{ChaCha20Poly1305, Key as ChaChaKey};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, ZeroizeOnDrop};

const IDENTITY_FILE: &str = "identity.enc";
const NONCE_SIZE: usize = 12;

/// Core identity structure with X25519 keypair
#[derive(Clone)]
pub struct Identity {
    pub public_key: PublicKey,
    private_key: StaticSecret,
}

impl Identity {
    /// Generate a new random identity
    pub fn generate() -> Self {
        let private_key = StaticSecret::random_from_rng(OsRng);
        let public_key = PublicKey::from(&private_key);
        Self {
            public_key,
            private_key,
        }
    }

    /// Get public key as base58 string for P2P identification
    pub fn public_id(&self) -> String {
        bs58::encode(self.public_key.as_bytes()).into_string()
    }

    /// Perform ECDH key exchange
    pub fn shared_secret(&self, peer_public: &PublicKey) -> [u8; 32] {
        self.private_key.diffie_hellman(peer_public).to_bytes()
    }

    /// Load or generate identity from encrypted storage
    pub fn load_or_generate(password: &str, data_dir: PathBuf) -> Result<Self> {
        let identity_path = data_dir.join(IDENTITY_FILE);

        if identity_path.exists() {
            Self::load_from_disk(password, &identity_path)
        } else {
            let identity = Self::generate();
            identity.save_to_disk(password, &identity_path)?;
            Ok(identity)
        }
    }

    /// Save encrypted identity to disk using Argon2 + AES-GCM
    fn save_to_disk(&self, password: &str, path: &PathBuf) -> Result<()> {
        println!("Generating encryption key (this may take a moment)...");
        
        // Derive key from password using Argon2
        let salt = SaltString::generate(&mut OsRng);
        
        // Argon2 parameters: 16 MB memory, 3 iterations, 1 thread
        // This provides good security while remaining reasonably fast
        use argon2::{Algorithm, Params, Version};
        let params = Params::new(
            16384, // 16 MB memory (good balance of security and speed)
            3,     // 3 iterations (standard)
            1,     // 1 thread (single-threaded for consistency)
            None,
        ).map_err(|e| anyhow::anyhow!("Failed to create Argon2 params: {:?}", e))?;
        
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let password_hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash password: {:?}", e))?;
        
        println!("Key generated successfully");

        // Extract 32-byte key from hash
        let key_material = password_hash.hash.context("No hash generated")?;
        let key = &key_material.as_bytes()[..32];

        // Encrypt private key
        let cipher = Aes256Gcm::new_from_slice(key).context("Invalid key length")?;
        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let private_bytes = self.private_key.to_bytes();
        let ciphertext = cipher
            .encrypt(nonce, private_bytes.as_ref())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Store: salt || nonce || ciphertext
        let stored_data = StoredIdentity {
            salt: salt.to_string(),
            nonce: nonce_bytes.to_vec(),
            ciphertext,
        };

        let json = serde_json::to_string(&stored_data)?;
        fs::write(path, json)?;

        Ok(())
    }

    /// Load encrypted identity from disk
    fn load_from_disk(password: &str, path: &PathBuf) -> Result<Self> {
        println!("Loading identity from disk...");
        let json = fs::read_to_string(path)?;
        let stored: StoredIdentity = serde_json::from_str(&json)?;

        // Parse salt directly from stored string (it's already in the right format)
        let salt = SaltString::from_b64(&stored.salt)
            .map_err(|e| anyhow::anyhow!("Failed to parse salt: {:?}", e))?;
        
        // Use same params as save_to_disk: 16 MB memory, 3 iterations, 1 thread
        use argon2::{Algorithm, Params, Version};
        let params = Params::new(16384, 3, 1, None)
            .map_err(|e| anyhow::anyhow!("Failed to create Argon2 params: {:?}", e))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let password_hash = argon2.hash_password(password.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash password: {:?}", e))?;
        
        println!("Identity loaded successfully");

        let key_material = password_hash.hash.context("No hash generated")?;
        let key = &key_material.as_bytes()[..32];

        // Decrypt private key
        let cipher = Aes256Gcm::new_from_slice(key)?;
        let nonce = Nonce::from_slice(&stored.nonce);

        let mut plaintext = cipher
            .decrypt(nonce, stored.ciphertext.as_ref())
            .map_err(|_| anyhow::anyhow!("Decryption failed - wrong password?"))?;

        if plaintext.len() != 32 {
            plaintext.zeroize();
            anyhow::bail!("Invalid private key length");
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&plaintext);
        plaintext.zeroize();

        let private_key = StaticSecret::from(key_bytes);
        key_bytes.zeroize();

        let public_key = PublicKey::from(&private_key);

        Ok(Self {
            public_key,
            private_key,
        })
    }
}

impl Drop for Identity {
    fn drop(&mut self) {
        // Zeroize private key on drop
        let mut bytes = self.private_key.to_bytes();
        bytes.zeroize();
    }
}

#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    salt: String,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

/// Session key for file encryption - auto-zeroized on drop
#[derive(Clone, ZeroizeOnDrop)]
pub struct SessionKey {
    #[zeroize(skip)]
    key: ChaChaKey,
}

impl SessionKey {
    /// Generate a random session key
    pub fn generate() -> Self {
        let mut key_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut key_bytes);
        let key = ChaChaKey::from(key_bytes);
        key_bytes.zeroize();
        Self { key }
    }

    /// Create from raw bytes (for Shamir reconstruction)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 32 {
            anyhow::bail!("Invalid key length: expected 32 bytes");
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(bytes);
        let key = ChaChaKey::from(key_bytes);
        key_bytes.zeroize();
        Ok(Self { key })
    }

    /// Get key bytes (use carefully - caller must zeroize)
    pub fn as_bytes(&self) -> [u8; 32] {
        self.key.into()
    }

    /// Encrypt file data using ChaCha20-Poly1305
    pub fn encrypt_file(&self, data: &[u8]) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new(&self.key);

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = chacha20poly1305::Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Return: nonce || ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt file data using ChaCha20-Poly1305
    pub fn decrypt_file(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < NONCE_SIZE {
            anyhow::bail!("Invalid encrypted data: too short");
        }

        let (nonce_bytes, ciphertext) = data.split_at(NONCE_SIZE);
        let nonce = chacha20poly1305::Nonce::from_slice(nonce_bytes);

        let cipher = ChaCha20Poly1305::new(&self.key);
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        Ok(plaintext)
    }
}

/// Encrypt message for P2P using shared secret
pub fn encrypt_message(shared_secret: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    // Derive encryption key from shared secret
    let mut hasher = Sha256::new();
    hasher.update(b"deaddrop-message-key");
    hasher.update(shared_secret);
    let key_bytes = hasher.finalize();

    let key = ChaChaKey::from_slice(&key_bytes);
    let cipher = ChaCha20Poly1305::new(key);

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = chacha20poly1305::Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("Message encryption failed: {}", e))?;

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&ciphertext);

    Ok(result)
}

/// Decrypt message from P2P using shared secret
pub fn decrypt_message(shared_secret: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_SIZE {
        anyhow::bail!("Invalid encrypted message: too short");
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_SIZE);
    let nonce = chacha20poly1305::Nonce::from_slice(nonce_bytes);

    // Derive encryption key from shared secret
    let mut hasher = Sha256::new();
    hasher.update(b"deaddrop-message-key");
    hasher.update(shared_secret);
    let key_bytes = hasher.finalize();

    let key = ChaChaKey::from_slice(&key_bytes);
    let cipher = ChaCha20Poly1305::new(key);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("Message decryption failed: {}", e))?;

    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_key_encryption() {
        let key = SessionKey::generate();
        let data = b"Secret military intel";

        let encrypted = key.encrypt_file(data).unwrap();
        let decrypted = key.decrypt_file(&encrypted).unwrap();

        assert_eq!(data.as_ref(), decrypted.as_slice());
    }

    #[test]
    fn test_identity_key_exchange() {
        let alice = Identity::generate();
        let bob = Identity::generate();

        let alice_shared = alice.shared_secret(&bob.public_key);
        let bob_shared = bob.shared_secret(&alice.public_key);

        assert_eq!(alice_shared, bob_shared);
    }
}
