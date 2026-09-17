// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, Docker-free reasoned-graph verify.
//!
//! The closed-world QC half of the hybrid OWL+SHACL architecture, in Rust. It
//! replaces the ROBOT `verify` Docker step: materialize the reasoned graph (the
//! asserted RDF-1.2 graph *unioned* with the native EL/DL derived subsumption /
//! type / equivalent-class edges), then run each `queries/verify/*.rq`
//! "bad-example" SPARQL SELECT over it. Any returned solution row is a
//! violation, surfaced as an `error` [`gmeow_errors::Finding`].
//!
//! The authority lives here (Principles 17/18): closure materialization, SPARQL
//! execution, and `Finding`/`Report` construction are all native. Python only
//! discovers the query files (repo/slice layout), calls in, and writes the
//! diagnostics artifacts.

use std::sync::Arc;

use gmeow_errors::{Finding, Location, Report, Severity};
use purrdf::sparql::{NativeSparqlEngine, PreparedQuery, QueryOptions};
use purrdf::{
    DatasetMut, MutableDataset, QuadValues, RdfDataset, RdfQuad, RdfTerm, SparqlResult, TermValue,
};

use crate::math_expression::{MATH_ALPHA_EQUIVALENCE_CLASS, MATH_ALPHA_EQUIVALENCE_CLASS_TYPE};
use crate::reason::dl::gaps_from_unsupported;
use crate::reason::reason_all;
use crate::result::ReasoningResult;

pub(crate) mod prepared_gates;
pub use prepared_gates::{GATE_SOURCES, PREPARED_GATES_CHANNEL, PreparedReasonedGates};

/// One selected query set and native law preparation reused across verification scenes.
/// The same reasoned-graph materialization and finding path backs every entry point.
pub struct PreparedVerification<'a> {
    gates: &'a PreparedReasonedGates,
    engine: NativeSparqlEngine,
    queries: Vec<(String, Arc<PreparedQuery>)>,
}

impl<'a> PreparedVerification<'a> {
    /// The exact immutable native law preparation used by this verifier.
    #[must_use]
    pub fn gates(&self) -> &PreparedReasonedGates {
        self.gates
    }

    /// Retain prepared query plans over the explicitly supplied native law identity.
    ///
    /// # Errors
    /// Rejects stale law identities and malformed selected queries.
    pub fn new(
        queries: &[(String, String)],
        gates: &'a PreparedReasonedGates,
    ) -> gmeow_errors::Result<Self> {
        gates.validate_source_identity()?;
        let engine = NativeSparqlEngine::new();
        let queries = queries
            .iter()
            .map(|(name, text)| {
                let prepared = engine.prepare_query(text, None).map_err(|error| {
                    gmeow_errors::Diag::of_kind(crate::error::Verify {
                        detail: format!("verify query {name} preparation error: {error}"),
                    })
                })?;
                Ok((name.clone(), prepared))
            })
            .collect::<gmeow_errors::Result<_>>()?;
        Ok(Self {
            gates,
            engine,
            queries,
        })
    }

    /// Evaluate the selected native logical worlds once using these retained laws and queries.
    ///
    /// # Errors
    /// Propagates the same reasoning, closure and query failures as [`verify`].
    pub fn verify(
        &self,
        edb: &RdfDataset,
        domains: &crate::physical::SelectedDomains,
    ) -> gmeow_errors::Result<Report> {
        let input = crate::reason::prepare_reasoning_input(edb)?;
        let result = reason_all(input, domains)?;
        self.verify_with_reasoning_result(edb, &result)
    }

    /// Reuse a caller-owned complete native result without another reasoning chase.
    ///
    /// # Errors
    /// Propagates the same materialization and query failures as [`verify_with_reasoning_result`].
    pub fn verify_with_reasoning_result(
        &self,
        edb: &RdfDataset,
        result: &ReasoningResult,
    ) -> gmeow_errors::Result<Report> {
        verify_prepared(edb, result, self)
    }

    /// Materialize a caller-owned closure under the same explicitly selected native laws.
    ///
    /// # Errors
    /// Propagates the same gate and insertion errors as [`materialize_reasoned_graph`].
    pub fn materialize_reasoned_graph(
        &self,
        edb: &RdfDataset,
        result: &ReasoningResult,
    ) -> gmeow_errors::Result<ReasonedGraphOutcome> {
        materialize_with_gates(edb, result, self.gates)
    }
}

