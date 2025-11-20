use crate::crypto::{decrypt_message, encrypt_message, Identity};
use anyhow::{Context, Result};
use futures::StreamExt;
use libp2p::{
    dcutr,
    gossipsub::{self, IdentTopic, MessageAuthenticity, ValidationMode},
    identify, identity::Keypair, mdns, noise,
    relay,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::time::Duration;
use tauri::Window;
use tokio::sync::mpsc;
use x25519_dalek::PublicKey;

/// Commands sent to the P2P actor
#[derive(Debug)]
pub enum P2PCommand {
    SendMessage {
        target_public_key: String,
        content: String,
        message_id: String, // UUID for tracking ACKs
    },
    Shutdown,
}

/// Message structure for Ghost Mode with UUID for ACK tracking
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GhostMessage {
    pub id: String, // UUID
    pub from: String,
    pub content: String,
    pub timestamp: u64,
}

/// ACK/Receipt message
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MessageReceipt {
    pub message_id: String, // UUID of original message
    pub from: String,       // Who is acknowledging
    pub timestamp: u64,
}

/// Message type enum for routing
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum P2PMessage {
    #[serde(rename = "message")]
    Message(GhostMessage),
    #[serde(rename = "receipt")]
    Receipt(MessageReceipt),
}

/// P2P Network Behavior with Relay, Identify, and DCUtR
#[derive(NetworkBehaviour)]
struct DeadDropBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    relay_client: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
    identify: identify::Behaviour,
    ping: libp2p::ping::Behaviour,
}

/// Pending ACKs tracker
struct PendingAcks {
    pending: HashMap<String, (String, u64)>, // message_id -> (target_public_key, timestamp)
}

impl PendingAcks {
    fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    fn add(&mut self, message_id: String, target: String) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.pending.insert(message_id, (target, timestamp));
    }

    fn remove(&mut self, message_id: &str) -> Option<(String, u64)> {
        self.pending.remove(message_id)
    }

    fn cleanup_old(&mut self, max_age_secs: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.pending.retain(|_, (_, timestamp)| {
            now - *timestamp < max_age_secs
        });
    }
}

/// Initialize P2P actor with the Actor Model pattern
/// Returns a channel sender to communicate with the actor
pub fn init_p2p_actor(identity: Identity, window: Window) -> Result<mpsc::Sender<P2PCommand>> {
    let (tx, mut rx) = mpsc::channel::<P2PCommand>(100);

    // Clone identity for the actor thread
    let actor_identity = identity.clone();
    let public_id = identity.public_id();

    tokio::spawn(async move {
        if let Err(e) = run_p2p_actor(actor_identity, public_id, &mut rx, window).await {
            eprintln!("P2P Actor error: {}", e);
        }
    });

    Ok(tx)
}

