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
//! the production surface `conformance::corpus_tests::diagnostics_meta_findings`
//! proves over the actual authored ontology.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_errors::Report;
use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
    reason_program,
};
use gmeow_logic_compile::frontend::{
    CompiledTheory, Diagnostic, OwnerDisposition, OwnerFamily, OwnerLowering, PreparedLogicSource,
    SourceNode, default_source_statements,
};
use gmeow_logic_compile::ir::{LogicProgram, LogicRule};
use purrdf::sparql::{NativeSparqlEngine, PreparedQuery, QueryOptions};
use purrdf::{
    NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, SparqlResult, TermRef,
    TermValue, dataset_from_bytes,
};

use gmeow_ns::GMEOW_NS;
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

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
    engine: NativeSparqlEngine,
    fact_query: Arc<PreparedQuery>,
}

/// The reasoner-derived meta-findings, collected from one chase over a projected
/// finding graph. Every collection is a sorted set, so the re-projection and the
/// report enrichment are deterministic regardless of the chase's emission order.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MetaDerivation {
    /// Internal antecedent reachability evidence; retained for observation, not projected.
    pub traces: BTreeSet<(String, String)>,
    /// `(finding, root)` — each finding's traced childless-root antecedent.
    pub root_cause: BTreeSet<(String, String)>,
    /// `(finding, root)` — each finding's membership in the root-keyed cluster.
    pub cluster: BTreeSet<(String, String)>,
    /// The shared root carried once on the cluster node (the `gmeow:clusterRoot`
    /// self-edge subject; the rule head is `?root clusterRoot ?root`, so only the
    /// single root IRI is retained).
    pub cluster_root: BTreeSet<String>,
    /// Exact cluster-root edges retained before the projection selects their subjects.
    pub cluster_root_edges: BTreeSet<(String, String)>,
    /// Roots typed `gmeow:FindingCluster` (the grouping node).
    pub cluster_typed: BTreeSet<String>,
    /// Roots typed `gmeow:RootFinding` (the extensibility demonstrator).
    pub root_finding_typed: BTreeSet<String>,
    /// `(supported, opposed)` — the directed cross-node glut edges (Supported →
    /// Opposed), the raw material the `gmeow:CrossNodeGlutWitness` is minted from.
    pub glut: BTreeSet<(String, String)>,
}

impl MetaDerivation {
    /// Whether the fold has no public meta-findings to project or attach to a report.
    /// Internal trace evidence does not change that public projection.
    pub fn is_empty(&self) -> bool {
        self.root_cause.is_empty()
            && self.cluster.is_empty()
            && self.cluster_root.is_empty()
            && self.cluster_typed.is_empty()
            && self.root_finding_typed.is_empty()
            && self.glut.is_empty()
    }

    /// Project the derived public meta-findings directly into their native graph.
    /// Trace evidence remains on this derivation; only the governed public fields
    /// and materialized glut witnesses enter the diagnostics dataset.
    pub fn append_to(&self, builder: &mut RdfDatasetBuilder, graph_iri: &str) {
        let graph = builder.intern_iri(graph_iri);
        for (predicate, edges) in [
            (FINDING_ROOT_CAUSE, &self.root_cause),
            (FINDING_CLUSTER, &self.cluster),
        ] {
            for (finding, root) in edges {
                push_native_iri(builder, graph, finding, predicate, root);
            }
        }
        for root in &self.cluster_root {
            push_native_iri(builder, graph, root, CLUSTER_ROOT, root);
        }
        for (class, roots) in [
            (FINDING_CLUSTER_CLASS, &self.cluster_typed),
            (ROOT_FINDING_CLASS, &self.root_finding_typed),
        ] {
            for root in roots {
                push_native_iri(builder, graph, root, RDF_TYPE, class);
            }
        }
        for (a, b) in &self.glut {
            self.push_witness(builder, graph, graph_iri, a, b);
        }
    }

