// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared diagnostics-projection renderer: one canonical `gmeow_errors::Report`
//! → the four committed diagnostics artifacts (JSON, SARIF, HTML, and `gmeow:Finding`
//! N-Quads). Both `stage-validate` (SHACL) and `stage-compile-logic` (the logic
//! compiler) route their reports through this single path, so the SARIF surface and
//! the diagnostics named graph are normalized identically no matter which stage
//! produced the findings — one renderer, not a per-stage copy.

use std::collections::BTreeMap;

use gmeow_errors::{Diag, DiagLedger, DiagNode, Report, Severity, StageId, model::Location};
use purrdf::{RdfDiagnostic, RdfLocation, RdfSeverity};

/// The four committed logical paths a diagnostics report renders to.
pub struct DiagnosticsPaths<'a> {
    /// JSON projection path.
    pub json: &'a str,
    /// SARIF projection path.
    pub sarif: &'a str,
    /// HTML projection path.
    pub html: &'a str,
    /// `gmeow:Finding` N-Quads projection path.
    pub rdf: &'a str,
}

/// Map the canonical [`Severity`] to the purrdf ingestion boundary's [`RdfSeverity`]
/// (the inverse of `gmeow_errors::severity_from_rdf`).
fn rdf_severity(severity: Severity) -> RdfSeverity {
    match severity {
        Severity::Error => RdfSeverity::Error,
        Severity::Warning => RdfSeverity::Warning,
        Severity::Note => RdfSeverity::Note,
        Severity::Info => RdfSeverity::Info,
    }
}

/// Project a finding [`Location`] into the LOSSY [`RdfLocation`] the diagnostics RDF
/// carries — path + GTS wire coordinates ONLY. The committed `gmeow:Finding` N-Quads
/// projection (`gmeow_errors::render::to_gmeow_rdf`) emits only these coordinates (no
/// `line`/`column`/`logical`), so the FORWARD `Finding → DiagNode` fold must reproduce
/// exactly the same `RdfLocation` the BACKWARD (render → parse → ingest) path did,
/// making the two ledgers byte-identical (zero golden churn).
fn rdf_location_lossy(location: &Location) -> RdfLocation {
    RdfLocation {
        path: location.path.clone(),
        line: None,
        column: None,
        logical: None,
        gts_term_id: location.gts_term_id.map(|v| v as usize),
        gts_quad_index: location.gts_quad_index.map(|v| v as usize),
        gts_reifier_id: location.gts_reifier_id.map(|v| v as usize),
        gts_frame_index: location.gts_frame_index.map(|v| v as usize),
        gts_segment_index: location.gts_segment_index.map(|v| v as usize),
        subject: None,
    }
}

/// The FORWARD projection of a producer's `gmeow_errors::Report` findings into the
/// pre-lowered [`DiagNode`]s the run-level `DiagLedger` folds — the SINGLE source of
/// the run ledger (the backward RDF→ledger read is gone).
///
/// Each `Finding` becomes an [`RdfDiagnostic`] carrying its severity / code / message
/// and (lossily, via [`rdf_location_lossy`]) its primary location, then goes through
/// the EXACT `Diag::from_rdf` mapping the retired backward path used, attached to a
/// LOCAL ledger attributed to `stage`. The returned nodes are byte-identical to what
/// the backward `graph/diagnostics` ingest produced for the same report, so no shipped
/// artifact or golden changes.
///
/// The report is normalized first (the RDF renderer normalizes too), so the fold is a
/// pure function of the report content, independent of finding insertion order.
/// Findings carry no losses, so every produced node has EMPTY antecedents (asserted).
pub fn finding_nodes(report: &Report, stage: &str) -> Vec<DiagNode> {
    let normalized = report.normalized();
    let mut ledger = DiagLedger::new();
    let stage_id = StageId::new(stage);
    for finding in &normalized.findings {
        let mut rdf_diag = RdfDiagnostic::new(
            rdf_severity(finding.severity),
            finding.code.clone(),
            finding.message.clone(),
        );
        if let Some(location) = finding.primary_location() {
            let rdf_location = rdf_location_lossy(location);
            if !rdf_location.is_empty() {
                rdf_diag = rdf_diag.with_location(rdf_location);
            }
        }
        let parent = Diag::from_rdf(&rdf_diag, &mut ledger, stage_id.clone());
        ledger.attach(parent, stage_id.clone());
    }
    let nodes: Vec<DiagNode> = ledger.emit_sorted().into_iter().cloned().collect();
    for node in &nodes {
        assert!(
            node.antecedents.is_empty(),
            "forward finding node `{}` carries antecedents, but findings have no losses",
            node.code
        );
    }
    nodes
}

fn text_artifact(mut text: String) -> Vec<u8> {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.into_bytes()
}

