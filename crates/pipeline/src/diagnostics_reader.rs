// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `graph/diagnostics` RDF reader — the projection's right-inverse (section).
//!
//! `gmeow_errors::render::to_gmeow_rdf_in_graph` EMITS a
//! [`gmeow_errors::Report`]'s findings as
//! `gmeow:Finding` N-Quads into the `graph/diagnostics` named graph (subjects are
//! the ledger's content-addressed fingerprint IRIs, edges are
//! `gmeow:findingAntecedent`). This module is the READER that turns those quads
//! back into [`Finding`]s and the provenance DAG — the section of that projection.
//!
//! # What round-trips (the carried subset)
//!
//! The projection is deliberately lossy (Principle 17). This reader reconstructs
//! exactly the carried subgraph — for every finding: its subject/fingerprint IRI,
//! severity, code, message, tool, [`FindingCategory`], [`Standpoint`], the
//! `gmeow:findingAntecedent` provenance edges, the code-blind
//! `gmeow:findingAnchor` (and its `gmeow:NonTrivialAnchor` flag), the primary
//! `gmeow:findingLocation` GTS/source coordinates, the text-bearing
//! `gmeow:relatedLabel` secondary spans, and the advisory `gmeow:findingSuggestion`
//! strings. `read(emit(x))` reproduces those fields for every finding — the
//! retraction law.
//!
//! # Dogfooding: the read goes THROUGH the native SPARQL engine
//!
//! The reader issues SPARQL SELECTs (scoped to the `graph/diagnostics` named graph
//! via [`RdfDataset::project_named_graph`]) through the native
//! [`crate::stages::native_query`] engine — never a hand-rolled N-Quads parser.
//!
//! # What it reconstructs: a finding-index + antecedent adjacency, NOT a `DiagLedger`
//!
//! A full [`gmeow_errors::ledger::DiagLedger`] cannot be rehydrated from
//! `graph/diagnostics` alone: a ledger node's F1 pin-digest invariant requires the
//! stored [`DiagFingerprint`](gmeow_errors::ledger::DiagFingerprint) to equal the
//! one recomputed from `(code, category, source-context)`, and the source-context
//! identity fields the fingerprint hashes (`term_role`, `focus`) are NOT projected
//! into `graph/diagnostics` (only the resulting fingerprint IRI is). So a
//! reconstructed node would fail the pin-digest self-check on `insert`. The honest
//! reconstruction is therefore a [`FindingIndex`] (a `BTreeMap<fingerprint_iri,
//! Finding>`) plus the antecedent adjacency carried on each finding — everything
//! `explain_finding` and the verdict/minimal-fatal-cut need, keyed deterministically
//! so N-Quad/binding order never leaks.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::{RdfDataset, parse_dataset};

use gmeow_errors::Diag;
use gmeow_errors::dag::{DagError, DagNode, walk};
use gmeow_errors::grade::{BoundedLattice, FindingCategory, GateVerdict, Grade, Standpoint, gate};
use gmeow_errors::model::{Finding, Location, RelatedLabel, Severity};

use crate::error::Parse;
use crate::gmeow_ns::{GMEOW_NS, LOGIC_NS};
use crate::stages::carrier::GRAPH_DIAGNOSTICS;
use crate::stages::native_query::{Solutions, select, term_iri, term_str};

use purrdf::TermValue;

/// The rehydrated finding graph: a `fingerprint_iri → Finding` index (the
/// deterministic `BTreeMap` key defeats unstable N-Quad/binding order) whose
/// findings carry the `gmeow:findingAntecedent` edges as the provenance adjacency.
#[derive(Debug, Clone, Default)]
pub struct FindingIndex {
    /// The findings keyed by their subject / fingerprint IRI (`Finding::finding_iri`).
    pub findings: BTreeMap<String, Finding>,
}

impl FindingIndex {
    /// Resolve a finding by its fingerprint IRI.
    pub fn get(&self, iri: &str) -> Option<&Finding> {
        self.findings.get(iri)
    }

    /// The number of rehydrated findings.
    pub fn len(&self) -> usize {
        self.findings.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }
}

fn parse_error(message: impl Into<String>) -> Diag {
    Diag::of_kind(Parse {
        message: message.into(),
    })
}

/// The value in row column `var`, if bound.
fn cell<'a>(sol: &Solutions, row: &'a [Option<TermValue>], var: &str) -> Option<&'a TermValue> {
    sol.col(var)
        .and_then(|i| row.get(i))
        .and_then(Option::as_ref)
}

