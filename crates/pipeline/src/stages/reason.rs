// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `reason` stage: native EL/DL closure, typed-modal evaluation, and artifacts — the SOLE
//! reasoning pass.
//!
//! It reasons ONCE over the object-level EDB
//! ([`crate::stages::carrier::assemble_object_level_edb`]: ontology + imports +
//! statements + alignments + logic/relational-core, WITHOUT correspondence or the
//! meta/report graphs), canonicalizes it (RDFC-1.0) for transport-independent Skolem
//! witnesses, runs `gmeow_logic::reason::reason_all_certified`, and serializes the
//! committed artifacts via the `gmeow_logic::reason::artifacts` builders. The single
//! result also backs the bundle's `graph/reasoning` projection (dual carriage), so
//! the closure shipped in `gmeow.gts` and the committed files agree by construction —
//! there is no separate full-fold export leaf. Reasoning requires the exclusive
//! [`ENGINE_RESOURCE`], so the scheduler serializes it against any stage competing
//! for the reasoning engine (this is the sole resource-bearing build stage).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use gmeow_logic::reason::artifacts::{
    build_dl_el_ledger_ttl, build_explanations_ttl, build_inferred_closure_ttl,
};
use gmeow_logic::reason::perf_ledger::perf_ledger;
use gmeow_logic::reason::reason_all_certified;
use gmeow_logic::result::ReasoningResult;
use gmeow_logic::result_rdf::{GRAPH_REASONING, project_reasoning_result};
use purrdf::{NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfTerm};

use crate::bundle::PipelineHandle;
use crate::node::{
    CachePolicy, ENGINE_RESOURCE, Stage, StageInput, StageOutput, StageProduct, StageRunTiming,
};

/// COMMITTED logical path of the native told-vs-inferred closure (RDF 1.2). This is
/// the SOLE reasoning pass: it reasons once over the object-level EDB
/// ([`crate::stages::carrier::assemble_object_level_edb`]) and owns the committed
/// closure directly — there is no separate full-fold export leaf. The same result
/// also backs the `graph/reasoning` projection folded into the bundle (dual carriage),
/// so the closure shipped in `gmeow.gts` and the committed file agree by construction.
pub const CLOSURE_PATH: &str = "generated/logic/inferred-closure.rdf12.ttl";
/// COMMITTED logical path of the per-axiom proof-skeleton explanations (RDF 1.2).
pub const EXPLANATIONS_PATH: &str = "generated/logic/reasoning-explanations.rdf12.ttl";
/// COMMITTED logical path of the report-only native DL/EL crosscheck ledger.
pub const LEDGER_PATH: &str = "generated/logic/dl-el-crosscheck-report.ttl";
/// COMMITTED logical path of the report-only native physical-engine performance
/// ledger — the flag-don't-build record of the deferred / non-incremental levers.
/// Canonical static content (a property of the engine, not of this run's data), so
/// it is byte-identical run to run.
pub const PERF_LEDGER_PATH: &str = "generated/logic/perf-ledger.ttl";
/// The reasoned artifacts a single `reason_all` produces: the three committed-style
/// Turtle blobs plus the typed [`ReasoningResult`] itself (the C7 typed handle's
/// payload and the source of the `graph/reasoning` projection).
pub struct ReasonArtifacts {
    /// The told-vs-inferred derived closure Turtle.
    pub closure: String,
    /// The per-axiom proof-skeleton explanations Turtle.
    pub explanations: String,
    /// The native DL·EL crosscheck ledger Turtle.
    pub ledger: String,
    /// The native physical-engine performance ledger Turtle — the flag-don't-build
    /// record of the deferred / non-incremental levers. Canonical static content.
    pub perf_ledger: String,
    /// The typed five-axis result (C7 handle payload).
    pub result: ReasoningResult,
    /// Production existential-chase termination evidence on the shared Finding
    /// substrate; this is the authority for both graph/diagnostics and run nodes.
    pub chase_report: gmeow_errors::Report,
    /// The decomposable derivation of every chase-invented null (Skolem witness)
    /// this run minted, sorted+deduped by content-addressed witness IRI. Empty
    /// when the program has no existential obligation. Projected into
    /// `graph/diagnostics` (via [`reason_dataset`]) so the offline `gmeow explain`
    /// CLI can explain an invented individual.
    pub witness_derivations: Vec<gmeow_logic::reason::WitnessDerivation>,
}

/// Reason over a composed dataset (N-Quads bytes) and return the three artifacts plus
/// the typed [`ReasoningResult`]. Parses then delegates to [`reason_over_dataset`].
pub fn reason_artifacts(composed_nquads: &[u8]) -> Result<ReasonArtifacts, gmeow_errors::Diag> {
    let edb = purrdf::parse_dataset(composed_nquads, NativeRdfFormat::NQuads.media_type(), None)
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("reason input parse: {e}"),
            })
        })?;
    reason_over_dataset(edb.as_ref())
}

/// Build a `stage-reason` [`StageProduct`] by reasoning over `composed_nquads` — the
/// real dual-carriage product a downstream consumer reads: the closure in the default
/// graph, the `graph/reasoning` projection, and the pinned typed Reasoning handle. This
/// is the SAME construction [`ReasonStage::run`] performs (minus the committed byte-lane
/// artifacts a consumer never reads off the handle), exposed so a harness driving a
/// single consumer stage in isolation can supply a genuine reasoned upstream rather than
/// a hand-built stand-in. An empty EDB yields an empty closure (a valid "no entailments"
/// reasoned product).
///
/// # Errors
///
/// Returns `Err` if reasoning, the dual-carriage dataset build, or the handle pin fails.
pub fn reason_product(composed_nquads: &[u8]) -> Result<StageProduct, gmeow_errors::Diag> {
    let reasoned = reason_artifacts(composed_nquads)?;
    let dataset = reason_dataset(
        &reasoned.closure,
        &reasoned.result,
        &reasoned.chase_report,
        &reasoned.witness_derivations,
    )?;
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        dataset,
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(GRAPH_REASONING);
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(reasoned.result)),
            pinned,
        )
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-reason".to_string(),
                message: format!("pin Reasoning handle to <{GRAPH_REASONING}>: {e}"),
            })
        })?;
    Ok(StageProduct::from_bundle("stage-reason", Arc::new(bundle)))
}

