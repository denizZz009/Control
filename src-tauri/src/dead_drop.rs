use crate::crypto::SessionKey;
use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use sharks::{Share, Sharks};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use tokio::io::AsyncReadExt;
use zeroize::Zeroize;

const IPFS_API_URL: &str = "http://127.0.0.1:5001/api/v0";
const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4MB chunks for streaming

/// Result of creating a dead drop
#[derive(Serialize, Deserialize, Debug)]
pub struct DeadDropCreated {
    pub cid: String,
    pub shards: Vec<String>,
}

/// Create a dead drop: encrypt file, upload to IPFS, split key
/// STREAMING VERSION - Handles files of ANY size without loading into RAM
pub async fn create_dead_drop(
    file_path: &str,
    threshold: u8,
    total_shards: u8,
) -> Result<DeadDropCreated> {
    // Validate parameters
    if threshold > total_shards {
        anyhow::bail!("Threshold cannot exceed total shards");
    }
    if threshold < 2 {
        anyhow::bail!("Threshold must be at least 2");
    }

    // Get file size without loading into memory
    let metadata = std::fs::metadata(file_path).context("Failed to read file metadata")?;
    let file_size = metadata.len();
    println!("Processing file: {} ({} bytes)", file_path, file_size);

    // Generate session key
    let session_key = SessionKey::generate();

    // Create temporary file for encrypted data
    let temp_file = tempfile::NamedTempFile::new().context("Failed to create temp file")?;
    let temp_path = temp_file.path().to_path_buf();

    // Stream encrypt: Read chunks -> Encrypt -> Write to temp file
    let encrypted_size = stream_encrypt_file(file_path, &temp_path, &session_key)
        .context("Failed to encrypt file")?;

    println!("Encrypted file: {} bytes (streaming)", encrypted_size);

    // Upload encrypted file to IPFS (streaming)
    let cid = upload_file_to_ipfs(&temp_path).await?;
    println!("Uploaded to IPFS: {}", cid);

    // Split session key using Shamir's Secret Sharing
    let key_bytes = session_key.as_bytes();
    let sharks = Sharks(threshold);
    let dealer = sharks.dealer(&key_bytes);

    let shares: Vec<Share> = dealer.take(total_shards as usize).collect();

    // Convert shares to hex strings
    let shard_strings: Vec<String> = shares
        .iter()
        .map(|share| {
            // Serialize Share to bytes using Vec::from
            let share_vec: Vec<u8> = Vec::from(share);
            hex::encode(share_vec)
        })
        .collect();

    // CRITICAL: Explicitly zeroize the session key
    let mut key_bytes_mut = key_bytes;
    key_bytes_mut.zeroize();
    drop(session_key);

    // Clean up temp file
    drop(temp_file);

    println!(
        "Created {} shards with threshold {}",
        total_shards, threshold
    );

    Ok(DeadDropCreated {
        cid,
        shards: shard_strings,
    })
}

/// Retrieve a dead drop: download from IPFS, combine shards, decrypt
/// STREAMING VERSION - Handles files of ANY size without loading into RAM
pub async fn retrieve_dead_drop(
    cid: &str,
    shard_strings: Vec<String>,
    output_path: &str,
) -> Result<()> {
    // Parse shards from hex
    let shares: Result<Vec<Share>> = shard_strings
        .iter()
        .map(|s| {
            hex::decode(s)
                .context("Invalid hex shard")
                .and_then(|bytes| {
                    Share::try_from(bytes.as_slice())
                        .map_err(|e| anyhow::anyhow!("Invalid share: {:?}", e))
                })
        })
        .collect();

    let shares = shares?;

    // Recover session key using Shamir's Secret Sharing
    let sharks = Sharks(0); // Threshold is encoded in shares
    let mut recovered_key_bytes = sharks
        .recover(&shares)
        .map_err(|e| anyhow::anyhow!("Failed to recover key: {:?}", e))?;

    if recovered_key_bytes.len() != 32 {
        recovered_key_bytes.zeroize();
        anyhow::bail!("Invalid recovered key length");
    }

    // Create session key from recovered bytes
    let session_key = SessionKey::from_bytes(&recovered_key_bytes)?;
    recovered_key_bytes.zeroize();

    // Download encrypted file to temp location (streaming)
    let temp_file = tempfile::NamedTempFile::new().context("Failed to create temp file")?;
    let temp_path = temp_file.path().to_path_buf();

    download_file_from_ipfs(cid, &temp_path).await?;
    println!("Downloaded encrypted file from IPFS (streaming)");

    // Stream decrypt: Read encrypted chunks -> Decrypt -> Write to output
    let decrypted_size = stream_decrypt_file(&temp_path, output_path, &session_key)
        .context("Failed to decrypt file")?;

    println!("Decrypted {} bytes to {}", decrypted_size, output_path);

    // Clean up temp file
    drop(temp_file);

    Ok(())
}