/// Strip a single pair of angle brackets from an IRI term, if present.
///
/// The native engine emits subjects/predicates as bare IRI strings and objects
/// already wrapped in `<...>` (mirroring `reason.py::_iri_term`); this collapses
/// both to the bare IRI so `NamedNode::new` accepts them.
fn bare_iri(value: &str) -> &str {
    value
        .strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(value)
}

use crate::provenance::term_display;

// The compile-time-embedded verify "bad-example" query set: `VERIFY_QUERIES:
// &[(&str, &str)]`, `(stem, sparql_text)` pairs sorted by stem. Generated by
// `build.rs` from `queries/verify/*.rq` + `slices/**/queries/verify/*.rq` — see
// that file for the walk, the sort, and the stem-collision hard-fail.
include!(concat!(env!("OUT_DIR"), "/verify_queries.rs"));

/// The embedded verify bad-example query set as owned `(stem, sparql)` pairs,
/// sorted by stem.
///
/// Subsumes the former disk-walk loaders in `crates/pipeline` (`carrier.rs`)
/// and `gmeow-dev-cli` (`dev_reason.rs`): the query set is now compiled into
/// the binary (see `build.rs`), so no production caller reads `queries/verify/`
/// or `slices/**/queries/verify/` off disk at runtime.
pub fn embedded_verify_queries() -> Vec<(String, String)> {
    VERIFY_QUERIES
        .iter()
        .map(|(stem, sparql)| ((*stem).to_owned(), (*sparql).to_owned()))
        .collect()
}

/// Derive a stable, short check name from a repo-relative `.rq` path.
///
/// `queries/verify/axis-not-disjoint.rq` → `axis-not-disjoint`. Used for the
/// `verify.<name>` finding code; the full path is kept on the finding location.
fn query_stem(name: &str) -> &str {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .strip_suffix(".rq")
        .unwrap_or_else(|| name.rsplit('/').next().unwrap_or(name))
}

/// The materialized reasoned graph plus the DERIVED (non-EDB) predicate set every
/// verify query, obligation check, and math: gate evaluates against — the [`Ready`]
/// half of [`materialize_reasoned_graph`]'s outcome.
///
/// [`Ready`]: ReasonedGraphOutcome::Ready
pub struct ReasonedGraph {
    /// The flat asserted graph (default graph) unioned with the DL-derived (non-EDB)
    /// edges and alpha-equivalence projection. Full verification additionally
    /// includes its math-dimension and enactment-integrity markers.
    pub dataset: Arc<RdfDataset>,
    /// The predicate IRIs of the DERIVED (non-EDB) edges — the finite-closure oracle
    /// the non-entailment obligation check (Arm B) needs: a forbidden predicate that
    /// was *derived* (not merely asserted) is a violation.
    pub derived_predicates: std::collections::BTreeSet<String>,
}

/// The outcome of [`materialize_reasoned_graph`].
pub enum ReasonedGraphOutcome {
    /// The native reasoner decided every OWL construct present: the reasoned graph
    /// is complete and safe to check.
    Ready(ReasonedGraph),
    /// The native reasoner left a DL coverage gap (an OWL construct it could not
    /// decide): the reasoned closure may be INCOMPLETE, so checking it risks a false
    /// negative. Carries the SAME `verify.dl-gap.*` error findings plus the aborted
    /// `verify.native.summary` note [`verify_with_reasoning_result`] emits in this
    /// case, already fully built — the caller folds them into its own report instead
    /// of running any check over an untrustworthy closure.
    IncompleteClosure(Vec<Finding>),
}

/// An insert into the reasoned graph refused a relative IRI.
///
/// The insert validates that every IRI is absolute. Nothing spliced into the
/// reasoned graph may carry a scheme-less reference: a verify query joins on IRI
/// identity, so an unresolvable term would silently fail to match rather than
/// report anything, and the gate would pass by not looking.
fn reasoned_insert_err(e: impl std::fmt::Display) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Verify {
        detail: format!("reasoned-graph insert refused a relative IRI: {e}"),
    })
}

