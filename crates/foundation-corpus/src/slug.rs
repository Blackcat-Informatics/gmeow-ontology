// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `_slug` — the IRI-slug normalizer, a faithful port of the Python helper.
//!
//! Python:
//! ```text
//! norm = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode()
//! return re.sub(r"[^a-z0-9]+", "-", norm.lower()).strip("-") or "x"
//! ```

use unicode_normalization::UnicodeNormalization;

/// Normalize `text` into an IRI-safe slug.
///
/// NFKD-normalize, drop non-ASCII code points (`encode("ascii", "ignore")`),
/// lowercase, collapse runs of non-`[a-z0-9]` characters to a single `-`, trim
/// leading/trailing `-`, and fall back to `"x"` when the result is empty.
pub fn slug(text: &str) -> String {
    // NFKD then drop any char outside the ASCII range (mirrors
    // `.encode("ascii", "ignore")`), then lowercase. Python's `str.lower()`
    // operates on the post-ASCII-filter string, which here is pure ASCII.
    let ascii_lower: String = text
        .nfkd()
        .filter(|c| c.is_ascii())
        .collect::<String>()
        .to_lowercase();

    // re.sub(r"[^a-z0-9]+", "-", ...): collapse runs of disallowed chars to "-".
    let mut out = String::with_capacity(ascii_lower.len());
    let mut in_run = false;
    for ch in ascii_lower.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            out.push(ch);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }

    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "x".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Take the first `n` Unicode characters of `text` (Python `text[:n]` semantics).
pub fn char_prefix(text: &str, n: usize) -> String {
    text.chars().take(n).collect()
}

/// Take the last `n` Unicode characters of `text` (Python `text[-n:]` semantics).
pub fn char_suffix(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.len().saturating_sub(n);
    chars[start..].iter().collect()
}

#[path = "slug.tests.rs"]
#[cfg(test)]
mod tests;
