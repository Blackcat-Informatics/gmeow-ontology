// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoner-derived diagnostic meta-findings (root-cause / cluster / cross-node
//! glut) materialization for the shipped diagnostics graph.
//!
//! This is the `MetaProgram` twin of [`crate::stages::gate_verdict::GateProgram`].
//! Where the gate program runs the single authored `logic:ruleGateFatalVerdict`
//! up-set rule, this program runs EVERY rule the source graph types
//! `gmeow:DiagnosticMetaRule` — discovered BY TYPE, never by a hardcoded head
//! predicate, so a new meta-finding is added by tagging a rule and needs no engine
//! change (the extensibility contract). It reasons the projected `gmeow:Finding`
//! graph (via the native chase `reason_program`, NOT a Rust morphism) and returns:
//!
//! * `gmeow:findingRootCause` — a finding's traced childless-root antecedent,
//! * `gmeow:findingCluster` / `gmeow:clusterRoot` + the `gmeow:FindingCluster` /
//!   `gmeow:RootFinding` type markers — the shared-root grouping surface,
//! * the cross-node glut — MATERIALIZED as a `gmeow:CrossNodeGlutWitness` node with
//!   two `gmeow:glutWitnessOf` edges, from the reasoner's directed
//!   `gmeow:crossNodeGlutWith` edges. Because the reified-Horn chase cannot mint a
//!   fresh witness node, the witness IRI is minted HERE as a content-addressed IRI
//!   (`blake3` over the SORTED pair of participating finding IRIs + the head
//!   predicate), so it is deterministic and stable.
//!
//! The rules and the `gmeow:categoryPolarity` wiring the cross-node-glut rule joins
//! against are READ from the authored source graph, never re-typed here — exactly
//! the production surface `crates/conformance/tests/diagnostics_meta_findings.rs`
//! proves over the actual authored ontology.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_errors::Report;
use gmeow_logic::reason::reason_program;
use gmeow_logic_compile::frontend::parse_logic_dataset;
use gmeow_logic_compile::ir::{LogicProgram, LogicRule};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{
    NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, SparqlEngine, SparqlRequest,
    SparqlResult, TermValue, dataset_from_bytes,
};

use gmeow_ns::GMEOW_NS;
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";

/// The class every diagnostic meta-rule is typed with — the class-based selection.
const DIAGNOSTIC_META_RULE: &str = "https://blackcatinformatics.ca/gmeow/DiagnosticMetaRule";
/// The category→Belnap-polarity wiring the cross-node-glut rule joins against.
const CATEGORY_POLARITY: &str = "https://blackcatinformatics.ca/gmeow/categoryPolarity";

/// The single named-graph world the projected finding facts + polarity wiring are
/// re-scoped into for the chase (a plain default-graph fact is invisible to the
/// chase by design, so the whole EDB is world-scoped — the gate-verdict discipline).
const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics-meta-derivation";

// The derived meta predicates + type markers the fold collects and re-projects.
const FINDING_ROOT_CAUSE: &str = "https://blackcatinformatics.ca/gmeow/findingRootCause";
const FINDING_CLUSTER: &str = "https://blackcatinformatics.ca/gmeow/findingCluster";
const CLUSTER_ROOT: &str = "https://blackcatinformatics.ca/gmeow/clusterRoot";
const CROSS_NODE_GLUT_WITH: &str = "https://blackcatinformatics.ca/gmeow/crossNodeGlutWith";
const FINDING_CLUSTER_CLASS: &str = "https://blackcatinformatics.ca/gmeow/FindingCluster";
const ROOT_FINDING_CLASS: &str = "https://blackcatinformatics.ca/gmeow/RootFinding";

// The materialized cross-node glut witness vocabulary + the assertional grade the
// minted witness carries so it is a well-formed gmeow:Finding (FindingShape).
const CROSS_NODE_GLUT_WITNESS_CLASS: &str =
    "https://blackcatinformatics.ca/gmeow/CrossNodeGlutWitness";