fn cell_iri(sol: &Solutions, row: &[Option<TermValue>], var: &str) -> Option<String> {
    term_iri(cell(sol, row, var))
}

fn cell_str(sol: &Solutions, row: &[Option<TermValue>], var: &str) -> Option<String> {
    term_str(cell(sol, row, var))
}

fn cell_u64(sol: &Solutions, row: &[Option<TermValue>], var: &str) -> Option<u64> {
    cell_str(sol, row, var).and_then(|s| s.parse::<u64>().ok())
}

fn cell_u32(sol: &Solutions, row: &[Option<TermValue>], var: &str) -> Option<u32> {
    cell_str(sol, row, var).and_then(|s| s.parse::<u32>().ok())
}

/// Read the location coordinates hung on a finding-location or related-label node
/// (`?path/?line/?col/?term/?quad/?reifier/?frame/?segment`). The `logical` field
/// is not projected into `graph/diagnostics` (it survives only into SARIF logical
/// locations), so it is always `None` here — an acknowledged projection loss.
fn location_from_row(sol: &Solutions, row: &[Option<TermValue>]) -> Location {
    let mut loc = Location::new(
        cell_str(sol, row, "path"),
        cell_u32(sol, row, "line"),
        cell_u32(sol, row, "col"),
        None,
    );
    if let Some(v) = cell_u64(sol, row, "term") {
        loc = loc.with_gts_term(v);
    }
    if let Some(v) = cell_u64(sol, row, "quad") {
        loc = loc.with_gts_quad(v);
    }
    if let Some(v) = cell_u64(sol, row, "reifier") {
        loc = loc.with_gts_reifier(v);
    }
    if let Some(v) = cell_u64(sol, row, "frame") {
        loc = loc.with_gts_frame(v);
    }
    if let Some(v) = cell_u64(sol, row, "segment") {
        loc = loc.with_gts_segment(v);
    }
    loc
}

/// The optional-location OPTIONAL block shared by the location and related-label
/// queries — binds `?path/?line/?col/?term/?quad/?reifier/?frame/?segment` off the
/// node bound to `?node`.
fn location_optionals(node: &str) -> String {
    format!(
        "  OPTIONAL {{ {node} <{GMEOW_NS}findingLocationPath> ?path }}\n\
         \x20 OPTIONAL {{ {node} <{GMEOW_NS}findingLocationLine> ?line }}\n\
         \x20 OPTIONAL {{ {node} <{GMEOW_NS}findingLocationColumn> ?col }}\n\
         \x20 OPTIONAL {{ {node} <{GMEOW_NS}gtsTermId> ?term }}\n\
         \x20 OPTIONAL {{ {node} <{GMEOW_NS}gtsQuadIndex> ?quad }}\n\
         \x20 OPTIONAL {{ {node} <{GMEOW_NS}gtsReifierId> ?reifier }}\n\
         \x20 OPTIONAL {{ {node} <{GMEOW_NS}gtsFrameIndex> ?frame }}\n\
         \x20 OPTIONAL {{ {node} <{GMEOW_NS}gtsSegmentIndex> ?segment }}\n"
    )
}

