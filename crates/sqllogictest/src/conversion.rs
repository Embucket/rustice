// Adapted from Apache DataFusion's sqllogictest harness
// (`datafusion/sqllogictest/src/engines/conversion.rs`).
// Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

use bigdecimal::BigDecimal;
use datafusion::arrow::datatypes::{Decimal128Type, Decimal256Type, DecimalType, i256};
use half::f16;
use std::str::FromStr;

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
    // Rustice), re-emit it as compact JSON wrapped in single quotes.
    if matches!(value.as_bytes().first(), Some(b'{' | b'[')) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(value) {
            if json.is_array() || json.is_object() {
                if let Ok(rendered) = serde_json::to_string(&json) {
                    return format!("'{rendered}'");
                }
            }
        }
    }
    value.trim_end_matches('\n').replace('\u{0000}', "\\0")
}

pub(crate) fn f16_to_str(value: f16) -> String {
    if value.is_nan() {
        "nan".to_string()
    } else if value == f16::INFINITY {
        "inf".to_string()
    } else if value == f16::NEG_INFINITY {
        "-inf".to_string()
    } else {
        preserve_trailing_dot_zero(big_decimal_to_str(
            BigDecimal::from_str(&value.to_string()).unwrap(),
            None,
        ))
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
        preserve_trailing_dot_zero(big_decimal_to_str(
            BigDecimal::from_str(&value.to_string()).unwrap(),
            None,
        ))
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
        preserve_trailing_dot_zero(big_decimal_to_str(
            BigDecimal::from_str(&value.to_string()).unwrap(),
            None,
        ))
    }
}

pub(crate) fn decimal_128_to_str(value: i128, scale: i8) -> String {
    let precision = u8::MAX;
    big_decimal_to_str(
        BigDecimal::from_str(&Decimal128Type::format_decimal(value, precision, scale)).unwrap(),
        None,
    )
}

pub(crate) fn decimal_256_to_str(value: i256, scale: i8) -> String {
    let precision = u8::MAX;
    big_decimal_to_str(
        BigDecimal::from_str(&Decimal256Type::format_decimal(value, precision, scale)).unwrap(),
        None,
    )
}

#[expect(clippy::needless_pass_by_value)]
pub(crate) fn big_decimal_to_str(value: BigDecimal, round_digits: Option<i64>) -> String {
    let value = value.round(round_digits.unwrap_or(12)).normalized();
    value.to_plain_string()
}

// Whole-number floats lose their decimal point through `BigDecimal::normalized`
// (e.g. `0.0` -> `"0"`). Snowflake's Python connector renders `str(0.0)` as
// `"0.0"`, so re-append `.0` when the rendered form has no fractional part.
fn preserve_trailing_dot_zero(s: String) -> String {
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{s}.0")
    }
}