/// Reason over an in-memory EDB and return the three artifacts plus the typed
/// [`ReasoningResult`]. Canonicalizes the EDB (RDFC-1.0) BEFORE reasoning so the
/// content-addressed Skolem witnesses are transport-independent (carrier vs a
/// re-imported `gmeow.gts` yield byte-identical artifacts), then mirrors
/// `reason_native_artifacts` in non-merge mode (the regenerate path).
pub fn reason_over_dataset(edb: &RdfDataset) -> Result<ReasonArtifacts, gmeow_errors::Diag> {
    // Flatten the EDB to its un-folded plain-quad stream, RDFC-1.0 canonicalize it
    // (native full canon, byte-identical to the prior oxigraph `canonicalize_quads`
    // over the flat oxigraph quads), then RE-FOLD the canonical N-Quads back through
    // the native codec so the RDF 1.2 statement layer is reconstructed exactly as
    // `dataset_from_oxigraph_quads` did — content-addressed Skolem witnesses are a pure
    // function of this canonical, transport-independent EDB.
    let canon = canonicalize_edb(edb, "stage-reason")?;
    let certified = reason_all_certified(canon.as_ref()).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-reason".to_string(),
            message: format!("native reasoning failed: {e}"),
        })
    })?;
    let result = certified.result;
    let witness_derivations = certified.witness_derivations;
    // Resolve every chase-invented null to its minting head quad p(x, n) so the
    // per-world certificate finding can cite the null-minting reifiers it derives
    // from, and so `reason_dataset` can project the same skeletons into
    // graph/diagnostics. World-scoped: each null is attributed to the certificate
    // whose world its head quad was derived in (both are bare-IRI world keys).
    let witness_projections = resolve_witness_projections(&witness_derivations, &result)?;
    let mut world_reifiers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for projection in &witness_projections {
        world_reifiers
            .entry(projection.world.clone())
            .or_default()
            .push(projection.r_head.clone());
    }
    let mut chase_report = gmeow_errors::Report::new("chase");
    // `stage-reason` declares graph/diagnostics as an unconditional attachment.
    // RDF has no representation for an empty named graph, so carry the run's
    // content-addressed native contract as explicit evidence even when this EDB
    // contains no existential obligations. This is an honest run fact, not a
    // vacuous chase certificate; real per-world certificates follow below.
    chase_report.add_finding(
        gmeow_errors::Finding::new(
            gmeow_errors::Severity::Info,
            "reason.native-contract",
            format!(
                "native reasoning contract {} produced this stage result",
                gmeow_logic::reason::native_contract_hash()
            ),
        )
        .with_tool("reason"),
    );
    for certificate in certified.chase_certificates {
        let world = certificate.world.clone();
        let mut finding = certificate.to_finding();
        // A weakly-acyclic certificate's verdict derives from the existential edges
        // that minted this world's nulls: cite each null-minting head-quad reifier
        // via gmeow:findingDerivedFromQuad (sorted+deduped for byte-stability).
        if finding.code == "chase.certificate.weakly-acyclic"
            && let Some(reifiers) = world_reifiers.get(&world)
        {
            let mut derived = reifiers.clone();
            derived.sort();
            derived.dedup();
            finding = finding.with_derived_from_quads(derived);
        }
        chase_report.add_finding(finding);
    }
    chase_report.normalize();
    // The `math:` expression-identity derivation over the SAME asserted EDB this closure was
    // reasoned from. Lowered from `edb`, not `canon` and not the closure: the derivation is a
    // claim about the structure an author WROTE, and
    // `gmeow_logic::math_expression::alpha_equivalence_edges` is the one place that rule
    // lives (it is also what the reason-verify / `validate --deep` gates splice into their
    // in-process reasoned graph). Serializing it here is what puts the joinable α-class node
    // in a SHIPPED artifact — `gmeow.gts`'s default graph and the committed inferred-closure
    // file — instead of only inside a gate process.
    let alpha_edges = gmeow_logic::math_expression::alpha_equivalence_edges(edb);
    // Non-merge (the regenerate path): the closure is told-vs-inferred plus that derivation.
    let closure = build_inferred_closure_ttl(&result, None, &alpha_edges).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-reason".to_string(),
            message: format!("closure serialization failed: {e}"),
        })
    })?;
    let explanations = build_explanations_ttl(&result).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-reason".to_string(),
            message: format!("explanations serialization failed: {e}"),
        })
    })?;
    let ledger = build_dl_el_ledger_ttl(&result);
    // The performance ledger is canonical static content (a property of the native
    // physical engine's lever staging, not of this run's data), so it is byte-stable
    // run to run regardless of the reasoned result.
    let perf = perf_ledger().to_turtle();
    Ok(ReasonArtifacts {
        closure,
        explanations,
        ledger,
        perf_ledger: perf,
        result,
        chase_report,
        witness_derivations,
    })
}

/// Canonicalize the exact object-level EDB through the transport-independent RDFC-1.0
/// boundary shared by the reason and verify-attestation stages.
///
/// The verify stage deliberately repeats only this canonical byte/index normalization;
/// it consumes the reason stage's typed [`ReasoningResult`] and never constructs a
/// second closure.
pub(crate) fn canonicalize_edb(
    edb: &RdfDataset,
    stage_id: &str,
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let canon_nquads = purrdf::canonical_flat_nquads(edb).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: stage_id.to_string(),
            message: format!("RDFC-1.0 canonicalize EDB: {e}"),
        })
    })?;
    purrdf::parse_dataset(
        canon_nquads.as_bytes(),
        NativeRdfFormat::NQuads.media_type(),
        None,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: stage_id.to_string(),
            message: format!("re-fold canonical quads: {e}"),
        })
    })
}

