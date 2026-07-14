// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Thin CLI-glue confirmations over ALREADY-native authorities.
//!
//! The Python surfaces `up_projection_audit.py`, `mappings.py`, `gts_views.py`
//! (`export_docs_site`) is a thin glue over Rust that
//! already exists. This module exposes each confirmed native authority under one
//! stable call name so a `gmeow` / `gmeow-dev` binary can invoke it directly — it
//! duplicates NONE of the native logic; every function delegates.

use std::path::Path;

/// The up-projection invertibility audit — the gate-derived verdict ledger plus its
/// rendered Markdown report.
///
/// Confirms and exposes the native authority behind `up_projection_audit.py`:
/// [`crate::up_projection_gates::gate_derived_audit`] +
/// [`crate::up_projection_report::render_audit_markdown`]. No count is computed here
/// (the Python glue computed none either); the corpus Turtle→N-Triples conversion is
/// the native [`crate::up_projection_corpus::ttl_to_nt`]. Inputs are passed in by the
/// caller (SSSOM texts, projection TTLs, `(name, turtle)` corpus), keeping the
/// driver consumer-safe.
///
/// Returns the gate-verdict [`AuditLedger`](crate::up_projection_gates::AuditLedger)
/// alongside the rendered Markdown.
///
/// # Errors
///
/// - A corpus Turtle fails to convert, or the gate audit fails.
pub fn up_projection_gate_audit(
    sssom_texts: &[String],
    projection_ttls: &[String],
    corpus_ttls: &[(String, String)],
) -> Result<(crate::up_projection_gates::AuditLedger, String), gmeow_errors::Diag> {
    let mut corpus_nts = Vec::with_capacity(corpus_ttls.len());
    for (name, ttl) in corpus_ttls {
        let nt = crate::up_projection_corpus::ttl_to_nt(ttl).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "up-projection-audit".to_string(),
                message: format!("corpus {name} ttl→nt: {e}"),
            })
        })?;
        corpus_nts.push((name.clone(), nt));
    }
    let ledger =
        crate::up_projection_gates::gate_derived_audit(sssom_texts, projection_ttls, &corpus_nts)
            .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "up-projection-audit".to_string(),
                message: e.to_string(),
            })
        })?;
    let markdown = crate::up_projection_report::render_audit_markdown(&ledger);
    Ok((ledger, markdown))
}

/// Compile every mapping family (SSSOM + FnO + EDOAL + SPARQL + standpoint) plus the
/// DSL surface-count summary and the per-correspondence loss ledger from `root`.
///
/// Confirms and exposes the native authority behind `mappings.py`, which is
/// superseded: [`crate::stages::mappings::compile_mappings`]. The Rust stage owns all
/// five families and the loss ledger; this is a one-name pass-through so a bin need
/// not reach into `stages::mappings` directly.
///
/// # Errors
///
/// - Any mapping compile failure (prefix-consistency / purity gate, lowering error).
pub fn compile_mappings(
    root: &Path,
) -> Result<crate::stages::mappings::CompiledMappings, gmeow_errors::Diag> {
    crate::stages::mappings::compile_mappings(root)
}

/// Summarise every documented VOCABULARY term (class / property) in a folded GTS
/// snapshot as `(curie, label, definition)`.
///
/// Confirms and exposes the native authority behind `bundle_term_summaries`:
/// [`crate::stages::export::FoldView`] + [`crate::stages::export::collect_terms`]. A
/// term with an empty `label` / `definition` is a "missing" one for the source-free
/// bundle checks (`gmeow verify`). No logic is duplicated here — this is a one-name
/// pass-through so a `gmeow` / `gmeow-dev` binary need not reach into `stages::export`.
///
/// `collect_terms` also folds every `individual` (any instance of an in-namespace
/// class — content-addressed evidence records like `gmeow:conformance-comparison/*`,
/// `gmeow:rule/*`, `gmeow:verify-attestation/*`, self-description subjects, …), which
/// this check deliberately EXCLUDES: those are auto-minted DATA, not curated
/// vocabulary, and requiring a human-authored `rdfs:label`/`skos:definition` on every
/// one would be a category error (and would make the label/definition completeness
/// check permanently red on a healthy bundle, since evidence records are, by
/// definition, never hand-documented). The "term catalog" contract is over the
/// documented surface — classes and properties — matching the `gmeow-classes.csv` /
/// `gmeow-properties.csv` export split.
///
/// # Errors
///
/// - The snapshot cannot be folded into the carrier dataset.
pub fn bundle_term_summaries(gts_bytes: &[u8]) -> Result<Vec<TermSummary>, gmeow_errors::Diag> {
    let dataset = purrdf::gts::flattened_dataset_from_bytes(gts_bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "bundle-checks".to_string(),
            message: format!("fold gts snapshot: {e}"),
        })
    })?;
    let view = crate::stages::export::FoldView::new(&dataset);
    let terms = crate::stages::export::collect_terms(&view);
    Ok(terms
        .into_iter()
        .filter(|t| t.category == "class" || t.category == "property")
        .map(|t| TermSummary {
            iri: t.iri,
            curie: t.curie,
            label: t.label,
            definition: t.definition,
        })
        .collect())
}