/// Rehydrate the finding graph from a bundle dataset, scoped to the
/// `graph/diagnostics` named graph. The dataset must RETAIN named graphs (the
/// pipeline's in-memory carrier does); a graph-flattened dataset — e.g. one built
/// with `purrdf::gts::flattened_dataset_from_bytes` — has dropped the graph label
/// and yields an empty index.
pub fn read_findings(dataset: &Arc<RdfDataset>) -> Result<FindingIndex, Diag> {
    let diag = Arc::new(dataset.project_named_graph(GRAPH_DIAGNOSTICS));
    let mut findings: BTreeMap<String, Finding> = BTreeMap::new();

    // Q1 — one row per finding subject: the required grade coordinates + optional
    // tool/category/standpoint/anchor. severity/code/message are always emitted.
    let core = select(
        &diag,
        &format!(
            "SELECT ?s ?sev ?code ?msg ?tool ?cat ?sp ?anchor WHERE {{\n\
             \x20 ?s a <{GMEOW_NS}Finding> .\n\
             \x20 ?s <{GMEOW_NS}findingSeverity> ?sev .\n\
             \x20 ?s <{GMEOW_NS}findingCode> ?code .\n\
             \x20 ?s <{GMEOW_NS}findingMessage> ?msg .\n\
             \x20 OPTIONAL {{ ?s <{GMEOW_NS}findingTool> ?tool }}\n\
             \x20 OPTIONAL {{ ?s <{GMEOW_NS}findingCategory> ?cat }}\n\
             \x20 OPTIONAL {{ ?s <{GMEOW_NS}findingStandpoint> ?sp }}\n\
             \x20 OPTIONAL {{ ?s <{GMEOW_NS}findingAnchor> ?anchor }}\n\
             }}"
        ),
    )?;
    for row in &core.rows {
        let subject = cell_iri(&core, row, "s")
            .ok_or_else(|| parse_error("finding subject is not an IRI"))?;
        let sev_iri = cell_iri(&core, row, "sev")
            .ok_or_else(|| parse_error(format!("finding `{subject}` has no severity IRI")))?;
        let severity = sev_iri
            .strip_prefix(GMEOW_NS)
            .and_then(Severity::from_individual_local)
            .ok_or_else(|| parse_error(format!("unknown severity individual `{sev_iri}`")))?;
        let code = cell_str(&core, row, "code")
            .ok_or_else(|| parse_error(format!("finding `{subject}` has no code")))?;
        let message = cell_str(&core, row, "msg")
            .ok_or_else(|| parse_error(format!("finding `{subject}` has no message")))?;

        let mut finding = Finding::new(severity, code, message);
        finding.finding_iri = Some(subject.clone());
        finding.tool = cell_str(&core, row, "tool");
        if let Some(cat_iri) = cell_iri(&core, row, "cat") {
            finding.category = Some(
                cat_iri
                    .strip_prefix(LOGIC_NS)
                    .and_then(FindingCategory::from_iri_local)
                    .ok_or_else(|| parse_error(format!("unknown finding category `{cat_iri}`")))?,
            );
        }
        if let Some(sp_iri) = cell_iri(&core, row, "sp") {
            finding.standpoint = Some(
                sp_iri
                    .strip_prefix(GMEOW_NS)
                    .and_then(Standpoint::from_iri_local)
                    .ok_or_else(|| parse_error(format!("unknown standpoint `{sp_iri}`")))?,
            );
        }
        finding.anchor_iri = cell_iri(&core, row, "anchor");
        findings.insert(subject, finding);
    }

    // Q2 — the non-trivial anchor set (the guard the cross-node-glut join reads).
    let mut non_trivial: BTreeSet<String> = BTreeSet::new();
    let anchors = select(
        &diag,
        &format!("SELECT ?a WHERE {{ ?a a <{GMEOW_NS}NonTrivialAnchor> }}"),
    )?;
    for row in &anchors.rows {
        if let Some(a) = cell_iri(&anchors, row, "a") {
            non_trivial.insert(a);
        }
    }
    for finding in findings.values_mut() {
        if let Some(anchor) = &finding.anchor_iri {
            finding.anchor_non_trivial = non_trivial.contains(anchor);
        }
    }

    // Q3 — the provenance-DAG antecedent edges.
    let ants = select(
        &diag,
        &format!("SELECT ?s ?ant WHERE {{ ?s <{GMEOW_NS}findingAntecedent> ?ant }}"),
    )?;
    for row in &ants.rows {
        if let (Some(s), Some(ant)) = (cell_iri(&ants, row, "s"), cell_iri(&ants, row, "ant"))
            && let Some(finding) = findings.get_mut(&s)
        {
            finding.antecedents.push(ant);
        }
    }

    // Q3b — the SEPARATE reasoned-quad-reifier provenance edges
    // (`gmeow:findingDerivedFromQuad`): the null-minting head-quad reifiers a
    // chase certificate's verdict derives from. Kept DISTINCT from the
    // `gmeow:findingAntecedent` finding-DAG edges (Q3) — this edge points at a
    // reasoned quad's reifier, not at another finding — and rehydrated
    // sorted+deduped so the index is byte-deterministic.
    let derived = select(
        &diag,
        &format!("SELECT ?s ?q WHERE {{ ?s <{GMEOW_NS}findingDerivedFromQuad> ?q }}"),
    )?;
    for row in &derived.rows {
        if let (Some(s), Some(q)) = (cell_iri(&derived, row, "s"), cell_iri(&derived, row, "q"))
            && let Some(finding) = findings.get_mut(&s)
        {
            finding.derived_from_quads.push(q);
        }
    }

    // Q4 — advisory suggestion strings.
    let sugs = select(
        &diag,
        &format!("SELECT ?s ?sug WHERE {{ ?s <{GMEOW_NS}findingSuggestion> ?sug }}"),
    )?;
    for row in &sugs.rows {
        if let (Some(s), Some(sug)) = (cell_iri(&sugs, row, "s"), cell_str(&sugs, row, "sug"))
            && let Some(finding) = findings.get_mut(&s)
        {
            finding.suggestions.push(sug);
        }
    }

    // Q5 — the primary finding-location GTS/source coordinates.
    let locs = select(
        &diag,
        &format!(
            "SELECT ?s ?loc ?path ?line ?col ?term ?quad ?reifier ?frame ?segment WHERE {{\n\
             \x20 ?s <{GMEOW_NS}findingLocation> ?loc .\n\
             {optionals}\
             }}",
            optionals = location_optionals("?loc")
        ),
    )?;
    for row in &locs.rows {
        if let Some(s) = cell_iri(&locs, row, "s")
            && let Some(finding) = findings.get_mut(&s)
        {
            finding.add_location(location_from_row(&locs, row));
        }
    }

    // Q6 — the text-bearing secondary labels (message + location).
    let labels = select(
        &diag,
        &format!(
            "SELECT ?s ?lab ?msg ?path ?line ?col ?term ?quad ?reifier ?frame ?segment WHERE {{\n\
             \x20 ?s <{GMEOW_NS}relatedLabel> ?lab .\n\
             \x20 ?lab <{GMEOW_NS}labelMessage> ?msg .\n\
             {optionals}\
             }}",
            optionals = location_optionals("?lab")
        ),
    )?;
    for row in &labels.rows {
        if let (Some(s), Some(message)) =
            (cell_iri(&labels, row, "s"), cell_str(&labels, row, "msg"))
            && let Some(finding) = findings.get_mut(&s)
        {
            finding.add_related_label(RelatedLabel {
                location: location_from_row(&labels, row),
                message,
            });
        }
    }

    // Normalize each finding so the rehydrated index is byte-deterministic and
    // matches `Report::normalized()` (the shape the emitter projected from).
    for finding in findings.values_mut() {
        finding.normalize();
    }

    Ok(FindingIndex { findings })
}