    /// The symmetric conflict has one content-addressed witness, with two links
    /// and its own assertional grade. The dataset applies RDF set semantics.
    fn push_witness(
        &self,
        builder: &mut RdfDatasetBuilder,
        graph: purrdf::TermId,
        graph_iri: &str,
        a: &str,
        b: &str,
    ) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let witness = glut_witness_iri(lo, hi);
        for (predicate, object) in [
            (RDF_TYPE, CROSS_NODE_GLUT_WITNESS_CLASS),
            (RDF_TYPE, FINDING_CLASS),
            (FINDING_SEVERITY, SEVERITY_NOTE),
            (FINDING_CATEGORY, FINDING_PERMITTED_CONFLICT),
            (FINDING_STANDPOINT, STANDPOINT_ADVISORY),
            (GLUT_WITNESS_OF, lo),
            (GLUT_WITNESS_OF, hi),
        ] {
            push_native_iri(builder, graph, &witness, predicate, object);
        }
        let label = format!(
            "cross-node glut between {} and {} at a shared anchor",
            short_finding(lo),
            short_finding(hi)
        );
        let message = format!(
            "cross-node glut: {lo} and {hi} carry opposing coherence polarity at one anchor"
        );
        gmeow_errors::abox::annotate_builder(builder, &witness, &label, &message, graph_iri);
        let subject = builder.intern_iri(&witness);
        for (predicate, value) in [
            (FINDING_CODE, GLUT_WITNESS_CODE),
            (FINDING_MESSAGE, message.as_str()),
        ] {
            let predicate = builder.intern_iri(predicate);
            let object = builder.intern_literal(purrdf::RdfLiteral::simple(value));
            builder.push_quad(subject, predicate, object, Some(graph));
        }
    }
}

fn push_native_iri(
    builder: &mut RdfDatasetBuilder,
    graph: purrdf::TermId,
    s: &str,
    p: &str,
    o: &str,
) {
    let subject = builder.intern_iri(s);
    let predicate = builder.intern_iri(p);
    let object = builder.intern_iri(o);
    builder.push_quad(subject, predicate, object, Some(graph));
}