/// Materialize the full reasoned-verification graph: flatten `edb`'s default
/// graph, layer the native EL/DL closure's derived edges on top, run the authored
/// math-dimension and enactment-integrity gate programs, attach expression
/// identity, and freeze. If the reasoner left a DL coverage gap, return the gap
/// findings instead of an untrustworthy closure.
///
/// This is the SHARED first stage [`verify_with_reasoning_result`] (the embedded
/// `queries/verify/*.rq` bad-example battery + the typed-formalization obligations)
/// and full-gate consumers (for example `gmeow validate --deep` reasoning a
/// consumer's own partial data graph without the full-ontology-shaped query
/// battery, where fixed-vocabulary checks like
/// `axis-not-disjoint` would misfire on a bundle that never carries gmeow's own
/// identity-axis classes) build from.
///
/// One class of defect is NOT reported as a finding but as a hard `Err`: a reasoned
/// closure that derives an enactment-kernel effect record. A violation there means the
/// engine crossed the commitment layer's observed-not-derived boundary, which invalidates
/// the run rather than describing a fact about the data, so it aborts before any gate or
/// query runs (see [`crate::reason::enactment::reject_banned_heads`]).
///
/// # Errors
/// Returns `Err` if the dimension gate or the freeze fails, or if the reasoned closure
/// derives a `logic:EffectAttempt` / `logic:ExternalEffectReceipt` — the enactment
/// kernel's observed-not-derived boundary, rejected here because this is the shared
/// stage every reasoned-graph consumer builds from. It also returns `Err` if any
/// quad spliced into the reasoned graph names a relative IRI — see
/// [`reasoned_insert_err`].
pub fn materialize_reasoned_graph(
    edb: &RdfDataset,
    result: &ReasoningResult,
) -> gmeow_errors::Result<ReasonedGraphOutcome> {
    materialize_with_gates(edb, result, prepared_gates::shared())
}

/// Materialize the checked native closure and expression identity projection
/// without running the independent math-dimension and enactment-integrity gate
/// programs.
///
/// This is the typed projection for consumers that inspect consequences of one
/// selected reasoning scene rather than execute the repository's full
/// reasoned-verify contract. It still rejects incomplete DL coverage, malformed
/// RDF terms, and derived effect records, and it uses the same closure and
/// alpha-equivalence materialization as [`materialize_reasoned_graph`].
///
/// # Errors
/// Returns `Err` when closure materialization violates any of those shared
/// contracts.
pub fn materialize_reasoned_closure(
    edb: &RdfDataset,
    result: &ReasoningResult,
) -> gmeow_errors::Result<ReasonedGraphOutcome> {
    match materialize_closure_base(edb, result)? {
        ClosureBaseOutcome::Ready(base) => finalize_reasoned_graph(edb, base),
        ClosureBaseOutcome::IncompleteClosure(findings) => {
            Ok(ReasonedGraphOutcome::IncompleteClosure(findings))
        }
    }
}

struct ClosureBase {
    store: MutableDataset,
    derived_edges: Vec<RdfQuad>,
    derived_predicates: std::collections::BTreeSet<String>,
}

enum ClosureBaseOutcome {
    Ready(ClosureBase),
    IncompleteClosure(Vec<Finding>),
}