const GLUT_WITNESS_OF: &str = "https://blackcatinformatics.ca/gmeow/glutWitnessOf";
const FINDING_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Finding";
const FINDING_SEVERITY: &str = "https://blackcatinformatics.ca/gmeow/findingSeverity";
const SEVERITY_NOTE: &str = "https://blackcatinformatics.ca/gmeow/severityNote";
const FINDING_CODE: &str = "https://blackcatinformatics.ca/gmeow/findingCode";
const FINDING_MESSAGE: &str = "https://blackcatinformatics.ca/gmeow/findingMessage";
const FINDING_CATEGORY: &str = "https://blackcatinformatics.ca/gmeow/findingCategory";
const FINDING_STANDPOINT: &str = "https://blackcatinformatics.ca/gmeow/findingStandpoint";
const STANDPOINT_ADVISORY: &str = "https://blackcatinformatics.ca/gmeow/standpointAdvisory";
const FINDING_PERMITTED_CONFLICT: &str =
    "https://blackcatinformatics.ca/logic/FindingPermittedEpistemicConflict";
const GRAPH_BOX_ROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";
const BOX_ABOX: &str = "https://blackcatinformatics.ca/gmeow/boxABox";
/// The stable finding code the minted cross-node glut witness carries.
const GLUT_WITNESS_CODE: &str = "diagnostics.cross-node-glut";

/// The authored diagnostic meta-reasoning fold, extracted ONCE from the source
/// graph: every `gmeow:DiagnosticMetaRule` (selected by TYPE) plus the
/// `gmeow:categoryPolarity` category→Belnap-value wiring the cross-node-glut rule
/// reads. Reasoning any projected finding graph against this reproduces the
/// ontology's derived meta-findings for the shipped bundle.
pub struct MetaProgram {
    program: LogicProgram,
    category_polarity: Vec<(String, String)>,
}

/// The reasoner-derived meta-findings, collected from one chase over a projected
/// finding graph. Every collection is a sorted set, so the re-projection and the
/// report enrichment are deterministic regardless of the chase's emission order.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MetaDerivation {
    /// `(finding, root)` — each finding's traced childless-root antecedent.
    pub root_cause: BTreeSet<(String, String)>,
    /// `(finding, root)` — each finding's membership in the root-keyed cluster.
    pub cluster: BTreeSet<(String, String)>,
    /// The shared root carried once on the cluster node (the `gmeow:clusterRoot`
    /// self-edge subject; the rule head is `?root clusterRoot ?root`, so only the
    /// single root IRI is retained).
    pub cluster_root: BTreeSet<String>,
    /// Roots typed `gmeow:FindingCluster` (the grouping node).
    pub cluster_typed: BTreeSet<String>,
    /// Roots typed `gmeow:RootFinding` (the extensibility demonstrator).
    pub root_finding_typed: BTreeSet<String>,
    /// `(supported, opposed)` — the directed cross-node glut edges (Supported →
    /// Opposed), the raw material the `gmeow:CrossNodeGlutWitness` is minted from.
    pub glut: BTreeSet<(String, String)>,
}

impl MetaDerivation {
    /// Whether the fold derived nothing (a byte-unchanged projection).
    pub fn is_empty(&self) -> bool {
        self.root_cause.is_empty()
            && self.cluster.is_empty()
            && self.cluster_root.is_empty()
            && self.cluster_typed.is_empty()
            && self.root_finding_typed.is_empty()
            && self.glut.is_empty()
    }

    /// The derived meta N-Quads for the findings' `graph_iri` — the root-cause,
    /// cluster, and MATERIALIZED cross-node glut witness triples, sorted+deduped so
    /// the projection is byte-stable. Empty string when the fold derived nothing.
    pub fn to_nquads(&self, graph_iri: &str) -> String {
        let g = format!("<{graph_iri}>");
        let mut lines: Vec<String> = Vec::new();
        for (finding, root) in &self.root_cause {
            lines.push(format!("<{finding}> <{FINDING_ROOT_CAUSE}> <{root}> {g} ."));
        }
        for (finding, root) in &self.cluster {
            lines.push(format!("<{finding}> <{FINDING_CLUSTER}> <{root}> {g} ."));
        }
        for root in &self.cluster_root {
            lines.push(format!("<{root}> <{CLUSTER_ROOT}> <{root}> {g} ."));
        }
        for root in &self.cluster_typed {
            lines.push(format!(
                "<{root}> <{RDF_TYPE}> <{FINDING_CLUSTER_CLASS}> {g} ."
            ));
        }
        for root in &self.root_finding_typed {
            lines.push(format!(
                "<{root}> <{RDF_TYPE}> <{ROOT_FINDING_CLASS}> {g} ."
            ));
        }
        for (a, b) in &self.glut {
            self.push_witness_lines(a, b, &g, &mut lines);
        }
        lines.sort();
        lines.dedup();
        if lines.is_empty() {
            String::new()
        } else {
            let mut out = lines.join("\n");
            out.push('\n');
            out
        }
    }

