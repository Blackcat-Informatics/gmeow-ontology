// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `reason` stage: native EL/DL closure, typed-modal evaluation, and artifacts — the SOLE
//! reasoning pass.
//!
//! It reasons ONCE over the object-level EDB
//! ([`crate::stages::carrier::assemble_object_level_edb`]: ontology + imports +
//! statements + logic/relational-core, WITHOUT correspondence or the
//! meta/report graphs), canonicalizes it (RDFC-1.0) for transport-independent Skolem
//! witnesses, runs `gmeow_logic::reason::reason_all`, and serializes the
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
    build_dl_el_ledger_ttl, build_explanations_ttl, build_inferred_closure_into,
};
use gmeow_logic::reason::perf_ledger::perf_ledger;
#[cfg(test)]
use gmeow_logic::reason::{DomainProfile, SelectedLogicalWorld};
use gmeow_logic::reason::{
    LogicalGraph, PreparedReasoningInput, SelectedDomains, prepare_reasoning_input, reason_all,
};
use gmeow_logic::result::ReasoningResult;
use gmeow_logic::result_rdf::{GRAPH_REASONING, project_reasoning_dataset};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfTerm};

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
    /// Native dual-carriage product projected alongside the closure artifact,
    /// including its proof reifiers, reasoning graph and diagnostics.
    pub dataset: Arc<RdfDataset>,
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
}

/// Produce the pinned reasoning product from an admitted dataset and mandatory
/// caller-owned domain selection, without a serialized stage input.
///
/// # Errors
/// Propagates native reasoning, projection and typed-handle admission failures.
pub fn reason_product_over_dataset(
    edb: &RdfDataset,
    domains: &SelectedDomains,
) -> Result<StageProduct, gmeow_errors::Diag> {
    reason_product_from_artifacts(reason_over_dataset(edb, domains)?)
}

fn reason_product_from_artifacts(
    reasoned: ReasonArtifacts,
) -> Result<StageProduct, gmeow_errors::Diag> {
    let mut bundle = crate::bundle::bundle_from_artifacts_over(
        reasoned.dataset,
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
/// content-addressed Skolem witnesses are transport-independent for the same
/// retained domain selection. Pipeline theory roles are Default or named IRIs;
/// selected blank graph identities cannot cross relabelling without its issuer map.
/// The caller supplies the exact theory-role domain authority; merely occupying
/// a support or diagnostic graph does not select a domain law.
///
/// # Errors
/// Rejects native input/domain admission, execution, evidence or projection failures.
pub fn reason_over_dataset(
    edb: &RdfDataset,
    domains: &SelectedDomains,
) -> Result<ReasonArtifacts, gmeow_errors::Diag> {
    if domains.worlds().iter().any(
        |domain| matches!(domain.graph(), LogicalGraph::Named(graph) if graph.as_iri().is_none()),
    ) {
        return Err(stage_failure(
            "pipeline theory roles require Default or named IRIs; a selected blank graph cannot cross canonical relabelling without its issuer mapping",
        ));
    }
    // Issue canonical labels directly over the native RDF 1.2 carrier. The
    // shared issuer determines Skolem identity without a text document and a
    // second parse/freeze; every statement-layer row remains in its own table.
    let canon = canonicalize_edb(edb, "stage-reason")?;
    let input = prepare_reasoning_input(canon.as_ref())?;
    reason_prepared_input(edb, input, domains)
}

fn stage_failure(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-reason".to_owned(),
        message: message.into(),
    })
}