fn materialize_closure_base(
    edb: &RdfDataset,
    result: &ReasoningResult,
) -> gmeow_errors::Result<ClosureBaseOutcome> {
    // 1. Flat asserted graph (default graph; literals + owl:members lists kept).
    //    A no-GRAPH verify query then matches it, exactly like ROBOT's single
    //    merged reasoned graph. The native flatten re-materializes the RDF 1.2
    //    statement layer (rdf:reifies reifiers + annotations) into the default
    //    graph, byte-for-byte the same set the oxigraph FlattenToDefaultGraph path
    //    produced.
    let mut store = MutableDataset::new(Arc::new(RdfDataset::union(&[])));
    for quad in edb.flat_default_graph_quads() {
        store.insert(quad).map_err(reasoned_insert_err)?;
    }

    // 2. Native EL/DL closure; layer the derived (non-EDB) edges on top, also in
    //    the default graph. The native closure only materializes subsumption /
    //    type / equivalent-class edges, which is what the inferred-edge verify
    //    queries (class-in-two-disjoint-axes, class-without-stereotype) rely on.
    // The DL coverage gaps are reconstructed from the shared model's
    // unsupported-construct set via the one recipe `verdict_from_inferred` uses,
    // so the verify findings stay byte-identical. The committed bundle is
    // gap-zero, so this is empty on a healthy run.
    let gaps = gaps_from_unsupported(result.preservation.unsupported_constructs.iter());

    // 2a. Hard-fail on any DL coverage gap.
    //
    // A gap means the native reasoner could NOT genuinely decide the consequences
    // of one or more OWL constructs present in the bundle: the reasoned closure is
    // potentially INCOMPLETE for those constructs, so the negative-test queries
    // below may produce false negatives. Defense-in-depth: surface each gap as an
    // error Finding so `make verify` fails fast rather than silently passing an
    // incomplete closure. The committed bundle is genuinely gap-zero, so this
    // branch is dead on a healthy run — it fires only when a new undecided
    // construct is introduced without being wired into the native handler first.
    if !gaps.is_empty() {
        let mut findings: Vec<Finding> = gaps
            .iter()
            .map(|gap| {
                let mut finding = Finding::new(
                    Severity::Error,
                    format!("verify.dl-gap.{}", gap.code),
                    format!(
                        "DL coverage gap — reasoned closure may be incomplete: {} ({})",
                        gap.message, gap.code
                    ),
                )
                .with_tool("verify");
                finding.tags = vec![
                    "dl-coverage".to_owned(),
                    "reasoned-graph".to_owned(),
                    "incomplete-closure".to_owned(),
                ];
                finding
            })
            .collect();
        // Return early: the closure is incomplete, so running the verify queries
        // against it would be misleading. The gap findings above are sufficient
        // for the caller to diagnose and fix the coverage hole.
        findings.push(
            Finding::new(
                Severity::Note,
                "verify.native.summary",
                format!(
                    "native reasoned-graph verify: aborted — {} DL coverage gap(s) prevent a \
                     complete closure",
                    gaps.len()
                ),
            )
            .with_tool("verify"),
        );
        return Ok(ClosureBaseOutcome::IncompleteClosure(findings));
    }

    // The predicate IRIs of the DERIVED (non-EDB) edges — the finite-closure oracle
    // for the non-entailment obligation check (Arm B): a forbidden predicate that was
    // *derived* (not merely asserted) is a violation.
    let mut derived_predicates: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    // The DL-derived (non-EDB) edges, retained as quads so the reasoner-derived math
    // dimension gate below chases the SAME reasoned closure the verify queries do — not
    // merely the raw asserted EDB. A dimension-relevant triple (`math:hasDimension`,
    // `math:homogeneousOperand`, `math:integrand`, `math:withRespectTo`, or an `rdf:type`
    // classifying a dimension node) that is *derived* rather than asserted must still
    // reach the hard-fail gate. Native literal and quoted-triple objects are retained.
    let mut derived_edges: Vec<RdfQuad> = Vec::new();
    // The same derived edges as `(subject, predicate, object)` rows, for the enactment
    // kernel's observed-not-derived guard immediately below. Built here rather than
    // decoded back out of `derived_edges` because the bare IRI strings are already in
    // hand at this point.
    let mut derived_rows: Vec<(String, String, String)> = Vec::new();
    for ax in result.inferred() {
        if ax.is_edb {
            continue;
        }
        let subject = bare_iri(&ax.subject);
        let predicate = bare_iri(&ax.predicate);
        derived_predicates.insert(predicate.to_owned());
        store
            .insert(QuadValues {
                s: TermValue::iri(subject),
                p: TermValue::iri(predicate),
                o: ax.object.clone(),
                g: None,
            })
            .map_err(reasoned_insert_err)?;
        derived_edges.push(RdfQuad::new(
            RdfTerm::iri(subject),
            predicate,
            crate::reason::term_value_to_rdf_term(&ax.object)?,
        ));
        let object = ax.object.as_iri().map_or_else(
            || crate::provenance::term_display(&ax.object),
            str::to_owned,
        );
        derived_rows.push((subject.to_owned(), predicate.to_owned(), object));
    }

    // The enactment kernel's observed-not-derived guard, over the REASONED CLOSURE — the
    // derived (non-EDB) edges just materialized, i.e. what the shipped reasoner actually
    // concluded from the bundle's own rules. Run unconditionally and FIRST, before any
    // gate-marker work, because it is the one check whose failure means the engine crossed
    // the commitment layer's hardest boundary: effect attempts and receipts are records of
    // what happened in the world, and a reasoner that could conclude an attempt happened
    // could conclude the world changed. If that inference is in the closure, there is
    // nothing worth gating afterwards.
    //
    // The authored `logic:EffectRecordsAreObservedNotDerivedConstraint` says the same
    // thing, but a constraint only binds if it is actually run, so the rule is carried here
    // as a Rust-side guard too — and it HARD-FAILS rather than filtering the offending row
    // away, since dropping it would preserve the invariant in the output while hiding the
    // defect that produced it.
    //
    // ASSERTED effect records never reach this call: `derived_edges` skips every `is_edb`
    // axiom, so an attempt the dispatching organ wrote down stays a legitimate observation
    // the verify queries reason about like any other data.
    crate::reason::enactment::reject_banned_heads(&derived_rows)?;

    Ok(ClosureBaseOutcome::Ready(ClosureBase {
        store,
        derived_edges,
        derived_predicates,
    }))
}

