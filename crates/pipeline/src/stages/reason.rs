// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `reason` stage: native EL/DL reasoned closure + artifacts — the SOLE
//! reasoning pass.
//!
//! It reasons ONCE over the object-level EDB
//! ([`crate::stages::carrier::assemble_object_level_edb`]: ontology + imports +
//! statements + alignments + logic/relational-core/correspondence, WITHOUT the
//! meta/report graphs), canonicalizes it (RDFC-1.0) for transport-independent Skolem
//! witnesses, runs `gmeow_logic::reason::reason_all`, and serializes the three
//! committed artifacts via the `gmeow_logic::reason::artifacts` builders. The single
//! result also backs the bundle's `graph/reasoning` projection (dual carriage), so
//! the closure shipped in `gmeow.gts` and the committed files agree by construction —
//! there is no separate full-fold export leaf. Reasoning requires the exclusive
//! [`ENGINE_RESOURCE`], so the scheduler serializes it against any stage competing
//! for the reasoning engine (this is the sole resource-bearing build stage).

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_logic::reason::artifacts::{
    DL_CERTIFIED_AGREE, EL_CERTIFIED_AGREE, RL_CERTIFIED_AGREE,
    build_all_subsumption_correspondences_ttl, build_dl_el_ledger_ttl, build_explanations_ttl,
    build_inferred_closure_ttl,
};
use gmeow_logic::reason::perf_ledger::perf_ledger;
use gmeow_logic::reason::reason_all;
use gmeow_logic::result::ReasoningResult;
use gmeow_logic::result_rdf::{GRAPH_REASONING, project_reasoning_result};
use purrdf::{NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfTerm};

use crate::bundle::{PipelineHandle, bundle_from_artifacts_over};
use crate::node::{ENGINE_RESOURCE, Stage, StageInput, StageOutput, StageProduct};

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
/// COMMITTED logical path of the native ⊒ oracle EL ⊂ RL ⊂ DL subsumption
/// correspondence bundle — the three reified `logic:Correspondence` individuals
/// certifying that the native forward engine subsumes the demoted oracle on each
/// fragment of the promotion lattice (a gap-zero section/retraction, complete
/// over-approximation). The on-gate correspondence drift-gate reads THIS committed
/// file and refuses a claim minted under a different native contract hash.
pub const CORRESPONDENCE_PATH: &str = "generated/logic/subsumption-correspondence.ttl";

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
    /// The native ⊒ oracle EL ⊂ RL ⊂ DL subsumption correspondence bundle Turtle —
    /// the three reified `logic:Correspondence` individuals (one per lattice edge)
    /// certifying native subsumption of the demoted oracle, gap-zero, bound to the
    /// native contract hash. The consumer for the on-gate correspondence drift-gate.
    pub subsumption_correspondence: String,
    /// The typed five-axis result (C7 handle payload).
    pub result: ReasoningResult,
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
    let canon_nquads = purrdf::canonical_flat_nquads(edb).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-reason".to_string(),
            message: format!("RDFC-1.0 canonicalize EDB: {e}"),
        })
    })?;
    let canon = purrdf::parse_dataset(
        canon_nquads.as_bytes(),
        NativeRdfFormat::NQuads.media_type(),
        None,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-reason".to_string(),
            message: format!("re-fold canonical quads: {e}"),
        })
    })?;
    let result = reason_all(canon.as_ref()).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-reason".to_string(),
            message: format!("native reasoning failed: {e}"),
        })
    })?;
    // Non-merge (the regenerate path): the closure is told-vs-inferred only.
    let closure = build_inferred_closure_ttl(&result, None).map_err(|e| {
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
    // The native ⊒ oracle subsumption correspondence bundle (Part B of the
    // native-chase promotion): three reified `logic:Correspondence` individuals, one
    // per EL ⊂ RL ⊂ DL lattice edge. The builder sources each edge's gap-zero
    // divergence counts from the committed per-fragment constants
    // (`EL/RL/DL_CERTIFIED_AGREE` / `_NATIVE_ONLY` in `gmeow_logic::reason::artifacts`) —
    // the REAL, non-zero native↔Nemo agreement measured on-gate by the parity gates
    // (`el/rl/dl_native_oracle_ledger_gap_zero`), which pin those constants to the live
    // measurement with an `assert_eq!`. The build itself does NOT run Nemo (native is the
    // production engine); carrying the measured counts as committed constants keeps the
    // shipped certificate's `agreeCount` real and drift-gated without a build-time oracle
    // run. It binds to the native contract hash carried on this run's result provenance
    // (identical to `native_contract_hash()`); `view_engine` names the demoted oracle
    // Nemo (native ⊒ nemo).
    let subsumption_correspondence =
        build_all_subsumption_correspondences_ttl(&result.provenance.contract_hash, "nemo");
    Ok(ReasonArtifacts {
        closure,
        explanations,
        ledger,
        perf_ledger: perf,
        subsumption_correspondence,
        result,
    })
}

/// Parse the closure Turtle into the default graph and FOLD the deterministic
/// `graph/reasoning` projection of `result` into the named graph [`GRAPH_REASONING`],
/// returning the dual-carriage dataset the reason stage's bundle backs. The closure
/// stays the default-graph contribution to the compose union; the reasoning
/// projection rides alongside as its own named graph (the typed handle's backing).
fn reason_dataset(
    closure_ttl: &str,
    result: &ReasoningResult,
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
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("freeze reason dual-carriage dataset: {e}"),
        })
    })
}

