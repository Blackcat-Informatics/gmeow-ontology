// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! W3C `manifest.ttl` entailment-test ingestion (#753).
//!
//! Reads a W3C test `manifest.ttl` (dogfooding the native `gmeow_rdf::parse_dataset`
//! Turtle codec — never a second parser) and extracts the entailment entries:
//! `mf:PositiveEntailment` / `mf:NegativeEntailment` nodes with their `mf:name`,
//! `mf:action` (premise document) and `mf:result` (conclusion document). Each kind
//! maps to a normalized [`ExternalOutcome`] via the shared
//! [`crate::external::status`] table.
//!
//! Entailment-reduction caveat: `A ⊨ C` iff `A ∪ ¬C` is unsatisfiable. A
//! `PositiveEntailment` therefore lowers to `Inconsistent` ONLY once the negated
//! conclusion is folded into the EDB. For the self-authored seed corpus (#753 User
//! Decision 1) that negation is pre-baked into the lowered `input.nq`; this parser's
//! job is solely to read the manifest and report the declared outcome (the external
//! ground truth the soundness gate cross-checks the engine against).

use gmeow_rdf::{parse_dataset, TermRef};

use crate::external::status::ExternalOutcome;

/// The W3C test-manifest vocabulary namespace.
const MF: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Whether a manifest entry asserts the entailment holds or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntailmentKind {
    /// `mf:PositiveEntailment` — the premises entail the conclusion.
    Positive,
    /// `mf:NegativeEntailment` — the premises do NOT entail the conclusion.
    Negative,
}

impl EntailmentKind {
    /// The normalized outcome this entailment kind declares.
    pub fn outcome(self) -> ExternalOutcome {
        match self {
            // Entailment holds ⇒ premises ∧ ¬conclusion is unsatisfiable.
            EntailmentKind::Positive => ExternalOutcome::Inconsistent,
            // Entailment fails ⇒ premises ∧ ¬conclusion has a model.
            EntailmentKind::Negative => ExternalOutcome::Consistent,
        }
    }
}

/// One entailment test extracted from a `manifest.ttl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// The test subject IRI.
    pub iri: String,
    /// The `mf:name` label (falls back to the subject IRI when absent).
    pub name: String,
    /// Positive vs negative entailment.
    pub kind: EntailmentKind,
    /// The `mf:action` IRI (premise document).
    pub action: String,
    /// The `mf:result` IRI (conclusion document), when present.
    pub result: Option<String>,
}

impl ManifestEntry {
    /// The normalized external outcome this entry declares.
    pub fn outcome(&self) -> ExternalOutcome {
        self.kind.outcome()
    }
}

/// Parse a W3C `manifest.ttl` and return every entailment entry, sorted by subject
/// IRI (deterministic order).
///
/// Non-entailment manifest entries (syntax/eval tests, etc.) are ignored. Hard-fail
/// (no-optionality): a Turtle parse failure, or an entailment entry missing its
/// required `mf:action`, is an error.
pub fn parse_entailment_manifest(
    source: &str,
    base: Option<&str>,
) -> Result<Vec<ManifestEntry>, String> {
    let ds = parse_dataset(source.as_bytes(), "text/turtle", base)
        .map_err(|e| format!("manifest Turtle parse failed: {e}"))?;

    #[derive(Default)]
    struct Row {
        kind: Option<EntailmentKind>,
        name: Option<String>,
        action: Option<String>,
        result: Option<String>,
    }
    let mut rows: std::collections::BTreeMap<String, Row> = std::collections::BTreeMap::new();

    let type_pos = format!("{MF}PositiveEntailment");
    let type_neg = format!("{MF}NegativeEntailment");
    let p_name = format!("{MF}name");
    let p_action = format!("{MF}action");
    let p_result = format!("{MF}result");

    for q in ds.quad_refs() {
        let TermRef::Iri(subj) = q.s else { continue };
        let TermRef::Iri(pred) = q.p else { continue };
        let row = rows.entry(subj.to_owned()).or_default();
        if pred == RDF_TYPE {
            if let TermRef::Iri(t) = q.o {
                if t == type_pos {
                    row.kind = Some(EntailmentKind::Positive);
                } else if t == type_neg {
                    row.kind = Some(EntailmentKind::Negative);
                }
            }
        } else if pred == p_name {
            if let TermRef::Literal { lexical, .. } = q.o {
                row.name = Some(lexical.to_owned());
            }
        } else if pred == p_action {
            if let TermRef::Iri(a) = q.o {
                row.action = Some(a.to_owned());
            }
        } else if pred == p_result {
            if let TermRef::Iri(r) = q.o {
                row.result = Some(r.to_owned());
            }
        }
    }

    let mut entries = Vec::new();
    for (iri, row) in rows {
        // Only entailment entries are in scope; skip everything else.
        let Some(kind) = row.kind else { continue };
        let action = row
            .action
            .ok_or_else(|| format!("manifest entailment entry {iri} has no mf:action"))?;
        let name = row.name.unwrap_or_else(|| iri.clone());
        entries.push(ManifestEntry {
            iri,
            name,
            kind,
            action,
            result: row.result,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "\
@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:pos a mf:PositiveEntailment ;\n\
    mf:name \"clash-entails\" ;\n\
    mf:action ex:premise.nq ;\n\
    mf:result ex:conclusion.nq .\n\
ex:neg a mf:NegativeEntailment ;\n\
    mf:name \"no-entailment\" ;\n\
    mf:action ex:open.nq .\n";

    #[test]
    fn extracts_positive_and_negative_entries() {
        let entries = parse_entailment_manifest(MANIFEST, None).unwrap();
        assert_eq!(entries.len(), 2);

        let pos = entries.iter().find(|e| e.name == "clash-entails").unwrap();
        assert_eq!(pos.kind, EntailmentKind::Positive);
        assert_eq!(pos.outcome(), ExternalOutcome::Inconsistent);
        assert!(pos.action.ends_with("premise.nq"));
        assert!(pos.result.as_deref().unwrap().ends_with("conclusion.nq"));

        let neg = entries.iter().find(|e| e.name == "no-entailment").unwrap();
        assert_eq!(neg.kind, EntailmentKind::Negative);
        assert_eq!(neg.outcome(), ExternalOutcome::Consistent);
        assert!(neg.result.is_none());
    }

    #[test]
    fn ignores_non_entailment_entries() {
        let src = "\
@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:syntax a mf:PositiveSyntax ; mf:action ex:a.ttl .\n";
        assert!(parse_entailment_manifest(src, None).unwrap().is_empty());
    }

    #[test]
    fn entailment_entry_missing_action_hard_fails() {
        let src = "\
@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:pos a mf:PositiveEntailment ; mf:name \"x\" .\n";
        let err = parse_entailment_manifest(src, None).unwrap_err();
        assert!(err.contains("no mf:action"), "{err}");
    }

    #[test]
    fn malformed_turtle_hard_fails() {
        assert!(parse_entailment_manifest("@prefix bad <", None).is_err());
    }
}