fn finalize_reasoned_graph(
    edb: &RdfDataset,
    base: ClosureBase,
) -> gmeow_errors::Result<ReasonedGraphOutcome> {
    let ClosureBase {
        store,
        derived_edges: _,
        derived_predicates,
    } = base;
    let dataset = store.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Verify {
            detail: format!("freeze reasoned graph failed: {e}"),
        })
    })?;

    // One asserted-graph derivation supplies the in-process verifier and the
    // shipped closure, so accepted expression roots expose identical joinable
    // alpha-class individuals on both surfaces.
    let alpha_edges = crate::math_expression::alpha_equivalence_edges(edb);
    if alpha_edges.is_empty() {
        return Ok(ReasonedGraphOutcome::Ready(ReasonedGraph {
            dataset,
            derived_predicates,
        }));
    }
    let mut with_alpha = MutableDataset::new(Arc::clone(&dataset));
    for (root, alpha_class) in alpha_edges {
        with_alpha
            .insert(QuadValues {
                s: TermValue::iri(root),
                p: TermValue::iri(MATH_ALPHA_EQUIVALENCE_CLASS),
                o: TermValue::iri(alpha_class.clone()),
                g: None,
            })
            .map_err(reasoned_insert_err)?;
        with_alpha
            .insert(QuadValues {
                s: TermValue::iri(alpha_class),
                p: TermValue::iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
                o: TermValue::iri(MATH_ALPHA_EQUIVALENCE_CLASS_TYPE),
                g: None,
            })
            .map_err(reasoned_insert_err)?;
    }
    let dataset = with_alpha.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Verify {
            detail: format!("freeze reasoned graph with alpha-equivalence classes failed: {e}"),
        })
    })?;
    Ok(ReasonedGraphOutcome::Ready(ReasonedGraph {
        dataset,
        derived_predicates,
    }))
}

