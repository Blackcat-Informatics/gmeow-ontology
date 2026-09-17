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

use gmeow_errors::abox::{AboxObject, X_GMEOW_ENGLISH, abox_annotations};
use purrdf::RdfLiteral;

use crate::loss_ledger::LossLedger;

use super::super::ir::LogicProgram;
use super::rdf::TripleSink;
use super::{
    GMEOW_NS, LOGIC_NS, OverclaimError, ProjectionResult, RDF_TYPE, XSD_NS, assert_no_overclaim,
};

/// The named graph the compiler's projection-report loss ledger is folded into
/// downstream (mirrors `crates/pipeline::stages::carrier::GRAPH_PROJECTION_LEDGER`
/// verbatim — `gmeow-logic-compile` has zero dependency on `gmeow-pipeline`, so the
/// literal is pinned here rather than imported). This is the `rdfs:isDefinedBy` target
/// for every A-Box individual this report mints: every `logic:ProjectionTarget` and
/// `logic:TermProjectionLoss` genuinely lives in this named graph once folded, so the
/// annotation is a true fact, not a placeholder.
const GRAPH_PROJECTION_LEDGER: &str =
    "https://blackcatinformatics.ca/gmeow/graph/projection-ledger";

fn logic(local: &str) -> String {
    format!("{LOGIC_NS}{local}")
}

/// Route the four mandatory A-Box annotations (`rdfs:label`, `skos:definition`,
/// `rdfs:isDefinedBy`, `gmeow:graphBoxRole`) for one generated individual through the
/// single shared contract ([`gmeow_errors::abox::abox_annotations`]), so this emitter
/// cannot drift from the other A-Box producers (`render`, `gmeow_docs::rdf`, the
/// pipeline's provenance/evals generators). Label/definition literals carry the
/// [`X_GMEOW_ENGLISH`] carrier language tag via [`RdfLiteral::language_tagged`], never a
/// bare `en` tag.
fn emit_abox_annotations(
    g: &mut TripleSink,
    subject_iri: &str,
    label: &str,
    definition: &str,
    graph_iri: &str,
) {
    for (predicate, object) in abox_annotations(subject_iri, label, definition, graph_iri) {
        match object {
            AboxObject::Iri(iri) => g.add_iri(subject_iri, predicate, &iri),
            AboxObject::CarrierLiteral(value) => g.add_lit(
                subject_iri,
                predicate,
                RdfLiteral::language_tagged(value, X_GMEOW_ENGLISH),
            ),
        }
    }
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

/// The report judgment for one projection, independent of its emitted body.
/// The complete output remains owned by the selected projection artifact.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectionReportRow {
    /// The exact target key used to join this row to the shared loss ledger.
    pub target: String,
    /// Whether the separately emitted body is an RDF serialization.
    pub is_rdf: bool,
    /// The declared preservation judgment, checked against the joined residue.
    pub preservation: crate::ir::PreservationKind,
    /// The declared complexity class, retained verbatim in the report.
    pub complexity: String,
}

/// A borrowed report judgment from either a projection or a compact native row.
#[derive(Clone, Copy)]
pub struct ProjectionReportRowRef<'a> {
    /// The exact target key used by the loss ledger.
    pub target: &'a str,
    /// The declared preservation judgment.
    pub preservation: crate::ir::PreservationKind,
    /// The declared complexity class.
    pub complexity: &'a str,
}

impl From<&ProjectionResult> for ProjectionReportRow {
    /// Copy only report metadata while the caller retains its emitted body.
    fn from(projection: &ProjectionResult) -> Self {
        Self {
            target: projection.target.clone(),
            is_rdf: projection.is_rdf,
            preservation: projection.preservation,
            complexity: projection.complexity.clone(),
        }
    }
}

impl From<ProjectionResult> for ProjectionReportRow {
    /// Move report metadata and release a body whose artifact has already been emitted.
    fn from(projection: ProjectionResult) -> Self {
        Self {
            target: projection.target,
            is_rdf: projection.is_rdf,
            preservation: projection.preservation,
            complexity: projection.complexity,
        }
    }
}

