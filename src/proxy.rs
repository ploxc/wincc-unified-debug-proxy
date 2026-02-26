use anyhow::Result;
use colored::Colorize;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use warp::Filter;

use crate::config::config;
use crate::logging::*;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DebugTarget {
    description: String,
    devtools_frontend_url: String,
    id: String,
    title: String,
    #[serde(rename = "type")]
    target_type: String,
    url: String,
    web_socket_debugger_url: String,
}

#[derive(Debug)]
struct AppState {
    dynamics_path: Option<String>,
    events_path: Option<String>,
    highest_dynamics_vcs: u32,
    highest_events_vcs: u32,
    consecutive_failures: u32,
    target_available: bool,
    dynamics_clients_shutdown_tx: Option<broadcast::Sender<()>>,
    dynamics_server_shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    dynamics_server_handle: Option<tokio::task::JoinHandle<()>>,
    events_clients_shutdown_tx: Option<broadcast::Sender<()>>,
    events_server_shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    events_server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            dynamics_path: None,
            events_path: None,
            highest_dynamics_vcs: 0,
            highest_events_vcs: 0,
            consecutive_failures: 0,
            target_available: false,
            dynamics_clients_shutdown_tx: None,
            dynamics_server_shutdown_tx: None,
            dynamics_server_handle: None,
            events_clients_shutdown_tx: None,
            events_server_shutdown_tx: None,
            events_server_handle: None,
        }
    }
}

type SharedState = Arc<RwLock<AppState>>;

// ============================================================================
// CDP Message Rewriting
// ============================================================================

/// Shorten a WinCC script URL by stripping known prefixes and intermediate segments.
///
/// Transforms paths like:
///   /screen_modules/Screen_Content/HMI_RT_1::HMI_Screen/faceplate_modules/CM_Freq/Events.js
/// Into:
///   HMI_Screen/CM_Freq/Events.js
fn shorten_script_url(url: &str) -> Option<String> {
    // Strip optional leading slash, then the known prefix
    let rest = url.strip_prefix('/').unwrap_or(url);
    let rest = rest.strip_prefix("screen_modules/Screen_Content/")?;

    // Strip HMI_RT_\d+:: (double colon) or HMI_RT_\d+: (single colon) prefix
    let rest = if let Some(colon_pos) = rest.find(':') {
        let before_colon = &rest[..colon_pos];
        if before_colon.starts_with("HMI_RT_")
            && before_colon["HMI_RT_".len()..].chars().all(|c| c.is_ascii_digit())
        {
            // Skip past all consecutive colons (handles both : and ::)
            let after_colon = &rest[colon_pos..];
            after_colon.trim_start_matches(':')
        } else {
            rest
        }
    } else {
        rest
    };

    // Strip faceplate_modules/ intermediate segment
    let result = rest.replace("/faceplate_modules/", "/");

    Some(result)
}

/// Inspect a CDP JSON message; if it is a `Debugger.scriptParsed` event,
/// rewrite `params.url` to a shorter form. Returns the (possibly rewritten) text.
fn maybe_rewrite_cdp_message(text: &str) -> String {
    // Skip rewriting when long paths are requested
    if config().long_paths {
        return text.to_string();
    }

    // Quick bailout: avoid JSON parsing for the vast majority of messages
    if !text.contains("scriptParsed") {
        return text.to_string();
    }

    let mut parsed: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return text.to_string(),
    };

    // Check that this is a Debugger.scriptParsed event
    let is_script_parsed = parsed
        .get("method")
        .and_then(|m| m.as_str())
        .map_or(false, |m| m == "Debugger.scriptParsed");

    if !is_script_parsed {
        return text.to_string();
    }

    // Try to rewrite params.url
    if let Some(params) = parsed.get_mut("params") {
        if let Some(url_val) = params.get("url") {
            if let Some(url_str) = url_val.as_str() {
                if let Some(short) = shorten_script_url(url_str) {
                    log_verbose(&format!("Rewrote script URL: {} -> {}", url_str, short));
                    params.as_object_mut().unwrap().insert(
                        "url".to_string(),
                        serde_json::Value::String(short),
                    );
                    // Re-serialize
                    return serde_json::to_string(&parsed).unwrap_or_else(|_| text.to_string());
                }
            }
        }
    }

    text.to_string()
}

// ============================================================================
// TCP Connectivity Check
// ============================================================================