/// One documented bundle term's completeness summary — its full IRI (the identity
/// anchor a `gmeow verify` ontology finding carries as its `documented_terms`
/// join key), its CURIE (the human-facing name in the finding message), and its
/// `rdfs:label` / `skos:definition` (empty when absent — the "missing" signal the
/// verify completeness checks key on).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermSummary {
    /// The term's full IRI — the `documented_terms` join key.
    pub iri: String,
    /// The term's CURIE — the human-facing name in a finding message.
    pub curie: String,
    /// The term's `rdfs:label` (empty when absent).
    pub label: String,
    /// The term's `skos:definition` (empty when absent).
    pub definition: String,
}

/// Render every flat consumer export view from a folded GTS snapshot and write each to
/// `out_dir`, returning the written paths (sorted).
///
/// Confirms and exposes the native authority behind the retired Python
/// `gmeow_tools.export` orchestration:
/// [`crate::stages::export::render_all_with_languages`] (honoring the requested public
/// BCP-47 `languages`; an empty list falls back to `["en"]`). Each artifact is keyed by
/// its canonical `dist/<basename>` name; this writes `<out_dir>/<basename>`. No logic is
/// duplicated here — every rendered byte comes from the native export stage.
///
/// # Errors
///
/// - The snapshot cannot be folded, an export view fails to render, `out_dir` cannot be
///   created, or an artifact cannot be written.
pub fn export_views(
    gts_bytes: &[u8],
    out_dir: &Path,
    languages: &[String],
) -> Result<Vec<String>, gmeow_errors::Diag> {
    let dataset = purrdf::gts::flattened_dataset_from_bytes(gts_bytes).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export".to_string(),
            message: format!("fold gts snapshot: {e}"),
        })
    })?;
    let langs: Vec<String> = if languages.is_empty() {
        vec!["en".to_string()]
    } else {
        languages.to_vec()
    };
    // The JSON Schema `$defs` key set folded into THIS bundle's `schemas-archive`
    // blob — the model-existence signal `llms-full.txt`'s cards gate their
    // `python_model` link on (see `crate::stages::export::class_is_modeled`), read
    // straight off the bytes already in hand (never a repo disk read: this path is
    // repo-free).
    let modeled_defs = crate::bundle_blobs::Bundle::from_snapshot(gts_bytes)?.modeled_def_keys()?;
    let artifacts =
        crate::stages::export::render_all_with_languages(&dataset, &langs, &modeled_defs)?;
    std::fs::create_dir_all(out_dir).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "export".to_string(),
            message: format!("cannot create {}: {e}", out_dir.display()),
        })
    })?;
    let mut written: Vec<String> = Vec::with_capacity(artifacts.len());
    for (dist_path, bytes) in &artifacts {
        let name = Path::new(dist_path).file_name().ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "export".to_string(),
                message: format!("export artifact {dist_path:?} has no file name"),
            })
        })?;
        let dest = out_dir.join(name);
        std::fs::write(&dest, bytes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "export".to_string(),
                message: format!("cannot write {}: {e}", dest.display()),
            })
        })?;
        written.push(dest.display().to_string());
    }
    written.sort();
    Ok(written)
}

/// Rewrite a Turtle document in native canonical, review-friendly form.
///
/// Confirms and exposes the native authority behind `normalize.canonicalize`:
/// `purrdf::turtle_normalize::canonical_turtle` (the oxigraph-free replacement for
/// rdflib `longturtle`, serialized over the gmeow-rdf IR). `extra_prefixes` supplies
/// the project's standard prefix bindings (only those a file uses are emitted).
///
/// # Errors
///
/// - The Turtle fails to parse.
pub fn canonicalize_turtle(
    bytes: &[u8],
    extra_prefixes: &[(String, String)],
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let canonical =
        purrdf::turtle_normalize::canonical_turtle(bytes, extra_prefixes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "normalize".to_string(),
                message: format!("canonical turtle failed: {e}"),
            })
        })?;
    Ok(canonical.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefixes() -> Vec<(String, String)> {
        crate::stages::superset::rdf_prefixes()
    }

    const SAMPLE_TTL: &str = r#"@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .

<http://example.org/B> a owl:Class ; rdfs:label "B" .
<http://example.org/A> a owl:Class ; rdfs:label "A" .
"#;

    #[test]
    fn normalize_is_idempotent() {
        let once = canonicalize_turtle(SAMPLE_TTL.as_bytes(), &prefixes()).expect("first pass");
        let twice = canonicalize_turtle(&once, &prefixes()).expect("second pass");
        assert_eq!(
            once, twice,
            "canonicalization must be a fixed point (idempotent)"
        );
        // The canonical form is non-empty valid Turtle.
        let text = String::from_utf8(once).expect("utf-8");
        assert!(text.contains("example.org/A"), "content preserved");
        assert!(text.contains("example.org/B"), "content preserved");
    }

    #[test]
    fn up_projection_gate_audit_smoke() {
        // A minimal corpus with no lift rules: the audit runs end to end and renders a
        // Markdown report without panicking (the wiring smoke; the heavy real-corpus
        // audit is exercised by the up_projection_gates own tests).
        let corpus = vec![(
            "smoke".to_string(),
            "@prefix ex: <http://example.org/> .\nex:a ex:p ex:b .\n".to_string(),
        )];
        let (ledger, markdown) =
            up_projection_gate_audit(&[], &[], &corpus).expect("audit runs end to end");
        assert!(!markdown.trim().is_empty(), "a report is rendered");
        // Sanity: the ledger total is the partition sum of its tiers.
        assert_eq!(ledger.total(), ledger.totals.total());
    }
}
