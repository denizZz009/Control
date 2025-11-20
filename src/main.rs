#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod crypto;
mod dead_drop;
mod p2p;

use crypto::Identity;
use dead_drop::{create_dead_drop, retrieve_dead_drop, DeadDropCreated};
use p2p::{init_p2p_actor, P2PCommand};
use std::sync::Mutex;
use tauri::State;
use tokio::sync::mpsc;

/// Application state shared across commands
pub struct AppState {
    pub identity: Mutex<Option<Identity>>,
    pub p2p_sender: Mutex<Option<mpsc::Sender<P2PCommand>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            identity: Mutex::new(None),
            p2p_sender: Mutex::new(None),
        }
    }
}

/// Initialize identity with password
#[tauri::command]
async fn init_identity(password: String, state: State<'_, AppState>) -> Result<String, String> {
    let app_data_dir = tauri::api::path::app_data_dir(&tauri::Config::default())
        .ok_or("Failed to get app data directory")?;

    // Try to load or generate identity
    let identity = match Identity::load_or_generate(&password, app_data_dir.clone()) {
        Ok(id) => id,
        Err(e) => {
            // If loading fails, delete old identity file and create new one
            eprintln!("Failed to load identity: {}. Creating new identity...", e);
            let identity_path = app_data_dir.join("identity.enc");
            if identity_path.exists() {
                std::fs::remove_file(&identity_path)
                    .map_err(|e| format!("Failed to delete old identity: {}", e))?;
            }
            Identity::load_or_generate(&password, app_data_dir)
                .map_err(|e| format!("Failed to create new identity: {}", e))?
        }
    };

    let public_id = identity.public_id();

    *state.identity.lock().unwrap() = Some(identity);

    Ok(public_id)
}

/// Get current public identity
#[tauri::command]
async fn get_public_id(state: State<'_, AppState>) -> Result<String, String> {
    let identity_guard = state.identity.lock().unwrap();
    let identity = identity_guard
        .as_ref()
        .ok_or("Identity not initialized")?;

    Ok(identity.public_id())
}

/// Start Ghost Mode (P2P messaging)
#[tauri::command]
async fn start_ghost_mode(
    window: tauri::Window,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let identity = {
        let identity_guard = state.identity.lock().unwrap();
        identity_guard
            .as_ref()
            .ok_or("Identity not initialized")?
            .clone()
    };

    let p2p_sender = init_p2p_actor(identity.clone(), window)
        .map_err(|e| format!("Failed to start P2P: {}", e))?;

    *state.p2p_sender.lock().unwrap() = Some(p2p_sender);

    Ok("Ghost Mode activated".to_string())
}

/// Send encrypted message via P2P with ACK tracking
#[tauri::command]
async fn send_ghost_message(
    target_public_key: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let sender = {
        let sender_guard = state.p2p_sender.lock().unwrap();
        sender_guard
            .as_ref()
            .ok_or("Ghost Mode not started")?
            .clone()
    };

    // Generate UUID for message tracking
    let message_id = uuid::Uuid::new_v4().to_string();

    sender
        .send(P2PCommand::SendMessage {
            target_public_key,
            content,
            message_id: message_id.clone(),
        })
        .await
        .map_err(|e| format!("Failed to send message: {}", e))?;

    // Return message_id so frontend can track delivery
    Ok(message_id)
}

/// Create a dead drop (encrypt, upload to IPFS, split key)
#[tauri::command]
async fn create_drop(
    file_path: String,
    threshold: u8,
    total_shards: u8,
) -> Result<DeadDropCreated, String> {
    create_dead_drop(&file_path, threshold, total_shards)
        .await
        .map_err(|e| format!("Failed to create dead drop: {}", e))
}

/// Retrieve a dead drop (download from IPFS, combine shards, decrypt)
#[tauri::command]
async fn retrieve_drop(
    cid: String,
    shards: Vec<String>,
    output_path: String,
) -> Result<(), String> {
    retrieve_dead_drop(&cid, shards, &output_path)
        .await
        .map_err(|e| format!("Failed to retrieve dead drop: {}", e))
}

/// Shutdown P2P actor
#[tauri::command]
async fn stop_ghost_mode(state: State<'_, AppState>) -> Result<(), String> {
    let sender = {
        let sender_guard = state.p2p_sender.lock().unwrap();
        sender_guard.as_ref().cloned()
    };
    
    if let Some(sender) = sender {
        sender
            .send(P2PCommand::Shutdown)
            .await
            .map_err(|e| format!("Failed to stop P2P: {}", e))?;
    }

    Ok(())
}

/// Test IPFS connection
#[tauri::command]
async fn test_ipfs() -> Result<String, String> {
    let client = reqwest::Client::new();
    
    match client
        .post("http://127.0.0.1:5001/api/v0/version")
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                let text = response.text().await.unwrap_or_default();
                Ok(format!("IPFS Connected: {}", text))
            } else {
                Err(format!("IPFS returned error: {}", response.status()))
            }
        }
        Err(e) => Err(format!("IPFS not running: {}", e)),
    }
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            init_identity,
            get_public_id,
            start_ghost_mode,
            send_ghost_message,
            create_drop,
            retrieve_drop,
            stop_ghost_mode,
            test_ipfs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