fn materialize_with_gates(
    edb: &RdfDataset,
    result: &ReasoningResult,
    gates: &PreparedReasonedGates,
) -> gmeow_errors::Result<ReasonedGraphOutcome> {
    let base = match materialize_closure_base(edb, result)? {
        ClosureBaseOutcome::Ready(base) => base,
        ClosureBaseOutcome::IncompleteClosure(findings) => {
            return Ok(ReasonedGraphOutcome::IncompleteClosure(findings));
        }
    };
    let ClosureBase {
        mut store,
        derived_edges,
        derived_predicates,
    } = base;

    // The reasoner-derived `math:` dimensional-homogeneity gate: compiles the two
    // builtin-bound-consequent `logic:Constraint`s authored in `slices/grounding/math/
    // module.ttl` into VIOLATION-EMITTING forward rules and materializes
    // `math:DimensionalInhomogeneity` markers from the authored laws over the SAME
    // reasoned closure the verify queries below evaluate — the asserted EDB UNIONED with
    // the DL-derived edges layered into `store` just above (`derived_edges`), never the
    // raw pre-inference graph — so a dimension edge that is *derived* rather than
    // asserted is gated too. The gate runs its own literal-preserving forward chase (the
    // typed EDB fact stream drops the default-graph literal exponent cells the ℚ⁷
    // dimension builtins read on demand), but over this reasoned input. Always-on (no
    // flag); the marker is an ordinary `rdf:type` triple spliced into `store` before the
    // freeze below, so the `dimensional-inhomogeneity.rq` verify query (and every
    // obligation check below) renders it like any other row — never a Rust side-channel
    // finding.
    for (subject, failure_class) in crate::reason::math_gate::dimension_gate_markers_with_rules(
        edb,
        &derived_edges,
        &gates.math_rules,
    )? {
        store
            .insert(QuadValues {
                s: TermValue::iri(subject),
                p: TermValue::iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
                o: TermValue::iri(failure_class),
                g: None,
            })
            .map_err(reasoned_insert_err)?;
    }

    // The reasoner-derived enactment-kernel gate: compiles the enactment `logic:Constraint`s
    // authored in `slices/grounding/logic/module.ttl` into VIOLATION-EMITTING forward rules
    // (each law's antecedent plus its NEGATED consequent) and materializes
    // `logic:EnactmentIntegrityViolation` markers from those laws over the SAME reasoned
    // closure the verify queries below evaluate — the asserted EDB UNIONED with the
    // DL-derived edges layered into `store` just above — so a kernel record whose type or
    // binding is *derived* rather than asserted is gated too. The marker is an ordinary
    // `rdf:type` triple spliced into `store` before the freeze below, so the
    // `enactment-integrity-violation.rq` verify query renders it like any other row — never
    // a Rust side-channel finding.
    //
    // Its marker output is put through the observed-not-derived guard a second time on the
    // way out: a gate that materializes markers from authored laws is itself a derivation,
    // so it is held to the boundary exactly like the closure above. The boundary is enforced
    // on the broadest surface by the unconditional pass over `derived_rows` above; this pass
    // binds the gate's own output specifically.
    //
    // Each finding is spliced in as TWO quads, not one: the `rdf:type` marker naming the
    // condemned record, and a `logic:violatedLaw` edge naming the authored
    // `logic:Constraint` that condemned it. The kernel shares one failure class across all
    // forty of its laws deliberately, so the marker alone tells an operator that a record
    // breached enactment integrity and not WHICH obligation it broke.
    //
    // The law edge is the one the CHASE derived — every violation rule heads on
    // `logic:violatedLaw` precisely so that two laws condemning one record stay two
    // distinct derived tuples instead of collapsing into a single shared marker with one
    // surviving provenance. The `rdf:type` marker is minted here from the law's authored
    // `gmeow:enforcesFailureClass`: which class a law's findings carry is a property of
    // the law, so writing it down is a projection of what the author declared and not a
    // second decision about the record.
    let kernel_markers = crate::reason::enactment::enactment_gate_markers_with_laws(
        edb,
        &derived_edges,
        &gates.enactment,
    )?;
    crate::reason::enactment::reject_banned_heads(
        &kernel_markers
            .iter()
            .map(|violation| {
                (
                    violation.subject.clone(),
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
                    violation.failure_class.clone(),
                )
            })
            .collect::<Vec<_>>(),
    )?;
    for violation in kernel_markers {
        store
            .insert(QuadValues {
                s: TermValue::iri(violation.subject.clone()),
                p: TermValue::iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
                o: TermValue::iri(violation.failure_class),
                g: None,
            })
            .map_err(reasoned_insert_err)?;
        store
            .insert(QuadValues {
                s: TermValue::iri(violation.subject),
                p: TermValue::iri("https://blackcatinformatics.ca/logic/violatedLaw"),
                o: TermValue::iri(violation.law),
                g: None,
            })
            .map_err(reasoned_insert_err)?;
    }

    finalize_reasoned_graph(
        edb,
        ClosureBase {
            store,
            derived_edges,
            derived_predicates,
        },
    )
}