async fn wait_for_target_connectivity() {
    let cfg = config();
    let addr = format!("{}:{}", cfg.target_host, cfg.target_port);
    let mut shown_error = false;

    loop {
        log_verbose(&format!("Checking TCP connectivity to {}...", addr));

        match tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::TcpStream::connect(&addr),
        )
        .await
        {
            Ok(Ok(_)) => {
                log_success(&format!("Target {} is reachable", addr));
                return;
            }
            Ok(Err(e)) => {
                if !shown_error {
                    log_warn(&format!("Cannot connect to {}: {}", addr, e));
                    log_warn("Troubleshooting:");
                    log_warn("  - Is WinCC Unified running with debugging enabled?");
                    log_warn("  - Check firewall rules for port 9222 (in/out)");
                    log_warn("  - If remote: verify netsh portproxy is configured");
                    log_warn("  - After Windows restart: delete and re-add netsh rules");
                    log_warn("  - Run with --help for detailed setup instructions");
                    shown_error = true;
                }
                log(&format!("Retrying in {} seconds...", cfg.poll_interval));
            }
            Err(_) => {
                if !shown_error {
                    log_warn(&format!("Connection to {} timed out", addr));
                    log_warn("Troubleshooting:");
                    log_warn("  - Is WinCC Unified running with debugging enabled?");
                    log_warn("  - Check firewall rules for port 9222 (in/out)");
                    log_warn("  - If remote: verify netsh portproxy is configured");
                    log_warn("  - After Windows restart: delete and re-add netsh rules");
                    log_warn("  - Run with --help for detailed setup instructions");
                    shown_error = true;
                }
                log(&format!("Retrying in {} seconds...", cfg.poll_interval));
            }
        }

        tokio::time::sleep(Duration::from_secs(cfg.poll_interval)).await;
    }
}

// ============================================================================
// Target Discovery & Health Checking
// ============================================================================

fn extract_vcs_number(title: &str) -> Option<u32> {
    // Parse "VCS_8" from titles like " @localhost VCS_8 Dynamics"
    title.split("VCS_")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_numeric())
            .next()
            .and_then(|n| n.parse().ok()))
}

async fn select_best_target(
    candidates: Vec<DebugTarget>,
    target_type: &str,
    current_highest_vcs: u32,
) -> Option<(DebugTarget, u32)> {
    if candidates.is_empty() {
        return None;
    }

    log_verbose(&format!("Selecting best {} target from {} candidates", target_type, candidates.len()));

    if candidates.is_empty() {
        log_error(&format!("No alive {} targets found!", target_type));
        return None;
    }

    // Select target with highest VCS number
    let best_target = candidates.into_iter()
        .max_by_key(|t| extract_vcs_number(&t.title).unwrap_or(0))?;

    let vcs_num = extract_vcs_number(&best_target.title).unwrap_or(0);

    // Only update highest VCS if new number is higher
    let new_highest = if vcs_num > current_highest_vcs {
        log_verbose(&format!("  VCS number increased: {} -> {}", current_highest_vcs, vcs_num));
        vcs_num
    } else {
        current_highest_vcs
    };

    Some((best_target, new_highest))
}

async fn fetch_targets() -> Result<Vec<DebugTarget>> {
    let cfg = config();
    let url = format!("http://{}:{}/json", cfg.target_host, cfg.target_port);
    log_verbose(&format!("Fetching targets from {}", url));

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client.get(&url).send().await?;
    let targets: Vec<DebugTarget> = response.json().await?;

    log_verbose(&format!("Received {} debug targets", targets.len()));
    Ok(targets)
}

