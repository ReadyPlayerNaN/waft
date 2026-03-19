//! Implementation of `waft commands` — list and run command palette actions.

use std::collections::HashMap;
use std::time::Duration;

use serde::Serialize;
use waft_protocol::commands::{COMMAND_DEFS, command_entity_types};
use waft_protocol::message::{AppMessage, AppNotification};
use waft_protocol::urn::Urn;

use crate::socket_io::{connect_daemon, read_message, send_message};

/// A resolved command ready for display or execution.
#[derive(Debug, Clone, Serialize)]
struct ResolvedCommand {
    label: String,
    subtitle: Option<String>,
    urn: Urn,
    action: String,
    icon: String,
}

/// Entry point for `waft commands`.
pub fn run(json: bool, filter: Option<&str>, run: bool) {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    let result = rt.block_on(async { run_commands(filter, run).await });

    match result {
        Ok(commands) => {
            if run {
                // run mode: result is empty vec on success, error on failure
                return;
            }
            if commands.is_empty() {
                if json {
                    println!("[]");
                } else {
                    match filter {
                        Some(f) => eprintln!("No commands matching '{f}'."),
                        None => eprintln!("No commands available."),
                    }
                }
                return;
            }
            if json {
                print_json(&commands);
            } else {
                print_text(&commands);
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// Connect to daemon, subscribe to command entity types, collect entities, resolve commands.
async fn run_commands(
    filter: Option<&str>,
    run: bool,
) -> Result<Vec<ResolvedCommand>, String> {
    let mut stream = connect_daemon().await?;

    // Subscribe to all command entity types to trigger plugin spawning
    let entity_types = command_entity_types();
    for &et in entity_types {
        send_message(
            &mut stream,
            &AppMessage::Subscribe {
                entity_type: et.to_string(),
            },
        )
        .await
        .map_err(|e| format!("Failed to send Subscribe: {e}"))?;
    }

    // Wait for entity updates with timeout
    let deadline = tokio::time::Instant::now() + Duration::from_millis(3000);
    let mut entity_map: HashMap<String, Vec<(Urn, serde_json::Value)>> = HashMap::new();

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        match tokio::time::timeout(remaining, read_message(&mut stream)).await {
            Ok(Ok(Some(notification))) => {
                if let AppNotification::EntityUpdated {
                    urn,
                    entity_type,
                    data,
                } = notification
                {
                    entity_map
                        .entry(entity_type)
                        .or_default()
                        .push((urn, data));
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(format!("Failed to read from daemon: {e}")),
            Err(_) => break,
        }
    }

    // Also request cached Status for each type
    for &et in entity_types {
        send_message(
            &mut stream,
            &AppMessage::Status {
                entity_type: et.to_string(),
            },
        )
        .await
        .map_err(|e| format!("Failed to send Status: {e}"))?;
    }

    // Collect status responses
    let read_timeout = Duration::from_millis(500);
    loop {
        match tokio::time::timeout(read_timeout, read_message(&mut stream)).await {
            Ok(Ok(Some(notification))) => {
                if let AppNotification::EntityUpdated {
                    urn,
                    entity_type,
                    data,
                } = notification
                {
                    entity_map
                        .entry(entity_type)
                        .or_default()
                        .push((urn, data));
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(format!("Failed to read from daemon: {e}")),
            Err(_) => break,
        }
    }

    // Unsubscribe
    for &et in entity_types {
        let _ = send_message(
            &mut stream,
            &AppMessage::Unsubscribe {
                entity_type: et.to_string(),
            },
        )
        .await;
    }

    // Deduplicate entities by URN (keep last)
    for entities in entity_map.values_mut() {
        let mut seen = std::collections::HashSet::new();
        entities.reverse();
        entities.retain(|(urn, _)| seen.insert(urn.to_string()));
        entities.reverse();
    }

    // Build command list from definitions + collected entities
    let mut commands = Vec::new();
    for def in COMMAND_DEFS {
        let Some(entities) = entity_map.get(def.entity_type) else {
            continue;
        };

        for (urn, data) in entities {
            let subtitle = (def.subtitle_fn)(data);

            let label = if entities.len() > 1 {
                match subtitle.as_deref() {
                    Some(name) => format!("{} {}", def.label, name),
                    None => def.label.to_string(),
                }
            } else {
                def.label.to_string()
            };

            commands.push(ResolvedCommand {
                label,
                subtitle,
                urn: urn.clone(),
                action: def.action.to_string(),
                icon: def.icon.to_string(),
            });
        }
    }

    // Filter by label if requested
    if let Some(filter) = filter {
        let filter_lower = filter.to_lowercase();
        commands.retain(|cmd| cmd.label.to_lowercase().contains(&filter_lower));
    }

    // If --run, execute the best match
    if run {
        if commands.is_empty() {
            return Err(match filter {
                Some(f) => format!("No commands matching '{f}'."),
                None => "No commands available.".to_string(),
            });
        }

        let best = &commands[0];
        let action_id = uuid::Uuid::new_v4();

        send_message(
            &mut stream,
            &AppMessage::TriggerAction {
                urn: best.urn.clone(),
                action: best.action.clone(),
                action_id,
                params: serde_json::Value::Null,
                timeout_ms: None,
            },
        )
        .await
        .map_err(|e| format!("Failed to send TriggerAction: {e}"))?;

        // Wait for action result
        let action_timeout = Duration::from_millis(5000);
        match tokio::time::timeout(action_timeout, wait_for_action(&mut stream, action_id)).await {
            Ok(Ok(())) => {
                eprintln!("Executed: {} → {} → {}", best.label, best.urn, best.action);
            }
            Ok(Err(e)) => return Err(format!("Action failed: {e}")),
            Err(_) => {
                eprintln!(
                    "Executed: {} → {} → {} (no confirmation within timeout)",
                    best.label, best.urn, best.action
                );
            }
        }

        return Ok(Vec::new());
    }

    Ok(commands)
}

/// Wait for ActionSuccess or ActionError for a specific action_id.
async fn wait_for_action(
    stream: &mut tokio::net::UnixStream,
    action_id: uuid::Uuid,
) -> Result<(), String> {
    loop {
        match read_message(stream).await {
            Ok(Some(AppNotification::ActionSuccess { action_id: id, .. })) if id == action_id => {
                return Ok(());
            }
            Ok(Some(AppNotification::ActionError {
                action_id: id,
                error,
            })) if id == action_id => {
                return Err(error);
            }
            Ok(Some(_)) => continue,
            Ok(None) => return Err("daemon disconnected".to_string()),
            Err(e) => return Err(format!("read error: {e}")),
        }
    }
}

fn print_json(commands: &[ResolvedCommand]) {
    match serde_json::to_string_pretty(commands) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("Failed to serialize commands: {e}");
            std::process::exit(1);
        }
    }
}

fn print_text(commands: &[ResolvedCommand]) {
    let max_label = commands.iter().map(|c| c.label.len()).max().unwrap_or(0);
    let max_subtitle = commands
        .iter()
        .map(|c| c.subtitle.as_deref().map_or(0, |s| s.len()))
        .max()
        .unwrap_or(0);

    for cmd in commands {
        let subtitle_str = cmd.subtitle.as_deref().unwrap_or("");
        println!(
            "{:<label_w$}  {:<sub_w$}  {} → {}",
            cmd.label,
            subtitle_str,
            cmd.urn,
            cmd.action,
            label_w = max_label,
            sub_w = max_subtitle,
        );
    }
}
