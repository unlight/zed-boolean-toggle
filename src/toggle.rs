//! Pure boolean-token toggle logic.
//!
//! This module is intentionally free of any Zed / WASM dependencies so it
//! can be unit-tested with plain `cargo test` and reused in other hosts
//! (e.g. a CLI, an LSP server, a different editor extension).
//!
//! # Behaviour
//!
//! * Default pairs (every language): `true` ↔ `false`.
//! * Markdown-only extra pairs: `1` ↔ `0`, `on` ↔ `off`, `yes` ↔ `no`.
//! * Case is preserved on the *original* token:
//!     - `TRUE`  → `FALSE`
//!     - `True`  → `False`
//!     - `true`  → `false`
//!     - `tRuE`  → `false`  (mixed-case falls back to lowercase)
//! * Non-alphabetic tokens (e.g. `1` / `0`) are returned as-is.
//! * Unknown tokens return `None` so the caller can leave the buffer
//!   untouched and surface a friendly message.

/// A toggle pair. `from` is matched case-insensitively against the input
/// token; `to` is the lowercase replacement that will be re-cased to
/// match the original.
#[derive(Debug, Clone, Copy)]
struct Pair {
    from: &'static str,
    to: &'static str,
}

/// Pairs that apply to every language.
const BASE_PAIRS: &[Pair] = &[
    Pair {
        from: "true",
        to: "false",
    },
    Pair {
        from: "false",
        to: "true",
    },
];

/// Pairs that only apply inside Markdown documents.
const MARKDOWN_PAIRS: &[Pair] = &[
    Pair { from: "1", to: "0" },
    Pair { from: "0", to: "1" },
    Pair {
        from: "on",
        to: "off",
    },
    Pair {
        from: "off",
        to: "on",
    },
    Pair {
        from: "yes",
        to: "no",
    },
    Pair {
        from: "no",
        to: "yes",
    },
];

/// Identify whether the given Zed language name should enable the
/// extended Markdown pairs.
///
/// Zed reports language names in title-case (e.g. `"Markdown"`), but we
/// accept common variants defensively.
pub fn is_markdown(language: Option<&str>) -> bool {
    matches!(
        language.map(str::to_ascii_lowercase).as_deref(),
        Some("markdown") | Some("md") | Some("mdx")
    )
}

/// Toggle a single token. Returns `None` if the token is not recognised.
///
/// `language` is the Zed-reported language name (or `None` for plain
/// text). It is used to enable the Markdown-only pairs.
pub fn toggle_token(token: &str, language: Option<&str>) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    let pairs = BASE_PAIRS.iter().chain(if is_markdown(language) {
        MARKDOWN_PAIRS
    } else {
        &[]
    });

    for pair in pairs {
        if trimmed.eq_ignore_ascii_case(pair.from) {
            return Some(match_case(trimmed, pair.to));
        }
    }
    None
}

/// Re-case `replacement` so it visually matches `original`.
///
/// * All-uppercase original → all-uppercase replacement.
/// * Title-case original    → title-case replacement.
/// * Anything else (incl. all-lowercase and mixed) → lowercase.
fn match_case(original: &str, replacement: &str) -> String {
    // Treat purely non-alphabetic tokens (e.g. "1") as case-neutral.
    if !original.chars().any(|c| c.is_alphabetic()) {
        return replacement.to_string();
    }

    let letters: Vec<char> = original.chars().filter(|c| c.is_alphabetic()).collect();
    let all_upper = letters.iter().all(|c| c.is_uppercase());
    let first_upper = letters.first().map(|c| c.is_uppercase()).unwrap_or(false);
    let rest_lower = letters.iter().skip(1).all(|c| c.is_lowercase());

    if all_upper && letters.len() > 1 {
        replacement.to_ascii_uppercase()
    } else if first_upper && rest_lower {
        // Title-case: capitalise the first ASCII letter.
        let mut out = String::with_capacity(replacement.len());
        let mut chars = replacement.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
        }
        out.extend(chars);
        out
    } else {
        replacement.to_ascii_lowercase()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_basic_lowercase() {
        assert_eq!(toggle_token("true", None).as_deref(), Some("false"));
        assert_eq!(toggle_token("false", None).as_deref(), Some("true"));
    }

    #[test]
    fn toggles_uppercase() {
        assert_eq!(toggle_token("TRUE", None).as_deref(), Some("FALSE"));
        assert_eq!(toggle_token("FALSE", None).as_deref(), Some("TRUE"));
    }

    #[test]
    fn toggles_title_case() {
        assert_eq!(toggle_token("True", None).as_deref(), Some("False"));
        assert_eq!(toggle_token("False", None).as_deref(), Some("True"));
    }

    #[test]
    fn mixed_case_falls_back_to_lowercase() {
        assert_eq!(toggle_token("tRuE", None).as_deref(), Some("false"));
    }

    #[test]
    fn unknown_token_returns_none() {
        assert!(toggle_token("maybe", None).is_none());
        assert!(toggle_token("", None).is_none());
        assert!(toggle_token("   ", None).is_none());
    }

    #[test]
    fn non_markdown_does_not_toggle_extras() {
        assert!(toggle_token("on", Some("Rust")).is_none());
        assert!(toggle_token("yes", Some("Python")).is_none());
        assert!(toggle_token("1", Some("JavaScript")).is_none());
    }

    #[test]
    fn markdown_toggles_extras() {
        assert_eq!(toggle_token("on", Some("Markdown")).as_deref(), Some("off"));
        assert_eq!(toggle_token("OFF", Some("Markdown")).as_deref(), Some("ON"));
        assert_eq!(toggle_token("Yes", Some("Markdown")).as_deref(), Some("No"));
        assert_eq!(toggle_token("no", Some("Markdown")).as_deref(), Some("yes"));
        assert_eq!(toggle_token("1", Some("Markdown")).as_deref(), Some("0"));
        assert_eq!(toggle_token("0", Some("md")).as_deref(), Some("1"));
    }

    #[test]
    fn markdown_still_toggles_base_pairs() {
        assert_eq!(
            toggle_token("true", Some("Markdown")).as_deref(),
            Some("false")
        );
    }

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(toggle_token("  true  ", None).as_deref(), Some("false"));
    }

    #[test]
    fn is_markdown_accepts_variants() {
        assert!(is_markdown(Some("Markdown")));
        assert!(is_markdown(Some("markdown")));
        assert!(is_markdown(Some("md")));
        assert!(is_markdown(Some("MDX")));
        assert!(!is_markdown(Some("Rust")));
        assert!(!is_markdown(None));
    }
}