impl MetaProgram {
    /// Parse the authored `gmeow:DiagnosticMetaRule` fold and the
    /// `gmeow:categoryPolarity` wiring out of the source graph N-Quads (the validate
    /// stage's base-graph bytes, which carry the logic + diagnostics slices).
    ///
    /// Returns `Ok(None)` when the source graph carries no meta-rules — a source
    /// without them derives nothing, so the projection stays byte-unchanged. A
    /// malformed source graph, or any selected `gmeow:DiagnosticMetaRule` subject
    /// that does not parse into a logic rule, is a HARD FAIL (`Err`): a real defect
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
    pub fn from_source_dataset(dataset: &RdfDataset) -> gmeow_errors::Result<Option<MetaProgram>> {
        if meta_rule_nodes(dataset).is_empty() {
            return Ok(None);
        }
        let source = PreparedLogicSource::new(dataset).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!("prepare authored diagnostic meta-rules: {error}"),
            })
        })?;
        Self::from_prepared_source(&source)
    }

    /// Select diagnostic meta-rules from an already canonicalized native source.
    /// The compiler shares this boundary with its other augmentation readers.
    ///
    /// # Errors
    /// Refuses malformed rules or any frontend error in the selected source.
    pub fn from_prepared_source(
        source: &PreparedLogicSource,
    ) -> gmeow_errors::Result<Option<MetaProgram>> {
        let meta_nodes = meta_rule_nodes(source.dataset());
        if meta_nodes.is_empty() {
            // The genuine "no meta-rules authored" case — nothing to derive.
            return Ok(None);
        }
        // The source graph DOES carry `gmeow:DiagnosticMetaRule` subjects, so a parse
        // failure past this point is a real defect, not an absence — surface it.
        let compiled = source.compile_with_sources(None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!("parse authored diagnostic meta-rules: {e}"),
            })
        })?;
        Self::from_compiled_parts(
            source.dataset(),
            compiled.program(),
            compiled.diagnostics(),
            compiled.owner_lowerings(),
            meta_nodes,
        )
    }

    /// Select the meta-rule program from the producer's exact shared compilation.
    /// Source kinds, original diagnostics and rule values are borrowed together;
    /// this path never reparses or lowers the source a second time.
    pub fn from_compiled_theory(theory: &CompiledTheory) -> gmeow_errors::Result<Option<Self>> {
        Self::from_compiled_theory_with_wiring(theory, theory.source().dataset())
    }

    /// Select the shared compiled rules and borrow separately admitted category wiring.
    /// This avoids combining and recompiling independent source documents.
    ///
    /// # Errors
    /// Refuses rejected source owners, invalid rules and query preparation failures.
    pub fn from_compiled_theory_with_wiring(
        theory: &CompiledTheory,
        wiring: &RdfDataset,
    ) -> gmeow_errors::Result<Option<Self>> {
        Self::from_compiled_parts(
            wiring,
            theory.program(),
            theory.diagnostics(),
            theory.owner_lowerings(),
            meta_rule_nodes(theory.source().dataset()),
        )
    }

    fn from_compiled_parts(
        wiring: &RdfDataset,
        program: &LogicProgram,
        diags: &[Diagnostic],
        owners: &[OwnerLowering],
        meta_nodes: BTreeSet<SourceNode>,
    ) -> gmeow_errors::Result<Option<Self>> {
        if meta_nodes.is_empty() {
            return Ok(None);
        }
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
        // Bind the class-based selection to actual owner emissions. Provenance is
        // authored evidence, never an index into this program's rule collection.
        let emitted: BTreeMap<_, _> = owners
            .iter()
            .filter(|owner| owner.family == OwnerFamily::Rule)
            .filter_map(|owner| match owner.disposition {
                OwnerDisposition::Emitted { index } => Some((owner.source, index)),
                OwnerDisposition::Rejected | OwnerDisposition::OutsideDefaultGraph => None,
            })
            .collect();
        let missing = meta_nodes
            .iter()
            .filter(|node| !emitted.contains_key(node))
            .count();
        if missing > 0 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!(
                    "{missing} of {} selected gmeow:DiagnosticMetaRule subject(s) did not emit a logic rule",
                    meta_nodes.len()
                ),
            }));
        }
        let rules: Vec<LogicRule> = meta_nodes
            .iter()
            .map(|node| program.rules[emitted[node]].clone())
            .collect();
        let category_polarity = native_iri_pairs(wiring, CATEGORY_POLARITY);
        let engine = NativeSparqlEngine::new();
        let fact_query = engine
            .prepare_query(
                "SELECT ?s ?p ?o WHERE { { ?s ?p ?o } UNION { GRAPH ?g { ?s ?p ?o } } }",
                None,
            )
            .map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                    message: format!("prepare diagnostic fact query: {error}"),
                })
            })?;
        Ok(Some(MetaProgram {
            program: LogicProgram::new(Vec::new(), rules, Vec::new(), None),
            category_polarity,
            engine,
            fact_query,
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
        let dataset = dataset_from_bytes(finding_nq.as_bytes(), NativeRdfFormat::NQuads).map_err(
            |error| {
                gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                    message: format!("parse diagnostic findings: {error}"),
                })
            },
        )?;
        self.derive_dataset(&dataset)
    }

    /// Derive over an existing native finding dataset using the retained query and program.
    ///
    /// # Errors
    /// Invalid query results, native dataset construction and reasoning fail closed.
    pub fn derive_dataset(
        &self,
        findings: &Arc<RdfDataset>,
    ) -> gmeow_errors::Result<MetaDerivation> {
        let mf = |message: String| gmeow_errors::Diag::of_kind(crate::error::MetaFold { message });
        let result = self
            .engine
            .query_prepared(findings, &self.fact_query, &[], QueryOptions::EMPTY)
            .map_err(|error| mf(format!("diagnostic fact query: {error}")))?;
        let SparqlResult::Solutions {
            variables, rows, ..
        } = result
        else {
            return Err(mf("diagnostic fact query must return solutions".to_owned()));
        };
        let column = |name: &str| {
            variables
                .iter()
                .position(|variable| variable == name)
                .ok_or_else(|| mf(format!("diagnostic fact query missing {name}")))
        };
        let (si, pi, oi) = (column("s")?, column("p")?, column("o")?);
        let mut builder = RdfDatasetBuilder::new();
        let mut fact_count = 0;
        for row in &rows {
            if let (Some(s), Some(p), Some(o)) = (
                row[si].as_ref().and_then(iri_of),
                row[pi].as_ref().and_then(iri_of),
                row[oi].as_ref().and_then(iri_of),
            ) {
                push_world(&mut builder, &s, &p, &o);
                fact_count += 1;
            }
        }
        if fact_count == 0 {
            return Ok(MetaDerivation::default());
        }
        for (c, p) in &self.category_polarity {
            push_world(&mut builder, c, CATEGORY_POLARITY, p);
        }
        let edb = builder.freeze().map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!("freeze meta-derivation EDB: {e}"),
            })
        })?;
        let reasoning_input = prepare_reasoning_input(&edb)?;
        let domains = SelectedDomains::new([SelectedLogicalWorld::new(
            LogicalGraph::Named(purrdf::TermValue::iri(WORLD)),
            DomainProfile::NonemptyObjectDomainV1,
            "gmeow.pipeline.diagnostics-meta.v1".to_owned(),
            *reasoning_input.ingress_contract(),
        )?])?;
        let result = reason_program(&self.program, reasoning_input, &domains).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                message: format!("reason diagnostic meta-rules: {e}"),
            })
        })?;

        let mut derivation = MetaDerivation::default();
        for atom in result.inferred() {
            if atom.is_edb {
                continue;
            }
            let object = atom
                .object
                .as_iri()
                .ok_or_else(|| {
                    gmeow_errors::Diag::of_kind(crate::error::MetaFold {
                        message: format!(
                            "diagnostic meta-rule emitted a non-resource object {:?}",
                            atom.object
                        ),
                    })
                })?
                .to_owned();
            match atom.predicate.as_str() {
                "https://blackcatinformatics.ca/gmeow/findingTraces" => {
                    derivation.traces.insert((atom.subject.clone(), object));
                }
                FINDING_ROOT_CAUSE => {
                    derivation.root_cause.insert((atom.subject.clone(), object));
                }
                FINDING_CLUSTER => {
                    derivation.cluster.insert((atom.subject.clone(), object));
                }
                CLUSTER_ROOT => {
                    // The rule head is `?root gmeow:clusterRoot ?root` (a self-edge),
                    // so subject == object — retain the single root IRI.
                    derivation
                        .cluster_root_edges
                        .insert((atom.subject.clone(), object));
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

/// The IRI string of a bound SPARQL term, or `None` if it is not an IRI.
fn iri_of(term: &TermValue) -> Option<String> {
    match term {
        TermValue::Iri(i) => Some(i.clone()),
        _ => None,
    }
}

/// Indexed default-graph selection matching the authored meta-rule context.
fn meta_rule_nodes(dataset: &RdfDataset) -> BTreeSet<SourceNode> {
    let (Some(predicate), Some(class)) = (
        dataset.term_id_by_iri(RDF_TYPE),
        dataset.term_id_by_iri(DIAGNOSTIC_META_RULE),
    ) else {
        return BTreeSet::new();
    };
    default_source_statements(dataset, None, Some(predicate), Some(class))
        .map(|quad| SourceNode {
            term: quad.s,
            graph: quad.g,
        })
        .collect()
}

/// Keep all authored default-graph IRI pairs, sorted and deduplicated.
fn native_iri_pairs(dataset: &RdfDataset, predicate: &str) -> Vec<(String, String)> {
    let Some(predicate) = dataset.term_id_by_iri(predicate) else {
        return Vec::new();
    };
    default_source_statements(dataset, None, Some(predicate), None)
        .filter_map(
            |quad| match (dataset.resolve(quad.s), dataset.resolve(quad.o)) {
                (TermRef::Iri(subject), TermRef::Iri(object)) => {
                    Some((subject.to_owned(), object.to_owned()))
                }
                _ => None,
            },
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[path = "meta_findings.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_support;