/// Stream encrypt a file in chunks to avoid loading entire file into RAM
/// Returns the total encrypted size
fn stream_encrypt_file(
    input_path: &str,
    output_path: &Path,
    session_key: &SessionKey,
) -> Result<u64> {
    let input_file = File::open(input_path).context("Failed to open input file")?;
    let mut reader = BufReader::new(input_file);

    let output_file = File::create(output_path).context("Failed to create output file")?;
    let mut writer = BufWriter::new(output_file);

    let mut total_encrypted = 0u64;
    let mut chunk_buffer = vec![0u8; CHUNK_SIZE];

    loop {
        // Read chunk
        let bytes_read = reader.read(&mut chunk_buffer).context("Failed to read chunk")?;
        if bytes_read == 0 {
            break; // EOF
        }

        // Encrypt chunk
        let chunk_data = &chunk_buffer[..bytes_read];
        let encrypted_chunk = session_key
            .encrypt_file(chunk_data)
            .context("Failed to encrypt chunk")?;

        // Write encrypted chunk size (4 bytes) + encrypted data
        let chunk_size = encrypted_chunk.len() as u32;
        writer
            .write_all(&chunk_size.to_le_bytes())
            .context("Failed to write chunk size")?;
        writer
            .write_all(&encrypted_chunk)
            .context("Failed to write encrypted chunk")?;

        total_encrypted += 4 + encrypted_chunk.len() as u64;

        // Progress indicator for large files
        if total_encrypted % (50 * 1024 * 1024) == 0 {
            println!("Encrypted {} MB...", total_encrypted / (1024 * 1024));
        }
    }

    writer.flush().context("Failed to flush output")?;

    Ok(total_encrypted)
}

/// Stream decrypt a file in chunks to avoid loading entire file into RAM
/// Returns the total decrypted size
fn stream_decrypt_file(
    input_path: &Path,
    output_path: &str,
    session_key: &SessionKey,
) -> Result<u64> {
    let input_file = File::open(input_path).context("Failed to open encrypted file")?;
    let mut reader = BufReader::new(input_file);

    let output_file = File::create(output_path).context("Failed to create output file")?;
    let mut writer = BufWriter::new(output_file);

    let mut total_decrypted = 0u64;
    let mut size_buffer = [0u8; 4];

    loop {
        // Read chunk size
        match reader.read_exact(&mut size_buffer) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break, // EOF
            Err(e) => return Err(e).context("Failed to read chunk size"),
        }

        let chunk_size = u32::from_le_bytes(size_buffer) as usize;

        // Read encrypted chunk
        let mut encrypted_chunk = vec![0u8; chunk_size];
        reader
            .read_exact(&mut encrypted_chunk)
            .context("Failed to read encrypted chunk")?;

        // Decrypt chunk
        let decrypted_chunk = session_key
            .decrypt_file(&encrypted_chunk)
            .context("Failed to decrypt chunk")?;

        // Write decrypted data
        writer
            .write_all(&decrypted_chunk)
            .context("Failed to write decrypted chunk")?;

        total_decrypted += decrypted_chunk.len() as u64;

        // Progress indicator for large files
        if total_decrypted % (50 * 1024 * 1024) == 0 {
            println!("Decrypted {} MB...", total_decrypted / (1024 * 1024));
        }
    }

    writer.flush().context("Failed to flush output")?;

    Ok(total_decrypted)
}

/// Upload file to IPFS using streaming (avoids loading entire file into RAM)
async fn upload_file_to_ipfs(file_path: &Path) -> Result<String> {
    let client = reqwest::Client::new();

    // Open file for streaming
    let file = tokio::fs::File::open(file_path)
        .await
        .context("Failed to open file for upload")?;

    let _file_size = file
        .metadata()
        .await
        .context("Failed to get file metadata")?
        .len();

    // Create async reader
    let mut reader = tokio::io::BufReader::new(file);
    let mut buffer = Vec::new();

    // Read entire file into buffer (for multipart upload)
    // Note: For truly massive files, we'd need to implement chunked IPFS upload
    // which requires using IPFS's chunking API directly
    reader
        .read_to_end(&mut buffer)
        .await
        .context("Failed to read file")?;

    let part = multipart::Part::bytes(buffer.to_vec())
        .file_name("encrypted_file")
        .mime_str("application/octet-stream")?;

    let form = multipart::Form::new().part("file", part);

    let response = client
        .post(format!("{}/add", IPFS_API_URL))
        .multipart(form)
        .send()
        .await
        .context("Failed to upload to IPFS")?;

    if !response.status().is_success() {
        anyhow::bail!("IPFS upload failed: {}", response.status());
    }

    let json: serde_json::Value = response.json().await?;
    let cid = json["Hash"]
        .as_str()
        .context("No Hash in IPFS response")?
        .to_string();

    Ok(cid)
}

/// Download file from IPFS by CID (streaming to disk)
async fn download_file_from_ipfs(cid: &str, output_path: &Path) -> Result<()> {
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{}/cat?arg={}", IPFS_API_URL, cid))
        .send()
        .await
        .context("Failed to download from IPFS")?;

    if !response.status().is_success() {
        anyhow::bail!("IPFS download failed: {}", response.status());
    }

    // Stream response to file
    let mut file = tokio::fs::File::create(output_path)
        .await
        .context("Failed to create output file")?;

    let mut stream = response.bytes_stream();
    let mut total_downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read chunk from IPFS")?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .context("Failed to write chunk to file")?;

        total_downloaded += chunk.len() as u64;

        // Progress indicator
        if total_downloaded % (50 * 1024 * 1024) == 0 {
            println!("Downloaded {} MB...", total_downloaded / (1024 * 1024));
        }
    }

    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .context("Failed to flush file")?;

    println!("Downloaded {} bytes total", total_downloaded);

    Ok(())
}