/// The P2P actor loop - owns the Swarm
async fn run_p2p_actor(
    identity: Identity,
    public_id: String,
    rx: &mut mpsc::Receiver<P2PCommand>,
    window: Window,
) -> Result<()> {
    // Create libp2p identity from random keypair (separate from X25519)
    let local_key = Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local PeerID: {}", local_peer_id);
    println!("Public Identity: {}", public_id);

    // Build transport with relay support
    let (relay_transport, relay_client) = relay::client::new(local_peer_id);

    let transport = tcp::tokio::Transport::default()
        .or_transport(relay_transport)
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(noise::Config::new(&local_key)?)
        .multiplex(yamux::Config::default())
        .boxed();

    // Configure GossipSub
    let message_id_fn = |message: &gossipsub::Message| {
        let mut s = DefaultHasher::new();
        message.data.hash(&mut s);
        gossipsub::MessageId::from(s.finish().to_string())
    };

    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(ValidationMode::Permissive)
        .message_id_fn(message_id_fn)
        .build()
        .map_err(|e| anyhow::anyhow!("GossipSub config error: {}", e))?;

    let mut gossipsub = gossipsub::Behaviour::new(
        MessageAuthenticity::Signed(local_key.clone()),
        gossipsub_config,
    )
    .map_err(|e| anyhow::anyhow!("GossipSub init error: {}", e))?;

    // Subscribe to personal inbox topic
    let inbox_topic = IdentTopic::new(format!("/deaddrop/inbox/{}", public_id));
    gossipsub.subscribe(&inbox_topic)?;
    println!("Subscribed to topic: {}", inbox_topic);

    // Create mDNS for local peer discovery
    let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_peer_id)?;

    // Create Identify protocol for peer information exchange
    let identify = identify::Behaviour::new(identify::Config::new(
        "/deaddrop/1.0.0".to_string(),
        local_key.public(),
    ));

    // Create DCUtR for NAT hole punching
    let dcutr = dcutr::Behaviour::new(local_peer_id);

    // Create Ping for connection health
    let ping = libp2p::ping::Behaviour::new(libp2p::ping::Config::new());

    // Build Swarm
    let behaviour = DeadDropBehaviour {
        gossipsub,
        mdns,
        relay_client,
        dcutr,
        identify,
        ping,
    };

    let mut swarm = Swarm::new(
        transport,
        behaviour,
        local_peer_id,
        libp2p::swarm::Config::with_tokio_executor(),
    );

    // Listen on all interfaces
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // Connect to public relay servers for NAT traversal
    // These are example relay addresses - in production, use your own or public relays
    let relay_addresses: Vec<&str> = vec![
        // Add public relay multiaddrs here when available
        // Example: "/ip4/relay.example.com/tcp/4001/p2p/12D3KooW..."
    ];

    for addr_str in relay_addresses {
        if let Ok(addr) = addr_str.parse::<Multiaddr>() {
            if let Err(e) = swarm.dial(addr.clone()) {
                eprintln!("Failed to dial relay {}: {}", addr, e);
            } else {
                println!("Dialing relay: {}", addr);
            }
        }
    }

    println!("P2P Actor started successfully with Relay & Identify support");

    // Track pending ACKs
    let mut pending_acks = PendingAcks::new();
    
    // Queue for receipts to send
    let mut receipt_queue: ReceiptQueue = Vec::new();

    // Main event loop
    loop {
        tokio::select! {
            // Handle incoming P2P events
            event = swarm.select_next_some() => {
                if let Err(e) = handle_swarm_event(
                    event,
                    &identity,
                    &window,
                    &mut pending_acks,
                    &mut receipt_queue,
                ).await {
                    eprintln!("Error handling swarm event: {}", e);
                }
                
                // Process queued receipts
                while let Some((sender_pk, msg_id, sender_id)) = receipt_queue.pop() {
                    if let Err(e) = send_receipt(
                        &mut swarm,
                        &identity,
                        &sender_pk,
                        &msg_id,
                        &sender_id,
                    ) {
                        eprintln!("Failed to send receipt: {}", e);
                    }
                }
            }

            // Handle incoming commands from application
            Some(cmd) = rx.recv() => {
                match cmd {
                    P2PCommand::SendMessage { target_public_key, content, message_id } => {
                        // Track this message for ACK
                        pending_acks.add(message_id.clone(), target_public_key.clone());

                        if let Err(e) = send_ghost_message(
                            &mut swarm,
                            &identity,
                            &target_public_key,
                            &content,
                            &message_id,
                        ) {
                            eprintln!("Failed to send message: {}", e);
                            let _ = window.emit("ghost_error", format!("Send failed: {}", e));
                        }
                    }
                    P2PCommand::Shutdown => {
                        println!("P2P Actor shutting down");
                        break;
                    }
                }
            }

            // Periodic cleanup of old pending ACKs (every 60 seconds)
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                pending_acks.cleanup_old(300); // Remove ACKs older than 5 minutes
            }
        }
    }

    Ok(())
}

