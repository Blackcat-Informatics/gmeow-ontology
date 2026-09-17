// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! W3C test-manifest ingestion.
//!
//! Reads a W3C test `manifest.ttl` (dogfooding the native `purrdf::parse_dataset`
//! Turtle codec — never a second parser) and extracts test entries from BOTH the
//! DAWG `mf:` vocabulary and the OWL 2 `otest:` vocabulary. Each kind maps to a
//! normalized [`ExternalOutcome`] via the shared [`crate::external::status`] table.
//!
//! Supported `mf:` types:
//! - `mf:PositiveEntailment` → `ExternalOutcome::Inconsistent`
//! - `mf:NegativeEntailment` → `ExternalOutcome::Consistent`
//!
//! Supported `otest:` types:
//! - `otest:PositiveEntailmentTest` → `ExternalOutcome::Inconsistent`
//! - `otest:NegativeEntailmentTest` → `ExternalOutcome::Consistent`
//! - `otest:ConsistencyTest`        → `ExternalOutcome::Consistent`
//! - `otest:InconsistencyTest`      → `ExternalOutcome::Inconsistent`
//!
//! Entailment-reduction caveat: `A ⊨ C` iff `A ∪ ¬C` is unsatisfiable. A
//! `PositiveEntailment` therefore lowers to `Inconsistent` ONLY once the negated
//! conclusion is folded into the EDB. For the self-authored seed corpus (User
//! Decision 1) that negation is pre-baked into the lowered `input.nq`; this parser's
//! job is solely to read the manifest and report the declared outcome (the external
//! ground truth the soundness gate cross-checks the engine against).

use gmeow_errors::Diag;
use purrdf::{TermRef, parse_dataset};

use crate::error::{ManifestInvalid, ManifestParse};
use crate::external::status::ExternalOutcome;

/// The W3C test-manifest (`mf:`) vocabulary namespace.
const MF: &str = "http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#";
/// The W3C OWL 2 test ontology (`otest:`) vocabulary namespace.
const OTEST: &str = "http://www.w3.org/2007/OWL/testOntology#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// The kind of test declared by a manifest entry, spanning both the DAWG `mf:`
/// vocabulary and the OWL 2 `otest:` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestTestKind {
    /// `mf:PositiveEntailment` or `otest:PositiveEntailmentTest` — the premises
    /// entail the conclusion; `premises ∧ ¬conclusion` is unsatisfiable.
    PositiveEntailment,
    /// `mf:NegativeEntailment` or `otest:NegativeEntailmentTest` — the premises do
    /// NOT entail the conclusion; a counter-model exists.
    NegativeEntailment,
    /// `otest:ConsistencyTest` — the ontology has at least one model.
    Consistency,
    /// `otest:InconsistencyTest` — the ontology has no model.
    Inconsistency,
}

impl ManifestTestKind {
    /// The normalized outcome this test kind declares.
    pub fn outcome(self) -> ExternalOutcome {
        match self {
            // Entailment holds ⇒ premises ∧ ¬conclusion is unsatisfiable.
            ManifestTestKind::PositiveEntailment => ExternalOutcome::Inconsistent,
            // Entailment fails ⇒ premises ∧ ¬conclusion has a model.
            ManifestTestKind::NegativeEntailment => ExternalOutcome::Consistent,
            // At least one model exists.
            ManifestTestKind::Consistency => ExternalOutcome::Consistent,
            // No model exists.
            ManifestTestKind::Inconsistency => ExternalOutcome::Inconsistent,
        }
    }
}

/// A premise or conclusion document carried in a manifest entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OntologyDoc {
    /// `mf:action` / `mf:result` — an IRI reference to a sibling document.
    Reference(String),
    /// `otest:rdfXmlPremiseOntology` / `otest:rdfXmlConclusionOntology` — inline
    /// RDF/XML literal content (parseable by purrdf via `application/rdf+xml`).
    InlineRdfXml(String),
}

/// One test entry extracted from a `manifest.ttl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// The test subject IRI.
    pub iri: String,
    /// The test name: `mf:name`, then `rdfs:label`, then `otest:identifier`, then
    /// the subject IRI (first present wins).
    pub name: String,
    /// The test kind.
    pub kind: ManifestTestKind,
    /// Whether the entry ALSO declares a positive-entailment test type
    /// (`otest:PositiveEntailmentTest` / `mf:PositiveEntailment`) in addition to its
    /// primary [`kind`](Self::kind). The W3C OWL 2 `rdfbased-sem-*` metamodeling cases
    /// are dual-typed `PositiveEntailmentTest + ConsistencyTest`: their consistency
    /// premise is empty (`<rdf:RDF/>`) and the real content is the entailment
    /// conclusion, so an empty-premise dual-typed entry is graded as an entailment
    /// case, not a vacuous consistency case. Order-independent (set from the presence
    /// of the type quad, regardless of which type wins `kind`).
    pub also_positive_entailment: bool,
    /// The premise document. `None` when neither `mf:action` IRI nor
    /// `otest:rdfXmlPremiseOntology` literal is present (e.g. a functional-syntax-only
    /// entry in a real-world W3C corpus). [`parse_test_manifest`] hard-fails on such
    /// entries; [`parse_test_manifest_rdfxml`] silently drops them.
    pub action: Option<OntologyDoc>,
    /// The conclusion document, when present.
    pub result: Option<OntologyDoc>,
}

