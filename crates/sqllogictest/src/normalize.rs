// Adapted from Apache DataFusion's sqllogictest harness
// (`datafusion/sqllogictest/src/engines/datafusion_engine/normalize.rs`).
// Licensed under the Apache License, Version 2.0.

#![allow(clippy::unwrap_used)]

use crate::conversion::{
    NULL_STR, bool_to_str, decimal_128_to_str, decimal_256_to_str, f16_to_str, f32_to_str,
    f64_to_str, varchar_to_str,
};
use crate::error::{Error, Result};
use crate::output::DFColumnType;
use datafusion::arrow::array::{Array, AsArray};
use datafusion::arrow::datatypes::{
    Fields, Float16Type, Float32Type, Float64Type, Int8Type, Int16Type, Int32Type, Int64Type,
    Schema, UInt8Type, UInt16Type, UInt32Type, UInt64Type,
};
use datafusion::arrow::util::display::ArrayFormatter;
use datafusion::arrow::{array, array::ArrayRef, datatypes::DataType, record_batch::RecordBatch};
use serde_json::Value as JsonValue;

/// Convert a `Vec<RecordBatch>` to the row-of-strings format used by sqllogictest.
pub fn convert_batches(schema: &Schema, batches: Vec<RecordBatch>) -> Result<Vec<Vec<String>>> {
    let mut rows = vec![];
    for batch in batches {
        if !schema.contains(&batch.schema()) {
            return Err(Error::Other(format!(
                "Schema mismatch. Previously had\n{:#?}\n\nGot:\n{:#?}",
                &schema,
                batch.schema()
            )));
        }

        let new_rows = (0..batch.num_rows())
            .map(|row| {
                batch
                    .columns()
                    .iter()
                    .map(|col| cell_to_string(col, row))
                    .collect::<Result<Vec<String>>>()
            })
            .collect::<Result<Vec<Vec<String>>>>()?
            .into_iter()
            .flat_map(expand_row);
        rows.extend(new_rows);
    }
    Ok(rows)
}

/// Special-case rows with newlines (EXPLAIN-style output) by splitting the
/// last cell into multiple physical rows. Matches `DataFusion`'s behavior so
/// `.slt` files authored against `DataFusion`-style explain plans still parse.
///
/// Only applied to single-column rows. Multi-column rows are passed through
/// verbatim: their cells may legitimately contain newlines (VARIANT/JSON
/// output) and splitting would silently drop preceding columns.
fn expand_row(mut row: Vec<String>) -> impl Iterator<Item = Vec<String>> {
    use itertools::Either;
    use std::iter::once;

    if row.len() != 1 {
        return Either::Left(once(row));
    }

    if let Some(cell) = row.pop() {
        let lines: Vec<_> = cell.split('\n').collect();
        if lines.len() < 2 {
            row.push(cell);
            return Either::Left(once(row));
        }
        let new_lines: Vec<_> = lines
            .into_iter()
            .enumerate()
            .map(|(idx, l)| {
                let content = l.trim_start();
                let new_prefix = "-".repeat(l.len() - content.len());
                let line_num = idx + 1;
                vec![format!("{line_num:02}){new_prefix}{content}")]
            })
            .collect();
        Either::Right(once(row).chain(new_lines))
    } else {
        Either::Left(once(row))
    }
}

macro_rules! get_row_value {
    ($array_type:ty, $column:ident, $row:ident) => {{
        let array = $column.as_any().downcast_ref::<$array_type>().unwrap();
        array.value($row)
    }};
}