/// Handle Swarm events including Relay, Identify, and DCUtR
async fn handle_swarm_event<THandlerErr>(
    event: SwarmEvent<DeadDropBehaviourEvent, THandlerErr>,
    identity: &Identity,
    window: &Window,
    pending_acks: &mut PendingAcks,
    receipt_queue: &mut ReceiptQueue,
) -> Result<()>
where
    THandlerErr: std::fmt::Debug,
{
    match event {
        SwarmEvent::Behaviour(DeadDropBehaviourEvent::Gossipsub(
            gossipsub::Event::Message {
                propagation_source: _,
                message_id: _,
                message,
            },
        )) => {
            // Handle incoming message or receipt
            if let Err(e) = handle_incoming_p2p_message(message, identity, window, pending_acks, receipt_queue) {
                eprintln!("Failed to handle incoming message: {}", e);
            }
        }
        SwarmEvent::Behaviour(DeadDropBehaviourEvent::Mdns(mdns::Event::Discovered(peers))) => {
            for (peer_id, _) in peers {
                println!("mDNS: Discovered peer: {}", peer_id);
            }
        }
        SwarmEvent::Behaviour(DeadDropBehaviourEvent::Mdns(mdns::Event::Expired(peers))) => {
            for (peer_id, _) in peers {
                println!("mDNS: Peer expired: {}", peer_id);
            }
        }
        SwarmEvent::Behaviour(DeadDropBehaviourEvent::Identify(identify::Event::Received {
            peer_id,
            info,
        })) => {
            println!("Identify: Received info from {}", peer_id);
            println!("  Protocol Version: {}", info.protocol_version);
            println!("  Agent Version: {}", info.agent_version);
            println!("  Listen Addrs: {:?}", info.listen_addrs);
        }
        SwarmEvent::Behaviour(DeadDropBehaviourEvent::RelayClient(
            relay::client::Event::ReservationReqAccepted { relay_peer_id, .. },
        )) => {
            println!("Relay: Reservation accepted by {}", relay_peer_id);
            let _ = window.emit("relay_connected", relay_peer_id.to_string());
        }
        SwarmEvent::Behaviour(DeadDropBehaviourEvent::Dcutr(event)) => {
            match event {
                dcutr::Event::RemoteInitiatedDirectConnectionUpgrade { remote_peer_id, .. } => {
                    println!("DCUtR: Remote initiated hole punch with {}", remote_peer_id);
                }
                dcutr::Event::InitiatedDirectConnectionUpgrade { remote_peer_id, .. } => {
                    println!("DCUtR: Initiated hole punch with {}", remote_peer_id);
                }
                dcutr::Event::DirectConnectionUpgradeSucceeded { remote_peer_id } => {
                    println!("DCUtR: Hole punch successful with {}", remote_peer_id);
                }
                dcutr::Event::DirectConnectionUpgradeFailed { remote_peer_id, error } => {
                    eprintln!("DCUtR: Hole punch failed with {}: {:?}", remote_peer_id, error);
                }
            }
        }
        SwarmEvent::NewListenAddr { address, .. } => {
            println!("Listening on: {}", address);
        }
        SwarmEvent::ConnectionEstablished {
            peer_id, endpoint, ..
        } => {
            println!("Connection established with {} via {}", peer_id, endpoint.get_remote_address());
        }
        _ => {}
    }
    Ok(())
}

/// Receipt queue for sending ACKs
type ReceiptQueue = Vec<(PublicKey, String, String)>; // (sender_public_key, message_id, sender_id)

/// Handle incoming P2P message (either GhostMessage or Receipt)
fn handle_incoming_p2p_message(
    message: gossipsub::Message,
    identity: &Identity,
    window: &Window,
    pending_acks: &mut PendingAcks,
    receipt_queue: &mut ReceiptQueue,
) -> Result<()> {
    // Message format: sender_public_key (32 bytes) || encrypted_payload
    if message.data.len() < 32 {
        anyhow::bail!("Invalid message format: too short");
    }

    let (sender_key_bytes, encrypted_payload) = message.data.split_at(32);

    // Parse sender's public key
    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(sender_key_bytes);
    let sender_public_key = PublicKey::from(key_array);

    // Perform ECDH to get shared secret
    let shared_secret = identity.shared_secret(&sender_public_key);

    // Decrypt message
    let decrypted = decrypt_message(&shared_secret, encrypted_payload)?;
    let message_json = String::from_utf8(decrypted)?;

    // Parse as P2PMessage to determine type
    let p2p_message: P2PMessage = serde_json::from_str(&message_json)?;

    match p2p_message {
        P2PMessage::Message(ghost_msg) => {
            println!(
                "Received message from {}: {}",
                ghost_msg.from, ghost_msg.content
            );

            // Queue receipt to be sent back
            receipt_queue.push((
                sender_public_key,
                ghost_msg.id.clone(),
                ghost_msg.from.clone(),
            ));

            // Emit to frontend
            window
                .emit("ghost_msg", &ghost_msg)
                .context("Failed to emit message to frontend")?;
        }
        P2PMessage::Receipt(receipt) => {
            println!(
                "Received ACK for message {} from {}",
                receipt.message_id, receipt.from
            );

            // Remove from pending ACKs
            if let Some((target, _)) = pending_acks.remove(&receipt.message_id) {
                // Emit delivery confirmation to frontend
                window
                    .emit(
                        "msg_delivered",
                        serde_json::json!({
                            "message_id": receipt.message_id,
                            "target": target,
                            "delivered_at": receipt.timestamp,
                        }),
                    )
                    .context("Failed to emit delivery confirmation")?;
            }
        }
    }

    Ok(())
}