/// Rehydrate the finding graph from the `graph/diagnostics` N-Quads the emitter
/// produced. Parses with the native codec (`parse_dataset`), which PRESERVES the
/// named-graph label, then reuses [`read_findings`].
pub fn read_findings_from_nquads(nquads: &[u8]) -> Result<FindingIndex, Diag> {
    let dataset = parse_dataset(nquads, "application/n-quads", None)
        .map_err(|e| parse_error(format!("graph/diagnostics N-Quads parse failed: {e}")))?;
    read_findings(&dataset)
}

/// The finding-DAG walk, reconstructed via the ONE shared DAG engine
/// ([`gmeow_errors::dag::walk`]) over the rehydrated index: resolve each key to its
/// [`Finding`], descend along the `gmeow:findingAntecedent` edges (sorted+deduped
/// for a deterministic, golden-stable order). A missing antecedent is
/// [`DagError::Unresolved`] and a cycle is [`DagError::Cycle`] — both hard fails the
/// engine owns.
pub fn explain_finding(
    index: &FindingIndex,
    root_iri: &str,
) -> Result<DagNode<String, Finding>, DagError<String>> {
    walk(
        root_iri.to_owned(),
        |k: &String| index.findings.get(k).cloned(),
        |_k: &String, finding: &Finding| {
            let mut edges: Vec<String> = finding.antecedents.clone();
            edges.sort();
            edges.dedup();
            edges
        },
    )
}

/// The decomposable derivation of one chase-invented null (Skolem witness),
/// rehydrated from `graph/diagnostics`: the firing rule, the existential ordinal,
/// the head-quad predicate, and the frontier binding(s) — the Skolem-function
/// arguments. A frontier binding that is itself an invented null is the recursive
/// descent edge (it appears as another `WitnessRecord` key).
#[derive(Debug, Clone, Default)]
pub struct WitnessRecord {
    /// The invented-null IRI being explained.
    pub witness: String,
    /// The content-addressed firing rule IRI that minted the null.
    pub rule_iri: String,
    /// The 0-based existential head-variable ordinal the null fills.
    pub ordinal: u64,
    /// The head-quad predicate `p` in `p(x, null)`.
    pub predicate: String,
    /// The frontier binding(s) `x` — the head-quad subject(s), sorted+deduped.
    pub frontier: Vec<String>,
}

