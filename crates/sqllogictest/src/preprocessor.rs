//! Strip non-standard directives from embucket `.slt` files before handing the
//! content to the upstream `sqllogictest` parser.
//!
//! The embucket Python runner accepts three extra directives that the Rust
//! parser does not understand:
//!
//!   * `exclude-from-coverage` — a single-line marker that precedes a
//!     `statement` record, indicating the block should not count towards
//!     coverage. Functionally a no-op for test execution.
//!   * `skip-if <condition>` — inline conditional skip.
//!   * `only-if <condition>` — inline conditional run.
//!
//! Since the embucket corpus does not currently use `skip-if`/`only-if` to
//! gate on the rustice engine label and we always run with the same target,
//! all three directives can be safely dropped at the line level before
//! parsing. Standard `onlyif`/`skipif` (the upstream forms, no hyphen) are
//! left untouched.

const STRIP_PREFIXES: &[&str] = &[
    "exclude-from-coverage",
    "skip-if ",
    "only-if ",
];

/// Returns the input with any line starting (after leading whitespace) with one
/// of the embucket-specific directive prefixes removed. Other lines pass
/// through unchanged, including newline characters at the end of each line.
#[must_use]
pub fn strip_custom_directives(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for line in input.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if STRIP_PREFIXES.iter().any(|p| trimmed.starts_with(p)) {
            continue;
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_custom_directives;

    #[test]
    fn strips_exclude_from_coverage() {
        let input = "exclude-from-coverage\nstatement ok\nSELECT 1;\n";
        assert_eq!(strip_custom_directives(input), "statement ok\nSELECT 1;\n");
    }

    #[test]
    fn strips_skip_if_and_only_if() {
        let input = "skip-if foo\nonly-if bar\nstatement ok\nSELECT 1;\n";
        assert_eq!(strip_custom_directives(input), "statement ok\nSELECT 1;\n");
    }

    #[test]
    fn leaves_upstream_directives_alone() {
        let input = "onlyif embucket\nstatement ok\nSELECT 1;\n";
        assert_eq!(strip_custom_directives(input), input);
    }

    #[test]
    fn preserves_blank_lines_and_query_results() {
        let input = "query I\nSELECT 1;\n----\n1\n\nquery I\nSELECT 2;\n----\n2\n";
        assert_eq!(strip_custom_directives(input), input);
    }
}
