# Control
<img width="1080" height="1080" alt="image" src="https://github.com/user-attachments/assets/9e71b464-b904-4538-bf0c-10628372bb1b" />

A secure, decentralized communication platform built with Rust and Tauri, featuring end-to-end encrypted messaging and offline file exchange capabilities.

## Overview

Control provides two primary communication modes:

1. **Ghost Mode**: Real-time peer-to-peer encrypted messaging over local networks
2. **Dead Drop**: Offline file exchange using encryption and secret sharing

## Architecture

### Technology Stack

**Backend:**
- Rust (cryptographic operations, P2P networking)
- Tauri 1.6 (desktop application framework)
- libp2p 0.52 (peer-to-peer networking)
- IPFS (distributed file storage)

**Frontend:**
- React 18 + TypeScript
- Vite (build tool)

**Cryptography:**
- X25519 (key exchange)
- ChaCha20-Poly1305 (symmetric encryption)
- Argon2id (password hashing)
- AES-256-GCM (identity storage)
- Shamir's Secret Sharing (key splitting)

### System Components

```
┌─────────────────────────────────────────────────────────┐
│                    Frontend (React)                      │
│                  User Interface Layer                    │
└────────────────────┬────────────────────────────────────┘
                     │ Tauri IPC
┌────────────────────┴────────────────────────────────────┐
│                  Backend (Rust)                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   crypto.rs  │  │    p2p.rs    │  │ dead_drop.rs │  │
│  │   Identity   │  │  libp2p      │  │  IPFS        │  │
│  │   Encryption │  │  GossipSub   │  │  Shamir      │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Features

### 1. Identity Management

Each user has a cryptographic identity consisting of:
- X25519 keypair (public/private keys)
- Base58-encoded public identifier
- Encrypted storage on disk

**Key Generation:**
```
Password → Argon2id (16MB, 3 iterations) → AES-256-GCM → Encrypted Identity
```

**Security Properties:**
- Private keys never leave the device
- Keys are zeroized from memory after use
- Password-protected with strong KDF

### 2. Ghost Mode (P2P Messaging)

Real-time encrypted messaging between peers on the same network.

**Network Architecture:**
```
Peer A                    Network                    Peer B
  │                          │                          │
  │ 1. ECDH Key Exchange     │                          │
  │──────────────────────────┼─────────────────────────►│
  │                          │                          │
  │ 2. Encrypted Message     │                          │
  │──────────────────────────┼─────────────────────────►│
  │                          │                          │
  │ 3. Delivery Receipt      │                          │
  │◄─────────────────────────┼──────────────────────────│
```

**Message Flow:**
1. Sender performs ECDH with recipient's public key
2. Derives shared secret
3. Encrypts message with ChaCha20-Poly1305
4. Publishes to recipient's GossipSub topic
5. Recipient decrypts and sends acknowledgment

**Features:**
- End-to-end encryption
- Message delivery confirmation (ACK)
- Automatic peer discovery (mDNS)
- NAT traversal support (Circuit Relay v2)

### 3. Dead Drop (Offline File Exchange)

Secure file sharing using encryption and threshold secret sharing.

**Process Flow:**
```
File → Encrypt → Upload to IPFS → Split Key → Distribute Shards
                                      ↓
                              Shamir's Secret Sharing
                                      ↓
                          Threshold: 3-of-5 shards required
```

**Encryption:**
1. Generate random session key (256-bit)
2. Encrypt file with ChaCha20-Poly1305
3. Upload encrypted file to IPFS (returns CID)
4. Split session key using Shamir's Secret Sharing
5. Distribute shards separately

**Decryption:**
1. Collect minimum threshold of shards
2. Reconstruct session key
3. Download encrypted file from IPFS
4. Decrypt with reconstructed key

**Security Properties:**
- Session keys are ephemeral and zeroized after use
- Encrypted files stored on distributed network (IPFS)
- Key shards can be distributed through separate channels
- Threshold scheme prevents single point of failure

## Installation

### Prerequisites

```bash
# Rust (1.70+)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Node.js (16+)
# Download from https://nodejs.org/

# IPFS (for Dead Drop mode)
# macOS: brew install ipfs
# Linux: https://docs.ipfs.tech/install/
```

### Build & Run

```bash
# Install dependencies
npm install

# Start IPFS daemon (separate terminal)
ipfs daemon

# Run development build
npm run tauri dev