/// Normalize a single cell into the string form sqllogictest expects.
pub fn cell_to_string(col: &ArrayRef, row: usize) -> Result<String> {
    if col.is_null(row) {
        return Ok(NULL_STR.to_string());
    }
    match col.data_type() {
        DataType::Null => Ok(NULL_STR.to_string()),
        DataType::Boolean => Ok(bool_to_str(get_row_value!(array::BooleanArray, col, row))),
        DataType::Float16 => Ok(f16_to_str(get_row_value!(array::Float16Array, col, row))),
        DataType::Float32 => Ok(f32_to_str(get_row_value!(array::Float32Array, col, row))),
        DataType::Float64 => Ok(f64_to_str(get_row_value!(array::Float64Array, col, row))),
        DataType::Decimal128(_, scale) => {
            let value = get_row_value!(array::Decimal128Array, col, row);
            Ok(decimal_128_to_str(value, *scale))
        }
        DataType::Decimal256(_, scale) => {
            let value = get_row_value!(array::Decimal256Array, col, row);
            Ok(decimal_256_to_str(value, *scale))
        }
        DataType::LargeUtf8 => Ok(varchar_to_str(get_row_value!(
            array::LargeStringArray,
            col,
            row
        ))),
        DataType::Utf8 => Ok(varchar_to_str(get_row_value!(array::StringArray, col, row))),
        DataType::Utf8View => Ok(varchar_to_str(get_row_value!(
            array::StringViewArray,
            col,
            row
        ))),
        DataType::Dictionary(_, _) => {
            let dict = col.as_any_dictionary();
            let key = dict.normalized_keys()[row];
            cell_to_string(dict.values(), key)
        }
        // Snowflake-style binary literal: x'<lowercase hex>'
        DataType::Binary => Ok(format!(
            "x'{}'",
            hex::encode(get_row_value!(array::BinaryArray, col, row))
        )),
        DataType::LargeBinary => Ok(format!(
            "x'{}'",
            hex::encode(get_row_value!(array::LargeBinaryArray, col, row))
        )),
        DataType::BinaryView => Ok(format!(
            "x'{}'",
            hex::encode(get_row_value!(array::BinaryViewArray, col, row))
        )),
        DataType::FixedSizeBinary(_) => Ok(format!(
            "x'{}'",
            hex::encode(get_row_value!(array::FixedSizeBinaryArray, col, row))
        )),
        // Snowflake renders DATE values in single-quoted ISO form.
        DataType::Date32 | DataType::Date64 => Ok(format!("'{}'", arrow_formatted(col, row)?)),
        // TIME/TIMESTAMP: same as DATE but Snowflake always renders the
        // subsecond fraction with 6 digits when present (`HH:MM:SS.123000`
        // not `HH:MM:SS.123`).
        DataType::Time32(_) | DataType::Time64(_) | DataType::Timestamp(_, _) => Ok(format!(
            "'{}'",
            pad_subseconds_to_microseconds(arrow_formatted(col, row)?)
        )),
        // VARIANT/ARRAY/OBJECT-style columns: serialise as compact JSON
        // (sorted object keys, primitives stringified the same way as
        // top-level cells) so the output matches snowflake-connector-python's
        // `json.dumps` rendering instead of Arrow's debug-format
        // `[a, b, c]` / `{a: 1, b: 2}`.
        DataType::List(_)
        | DataType::LargeList(_)
        | DataType::FixedSizeList(_, _)
        | DataType::ListView(_)
        | DataType::LargeListView(_)
        | DataType::Struct(_)
        | DataType::Map(_, _) => {
            let json = array_value_to_json(col, row);
            Ok(format!(
                "'{}'",
                serde_json::to_string(&json).unwrap_or_default()
            ))
        }
        _ => arrow_formatted(col, row),
    }
}

#[allow(clippy::too_many_lines)]
fn array_value_to_json(col: &ArrayRef, row: usize) -> JsonValue {
    if col.is_null(row) {
        return JsonValue::Null;
    }
    match col.data_type() {
        DataType::Null => JsonValue::Null,
        DataType::Boolean => JsonValue::Bool(get_row_value!(array::BooleanArray, col, row)),
        DataType::Int8 => JsonValue::from(col.as_primitive::<Int8Type>().value(row)),
        DataType::Int16 => JsonValue::from(col.as_primitive::<Int16Type>().value(row)),
        DataType::Int32 => JsonValue::from(col.as_primitive::<Int32Type>().value(row)),
        DataType::Int64 => JsonValue::from(col.as_primitive::<Int64Type>().value(row)),
        DataType::UInt8 => JsonValue::from(col.as_primitive::<UInt8Type>().value(row)),
        DataType::UInt16 => JsonValue::from(col.as_primitive::<UInt16Type>().value(row)),
        DataType::UInt32 => JsonValue::from(col.as_primitive::<UInt32Type>().value(row)),
        DataType::UInt64 => JsonValue::from(col.as_primitive::<UInt64Type>().value(row)),
        DataType::Float16 => {
            JsonValue::from(f32::from(col.as_primitive::<Float16Type>().value(row)))
        }
        DataType::Float32 => JsonValue::from(col.as_primitive::<Float32Type>().value(row)),
        DataType::Float64 => JsonValue::from(col.as_primitive::<Float64Type>().value(row)),
        DataType::Utf8 => {
            JsonValue::String(get_row_value!(array::StringArray, col, row).to_string())
        }
        DataType::LargeUtf8 => {
            JsonValue::String(get_row_value!(array::LargeStringArray, col, row).to_string())
        }
        DataType::Utf8View => {
            JsonValue::String(get_row_value!(array::StringViewArray, col, row).to_string())
        }
        DataType::List(_) => {
            let list = col.as_list::<i32>();
            let values = list.value(row);
            JsonValue::Array(
                (0..values.len())
                    .map(|i| array_value_to_json(&values, i))
                    .collect(),
            )
        }
        DataType::LargeList(_) => {
            let list = col.as_list::<i64>();
            let values = list.value(row);
            JsonValue::Array(
                (0..values.len())
                    .map(|i| array_value_to_json(&values, i))
                    .collect(),
            )
        }
        DataType::FixedSizeList(_, _) => {
            let list = col.as_fixed_size_list();
            let values = list.value(row);
            JsonValue::Array(
                (0..values.len())
                    .map(|i| array_value_to_json(&values, i))
                    .collect(),
            )
        }
        DataType::Struct(fields) => {
            let s = col.as_struct();
            let mut obj = serde_json::Map::new();
            // Sort by field name to match snowflake-connector-python's
            // sorted-key JSON output.
            let mut keyed: Vec<(String, ArrayRef)> = fields
                .iter()
                .enumerate()
                .map(|(idx, f)| (f.name().clone(), s.column(idx).clone()))
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, child) in keyed {
                obj.insert(name, array_value_to_json(&child, row));
            }
            JsonValue::Object(obj)
        }
        DataType::Map(_, _) => {
            let map = col.as_map();
            let keys = map.keys();
            let values = map.values();
            let offsets = map.value_offsets();
            let start = offsets[row] as usize;
            let end = offsets[row + 1] as usize;
            let mut entries: Vec<(String, JsonValue)> = (start..end)
                .map(|i| {
                    let k = match keys.data_type() {
                        DataType::Utf8 => keys
                            .as_any()
                            .downcast_ref::<array::StringArray>()
                            .unwrap()
                            .value(i)
                            .to_string(),
                        DataType::LargeUtf8 => keys
                            .as_any()
                            .downcast_ref::<array::LargeStringArray>()
                            .unwrap()
                            .value(i)
                            .to_string(),
                        _ => {
                            serde_json::to_string(&array_value_to_json(keys, i)).unwrap_or_default()
                        }
                    };
                    (k, array_value_to_json(values, i))
                })
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut obj = serde_json::Map::new();
            for (k, v) in entries {
                obj.insert(k, v);
            }
            JsonValue::Object(obj)
        }
        DataType::Dictionary(_, _) => {
            let dict = col.as_any_dictionary();
            let key = dict.normalized_keys()[row];
            array_value_to_json(dict.values(), key)
        }
        // Fall back to arrow's formatted form for less-common types.
        _ => arrow_formatted(col, row).map_or(JsonValue::Null, JsonValue::String),
    }
}

