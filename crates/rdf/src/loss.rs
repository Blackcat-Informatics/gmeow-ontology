// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The machine-readable RDF↔GTS loss ledger (#819 C0).
//!
//! "RDF 1.2 fidelity" cannot be claimed for a conversion until either the GTS
//! representation is extended to carry every RDF 1.2 feature, **or** every
//! intentional loss is enumerated, tested, and exposed as a stable contract. This
//! module is option (b): a small, deterministic ledger of the known, accepted
//! conversion losses between the RDF 1.2 dataset IR and the GTS transport. The
//! gate is simple — `RdfBundle` fidelity is asserted **only** where the relevant
//! ledger [`LossLedger::is_empty`].
//!
//! The ledger is kernel-clean (PyO3-free) and renders to byte-stable JSON sorted
//! by code; the rendered matrix is committed at `generated/rdf-loss-matrix.json`
//! and a drift gate ([`tests`]) re-derives and compares it.

/// One enumerated, intentional conversion loss between two representations.
///
/// Entries are `&'static` because the ledger is a compiled-in contract, not
/// runtime data: every code is a stable, reviewed promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LossEntry {
    /// Stable machine code, kebab-case (e.g. `direction-dropped`).
    pub code: &'static str,
    /// Source representation (e.g. `"rdf-1.2-dataset"`).
    pub from: &'static str,
    /// Target representation (e.g. `"gts"`).
    pub to: &'static str,
    /// `true` = a known, accepted conversion loss (the only kind this ledger
    /// records). A `false` value would mark an *unintentional* loss, which the
    /// fidelity gate treats as a bug rather than a documented contract.
    pub intentional: bool,
    /// Human-readable explanation of what is dropped and why.
    pub note: &'static str,
}

/// An ordered, deterministic set of [`LossEntry`] for one conversion direction
/// (or the combined matrix).
///
/// Entries are kept sorted by `code` so every render is byte-identical regardless
/// of construction order. Codes are unique within a ledger.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LossLedger {
    entries: Vec<LossEntry>,
}

impl LossLedger {
    /// Build a ledger from arbitrary entries, sorting by `code` for determinism.
    ///
    /// Panics on a duplicate `code`: the ledger is a compiled-in contract and a
    /// collision is a programming error (hard-fail, per the no-optionality
    /// doctrine), not a runtime condition to tolerate.
    fn from_entries(mut entries: Vec<LossEntry>) -> Self {
        entries.sort_by(|a, b| a.code.cmp(b.code));
        for pair in entries.windows(2) {
            assert_ne!(
                pair[0].code, pair[1].code,
                "duplicate loss code `{}` in ledger",
                pair[0].code
            );
        }
        Self { entries }
    }

    /// The ledger entries, sorted by `code`.
    pub fn entries(&self) -> &[LossEntry] {
        &self.entries
    }

    /// `true` when no losses are recorded. Fidelity is asserted only here.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Render the ledger as deterministic JSON: a sorted-by-code array of
    /// objects, 2-space indented, with a trailing newline.
    pub fn render_json(&self) -> String {
        render_entries(&self.entries)
    }
}

/// The intentional losses incurred projecting the RDF 1.2 dataset IR → GTS.
pub fn rdf_to_gts_loss_ledger() -> LossLedger {
    LossLedger::from_entries(vec![
        LossEntry {
            code: "direction-dropped",
            from: "rdf-1.2-dataset",
            to: "gts",
            intentional: true,
            note: "The GTS term schema has no literal base-direction field, so the writer \
                   drops a literal's base direction (`gmeow_gts` stores only lexical form, \
                   datatype, and language tag).",
        },
        LossEntry {
            code: "multi-reifier-collapsed",
            from: "rdf-1.2-dataset",
            to: "gts",
            intentional: true,
            note: "RDF 1.2 permits several explicit reifiers for one (s,p,o), but the current \
                   GTS writer rejects a second distinct explicit reifier for the same triple \
                   content (`rdf-conflicting-reifier`).",
        },
        LossEntry {
            code: "blob-bytes-absent",
            from: "rdf-1.2-dataset",
            to: "gts",
            intentional: true,
            note: "`RdfLookaside` blob records carry blob metadata but not the actual blob \
                   bytes, so the GTS writer cannot preserve blob payloads.",
        },
    ])
}

/// The intentional losses incurred reading GTS → the RDF 1.2 dataset IR.
pub fn gts_to_rdf_loss_ledger() -> LossLedger {
    LossLedger::from_entries(vec![LossEntry {
        code: "bnode-scope-flatten",
        from: "gts",
        to: "rdf-1.2-dataset",
        intentional: true,
        note: "`gmeow_gts::reader::read()` folds all segments into one term table, collapsing \
               per-segment blank-node scope; the distinct scopes are recovered only via the \
               streaming-event importer.",
    }])
}

/// The combined RDF↔GTS matrix as a single deterministic, sorted-by-code JSON
/// array — the body of the generated `generated/rdf-loss-matrix.json` artifact.
pub fn loss_matrix_json() -> String {
    let mut entries: Vec<LossEntry> = Vec::new();
    entries.extend_from_slice(rdf_to_gts_loss_ledger().entries());
    entries.extend_from_slice(gts_to_rdf_loss_ledger().entries());
    LossLedger::from_entries(entries).render_json()
}

