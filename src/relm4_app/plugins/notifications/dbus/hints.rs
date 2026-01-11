use anyhow::Result;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use zvariant::{Array, OwnedValue, Value};

use super::super::types::{NotificationCategory, NotificationUrgency};

/// A simplified representation of a freedesktop notification "hint" value.
///
/// The DBus spec uses a `dict<string, variant>`. In this codebase we avoid exposing
/// raw DBus `Variant` values outside the DBus layer. The DBus server should decode
/// variants into these supported types when possible.
///
/// Unknown or unhandled variants should be ignored by the DBus server rather than
/// leaking DBus-specific types across module boundaries.
#[derive(Clone, Debug)]
pub enum HintValue {
    Bool(bool),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
    Bytes(Vec<u8>),
}

fn decode_bytes_array(a: Array<'_>) -> Option<HintValue> {
    // Only accept `ay` (array of u8).
    let mut bytes: Vec<u8> = Vec::new();
    for item in a.iter() {
        match item {
            Value::U8(b) => bytes.push(*b),
            _ => return None,
        }
    }
    if bytes.is_empty() {
        None
    } else {
        Some(HintValue::Bytes(bytes))
    }
}

fn decode_hint_value(v: &OwnedValue) -> Option<HintValue> {
    // Conservative subset of types commonly used in notification hints:
    // - bool, i32/u32, i64/u64, f64, string, bytes (ay).
    //
    // In zvariant v5, `OwnedValue::downcast_ref::<T>()` returns `Result<T, _>`.
    let val: Value<'_> = match v.downcast_ref::<Value>() {
        Ok(r) => r,
        Err(_) => return None,
    };

    match val {
        Value::Bool(b) => Some(HintValue::Bool(b)),
        Value::I16(i) => Some(HintValue::I32(i as i32)),
        Value::I32(i) => Some(HintValue::I32(i)),
        Value::I64(i) => Some(HintValue::I64(i)),
        Value::U16(u) => Some(HintValue::U32(u as u32)),
        Value::U32(u) => Some(HintValue::U32(u)),
        Value::U64(u) => Some(HintValue::U64(u)),
        Value::F64(f) => Some(HintValue::F64(f)),
        Value::Str(s) => Some(HintValue::String(s.to_string())),
        Value::Signature(s) => Some(HintValue::String(s.to_string())),
        Value::Array(a) => decode_bytes_array(a),
        _ => None,
    }
}

pub fn decode_hints(hints: HashMap<String, OwnedValue>) -> HashMap<String, HintValue> {
    // Best-effort decoding. Unknown/unsupported variants are ignored.
    let mut out = HashMap::new();
    for (k, v) in hints {
        if let Some(h) = decode_hint_value(&v) {
            out.insert(k, h);
        }
    }
    out
}

pub struct Hints {
    pub action_icons: bool,
    pub category: Option<NotificationCategory>,
    pub desktop_entry: Option<Arc<str>>,
    pub image_data: Option<Vec<u8>>,
    pub image_path: Option<Arc<str>>,
    pub resident: bool,
    pub sound_file: Option<Arc<str>>,
    pub sound_name: Option<Arc<str>>,
    pub suppress_sound: bool,
    pub transient: bool,
    pub urgency: NotificationUrgency,
    pub x: i32,
    pub y: i32,
}

fn get_bool_hint(hints: &HashMap<String, HintValue>, key: &str) -> bool {
    hints
        .get(key)
        .map(|v| match v {
            HintValue::Bool(b) => *b,
            _ => false,
        })
        .unwrap_or(false)
}

fn get_int_hint(hints: &HashMap<String, HintValue>, key: &str) -> i32 {
    hints
        .get(key)
        .map(|v| match v {
            HintValue::I32(i) => *i,
            _ => 0,
        })
        .unwrap_or(0)
}

fn get_str_hint(hints: &HashMap<String, HintValue>, key: &str) -> Option<Arc<str>> {
    hints.get(key).map(|v| match v {
        HintValue::String(s) => Arc::from(s.clone()),
        _ => Arc::from(""),
    })
}

fn get_urgency(hints: &HashMap<String, HintValue>) -> NotificationUrgency {
    match hints.get("urgency") {
        Some(HintValue::U32(urgency)) => match urgency {
            0 => NotificationUrgency::Low,
            1 => NotificationUrgency::Normal,
            2 => NotificationUrgency::Critical,
            _ => NotificationUrgency::Normal,
        },
        _ => NotificationUrgency::Normal,
    }
}

fn get_bytes_hint(hints: &HashMap<String, HintValue>, key: &str) -> Option<Vec<u8>> {
    hints.get(key).and_then(|v| match v {
        HintValue::Bytes(b) => Some(b.clone()),
        _ => None,
    })
}

pub fn parse_hints(hints: &HashMap<String, HintValue>) -> Result<Hints> {
    Ok(Hints {
        action_icons: get_bool_hint(hints, "action-icons"),
        category: get_str_hint(hints, "category")
            .map(|s| NotificationCategory::from_str(&s))
            .transpose()
            .map_err(anyhow::Error::msg)?,
        desktop_entry: get_str_hint(hints, "desktop-entry"),
        image_data: get_bytes_hint(hints, "image-data"),
        image_path: get_str_hint(hints, "image-path"),
        resident: get_bool_hint(hints, "resident"),
        sound_file: get_str_hint(hints, "sound-file"),
        sound_name: get_str_hint(hints, "sound-name"),
        suppress_sound: get_bool_hint(hints, "suppress-sound"),
        transient: get_bool_hint(hints, "transient"),
        urgency: get_urgency(hints),
        x: get_int_hint(hints, "x"),
        y: get_int_hint(hints, "y"),
    })
}
