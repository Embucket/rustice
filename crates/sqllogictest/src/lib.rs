//! Embucket SQL Logic Test harness.
//!
//! Layout:
//!   * [`conversion`] / [`normalize`] / [`output`] — Arrow `RecordBatch` →
//!     `Vec<Vec<String>>` conversion adapted verbatim from DataFusion's
//!     sqllogictest harness. Reused to match the float and decimal rounding
//!     used by the embucket corpus.
//!   * [`engine`] — `EmbucketSession`, the `AsyncDB` adapter that drives
//!     rustice's `UserSession`.
//!   * `validator` (in this file, [`embucket_validator`]) — recognises
//!     `<REGEX>:<pattern>` expected values used by the embucket corpus.

pub mod conversion;
pub mod engine;
pub mod error;
pub mod normalize;
pub mod output;

use sqllogictest::Normalizer;

/// Row-comparison validator: per-row, an expected value beginning with
/// `<REGEX>:` is compiled and matched against the actual row; everything
/// else is compared after running both expected and actual through the
/// upstream normalizer (which trims and collapses runs of whitespace).
///
/// We normalize both sides so that the literal tab the `.slt` parser
/// embeds between columns doesn't have to match the literal join
/// character used here. Joining actual with `" "` mirrors what the
/// upstream `default_validator` does.
#[must_use]
pub fn embucket_validator(
    normalizer: Normalizer,
    actual: &[Vec<String>],
    expected: &[String],
) -> bool {
    let actual_rows: Vec<String> = actual
        .iter()
        .map(|row| row.iter().map(&normalizer).collect::<Vec<_>>().join(" "))
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
        } else if *actual_row != normalizer(expected_row) {
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
            &["1 a".to_string()],
        ));
    }

    #[test]
    fn verbatim_mismatch() {
        assert!(!embucket_validator(
            id_norm,
            &[vec!["1".to_string(), "a".to_string()]],
            &["1 b".to_string()],
        ));
    }

    #[test]
    fn tab_in_expected_matches_space_in_actual_join() {
        // The .slt parser leaves a literal `\t` between columns; the
        // upstream normalizer collapses whitespace so the tab equates
        // to the space we join actual cells with.
        let norm = sqllogictest::default_normalizer;
        assert!(embucket_validator(
            norm,
            &[vec!["1".to_string(), "a".to_string()]],
            &["1\ta".to_string()],
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