impl<'a> From<&'a ProjectionResult> for ProjectionReportRowRef<'a> {
    /// Borrow the judgment without copying the projection's output or metadata.
    fn from(projection: &'a ProjectionResult) -> Self {
        Self {
            target: &projection.target,
            preservation: projection.preservation,
            complexity: &projection.complexity,
        }
    }
}

impl<'a> From<&'a ProjectionReportRow> for ProjectionReportRowRef<'a> {
    /// Borrow a compact native row for the shared report renderer.
    fn from(row: &'a ProjectionReportRow) -> Self {
        Self {
            target: &row.target,
            preservation: row.preservation,
            complexity: &row.complexity,
        }
    }
}

/// The three header counts of the projection report.  Carried as a small value so the
/// report can be assembled by a caller that has the counts but no [`LogicProgram`] —
/// e.g. the pipeline, which reconstructs the correspondence ledger separately from the
/// logic program and unions the two before serializing ONCE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReportHeader {
    /// `logic:axiomCount` — the number of axioms in the logic program.
    pub axiom_count: usize,
    /// `logic:ruleCount` — the number of rules in the logic program.
    pub rule_count: usize,
    /// `logic:profileCount` — the number of reasoning contracts (profiles).
    pub profile_count: usize,
    /// `logic:formulaCount` — the number of full-FOL formulas (the non-Horn layer).
    /// Emitted only when non-zero, so a formula-free report is byte-unchanged.
    pub formula_count: usize,
    /// `logic:correspondenceCount` — the number of authored `logic:Correspondence`
    /// nodes (the derived liftability statistic's denominator). Emitted only when
    /// non-zero, so a correspondence-free report is byte-unchanged.
    pub correspondence_count: usize,
    /// `logic:lawfulUpliftCount` — the number of correspondences whose up-lift is
    /// LAWFUL (the Round-trip or Mnemomorphism gate passes): the derived liftability
    /// statistic's numerator. This is the gate-verdict-derived replacement for the old
    /// SSSOM-heuristic "liftable" headline — `count(round-trip/mnemomorphism PASS) /
    /// count(correspondences)` — carried in the canonical loss ledger.
    pub lawful_uplift_count: usize,
    /// `logic:claimedUpliftCount` — the number of correspondences whose up-lift is
    /// LIFTABLE but only CLAIMED (asserted by the alignment relation, `ObligationUnknown`),
    /// not proved by inversion. Carried alongside `lawful_uplift_count` so the canonical
    /// loss ledger discloses the full proved/claimed split (maximal information flow), not
    /// only the proved numerator. Emitted only when non-zero.
    pub claimed_uplift_count: usize,
}

impl ReportHeader {
    /// The header counts read off a [`LogicProgram`]. The liftability numerator
    /// ([`Self::lawful_uplift_count`]) is gate-derived and is left 0 here; set it via
    /// [`Self::with_lawful_uplift`] once the correspondence gates have been evaluated.
    pub fn of_program(program: &LogicProgram) -> Self {
        Self {
            axiom_count: program.axioms.len(),
            rule_count: program.rules.len(),
            profile_count: program.contracts.len(),
            formula_count: program.formulas.len(),
            correspondence_count: program.correspondences.len(),
            lawful_uplift_count: 0,
            claimed_uplift_count: 0,
        }
    }

    /// Set the derived liftability numerator (the count of lawful up-lifts from the
    /// correspondence gate verdicts). The denominator is [`Self::correspondence_count`].
    pub fn with_lawful_uplift(mut self, lawful: usize) -> Self {
        self.lawful_uplift_count = lawful;
        self
    }

