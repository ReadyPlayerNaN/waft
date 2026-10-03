//! EDS source key-file parsing and ancestor-aware calendar eligibility.
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use zbus::{Connection, Proxy};
use zvariant::{OwnedObjectPath, OwnedValue};
pub const SOURCES_DEST: &str = "org.gnome.evolution.dataserver.Sources5";
pub const SOURCES_PATH: &str = "/org/gnome/evolution/dataserver/SourceManager";
pub const SOURCE_IFACE: &str = "org.gnome.evolution.dataserver.Source";
pub const FACTORY_DEST: &str = "org.gnome.evolution.dataserver.Calendar8";
pub const FACTORY_PATH: &str = "/org/gnome/evolution/dataserver/CalendarFactory";
type Objects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarSource {
    pub uid: String,
    pub path: String,
    pub display_name: String,
    pub parent: String,
    pub enabled: bool,
    pub calendar: bool,
    pub calendar_enabled: bool,
    pub goa_account_id: Option<String>,
    pub data: String,
    pub valid: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eligibility {
    Enabled,
    Disabled,
    Pending,
    Invalid,
}

pub fn parse_source(uid: String, path: String, data: String) -> CalendarSource {
    let mut source = CalendarSource {
        display_name: uid.clone(),
        uid,
        path,
        parent: String::new(),
        enabled: true,
        calendar: false,
        calendar_enabled: true,
        goa_account_id: None,
        data,
        valid: true,
    };
    let mut section = "";
    let mut base_section = false;
    for line in source.data.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            base_section |= section == "Data Source";
            source.calendar |= section == "Calendar";
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            source.valid = false;
            continue;
        };
        let boolean = |v: &str| match v.trim() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        };
        match (section, key.trim()) {
            ("Data Source", "DisplayName") => source.display_name = unescape(value),
            ("Data Source", "Parent") => source.parent = unescape(value),
            ("Data Source", "Enabled") => {
                if let Some(value) = boolean(value) {
                    source.enabled = value;
                } else {
                    source.valid = false;
                }
            }
            ("Collection", "CalendarEnabled") => {
                if let Some(value) = boolean(value) {
                    source.calendar_enabled = value;
                } else {
                    source.valid = false;
                }
            }
            ("GNOME Online Accounts", "AccountId") => {
                source.goa_account_id = Some(unescape(value)).filter(|v| !v.is_empty());
            }
            _ => {}
        }
    }
    source.valid &= base_section;
    source
}
fn unescape(value: &str) -> String {
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('s') => result.push(' '),
                Some(ch) => result.push(ch),
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }
    result
}
pub fn eligibility(uid: &str, sources: &HashMap<String, CalendarSource>) -> Eligibility {
    let mut current = uid;
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current) {
            return Eligibility::Invalid;
        }
        let Some(source) = sources.get(current) else {
            return Eligibility::Pending;
        };
        if !source.valid {
            return Eligibility::Invalid;
        }
        if !source.enabled || !source.calendar_enabled {
            return Eligibility::Disabled;
        }
        if source.parent.is_empty() {
            return Eligibility::Enabled;
        }
        current = &source.parent;
    }
}
pub fn goa_account(uid: &str, sources: &HashMap<String, CalendarSource>) -> Option<String> {
    let mut current = uid;
    let mut seen = HashSet::new();
    while seen.insert(current) {
        let source = sources.get(current)?;
        if source.goa_account_id.is_some() {
            return source.goa_account_id.clone();
        }
        if source.parent.is_empty() {
            break;
        }
        current = &source.parent;
    }
    None
}
pub async fn discover(conn: &Connection, owner: &str) -> Result<HashMap<String, CalendarSource>> {
    let proxy = Proxy::new(
        conn,
        owner,
        SOURCES_PATH,
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;
    let objects: Objects = proxy.call("GetManagedObjects", &()).await?;
    let mut sources = HashMap::new();
    for (path, interfaces) in objects {
        let Some(properties) = interfaces.get(SOURCE_IFACE) else {
            continue;
        };
        let uid = properties
            .get("UID")
            .and_then(|v| String::try_from(v.clone()).ok())
            .context("source missing UID")?;
        let data = properties
            .get("Data")
            .and_then(|v| String::try_from(v.clone()).ok())
            .context("source missing Data")?;
        anyhow::ensure!(!uid.is_empty(), "empty source UID");
        anyhow::ensure!(!sources.contains_key(&uid), "duplicate source UID");
        sources.insert(uid.clone(), parse_source(uid, path.to_string(), data));
    }
    Ok(sources)
}
/// Removal payloads contain an optional recurrence ID, not a different UID or source.
pub fn decode_removed_identifier(value: &str) -> Option<(&str, Option<&str>)> {
    let (uid, rid) = value.split_once('\n').map_or((value, None), |(uid, rid)| {
        (uid, (!rid.is_empty()).then_some(rid))
    });
    if uid.is_empty() || rid.is_some_and(|rid| rid.contains('\n')) {
        return None;
    }
    Some((uid, rid))
}

/// Escape separators too: UID pairs cannot collide with one another.
pub fn encode_identifier(value: &str) -> String {
    use std::fmt::Write;
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            result.push(char::from(byte));
        } else {
            write!(result, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(uid: &str, data: &str) -> CalendarSource {
        parse_source(uid.into(), format!("/fixture/{uid}"), data.into())
    }
    #[test]
    fn inherited_enablement_missing_parent_and_cycles() {
        let child = source(
            "child",
            include_str!("../tests/fixtures/enabled-child.source"),
        );
        let mut sources = HashMap::from([("child".into(), child)]);
        assert_eq!(eligibility("child", &sources), Eligibility::Pending);
        sources.insert(
            "parent".into(),
            source(
                "parent",
                include_str!("../tests/fixtures/disabled-collection.source"),
            ),
        );
        assert_eq!(eligibility("child", &sources), Eligibility::Disabled);
        sources.insert(
            "parent".into(),
            source("parent", "[Data Source]\nParent=child\n"),
        );
        assert_eq!(eligibility("child", &sources), Eligibility::Invalid);
        sources.insert(
            "parent".into(),
            source(
                "parent",
                "[Data Source]\n[GNOME Online Accounts]\nAccountId=fixture\n",
            ),
        );
        assert_eq!(eligibility("child", &sources), Eligibility::Enabled);
        assert_eq!(goa_account("child", &sources).as_deref(), Some("fixture"));
    }
    #[test]
    fn calendar_section_and_invalid_boolean_are_not_guessed() {
        assert!(!source("fixture", "[Data Source]\nDisplayName=[Calendar]\n").calendar);
        assert!(!source("fixture", "[Data Source]\nEnabled=perhaps\n[Calendar]\n").valid);
    }
    #[test]
    fn identifiers_are_source_qualified_and_injective() {
        assert_eq!(
            decode_removed_identifier("uid@host"),
            Some(("uid@host", None))
        );
        assert_eq!(
            decode_removed_identifier("uid@host\n20260216T100000Z"),
            Some(("uid@host", Some("20260216T100000Z")))
        );
        assert!(decode_removed_identifier("\ninvalid").is_none());
        assert_ne!(encode_identifier("a::b"), encode_identifier("a%3A%3Ab"));
        assert_eq!(encode_identifier("x/y@é"), "x%2Fy%40%C3%A9");
    }
}
