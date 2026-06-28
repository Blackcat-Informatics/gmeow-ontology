// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Projection report: the preservation loss ledger
//! (`generated/logic/projection-report.ttl`).
//!
//! The preservation-loss-ledger emitter (`build_projection_report`); the Python
//! duplicate has been retired.  Emits, per target, a
//! `logic:ProjectionTarget` node with `logic:preservationKind`,
//! `logic:complexityClass`, and aggregated `gmeow:lossyDrop` records; runs the
//! legalization gate per target so an overclaim — or an `Unsupported` target that
//! flags no residue — blocks serialization (red build).
//! Compared by RDF isomorphism, like the other RDF targets.

use gmeow_rdf::RdfLiteral;

use super::super::ir::LogicProgram;
use super::rdf::TripleSink;
use super::{
    assert_no_overclaim, OverclaimError, ProjectionResult, GMEOW_NS, LOGIC_NS, RDFS_NS, RDF_TYPE,
    XSD_NS,
};

fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}

/// Percent-encode a projection-target name into a legal IRI path segment.
///
/// The unreserved set + the IRI-path-legal punctuation that the whole-program logic
/// target names already use (`:` `/` `#` `.` `-` `_` `~`) pass through unchanged — so the
/// seven logic rows (and any `property-path:<iri>` row) serialize byte-identically. Every
/// other byte (space, `|`, the `://` scheme separators are covered by `:`/`/`, etc.) is
/// percent-encoded, yielding a deterministic, collision-free, legal segment for the
/// correspondence rows (whose names embed full IRIs and `|`/`::`/space separators).
fn iri_safe_segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for b in name.bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(b, b':' | b'/' | b'#' | b'.' | b'-' | b'_' | b'~');
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn int_literal(n: usize) -> RdfLiteral {
    RdfLiteral::typed(n.to_string(), format!("{XSD_NS}integer"))
}

/// The three header counts of the projection report.  Carried as a small value so the
/// report can be assembled by a caller that has the counts but no [`LogicProgram`] —
/// e.g. the pipeline, which reconstructs the correspondence ledger separately from the
/// logic program and unions the two before serializing ONCE.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ReportHeader {
    /// `logic:axiomCount` — the number of axioms in the logic program.
    pub axiom_count: usize,
    /// `logic:ruleCount` — the number of rules in the logic program.
    pub rule_count: usize,
    /// `logic:profileCount` — the number of reasoning contracts (profiles).
    pub profile_count: usize,
}

impl ReportHeader {
    /// The header counts read off a [`LogicProgram`].
    pub fn of_program(program: &LogicProgram) -> Self {
        Self {
            axiom_count: program.axioms.len(),
            rule_count: program.rules.len(),
            profile_count: program.contracts.len(),
        }
    }
}

/// Build the projection-report Turtle (`generated/logic/projection-report.ttl`) from a
/// [`LogicProgram`].  Thin wrapper over [`build_projection_report_from`] — the SINGLE
/// serialization routine — so the logic-row bytes are identical whichever caller
/// assembles the report.
///
/// # Errors
///
/// Returns [`OverclaimError`] if any projection declares `ExactPreservation` but
/// produced drops, or declares `Unsupported` (the legalization floor) yet flagged no
/// residue (a silent under-disclosure).
pub fn build_projection_report(
    program: &LogicProgram,
    projections: &[ProjectionResult],
) -> Result<String, OverclaimError> {
    build_projection_report_from(ReportHeader::of_program(program), projections)
}

/// The SINGLE projection-report serialization routine: header counts + the sorted
/// projection rows, run through one [`TripleSink`] with the same alphabetical target
/// sort.  Both [`build_projection_report`] and the pipeline (which unions the logic
/// projections with the correspondence ledger) funnel through here, so the seven
/// whole-program logic rows serialize byte-identically regardless of caller — only the
/// added correspondence rows differ.
///
/// # Errors
///
/// As [`build_projection_report`].
pub fn build_projection_report_from(
    header: ReportHeader,
    projections: &[ProjectionResult],
) -> Result<String, OverclaimError> {
    let mut g = TripleSink::default();

    let report_iri = logic("projection-report");
    g.add_iri(&report_iri, RDF_TYPE, &logic("ProjectionReport"));
    g.add_lit(
        &report_iri,
        &logic("axiomCount"),
        int_literal(header.axiom_count),
    );
    g.add_lit(
        &report_iri,
        &logic("ruleCount"),
        int_literal(header.rule_count),
    );
    g.add_lit(
        &report_iri,
        &logic("profileCount"),
        int_literal(header.profile_count),
    );

    // Targets in sorted order (the Python `sorted(projections, key=target)`).
    let mut sorted: Vec<&ProjectionResult> = projections.iter().collect();
    sorted.sort_by(|a, b| a.target.cmp(&b.target));

    for proj in sorted {
        // Legalization gate: a lowering is a total function into ⟨legal ⊕ flagged
        // residue⟩.  The residue is the full flagged set (structural lossy_drops +
        // concrete actual_drops) — exactly what is serialized below as gmeow:lossyDrop.
        // The gate fires on an Exact overclaim OR an Unsupported silent under-disclosure.
        let residue: Vec<&str> = proj
            .lossy_drops
            .iter()
            .chain(proj.actual_drops.iter())
            .map(String::as_str)
            .collect();
        assert_no_overclaim(&proj.target, proj.preservation, &residue)?;

        // The target name is the IRI's local segment AND the human label. Whole-program
        // logic target names use only IRI-safe characters, so the encoder below is the
        // identity for them (their rows stay byte-identical); correspondence target names
        // embed full IRIs + separators (`|`, `::`, spaces) that are illegal in an IRI, so
        // those are percent-encoded into a legal, deterministic segment. The unencoded
        // name remains the readable `rdfs:label`.
        let target_iri = format!("{LOGIC_NS}target/{}", iri_safe_segment(&proj.target));
        g.add_iri(&report_iri, &logic("hasProjection"), &target_iri);
        g.add_iri(&target_iri, RDF_TYPE, &logic("ProjectionTarget"));
        g.add_lit(
            &target_iri,
            &format!("{RDFS_NS}label"),
            RdfLiteral::simple(&proj.target),
        );
        g.add_iri(
            &target_iri,
            &logic("preservationKind"),
            &logic(proj.preservation.as_str()),
        );
        g.add_lit(
            &target_iri,
            &logic("complexityClass"),
            RdfLiteral::simple(&proj.complexity),
        );

        let lossy_drop = format!("{GMEOW_NS}lossyDrop");
        let mut structural = proj.lossy_drops.clone();
        structural.sort();
        for note in &structural {
            g.add_lit(&target_iri, &lossy_drop, RdfLiteral::simple(note));
        }
        let mut actual = proj.actual_drops.clone();
        actual.sort();
        for a in &actual {
            g.add_lit(
                &target_iri,
                &lossy_drop,
                RdfLiteral::simple(format!("actual: {a}")),
            );
        }
    }

    let banner = "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
                  # Preservation loss ledger for all logic: projections.\n";
    Ok(g.serialize(banner))
}
