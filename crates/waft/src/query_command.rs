use std::time::Duration;

use serde::Serialize;
use tokio::net::UnixStream;
use waft_protocol::entity::registry;
use waft_protocol::message::{AppMessage, AppNotification};
use waft_protocol::urn::Urn;

use crate::socket_io::{connect_daemon, read_message, send_message};

/// A collected entity from the daemon.
#[derive(Debug, Clone, Serialize)]
struct CollectedEntity {
    urn: Urn,
    entity_type: String,
    data: serde_json::Value,
}

/// Entry point for `waft query`.
pub fn run(json: bool, entity_type: Option<&str>, start: bool, timeout_ms: u64) {
    // Reject --start without entity_type
    if start && entity_type.is_none() {
        eprintln!("--start requires an entity type argument.");
        eprintln!("Usage: waft query <entity-type> --start");
        std::process::exit(1);
    }

    // Validate entity_type against static registry
    if let Some(et) = entity_type {
        let all = registry::all_entity_types();
        if !all.iter().any(|info| info.entity_type == et) {
            eprintln!("Unknown entity type: '{et}'. Run `waft protocol` to see all available types.");
            std::process::exit(1);
        }
    }

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    let result = rt.block_on(async { query_daemon(entity_type, start, timeout_ms).await });

    let entities = match result {
        Ok(entities) => entities,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    if entities.is_empty() {
        if json {
            println!("[]");
        } else {
            match entity_type {
                Some(et) => eprintln!("No entities of type '{et}' found."),
                None => eprintln!("No entities found."),
            }
        }
        return;
    }

    if json {
        print_json(&entities);
    } else {
        print_text(&entities);
    }
}

/// Connect to the daemon and collect entities.
async fn query_daemon(
    entity_type: Option<&str>,
    start: bool,
    timeout_ms: u64,
) -> Result<Vec<CollectedEntity>, String> {
    let mut stream = connect_daemon().await?;

    let mut entities = Vec::new();

    if let Some(et) = entity_type {
        if start {
            // Subscribe first to trigger plugin spawning
            send_message(
                &mut stream,
                &AppMessage::Subscribe {
                    entity_type: et.to_string(),
                },
            )
            .await
            .map_err(|e| format!("Failed to send Subscribe: {e}"))?;

            // Wait for EntityUpdated notifications with timeout
            let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
            loop {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }

                match tokio::time::timeout(remaining, read_message(&mut stream)).await {
                    Ok(Ok(Some(notification))) => {
                        collect_notification(&mut entities, notification);
                    }
                    Ok(Ok(None)) => break, // clean disconnect
                    Ok(Err(e)) => {
                        return Err(format!("Failed to read from daemon: {e}"));
                    }
                    Err(_) => break, // timeout
                }
            }

            // Send Status to also get any cached entities we might have missed
            send_message(
                &mut stream,
                &AppMessage::Status {
                    entity_type: et.to_string(),
                },
            )
            .await
            .map_err(|e| format!("Failed to send Status: {e}"))?;

            // Collect Status responses with a short read timeout
            collect_responses(&mut stream, &mut entities).await?;

            // Unsubscribe before disconnecting
            send_message(
                &mut stream,
                &AppMessage::Unsubscribe {
                    entity_type: et.to_string(),
                },
            )
            .await
            .map_err(|e| format!("Failed to send Unsubscribe: {e}"))?;
        } else {
            // Just query cached state
            send_message(
                &mut stream,
                &AppMessage::Status {
                    entity_type: et.to_string(),
                },
            )
            .await
            .map_err(|e| format!("Failed to send Status: {e}"))?;

            collect_responses(&mut stream, &mut entities).await?;
        }
    } else {
        // Query all entity types from the static registry
        let all = registry::all_entity_types();
        for info in all {
            send_message(
                &mut stream,
                &AppMessage::Status {
                    entity_type: info.entity_type.to_string(),
                },
            )
            .await
            .map_err(|e| format!("Failed to send Status: {e}"))?;
        }

        collect_responses(&mut stream, &mut entities).await?;
    }

    // Deduplicate by URN (--start may produce duplicates from subscription + status)
    dedup_by_urn(&mut entities);

    Ok(entities)
}

/// Read responses from the daemon with a short read timeout (500ms after last message).
async fn collect_responses(
    stream: &mut UnixStream,
    entities: &mut Vec<CollectedEntity>,
) -> Result<(), String> {
    let read_timeout = Duration::from_millis(500);
    loop {
        match tokio::time::timeout(read_timeout, read_message(stream)).await {
            Ok(Ok(Some(notification))) => {
                collect_notification(entities, notification);
            }
            Ok(Ok(None)) => break,    // clean disconnect
            Ok(Err(e)) => return Err(format!("Failed to read from daemon: {e}")),
            Err(_) => break,           // timeout — no more messages
        }
    }
    Ok(())
}

/// Extract EntityUpdated from a notification and add to the collection.
fn collect_notification(entities: &mut Vec<CollectedEntity>, notification: AppNotification) {
    if let AppNotification::EntityUpdated {
        urn,
        entity_type,
        data,
    } = notification
    {
        entities.push(CollectedEntity {
            urn,
            entity_type,
            data,
        });
    }
    // Ignore other notification types (ActionSuccess, ActionError, EntityStale, etc.)
}