async fn restart_server(state: SharedState, target_name: &str, old_path: String, new_path: String) {
    let old_decoded = urlencoding::decode(&old_path).unwrap_or_else(|_| old_path.clone().into());
    let new_decoded = urlencoding::decode(&new_path).unwrap_or_else(|_| new_path.clone().into());
    println!(
        "{} {} {} target changed:",
        format!("[{}]", timestamp()).dimmed(),
        "[CHANGE]".blue().bold(),
        target_name
    );
    log(&format!("   Old: {}", old_decoded));
    log(&format!("   New: {}", new_decoded));

    // Clean dumped scripts for this target type
    if let Some(ref dump_dir) = config().dump_output {
        let subdir = std::path::Path::new(dump_dir).join(target_name);
        if subdir.exists() {
            let _ = std::fs::remove_dir_all(&subdir);
            log(&format!("   Cleaned {}/", subdir.display()));
        }
    }

    println!(
        "{} {} Closing all {} client connections...",
        format!("[{}]", timestamp()).dimmed(),
        "[STOP]".magenta().bold(),
        target_name
    );

    // Step 1: Send shutdown signal to all clients
    let shutdown_tx = {
        let mut state_guard = state.write().await;
        match target_name {
            "Dynamics" => state_guard.dynamics_clients_shutdown_tx.take(),
            "Events" => state_guard.events_clients_shutdown_tx.take(),
            _ => None,
        }
    };

    if let Some(tx) = shutdown_tx {
        let _ = tx.send(());
        log(&format!(
            "   Sent disconnect signal to all {} clients",
            target_name
        ));
    }

    // Give clients a moment to close cleanly
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Step 2: Send shutdown signal to server and take the handle
    let (server_handle, server_shutdown_tx) = {
        let mut state_guard = state.write().await;
        let handle = match target_name {
            "Dynamics" => state_guard.dynamics_server_handle.take(),
            "Events" => state_guard.events_server_handle.take(),
            _ => None,
        };
        let tx = match target_name {
            "Dynamics" => state_guard.dynamics_server_shutdown_tx.take(),
            "Events" => state_guard.events_server_shutdown_tx.take(),
            _ => None,
        };

        // Store new path
        match target_name {
            "Dynamics" => state_guard.dynamics_path = Some(new_path.clone()),
            "Events" => state_guard.events_path = Some(new_path.clone()),
            _ => {}
        }

        (handle, tx)
    };

    if let Some(tx) = server_shutdown_tx {
        let _ = tx.send(());
        log(&format!("   Stopping {} proxy server...", target_name));
    }

    // Wait for server to actually stop
    if let Some(handle) = server_handle {
        log(&format!(
            "   Waiting for {} server shutdown...",
            target_name
        ));
        let _ = handle.await;
    }

    // Start new server (this waits until server is ready)
    log(&format!("   Restarting {} proxy server...", target_name));
    match target_name {
        "Dynamics" => start_dynamics_server(state.clone()).await,
        "Events" => start_events_server(state.clone()).await,
        _ => {}
    }
}

enum TargetChange {
    Initial { path: String, vcs: u32 },
    Changed { old: String, new: String, vcs: u32 },
    None { vcs: u32 },
}

fn check_target_change(
    result: Option<(DebugTarget, u32)>,
    current_path: &Option<String>,
    target_name: &str,
    candidate_count: usize,
) -> TargetChange {
    let (target, new_vcs) = match result {
        Some(pair) => pair,
        None => return TargetChange::None { vcs: 0 },
    };

    let path = match target.web_socket_debugger_url.split('/').last() {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return TargetChange::None { vcs: new_vcs },
    };

    if current_path.as_ref() == Some(&path) {
        return TargetChange::None { vcs: new_vcs };
    }

    if candidate_count > 1 {
        log_warn(&format!(
            "Multiple alive {} targets found ({}), selecting highest VCS number!",
            target_name, candidate_count
        ));
    }

    match current_path {
        Some(old) => TargetChange::Changed {
            old: old.clone(),
            new: path,
            vcs: new_vcs,
        },
        None => TargetChange::Initial { path, vcs: new_vcs },
    }
}