# Build for production
npm run tauri build
```

## Usage

### Identity Setup

On first launch, the application automatically generates a cryptographic identity. The public key is displayed in the Identity tab and can be shared with others.

### Ghost Mode

1. Navigate to Ghost Mode tab
2. Click "ACTIVATE GHOST MODE"
3. Enter recipient's public key
4. Type message and click "TRANSMIT"

**Note:** Both peers must be on the same local network or connected through a relay server.

### Dead Drop

1. Navigate to Dead Drop tab
2. Configure threshold (minimum shards) and total shards
3. Drag and drop file or click to select
4. Wait for encryption and upload
5. Copy CID and distribute shards to recipients

**Retrieval:**
1. Collect minimum threshold of shards
2. Enter CID and shards
3. Download and decrypt file

## Security Considerations

### Cryptographic Primitives

| Component | Algorithm | Key Size | Purpose |
|-----------|-----------|----------|---------|
| Key Exchange | X25519 | 256-bit | ECDH for shared secrets |
| Symmetric Encryption | ChaCha20-Poly1305 | 256-bit | File and message encryption |
| Password Hashing | Argon2id | 16MB, 3 iter | Key derivation |
| Identity Storage | AES-256-GCM | 256-bit | Encrypted key storage |

### Memory Safety

- All cryptographic keys implement `Zeroize` trait
- Private keys are explicitly cleared from memory after use
- Session keys use `ZeroizeOnDrop` for automatic cleanup
- No long-lived secrets in memory

### Network Security

- All P2P messages are end-to-end encrypted
- No plaintext metadata transmitted
- Topic-based routing prevents broadcast
- Optional relay servers for NAT traversal

### Threat Model

**Protected Against:**
- Network eavesdropping (encryption)
- Server compromise (no servers)
- Memory dumps (key zeroization)
- Partial key exposure (threshold secret sharing)

**Not Protected Against:**
- Endpoint compromise (malware on device)
- Physical access to unlocked device
- Quantum computers (classical cryptography)

## Performance

### Memory Usage

| Operation | RAM Usage |
|-----------|-----------|
| Base Application | ~50 MB |
| Ghost Mode Active | ~70 MB |
| File Encryption (10GB) | ~8 MB (streaming) |

### Cryptographic Operations

| Operation | Time |
|-----------|------|
| Identity Generation | ~2-3 seconds |
| Key Exchange (ECDH) | <1 ms |
| Message Encryption | <1 ms |
| File Encryption (1MB) | ~5-10 ms |

### Network Performance

| Operation | Latency |
|-----------|---------|
| Local P2P Message | 10-50 ms |
| Message Delivery ACK | 1-2 seconds |
| IPFS Upload (1MB) | 100-500 ms |

## Configuration

### Argon2 Parameters

Located in `src-tauri/src/crypto.rs`:

```rust
Params::new(
    16384, // 16 MB memory
    3,     // 3 iterations
    1,     // 1 thread
    None,
)
```

### IPFS Endpoint

Located in `src-tauri/src/dead_drop.rs`:

```rust
const IPFS_API_URL: &str = "http://127.0.0.1:5001/api/v0";
```

### Chunk Size (Streaming)

Located in `src-tauri/src/dead_drop.rs`:

```rust
const CHUNK_SIZE: usize = 4 * 1024 * 1024; // 4MB
```

## Development

### Project Structure

```
control/
├── src/                    # Frontend (React)
│   ├── App.tsx            # Main application
│   ├── components/        # UI components
│   └── index.css          # Styles
├── src-tauri/             # Backend (Rust)
│   ├── src/
│   │   ├── main.rs        # Tauri commands
│   │   ├── crypto.rs      # Cryptography
│   │   ├── p2p.rs         # P2P networking
│   │   └── dead_drop.rs   # File encryption
│   └── Cargo.toml         # Rust dependencies
└── package.json           # Node dependencies
```

### Testing

```bash
# Run Rust tests
cd src-tauri
cargo test

# Check compilation
cargo check

# Run with logging
RUST_LOG=debug cargo tauri dev
```

## Troubleshooting

### IPFS Connection Failed

Ensure IPFS daemon is running:
```bash
ipfs daemon
```

### Ghost Mode: No Peers Found

- Verify both instances are on the same network
- Check firewall settings
- Ensure Ghost Mode is activated on both peers

### Identity Loading Failed

Delete old identity file and restart:
```bash
# Windows
del %APPDATA%\com.control.app\identity.enc

# Linux/macOS
rm ~/.local/share/com.control.app/identity.enc
```

## License

MIT License - See LICENSE file for details

## Contributing

This is a security-focused project. All contributions should:
- Include tests for new functionality
- Follow Rust best practices
- Maintain memory safety guarantees
- Document cryptographic decisions

## Acknowledgments

Built with:
- [Tauri](https://tauri.app/) - Desktop application framework
- [libp2p](https://libp2p.io/) - P2P networking
- [IPFS](https://ipfs.tech/) - Distributed storage
- [RustCrypto](https://github.com/RustCrypto) - Cryptographic primitives