/// Send a receipt/ACK back to the sender
fn send_receipt(
    swarm: &mut libp2p::Swarm<DeadDropBehaviour>,
    identity: &Identity,
    sender_public_key: &PublicKey,
    message_id: &str,
    sender_id: &str,
) -> Result<()> {
    // Create receipt
    let receipt = MessageReceipt {
        message_id: message_id.to_string(),
        from: identity.public_id(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    // Wrap in P2PMessage enum
    let p2p_message = P2PMessage::Receipt(receipt);
    let message_json = serde_json::to_string(&p2p_message)?;

    // Perform ECDH
    let shared_secret = identity.shared_secret(sender_public_key);

    // Encrypt receipt
    let encrypted_payload = encrypt_message(&shared_secret, message_json.as_bytes())?;

    // Prepend our public key
    let mut full_message = identity.public_key.as_bytes().to_vec();
    full_message.extend_from_slice(&encrypted_payload);

    // Publish to sender's inbox topic
    let topic = IdentTopic::new(format!("/deaddrop/inbox/{}", sender_id));
    swarm
        .behaviour_mut()
        .gossipsub
        .publish(topic, full_message)
        .map_err(|e| anyhow::anyhow!("Receipt publish failed: {}", e))?;

    println!("Receipt sent for message {} to {}", message_id, sender_id);

    Ok(())
}

/// Send encrypted message via GossipSub with UUID for ACK tracking
fn send_ghost_message(
    swarm: &mut libp2p::Swarm<DeadDropBehaviour>,
    identity: &Identity,
    target_public_key_b58: &str,
    content: &str,
    message_id: &str,
) -> Result<()> {
    // Decode target's public key
    let target_key_bytes = bs58::decode(target_public_key_b58)
        .into_vec()
        .context("Invalid base58 public key")?;

    if target_key_bytes.len() != 32 {
        anyhow::bail!("Invalid public key length");
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&target_key_bytes);
    let target_public_key = PublicKey::from(key_array);

    // Create message with UUID
    let ghost_msg = GhostMessage {
        id: message_id.to_string(),
        from: identity.public_id(),
        content: content.to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };

    // Wrap in P2PMessage enum
    let p2p_message = P2PMessage::Message(ghost_msg);
    let message_json = serde_json::to_string(&p2p_message)?;

    // Perform ECDH
    let shared_secret = identity.shared_secret(&target_public_key);

    // Encrypt message
    let encrypted_payload = encrypt_message(&shared_secret, message_json.as_bytes())?;

    // Prepend sender's public key
    let mut full_message = identity.public_key.as_bytes().to_vec();
    full_message.extend_from_slice(&encrypted_payload);

    // Publish to target's inbox topic
    let topic = IdentTopic::new(format!("/deaddrop/inbox/{}", target_public_key_b58));
    swarm
        .behaviour_mut()
        .gossipsub
        .publish(topic, full_message)
        .map_err(|e| anyhow::anyhow!("Publish failed: {}", e))?;

    println!("Message {} sent to {}", message_id, target_public_key_b58);

    Ok(())
}