fn arrow_formatted(col: &ArrayRef, row: usize) -> Result<String> {
    let mut format_options = datafusion::arrow::util::display::FormatOptions::default();
    format_options = format_options.with_null("NULL");
    let f = ArrayFormatter::try_new(col.as_ref(), &format_options).map_err(Error::Arrow)?;
    Ok(f.value(row).to_string())
}

/// Snowflake's Python connector returns `datetime.datetime` / `datetime.time`
/// values whose `isoformat()` always renders the fractional component with
/// 6 digits (`microsecond`). Arrow's default formatter trims trailing
/// zeros and may also expose nanosecond precision; reshape its output to
/// always have exactly 6 fractional digits when a fractional part is
/// present.
fn pad_subseconds_to_microseconds(mut s: String) -> String {
    let Some(dot_idx) = s.find('.') else {
        return s;
    };
    // Ensure the `.` is part of a time fragment (`HH:MM:SS.XXX`), not a
    // decimal in some other position.
    if dot_idx < 2 || !s.as_bytes()[dot_idx - 1].is_ascii_digit() {
        return s;
    }
    let bytes = s.as_bytes();
    let mut end = dot_idx + 1;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let frac_len = end - dot_idx - 1;
    if frac_len == 6 {
        return s;
    }
    if frac_len < 6 {
        let pad = "0".repeat(6 - frac_len);
        s.insert_str(end, &pad);
    } else {
        // Truncate down to 6 digits (e.g. nanosecond -> microsecond).
        s.replace_range(dot_idx + 1 + 6..end, "");
    }
    s
}

/// Map Arrow schema fields to the sqllogictest `ColumnType` chars.
pub fn convert_schema_to_types(columns: &Fields) -> Vec<DFColumnType> {
    columns
        .iter()
        .map(|f| f.data_type())
        .map(|data_type| match data_type {
            DataType::Boolean => DFColumnType::Boolean,
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64 => DFColumnType::Integer,
            DataType::Float16
            | DataType::Float32
            | DataType::Float64
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _) => DFColumnType::Float,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => DFColumnType::Text,
            DataType::Date32 | DataType::Date64 | DataType::Time32(_) | DataType::Time64(_) => {
                DFColumnType::DateTime
            }
            DataType::Timestamp(_, _) => DFColumnType::Timestamp,
            DataType::Dictionary(key_type, value_type) => {
                if key_type.is_integer() {
                    match value_type.as_ref() {
                        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
                            DFColumnType::Text
                        }
                        _ => DFColumnType::Another,
                    }
                } else {
                    DFColumnType::Another
                }
            }
            _ => DFColumnType::Another,
        })
        .collect()
}
