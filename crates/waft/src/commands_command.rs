//! Implementation of `waft commands` — list and run command palette actions.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use waft_protocol::commands::{ResolvedCommand, command_entity_types, resolve_commands};
use waft_protocol::message::{AppMessage, AppNotification};
use waft_protocol::urn::Urn;

use crate::socket_io::{connect_daemon, read_message, send_message};

/// Entry point for `waft commands`.
pub fn run(json: bool, filter: Option<&str>, run: bool, refresh: bool) {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    let result = rt.block_on(async { run_commands(filter, run, refresh).await });

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
    refresh: bool,
) -> Result<Vec<ResolvedCommand>, String> {
    let mut stream = connect_daemon().await?;
    let entity_types = command_entity_types();

    if refresh {
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

        let deadline = tokio::time::Instant::now() + Duration::from_millis(3000);
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, read_message(&mut stream)).await {
                Ok(Ok(Some(_))) => {}
                Ok(Ok(None)) => break,
                Ok(Err(e)) => return Err(format!("Failed to read from daemon: {e}")),
                Err(_) => break,
            }
        }
    }

    let entity_map = collect_status_snapshot(&mut stream, entity_types).await?;

    if refresh {
        for &et in entity_types {
            let _ = send_message(
                &mut stream,
                &AppMessage::Unsubscribe {
                    entity_type: et.to_string(),
                },
            )
            .await;
        }
    }

    let mut commands = resolve_commands(&entity_map);

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

        // Explicitly activate the owning plugin before TriggerAction. The daemon
        // does not auto-spawn plugins on actions, so command execution must do it.
        send_message(
            &mut stream,
            &AppMessage::Subscribe {
                entity_type: best.entity_type.to_string(),
            },
        )
        .await
        .map_err(|e| format!("Failed to send Subscribe: {e}"))?;
        wait_for_live_activation(&mut stream, best.entity_type, Duration::from_millis(3000)).await?;
        let _ = collect_status_snapshot(&mut stream, &[best.entity_type]).await?;

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

        let _ = send_message(
            &mut stream,
            &AppMessage::Unsubscribe {
                entity_type: best.entity_type.to_string(),
            },
        )
        .await;

        return Ok(Vec::new());
    }

    Ok(commands)
}

async fn wait_for_live_activation(
    stream: &mut tokio::net::UnixStream,
    entity_type: &str,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, read_message(stream)).await {
            Ok(Ok(Some(notification @ AppNotification::EntityUpdated { .. })))
                if notification.entity_type() == Some(entity_type) =>
            {
                return Ok(());
            }
            Ok(Ok(Some(_))) => continue,
            Ok(Ok(None)) => break,
            Ok(Err(e)) => return Err(format!("Failed to read from daemon: {e}")),
            Err(_) => break,
        }
    }
    Ok(())
}

async fn collect_status_snapshot(
    stream: &mut tokio::net::UnixStream,
    entity_types: &[&str],
) -> Result<HashMap<String, Vec<(Urn, serde_json::Value)>>, String> {
    for &et in entity_types {
        send_message(
            stream,
            &AppMessage::Status {
                entity_type: et.to_string(),
            },
        )
        .await
        .map_err(|e| format!("Failed to send Status: {e}"))?;
    }

    let mut entity_map: HashMap<String, Vec<(Urn, serde_json::Value)>> = HashMap::new();
    let mut pending: HashSet<String> = entity_types.iter().map(|s| (*s).to_string()).collect();

    while !pending.is_empty() {
        match read_message(stream).await {
            Ok(Some(notification @ AppNotification::EntityUpdated { .. })) => {
                let entity_type = notification
                    .entity_type()
                    .ok_or_else(|| "EntityUpdated missing entity type".to_string())?
                    .to_string();
                if let AppNotification::EntityUpdated { urn, data, .. } = notification {
                    entity_map.entry(entity_type).or_default().push((urn, data));
                }
            }
            Ok(Some(AppNotification::StatusComplete { entity_type })) => {
                pending.remove(&entity_type);
            }
            Ok(Some(_)) => continue,
            Ok(None) => return Err("daemon disconnected".to_string()),
            Err(e) => return Err(format!("Failed to read from daemon: {e}")),
        }
    }

    for entities in entity_map.values_mut() {
        let mut seen = HashSet::new();
        entities.reverse();
        entities.retain(|(urn, _)| seen.insert(urn.to_string()));
        entities.reverse();
    }

    Ok(entity_map)
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
                ..
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
        .map(|c| c.subtitle.as_deref().map_or(0, str::len))
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