/// Execute the consuming input once and retain its native proof owner in the result.
fn reason_prepared_input(
    edb: &RdfDataset,
    input: PreparedReasoningInput,
    domains: &SelectedDomains,
) -> Result<ReasonArtifacts, gmeow_errors::Diag> {
    let result = reason_all(input, domains)
        .map_err(|error| stage_failure(format!("native reasoning failed: {error}")))?;
    let native = result.native_execution()?;
    // Admit the exact committed native minting heads and their world-local proof;
    // certificate findings cite those heads, including subject-only value nulls.
    let witness_projections = resolve_witness_projections(&native.witness_derivations, &result)?;
    let mut world_reifiers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for projection in &witness_projections {
        world_reifiers
            .entry(projection.world.clone())
            .or_default()
            .push(projection.r_head.clone());
    }
    let mut chase_report = gmeow_errors::Report::new("chase");
    // Record the run's content-addressed native contract even when this EDB
    // contains no existential obligations. This finding identifies the engine
    // behind the selected result; per-world chase certificates follow below.
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
    for certificate in &native.chase_certificates {
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
    let mut builder = RdfDatasetBuilder::new();
    let closure =
        build_inferred_closure_into(&result, &alpha_edges, &mut builder).map_err(|e| {
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
    let ledger = build_dl_el_ledger_ttl(&result)?;
    // The performance ledger is canonical static content (a property of the native
    // physical engine's lever staging, not of this run's data), so it is byte-stable
    // run to run regardless of the reasoned result.
    let perf = perf_ledger().to_turtle();
    let dataset = reason_dataset(builder, &result, &chase_report, &witness_projections)?;
    Ok(ReasonArtifacts {
        closure,
        dataset,
        explanations,
        ledger,
        perf_ledger: perf,
        result,
        chase_report,
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
    purrdf::canonical_relabel(edb).map(Arc::new).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: stage_id.to_string(),
            message: format!("RDFC-1.0 canonicalize EDB: {e}"),
        })
    })
}

/// One actual committed minting head, retaining native terms and exact source proof.
struct WitnessProjection {
    witness: String,
    subject: purrdf::TermValue,
    predicate: String,
    object: purrdf::TermValue,
    rule_iri: String,
    ordinal: usize,
    world: String,
    r_head: String,
    derivation_id: String,
    source_reifiers: Vec<String>,
    receipt: Arc<str>,
}

/// Admit retained minting heads and their actual world-local premises against this
/// result. No inference from later incident edges and no subject/object restriction
/// on where the invented value occurs. Only requested evidence is indexed.
fn resolve_witness_projections(
    witnesses: &[gmeow_logic::reason::WitnessDerivation],
    result: &ReasoningResult,
) -> Result<Vec<WitnessProjection>, gmeow_errors::Diag> {
    let fail = |message: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-reason".to_owned(),
            message,
        })
    };
    let mut required: BTreeMap<(&str, &str, &str), BTreeSet<&purrdf::TermValue>> = BTreeMap::new();
    let mut minted = BTreeMap::<(&str, &str, &str), BTreeSet<&purrdf::TermValue>>::new();
    let mut projections = Vec::new();
    for witness in witnesses {
        let receipt: Arc<str> = witness.to_wire()?.into();
        for head in &witness.heads {
            for statement in std::iter::once(&head.statement).chain(&head.premises) {
                let subject = statement.subject.as_iri().ok_or_else(|| {
                    fail(format!(
                        "witness <{}> requires a non-resource reasoning subject",
                        witness.witness
                    ))
                })?;
                required
                    .entry((&witness.scope.world, subject, &statement.predicate))
                    .or_default()
                    .insert(&statement.object);
            }
            let statement = &head.statement;
            let subject = statement
                .subject
                .as_iri()
                .expect("required head subject admitted");
            minted
                .entry((&witness.scope.world, subject, &statement.predicate))
                .or_default()
                .insert(&statement.object);
            let r_head = gmeow_logic::provenance::mint_reifier(
                &statement.subject,
                &statement.predicate,
                &statement.object,
            )?;
            let source_reifiers = head
                .premises
                .iter()
                .map(|premise| {
                    gmeow_logic::provenance::mint_reifier(
                        &premise.subject,
                        &premise.predicate,
                        &premise.object,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            projections.push(WitnessProjection {
                witness: witness.witness.clone(),
                subject: statement.subject.clone(),
                predicate: statement.predicate.clone(),
                object: statement.object.clone(),
                rule_iri: witness.rule_iri.clone(),
                ordinal: witness.ordinal,
                world: witness.scope.world.clone(),
                r_head,
                derivation_id: head.derivation_id.clone(),
                source_reifiers,
                receipt: Arc::clone(&receipt),
            });
        }
    }
    for axiom in result.inferred() {
        if !axiom.is_edb
            && let Some(objects) = minted.get_mut(&(
                axiom.world.as_str(),
                axiom.subject.as_str(),
                axiom.predicate.as_str(),
            ))
        {
            objects.remove(&axiom.object);
        }
        if let Some(objects) = required.get_mut(&(
            axiom.world.as_str(),
            axiom.subject.as_str(),
            axiom.predicate.as_str(),
        )) {
            objects.remove(&axiom.object);
        }
    }
    if let Some(((world, subject, predicate), objects)) = required
        .iter()
        .chain(&minted)
        .find(|(_, objects)| !objects.is_empty())
    {
        return Err(fail(format!(
            "retained witness head/premise is absent from committed closure in {world}: {subject} {predicate} {objects:?}"
        )));
    }
    projections.sort_by(|a, b| {
        (&a.witness, &a.world, &a.r_head, &a.derivation_id).cmp(&(
            &b.witness,
            &b.world,
            &b.r_head,
            &b.derivation_id,
        ))
    });
    Ok(projections)
}

/// Output-only native value projection; blank scopes and RDF 1.2 nested terms
/// remain exact. There is no disposable RDF document or parser boundary.
fn intern_witness_term(
    builder: &mut RdfDatasetBuilder,
    value: &purrdf::TermValue,
) -> purrdf::TermId {
    match value {
        purrdf::TermValue::Iri(iri) => builder.intern_iri(iri),
        purrdf::TermValue::Blank { label, scope } => builder.intern_blank(label, *scope),
        purrdf::TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        } => builder.intern_literal(RdfLiteral {
            lexical_form: lexical_form.clone(),
            datatype: Some(datatype.clone()),
            language: language.clone(),
            direction: *direction,
        }),
        purrdf::TermValue::Triple { s, p, o } => {
            let s = intern_witness_term(builder, s);
            let p = intern_witness_term(builder, p);
            let o = intern_witness_term(builder, o);
            builder.intern_triple(s, p, o)
        }
    }
}

/// Publish exact head skeletons and the versioned native witness receipt. The
/// receipt carries complete scope/frontier/head/position/premise identities;
/// RDF metadata retains the owning world and individual derivation evidence.
fn witness_projection_into(projections: &[WitnessProjection], builder: &mut RdfDatasetBuilder) {
    const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    const XSD_NNI: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";
    if projections.is_empty() {
        return;
    }
    let graph = builder.intern_iri(crate::stages::carrier::GRAPH_DIAGNOSTICS);
    let subject = builder.intern_iri(&format!("{RDF}subject"));
    let predicate = builder.intern_iri(&format!("{RDF}predicate"));
    let object = builder.intern_iri(&format!("{RDF}object"));
    let via_rule = builder.intern_iri(&format!("{GMEOW}viaRule"));
    let in_world = builder.intern_iri(&format!("{GMEOW}inWorld"));
    let rdf_type = builder.intern_iri(&format!("{RDF}type"));
    let invented = builder.intern_iri(&format!("{GMEOW}InventedWitness"));
    let ordinal = builder.intern_iri(&format!("{GMEOW}existentialOrdinal"));
    let value = builder.intern_iri("http://www.w3.org/ns/prov#value");
    let derived_from = builder.intern_iri("http://www.w3.org/ns/prov#wasDerivedFrom");
    let derivation =
        builder.intern_iri("https://blackcatinformatics.ca/logic/derivationIdentifier");
    let mut recorded = BTreeSet::new();
    for projection in projections {
        let r = builder.intern_iri(&projection.r_head);
        let s = intern_witness_term(builder, &projection.subject);
        let p = builder.intern_iri(&projection.predicate);
        let o = intern_witness_term(builder, &projection.object);
        let n = builder.intern_iri(&projection.witness);
        let rule = builder.intern_iri(&projection.rule_iri);
        let world = builder.intern_iri(&projection.world);
        let proof = builder.intern_literal(RdfLiteral::simple(&projection.derivation_id));
        let ord =
            builder.intern_literal(RdfLiteral::typed(projection.ordinal.to_string(), XSD_NNI));
        for (s, p, o) in [
            (r, subject, s),
            (r, predicate, p),
            (r, object, o),
            (r, via_rule, rule),
            (r, in_world, world),
            (r, derivation, proof),
            (n, rdf_type, invented),
            (n, ordinal, ord),
            (n, in_world, world),
        ] {
            builder.push_quad(s, p, o, Some(graph));
        }
        if recorded.insert(projection.witness.as_str()) {
            let receipt = builder.intern_literal(RdfLiteral::simple(projection.receipt.as_ref()));
            builder.push_quad(n, value, receipt, Some(graph));
        }
        for source in &projection.source_reifiers {
            let source = builder.intern_iri(source);
            builder.push_quad(r, derived_from, source, Some(graph));
        }
    }
}

/// Finish the native closure carrier with the deterministic
/// `graph/reasoning` projection of `result` into the named graph [`GRAPH_REASONING`],
/// returning the dual-carriage dataset the reason stage's bundle backs. The closure
/// stays the default-graph contribution to the compose union; the reasoning
/// projection rides alongside as its own named graph (the typed handle's backing).
fn reason_dataset(
    mut builder: RdfDatasetBuilder,
    result: &ReasoningResult,
    chase_report: &gmeow_errors::Report,
    projections: &[WitnessProjection],
) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let reasoning_ds = project_reasoning_dataset(result)?;
    // This is an independently minted summary graph. Standardize its blanks
    // apart from every already-projected source and proof identity.
    let scopes: BTreeSet<_> = builder.blank_identities().map(|(_, scope)| scope).collect();
    let mut scope = 1u32;
    for used in scopes {
        if used.0 < scope {
            continue;
        }
        if used.0 > scope {
            break;
        }
        scope = scope.checked_add(1).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-reason".to_owned(),
                message: "no blank scope remains for the reasoning summary".to_owned(),
            })
        })?;
    }
    // The graph/reasoning projection is routed into its own named graph.
    let graph = RdfTerm::Iri(GRAPH_REASONING.to_owned());
    for mut routed in reasoning_ds.owned_quads() {
        routed.graph_name = Some(graph.clone());
        builder.push_owned_quad_scoped(&routed, purrdf::BlankScope(scope));
    }
    for mut routed in reasoning_ds.owned_reifiers() {
        routed.graph = Some(graph.clone());
        builder.push_owned_reifier_scoped(&routed, purrdf::BlankScope(scope));
    }
    for mut routed in reasoning_ds.owned_annotations() {
        routed.graph = Some(graph.clone());
        builder.push_owned_annotation_scoped(&routed, purrdf::BlankScope(scope));
    }
    let graph_id = builder.intern_owned_term(&graph);
    builder.declare_named_graph(graph_id);
    gmeow_errors::render::append_gmeow_findings(
        chase_report,
        crate::stages::carrier::GRAPH_DIAGNOSTICS,
        &mut builder,
    );
    // Chase-invented nulls: project every retained committed minting head
    // and its scoped native receipt; type the value gmeow:InventedWitness,
    // routed into graph/diagnostics so the offline `gmeow explain` CLI can decompose
    // an invented individual. Byte-stable: content-addressed null IRIs, sorted.
    witness_projection_into(projections, &mut builder);
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("freeze reason dual-carriage dataset: {e}"),
        })
    })
}

