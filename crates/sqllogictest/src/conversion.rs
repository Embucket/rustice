// Adapted from Apache DataFusion's sqllogictest harness
// (`datafusion/sqllogictest/src/engines/conversion.rs`).
// Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

use datafusion::arrow::datatypes::{Decimal128Type, Decimal256Type, DecimalType, i256};
use half::f16;

pub const NULL_STR: &str = "NULL";

pub(crate) fn bool_to_str(value: bool) -> String {
    if value {
        "TRUE".to_string()
    } else {
        "FALSE".to_string()
    }
}

pub(crate) fn varchar_to_str(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    // Match the snowflake-connector-python output the bronze_scope .slt files
    // were generated against: if the string parses as a JSON array or object
    // (e.g. VARIANT / ARRAY / OBJECT columns are stored as Utf8 JSON in
    // Rustice), re-emit it as compact JSON wrapped in single quotes, with
    // object keys sorted alphabetically (snowflake-connector-python passes
    // the result through `json.dumps`, which sorts keys when fed a `dict`
    // — and Snowflake's VARIANT serialiser is also key-sorted).
    if matches!(value.as_bytes().first(), Some(b'{' | b'[')) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(value) {
            if json.is_array() || json.is_object() {
                let sorted = sort_json_keys(json);
                if let Ok(rendered) = serde_json::to_string(&sorted) {
                    return format!("'{rendered}'");
                }
            }
        }
    }
    // Escape embedded newlines to match Python's
    // `value.replace('\n', '\\n')` in slt_runner/result.py — sqllogictest
    // diffs would otherwise turn a single multi-line cell into multiple
    // pseudo-rows.
    value.replace('\n', "\\n").replace('\u{0000}', "\\0")
}

fn sort_json_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = serde_json::Map::new();
            for (k, v) in entries {
                out.insert(k, sort_json_keys(v));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.into_iter().map(sort_json_keys).collect())
        }
        other => other,
    }
}

pub(crate) fn f16_to_str(value: f16) -> String {
    if value.is_nan() {
        "nan".to_string()
    } else if value == f16::INFINITY {
        "inf".to_string()
    } else if value == f16::NEG_INFINITY {
        "-inf".to_string()
    } else {
        python_float_str(f64::from(value))
    }
}

pub(crate) fn f32_to_str(value: f32) -> String {
    if value.is_nan() {
        "nan".to_string()
    } else if value == f32::INFINITY {
        "inf".to_string()
    } else if value == f32::NEG_INFINITY {
        "-inf".to_string()
    } else {
        // Use the f32's own shortest round-trip representation so values like
        // `3.4028235e+38` survive without spurious precision.
        fixup_python_style_exponent(format!("{value:?}"))
    }
}

pub(crate) fn f64_to_str(value: f64) -> String {
    if value.is_nan() {
        "nan".to_string()
    } else if value == f64::INFINITY {
        "inf".to_string()
    } else if value == f64::NEG_INFINITY {
        "-inf".to_string()
    } else {
        python_float_str(value)
    }
}

pub(crate) fn decimal_128_to_str(value: i128, scale: i8) -> String {
    // Preserve the column's declared scale (trailing zeros included), matching
    // snowflake-connector-python's `str(Decimal(...))` output.
    Decimal128Type::format_decimal(value, u8::MAX, scale)
}

pub(crate) fn decimal_256_to_str(value: i256, scale: i8) -> String {
    Decimal256Type::format_decimal(value, u8::MAX, scale)
}

/// Render an `f64` using the same shortest round-trip representation
/// `repr(float)` / `str(float)` uses in Python (which is what
/// snowflake-connector-python emits). Rust's `{:?}` for floats already
/// uses the same shortest-round-trip algorithm; the only formatting
/// difference is that Python prints `e+20` for positive exponents
/// where Rust prints `e20`.
fn python_float_str(value: f64) -> String {
    fixup_python_style_exponent(format!("{value:?}"))
}

fn fixup_python_style_exponent(s: String) -> String {
    if let Some(pos) = s.find('e') {
        let after = pos + 1;
        if after < s.len() {
            let next = s.as_bytes()[after];
            if next != b'+' && next != b'-' {
                let mut out = String::with_capacity(s.len() + 1);
                out.push_str(&s[..after]);
                out.push('+');
                out.push_str(&s[after..]);
                return out;
            }
        }
    }
    s
}