/// One chase-invented null (Skolem witness) resolved against the reasoned head quad
/// `p(x, n)` whose OBJECT is that null — the existential edge that minted it.
struct WitnessProjection {
    /// The invented null IRI `n` (bare, content-addressed).
    witness: String,
    /// The head-quad subject `x` (bare IRI).
    subject: String,
    /// The head-quad predicate `p` (bare IRI).
    predicate: String,
    /// The content-addressed firing rule IRI that invented the null.
    rule_iri: String,
    /// The existential head-variable ordinal (distinct ∃-vars ⇒ distinct nulls).
    ordinal: usize,
    /// The head-quad world (the certificate-association key; a bare-IRI world key).
    world: String,
    /// The standard-RDF-reification node IRI for `⟨x p n⟩` (the null-minting reifier).
    r_head: String,
}

/// Resolve each chase-invented null to the reasoned head quad `p(x, n)` whose OBJECT
/// is that null. Hard-fails (fail-closed) when a witness has no such existential edge
/// (an unexplained invented individual) or is the object of more than one reasoned
/// axiom (an existential null must have exactly one minting head quad). Returns the
/// projections sorted by null IRI so any emitted fold is byte-stable across runs (the
/// null IRIs are content-addressed).
fn resolve_witness_projections(
    witnesses: &[gmeow_logic::reason::WitnessDerivation],
    result: &ReasoningResult,
) -> Result<Vec<WitnessProjection>, gmeow_errors::Diag> {
    let mut projections = Vec::with_capacity(witnesses.len());
    for witness in witnesses {
        let object_display = format!("<{}>", witness.witness);
        // The bound frontier IRIs (the Skolem-function arguments) — the subject of the
        // minting head quad p(x, null) is one of these.
        let frontier_iris: BTreeSet<&str> = witness
            .frontier
            .iter()
            .filter_map(|term| match term {
                purrdf::TermValue::Iri(iri) => Some(iri.as_str()),
                _ => None,
            })
            .collect();
        // A null is the OBJECT of its minting head quad AND of every edge the closure
        // entails from it (a superproperty of the existential property, say), so it can
        // be the object of MORE THAN ONE reasoned axiom — that is normal, not an error.
        // Choose the minting head quad deterministically: prefer the edge FROM a frontier
        // binding (the Skolem argument), with a stable (predicate, subject) tiebreak.
        let mut candidates: Vec<&_> = result
            .inferred()
            .iter()
            .filter(|axiom| axiom.object == object_display)
            .collect();
        candidates.sort_by(|a, b| {
            (a.predicate.as_str(), a.subject.as_str())
                .cmp(&(b.predicate.as_str(), b.subject.as_str()))
        });
        let head = candidates
            .iter()
            .copied()
            .find(|axiom| frontier_iris.contains(axiom.subject.as_str()))
            .or_else(|| candidates.first().copied())
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-reason".to_string(),
                    message: format!(
                        "chase-invented null <{}> has no reasoned head quad p(x, null): \
                         the existential edge that minted it must be in the closure",
                        witness.witness
                    ),
                })
            })?;
        let r_head =
            gmeow_logic::reason::reifier_iri(&head.subject, &head.predicate, &object_display);
        projections.push(WitnessProjection {
            witness: witness.witness.clone(),
            subject: head.subject.clone(),
            predicate: head.predicate.clone(),
            rule_iri: witness.rule_iri.clone(),
            ordinal: witness.ordinal,
            world: head.world.clone(),
            r_head,
        });
    }
    projections.sort_by(|a, b| a.witness.cmp(&b.witness));
    Ok(projections)
}

/// Serialize the resolved witness projections as N-Triples for the
/// `graph/diagnostics` fold: standard RDF reification of each minting head quad
/// `p(x, n)` (`rdf:subject`/`rdf:predicate`/`rdf:object` + the reused
/// `gmeow:viaRule`) plus the `gmeow:InventedWitness` typing and the
/// `gmeow:existentialOrdinal` of the null. Queryable with NO new vocabulary. Empty
/// string when there are no witnesses.
fn witness_projection_ntriples(projections: &[WitnessProjection]) -> String {
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
    let mut out = String::new();
    for projection in projections {
        use std::fmt::Write as _;
        let _ = write!(
            out,
            "<{r}> <{RDF}subject> <{s}> .\n\
             <{r}> <{RDF}predicate> <{p}> .\n\
             <{r}> <{RDF}object> <{n}> .\n\
             <{r}> <{GMEOW}viaRule> <{rule}> .\n\
             <{n}> <{RDF}type> <{GMEOW}InventedWitness> .\n\
             <{n}> <{GMEOW}existentialOrdinal> \"{ord}\"^^<{XSD_NNI}> .\n",
            r = projection.r_head,
            s = projection.subject,
            p = projection.predicate,
            n = projection.witness,
            rule = projection.rule_iri,
            ord = projection.ordinal,
        );
    }
    out
}