/// Deduplicate entities by URN, keeping the last occurrence (most recent data).
fn dedup_by_urn(entities: &mut Vec<CollectedEntity>) {
    let mut seen = std::collections::HashSet::new();
    // Iterate in reverse so we keep the last occurrence
    entities.reverse();
    entities.retain(|e| seen.insert(e.urn.to_string()));
    entities.reverse();
}

/// Print entities as pretty-printed JSON array.
fn print_json(entities: &[CollectedEntity]) {
    match serde_json::to_string_pretty(entities) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("Failed to serialize entities: {e}");
            std::process::exit(1);
        }
    }
}

/// Print entities in human-readable text format, grouped by entity type.
fn print_text(entities: &[CollectedEntity]) {
    let mut current_type = "";
    for entity in entities {
        if entity.entity_type != current_type {
            if !current_type.is_empty() {
                println!();
            }
            println!("{}", entity.entity_type);
            current_type = &entity.entity_type;
        }
        println!("  {}", entity.urn);
        if let Some(obj) = entity.data.as_object() {
            let max_key_len = obj.keys().map(|k| k.len()).max().unwrap_or(0);
            for (key, value) in obj {
                println!(
                    "    {:<width$}  {}",
                    key,
                    format_value(value),
                    width = max_key_len,
                );
            }
        }
    }
}

/// Format a JSON value for human-readable display.
fn format_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        // For arrays and objects, use compact JSON
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entity(urn_str: &str, entity_type: &str, data: serde_json::Value) -> CollectedEntity {
        CollectedEntity {
            urn: Urn::new(
                urn_str.split('/').next().unwrap(),
                entity_type,
                urn_str.split('/').nth(2).unwrap_or("default"),
            ),
            entity_type: entity_type.to_string(),
            data,
        }
    }

    #[test]
    fn text_format_single_entity() {
        let entities = vec![make_entity(
            "clock/clock/default",
            "clock",
            serde_json::json!({"time": "14:30", "date": "Thursday"}),
        )];
        // Verify no panic; output goes to stdout
        print_text(&entities);
    }

    #[test]
    fn text_format_multiple_types() {
        let entities = vec![
            make_entity(
                "clock/clock/default",
                "clock",
                serde_json::json!({"time": "14:30"}),
            ),
            make_entity(
                "battery/battery/BAT0",
                "battery",
                serde_json::json!({"percentage": 85.0, "state": "Discharging"}),
            ),
        ];
        print_text(&entities);
    }

    #[test]
    fn json_format_output() {
        let entities = vec![make_entity(
            "clock/clock/default",
            "clock",
            serde_json::json!({"time": "14:30", "date": "Thursday"}),
        )];
        let json_str = serde_json::to_string_pretty(&entities).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["entity_type"], "clock");
        // Urn serializes with its internal structure
        assert!(!arr[0]["urn"].is_null(), "urn should be present");
        assert!(arr[0]["data"].is_object());
        assert_eq!(arr[0]["data"]["time"], "14:30");
    }

    #[test]
    fn json_format_empty() {
        let entities: Vec<CollectedEntity> = vec![];
        let json_str = serde_json::to_string_pretty(&entities).unwrap();
        assert_eq!(json_str, "[]");
    }

    #[test]
    fn format_value_string() {
        assert_eq!(format_value(&serde_json::json!("hello")), "hello");
    }

    #[test]
    fn format_value_number() {
        assert_eq!(format_value(&serde_json::json!(42)), "42");
    }

    #[test]
    fn format_value_bool() {
        assert_eq!(format_value(&serde_json::json!(true)), "true");
    }

    #[test]
    fn format_value_null() {
        assert_eq!(format_value(&serde_json::json!(null)), "null");
    }

    #[test]
    fn format_value_array() {
        let val = serde_json::json!([1, 2, 3]);
        assert_eq!(format_value(&val), "[1,2,3]");
    }

    #[test]
    fn dedup_keeps_last_occurrence() {
        let mut entities = vec![
            make_entity("clock/clock/default", "clock", serde_json::json!({"time": "14:30"})),
            make_entity("clock/clock/default", "clock", serde_json::json!({"time": "14:31"})),
        ];
        dedup_by_urn(&mut entities);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].data["time"], "14:31");
    }

    #[test]
    fn dedup_preserves_different_urns() {
        let mut entities = vec![
            make_entity("clock/clock/default", "clock", serde_json::json!({"time": "14:30"})),
            make_entity("battery/battery/BAT0", "battery", serde_json::json!({"percentage": 85})),
        ];
        dedup_by_urn(&mut entities);
        assert_eq!(entities.len(), 2);
    }

    #[test]
    fn collect_notification_entity_updated() {
        let mut entities = Vec::new();
        let notification = AppNotification::EntityUpdated {
            urn: Urn::new("clock", "clock", "default"),
            entity_type: "clock".to_string(),
            data: serde_json::json!({"time": "14:30"}),
        };
        collect_notification(&mut entities, notification);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, "clock");
    }

    #[test]
    fn collect_notification_ignores_other_types() {
        let mut entities = Vec::new();
        let notification = AppNotification::ActionSuccess {
            action_id: uuid::Uuid::new_v4(),
            data: None,
        };
        collect_notification(&mut entities, notification);
        assert!(entities.is_empty());
    }
}
