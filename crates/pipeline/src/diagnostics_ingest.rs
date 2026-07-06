// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The run-level ledger consumer of the carrier's `graph/diagnostics` fold.
//!
//! `stage-validate` and `stage-compile-logic` each project their `gmeow-errors`
//! [`Report`](gmeow_errors::Report) into `gmeow:Finding` RDF and attach it to the
//! carrier's `graph/diagnostics` named graph (`crate::stages::carrier` unions the
//! two producers there). The four committed byte artifacts and that RDF fold are
//! the canonical diagnostics; nothing re-computes them here.
//!
//! This module reads that SAME cache-stable graph back out of the producers'
//! datasets and INGESTS every `gmeow:Finding` into the run-level
//! [`DiagLedger`](gmeow_errors::DiagLedger) through the designed purrdf ingestion
//! boundary [`gmeow_errors::Diag::from_rdf`], attributing each finding to its REAL
//! producing stage (`stage-validate` / `stage-compile-logic`) rather than the
//! synthetic reconcile stage. Because the source is the content-addressed carrier
//! graph (identical across warm/cold cache) and findings are walked in sorted
//! subject order, the ledger content is byte-identical run to run — the ledger is
//! not a second, divergent copy of the diagnostics, it is a projection of the one
//! that ships in `gmeow.gts`.

use std::collections::BTreeMap;

use gmeow_errors::{Diag, DiagLedger, StageId};
use purrdf::{RdfDataset, RdfDiagnostic, RdfLocation, RdfSeverity, RdfTerm};

/// gmeow's canonical namespace (mirror `crate::gmeow_ns::GMEOW_NS`).
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The `gmeow:Finding` RDF shape the errors-crate projection emits
/// (`gmeow_errors::render::to_gmeow_rdf_in_graph`). Ingestion reads exactly this
/// shape back.
fn p(local: &str) -> String {
    format!("{GMEOW_NS}{local}")
}

/// The IRI value of `term` if it is an IRI, else `None`.
fn iri(term: &RdfTerm) -> Option<&str> {
    match term {
        RdfTerm::Iri(value) => Some(value.as_str()),
        _ => None,
    }
}

/// The lexical form of `term` if it is a literal, else `None`.
fn lexical(term: &RdfTerm) -> Option<&str> {
    match term {
        RdfTerm::Literal(literal) => Some(literal.lexical_form.as_str()),
        _ => None,
    }
}

/// Map a `gmeow:severity*` individual IRI back to the canonical [`RdfSeverity`].
/// Mirror of `gmeow_errors::render::severity_individual`.
fn severity_from_individual(individual: &str) -> Option<RdfSeverity> {
    match individual.strip_prefix(GMEOW_NS)? {
        "severityError" => Some(RdfSeverity::Error),
        "severityWarning" => Some(RdfSeverity::Warning),
        "severityNote" => Some(RdfSeverity::Note),
        "severityInfo" => Some(RdfSeverity::Info),
        _ => None,
    }
}

/// The mutable accumulator for one `gmeow:Finding` subject as its triples stream in.
#[derive(Default)]
struct FindingAccum {
    is_finding: bool,
    severity: Option<RdfSeverity>,
    code: Option<String>,
    message: Option<String>,
    /// The `gmeow:findingLocation` node IRIs, kept sorted for determinism (a
    /// projected finding carries at most one, but the reader stays total).
    location_nodes: Vec<String>,
}