    /// Set the claimed (asserted-but-not-proved) up-lift count carried alongside the lawful
    /// numerator, so the loss ledger discloses the full proved/claimed split.
    pub fn with_claimed_uplift(mut self, claimed: usize) -> Self {
        self.claimed_uplift_count = claimed;
        self
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
    ledger: &LossLedger,
) -> Result<String, OverclaimError> {
    build_projection_report_from(ReportHeader::of_program(program), projections, ledger)
}

/// Borrow report judgments from full projection results and delegate to the same
/// [`build_projection_report_rows`] renderer used by native pipeline publications.
/// Emitted projection bodies are never copied to assemble the report.
///
/// # Errors
///
/// As [`build_projection_report`].
pub fn build_projection_report_from(
    header: ReportHeader,
    projections: &[ProjectionResult],
    ledger: &LossLedger,
) -> Result<String, OverclaimError> {
    build_projection_report_rows(
        header,
        projections.iter().map(ProjectionReportRowRef::from),
        &[ledger],
    )
}

/// Render borrowed report rows over the union of complete native loss stores.
/// Only the sortable row references and projected notes are allocated; projection
/// bodies and diagnostic nodes are neither copied nor reconstructed for the join.
///
/// # Errors
/// Refuses exact overclaims and unsupported rows with no disclosed residue.
pub fn build_projection_report_rows<'a>(
    header: ReportHeader,
    projections: impl IntoIterator<Item = ProjectionReportRowRef<'a>>,
    ledgers: &[&LossLedger],
) -> Result<String, OverclaimError> {
    let ledger = crate::loss_ledger::ProjectionLossView::new(ledgers);
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
    if header.formula_count > 0 {
        g.add_lit(
            &report_iri,
            &logic("formulaCount"),
            int_literal(header.formula_count),
        );
    }
    // The derived liftability statistic (gate-verdict-derived, replacing the SSSOM
    // heuristic): emitted only when the program authors correspondences, so a
    // correspondence-free report is byte-identical.
    if header.correspondence_count > 0 {
        g.add_lit(
            &report_iri,
            &logic("correspondenceCount"),
            int_literal(header.correspondence_count),
        );
        g.add_lit(
            &report_iri,
            &logic("lawfulUpliftCount"),
            int_literal(header.lawful_uplift_count),
        );
        // The claimed tier (asserted-but-not-proved up-lifts) — emitted only when non-zero so
        // a claim-free report stays byte-identical, while the proved/claimed split is never
        // collapsed when present.
        if header.claimed_uplift_count > 0 {
            g.add_lit(
                &report_iri,
                &logic("claimedUpliftCount"),
                int_literal(header.claimed_uplift_count),
            );
        }
    }

    // Targets in sorted order (the Python `sorted(projections, key=target)`).
    let mut sorted: Vec<ProjectionReportRowRef<'_>> = projections.into_iter().collect();
    sorted.sort_by(|a, b| a.target.cmp(b.target));

    for proj in sorted {
        // The per-target drop set read back from the ONE loss store the producers interned
        // into — the single source of truth for both the legalization gate's residue and
        // the `gmeow:lossyDrop` records serialized below (structural notes sorted, then the
        // `actual: `-prefixed per-run notes sorted).
        let drops = ledger.projection_drops_for(proj.target);

        // Legalization gate: a lowering is a total function into ⟨legal ⊕ flagged
        // residue⟩. The residue is the full flagged set — exactly what is serialized below
        // as gmeow:lossyDrop. The gate fires on an Exact overclaim OR an Unsupported silent
        // under-disclosure.
        let residue: Vec<&str> = drops.iter().map(String::as_str).collect();
        assert_no_overclaim(proj.target, proj.preservation, &residue)?;

        // The target name is the IRI's local segment AND the human label. Whole-program
        // logic target names use only IRI-safe characters, so the encoder below is the
        // identity for them (their rows stay byte-identical); correspondence target names
        // embed full IRIs + separators (`|`, `::`, spaces) that are illegal in an IRI, so
        // those are percent-encoded into a legal, deterministic segment. The unencoded
        // name remains the readable `rdfs:label`.
        // Identity is mandatory target metadata. Loss notes never supply a label
        // or authorize a preservation claim, including notes with old label text.
        let target_key = proj.target;

        let target_iri = format!("{LOGIC_NS}target/{}", iri_safe_segment(proj.target));
        g.add_iri(&report_iri, &logic("hasProjection"), &target_iri);
        g.add_iri(&target_iri, RDF_TYPE, &logic("ProjectionTarget"));
        let target_definition = format!(
            "Projection to {target_key}: preservation {}, complexity {}.",
            proj.preservation.as_str(),
            proj.complexity
        );
        emit_abox_annotations(
            &mut g,
            &target_iri,
            target_key,
            &target_definition,
            GRAPH_PROJECTION_LEDGER,
        );
        g.add_iri(
            &target_iri,
            &logic("preservationKind"),
            &logic(proj.preservation.as_str()),
        );
        g.add_lit(
            &target_iri,
            &logic("complexityClass"),
            RdfLiteral::simple(proj.complexity),
        );

        let lossy_drop = format!("{GMEOW_NS}lossyDrop");
        for note in &drops {
            g.add_lit(&target_iri, &lossy_drop, RdfLiteral::simple(note));
        }

        // Per-term projection-loss attribution (additive; the `gmeow:lossyDrop` literals
        // above are byte-unchanged). Each actual drop that names a DOCUMENTED source term
        // (carried structurally on its `observed` slot — never scraped from the note) is
        // reified as a `logic:TermProjectionLoss` node keyed by (target, source term), so the
        // docs per-term projection-loss table can join the drop to that term's page. Drops
        // sharing a (target, source term) fold their notes onto ONE node; a genuinely
        // program-wide drop carries no source term and is NOT reified here (it stays on the
        // static loss-ledger page). `term_source_drops` returns them sorted by (source, note),
        // so the emission is deterministic.
        let mut by_source: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for (note, source_term) in ledger.term_source_drops(proj.target) {
            by_source.entry(source_term).or_default().push(note);
        }
        let lossy_source_term = format!("{GMEOW_NS}lossySourceTerm");
        for (source_term, notes) in &by_source {
            // Minted as a CHILD of the parent target IRI (`.../target/<target>/termloss/...`)
            // and back-linked with `logic:lossOfTarget` — NOT with a forward `hasTermLoss`
            // triple ON the target block. That keeps every `logic:ProjectionTarget` block
            // byte-identical whether or not it now carries per-term attribution, so the
            // byte-stability gate (which pins the FIXED logic rows) is undisturbed; the
            // standalone term-loss blocks are stripped there by their `/termloss/` segment.
            let term_loss_iri = format!("{target_iri}/termloss/{}", iri_safe_segment(source_term));
            g.add_iri(&term_loss_iri, RDF_TYPE, &logic("TermProjectionLoss"));
            g.add_iri(&term_loss_iri, &logic("lossOfTarget"), &target_iri);
            g.add_iri(&term_loss_iri, &lossy_source_term, source_term);
            // The projection target this term-loss belongs to (the readable target name),
            // and the target's preservation kind + complexity — so the docs row is complete
            // without re-joining the parent target node. Label/definition are derived from
            // the same `target_key` + the DOCUMENTED source term (never `proj.target`'s
            // opaque hash) — the four mandatory A-Box annotations, via the shared contract.
            let term_loss_label = format!("{target_key}: loss of {source_term}");
            let term_loss_definition =
                format!("Term {source_term} is not preserved by the projection to {target_key}.");
            emit_abox_annotations(
                &mut g,
                &term_loss_iri,
                &term_loss_label,
                &term_loss_definition,
                GRAPH_PROJECTION_LEDGER,
            );
            g.add_iri(
                &term_loss_iri,
                &logic("preservationKind"),
                &logic(proj.preservation.as_str()),
            );
            g.add_lit(
                &term_loss_iri,
                &logic("complexityClass"),
                RdfLiteral::simple(proj.complexity),
            );
            for note in notes {
                g.add_lit(&term_loss_iri, &lossy_drop, RdfLiteral::simple(note));
            }
        }
    }

    let banner = "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
                  # Preservation loss ledger for all logic: projections.\n";
    Ok(g.serialize(banner))
}

#[path = "report.tests.rs"]
#[cfg(test)]
mod tests;