// ── correspondence drift-gate (the mandatory reader — adversary F4) ────────────

/// Select the pipeline's declared object-level roles, never the union's graph census.
/// The semantic identity binds the selected domain profile and fixed role envelope.
/// Source bytes and complete producer/handle identities remain separately bound by
/// native input admission and scheduler receipts; data edits do not remint domains.
fn object_level_domains(
    stage: &ReasonStage,
    input: &StageInput<'_>,
) -> Result<SelectedDomains, gmeow_errors::Diag> {
    for producer in stage.consumes() {
        let product = input.upstream.get(producer).ok_or_else(|| {
            stage_failure(format!("domain selection requires producer {producer}"))
        })?;
        if product.stage_id != *producer {
            return Err(stage_failure(format!(
                "domain selection producer key {producer} carries {}",
                product.stage_id
            )));
        }
        if product.carrier_released {
            return Err(stage_failure(format!(
                "domain selection cannot use released producer {producer}"
            )));
        }
    }
    gmeow_logic::reasoning_graphs::object_level_domains()
}

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
        // Explicit domain selection and retained single-run native evidence are
        // part of both the result identity and every projected witness receipt.
        "reason.v13-selected-native-execution"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let mut timings = Vec::with_capacity(3);
        // Reason ONCE over the object-level EDB (ontology + imports + statements +
        // logic/relational-core), assembled in the SAME
        // graph layout the bundle carries but WITHOUT the meta/report graphs — they
        // assert no axioms, so excluding them is closure-isomorphic and makes the
        // Skolem witnesses a function of the ontology alone. This pass owns the
        // committed closure AND backs the bundle's `graph/reasoning`; there is no
        // second full-fold export leaf.
        let edb_started = Instant::now();
        // Admit the declared producers and select theory roles before
        // the physical union. Unselected report/support graphs cannot acquire an
        // intrinsic nonempty-domain law merely by occupying the carrier.
        let domains = object_level_domains(self, &input)?;
        let edb = crate::stages::carrier::assemble_object_level_edb(input.upstream)?;
        let edb_quads = edb.quad_count();
        timings.push(StageRunTiming {
            phase: "assemble-object-edb".to_string(),
            elapsed_ms: edb_started.elapsed().as_millis(),
            metadata: Some(format!("edb-quads={edb_quads}")),
        });
        let closure_started = Instant::now();
        let reasoned = reason_over_dataset(edb.as_ref(), &domains)?;
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
        let closure_reparse_bytes_removed = reasoned.closure.len();
        timings.push(StageRunTiming {
            phase: "construct-closure-and-artifacts".to_string(),
            elapsed_ms: closure_started.elapsed().as_millis(),
            metadata: Some(format!(
                "closure-constructions=1;edb-quads={edb_quads};inferred-axioms={inferred_axioms};\
                 budget-consumed={};budget-allowance={budget_allowance};budget-limit={budget_limit};\
                 witness-derivations={};artifact-bytes={artifact_bytes};\
                 closure-rdf-parses=0;closure-intermediate-freezes=0;\
                 closure-reparse-bytes-removed={closure_reparse_bytes_removed};\
                 witness-projection-passes=1;witness-wire-bytes=0",
                budget.consumed,
                reasoned.result.native_execution()?.witness_derivations.len(),
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
        let dataset = reasoned.dataset;
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

#[path = "reason.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
mod witness_tests;

#[cfg(test)]
#[path = "reason_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::reason_artifacts;
#[cfg(test)]
pub(crate) use test_support::reason_product;
#[cfg(test)]
pub(crate) use test_support::reason_test_dataset;