/// Run the reasoned-graph negative tests natively over `edb` and an already-built closure.
///
/// Materializes a flat oxigraph store = the asserted graph (flattened to the
/// default graph, literals and `owl:members` RDF lists preserved) unioned with
/// the native non-EDB derived edges from `result`, then evaluates
/// each `(name, sparql)` SELECT query against it. A query returning any rows is
/// a violation → an `error` finding (offending bindings in `detail`, the query
/// path as the finding location). A trailing `note` summarizes the run.
///
/// Never panics on a violation; the caller inspects [`Report::ok`]. The returned
/// report is NOT yet normalized (the PyO3 layer normalizes before serializing).
///
/// # Errors
///
/// Returns `Err` if a query fails to parse/evaluate, if a query is not a
/// SELECT, or if a derived edge cannot be built as a quad.
pub fn verify_with_reasoning_result(
    edb: &RdfDataset,
    result: &ReasoningResult,
    queries: &[(String, String)],
) -> gmeow_errors::Result<Report> {
    PreparedVerification::new(queries, prepared_gates::shared())?
        .verify_with_reasoning_result(edb, result)
}

fn verify_prepared(
    edb: &RdfDataset,
    result: &ReasoningResult,
    prepared: &PreparedVerification<'_>,
) -> gmeow_errors::Result<Report> {
    let mut report = Report::new("verify");
    let ReasonedGraph {
        dataset: reasoned,
        derived_predicates,
    } = match prepared.materialize_reasoned_graph(edb, result)? {
        ReasonedGraphOutcome::Ready(graph) => graph,
        ReasonedGraphOutcome::IncompleteClosure(findings) => {
            for finding in findings {
                report.add_finding(finding);
            }
            return Ok(report);
        }
    };
    // 3. Evaluate each verify query; any solution row is a violation.
    let mut violations = 0usize;
    for (name, query) in &prepared.queries {
        let stem = query_stem(name);
        let result = prepared
            .engine
            .query_prepared(&reasoned, query, &[], QueryOptions::EMPTY)
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Verify {
                    detail: format!("verify query {name} evaluation error: {e}"),
                })
            })?;

        let (variables, result_rows) = match result {
            SparqlResult::Solutions {
                variables, rows, ..
            } => (variables, rows),
            SparqlResult::Boolean(_) | SparqlResult::Graph(_) => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Verify {
                    detail: format!(
                        "verify query {name} must be a SPARQL SELECT (got ASK or \
                         CONSTRUCT/DESCRIBE)"
                    ),
                }));
            }
        };

        let mut rows: Vec<String> = Vec::new();
        // The structured evidence-term citation surface: ONLY genuine
        // `TermValue::Iri` bindings, never a literal's lexical form — so an
        // agent-controlled overlay literal like `"see <urn:fake>"` can never be
        // mistaken for a citation (unlike a text-scrape over the rendered
        // `detail` string below, which cannot distinguish a bare IRI term from
        // angle-bracket text inside a quoted literal).
        let mut cited_iris: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for sol in &result_rows {
            let mut binding: Vec<String> = Vec::new();
            for (var, cell) in variables.iter().zip(sol.iter()) {
                // Unbound variables are omitted, mirroring the prior oxigraph
                // `QuerySolution::iter()` which only yields bound (var, term) pairs.
                if let Some(term) = cell {
                    binding.push(format!("{}={}", var, term_display(term)));
                    if let purrdf::TermValue::Iri(iri) = term {
                        cited_iris.insert(iri.clone());
                    }
                }
            }
            // Sort the per-row bindings so the joined detail is independent of the
            // query engine's variable-projection / iteration order — keeping the
            // report content hash and the GTS feedback bundle byte-deterministic.
            binding.sort();
            rows.push(binding.join(", "));
        }

        if rows.is_empty() {
            continue;
        }
        violations += 1;
        // Sort the offending rows so the finding detail (and thus the report
        // content hash / GTS bundle) is deterministic.
        rows.sort();
        let mut finding = Finding::new(
            Severity::Error,
            format!("verify.{stem}"),
            format!(
                "{stem}: {} offending row(s) on the reasoned graph",
                rows.len()
            ),
        )
        .with_tool("verify");
        finding.detail = Some(rows.join("; "));
        finding.cited_iris = cited_iris.into_iter().collect();
        finding.tags = vec!["reasoned-graph".to_owned(), "negative-test".to_owned()];
        // Graph-located findings still need a physicalLocation for SARIF
        // code-scanning upload: anchor on the query's repo-relative path.
        finding.add_location(Location::new(Some(name.clone()), None, None, None));
        report.add_finding(finding);
    }

    // 4. Typed formalization governance (LOGIC-FOUNDATION.md, §Typed formalization
    //    governance): the executable non-entailment obligation checks (Arm A —
    //    syntactic reachability over the foundation rule strata, plus the
    //    unwired-discharge hard error) and the per-category formalization-candidate
    //    coverage report (with the uncategorized-candidate hard error). Arm B (finite
    //    closure) is the non-entailment-*.rq negative tests already run above. These
    //    read the reasoned `store` directly — Rust authority, surfaced through the
    //    already-wired `make verify` gate.
    for finding in
        crate::obligations::check_non_entailment_obligations(&reasoned, &derived_predicates)?
    {
        report.add_finding(finding);
    }
    for finding in crate::obligations::formalization_coverage(&reasoned)? {
        report.add_finding(finding);
    }
    // Recompute-and-enforce logic:candidateSourceHash drift: gives the "a later prose
    // edit surfaces as drift" governance claim executable teeth the presence-only SHACL
    // shape cannot express — a stale hash on a harvested candidate is a hard error.
    for finding in crate::obligations::check_candidate_source_hash_drift(&reasoned)? {
        report.add_finding(finding);
    }
    // The soft-advice peer: an advisory logic:Constraint whose logic:message must mirror its
    // logic:formalizes term's gmeow:avoidWhen prose (declared via logic:adviceSourceField) is
    // held to that prose by a direct string binding — a diverged advice message is a hard error,
    // so the surfaced advice can never silently drift from the prose it formalizes.
    for finding in crate::obligations::check_advice_message_prose_binding(&reasoned)? {
        report.add_finding(finding);
    }
    // 5. The math: measure-and-dimension reasoned gate — dimensional homogeneity,
    //    integral dimensional composition, math:dimensionVector string drift, and the
    //    positive-definiteness of every authored math:GramMatrix used as a metric form.
    //    Each is computed THROUGH the one exact-rational (ℚ⁷) gmeow_math source over
    //    this same frozen reasoned graph, never asserted data; a violation is a
    //    Severity::Error Finding naming its typed math: failure class. It is the
    //    executable lowering of the math: dimensional-homogeneity laws and the Gram
    //    positive-definiteness constraint (the sole positive-definiteness enforcement
    //    point the runtime distance builtin trusts).
    for finding in crate::math_dimension::check_math_dimension_findings(&reasoned) {
        report.add_finding(finding);
    }
    // 6. The math: expression-identity reasoned gate — recomputed math:structuralKey
    //    drift, math:NormalizationDeclaration surface leaks, and a claimed structural key
    //    on an expression the math: lowering rejects. Runs alongside the measure-and-
    //    dimension gate above, over this same frozen reasoned graph.
    for finding in crate::math_expression::check_math_expression_findings(edb, &reasoned) {
        report.add_finding(finding);
    }

    report.add_finding(
        Finding::new(
            Severity::Note,
            "verify.native.summary",
            format!(
                "native reasoned-graph verify: {} quer{} run, {violations} with violations",
                prepared.queries.len(),
                if prepared.queries.len() == 1 {
                    "y"
                } else {
                    "ies"
                }
            ),
        )
        .with_tool("verify"),
    );

    Ok(report)
}

/// Run native reasoned-graph verify, computing the EL/DL closure internally.
///
/// Call [`verify_with_reasoning_result`] when the caller has already run
/// [`reason_all`] and needs to avoid a second native chase.
pub fn verify(
    edb: &RdfDataset,
    queries: &[(String, String)],
    domains: &crate::physical::SelectedDomains,
) -> gmeow_errors::Result<Report> {
    let input = crate::reason::prepare_reasoning_input(edb)?;
    let result = reason_all(input, domains)?;
    verify_with_reasoning_result(edb, &result, queries)
}

#[path = "verify.tests.rs"]
#[cfg(test)]
mod tests;