    /// Materialize one `gmeow:CrossNodeGlutWitness` from a derived directed glut
    /// edge: a content-addressed witness node carrying its own assertional grade
    /// (so it is a well-formed `gmeow:Finding`) and exactly two `gmeow:glutWitnessOf`
    /// links (one per conflicting finding). The witness IRI is `blake3` over the
    /// SORTED pair of finding IRIs + the head predicate, so the two directions of
    /// the symmetric conflict mint ONE stable witness.
    fn push_witness_lines(&self, a: &str, b: &str, g: &str, lines: &mut Vec<String>) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let witness = glut_witness_iri(lo, hi);
        let w = format!("<{witness}>");
        let label = format!(
            "cross-node glut between {} and {} at a shared anchor",
            short_finding(lo),
            short_finding(hi)
        );
        let message = format!(
            "cross-node glut: {lo} and {hi} carry opposing coherence polarity at one anchor"
        );
        lines.push(format!(
            "{w} <{RDF_TYPE}> <{CROSS_NODE_GLUT_WITNESS_CLASS}> {g} ."
        ));
        lines.push(format!("{w} <{RDF_TYPE}> <{FINDING_CLASS}> {g} ."));
        lines.push(format!(
            "{w} <{RDFS_LABEL}> \"{}\" {g} .",
            nq_escape(&label)
        ));
        lines.push(format!("{w} <{RDFS_IS_DEFINED_BY}> {g} {g} ."));
        lines.push(format!("{w} <{GRAPH_BOX_ROLE}> <{BOX_ABOX}> {g} ."));
        lines.push(format!("{w} <{FINDING_SEVERITY}> <{SEVERITY_NOTE}> {g} ."));
        lines.push(format!(
            "{w} <{FINDING_CODE}> \"{}\" {g} .",
            nq_escape(GLUT_WITNESS_CODE)
        ));
        lines.push(format!(
            "{w} <{FINDING_MESSAGE}> \"{}\" {g} .",
            nq_escape(&message)
        ));
        lines.push(format!(
            "{w} <{FINDING_CATEGORY}> <{FINDING_PERMITTED_CONFLICT}> {g} ."
        ));
        lines.push(format!(
            "{w} <{FINDING_STANDPOINT}> <{STANDPOINT_ADVISORY}> {g} ."
        ));
        lines.push(format!("{w} <{GLUT_WITNESS_OF}> <{lo}> {g} ."));
        lines.push(format!("{w} <{GLUT_WITNESS_OF}> <{hi}> {g} ."));
    }
}