/// The rehydrated invented-null graph: a `skolem_iri → WitnessRecord` index over the
/// `gmeow:InventedWitness` typings and their minting head-quad reifiers projected
/// into `graph/diagnostics`. The deterministic `BTreeMap` key defeats unstable
/// binding order; a frontier binding that is itself a key is the recursive descent
/// edge the shared [`walk`] engine follows.
#[derive(Debug, Clone, Default)]
pub struct WitnessIndex {
    /// The invented nulls keyed by their content-addressed skolem IRI.
    pub witnesses: BTreeMap<String, WitnessRecord>,
}

impl WitnessIndex {
    /// Resolve an invented null by its skolem IRI.
    pub fn get(&self, iri: &str) -> Option<&WitnessRecord> {
        self.witnesses.get(iri)
    }

    /// The number of rehydrated invented nulls.
    pub fn len(&self) -> usize {
        self.witnesses.len()
    }

    /// Whether the index is empty (no existential obligation fired).
    pub fn is_empty(&self) -> bool {
        self.witnesses.is_empty()
    }
}

/// Rehydrate the chase-invented nulls from a bundle dataset, scoped to the
/// `graph/diagnostics` named graph — the offline right-inverse of the reason
/// stage's witness projection. Each null carries its firing rule, existential
/// ordinal, head-quad predicate, and frontier binding(s), reconstructed from the
/// `gmeow:InventedWitness`/`gmeow:existentialOrdinal` typings and the standard-RDF-
/// reification head-quad node (`rdf:subject`/`rdf:predicate`/`rdf:object` +
/// `gmeow:viaRule`) whose `rdf:object` is the null. Empty when the shipped program
/// had no existential obligation.
pub fn read_invented_witnesses(dataset: &Arc<RdfDataset>) -> Result<WitnessIndex, Diag> {
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let diag = Arc::new(dataset.project_named_graph(GRAPH_DIAGNOSTICS));
    let mut witnesses: BTreeMap<String, WitnessRecord> = BTreeMap::new();

    // W1 — the invented nulls and their existential ordinal.
    let cores = select(
        &diag,
        &format!(
            "SELECT ?n ?ord WHERE {{\n\
             \x20 ?n a <{GMEOW_NS}InventedWitness> .\n\
             \x20 ?n <{GMEOW_NS}existentialOrdinal> ?ord .\n\
             }}"
        ),
    )?;
    for row in &cores.rows {
        if let Some(n) = cell_iri(&cores, row, "n") {
            let ordinal = cell_u64(&cores, row, "ord").unwrap_or(0);
            witnesses.insert(
                n.clone(),
                WitnessRecord {
                    witness: n,
                    ordinal,
                    ..Default::default()
                },
            );
        }
    }

    // W2 — the minting head-quad reifier for each null: its subject (the frontier
    // binding), predicate, and firing rule. Joined on the reifier's rdf:object being
    // an already-known invented null, so non-witness reifiers (if any) are ignored.
    let heads = select(
        &diag,
        &format!(
            "SELECT ?s ?p ?n ?rule WHERE {{\n\
             \x20 ?r <{RDF}subject> ?s .\n\
             \x20 ?r <{RDF}predicate> ?p .\n\
             \x20 ?r <{RDF}object> ?n .\n\
             \x20 ?r <{GMEOW_NS}viaRule> ?rule .\n\
             }}"
        ),
    )?;
    for row in &heads.rows {
        if let (Some(s), Some(p), Some(n), Some(rule)) = (
            cell_iri(&heads, row, "s"),
            cell_iri(&heads, row, "p"),
            cell_iri(&heads, row, "n"),
            cell_iri(&heads, row, "rule"),
        ) && let Some(record) = witnesses.get_mut(&n)
        {
            record.predicate = p;
            record.rule_iri = rule;
            if !record.frontier.contains(&s) {
                record.frontier.push(s);
            }
        }
    }
    for record in witnesses.values_mut() {
        record.frontier.sort();
        record.frontier.dedup();
    }

    Ok(WitnessIndex { witnesses })
}