/// The CANONICAL form of a diagnostics report for self-attestation: exactly the value
/// [`gmeow_errors::render::to_json`] serializes into the committed JSON artifact
/// ([`Report::normalized`]), with the self-referential digest entry `key` removed.
///
/// This function exists so the fold has ONE definition of "the record's content", and so
/// that definition is the RENDERED one. A digest taken over the pre-render report is a
/// digest of a value no consumer ever sees: `to_json` writes `report.normalized()`, which
/// sorts findings by [`gmeow_errors::Finding::sort_key`], sorts each finding's inner
/// vectors, and sorts + DEDUPLICATES rules. A producer that pushes one rule per firing
/// therefore writes fewer rules than it folded, and a producer that appends findings in
/// arrival order writes them in a different order than it folded — either alone makes the
/// writer's digest disagree with every reader's.
fn canonical_record(report: &Report, key: &str) -> Report {
    let mut canonical = report.normalized();
    canonical.metadata.remove(key);
    canonical
}

/// The digest a diagnostics report carries over its OWN recorded content, under the
/// metadata `key` that carries it.
///
/// Folded over the serialized [`canonical_record`] rather than the raw file bytes, so it
/// is invariant to JSON whitespace and key-order rendering while remaining sensitive to
/// every value a consumer reads. `key`'s own entry is excluded for the obvious reason
/// that it cannot digest itself.
///
/// This is the SINGLE fold: [`render_diagnostics_artifacts`] stamps it as the last
/// mutation before the renderers run, and [`verify_record_digest`] recomputes it on the
/// consumer side. Neither has a private copy.
///
/// # Errors
/// If the canonical record fails to serialize — a serde regression, never a data
/// condition.
pub fn record_digest(report: &Report, key: &str) -> Result<String, gmeow_errors::Diag> {
    let canonical = canonical_record(report, key);
    let bytes = serde_json::to_vec(&canonical).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("digesting the recorded {} verdict: {e}", canonical.tool),
        })
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"gmeow-diagnostics-record-v1\x1e");
    hasher.update(key.as_bytes());
    hasher.update(b"\x1e");
    hasher.update(&bytes);
    Ok(format!("blake3:{}", hasher.finalize().to_hex()))
}

/// Admit `recorded` only if it carries a `key` metadata entry AND its content folds to
/// that value — the integrity half of the recorded-verdict contract, paired with the
/// freshness half a caller checks against an input digest.
///
/// `origin` names the record in the diagnostic (its path, for a disk read). It lives
/// beside [`record_digest`] and [`render_diagnostics_artifacts`] so the fold that WRITES
/// the digest and the fold that CHECKS it are literally the same call and cannot drift.
///
/// # Errors
/// If the record carries no `key` metadata (unattestable content), or if its content does
/// not fold to the declared value (edited after production). Neither is a skip and
/// neither is a pass.
pub fn verify_record_digest(
    recorded: &Report,
    key: &str,
    origin: &str,
) -> Result<(), gmeow_errors::Diag> {
    let declared = recorded
        .metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!(
                    "the recorded diagnostics verdict at {origin} carries no {key} metadata, so \
                     its own content cannot be attested — an edited verdict would be \
                     indistinguishable from a produced one. Regenerate it (`make check`)"
                ),
            })
        })?
        .to_owned();
    let live = record_digest(recorded, key)?;
    if live != declared {
        return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!(
                "the recorded diagnostics verdict at {origin} does not match its own {key}: it \
                 declares {declared} but its recorded findings and metadata fold to {live}. The \
                 record has been edited after it was produced — regenerate it (`make check`); an \
                 edited verdict is never accepted"
            ),
        }));
    }
    Ok(())
}

/// The shared native diagnostics publication and its terminal presentation artifacts.
/// Producers carry this dataset directly; the RDF artifact is never an ingress.
pub struct RenderedDiagnostics {
    /// The exact normalized, meta-enriched and selected-seal-bearing value used
    /// by every terminal renderer and native downstream report consumer.
    pub report: std::sync::Arc<Report>,
    /// Terminal JSON, SARIF, HTML and canonical RDF projections.
    pub artifacts: BTreeMap<String, Vec<u8>>,
    /// The exact projected findings plus authored gate and meta-rule conclusions.
    pub dataset: std::sync::Arc<purrdf::RdfDataset>,
}