/// Render a sorted slice of entries to deterministic JSON.
///
/// Hand-rolled to avoid pulling serde into the kernel rlib (the crate does not
/// depend on it). Fields are emitted in a fixed order; strings are JSON-escaped.
fn render_entries(entries: &[LossEntry]) -> String {
    let mut out = String::from("[\n");
    for (i, entry) in entries.iter().enumerate() {
        out.push_str("  {\n");
        push_field(&mut out, "code", entry.code, false);
        push_field(&mut out, "from", entry.from, false);
        push_field(&mut out, "to", entry.to, false);
        push_bool_field(&mut out, "intentional", entry.intentional);
        push_field(&mut out, "note", entry.note, true);
        out.push_str("  }");
        if i + 1 < entries.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("]\n");
    out
}

/// Append `  "key": "value",\n` (or no trailing comma when `last`).
fn push_field(out: &mut String, key: &str, value: &str, last: bool) {
    out.push_str("    \"");
    out.push_str(key);
    out.push_str("\": \"");
    escape_json_into(out, value);
    out.push('"');
    if !last {
        out.push(',');
    }
    out.push('\n');
}

/// Append `  "key": true|false,\n` (booleans are never the last field here).
fn push_bool_field(out: &mut String, key: &str, value: bool) {
    out.push_str("    \"");
    out.push_str(key);
    out.push_str("\": ");
    out.push_str(if value { "true" } else { "false" });
    out.push_str(",\n");
}

/// Escape a string per the JSON string grammar (RFC 8259) into `out`.
fn escape_json_into(out: &mut String, value: &str) {
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The four intentional loss codes this ledger is required to enumerate.
    const EXPECTED_CODES: [&str; 4] = [
        "blob-bytes-absent",
        "bnode-scope-flatten",
        "direction-dropped",
        "multi-reifier-collapsed",
    ];

    fn matrix_codes() -> Vec<String> {
        let mut codes: Vec<String> = Vec::new();
        for entry in rdf_to_gts_loss_ledger().entries() {
            codes.push(entry.code.to_string());
        }
        for entry in gts_to_rdf_loss_ledger().entries() {
            codes.push(entry.code.to_string());
        }
        codes.sort();
        codes
    }

    #[test]
    fn render_is_deterministic() {
        assert_eq!(loss_matrix_json(), loss_matrix_json());
        let ledger = rdf_to_gts_loss_ledger();
        assert_eq!(ledger.render_json(), ledger.render_json());
    }

    #[test]
    fn all_four_intentional_codes_present() {
        let codes = matrix_codes();
        for expected in EXPECTED_CODES {
            assert!(
                codes.iter().any(|c| c == expected),
                "missing intentional loss code `{expected}`"
            );
        }
        assert_eq!(codes.len(), EXPECTED_CODES.len(), "unexpected extra codes");
    }

    #[test]
    fn every_recorded_loss_is_intentional() {
        for entry in rdf_to_gts_loss_ledger().entries() {
            assert!(entry.intentional, "{} not marked intentional", entry.code);
        }
        for entry in gts_to_rdf_loss_ledger().entries() {
            assert!(entry.intentional, "{} not marked intentional", entry.code);
        }
    }

    #[test]
    fn directions_are_correct() {
        for entry in rdf_to_gts_loss_ledger().entries() {
            assert_eq!(entry.from, "rdf-1.2-dataset");
            assert_eq!(entry.to, "gts");
        }
        for entry in gts_to_rdf_loss_ledger().entries() {
            assert_eq!(entry.from, "gts");
            assert_eq!(entry.to, "rdf-1.2-dataset");
        }
    }

    #[test]
    fn is_empty_reflects_contents() {
        assert!(!rdf_to_gts_loss_ledger().is_empty());
        assert!(!gts_to_rdf_loss_ledger().is_empty());
        assert!(LossLedger::default().is_empty());
        assert!(LossLedger::from_entries(vec![]).is_empty());
    }

    #[test]
    fn json_is_structurally_valid() {
        let json = loss_matrix_json();
        // Deterministic shape: a JSON array with a trailing newline.
        assert!(json.starts_with("[\n"));
        assert!(json.ends_with("]\n"));
        // One object per intentional code, with each field key present once.
        assert_eq!(json.matches("\"code\":").count(), EXPECTED_CODES.len());
        assert_eq!(
            json.matches("\"intentional\": true").count(),
            EXPECTED_CODES.len()
        );
        for code in EXPECTED_CODES {
            assert!(
                json.contains(&format!("\"code\": \"{code}\"")),
                "missing {code}"
            );
        }
        // Sorted-by-code: codes appear in ascending order in the rendered text.
        let mut last = 0usize;
        for code in EXPECTED_CODES {
            let at = json.find(code).expect("code present");
            assert!(at >= last, "codes not sorted: {code}");
            last = at;
        }
    }

    #[test]
    fn json_escapes_control_characters() {
        let mut s = String::new();
        escape_json_into(&mut s, "a\"b\\c\nd\te\u{01}");
        assert_eq!(s, "a\\\"b\\\\c\\nd\\te\\u0001");
    }

    /// Drift gate: the committed artifact must byte-equal the freshly rendered
    /// matrix. Regenerate `generated/rdf-loss-matrix.json` when the ledger
    /// changes.
    #[test]
    fn generated_artifact_has_not_drifted() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("generated")
            .join("rdf-loss-matrix.json");
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert_eq!(
            committed,
            loss_matrix_json(),
            "generated/rdf-loss-matrix.json is stale; regenerate it from loss_matrix_json()"
        );
    }
}