impl ManifestEntry {
    /// The normalized external outcome this entry declares.
    pub fn outcome(&self) -> ExternalOutcome {
        self.kind.outcome()
    }
}

/// Parse a W3C `manifest.ttl` and return every test entry, sorted by subject IRI
/// (deterministic order).
///
/// Non-test manifest entries (syntax/eval tests, etc.) are ignored. Hard-fail
/// (no-optionality): a Turtle parse failure, a recognized test entry missing its
/// required premise document, or an inline RDF/XML literal that is empty or
/// whitespace-only are all errors.
pub fn parse_test_manifest(
    source: &str,
    base: Option<&str>,
) -> gmeow_errors::Result<Vec<ManifestEntry>> {
    let ds = parse_dataset(source.as_bytes(), "text/turtle", base).map_err(|e| {
        Diag::of_kind(ManifestParse {
            detail: format!("manifest Turtle parse failed: {e}"),
        })
    })?;
    let entries = manifest_entries(&ds)?;
    // Strict post-pass: every recognized entry MUST have a premise. The self-authored
    // seed corpora always satisfy this; a missing premise is a manifest authoring error.
    for e in &entries {
        if e.action.is_none() {
            return Err(Diag::of_kind(ManifestInvalid {
                detail: format!(
                    "manifest test entry {} has no premise document \
                     (no mf:action IRI or otest:rdfXmlPremiseOntology literal)",
                    e.iri
                ),
            }));
        }
    }
    Ok(entries)
}

/// Parse a W3C manifest from RDF/XML source and return every test entry, sorted by
/// subject IRI (deterministic order).
///
/// Similar to [`parse_test_manifest`] but accepts `application/rdf+xml` input.
/// Entries without a recognized premise document are SILENTLY DROPPED (not a hard
/// error): real-world W3C corpora contain entries with only a functional-syntax premise
/// (`otest:fsPremiseOntology`) that this crate cannot parse; skipping them is correct
/// behaviour for the vendor step, which logs the skip itself.
pub fn parse_test_manifest_rdfxml(
    source: &str,
    base: Option<&str>,
) -> gmeow_errors::Result<Vec<ManifestEntry>> {
    let ds = parse_dataset(source.as_bytes(), "application/rdf+xml", base).map_err(|e| {
        Diag::of_kind(ManifestParse {
            detail: format!("manifest RDF/XML parse failed: {e}"),
        })
    })?;
    // Lenient: keep only entries with a recognized premise; drop the rest (the
    // caller — the vendor step — logs skipped cases itself).
    Ok(manifest_entries(&ds)?
        .into_iter()
        .filter(|e| e.action.is_some())
        .collect())
}

