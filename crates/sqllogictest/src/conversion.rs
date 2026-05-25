// Adapted from Apache DataFusion's sqllogictest harness
// (`datafusion/sqllogictest/src/engines/conversion.rs`).
// Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

use datafusion::arrow::datatypes::{Decimal128Type, Decimal256Type, DecimalType, i256};
use bigdecimal::BigDecimal;
use half::f16;
use std::str::FromStr;

pub const NULL_STR: &str = "NULL";

pub(crate) fn bool_to_str(value: bool) -> String {
    if value { "true".to_string() } else { "false".to_string() }
}

pub(crate) fn varchar_to_str(value: &str) -> String {
    if value.is_empty() {
        "(empty)".to_string()
    } else {
        value.trim_end_matches('\n').replace('\u{0000}', "\\0")
    }
}

pub(crate) fn f16_to_str(value: f16) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f16::INFINITY {
        "Infinity".to_string()
    } else if value == f16::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        big_decimal_to_str(BigDecimal::from_str(&value.to_string()).unwrap(), None)
    }
}

pub(crate) fn f32_to_str(value: f32) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f32::INFINITY {
        "Infinity".to_string()
    } else if value == f32::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        big_decimal_to_str(BigDecimal::from_str(&value.to_string()).unwrap(), None)
    }
}

pub(crate) fn f64_to_str(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value == f64::INFINITY {
        "Infinity".to_string()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else {
        big_decimal_to_str(BigDecimal::from_str(&value.to_string()).unwrap(), None)
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