async fn update_targets(state: SharedState) {
    log_verbose("--- Target Update Cycle ---");

    match fetch_targets().await {
        Ok(targets) => {
            let mut state_guard = state.write().await;

            // Reset failure counter on success
            if state_guard.consecutive_failures > 0 {
                log_success("Target server is back online!");
                state_guard.consecutive_failures = 0;
            }

            if !state_guard.target_available {
                state_guard.target_available = true;
                let cfg = config();
                println!(
                    "{} {} WinCC target server connected at {}:{}",
                    format!("[{}]", timestamp()).dimmed(),
                    "[CONN]".cyan().bold(),
                    cfg.target_host,
                    cfg.target_port
                );
            }

            // Separate Dynamics and Events targets
            let dynamics_candidates: Vec<DebugTarget> = targets
                .iter()
                .filter(|t| t.title.to_lowercase().contains("dynamics"))
                .cloned()
                .collect();

            let events_candidates: Vec<DebugTarget> = targets
                .iter()
                .filter(|t| t.title.to_lowercase().contains("events"))
                .cloned()
                .collect();

            let current_dynamics_vcs = state_guard.highest_dynamics_vcs;
            let current_events_vcs = state_guard.highest_events_vcs;

            let dynamics_count = dynamics_candidates.len();
            let events_count = events_candidates.len();

            // Release lock during health checks
            drop(state_guard);

            // Select best targets with health checks
            let dynamics_result = select_best_target(
                dynamics_candidates,
                "Dynamics",
                current_dynamics_vcs,
            ).await;

            let events_result = select_best_target(
                events_candidates,
                "Events",
                current_events_vcs,
            ).await;

            // Reacquire lock for updates
            let mut state_guard = state.write().await;

            let dynamics_change = check_target_change(
                dynamics_result,
                &state_guard.dynamics_path,
                "Dynamics",
                dynamics_count,
            );
            let events_change = check_target_change(
                events_result,
                &state_guard.events_path,
                "Events",
                events_count,
            );

            // Apply Dynamics change
            let dynamics_restart = match dynamics_change {
                TargetChange::Initial { path, vcs } => {
                    state_guard.highest_dynamics_vcs = vcs;
                    let decoded = urlencoding::decode(&path)
                        .unwrap_or_else(|_| path.clone().into());
                    println!(
                        "{} {} Dynamics target discovered: {}",
                        format!("[{}]", timestamp()).dimmed(),
                        "[CONN]".cyan().bold(),
                        decoded
                    );
                    state_guard.dynamics_path = Some(path);
                    None
                }
                TargetChange::Changed { old, new, vcs } => {
                    state_guard.highest_dynamics_vcs = vcs;
                    Some((old, new))
                }
                TargetChange::None { vcs } => {
                    if vcs > 0 { state_guard.highest_dynamics_vcs = vcs; }
                    None
                }
            };

            // Apply Events change
            let events_restart = match events_change {
                TargetChange::Initial { path, vcs } => {
                    state_guard.highest_events_vcs = vcs;
                    let decoded = urlencoding::decode(&path)
                        .unwrap_or_else(|_| path.clone().into());
                    println!(
                        "{} {} Events target discovered: {}",
                        format!("[{}]", timestamp()).dimmed(),
                        "[CONN]".cyan().bold(),
                        decoded
                    );
                    state_guard.events_path = Some(path);
                    None
                }
                TargetChange::Changed { old, new, vcs } => {
                    state_guard.highest_events_vcs = vcs;
                    Some((old, new))
                }
                TargetChange::None { vcs } => {
                    if vcs > 0 { state_guard.highest_events_vcs = vcs; }
                    None
                }
            };

            // Release lock before restarting
            drop(state_guard);

            // Restart servers sequentially
            match (dynamics_restart, events_restart) {
                (Some((old_dyn, new_dyn)), Some((old_evt, new_evt))) => {
                    // Both changed - restart sequentially
                    println!(
                        "{} {} Both targets changed - restarting sequentially",
                        format!("[{}]", timestamp()).dimmed(),
                        "[CHANGE]".blue().bold()
                    );
                    restart_server(state.clone(), "Dynamics", old_dyn, new_dyn).await;
                    restart_server(state.clone(), "Events", old_evt, new_evt).await;
                }
                (Some((old, new)), None) => {
                    // Only Dynamics changed
                    restart_server(state.clone(), "Dynamics", old, new).await;
                }
                (None, Some((old, new))) => {
                    // Only Events changed
                    restart_server(state.clone(), "Events", old, new).await;
                }
                (None, None) => {
                    // No changes
                }
            }

            log_verbose("--- End Target Update ---\n");
        }
        Err(e) => {
            let mut state_guard = state.write().await;
            state_guard.consecutive_failures += 1;
            let cfg = config();

            if state_guard.consecutive_failures == 1 {
                log_error(&format!(
                    "Cannot connect to WinCC at {}:{}",
                    cfg.target_host, cfg.target_port
                ));
                log_error(&format!("   Reason: {}", e));
                log(&format!(
                    "Will retry every {} seconds...",
                    cfg.poll_interval
                ));
                state_guard.target_available = false;
            } else if state_guard.consecutive_failures % 5 == 0 {
                log(&format!(
                    "Still cannot connect to WinCC ({} failed attempts, retrying every {}s)",
                    state_guard.consecutive_failures, cfg.poll_interval
                ));
            }

            log_verbose("--- End Target Update (failed) ---\n");
        }
    }
}

// ============================================================================
// HTTP Proxy
// ============================================================================