/// Extract manifest entries from an already-parsed [`purrdf::RdfDataset`].
///
/// This is the shared quad-walk logic used by both [`parse_test_manifest`] (Turtle
/// input) and [`parse_test_manifest_rdfxml`] (RDF/XML input). Returns entries sorted
/// by subject IRI for deterministic order.
///
/// Entries whose premise is an empty or whitespace-only inline RDF/XML literal are a
/// hard error (vacuous pass not permitted). Entries with no recognized premise document
/// at all (no `mf:action` IRI and no `otest:rdfXmlPremiseOntology` literal) are
/// returned with `action = None`; the caller decides whether to hard-fail or skip
/// them. [`parse_test_manifest`] hard-fails on such entries; [`parse_test_manifest_rdfxml`]
/// silently drops them.
pub fn manifest_entries(ds: &purrdf::RdfDataset) -> gmeow_errors::Result<Vec<ManifestEntry>> {
    #[derive(Default)]
    struct Row {
        kind: Option<ManifestTestKind>,
        /// Whether a positive-entailment type quad was seen for this subject
        /// (independent of which type wins `kind` — dual-typed entries declare both).
        saw_positive_entailment: bool,
        /// `mf:name` (highest precedence name)
        mf_name: Option<String>,
        /// `rdfs:label` (second precedence name)
        rdfs_label: Option<String>,
        /// `otest:identifier` (third precedence name)
        otest_identifier: Option<String>,
        /// `mf:action` IRI reference premise
        mf_action: Option<String>,
        /// `mf:result` IRI reference conclusion
        mf_result: Option<String>,
        /// `otest:rdfXmlPremiseOntology` inline RDF/XML premise
        otest_premise: Option<String>,
        /// `otest:rdfXmlConclusionOntology` inline RDF/XML conclusion
        otest_conclusion: Option<String>,
    }
    let mut rows: std::collections::BTreeMap<String, Row> = std::collections::BTreeMap::new();

    let type_mf_pos = format!("{MF}PositiveEntailment");
    let type_mf_neg = format!("{MF}NegativeEntailment");
    let type_otest_pos = format!("{OTEST}PositiveEntailmentTest");
    let type_otest_neg = format!("{OTEST}NegativeEntailmentTest");
    let type_otest_con = format!("{OTEST}ConsistencyTest");
    let type_otest_inc = format!("{OTEST}InconsistencyTest");

    let p_mf_name = format!("{MF}name");
    let p_mf_action = format!("{MF}action");
    let p_mf_result = format!("{MF}result");
    let p_otest_identifier = format!("{OTEST}identifier");
    let p_otest_premise = format!("{OTEST}rdfXmlPremiseOntology");
    let p_otest_conclusion = format!("{OTEST}rdfXmlConclusionOntology");

    for q in ds.quad_refs() {
        let TermRef::Iri(subj) = q.s else { continue };
        let TermRef::Iri(pred) = q.p else { continue };
        let row = rows.entry(subj.to_owned()).or_default();

        if pred == RDF_TYPE {
            if let TermRef::Iri(t) = q.o {
                if t == type_mf_pos || t == type_otest_pos {
                    row.kind = Some(ManifestTestKind::PositiveEntailment);
                    row.saw_positive_entailment = true;
                } else if t == type_mf_neg || t == type_otest_neg {
                    row.kind = Some(ManifestTestKind::NegativeEntailment);
                } else if t == type_otest_con {
                    row.kind = Some(ManifestTestKind::Consistency);
                } else if t == type_otest_inc {
                    row.kind = Some(ManifestTestKind::Inconsistency);
                }
            }
        } else if pred == p_mf_name {
            if let TermRef::Literal { lexical, .. } = q.o {
                row.mf_name = Some(lexical.to_owned());
            }
        } else if pred == RDFS_LABEL {
            if let TermRef::Literal { lexical, .. } = q.o {
                row.rdfs_label = Some(lexical.to_owned());
            }
        } else if pred == p_otest_identifier {
            if let TermRef::Literal { lexical, .. } = q.o {
                row.otest_identifier = Some(lexical.to_owned());
            }
        } else if pred == p_mf_action {
            if let TermRef::Iri(a) = q.o {
                row.mf_action = Some(a.to_owned());
            }
        } else if pred == p_mf_result {
            if let TermRef::Iri(r) = q.o {
                row.mf_result = Some(r.to_owned());
            }
        } else if pred == p_otest_premise {
            if let TermRef::Literal { lexical, .. } = q.o {
                row.otest_premise = Some(lexical.to_owned());
            }
        } else if pred == p_otest_conclusion
            && let TermRef::Literal { lexical, .. } = q.o
        {
            row.otest_conclusion = Some(lexical.to_owned());
        }
    }

    let mut entries = Vec::new();
    for (iri, row) in rows {
        // Only recognized test entries are in scope; skip everything else.
        let Some(kind) = row.kind else { continue };

        // Name: mf:name > rdfs:label > otest:identifier > subject IRI.
        let name = row
            .mf_name
            .or(row.rdfs_label)
            .or(row.otest_identifier)
            .unwrap_or_else(|| iri.clone());

        // Premise (action): prefer otest:rdfXmlPremiseOntology inline literal, then
        // mf:action IRI reference. If neither is present, return `None`; the caller
        // decides whether to hard-fail or skip the entry.
        let action: Option<OntologyDoc> = if let Some(inline) = row.otest_premise {
            if inline.trim().is_empty() {
                return Err(Diag::of_kind(ManifestInvalid {
                    detail: format!(
                        "manifest entry {iri} has an empty otest:rdfXmlPremiseOntology literal \
                         (vacuous pass not permitted)"
                    ),
                }));
            }
            Some(OntologyDoc::InlineRdfXml(inline))
        } else {
            row.mf_action.map(OntologyDoc::Reference)
        };

        // Conclusion (result): prefer otest:rdfXmlConclusionOntology inline literal,
        // then mf:result IRI reference. Optional.
        let result: Option<OntologyDoc> = if let Some(inline) = row.otest_conclusion {
            Some(OntologyDoc::InlineRdfXml(inline))
        } else {
            row.mf_result.map(OntologyDoc::Reference)
        };

        entries.push(ManifestEntry {
            iri,
            name,
            kind,
            also_positive_entailment: row.saw_positive_entailment,
            action,
            result,
        });
    }
    Ok(entries)
}

#[path = "manifest.tests.rs"]
#[cfg(test)]
mod tests;