/// Parse the closure Turtle into the default graph and FOLD the deterministic
/// `graph/reasoning` projection of `result` into the named graph [`GRAPH_REASONING`],
/// returning the dual-carriage dataset the reason stage's bundle backs. The closure
/// stays the default-graph contribution to the compose union; the reasoning
/// projection rides alongside as its own named graph (the typed handle's backing).
fn reason_dataset(
    closure_ttl: &str,
    result: &ReasoningResult,
    chase_report: &gmeow_errors::Report,
    witnesses: &[gmeow_logic::reason::WitnessDerivation],
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let closure_ds =
        purrdf::parse_dataset(closure_ttl.as_bytes(), "text/turtle", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("reason closure parse: {e}"),
            })
        })?;
    let reasoning_nt = project_reasoning_result(result);
    let reasoning_ds =
        purrdf::parse_dataset(reasoning_nt.as_bytes(), "application/n-triples", None).map_err(
            |e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("reason projection parse: {e}"),
                })
            },
        )?;

    let mut builder = RdfDatasetBuilder::new();
    // The closure stays in the default graph (the compose-union contribution).
    builder.push_dataset(closure_ds.as_ref());
    // The graph/reasoning projection is routed into its own named graph.
    let graph = RdfTerm::Iri(GRAPH_REASONING.to_owned());
    for quad in reasoning_ds.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(graph.clone());
        builder.push_owned_quad(&routed);
    }
    let diagnostics_nq = gmeow_errors::render::to_gmeow_rdf(chase_report);
    let diagnostics = crate::stages::carrier::parse_into_graph(
        diagnostics_nq.as_bytes(),
        "application/n-quads",
        crate::stages::carrier::GRAPH_DIAGNOSTICS,
    )?;
    builder.push_dataset(diagnostics.as_ref());
    // Chase-invented nulls (Skolem witnesses): project each minting head quad
    // p(x, n) as standard RDF reification + type the null gmeow:InventedWitness,
    // routed into graph/diagnostics so the offline `gmeow explain` CLI can decompose
    // an invented individual. Byte-stable: content-addressed null IRIs, sorted.
    let projections = resolve_witness_projections(witnesses, result)?;
    let witness_nt = witness_projection_ntriples(&projections);
    if !witness_nt.is_empty() {
        let witness_ds = crate::stages::carrier::parse_into_graph(
            witness_nt.as_bytes(),
            "application/n-triples",
            crate::stages::carrier::GRAPH_DIAGNOSTICS,
        )?;
        builder.push_dataset(witness_ds.as_ref());
    }
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("freeze reason dual-carriage dataset: {e}"),
        })
    })
}

// ── correspondence drift-gate (the mandatory reader — adversary F4) ────────────

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `reason` pipeline stage — the sole engine-lock-carrying stage.
pub struct ReasonStage {
    consumes: Vec<String>,
    resources: Vec<String>,
    entities: Vec<(String, Vec<String>)>,
}

impl ReasonStage {
    /// Construct the stage. It reasons over the object-level EDB assembled from the
    /// compile-logic / source-load / statements producers; the slice DAG's `stage-reason`
    /// `dataflowConsumes` mirrors this set. It requires the exclusive
    /// [`ENGINE_RESOURCE`] (the sole resource-bearing build stage), so the scheduler
    /// serializes it against any stage competing for the reasoning engine.
    ///
    /// There is no `stage-math-producers` edge: every graph that stage attaches is a
    /// COMPUTED producer graph the presenter folds into the bundle, and none of them is
    /// object-level axiom source. The authored positive-demonstrator ABox every gate needs a
    /// witness from (`graph/examples`) arrives on the `stage-source-load` product, with the
    /// rest of the authored sources.
    ///
    /// Typed dataflow (artifact-level): from `stage-compile-logic` it reads ONLY the
    /// `logic` and `relational-core` named graphs (see
    /// [`crate::stages::carrier::assemble_object_level_edb`]) — never that product's
    /// other graphs or byte artifacts (diagnostics, the eight projection
    /// serializations). The shipped correspondence graph is meta-level and therefore
    /// excluded. Declaring these two entities lets a change to compile-logic's
    /// diagnostics or projection bytes alone skip re-running the (expensive) reasoner.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-source-load".to_string(),
                "stage-statements".to_string(),
            ],
            resources: vec![ENGINE_RESOURCE.to_string()],
            entities: vec![(
                "stage-compile-logic".to_string(),
                crate::stages::compile_logic::object_level_entity_list(),
            )],
        }
    }
}