async fn handle_json_request(
    _state: SharedState,
    port: u16,
    filter_title: String,
) -> Result<impl warp::Reply, warp::Rejection> {
    let cfg = config();
    let url = format!("http://{}:{}/json", cfg.target_host, cfg.target_port);

    match reqwest::get(&url).await {
        Ok(response) => {
            if let Ok(targets) = response.json::<Vec<DebugTarget>>().await {
                let filtered: Vec<DebugTarget> = targets
                    .into_iter()
                    .filter(|t| t.title.contains(&filter_title))
                    .map(|mut t| {
                        t.web_socket_debugger_url = format!("ws://localhost:{}", port);
                        t
                    })
                    .collect();

                Ok(warp::reply::json(&filtered))
            } else {
                Ok(warp::reply::json(&Vec::<DebugTarget>::new()))
            }
        }
        Err(_) => {
            log_verbose(&format!("[HTTP Proxy] Target unavailable for /json"));
            Ok(warp::reply::json(&Vec::<DebugTarget>::new()))
        }
    }
}

async fn handle_version_request() -> Result<impl warp::Reply, warp::Rejection> {
    let cfg = config();
    let url = format!(
        "http://{}:{}/json/version",
        cfg.target_host, cfg.target_port
    );

    match reqwest::get(&url).await {
        Ok(response) => {
            if let Ok(text) = response.text().await {
                Ok(warp::reply::html(text))
            } else {
                let fallback = r#"{"Browser":"WinCC-Proxy/1.0","Protocol-Version":"1.3"}"#;
                Ok(warp::reply::html(fallback.to_string()))
            }
        }
        Err(_) => {
            log_verbose("[HTTP Proxy] Target unavailable for /json/version");
            let fallback = r#"{"Browser":"WinCC-Proxy/1.0","Protocol-Version":"1.3"}"#;
            Ok(warp::reply::html(fallback.to_string()))
        }
    }
}

// ============================================================================
// WebSocket Proxy
// ============================================================================