/// Ingest every `gmeow:Finding` in `diagnostics` into `ledger`, attributing each to
/// `stage` (the REAL producing stage). `diagnostics` is a producer's slice of the
/// carrier's `graph/diagnostics` fold. Findings are ingested in sorted subject
/// order so attach order is deterministic. Returns the number of parent findings
/// ingested.
///
/// A subject typed `gmeow:Finding` that is missing any of its required
/// severity/code/message triples is a malformed carrier graph — a HARD FAIL, never
/// a silently-dropped finding.
pub(crate) fn ingest_diagnostics_graph(
    diagnostics: &RdfDataset,
    ledger: &mut DiagLedger,
    stage: &str,
) -> Result<usize, crate::error::PipelineError> {
    let finding_type = p("Finding");
    let sev_pred = p("findingSeverity");
    let code_pred = p("findingCode");
    let msg_pred = p("findingMessage");
    let loc_pred = p("findingLocation");
    let loc_path_pred = p("findingLocationPath");
    let gts_term = p("gtsTermId");
    let gts_quad = p("gtsQuadIndex");
    let gts_reifier = p("gtsReifierId");
    let gts_frame = p("gtsFrameIndex");
    let gts_segment = p("gtsSegmentIndex");

    let mut findings: BTreeMap<String, FindingAccum> = BTreeMap::new();
    // Location nodes are described by their own triples (path + GTS wire
    // coordinates), keyed by the location node IRI.
    let mut locations: BTreeMap<String, RdfLocation> = BTreeMap::new();

    for quad in diagnostics.owned_quads() {
        let Some(subject) = iri(&quad.subject) else {
            continue;
        };
        let predicate = quad.predicate.as_str();
        if predicate == RDF_TYPE {
            if iri(&quad.object) == Some(finding_type.as_str()) {
                findings.entry(subject.to_owned()).or_default().is_finding = true;
            }
            continue;
        }
        if predicate == sev_pred {
            if let Some(individual) = iri(&quad.object) {
                findings.entry(subject.to_owned()).or_default().severity =
                    severity_from_individual(individual);
            }
            continue;
        }
        if predicate == code_pred {
            if let Some(value) = lexical(&quad.object) {
                findings.entry(subject.to_owned()).or_default().code = Some(value.to_owned());
            }
            continue;
        }
        if predicate == msg_pred {
            if let Some(value) = lexical(&quad.object) {
                findings.entry(subject.to_owned()).or_default().message = Some(value.to_owned());
            }
            continue;
        }
        if predicate == loc_pred {
            if let Some(node) = iri(&quad.object) {
                let acc = findings.entry(subject.to_owned()).or_default();
                acc.location_nodes.push(node.to_owned());
                acc.location_nodes.sort();
                acc.location_nodes.dedup();
            }
            continue;
        }
        // Location-node describing triples. The subject is the location node.
        if predicate == loc_path_pred {
            if let Some(value) = lexical(&quad.object) {
                locations.entry(subject.to_owned()).or_default().path = Some(value.to_owned());
            }
            continue;
        }
        let gts_slot: Option<fn(&mut RdfLocation, usize)> = if predicate == gts_term {
            Some(|loc, v| loc.gts_term_id = Some(v))
        } else if predicate == gts_quad {
            Some(|loc, v| loc.gts_quad_index = Some(v))
        } else if predicate == gts_reifier {
            Some(|loc, v| loc.gts_reifier_id = Some(v))
        } else if predicate == gts_frame {
            Some(|loc, v| loc.gts_frame_index = Some(v))
        } else if predicate == gts_segment {
            Some(|loc, v| loc.gts_segment_index = Some(v))
        } else {
            None
        };
        if let Some(set) = gts_slot
            && let Some(value) = lexical(&quad.object).and_then(|v| v.parse::<usize>().ok())
        {
            set(locations.entry(subject.to_owned()).or_default(), value);
        }
    }

    let mut ingested = 0usize;
    for (subject, acc) in &findings {
        if !acc.is_finding {
            continue;
        }
        let (Some(severity), Some(code), Some(message)) =
            (acc.severity, acc.code.as_ref(), acc.message.as_ref())
        else {
            return Err(crate::error::PipelineError::Stage {
                stage: stage.to_owned(),
                message: format!(
                    "malformed gmeow:Finding <{subject}> in graph/diagnostics: missing severity/code/message"
                ),
            });
        };
        let mut rdf_diag = RdfDiagnostic::new(severity, code.clone(), message.clone());
        if let Some(node) = acc.location_nodes.first()
            && let Some(location) = locations.get(node)
            && !location.is_empty()
        {
            rdf_diag = rdf_diag.with_location(location.clone());
        }
        let parent = Diag::from_rdf(&rdf_diag, ledger, StageId::new(stage));
        ledger.attach(parent, StageId::new(stage));
        ingested += 1;
    }
    Ok(ingested)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::{Finding, Report, Severity};

    /// Build the `graph/diagnostics` dataset the way `stage-validate` does: project
    /// an errors `Report` to `gmeow:Finding` N-Quads in the diagnostics graph and
    /// parse it into a dataset.
    fn diagnostics_dataset(report: &Report, graph_iri: &str) -> std::sync::Arc<RdfDataset> {
        let nquads = gmeow_errors::render::to_gmeow_rdf_in_graph(report, graph_iri);
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("diagnostics n-quads parse")
    }

    #[test]
    fn ingests_findings_attributed_to_the_real_stage() {
        let graph = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
        let mut report = Report::new("shacl");
        report.add_finding(
            Finding::new(
                Severity::Error,
                "shacl.MinCountConstraintComponent",
                "required value is missing",
            )
            .with_tool("shacl"),
        );
        let dataset = diagnostics_dataset(&report, graph);

        let mut ledger = DiagLedger::new();
        let ingested = ingest_diagnostics_graph(dataset.as_ref(), &mut ledger, "stage-validate")
            .expect("ingest");
        assert_eq!(ingested, 1);

        let nodes = ledger.emit_sorted();
        let node = nodes
            .iter()
            .find(|n| n.code == "shacl.MinCountConstraintComponent")
            .expect("finding ingested");
        assert_eq!(node.stage.as_str(), "stage-validate");
        assert_eq!(node.grade.severity, Severity::Error);
    }

    #[test]
    fn ingestion_is_deterministic_across_repeats() {
        let graph = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
        let mut report = Report::new("shacl");
        for i in 0..5 {
            report.add_finding(Finding::new(
                Severity::Warning,
                format!("shacl.rule{i}"),
                format!("warning {i}"),
            ));
        }
        let dataset = diagnostics_dataset(&report, graph);

        let mut a = DiagLedger::new();
        ingest_diagnostics_graph(dataset.as_ref(), &mut a, "stage-validate").expect("ingest a");
        let mut b = DiagLedger::new();
        ingest_diagnostics_graph(dataset.as_ref(), &mut b, "stage-validate").expect("ingest b");

        let a_codes: Vec<_> = a.emit_sorted().iter().map(|n| n.code.clone()).collect();
        let b_codes: Vec<_> = b.emit_sorted().iter().map(|n| n.code.clone()).collect();
        assert_eq!(a_codes, b_codes);
        assert_eq!(a.len(), 5);
    }

    #[test]
    fn malformed_finding_is_a_hard_fail() {
        // A subject typed gmeow:Finding but carrying no severity/code/message.
        let graph = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
        let nquads = format!(
            "<{GMEOW_NS}diagnostics/finding/x> <{RDF_TYPE}> <{GMEOW_NS}Finding> <{graph}> .\n"
        );
        let dataset =
            purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None).expect("parse");
        let mut ledger = DiagLedger::new();
        let err = ingest_diagnostics_graph(dataset.as_ref(), &mut ledger, "stage-validate")
            .expect_err("malformed finding must hard-fail");
        assert!(format!("{err}").contains("malformed gmeow:Finding"));
    }
}
