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
/// `<REGEX>:` or `<!REGEX>:` is compiled and matched (or negative-matched)
/// against the actual row; everything else is compared after running both
/// expected and actual through the upstream normalizer (which trims and
/// collapses runs of whitespace).
///
/// We normalize both sides so that the literal tab the `.slt` parser
/// embeds between columns doesn't have to match the literal join
/// character used here. Joining actual with `" "` mirrors what the
/// upstream `default_validator` does.
///
/// Regex semantics match the Python runner's `matches_regex`: the pattern
/// is anchored (`fullmatch`) and `.` matches newlines (`re.DOTALL`).
#[must_use]
pub fn embucket_validator(
    normalizer: Normalizer,
    actual: &[Vec<String>],
    expected: &[String],
) -> bool {
    let actual_rows: Vec<String> = actual
        .iter()
        .map(|row| {
            // Renormalize the joined row so that a whitespace-only cell
            // (e.g. `"   "` → empty after `trim`) doesn't leave a stray
            // separator behind. Without the outer pass, an empty cell
            // would produce `" 0"` where the upstream-normalized
            // expected line is just `"0"`.
            let joined = row.iter().map(&normalizer).collect::<Vec<_>>().join(" ");
            normalizer(&joined)
        })
        .collect();

    if actual_rows.len() != expected.len() {
        return false;
    }

    for (actual_row, expected_row) in actual_rows.iter().zip(expected.iter()) {
        if let Some((pattern, want_match)) = strip_regex_prefix(expected_row) {
            // (?s) = DOTALL; \A...\z = fullmatch.
            let wrapped = format!("(?s)\\A(?:{pattern})\\z");
            match regex::Regex::new(&wrapped) {
                Ok(re) => {
                    if re.is_match(actual_row) != want_match {
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

/// Returns `(pattern, want_match)` for `<REGEX>:` / `<!REGEX>:` prefixed
/// expected rows. `None` if neither prefix is present.
fn strip_regex_prefix(s: &str) -> Option<(&str, bool)> {
    if let Some(rest) = s.strip_prefix("<REGEX>:") {
        Some((rest, true))
    } else {
        s.strip_prefix("<!REGEX>:").map(|rest| (rest, false))
    }
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
    fn rtrimmed_length_repro() {
        // The actual values are the multi-space strings the test originally
        // emitted; the expected lines use tabs between columns. The default
        // normalizer collapses both into "<inner> <count>".
        let norm = sqllogictest::default_normalizer;
        let actual = vec![
            vec!["  hello  ".to_string(), "7".to_string()],
            vec!["''".to_string(), "0".to_string()],
            vec!["test   ".to_string(), "4".to_string()],
        ];
        let expected = vec![
            "  hello  \t7".to_string(),
            "''\t0".to_string(),
            "test   \t4".to_string(),
        ];
        assert!(embucket_validator(norm, &actual, &expected));
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
    fn regex_fullmatch_anchored() {
        // Pattern matches `abc` exactly; actual `abcd` should NOT match
        // because we anchor the pattern (fullmatch semantics) the same way
        // Python's `re.fullmatch` does.
        assert!(!embucket_validator(
            id_norm,
            &[vec!["abcd".to_string()]],
            &["<REGEX>:abc".to_string()],
        ));
    }

    #[test]
    fn regex_dotall_matches_newline() {
        // `.` should match newlines (DOTALL). The default `regex` crate
        // would otherwise refuse a `.` against `\n`.
        assert!(embucket_validator(
            id_norm,
            &[vec!["a\nb".to_string()]],
            &["<REGEX>:a.b".to_string()],
        ));
    }

    #[test]
    fn negative_regex_blocks_match() {
        // `<!REGEX>:` returns true only when the pattern does NOT match.
        assert!(!embucket_validator(
            id_norm,
            &[vec!["FOO".to_string()]],
            &["<!REGEX>:^[FO|]{3}$".to_string()],
        ));
    }

    #[test]
    fn negative_regex_passes_on_nonmatch() {
        assert!(embucket_validator(
            id_norm,
            &[vec!["FOX".to_string()]],
            &["<!REGEX>:^[FO|]{3}$".to_string()],
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