/// Render the four committed diagnostics projections for `report`, keyed by the
/// supplied `paths`. `stage` names the producing stage for error attribution.
///
/// The reasoner-derived DIAGNOSTIC META-FINDINGS run FIRST (before any renderer):
/// the authored `gmeow:DiagnosticMetaRule` fold (`meta`) reasons the projected
/// finding graph and derives the root-cause / cluster / cross-node-glut
/// meta-findings, which ENRICH the report so the user SEES them on every surface
/// (the JSON serialization, the CLI/HTML text, and the derived `.nq`), not just the
/// graph. json / sarif / html / nq all then render from the ENRICHED report + the
/// derived meta N-Quads. A producer with no meta-rules passes `None` and the
/// projection stays byte-unchanged.
///
/// `seal` names the metadata key under which the report's SELF-digest
/// ([`record_digest`]) is stamped, for a producer whose record a consumer reads back
/// instead of re-running the pass. The stamp is applied HERE — after enrichment, as the
/// last mutation before any renderer runs — because the digest must attest the content
/// that is actually written. A producer whose record nobody reads back passes `None` and
/// its projections are byte-unchanged. Only the JSON projection is affected when it IS
/// stamped: `to_gmeow_rdf` projects findings and ignores metadata entirely, and the SARIF
/// surface reads only the `category` metadata key.
pub fn render_diagnostics_artifacts(
    stage: &str,
    mut report: Report,
    paths: &DiagnosticsPaths<'_>,
    gate: Option<&crate::stages::gate_verdict::GateProgram>,
    meta: Option<&crate::stages::meta_findings::MetaProgram>,
    seal: Option<&str>,
) -> Result<RenderedDiagnostics, gmeow_errors::Diag> {
    let stage_err = |what: &str, detail: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: stage.to_owned(),
            message: format!("render {what} diagnostics: {detail}"),
        })
    };
    // The projected finding graph BEFORE enrichment — the EDB the meta chase reads
    // (the derived root-cause/cluster/glut fields the enrichment adds are ignored by
    // `to_gmeow_rdf`, so this projection is identical to the enriched report's).
    let mut projected = purrdf::RdfDatasetBuilder::new();
    let graph = projected.intern_iri(crate::stages::carrier::GRAPH_DIAGNOSTICS);
    projected.declare_named_graph(graph);
    gmeow_errors::render::append_gmeow_findings(
        &report,
        crate::stages::carrier::GRAPH_DIAGNOSTICS,
        &mut projected,
    );
    let projected = projected
        .freeze()
        .map_err(|e| stage_err("RDF", format!("project native findings: {e}")))?;
    // Run the authored diagnostic meta-rules over the projected graph, then enrich
    // the owned report so every renderer surfaces the meta-findings.
    let derivation = match meta {
        Some(meta) => meta
            .derive_dataset(&projected)
            .map_err(|e| stage_err("meta", e.to_string()))?,
        None => crate::stages::meta_findings::MetaDerivation::default(),
    };
    // The producer transfers ownership once; enrich in place and retain the same
    // complete normalized value instead of discarding it after JSON rendering.
    if !derivation.is_empty() {
        crate::stages::meta_findings::enrich_report(&mut report, &derivation);
    }
    report.normalize();

    // SEAL the verdict: the last mutation before the renderers run. Everything above —
    // the caller's own span enrichment and advisory wings, plus the meta-fold enrichment
    // just applied — is already in the report, so the stamped digest attests EXACTLY the
    // content the committed JSON carries. Folded through `record_digest`, which digests
    // the RENDERED (normalized) form, so the value a consumer recomputes off the parsed
    // file is the value written here.
    if let Some(key) = seal {
        let digest = record_digest(&report, key)?;
        report
            .metadata
            .insert(key.to_owned(), serde_json::Value::String(digest));
    }
    let report = std::sync::Arc::new(report);

    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(
        paths.json.to_owned(),
        text_artifact(
            gmeow_errors::render::to_json(&report).map_err(|e| stage_err("JSON", e.to_string()))?,
        ),
    );
    artifacts.insert(
        paths.sarif.to_owned(),
        text_artifact(
            gmeow_errors::render::to_sarif(&report)
                .map_err(|e| stage_err("SARIF", e.to_string()))?,
        ),
    );
    artifacts.insert(
        paths.html.to_owned(),
        text_artifact(gmeow_errors::render::to_html(&report)),
    );
    // The `.nq` diagnostics graph rides as an RDF-fanout named graph: emit the RDFC-1.0
    // canonical N-Quads (keeping the `graph/diagnostics` 4th-column label) so the
    // superset gate reconstructs it byte-for-byte.
    // Authored rules derive verdicts from the projected grade coordinates. A
    // selected gate always executes, including when its result is empty.
    let gate_dataset = gate
        .map(|gate| {
            gate.derived_verdict_dataset(&projected, crate::stages::carrier::GRAPH_DIAGNOSTICS)
                .map_err(|e| stage_err("RDF", format!("derive gate verdict: {e}")))
        })
        .transpose()?;
    let nq_ds = if derivation.is_empty()
        && gate_dataset.as_ref().is_none_or(|dataset| {
            dataset.rdf_row_count() == 0 && dataset.named_graphs().next().is_none()
        }) {
        // No added rows: retain the existing immutable publication, with no copy
        // or second freeze. Absence of derived facts never skips a selected rule.
        projected
    } else {
        let mut combined = purrdf::RdfDatasetBuilder::new();
        combined.push_dataset(&projected);
        if let Some(derived) = gate_dataset {
            combined.push_dataset(&derived);
        }
        // Append directly into the final union; no intermediate meta dataset.
        derivation.append_to(&mut combined, crate::stages::carrier::GRAPH_DIAGNOSTICS);
        combined
            .freeze()
            .map_err(|e| stage_err("RDF", format!("freeze diagnostic union: {e}")))?
    };
    artifacts.insert(
        paths.rdf.to_owned(),
        crate::stages::superset::canonical_ntriples(&nq_ds)
            .map_err(|e| stage_err("RDF", format!("canonicalize N-Quads: {e}")))?,
    );
    Ok(RenderedDiagnostics {
        report,
        artifacts,
        dataset: nq_ds,
    })
}

#[path = "diag_render.tests.rs"]
#[cfg(test)]
mod tests;