async fn handle_websocket(ws: warp::ws::WebSocket, state: SharedState, target_name: String) {
    let client_id = rand::random::<u32>();
    let target_name_log = target_name.clone();
    log_success(&format!(
        "[{}] Client #{} connected",
        target_name_log, client_id
    ));

    // Subscribe to shutdown signal
    let state_guard = state.read().await;
    let target_path = match target_name.as_str() {
        "Dynamics" => state_guard.dynamics_path.clone(),
        "Events" => state_guard.events_path.clone(),
        _ => None,
    };
    let mut shutdown_rx = match target_name.as_str() {
        "Dynamics" => state_guard
            .dynamics_clients_shutdown_tx
            .as_ref()
            .map(|tx| tx.subscribe()),
        "Events" => state_guard
            .events_clients_shutdown_tx
            .as_ref()
            .map(|tx| tx.subscribe()),
        _ => None,
    };
    drop(state_guard);

    if target_path.is_none() {
        log_error(&format!(
            "[{}] Client #{}: No target path available yet",
            target_name_log, client_id
        ));
        return;
    }

    let target_path_str = target_path.unwrap();
    let cfg = config();
    let target_url = format!(
        "ws://{}:{}/{}",
        cfg.target_host, cfg.target_port, target_path_str
    );

    // Decode path for readable logging
    let decoded_path = urlencoding::decode(&target_path_str)
        .unwrap_or_else(|_| target_path_str.clone().into())
        .into_owned();

    log(&format!(
        "[{}] Client #{}: Connecting to target: {}",
        target_name_log, client_id, decoded_path
    ));

    // Connect to WinCC target
    let (target_stream, _) = match tokio_tungstenite::connect_async(&target_url).await {
        Ok(result) => result,
        Err(e) => {
            log_error(&format!(
                "[{}] Client #{}: Failed to connect to target: {}",
                target_name_log, client_id, e
            ));
            return;
        }
    };

    println!(
        "{} {} [{}] Client #{}: Connected to target",
        format!("[{}]", timestamp()).dimmed(),
        "[CONN]".blue().bold(),
        target_name_log,
        client_id
    );

    let (mut client_tx, mut client_rx) = ws.split();
    let (target_tx, mut target_rx) = target_stream.split();
    let target_tx = Arc::new(tokio::sync::Mutex::new(target_tx));

    let dump_output = config().dump_output.clone();

    // Clone for each async block
    let target_name_c2t = target_name_log.clone();
    let target_name_t2c = target_name_log.clone();

    // Forward messages from client to target
    let target_tx_c2t = target_tx.clone();
    let mut client_to_target = tokio::spawn(async move {
        while let Some(Ok(msg)) = client_rx.next().await {
            if let Ok(text) = msg.to_str() {
                log_very_verbose(&format!(
                    "[{}] Client #{}: Client -> Target ({} bytes)",
                    target_name_c2t,
                    client_id,
                    text.len()
                ));

                let mut tx = target_tx_c2t.lock().await;
                if tx.send(Message::Text(text.to_string())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Forward messages from target to client (with CDP rewriting + script dump)
    let target_tx_t2c = target_tx.clone();
    let target_name_dump = target_name_log.clone();
    let mut target_to_client = tokio::spawn(async move {
        let mut dump_msg_id: u64 = 900_000;
        let mut pending_dumps: std::collections::HashMap<u64, String> =
            std::collections::HashMap::new();
        let mut dump_count: u64 = 0;

        while let Some(Ok(msg)) = target_rx.next().await {
            if let Message::Text(text) = msg {
                log_very_verbose(&format!(
                    "[{}] Client #{}: Target -> Client ({} bytes)",
                    target_name_t2c,
                    client_id,
                    text.len()
                ));

                // --- Script dump interception ---
                if let Some(ref dump_dir) = dump_output {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                        // Intercept scriptParsed → request source
                        if parsed.get("method").and_then(|m| m.as_str())
                            == Some("Debugger.scriptParsed")
                        {
                            if let Some(params) = parsed.get("params") {
                                let script_id = params
                                    .get("scriptId")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("");
                                let script_url = params
                                    .get("url")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("");
                                if !script_url.is_empty()
                                    && !(script_url.starts_with("eval-")
                                        && script_url.ends_with(".cdp"))
                                {
                                    let safe_url = script_url
                                        .replace(':', "_")
                                        .replace('*', "_")
                                        .replace('?', "_")
                                        .replace('"', "_")
                                        .replace('<', "_")
                                        .replace('>', "_")
                                        .replace('|', "_");
                                    let target_dir = if target_name_dump.contains("Dynamics") {
                                        "Dynamics"
                                    } else {
                                        "Events"
                                    };
                                    let file_path =
                                        format!("{}/{}/{}", dump_dir, target_dir, safe_url);

                                    let get_msg = serde_json::json!({
                                        "id": dump_msg_id,
                                        "method": "Debugger.getScriptSource",
                                        "params": { "scriptId": script_id }
                                    });
                                    let mut tx = target_tx_t2c.lock().await;
                                    let _ = tx.send(Message::Text(get_msg.to_string())).await;
                                    pending_dumps.insert(dump_msg_id, file_path);
                                    dump_msg_id += 1;
                                }
                            }
                        }

                        // Intercept our getScriptSource responses → write to disk, don't forward
                        if let Some(id) = parsed.get("id").and_then(|id| id.as_u64()) {
                            if let Some(file_path) = pending_dumps.remove(&id) {
                                if let Some(source) = parsed
                                    .get("result")
                                    .and_then(|r| r.get("scriptSource"))
                                    .and_then(|s| s.as_str())
                                {
                                    let path = std::path::Path::new(&file_path);
                                    if let Some(parent) = path.parent() {
                                        let _ = std::fs::create_dir_all(parent);
                                    }
                                    let _ = std::fs::write(path, source);
                                    dump_count += 1;
                                    log_verbose(&format!("[DUMP] {}", file_path));
                                }
                                continue; // Don't forward our response to VS Code
                            }
                        }
                    }
                }

                // --- Existing CDP rewriting ---
                let text = maybe_rewrite_cdp_message(&text);

                if client_tx.send(warp::ws::Message::text(text)).await.is_err() {
                    break;
                }
            }
        }

        if dump_count > 0 {
            log(&format!(
                "[{}] Client #{}: Dumped {} scripts",
                target_name_t2c, client_id, dump_count
            ));
        }
    });

    // Wait for either direction to close OR shutdown signal
    tokio::select! {
        _ = &mut client_to_target => {
            println!(
                "{} {} [{}] Client #{} disconnected (client closed)",
                format!("[{}]", timestamp()).dimmed(),
                "[DISC]".magenta().bold(),
                target_name_log,
                client_id
            );
            target_to_client.abort();
        },
        _ = &mut target_to_client => {
            println!(
                "{} {} [{}] Client #{} disconnected (target closed)",
                format!("[{}]", timestamp()).dimmed(),
                "[DISC]".magenta().bold(),
                target_name_log,
                client_id
            );
            client_to_target.abort();
        },
        _ = async {
            if let Some(rx) = &mut shutdown_rx {
                let _ = rx.recv().await;
            } else {
                futures_util::future::pending::<()>().await;
            }
        } => {
            println!(
                "{} {} [{}] Client #{}: Closing due to target change",
                format!("[{}]", timestamp()).dimmed(),
                "[STOP]".magenta().bold(),
                target_name_log,
                client_id
            );
            // Abort both forwarding tasks to force close the connections
            client_to_target.abort();
            target_to_client.abort();
        },
    }
}

fn create_http_server(
    state: SharedState,
    port: u16,
    target_name: String,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let state_filter = warp::any().map(move || state.clone());
    let target_filter = warp::any().map(move || target_name.clone());

    // /json endpoint
    let json_route = warp::path("json")
        .and(warp::path::end())
        .and(state_filter.clone())
        .and(warp::any().map(move || port))
        .and(target_filter.clone())
        .and_then(handle_json_request);

    // /json/list endpoint (same as /json)
    let json_list_route = warp::path!("json" / "list")
        .and(state_filter.clone())
        .and(warp::any().map(move || port))
        .and(target_filter.clone())
        .and_then(handle_json_request);

    // /json/version endpoint
    let version_route = warp::path!("json" / "version").and_then(handle_version_request);

    // WebSocket upgrade
    let ws_route = warp::path::end()
        .and(warp::ws())
        .and(state_filter)
        .and(target_filter)
        .map(|ws: warp::ws::Ws, state, name| {
            ws.on_upgrade(move |socket| handle_websocket(socket, state, name))
        });

    json_route
        .or(json_list_route)
        .or(version_route)
        .or(ws_route)
}

// ============================================================================
// Server Management
// ============================================================================

async fn start_dynamics_server(state: SharedState) {
    let cfg = config();
    let dynamics_port = cfg.dynamics_port;

    // Create broadcast channel for clients (capacity of 10 receivers)
    let (clients_shutdown_tx, _) = broadcast::channel(10);

    // Create oneshot channel for server graceful shutdown
    let (server_shutdown_tx, server_shutdown_rx) = tokio::sync::oneshot::channel();

    // Create oneshot channel to signal server is ready
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

    let dynamics_state = state.clone();
    let dynamics_server = create_http_server(dynamics_state, dynamics_port, "Dynamics".to_string());

    let server_handle = tokio::spawn(async move {
        let (_, server) = warp::serve(dynamics_server).bind_with_graceful_shutdown(
            ([127, 0, 0, 1], dynamics_port),
            async move {
                server_shutdown_rx.await.ok();
            },
        );

        // Signal that server is ready
        let _ = ready_tx.send(());

        server.await;
        log_success("Dynamics proxy server stopped");
    });

    // Store shutdown senders and server handle in state
    {
        let mut state_guard = state.write().await;
        state_guard.dynamics_clients_shutdown_tx = Some(clients_shutdown_tx);
        state_guard.dynamics_server_shutdown_tx = Some(server_shutdown_tx);
        state_guard.dynamics_server_handle = Some(server_handle);
    }

    // Wait for server to be ready before returning
    let _ = ready_rx.await;

    log_success(&format!("Dynamics proxy ready on port {}", dynamics_port));
}

async fn start_events_server(state: SharedState) {
    let cfg = config();
    let events_port = cfg.events_port;

    // Create broadcast channel for clients (capacity of 10 receivers)
    let (clients_shutdown_tx, _) = broadcast::channel(10);

    // Create oneshot channel for server graceful shutdown
    let (server_shutdown_tx, server_shutdown_rx) = tokio::sync::oneshot::channel();

    // Create oneshot channel to signal server is ready
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

    let events_state = state.clone();
    let events_server = create_http_server(events_state, events_port, "Events".to_string());

    let server_handle = tokio::spawn(async move {
        let (_, server) = warp::serve(events_server).bind_with_graceful_shutdown(
            ([127, 0, 0, 1], events_port),
            async move {
                server_shutdown_rx.await.ok();
            },
        );

        // Signal that server is ready
        let _ = ready_tx.send(());

        server.await;
        log_success("Events proxy server stopped");
    });

    // Store shutdown senders and server handle in state
    {
        let mut state_guard = state.write().await;
        state_guard.events_clients_shutdown_tx = Some(clients_shutdown_tx);
        state_guard.events_server_shutdown_tx = Some(server_shutdown_tx);
        state_guard.events_server_handle = Some(server_handle);
    }

    // Wait for server to be ready before returning
    let _ = ready_rx.await;

    log_success(&format!("Events proxy ready on port {}", events_port));
}

// ============================================================================
// Public Entry Point
// ============================================================================

fn clean_dump_scripts(dump_dir: &str) {
    for subdir in ["Dynamics", "Events"] {
        let path = std::path::Path::new(dump_dir).join(subdir);
        if path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                log_warn(&format!("Could not clean {}: {}", path.display(), e));
            } else {
                log(&format!("Cleaned {}/", path.display()));
            }
        }
    }
}

pub async fn run_proxy() {
    let cfg = config();

    println!(
        "{} {} Starting WinCC Debug Proxy...",
        format!("[{}]", timestamp()).dimmed(),
        "[START]".cyan().bold()
    );

    let state = Arc::new(RwLock::new(AppState::new()));

    // Start servers
    start_dynamics_server(state.clone()).await;
    start_events_server(state.clone()).await;

    println!(
        "{} {} WinCC Debug Proxy is running!",
        format!("[{}]", timestamp()).dimmed(),
        "[READY]".green().bold()
    );
    println!();
    println!("{}", "Configuration:".cyan().bold());
    println!(
        "   Target:        {}:{}",
        cfg.target_host, cfg.target_port
    );
    println!("   Dynamics:      localhost:{}", cfg.dynamics_port);
    println!("   Events:        localhost:{}", cfg.events_port);
    println!("   Poll interval: {}s", cfg.poll_interval);
    println!();
    println!("{}", "VS Code launch.json ports:".cyan().bold());
    println!("   Dynamics: {}", cfg.dynamics_port);
    println!("   Events:   {}", cfg.events_port);
    println!();
    println!("{}", "Features:".cyan().bold());
    println!("   {} Server restarts when targets change", "[+]".green());
    println!("   {} Forces VS Code debugger reconnect", "[+]".green());
    println!("   {} No manual intervention needed!", "[+]".green());
    println!("   {} Separate debug sessions for Dynamics & Events", "[+]".green());
    println!("   {} Script path shortening: {}", "[+]".green(),
        if cfg.long_paths { "off (showing full paths)" } else { "on" });
    if let Some(ref dump_dir) = cfg.dump_output {
        // Clean old scripts at startup
        clean_dump_scripts(dump_dir);

        println!("   {} Continuous script dump -> {}/", "[+]".green(), dump_dir);

        // Write styleguide files into the dump directory + npm install
        if let Some(ref version) = cfg.styleguide_version {
            match crate::styleguide::write_styleguide(version, dump_dir) {
                Ok(_) => {
                    println!("   {} Styleguide ({}) written to {}/", "[+]".green(), version, dump_dir);

                    // Run npm install in the dump directory
                    log("Running npm install...");
                    match std::process::Command::new("cmd")
                        .args(["/C", "npm", "install"])
                        .current_dir(dump_dir)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::piped())
                        .status()
                    {
                        Ok(status) if status.success() => {
                            log_success("npm install completed — ESLint ready");
                        }
                        Ok(_) => {
                            log_warn("npm install failed — run it manually in the dump directory");
                        }
                        Err(e) => {
                            log_warn(&format!("Could not run npm install: {}", e));
                        }
                    }
                }
                Err(e) => {
                    log_error(&format!("Failed to write styleguide: {}", e));
                }
            }
        }
    }
    println!();
    println!("Press {} to stop", "Ctrl+C".yellow().bold());
    println!();

    // Wait for target to be reachable before fetching /json
    wait_for_target_connectivity().await;

    // Initial target fetch (after startup messages)
    update_targets(state.clone()).await;

    // Start target polling
    let poll_state = state.clone();
    let poll_interval = cfg.poll_interval;
    tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(Duration::from_secs(poll_interval));

        loop {
            interval_timer.tick().await;
            update_targets(poll_state.clone()).await;
        }
    });

    // Keep running forever
    tokio::signal::ctrl_c().await.unwrap();
    println!(
        "{} {} Shutting down...",
        format!("[{}]", timestamp()).dimmed(),
        "[STOP]".magenta().bold()
    );
}