// ── correspondence drift-gate (the mandatory reader — adversary F4) ────────────

/// The `logic:` term namespace (mirrors `gmeow_logic::reason::artifacts`'s `logic()`).
const DRIFT_LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
/// The `gmeow:` term namespace (mirrors `gmeow_logic::reason::artifacts`'s `gmeow()`).
const DRIFT_GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The on-gate CONSUMER of the committed `subsumption-correspondence.ttl` projection.
///
/// It LOADS the committed bundle (Turtle bytes read from disk — NOT a fresh in-memory
/// build) and refuses it unless EVERY certified EL/RL/DL lattice edge still carries the
/// full, un-weakened native ⊒ oracle section/retraction claim AND binds to the CURRENT
/// native contract hash. Returns `Err` (the drift-gate's non-zero signal) on:
///
/// * a missing fragment (an edge silently dropped from the bundle);
/// * an absent or weakened status (`logic:correspondenceRelation` ≠ `logic:Subsumes`,
///   `logic:morphismClass` ≠ `logic:SectionRetraction`, `logic:preservationKind` ≠
///   `logic:CompleteOverApproximation`, or the discharged `logic:SectionLaw` claim not
///   `logic:ObligationDischarged` under `logic:DischargeCertifiedFragment`);
/// * `logic:contractHash` ≠ `expected_contract_hash` — the STALE-hash case: the native
///   engine's reasoning contract changed but the subsumption claim was not re-minted, so
///   CI must refuse to ship a correspondence that certifies a contract it no longer runs.
///
/// This is the property "CI can refuse a broken subsumption": a green `rust-test` lane
/// requires the committed claim to be current, complete, and discharged.
pub fn check_subsumption_correspondence_drift(
    ttl: &str,
    expected_contract_hash: &str,
) -> gmeow_errors::Result<()> {
    let sd =
        |message: String| gmeow_errors::Diag::of_kind(crate::error::SubsumptionDrift { message });
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .map_err(|e| sd(format!("parse subsumption-correspondence bundle: {e}")))?;
    let quads: Vec<purrdf::RdfQuad> = ds.owned_quads().collect();

    // Resolve the single IRI object of (subject, predicate); Err naming the miss.
    let iri_object = |subject: &str, predicate: &str| -> gmeow_errors::Result<String> {
        let mut hits = quads.iter().filter(|q| {
            matches!(&q.subject, purrdf::RdfTerm::Iri(s) if s == subject)
                && q.predicate == predicate
        });
        match hits.next() {
            Some(q) => match &q.object {
                purrdf::RdfTerm::Iri(o) => Ok(o.clone()),
                other => Err(sd(format!(
                    "<{subject}> <{predicate}> is not an IRI ({other})"
                ))),
            },
            None => Err(sd(format!("<{subject}> is missing <{predicate}>"))),
        }
    };
    // Resolve the single literal lexical value of (subject, predicate).
    let literal_object = |subject: &str, predicate: &str| -> gmeow_errors::Result<String> {
        let mut hits = quads.iter().filter(|q| {
            matches!(&q.subject, purrdf::RdfTerm::Iri(s) if s == subject)
                && q.predicate == predicate
        });
        match hits.next() {
            Some(q) => match &q.object {
                purrdf::RdfTerm::Literal(l) => Ok(l.lexical_form.clone()),
                other => Err(sd(format!(
                    "<{subject}> <{predicate}> is not a literal ({other})"
                ))),
            },
            None => Err(sd(format!("<{subject}> is missing <{predicate}>"))),
        }
    };
    let logic = |local: &str| format!("{DRIFT_LOGIC_NS}{local}");
    let gmeow = |local: &str| format!("{DRIFT_GMEOW_NS}{local}");
    // Resolve a `gmeow:` count literal on the correspondence subject and parse it to
    // usize; Err naming the miss or the un-parseable lexical form.
    let count_object = |subject: &str, local: &str| -> gmeow_errors::Result<usize> {
        let predicate = gmeow(local);
        let lexical = literal_object(subject, &predicate)?;
        lexical.parse::<usize>().map_err(|e| {
            sd(format!(
                "<{subject}> <{predicate}> = \"{lexical}\" is not a count: {e}"
            ))
        })
    };
    let assert_iri = |subject: &str, predicate: &str, want: &str| -> gmeow_errors::Result<()> {
        let got = iri_object(subject, predicate)?;
        if got == want {
            Ok(())
        } else {
            Err(sd(format!(
                "<{subject}> <{predicate}> = <{got}>, expected <{want}> (weakened/altered claim)"
            )))
        }
    };

    for slug in ["el", "rl", "dl"] {
        let correspondence = format!("{DRIFT_GMEOW_NS}{slug}-native-subsumption-correspondence");
        let law_claim = format!("{DRIFT_GMEOW_NS}{slug}-native-subsumption-lawclaim");

        // 1) the reified correspondence carries the full native ⊒ oracle claim shape.
        assert_iri(
            &correspondence,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            &logic("Correspondence"),
        )?;
        assert_iri(
            &correspondence,
            &logic("correspondenceRelation"),
            &logic("Subsumes"),
        )?;
        assert_iri(
            &correspondence,
            &logic("morphismClass"),
            &logic("SectionRetraction"),
        )?;
        assert_iri(
            &correspondence,
            &logic("preservationKind"),
            &logic("CompleteOverApproximation"),
        )?;
        let linked_claim = iri_object(&correspondence, &logic("hasLawClaim"))?;
        if linked_claim != law_claim {
            return Err(sd(format!(
                "{slug}: hasLawClaim points at <{linked_claim}>, expected <{law_claim}>"
            )));
        }

        // 2) the section law is discharged within the certified fragment — not weakened.
        assert_iri(
            &law_claim,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            &logic("LawClaim"),
        )?;
        assert_iri(&law_claim, &logic("lawClaimed"), &logic("SectionLaw"))?;
        assert_iri(
            &law_claim,
            &logic("lawDischargeVerdict"),
            &logic("ObligationDischarged"),
        )?;
        assert_iri(
            &law_claim,
            &logic("lawDischargeCondition"),
            &logic("DischargeCertifiedFragment"),
        )?;

        // 3) the claim binds to the CURRENT native contract hash (stale = refused).
        let hash = literal_object(&correspondence, &logic("contractHash"))?;
        if hash != expected_contract_hash {
            return Err(sd(format!(
                "{slug}: contractHash \"{hash}\" != current native contract \
                 \"{expected_contract_hash}\" — engine changed, subsumption claim not re-minted"
            )));
        }

        // 4) NON-VACUITY: the certificate must carry the REAL, measured native↔oracle
        // agreement — an all-zero (fabricated / hollow) certificate is refused. The
        // shipped `agreeCount` must equal the committed per-fragment constant (which the
        // on-gate parity gates pin to the live native↔Nemo measurement) AND be > 0. A
        // hand-zeroed or drifted count fails this gate: CI cannot ship a vacuous proof.
        let certified_agree = match slug {
            "el" => EL_CERTIFIED_AGREE,
            "rl" => RL_CERTIFIED_AGREE,
            "dl" => DL_CERTIFIED_AGREE,
            other => return Err(sd(format!("unknown fragment slug <{other}>"))),
        };
        let agree = count_object(&correspondence, "agreeCount")?;
        if agree == 0 {
            return Err(sd(format!(
                "{slug}: agreeCount is 0 — the certificate carries ZERO measured evidence \
                 (a hollow / fabricated all-zero proof), refused"
            )));
        }
        if agree != certified_agree {
            return Err(sd(format!(
                "{slug}: agreeCount {agree} != certified native↔oracle agreement \
                 {certified_agree} — the measured parity count drifted; re-mint the committed \
                 constant from the live parity gate and regenerate the certificate"
            )));
        }

        // 5) GAP-ZERO invariant: the certified over-approximation must miss no answers —
        // zero oracle-only and zero dl-gap rows (native ⊇ oracle, complete over the
        // fragment). A non-zero either count means the claim is not gap-zero.
        let oracle_only = count_object(&correspondence, "oracleOnlyCount")?;
        if oracle_only != 0 {
            return Err(sd(format!(
                "{slug}: oracleOnlyCount {oracle_only} != 0 — the oracle derived answers the \
                 native closure missed; native ⊉ oracle, not gap-zero"
            )));
        }
        let dl_gap = count_object(&correspondence, "dlGapCount")?;
        if dl_gap != 0 {
            return Err(sd(format!(
                "{slug}: dlGapCount {dl_gap} != 0 — a native DL coverage defect; not gap-zero"
            )));
        }
    }
    Ok(())
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
    /// compile-logic / mappings / source-load / statements producers (plus the on-disk
    /// authored / imports / alignments sources); the slice DAG's `stage-reason`
    /// `dataflowConsumes` mirrors this set. It requires the exclusive
    /// [`ENGINE_RESOURCE`] (the sole resource-bearing build stage), so the scheduler
    /// serializes it against any stage competing for the reasoning engine.
    ///
    /// Typed dataflow (artifact-level): from `stage-compile-logic` it reads ONLY the
    /// `logic`, `relational-core`, and `correspondence` named graphs (see
    /// [`crate::stages::carrier::assemble_object_level_edb`]) — never that product's
    /// other graphs or byte artifacts (diagnostics, the eight projection
    /// serializations). Declaring those three entities lets a change to compile-logic's
    /// diagnostics or projection bytes alone skip re-running the (expensive) reasoner.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-mappings".to_string(),
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
        "reason.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Reason ONCE over the object-level EDB (ontology + imports + statements +
        // alignments + logic/relational-core/correspondence), assembled in the SAME
        // graph layout the bundle carries but WITHOUT the meta/report graphs — they
        // assert no axioms, so excluding them is closure-isomorphic and makes the
        // Skolem witnesses a function of the ontology alone. This pass owns the
        // committed closure AND backs the bundle's `graph/reasoning`; there is no
        // second full-fold export leaf.
        let edb = crate::stages::carrier::assemble_object_level_edb(input.upstream)?;
        let reasoned = reason_over_dataset(edb.as_ref())?;
        // The CLOSURE is the reason stage's contribution to `gts_compose`'s union and
        // stays the dataset's DEFAULT graph. The EXPLANATIONS and LEDGER are diagnostic
        // REPORTS (proof skeletons / DL·EL crosscheck), NOT ontology facts; they stay
        // byte-lane only and are EXCLUDED from the compose union BY CONSTRUCTION. The
        // typed five-axis result rides BOTH as the `graph/reasoning` named graph (the
        // repo-free RDF projection) AND as the typed `PipelineHandle::Reasoning` handle
        // pinned to that graph (C7) — dual carriage.
        let dataset = reason_dataset(&reasoned.closure, &reasoned.result)?;
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
        artifacts.insert(
            CORRESPONDENCE_PATH.to_string(),
            reasoned.subsumption_correspondence.into_bytes(),
        );

        // Attach the typed Reasoning handle, pinned to the `graph/reasoning` named
        // graph's canonical digest. `pin_handle` HARD-fails on a digest mismatch, so a
        // handle that disagrees with its backing graph can never attach (fail-closed).
        let mut bundle = bundle_from_artifacts_over(
            dataset,
            artifacts,
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
        Ok(StageOutput::new(StageProduct::from_bundle(
            self.id(),
            Arc::new(bundle),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            (
                "subsumption_correspondence",
                &reasoned.subsumption_correspondence,
            ),
        ] {
            assert!(!ttl.trim().is_empty(), "{name} artifact is empty");
        }
        // The correspondence bundle carries all three certified lattice edges bound to
        // the live native contract hash (the on-gate drift-gate's contract).
        for slug in ["el", "rl", "dl"] {
            assert!(
                reasoned
                    .subsumption_correspondence
                    .contains(&format!("{slug}-native-subsumption-correspondence")),
                "correspondence bundle carries the {slug} lattice edge"
            );
        }
        assert!(
            reasoned
                .subsumption_correspondence
                .contains(&gmeow_logic::reason::native_contract_hash()),
            "correspondence bundle binds to the live native contract hash"
        );
        assert!(reasoned.closure.contains("<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> ."));
        // The perf ledger flags the deferred / non-incremental levers (static content).
        assert!(
            reasoned
                .perf_ledger
                .contains("https://blackcatinformatics.ca/gmeow/FlaggedNonIncremental"),
            "the perf ledger flags the non-incremental hard parts"
        );
    }

    #[test]
    fn committed_subsumption_correspondence_is_current_and_discharged() {
        // The on-gate CONSUMER (adversary F4): load the COMMITTED bundle from disk and
        // assert every EL/RL/DL edge is present, fully discharged, and bound to the CURRENT
        // native contract hash. A stale/broken committed claim fails this `rust-test` lane.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("canonicalize repo root");
        let ttl = std::fs::read_to_string(root.join(CORRESPONDENCE_PATH)).unwrap_or_else(|e| {
            panic!("read committed {CORRESPONDENCE_PATH} (run `make regenerate` first): {e}")
        });
        check_subsumption_correspondence_drift(&ttl, &gmeow_logic::reason::native_contract_hash())
            .expect(
                "committed subsumption-correspondence must be current, complete, and discharged",
            );
    }

    #[test]
    fn drift_gate_accepts_a_freshly_minted_bundle() {
        let hash = gmeow_logic::reason::native_contract_hash();
        let ttl = build_all_subsumption_correspondences_ttl(&hash, "nemo");
        check_subsumption_correspondence_drift(&ttl, &hash)
            .expect("a freshly minted bundle at the current hash passes");
    }

    #[test]
    fn drift_gate_refuses_a_hand_zeroed_agree_count() {
        // The NON-VACUITY guard (the falsifiable teeth): take a VALID committed bundle
        // and hand-zero one edge's measured agreeCount. A hollow all-zero certificate —
        // indistinguishable from a fabricated proof carrying no measured evidence — MUST
        // be refused even though every claim-shape / discharge / hash assertion still holds.
        let hash = gmeow_logic::reason::native_contract_hash();
        let ttl = build_all_subsumption_correspondences_ttl(&hash, "nemo");
        check_subsumption_correspondence_drift(&ttl, &hash)
            .expect("the real bundle carries non-zero measured agreement");
        // Rewrite the EL edge's real count to 0 (the certified EL agree is 13).
        let zeroed = ttl.replace(
            &format!(
                "<{DRIFT_GMEOW_NS}agreeCount> {}",
                gmeow_logic::reason::artifacts::EL_CERTIFIED_AGREE
            ),
            &format!("<{DRIFT_GMEOW_NS}agreeCount> 0"),
        );
        assert_ne!(
            zeroed, ttl,
            "the string replace must have hit the EL agreeCount"
        );
        let err = check_subsumption_correspondence_drift(&zeroed, &hash)
            .expect_err("a hand-zeroed agreeCount must be refused");
        assert!(
            err.message().contains("agreeCount")
                && (err.message().contains("ZERO") || err.message().contains("drifted")),
            "the refusal names the vacuous / drifted count: {err}"
        );
    }

    #[test]
    fn drift_gate_refuses_a_stale_contract_hash() {
        // Mint under one contract, verify against a DIFFERENT (current) one: the engine
        // changed but the claim was not re-minted — the drift-gate MUST refuse it. The
        // committed counts still bind (they are a property of the corpus, not the hash),
        // so the refusal isolates the stale-hash cause.
        let ttl = build_all_subsumption_correspondences_ttl("0000-old-native-contract", "nemo");
        check_subsumption_correspondence_drift(&ttl, "0000-old-native-contract")
            .expect("matching hash passes");
        let err = check_subsumption_correspondence_drift(&ttl, "ffff-current-native-contract")
            .expect_err("a stale contract hash must be refused");
        assert!(
            err.message().contains("contractHash") && err.message().contains("not re-minted"),
            "the refusal names the stale-hash cause: {err}"
        );
    }

    #[test]
    fn drift_gate_refuses_a_weakened_section_law() {
        // Downgrade the discharged status: the drift-gate must refuse a weakened claim.
        let hash = gmeow_logic::reason::native_contract_hash();
        let ttl = build_all_subsumption_correspondences_ttl(&hash, "nemo");
        let weakened = ttl.replace("ObligationDischarged", "ObligationPending");
        let err = check_subsumption_correspondence_drift(&weakened, &hash)
            .expect_err("a weakened section-law discharge must be refused");
        assert!(
            err.message().contains("lawDischargeVerdict"),
            "the refusal names the weakened discharge: {err}"
        );
    }

    #[test]
    fn drift_gate_refuses_a_missing_fragment() {
        // Drop the DL edge entirely: the drift-gate must refuse an incomplete bundle.
        let hash = gmeow_logic::reason::native_contract_hash();
        let el = build_all_subsumption_correspondences_ttl(&hash, "nemo");
        // Cut at the DL section banner so EL+RL stay a VALID (parseable) Turtle doc but the
        // whole DL edge (subject + law claim) is gone — a silently incomplete bundle.
        let cut = el
            .find("the reified native ⊒ oracle DL correspondence")
            .expect("bundle carries the dl section");
        let truncated = &el[..cut];
        let err = check_subsumption_correspondence_drift(truncated, &hash)
            .expect_err("a missing DL fragment must be refused");
        assert!(
            err.message()
                .contains("dl-native-subsumption-correspondence")
                && err.message().contains("missing"),
            "the refusal names the dropped fragment: {err}"
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
        let dataset = reason_dataset(&reasoned.closure, &reasoned.result).expect("dual dataset");
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
    }

    #[test]
    fn pin_handle_hard_fails_on_a_digest_mismatch() {
        let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
"#;
        let reasoned = reason_artifacts(nq).expect("reason");
        let dataset = reason_dataset(&reasoned.closure, &reasoned.result).expect("dual dataset");
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
