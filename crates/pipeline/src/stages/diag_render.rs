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
pub fn render_diagnostics_artifacts(
    stage: &str,
    report: &Report,
    paths: &DiagnosticsPaths<'_>,
    gate: Option<&crate::stages::gate_verdict::GateProgram>,
    meta: Option<&crate::stages::meta_findings::MetaProgram>,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let stage_err = |what: &str, detail: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: stage.to_owned(),
            message: format!("render {what} diagnostics: {detail}"),
        })
    };
    // The projected finding graph BEFORE enrichment — the EDB the meta chase reads
    // (the derived root-cause/cluster/glut fields the enrichment adds are ignored by
    // `to_gmeow_rdf`, so this projection is identical to the enriched report's).
    let projected_nq = gmeow_errors::render::to_gmeow_rdf(report);
    // Run the authored diagnostic meta-rules over the projected graph, then enrich a
    // working copy of the report so every renderer surfaces the meta-findings.
    let derivation = match meta {
        Some(meta) => meta
            .derive(&projected_nq)
            .map_err(|e| stage_err("meta", e.to_string()))?,
        None => crate::stages::meta_findings::MetaDerivation::default(),
    };
    // Only clone + enrich when the meta chase actually derived findings; on the
    // common empty-derivation path (meta=None, or a chase that fired nothing) the
    // original `report` reference is used directly with NO clone — enrichment over an
    // empty derivation is a no-op, so the two paths are byte-identical.
    let enriched = (!derivation.is_empty()).then(|| {
        let mut enriched = report.clone();
        crate::stages::meta_findings::enrich_report(&mut enriched, &derivation);
        enriched
    });
    let report = enriched.as_ref().unwrap_or(report);

    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(
        paths.json.to_owned(),
        text_artifact(
            gmeow_errors::render::to_json(report).map_err(|e| stage_err("JSON", e.to_string()))?,
        ),
    );
    artifacts.insert(
        paths.sarif.to_owned(),
        text_artifact(
            gmeow_errors::render::to_sarif(report)
                .map_err(|e| stage_err("SARIF", e.to_string()))?,
        ),
    );
    artifacts.insert(
        paths.html.to_owned(),
        text_artifact(gmeow_errors::render::to_html(report)),
    );
    // The `.nq` diagnostics graph rides as an RDF-fanout named graph: emit the RDFC-1.0
    // canonical N-Quads (keeping the `graph/diagnostics` 4th-column label) so the
    // superset gate reconstructs it byte-for-byte.
    let mut nq = projected_nq;
    // Materialize the REASONER-DERIVED gate verdict: run the AUTHORED
    // logic:ruleGateFatalVerdict up-set rule (via the native chase, NOT the Rust
    // gate() morphism) over the projected finding grades and fold the derived
    // gmeow:findingGateVerdict gmeow:gateFatal triples in BEFORE canonicalization, so
    // both the byte artifact and the carrier graph carry the entailment and the
    // gmeow:GateFatalUpsetShape passes under validate-gts. Producers whose findings can
    // never join the up-set (e.g. the logic-compiler's Note-severity lossy drops) pass
    // `None` and the projection is byte-unchanged.
    if let Some(gate) = gate {
        let derived = gate
            .derived_verdict_nquads(&nq, crate::stages::carrier::GRAPH_DIAGNOSTICS)
            .map_err(|e| stage_err("RDF", format!("derive gate verdict: {e}")))?;
        nq.push_str(&derived);
    }
    // Fold the derived meta-findings (root-cause / cluster / materialized cross-node
    // glut witness) right after the gate fold, so both the byte artifact and the
    // carrier graph carry them alongside the grade coordinates.
    nq.push_str(&derivation.to_nquads(crate::stages::carrier::GRAPH_DIAGNOSTICS));
    let nq_ds = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
        .map_err(|e| stage_err("RDF", format!("parse N-Quads: {e}")))?;
    artifacts.insert(
        paths.rdf.to_owned(),
        crate::stages::superset::canonical_ntriples(&nq_ds)
            .map_err(|e| stage_err("RDF", format!("canonicalize N-Quads: {e}")))?,
    );
    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::{Finding, FindingCategory, StageId};

    fn report_with_one_finding() -> Report {
        let mut report = Report::new("shacl");
        report.add_finding(
            Finding::new(
                Severity::Error,
                "shacl.MinCountConstraintComponent",
                "required value is missing",
            )
            .with_tool("shacl"),
        );
        report
    }

    /// Node-equivalence: the FORWARD `finding_nodes` fold carries the SAME
    /// code / severity / stage / category the retired backward `Diag::from_rdf` ingest
    /// produced — so the shipped `graph/diagnostics` RDF and the model goldens do not
    /// change. Category is derived from severity (Error → ModelingDisciplineViolation),
    /// exactly as `gmeow_errors::rdf::default_category` maps it, NOT from `finding.category`.
    #[test]
    fn forward_nodes_carry_expected_code_severity_stage_category() {
        let nodes = finding_nodes(&report_with_one_finding(), "stage-validate");
        assert_eq!(nodes.len(), 1, "one finding → one node");
        let node = &nodes[0];
        assert_eq!(node.code, "shacl.MinCountConstraintComponent");
        assert_eq!(node.grade.severity, Severity::Error);
        assert_eq!(node.stage.as_str(), "stage-validate");
        assert_eq!(
            node.grade.category,
            FindingCategory::ModelingDisciplineViolation
        );
    }

    /// Findings carry no losses, so every forward node has EMPTY antecedents — there is
    /// no cross-stage dangling edge (each producer's node set is self-contained).
    #[test]
    fn forward_nodes_have_no_dangling_antecedents() {
        let nodes = finding_nodes(&report_with_one_finding(), "stage-validate");
        for node in &nodes {
            assert!(
                node.antecedents.is_empty(),
                "a forward finding node must carry no antecedents"
            );
        }
    }

    /// The fold is a pure function of report content: repeated calls (and a report
    /// whose findings are inserted out of order) yield byte-identical serialized nodes.
    #[test]
    fn forward_fold_is_deterministic_and_order_independent() {
        let mut a = Report::new("shacl");
        let mut b = Report::new("shacl");
        for (report, order) in [(&mut a, [0usize, 1, 2]), (&mut b, [2, 1, 0])] {
            for i in order {
                report.add_finding(Finding::new(
                    Severity::Warning,
                    format!("shacl.rule{i}"),
                    format!("warning {i}"),
                ));
            }
        }
        let na = serde_json::to_vec(&finding_nodes(&a, "stage-validate")).unwrap();
        let nb = serde_json::to_vec(&finding_nodes(&b, "stage-validate")).unwrap();
        assert_eq!(na, nb, "forward fold is insertion-order independent");
        assert_eq!(finding_nodes(&a, "stage-validate").len(), 3);
    }

    /// Cross-stage shared fingerprint: the SAME finding folded at two DIFFERENT stages
    /// hash-conses to ONE node whose stage min-merges to the lexicographic minimum,
    /// byte-identical regardless of replay order (the property the run ledger relies on
    /// when a fingerprint is emitted by both producers, one fresh and one cache-hit).
    #[test]
    fn cross_stage_shared_fingerprint_min_merges_order_independently() {
        let report = report_with_one_finding();
        let from_a = finding_nodes(&report, "stage-a");
        let from_b = finding_nodes(&report, "stage-b");

        let emit = |first: &[DiagNode], second: &[DiagNode]| -> Vec<u8> {
            let mut ledger = DiagLedger::new();
            ledger.replay(first.to_vec());
            ledger.replay(second.to_vec());
            serde_json::to_vec(
                &ledger
                    .emit_sorted()
                    .into_iter()
                    .cloned()
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        };
        let ab = emit(&from_a, &from_b);
        let ba = emit(&from_b, &from_a);
        assert_eq!(ab, ba, "cross-stage replay is order independent");

        let mut ledger = DiagLedger::new();
        ledger.replay(from_b.clone());
        ledger.replay(from_a.clone());
        let nodes = ledger.emit_sorted();
        assert_eq!(nodes.len(), 1, "one shared fingerprint → one node");
        assert_eq!(
            nodes[0].stage.as_str(),
            "stage-a",
            "the merged stage is the lexicographic minimum, not the first writer"
        );
        // Sanity: attributing to a single stage really does key the node by that stage.
        let single = finding_nodes(&report, "stage-z");
        assert_eq!(single[0].stage, StageId::new("stage-z"));
    }
}
