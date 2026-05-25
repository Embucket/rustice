//! Embucket SQL Logic Test harness.
//!
//! Layout:
//!   * [`conversion`] / [`normalize`] / [`output`] — Arrow `RecordBatch` →
//!     `Vec<Vec<String>>` conversion adapted verbatim from DataFusion's
//!     sqllogictest harness. Reused to match the float and decimal rounding
//!     used by `.slt` corpora authored against DataFusion.
//!   * [`preprocessor`] — strips embucket-specific directives so the upstream
//!     `sqllogictest` parser accepts the files.
//!   * [`engine`] — `EmbucketSession`, the `AsyncDB` adapter that drives
//!     rustice's `UserSession`.
//!   * `validator` (in this file, [`embucket_validator`]) — recognises
//!     `<REGEX>:<pattern>` expected values used by the embucket corpus.

pub mod conversion;
pub mod engine;
pub mod error;
pub mod normalize;
pub mod output;
pub mod preprocessor;

use sqllogictest::Normalizer;

/// Cell-comparison validator: per-cell, an expected value beginning with
/// `<REGEX>:` is compiled and matched against the actual cell; everything
/// else is compared verbatim (after the upstream normalizer).
#[must_use]
pub fn embucket_validator(
    normalizer: Normalizer,
    actual: &[Vec<String>],
    expected: &[String],
) -> bool {
    let actual_rows: Vec<String> = actual
        .iter()
        .map(|row| row.iter().map(&normalizer).collect::<Vec<_>>().join("\t"))
        .collect();

    if actual_rows.len() != expected.len() {
        return false;
    }

    for (actual_row, expected_row) in actual_rows.iter().zip(expected.iter()) {
        if let Some(pattern) = expected_row.strip_prefix("<REGEX>:") {
            match regex::Regex::new(pattern) {
                Ok(re) => {
                    if !re.is_match(actual_row) {
                        return false;
                    }
                }
                Err(_) => return false,
            }
        } else if actual_row != expected_row {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::embucket_validator;

    #[allow(clippy::ptr_arg)]
    fn id_norm(s: &String) -> String {
        s.clone()
    }

    #[test]
    fn verbatim_match() {
        assert!(embucket_validator(
            id_norm,
            &[vec!["1".to_string(), "a".to_string()]],
            &["1\ta".to_string()],
        ));
    }

    #[test]
    fn verbatim_mismatch() {
        assert!(!embucket_validator(
            id_norm,
            &[vec!["1".to_string(), "a".to_string()]],
            &["1\tb".to_string()],
        ));
    }

    #[test]
    fn regex_match() {
        assert!(embucket_validator(
            id_norm,
            &[vec!["F|O".to_string()]],
            &["<REGEX>:^[FO|]{3}$".to_string()],
        ));
    }

    #[test]
    fn regex_mismatch() {
        assert!(!embucket_validator(
            id_norm,
            &[vec!["FOX".to_string()]],
            &["<REGEX>:^[FO|]{3}$".to_string()],
        ));
    }

    #[test]
    fn row_count_mismatch() {
        assert!(!embucket_validator(
            id_norm,
            &[vec!["1".to_string()]],
            &["1".to_string(), "2".to_string()],
        ));
    }
}