/// The invented-null derivation walk, reconstructed via the ONE shared DAG engine
/// ([`gmeow_errors::dag::walk`]) — the SAME machinery [`explain_finding`] uses,
/// with a second entry point. Resolve each skolem IRI to its [`WitnessRecord`],
/// descend along the frontier bindings that are THEMSELVES invented nulls
/// (sorted+deduped for a deterministic order). A frontier binding with no record is
/// a leaf (an EDB/asserted term, not a null); a cycle is [`DagError::Cycle`] and an
/// unresolvable root is [`DagError::Unresolved`] — both hard fails the engine owns.
pub fn explain_witness(
    index: &WitnessIndex,
    root_iri: &str,
) -> Result<DagNode<String, WitnessRecord>, DagError<String>> {
    walk(
        root_iri.to_owned(),
        |k: &String| index.witnesses.get(k).cloned(),
        |_k: &String, record: &WitnessRecord| {
            let mut edges: Vec<String> = record
                .frontier
                .iter()
                .filter(|binding| index.witnesses.contains_key(*binding))
                .cloned()
                .collect();
            edges.sort();
            edges.dedup();
            edges
        },
    )
}

/// Render a reconstructed finding DAG so a shared antecedent (reached along more
/// than one path — the diamond `walk` re-expands as a tree) is printed IN FULL the
/// first time and rendered as a `↑ see <iri>` back-reference on every subsequent
/// visit. The sharing is a render-layer concern threaded through a memoized visited
/// set; [`walk`] stays a pure tree engine.
pub fn render_shared_dag(root: &DagNode<String, Finding>) -> String {
    let mut out = String::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    render_node(root, &mut visited, &mut out);
    out
}

fn render_node(node: &DagNode<String, Finding>, visited: &mut BTreeSet<String>, out: &mut String) {
    let indent = "  ".repeat(node.depth as usize);
    if visited.contains(&node.key) {
        out.push_str(&format!("{indent}↑ see {}\n", node.key));
        return;
    }
    visited.insert(node.key.clone());
    out.push_str(&format!(
        "{indent}{} [{}] {}\n",
        node.key,
        node.payload.severity.as_str(),
        node.payload.message
    ));
    for child in &node.children {
        render_node(child, visited, out);
    }
}

/// The gate verdict a single rehydrated finding contributes: the [`gate`] morphism
/// over its `(severity, category, standpoint)` grade. A finding whose category or
/// standpoint the projection did not carry defaults to a non-blocking / advisory
/// leg, so it can only be Fatal when it genuinely carried the full Fatal grade —
/// an honest absence, never a fabricated fatality.
fn finding_gate(finding: &Finding) -> GateVerdict {
    gate(Grade::new(
        finding.severity,
        finding.category.unwrap_or(FindingCategory::PolicyWarning),
        finding.standpoint.unwrap_or(Standpoint::Advisory),
    ))
}

/// The aggregate ledger verdict of the rehydrated index: the `⊔` join-fold of the
/// [`gate`] morphism over every finding — the same fold
/// [`DiagLedger::verdict`](gmeow_errors::ledger::DiagLedger::verdict) computes over
/// live witnesses. Fatal if any finding gates Fatal; [`Collected`] otherwise (an
/// empty index is Collected, the bottom).
///
/// [`Collected`]: GateVerdict::Collected
pub fn verdict(index: &FindingIndex) -> GateVerdict {
    index
        .findings
        .values()
        .map(finding_gate)
        .fold(GateVerdict::Collected, GateVerdict::join)
}