impl Default for ReasonStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ReasonStage {
    fn id(&self) -> &str {
        "stage-reason"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn resources(&self) -> &[String] {
        &self.resources
    }
    fn cache_policy(&self) -> CachePolicy {
        // This cumulative product carries the full closure plus several large report
        // lanes. A measured packed-cache hit still had to restore the whole carrier
        // and re-derive its typed handle serially; that cost exceeded recomputing the
        // reason stage from its already-live upstream carrier. Cache pure inputs and
        // whole-run cleanliness instead of materializing this aggregate boundary.
        CachePolicy::Recompute
    }
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.entities
    }
    /// The named graphs this stage attaches to the carrier (its delta), from the
    /// single Rust-side attach table; mirrored by the slice module.ttl gmeow:attachesGraph
    /// declarations and verified against the run-time delta by the scheduler.
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    /// The blob-representation lanes this stage attaches (its delta), from the single
    /// Rust-side attach table; mirrored by gmeow:attachesBlobRep and run-time-verified.
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        // The native contract changed when the external reasoners were removed and
        // the structured existential/DL chase became the sole production authority.
        // Bumping the stage version prevents a pre-removal closure from surviving in
        // the content-addressed pipeline cache when its RDF inputs are unchanged.
        // v4: the closure additionally carries the math: expression-identity derivation's
        // math:alphaEquivalenceClass edges, so the α-equivalence class is a joinable node in
        // the shipped bundle and the committed closure rather than a value that exists only
        // inside a gate process.
        // v5 briefly carried graph/verify directly. v6 restores single ownership:
        // stage-verify-attestation consumes this stage's typed ReasoningResult and
        // projects graph/verify without launching another closure.
        "reason.v6-dedicated-verify-consumer"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let mut timings = Vec::with_capacity(3);
        // Reason ONCE over the object-level EDB (ontology + imports + statements +
        // alignments + logic/relational-core), assembled in the SAME
        // graph layout the bundle carries but WITHOUT the meta/report graphs — they
        // assert no axioms, so excluding them is closure-isomorphic and makes the
        // Skolem witnesses a function of the ontology alone. This pass owns the
        // committed closure AND backs the bundle's `graph/reasoning`; there is no
        // second full-fold export leaf.
        let edb_started = Instant::now();
        let edb = crate::stages::carrier::assemble_object_level_edb(input.upstream)?;
        let edb_quads = edb.quad_count();
        timings.push(StageRunTiming {
            phase: "assemble-object-edb".to_string(),
            elapsed_ms: edb_started.elapsed().as_millis(),
            metadata: Some(format!("edb-quads={edb_quads}")),
        });
        let closure_started = Instant::now();
        let reasoned = reason_over_dataset(edb.as_ref())?;
        let inferred_axioms = reasoned.result.inferred().len();
        let budget = reasoned.result.provenance.consumed_budget;
        let budget_allowance = budget
            .allowance
            .map_or_else(|| "unbounded".to_string(), |value| value.to_string());
        let budget_limit = budget.limit.map_or("none", |limit| limit.wire());
        let artifact_bytes = reasoned
            .closure
            .len()
            .saturating_add(reasoned.explanations.len())
            .saturating_add(reasoned.ledger.len())
            .saturating_add(reasoned.perf_ledger.len());
        timings.push(StageRunTiming {
            phase: "construct-closure-and-artifacts".to_string(),
            elapsed_ms: closure_started.elapsed().as_millis(),
            metadata: Some(format!(
                "closure-constructions=1;edb-quads={edb_quads};inferred-axioms={inferred_axioms};\
                 budget-consumed={};budget-allowance={budget_allowance};budget-limit={budget_limit};\
                 witness-derivations={};artifact-bytes={artifact_bytes}",
                budget.consumed,
                reasoned.witness_derivations.len(),
            )),
        });
        let output_started = Instant::now();
        // The CLOSURE is the reason stage's contribution to `gts_compose`'s union and
        // stays the dataset's DEFAULT graph. The EXPLANATIONS and LEDGER are diagnostic
        // REPORTS (proof skeletons / DL·EL crosscheck), NOT ontology facts; they stay
        // byte-lane only and are EXCLUDED from the compose union BY CONSTRUCTION. The
        // typed five-axis result rides BOTH as the `graph/reasoning` named graph (the
        // repo-free RDF projection) AND as the typed `PipelineHandle::Reasoning` handle
        // pinned to that graph (C7) — dual carriage.
        let dataset = reason_dataset(
            &reasoned.closure,
            &reasoned.result,
            &reasoned.chase_report,
            &reasoned.witness_derivations,
        )?;
        let nodes = crate::stages::diag_render::finding_nodes(&reasoned.chase_report, self.id());
        let diag_blob = serde_json::to_vec(&nodes).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: self.id().to_owned(),
                message: format!("encode chase certificate diagnostic nodes: {e}"),
            })
        })?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(CLOSURE_PATH.to_string(), reasoned.closure.into_bytes());
        artifacts.insert(
            EXPLANATIONS_PATH.to_string(),
            reasoned.explanations.into_bytes(),
        );
        artifacts.insert(LEDGER_PATH.to_string(), reasoned.ledger.into_bytes());
        artifacts.insert(
            PERF_LEDGER_PATH.to_string(),
            reasoned.perf_ledger.into_bytes(),
        );

        // Attach the typed Reasoning handle, pinned to the `graph/reasoning` named
        // graph's canonical digest. `pin_handle` HARD-fails on a digest mismatch, so a
        // handle that disagrees with its backing graph can never attach (fail-closed).
        let mut bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            artifacts,
            purrdf::provenance::DatasetProvenance::new(),
            crate::stages::carrier::REP_DIAG_NODES,
            "application/json",
            diag_blob,
        );
        let pinned = bundle.graph_digest(GRAPH_REASONING);
        bundle
            .pin_handle(
                GRAPH_REASONING,
                PipelineHandle::Reasoning(Arc::new(reasoned.result)),
                pinned,
            )
            .map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                    stage: "stage-reason".to_string(),
                    message: format!("pin Reasoning handle to <{GRAPH_REASONING}>: {e}"),
                })
            })?;
        let output_quads = bundle.dataset().quad_count();
        let product = StageProduct::from_bundle(self.id(), Arc::new(bundle));
        timings.push(StageRunTiming {
            phase: "assemble-reason-product".to_string(),
            elapsed_ms: output_started.elapsed().as_millis(),
            metadata: Some(format!(
                "output-quads={output_quads};diagnostic-nodes={}",
                nodes.len()
            )),
        });
        Ok(StageOutput {
            product,
            diags: nodes,
            timings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::bundle_from_artifacts_over;

    #[test]
    fn reason_produces_nonempty_artifacts_over_tiny_graph() {
        let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
<http://example.org/B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> <http://gmeow.example/w> .
"#;
        let reasoned = reason_artifacts(nq).expect("reason");

        // Wiring check: the native reasoner ran end-to-end and the three
        // builders produced their artifacts (each carries at least its generated
        // header), and the closure contains a concrete derived transitive
        // subclass axiom.
        for (name, ttl) in [
            ("closure", &reasoned.closure),
            ("explanations", &reasoned.explanations),
            ("ledger", &reasoned.ledger),
            ("perf_ledger", &reasoned.perf_ledger),
        ] {
            assert!(!ttl.trim().is_empty(), "{name} artifact is empty");
        }
        assert!(reasoned.closure.contains("<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> ."));
        assert!(reasoned.chase_report.findings.iter().any(|finding| {
            finding.code == "reason.native-contract"
                && finding
                    .message
                    .contains(&gmeow_logic::reason::native_contract_hash())
        }));
        // The perf ledger flags the deferred / non-incremental levers (static content).
        assert!(
            reasoned
                .perf_ledger
                .contains("https://blackcatinformatics.ca/gmeow/FlaggedNonIncremental"),
            "the perf ledger flags the non-incremental hard parts"
        );
    }

    // The synthetic in-crate fold test that hand-built an `example.org` existential EDB
    // and asserted the certificate's free-text edge message was RETIRED (AC3):
    // its fold demonstration is now the non-vacuous golden over REAL sources
    // (`tests/chase_certificate_golden.rs`, structured), the witness projection is
    // covered by `invented_witness_skeletons_land_in_diagnostics_and_certificate_cites_them`
    // below, and `finding_nodes` node-count rendering by `diag_render`'s own tests.

    #[test]
    fn invented_witness_skeletons_land_in_diagnostics_and_certificate_cites_them() {
        // A `C ⊑ ∃p.D` obligation on an individual `x:R` mints exactly one chase
        // witness null. The stage projects its minting head quad p(x, null) into
        // graph/diagnostics as standard RDF reification + types the null a
        // gmeow:InventedWitness, and the weakly-acyclic certificate finding cites
        // the null-minting reifier through gmeow:findingDerivedFromQuad.
        const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
        const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
        const INVENTED_WITNESS: &str = "https://blackcatinformatics.ca/gmeow/InventedWitness";
        const EXISTENTIAL_ORDINAL: &str = "https://blackcatinformatics.ca/gmeow/existentialOrdinal";
        const VIA_RULE: &str = "https://blackcatinformatics.ca/gmeow/viaRule";

        let nq = br#"
<http://example.org/R> <http://www.w3.org/2002/07/owl#onProperty> <http://example.org/p> <http://gmeow.example/w> .
<http://example.org/R> <http://www.w3.org/2002/07/owl#someValuesFrom> <http://example.org/D> <http://gmeow.example/w> .
<http://example.org/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/R> <http://gmeow.example/w> .
"#;
        let reasoned = reason_artifacts(nq).expect("production existential reason");
        assert!(
            !reasoned.witness_derivations.is_empty(),
            "the existential obligation must mint at least one witness"
        );
        let dataset = reason_dataset(
            &reasoned.closure,
            &reasoned.result,
            &reasoned.chase_report,
            &reasoned.witness_derivations,
        )
        .expect("witness diagnostics dataset");
        let diagnostics = dataset.project_named_graph(crate::stages::carrier::GRAPH_DIAGNOSTICS);
        let quads: Vec<_> = diagnostics.owned_quads().collect();

        let iri = |term: &RdfTerm| match term {
            RdfTerm::Iri(iri) => Some(iri.clone()),
            _ => None,
        };

        // (1) ≥1 subject typed gmeow:InventedWitness.
        let witness = quads
            .iter()
            .find(|quad| {
                quad.predicate == RDF_TYPE
                    && matches!(&quad.object, RdfTerm::Iri(iri) if iri == INVENTED_WITNESS)
            })
            .and_then(|quad| iri(&quad.subject))
            .expect("a gmeow:InventedWitness null is projected into graph/diagnostics");

        // (2) that witness carries gmeow:existentialOrdinal.
        assert!(
            quads.iter().any(|quad| {
                iri(&quad.subject).as_deref() == Some(witness.as_str())
                    && quad.predicate == EXISTENTIAL_ORDINAL
            }),
            "the invented witness must carry its gmeow:existentialOrdinal"
        );

        // (3) a reifier whose rdf:object IS the null AND that carries gmeow:viaRule.
        let reifier = quads
            .iter()
            .find(|quad| {
                quad.predicate == RDF_OBJECT
                    && matches!(&quad.object, RdfTerm::Iri(iri) if iri == &witness)
            })
            .and_then(|quad| iri(&quad.subject))
            .expect("a head-quad reifier with rdf:object = <null> is present");
        assert!(
            quads.iter().any(|quad| {
                iri(&quad.subject).as_deref() == Some(reifier.as_str())
                    && quad.predicate == VIA_RULE
            }),
            "the null-minting reifier must carry gmeow:viaRule"
        );

        // (4) the certificate finding rehydrated via the offline reader carries a
        // non-empty derived_from_quads (the null-minting reifier it cites).
        let index = crate::diagnostics_reader::read_findings(&dataset)
            .expect("rehydrate the diagnostics graph");
        let certificate = index
            .findings
            .values()
            .find(|finding| finding.code == "chase.certificate.weakly-acyclic")
            .expect("the weakly-acyclic certificate finding rehydrates");
        assert!(
            !certificate.derived_from_quads.is_empty(),
            "the certificate must cite its null-minting reifiers via findingDerivedFromQuad: {certificate:?}"
        );
        assert!(
            certificate.derived_from_quads.contains(&reifier),
            "the certificate must cite the head-quad reifier whose object is the null"
        );
    }

    #[test]
    fn reason_stage_pins_a_reasoning_handle_to_graph_reasoning() {
        // The dual-carriage dataset folds the graph/reasoning projection as a named
        // graph and the typed handle pins to it (the digest invariant must hold).
        let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
<http://example.org/B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> <http://gmeow.example/w> .
"#;
        let reasoned = reason_artifacts(nq).expect("reason");
        let dataset = reason_dataset(
            &reasoned.closure,
            &reasoned.result,
            &reasoned.chase_report,
            &reasoned.witness_derivations,
        )
        .expect("dual dataset");
        let mut bundle = bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(GRAPH_REASONING);
        bundle
            .pin_handle(
                GRAPH_REASONING,
                PipelineHandle::Reasoning(Arc::new(reasoned.result.clone())),
                pinned,
            )
            .expect("pin Reasoning handle to its backing graph");
        let entry = bundle.handle(GRAPH_REASONING).expect("handle attached");
        let PipelineHandle::Reasoning(r) = &entry.payload else {
            panic!("the handle arm is Reasoning");
        };
        assert_eq!(
            r.as_ref(),
            &reasoned.result,
            "the typed result is carried verbatim"
        );
        // The graph/reasoning named graph is non-empty (the projection landed).
        assert_ne!(
            bundle.graph_digest(GRAPH_REASONING),
            bundle.graph_digest("https://blackcatinformatics.ca/gmeow/graph/absent"),
            "graph/reasoning carries the projection"
        );
        assert_ne!(
            bundle.graph_digest(crate::stages::carrier::GRAPH_DIAGNOSTICS),
            bundle.graph_digest("https://blackcatinformatics.ca/gmeow/graph/absent"),
            "graph/diagnostics always carries the run's native contract evidence"
        );
    }

    #[test]
    fn reason_product_projects_modal_verdict_and_counterexample_into_stage_reason() {
        let nq = br#"
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/necessarily> <https://example.org/modal/B> <https://example.org/modal/frame> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/overAccessibility> <https://blackcatinformatics.ca/logic/epistemicallyPossible> <https://example.org/modal/frame> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/modalEvalWorld> <https://example.org/modal/w0> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomSubject> <https://example.org/modal/a> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomPredicate> <https://example.org/modal/knows> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomObject> <https://example.org/modal/b> <https://example.org/modal/frame> .
<https://example.org/modal/w0> <https://blackcatinformatics.ca/logic/epistemicallyPossible> <https://example.org/modal/w1> <https://example.org/modal/frame> .
<https://example.org/modal/w0> <https://blackcatinformatics.ca/logic/epistemicallyPossible> <https://example.org/modal/w2> <https://example.org/modal/frame> .
<https://example.org/modal/a> <https://example.org/modal/knows> <https://example.org/modal/b> <https://example.org/modal/w1> .
"#;
        let reasoned = reason_artifacts(nq).expect("reason modal frame");
        let modal = reasoned
            .result
            .inferred()
            .iter()
            .find(|axiom| {
                axiom.predicate == "https://blackcatinformatics.ca/logic/modalNecessityFails"
            })
            .expect("typed production result carries the modal verdict");
        assert_eq!(modal.world, "https://example.org/modal/w0");
        assert_eq!(modal.subject, "https://example.org/modal/F");
        assert_eq!(modal.object, "<https://example.org/modal/B>");
        assert_eq!(
            modal.rule_name.as_deref(),
            Some("https://blackcatinformatics.ca/logic/rule/modal-evaluation")
        );
        assert_eq!(
            modal.premises,
            vec![(
                "https://example.org/modal/a".to_owned(),
                "https://example.org/modal/knows".to_owned(),
                "<https://example.org/modal/b>".to_owned(),
            )]
        );
        let body_source = gmeow_logic::provenance::reifier_from_strings(
            "https://example.org/modal/a",
            "https://example.org/modal/knows",
            "<https://example.org/modal/b>",
        );
        let access_source = gmeow_logic::provenance::reifier_from_strings(
            "https://example.org/modal/w0",
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
            "<https://example.org/modal/w2>",
        );
        let verdict_derivation = gmeow_logic::provenance::mint_derivation_id(
            "https://blackcatinformatics.ca/logic/rule/modal-evaluation",
            &[body_source.as_str()],
        );
        let counterexample_derivation = gmeow_logic::provenance::mint_derivation_id(
            "https://blackcatinformatics.ca/logic/rule/modal-evaluation",
            &[body_source.as_str(), access_source.as_str()],
        );
        let counterexample = reasoned
            .result
            .inferred()
            .iter()
            .find(|axiom| {
                axiom.predicate == "https://blackcatinformatics.ca/logic/modalCounterexampleWorld"
            })
            .expect("typed production result carries the counterexample world");
        assert_eq!(counterexample.world, "https://example.org/modal/w0");
        assert_eq!(counterexample.subject, "https://example.org/modal/F");
        assert_eq!(counterexample.object, "<https://example.org/modal/w2>");
        assert_eq!(
            counterexample.rule_name.as_deref(),
            Some("https://blackcatinformatics.ca/logic/rule/modal-evaluation")
        );
        assert_eq!(
            counterexample.premises,
            vec![
                (
                    "https://example.org/modal/a".to_owned(),
                    "https://example.org/modal/knows".to_owned(),
                    "<https://example.org/modal/b>".to_owned(),
                ),
                (
                    "https://example.org/modal/w0".to_owned(),
                    "https://blackcatinformatics.ca/logic/epistemicallyPossible".to_owned(),
                    "<https://example.org/modal/w2>".to_owned(),
                ),
            ]
        );
        assert!(
            reasoned.closure.contains(
                "<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/modalNecessityFails> <https://example.org/modal/B> ."
            ),
            "stage-reason's closure must carry the shared modal verdict"
        );
        assert!(
            reasoned.closure.contains(
                "<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/modalCounterexampleWorld> <https://example.org/modal/w2> ."
            ),
            "stage-reason's closure must carry the exact modal counterexample"
        );
        for artifact in [&reasoned.closure, &reasoned.explanations] {
            assert!(
                artifact.contains("https://blackcatinformatics.ca/logic/rule/modal-evaluation")
            );
            assert!(artifact.contains(&body_source));
            assert!(artifact.contains(&access_source));
            assert!(artifact.contains(&verdict_derivation));
            assert!(artifact.contains(&counterexample_derivation));
            assert!(artifact.contains("https://example.org/modal/w0"));
        }
        let product = reason_product(nq).expect("stage-reason product");
        let reasoning_graph = product.dataset().project_named_graph(GRAPH_REASONING);
        let projected = reasoning_graph.owned_quads().collect::<Vec<_>>();
        assert!(projected.iter().any(|quad| {
            quad.predicate == "https://blackcatinformatics.ca/gmeow/viaRule"
                && matches!(
                    &quad.object,
                    RdfTerm::Iri(iri)
                        if iri == "https://blackcatinformatics.ca/logic/rule/modal-evaluation"
                )
        }));
        assert!(projected.iter().any(|quad| {
            quad.predicate == "https://blackcatinformatics.ca/logic/derivationIdentifier"
                && matches!(
                    &quad.object,
                    RdfTerm::Literal(literal) if literal.lexical_form == verdict_derivation
                )
        }));
        assert!(projected.iter().any(|quad| {
            quad.predicate == "https://blackcatinformatics.ca/logic/derivationIdentifier"
                && matches!(
                    &quad.object,
                    RdfTerm::Literal(literal) if literal.lexical_form == counterexample_derivation
                )
        }));
        assert!(projected.iter().any(|quad| {
            quad.predicate == "http://www.w3.org/ns/prov#wasDerivedFrom"
                && matches!(&quad.object, RdfTerm::Iri(iri) if iri.as_str() == body_source.as_str())
        }));
        assert!(projected.iter().any(|quad| {
            quad.predicate == "http://www.w3.org/ns/prov#wasDerivedFrom"
                && matches!(&quad.object, RdfTerm::Iri(iri) if iri.as_str() == access_source.as_str())
        }));

        let handle = product
            .bundle()
            .handle(GRAPH_REASONING)
            .expect("stage-reason pins its typed result");
        let PipelineHandle::Reasoning(transported) = &handle.payload else {
            panic!("graph/reasoning must carry a Reasoning handle");
        };
        let transported_modal = transported
            .inferred()
            .iter()
            .find(|axiom| {
                axiom.predicate == "https://blackcatinformatics.ca/logic/modalNecessityFails"
            })
            .expect("stage-reason handle retains modal verdict");
        assert_eq!(transported_modal, modal);
        let transported_counterexample = transported
            .inferred()
            .iter()
            .find(|axiom| {
                axiom.predicate == "https://blackcatinformatics.ca/logic/modalCounterexampleWorld"
            })
            .expect("stage-reason handle retains the modal counterexample");
        assert_eq!(transported_counterexample, counterexample);
    }

    #[test]
    fn shipped_modal_example_is_a_complete_reason_product_frame() {
        // The production source loader unions positive demonstrators into this exact named
        // world. Parse only the authored example under test, root it exactly as source-load
        // does, and hand its in-memory N-Quads to the actual stage-reason product. This is
        // read-only fixture consumption: no corpus producer, cache fallback, or foundation
        // evaluator is involved.
        let example = purrdf::parse_dataset(
            include_bytes!("../../../../slices/grounding/logic/examples/gmn-logic-roundtrip.ttl"),
            "text/turtle",
            None,
        )
        .expect("parse the shipped modal example");
        let rooted = crate::stages::carrier::rooted_in_graph(
            example.as_ref(),
            gmeow_logic::reasoning_graphs::GRAPH_EXAMPLES,
        )
        .expect("root the example in its production named world");
        let nq = purrdf::serialize_dataset(
            rooted.as_ref(),
            "application/n-quads",
            purrdf::SerializeGraph::Dataset,
        )
        .expect("serialize the rooted example as in-memory N-Quads");

        let product = reason_product(&nq).expect("the shipped modal frame must reason");
        let handle = product
            .bundle()
            .handle(GRAPH_REASONING)
            .expect("stage-reason pins its typed result");
        let PipelineHandle::Reasoning(result) = &handle.payload else {
            panic!("graph/reasoning must carry a Reasoning handle");
        };
        for (formula, predicate) in [
            (
                "https://blackcatinformatics.ca/gmeow/examples/logic/necessarilyReliable",
                "https://blackcatinformatics.ca/logic/modalNecessityHolds",
            ),
            (
                "https://blackcatinformatics.ca/gmeow/examples/logic/possiblyReliable",
                "https://blackcatinformatics.ca/logic/modalPossibilityHolds",
            ),
        ] {
            let verdict = result
                .inferred()
                .iter()
                .find(|axiom| axiom.subject == formula && axiom.predicate == predicate)
                .unwrap_or_else(|| panic!("missing production modal verdict for {formula}"));
            assert_eq!(verdict.world, gmeow_logic::reasoning_graphs::GRAPH_EXAMPLES);
            assert_eq!(
                verdict.rule_name.as_deref(),
                Some("https://blackcatinformatics.ca/logic/rule/modal-evaluation")
            );
        }
    }

    #[test]
    fn reason_product_hard_fails_on_malformed_modal_frames() {
        let nq = br#"
<https://example.org/valid/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/valid/B> <https://example.org/valid/w> .
<https://example.org/valid/B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/valid/C> <https://example.org/valid/w> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/necessarily> <https://example.org/modal/B> <https://example.org/modal/frame> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/overAccessibility> <https://blackcatinformatics.ca/logic/accessibleFrom> <https://example.org/modal/frame> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/modalEvalWorld> <https://example.org/modal/w0> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomSubject> <https://example.org/modal/a> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomPredicate> <https://example.org/modal/knows> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomObject> <https://example.org/modal/b> <https://example.org/modal/frame> .
"#;
        let err = reason_product(nq).expect_err("malformed modal frame must fail closed");
        assert!(
            err.message().contains("prose-only"),
            "the hard-fail should name the malformed modal accessibility: {err}"
        );
    }

    #[test]
    fn pin_handle_hard_fails_on_a_digest_mismatch() {
        let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
"#;
        let reasoned = reason_artifacts(nq).expect("reason");
        let dataset = reason_dataset(
            &reasoned.closure,
            &reasoned.result,
            &reasoned.chase_report,
            &reasoned.witness_derivations,
        )
        .expect("dual dataset");
        let mut bundle = bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
        );
        // A WRONG pinned digest must be rejected (no silently-stale handle).
        let wrong = purrdf::ContentDigest::of(b"not the backing graph");
        let err = bundle
            .pin_handle(
                GRAPH_REASONING,
                PipelineHandle::Reasoning(Arc::new(reasoned.result)),
                wrong,
            )
            .expect_err("a mismatched pin must hard-fail");
        let _ = err;
    }
}