impl MetaProgram {
    /// Parse the authored `gmeow:DiagnosticMetaRule` fold and the
    /// `gmeow:categoryPolarity` wiring out of the source graph N-Quads (the validate
    /// stage's base-graph bytes, which carry the logic + diagnostics slices).
    ///
    /// Returns `Ok(None)` when the source graph carries no meta-rules — a source
    /// without them derives nothing, so the projection stays byte-unchanged. A
    /// malformed source graph, or one that types `gmeow:DiagnosticMetaRule` subjects
    /// none of which parse into a logic rule, is a HARD FAIL (`Err`): a real defect
    /// in a REQUIRED input must stop the pipeline, never silently collapse to the
    /// no-rules path and ship a byte-unchanged projection.
    pub fn from_source(source_nquads: &[u8]) -> gmeow_errors::Result<Option<MetaProgram>> {
        let dataset = dataset_from_bytes(source_nquads, NativeRdfFormat::NQuads).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!("parse diagnostic meta-fold source graph: {e}"),
            })
        })?;
        Self::from_source_dataset(&dataset)
    }

    /// The dataset-native entry [`from_source`](MetaProgram::from_source) wraps —
    /// the seam the unit test drives with a Turtle-parsed source graph.
    pub fn from_source_dataset(
        dataset: &Arc<RdfDataset>,
    ) -> gmeow_errors::Result<Option<MetaProgram>> {
        let meta_iris = select_iris(
            dataset,
            &format!("SELECT ?r WHERE {{ ?r <{RDF_TYPE}> <{DIAGNOSTIC_META_RULE}> . }}"),
            "r",
        );
        if meta_iris.is_empty() {
            // The genuine "no meta-rules authored" case — nothing to derive.
            return Ok(None);
        }
        // The source graph DOES carry `gmeow:DiagnosticMetaRule` subjects, so a parse
        // failure past this point is a real defect, not an absence — surface it.
        let (program, diags) = parse_logic_dataset(dataset, None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!("parse authored diagnostic meta-rules: {e}"),
            })
        })?;
        let error_diags = diags
            .iter()
            .filter(|d| d.severity == gmeow_logic_compile::frontend::Severity::Error)
            .count();
        if error_diags > 0 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!(
                    "authored diagnostic meta-rules carry {error_diags} parse error(s); refusing to ship a partial fold"
                ),
            }));
        }
        // Select ALL and ONLY the meta rules, matched to their parsed LogicRule via
        // logic:provenance (each rule carries its own IRI there) — the class-based
        // fold, isolated from the rest of the logic slice.
        let rules: Vec<LogicRule> = program
            .rules
            .into_iter()
            .filter(|r| {
                r.scope
                    .provenance
                    .as_deref()
                    .is_some_and(|p| meta_iris.contains(p))
            })
            .collect();
        if rules.is_empty() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!(
                    "source graph types {} gmeow:DiagnosticMetaRule subject(s) but none parsed into a logic rule",
                    meta_iris.len()
                ),
            }));
        }
        let category_polarity = select_pairs(
            dataset,
            &format!("SELECT ?c ?p WHERE {{ ?c <{CATEGORY_POLARITY}> ?p . }}"),
            "c",
            "p",
        );
        Ok(Some(MetaProgram {
            program: LogicProgram::new(Vec::new(), rules, Vec::new(), None),
            category_polarity,
        }))
    }

    /// Run the authored meta-rules over the projected diagnostics `finding_nq`
    /// (N-Quads) and collect the derived meta-findings. World-scopes the projected
    /// finding facts + the authored polarity wiring into ONE named world (the chase
    /// reads facts out of named-graph worlds), reasons them, and harvests the
    /// derived root-cause / cluster / glut rows.
    ///
    /// Hard-fails (`Err`) on a malformed `finding_nq` or a chase failure (e.g. an
    /// unstratifiable program) — never a silent fallback.
    pub fn derive(&self, finding_nq: &str) -> gmeow_errors::Result<MetaDerivation> {
        let facts = iri_triples(finding_nq)?;
        if facts.is_empty() {
            return Ok(MetaDerivation::default());
        }
        let mut builder = RdfDatasetBuilder::new();
        for (s, p, o) in &facts {
            push_world(&mut builder, s, p, o);
        }
        for (c, p) in &self.category_polarity {
            push_world(&mut builder, c, CATEGORY_POLARITY, p);
        }
        let edb = builder.freeze().map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!("freeze meta-derivation EDB: {e}"),
            })
        })?;
        let result = reason_program(&self.program, edb.as_ref()).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!("reason diagnostic meta-rules: {e}"),
            })
        })?;

        let mut derivation = MetaDerivation::default();
        for atom in result.inferred() {
            if atom.is_edb {
                continue;
            }
            let object = strip_angle(&atom.object);
            match atom.predicate.as_str() {
                FINDING_ROOT_CAUSE => {
                    derivation.root_cause.insert((atom.subject.clone(), object));
                }
                FINDING_CLUSTER => {
                    derivation.cluster.insert((atom.subject.clone(), object));
                }
                CLUSTER_ROOT => {
                    // The rule head is `?root gmeow:clusterRoot ?root` (a self-edge),
                    // so subject == object — retain the single root IRI.
                    derivation.cluster_root.insert(atom.subject.clone());
                }
                CROSS_NODE_GLUT_WITH => {
                    derivation.glut.insert((atom.subject.clone(), object));
                }
                RDF_TYPE => match object.as_str() {
                    FINDING_CLUSTER_CLASS => {
                        derivation.cluster_typed.insert(atom.subject.clone());
                    }
                    ROOT_FINDING_CLASS => {
                        derivation.root_finding_typed.insert(atom.subject.clone());
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(derivation)
    }
}

/// The content-addressed cross-node glut witness IRI: `blake3` over the SORTED pair
/// of participating finding IRIs + the `gmeow:crossNodeGlutWith` head predicate,
/// truncated to 16 bytes of hex. Deterministic and swap-invariant: the two
/// directions of the symmetric conflict mint ONE witness (the sort is internal).
fn glut_witness_iri(a: &str, b: &str) -> String {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut hasher = blake3::Hasher::new();
    feed(&mut hasher, b"predicate", CROSS_NODE_GLUT_WITH.as_bytes());
    feed(&mut hasher, b"finding", lo.as_bytes());
    feed(&mut hasher, b"finding", hi.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(32);
    use std::fmt::Write;
    for byte in &digest.as_bytes()[..16] {
        let _ = write!(hex, "{byte:02x}");
    }
    format!("{GMEOW_NS}diagnostics/glut-witness/{hex}")
}

/// Length-prefixed, domain-separated field feed — a length prefix before every
/// field makes cross-field delimiter-injection collisions impossible.
fn feed(hasher: &mut blake3::Hasher, tag: &[u8], bytes: &[u8]) {
    hasher.update(&(tag.len() as u64).to_le_bytes());
    hasher.update(tag);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// Enrich a [`Report`]'s findings with the reasoner-derived meta-findings so the
/// text / JSON / SARIF / HTML surfaces carry them, keyed on each finding's stable
/// `finding_iri` (the SAME IRI the projected graph's subject carries). A finding
/// that was never a ledger witness (no `finding_iri`) participates in no meta
/// derivation, so it is left untouched.
pub fn enrich_report(report: &mut Report, derivation: &MetaDerivation) {
    if derivation.is_empty() {
        return;
    }
    // Fold the symmetric glut edges into a per-finding peer set once.
    let mut glut_peers: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (a, b) in &derivation.glut {
        glut_peers.entry(a).or_default().insert(b);
        glut_peers.entry(b).or_default().insert(a);
    }
    // Pre-fold the (finding → smallest root) maps ONCE instead of re-scanning the
    // full `root_cause`/`cluster` sets per finding (was O(findings × derivations)).
    // Both sets iterate in sorted `(finding, root)` order, so the FIRST root seen for
    // a finding is its lexicographically smallest — the same choice as the old `.min()`
    // (the schema does not claim root uniqueness; the flat surface carries one).
    let mut root_by_finding: BTreeMap<&str, &str> = BTreeMap::new();
    for (f, r) in &derivation.root_cause {
        root_by_finding.entry(f.as_str()).or_insert(r.as_str());
    }
    let mut cluster_by_finding: BTreeMap<&str, &str> = BTreeMap::new();
    for (f, r) in &derivation.cluster {
        cluster_by_finding.entry(f.as_str()).or_insert(r.as_str());
    }
    for finding in &mut report.findings {
        let Some(iri) = finding.finding_iri.clone() else {
            continue;
        };
        if let Some(root) = root_by_finding.get(iri.as_str()) {
            finding.root_cause = Some((*root).to_owned());
        }
        if let Some(cluster) = cluster_by_finding.get(iri.as_str()) {
            finding.cluster = Some((*cluster).to_owned());
        }
        if let Some(peers) = glut_peers.get(iri.as_str()) {
            finding.cross_node_glut_with = peers.iter().map(|p| (*p).to_owned()).collect();
        }
    }
}

/// A short display suffix of a finding IRI (its final path segment), for the
/// witness label — the full IRI still rides on the `gmeow:glutWitnessOf` edges.
fn short_finding(iri: &str) -> &str {
    iri.rsplit('/').next().unwrap_or(iri)
}

/// Push one all-IRI triple into the single chase world.
fn push_world(builder: &mut RdfDatasetBuilder, s: &str, p: &str, o: &str) {
    let quad = RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(WORLD));
    builder.push_owned_quad(&quad);
}

/// Strip the angle brackets an N-Triples IRI object renders with.
fn strip_angle(s: &str) -> String {
    s.strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(s)
        .to_owned()
}

/// The IRI string of a bound SPARQL term, or `None` if it is not an IRI.
fn iri_of(term: &TermValue) -> Option<String> {
    match term {
        TermValue::Iri(i) => Some(i.clone()),
        _ => None,
    }
}

/// Escape a string literal for N-Quads (mirrors the diagnostics RDF projection).
fn nq_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Run a one-variable `SELECT` and collect its bound IRIs into a set.
fn select_iris(dataset: &Arc<RdfDataset>, query: &str, var: &str) -> BTreeSet<String> {
    let (variables, rows) = match run_select(dataset, query) {
        Some(v) => v,
        None => return BTreeSet::new(),
    };
    let Some(idx) = variables.iter().position(|v| v == var) else {
        return BTreeSet::new();
    };
    rows.iter()
        .filter_map(|sol| iri_of(sol.get(idx).and_then(|t| t.as_ref())?))
        .collect()
}

/// Run a two-variable `SELECT` and collect its `(a, b)` IRI pairs, sorted+deduped.
fn select_pairs(dataset: &Arc<RdfDataset>, query: &str, a: &str, b: &str) -> Vec<(String, String)> {
    let (variables, rows) = match run_select(dataset, query) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let (Some(ai), Some(bi)) = (
        variables.iter().position(|v| v == a),
        variables.iter().position(|v| v == b),
    ) else {
        return Vec::new();
    };
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for sol in &rows {
        if let (Some(av), Some(bv)) = (
            sol.get(ai).and_then(|t| t.as_ref()).and_then(iri_of),
            sol.get(bi).and_then(|t| t.as_ref()).and_then(iri_of),
        ) {
            pairs.insert((av, bv));
        }
    }
    pairs.into_iter().collect()
}

/// Evaluate a `SELECT`, returning its `(variables, rows)` — `None` on error or a
/// non-`SELECT` result.
#[allow(clippy::type_complexity)]
fn run_select(
    dataset: &Arc<RdfDataset>,
    query: &str,
) -> Option<(Vec<String>, Vec<Vec<Option<TermValue>>>)> {
    let engine = NativeSparqlEngine::new();
    let result = engine
        .query(
            dataset,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .ok()?;
    match result {
        SparqlResult::Solutions {
            variables, rows, ..
        } => Some((variables, rows)),
        _ => None,
    }
}

/// Extract every all-IRI triple from the projected diagnostics N-Quads (the finding
/// facts the meta-rules join on — `gmeow:findingAntecedent`, `gmeow:findingAnchor`,
/// the `gmeow:NonTrivialAnchor` type, `gmeow:findingCategory`). Literal-object
/// triples (message/code/label) are not part of any meta-rule body and are dropped.
fn iri_triples(nq: &str) -> gmeow_errors::Result<Vec<(String, String, String)>> {
    let mf = |message: String| gmeow_errors::Diag::of_kind(crate::error::MetaFold { message });
    let dataset = dataset_from_bytes(nq.as_bytes(), NativeRdfFormat::NQuads)
        .map_err(|e| mf(format!("parse diagnostics N-Quads: {e}")))?;
    let (variables, rows) = run_select(
        &dataset,
        "SELECT ?s ?p ?o WHERE { { ?s ?p ?o } UNION { GRAPH ?g { ?s ?p ?o } } }",
    )
    .ok_or_else(|| mf("meta EDB extraction query must be a SELECT".to_owned()))?;
    let (Some(si), Some(pi), Some(oi)) = (
        variables.iter().position(|v| v == "s"),
        variables.iter().position(|v| v == "p"),
        variables.iter().position(|v| v == "o"),
    ) else {
        return Err(mf("meta EDB query missing a column".to_owned()));
    };
    let mut out = Vec::new();
    for sol in &rows {
        if let (Some(s), Some(p), Some(o)) = (
            sol.get(si).and_then(|t| t.as_ref()).and_then(iri_of),
            sol.get(pi).and_then(|t| t.as_ref()).and_then(iri_of),
            sol.get(oi).and_then(|t| t.as_ref()).and_then(iri_of),
        ) {
            out.push((s, p, o));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::model::{Finding, FindingCategory, Report, Severity};
    use gmeow_errors::render::to_gmeow_rdf;
    use std::path::PathBuf;

    /// The repo root, relative to this crate's manifest dir.
    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Build the meta program from the ACTUAL authored slices — the logic module
    /// (the rules) unioned with the diagnostics module (the polarity wiring) — so
    /// the test discovers exactly the rules the shipped ontology carries, by TYPE.
    fn authored_meta_program() -> MetaProgram {
        let root = repo_root();
        let logic = std::fs::read(root.join("slices/grounding/logic/module.ttl"))
            .expect("read logic module");
        let diagnostics = std::fs::read(root.join("slices/core/diagnostics/module.ttl"))
            .expect("read diagnostics module");
        let mut combined = logic;
        combined.push(b'\n');
        combined.extend_from_slice(&diagnostics);
        let dataset = dataset_from_bytes(&combined, NativeRdfFormat::Turtle)
            .expect("the combined slices parse as Turtle");
        MetaProgram::from_source_dataset(&dataset)
            .expect("the combined slices parse for the diagnostic meta-fold")
            .expect("the authored slices carry gmeow:DiagnosticMetaRule rules + polarity wiring")
    }

    /// A finding that already carries the ledger-witness identity fields the meta
    /// rules join on (`finding_iri`, and optionally an antecedent edge / anchor).
    fn witness_finding(
        iri: &str,
        code: &str,
        category: FindingCategory,
        antecedents: Vec<String>,
    ) -> Finding {
        let mut finding = Finding::new(Severity::Error, code, "boom")
            .with_tool("shacl")
            .with_category(category);
        finding.finding_iri = Some(iri.to_owned());
        finding.antecedents = antecedents;
        finding
    }

    fn finding_iri(local: &str) -> String {
        format!("{GMEOW_NS}diagnostics/finding/{local}")
    }

    #[test]
    fn shared_antecedent_report_gains_root_cause_in_nq_and_on_findings() {
        let meta = authored_meta_program();
        let (root, effect_a, effect_b) = (
            finding_iri("rootF1"),
            finding_iri("effectF2"),
            finding_iri("effectF3"),
        );
        let mut report = Report::new("shacl");
        // Two effects that each derive from the ONE childless root.
        report.add_finding(witness_finding(
            &root,
            "discipline/relator-mediation",
            FindingCategory::ModelingDisciplineViolation,
            Vec::new(),
        ));
        report.add_finding(witness_finding(
            &effect_a,
            "shacl.MinCountConstraintComponent",
            FindingCategory::DataShapeViolation,
            vec![root.clone()],
        ));
        report.add_finding(witness_finding(
            &effect_b,
            "shacl.NodeKindConstraintComponent",
            FindingCategory::DataShapeViolation,
            vec![root.clone()],
        ));

        let projected = to_gmeow_rdf(&report);
        let derivation = meta.derive(&projected).expect("meta derivation succeeds");

        // Both effects derive the shared childless root; the root itself derives none.
        assert!(
            derivation
                .root_cause
                .contains(&(effect_a.clone(), root.clone())),
            "effect A must derive gmeow:findingRootCause → root; got {:?}",
            derivation.root_cause
        );
        assert!(
            derivation
                .root_cause
                .contains(&(effect_b.clone(), root.clone())),
            "effect B must derive gmeow:findingRootCause → root"
        );

        // The derived nq carries the root-cause edge and the cluster grouping.
        let nq = derivation.to_nquads(GMEOW_NS);
        assert!(
            nq.contains(&format!("<{effect_a}> <{FINDING_ROOT_CAUSE}> <{root}>")),
            "derived nq must carry the findingRootCause edge:\n{nq}"
        );
        assert!(
            nq.contains(&format!("<{root}> <{RDF_TYPE}> <{FINDING_CLUSTER_CLASS}>")),
            "derived nq must type the shared root as a FindingCluster:\n{nq}"
        );

        // Enrichment lands the root cause + cluster on the effect findings.
        enrich_report(&mut report, &derivation);
        let ea = report
            .findings
            .iter()
            .find(|f| f.finding_iri.as_deref() == Some(effect_a.as_str()))
            .expect("effect A present");
        assert_eq!(ea.root_cause.as_deref(), Some(root.as_str()));
        assert_eq!(ea.cluster.as_deref(), Some(root.as_str()));
        // The childless root has no root cause of its own.
        let r = report
            .findings
            .iter()
            .find(|f| f.finding_iri.as_deref() == Some(root.as_str()))
            .expect("root present");
        assert_eq!(r.root_cause, None);
    }

    #[test]
    fn opposing_polarity_pair_mints_a_content_addressed_glut_witness() {
        let meta = authored_meta_program();
        let (supported, opposed) = (finding_iri("glutSupported"), finding_iri("glutOpposed"));
        let anchor = format!("{GMEOW_NS}diagnostics/anchor/shared0");

        // Two DIFFERENT-code findings at ONE non-trivial anchor whose category
        // polarities oppose (DataShapeViolation = Supported, PermittedEpistemicConflict
        // = Opposed).
        let mut supported_finding = Finding::new(
            Severity::Error,
            "shacl.MinCountConstraintComponent",
            "supported",
        )
        .with_tool("shacl")
        .with_category(FindingCategory::DataShapeViolation);
        supported_finding.finding_iri = Some(supported.clone());
        supported_finding.anchor_iri = Some(anchor.clone());
        supported_finding.anchor_non_trivial = true;

        let mut opposed_finding = Finding::new(
            Severity::Warning,
            "validate.deep.permitted-conflict",
            "opposed",
        )
        .with_tool("shacl")
        .with_category(FindingCategory::PermittedEpistemicConflict);
        opposed_finding.finding_iri = Some(opposed.clone());
        opposed_finding.anchor_iri = Some(anchor.clone());
        opposed_finding.anchor_non_trivial = true;

        let mut report = Report::new("shacl");
        report.add_finding(supported_finding);
        report.add_finding(opposed_finding);

        let projected = to_gmeow_rdf(&report);
        let derivation = meta.derive(&projected).expect("meta derivation succeeds");
        assert!(
            derivation
                .glut
                .contains(&(supported.clone(), opposed.clone())),
            "the opposing-polarity pair must derive gmeow:crossNodeGlutWith; got {:?}",
            derivation.glut
        );

        // The witness is materialized with a content-addressed IRI over the SORTED
        // pair + head predicate, and carries exactly two glutWitnessOf edges.
        let (lo, hi) = if supported <= opposed {
            (&supported, &opposed)
        } else {
            (&opposed, &supported)
        };
        let witness = glut_witness_iri(lo, hi);
        let nq = derivation.to_nquads(GMEOW_NS);
        assert!(
            nq.contains(&format!(
                "<{witness}> <{RDF_TYPE}> <{CROSS_NODE_GLUT_WITNESS_CLASS}>"
            )),
            "derived nq must mint the CrossNodeGlutWitness node:\n{nq}"
        );
        assert!(
            nq.contains(&format!("<{witness}> <{GLUT_WITNESS_OF}> <{supported}>"))
                && nq.contains(&format!("<{witness}> <{GLUT_WITNESS_OF}> <{opposed}>")),
            "the witness must link BOTH conflicting findings via glutWitnessOf:\n{nq}"
        );
        // The witness IRI is stable/deterministic and swap-invariant (content address).
        assert_eq!(witness, glut_witness_iri(hi, lo));
        // The witness carries its own well-formed grade (FindingShape).
        assert!(nq.contains(&format!(
            "<{witness}> <{FINDING_SEVERITY}> <{SEVERITY_NOTE}>"
        )));

        // Enrichment surfaces the symmetric glut edge on BOTH findings.
        enrich_report(&mut report, &derivation);
        let s = report
            .findings
            .iter()
            .find(|f| f.finding_iri.as_deref() == Some(supported.as_str()))
            .unwrap();
        assert_eq!(s.cross_node_glut_with, vec![opposed.clone()]);
        let o = report
            .findings
            .iter()
            .find(|f| f.finding_iri.as_deref() == Some(opposed.as_str()))
            .unwrap();
        assert_eq!(o.cross_node_glut_with, vec![supported.clone()]);
    }

    #[test]
    fn absent_meta_rules_yield_none() {
        // A source graph with polarity wiring but NO gmeow:DiagnosticMetaRule → None.
        let ttl = format!(
            "@prefix gmeow: <{GMEOW_NS}> .\n@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             logic:FindingDataShapeViolation gmeow:categoryPolarity logic:InfoSupported .\n"
        );
        let dataset =
            dataset_from_bytes(ttl.as_bytes(), NativeRdfFormat::Turtle).expect("parse minimal");
        assert!(
            MetaProgram::from_source_dataset(&dataset)
                .expect("a well-formed source parses")
                .is_none(),
            "a source without any gmeow:DiagnosticMetaRule must yield None"
        );
    }
}