/// The minimal fatal cut: the fingerprint IRIs of the Fatal-gated findings whose
/// removal flips the verdict from Fatal to Collected.
///
/// The verdict is a join (OR) fold, so its Fatal region is the principal up-set of
/// the Fatal-gated witnesses; the verdict stays Fatal until EVERY Fatal-gated
/// finding is removed. The minimal cut is therefore exactly the set of Fatal-gated
/// findings — computed directly from grades via [`gate`] (not from a materialized
/// `logic:ruleGateFatalVerdict` closure, which the projection does not carry in the
/// carried subset). Empty exactly when the verdict is [`Collected`](GateVerdict::Collected).
pub fn minimal_fatal_cut(index: &FindingIndex) -> BTreeSet<String> {
    index
        .findings
        .iter()
        .filter(|(_, finding)| finding_gate(finding) == GateVerdict::Fatal)
        .map(|(iri, _)| iri.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::model::{Location, Report};
    use gmeow_errors::render::to_gmeow_rdf_in_graph;

    /// A finding's carried (right-invertible) fields — the subset the section
    /// reproduces. Comparing these makes the retraction law explicit.
    #[derive(Debug, PartialEq)]
    struct Carried {
        finding_iri: Option<String>,
        severity: Severity,
        code: String,
        message: String,
        category: Option<FindingCategory>,
        standpoint: Option<Standpoint>,
        anchor_iri: Option<String>,
        anchor_non_trivial: bool,
        antecedents: Vec<String>,
        related_labels: Vec<RelatedLabel>,
        locations: Vec<Location>,
    }

    fn carried(f: &Finding) -> Carried {
        Carried {
            finding_iri: f.finding_iri.clone(),
            severity: f.severity,
            code: f.code.clone(),
            message: f.message.clone(),
            category: f.category,
            standpoint: f.standpoint,
            anchor_iri: f.anchor_iri.clone(),
            anchor_non_trivial: f.anchor_non_trivial,
            antecedents: f.antecedents.clone(),
            related_labels: f.related_labels.clone(),
            locations: f.locations.clone(),
        }
    }

    fn finding_iri(hex: &str) -> String {
        format!("https://blackcatinformatics.ca/gmeow/diagnostics/finding/{hex}")
    }

    /// Build a small multi-node finding DAG: a Fatal root `R` deriving from two
    /// antecedents `A` and `B`, both of which derive from a SHARED leaf `C` (a
    /// diamond). `R` carries a non-trivial anchor + a text-bearing related label.
    fn sample_report() -> Report {
        let r_iri = finding_iri("aaaa0000aaaa0000");
        let a_iri = finding_iri("bbbb1111bbbb1111");
        let b_iri = finding_iri("cccc2222cccc2222");
        let c_iri = finding_iri("dddd3333dddd3333");
        let anchor = "https://blackcatinformatics.ca/gmeow/diagnostics/anchor/aaaa0000aaaa0000";

        let mut report = Report::new("validate");

        // Root R — Fatal (Error + DataShapeViolation + Binding), anchored, labelled.
        let mut r = Finding::new(
            Severity::Error,
            "shape.min-count",
            "focus node fails minCount",
        )
        .with_category(FindingCategory::DataShapeViolation)
        .with_standpoint(Standpoint::Binding)
        .with_tool("validate");
        r.finding_iri = Some(r_iri.clone());
        r.anchor_iri = Some(anchor.to_owned());
        r.anchor_non_trivial = true;
        r.antecedents = vec![a_iri.clone(), b_iri.clone()];
        r.add_location(
            Location::new(Some("core/x.ttl".to_owned()), Some(12), Some(4), None).with_gts_quad(42),
        );
        r.add_related_label(RelatedLabel {
            location: Location::new(Some("core/x.ttl".to_owned()), Some(3), Some(1), None)
                .with_gts_term(7),
            message: "shape declared here".to_owned(),
        });

        // A — derives from C.
        let mut a = Finding::new(Severity::Warning, "logic.derived-a", "intermediate A")
            .with_category(FindingCategory::ModelingDisciplineViolation)
            .with_standpoint(Standpoint::Perspectival);
        a.finding_iri = Some(a_iri.clone());
        a.antecedents = vec![c_iri.clone()];

        // B — also derives from C (the shared antecedent → diamond).
        let mut b = Finding::new(Severity::Note, "logic.derived-b", "intermediate B")
            .with_category(FindingCategory::PolicyWarning)
            .with_standpoint(Standpoint::Advisory);
        b.finding_iri = Some(b_iri.clone());
        b.antecedents = vec![c_iri.clone()];

        // C — the shared leaf root cause.
        let mut c = Finding::new(Severity::Error, "logic.root-cause", "the shared root cause")
            .with_category(FindingCategory::ContradictionWitness)
            .with_standpoint(Standpoint::Advisory);
        c.finding_iri = Some(c_iri.clone());

        report.add_finding(r);
        report.add_finding(a);
        report.add_finding(b);
        report.add_finding(c);
        report
    }

    /// Expected verdict of a report, computed from the same grade→gate fold the
    /// reader uses, so the assertion is an independent recomputation.
    fn report_verdict(report: &Report) -> GateVerdict {
        report
            .findings
            .iter()
            .map(finding_gate)
            .fold(GateVerdict::Collected, GateVerdict::join)
    }

    #[test]
    fn read_of_emit_reproduces_the_carried_finding_subset_and_dag() {
        // THE RETRACTION LAW: read(emit(x)) reproduces the carried subset for every
        // finding, and the rehydrated DAG walk reproduces the original structure.
        let report = sample_report();
        let normalized = report.normalized();

        // emit → graph/diagnostics N-Quads → read back through the SPARQL engine.
        let nquads = to_gmeow_rdf_in_graph(&report, GRAPH_DIAGNOSTICS);
        let index = read_findings_from_nquads(nquads.as_bytes()).expect("read back");

        // Same set of findings (keyed by fingerprint IRI).
        assert_eq!(index.len(), normalized.findings.len());

        // Carried subset reproduced for EVERY finding — code, grade (severity +
        // category + standpoint), message, anchor (+ non-trivial flag), antecedent
        // edges, related-label TEXT + location, and primary location.
        for original in &normalized.findings {
            let iri = original.finding_iri.clone().expect("witness has an IRI");
            let round = index.get(&iri).expect("finding rehydrated");
            assert_eq!(
                carried(round),
                carried(original),
                "carried subset mismatch for {iri}"
            );
        }

        // The related-label TEXT specifically survived (the recently-added leg).
        let r_iri = finding_iri("aaaa0000aaaa0000");
        let r = index.get(&r_iri).expect("root present");
        assert_eq!(r.related_labels.len(), 1);
        assert_eq!(r.related_labels[0].message, "shape declared here");
        assert_eq!(r.related_labels[0].location.gts_term_id, Some(7));

        // The rehydrated DAG walk from the root reproduces the original structure:
        // same node set + same edge set as the source finding graph.
        let tree = explain_finding(&index, &r_iri).expect("walk");
        let mut walked_nodes: BTreeSet<String> = BTreeSet::new();
        let mut walked_edges: BTreeSet<(String, String)> = BTreeSet::new();
        for node in tree.preorder() {
            walked_nodes.insert(node.key.clone());
            for child in &node.children {
                walked_edges.insert((node.key.clone(), child.key.clone()));
            }
        }
        // Expected structure straight off the original findings' antecedents.
        let mut expected_nodes: BTreeSet<String> = BTreeSet::new();
        let mut expected_edges: BTreeSet<(String, String)> = BTreeSet::new();
        let by_iri: BTreeMap<&str, &Finding> = normalized
            .findings
            .iter()
            .filter_map(|f| f.finding_iri.as_deref().map(|i| (i, f)))
            .collect();
        let mut stack = vec![r_iri.clone()];
        while let Some(key) = stack.pop() {
            expected_nodes.insert(key.clone());
            if let Some(f) = by_iri.get(key.as_str()) {
                for ant in &f.antecedents {
                    expected_edges.insert((key.clone(), ant.clone()));
                    stack.push(ant.clone());
                }
            }
        }
        assert_eq!(walked_nodes, expected_nodes, "DAG node set mismatch");
        assert_eq!(walked_edges, expected_edges, "DAG edge set mismatch");

        // The shared leaf C is reached along BOTH R→A→C and R→B→C: the shared-DAG
        // render prints it once in full and back-references it on the second visit.
        let rendered = render_shared_dag(&tree);
        let c_iri = finding_iri("dddd3333dddd3333");
        assert_eq!(
            rendered.matches(&format!("↑ see {c_iri}")).count(),
            1,
            "shared antecedent must be back-referenced exactly once:\n{rendered}"
        );
        assert!(
            rendered.contains("the shared root cause"),
            "shared antecedent must be printed in full once:\n{rendered}"
        );

        // The verdict computed from the rehydrated ledger equals the original's.
        assert_eq!(verdict(&index), report_verdict(&report));
        assert_eq!(verdict(&index), GateVerdict::Fatal, "root R is Fatal");

        // The minimal fatal cut is exactly the Fatal-gated finding (only R gates:
        // C is Error but Advisory, so it never gates).
        let cut = minimal_fatal_cut(&index);
        assert_eq!(cut, BTreeSet::from([r_iri.clone()]));
    }

    #[test]
    fn empty_diagnostics_graph_is_collected_with_no_findings() {
        // A report with no findings projects no diagnostics quads; the reader yields
        // an empty index whose verdict is the bottom (Collected) and whose cut is empty.
        let report = Report::new("validate");
        let nquads = to_gmeow_rdf_in_graph(&report, GRAPH_DIAGNOSTICS);
        let index = read_findings_from_nquads(nquads.as_bytes()).expect("read back");
        assert!(index.is_empty());
        assert_eq!(verdict(&index), GateVerdict::Collected);
        assert!(minimal_fatal_cut(&index).is_empty());
    }
}
