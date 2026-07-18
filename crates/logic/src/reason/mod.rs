// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native, ontology-independent OWL-2 reasoning.
//!
//! This module hosts fixed, built-in entailment rule sets — an intrinsic
//! entailment calculus — that run over an arbitrary TBox/ABox through the world-scoped
//! ternary gmeow encoding. Unlike the user-authored `logic:` programs the
//! the compiler pipeline projects, these rule sets are intrinsic to the
//! reasoner: they encode the OWL semantics themselves, not a domain ontology.
//!
//! Provides the EL subsumption closure ([`el`]), the predicate-as-DATA RL/DL
//! native closure ([`rl`] + [`dl`]), and the divergence ledger ([`ledger`])
//! comparing native results against captured external corpora.

pub mod artifacts;
pub mod dl;
pub mod el;
pub mod ledger;
pub mod perf_ledger;
pub mod rl;
pub(crate) mod rl_rules;

pub use dl::{DlGap, DlVerdict, InconsistencyWitness, UnsatClass, dl_consistency};
pub use el::{ElClosure, InferredAxiom, el_closure};
pub use ledger::{
    DivergenceKind, DivergenceLedger, ExternalComparison, LedgerRow, LedgerVerdict, build_ledger,
    compare_external_corpus, divergence_diag_ledger, divergence_findings, dl_gap_rows, enforce,
};
pub use rl::{RlClosure, RlTriple, rl_closure};

use crate::facts::TypedFactSet;
use crate::oracle::TypedRow;
use crate::query_ir::Budget;
use crate::result::{
    BudgetLimit, BudgetUsage, CompletenessStatus, EvaluationStatus, InformationState, InputStatus,
    PreservationClaim, ReasoningResult, ResultPayload, ResultProvenance,
};
use crate::seam::BudgetStatus;
use crate::store::WorldStore;
use purrdf::{
    DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm,
    RdfTriple, TermRef, TermValue,
};

/// One production existential-program admission certificate, scoped to the RDF
/// world whose obligations were chased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaseCertificate {
    /// Named-graph world whose existential program was certified and evaluated.
    pub world: String,
    /// The native chase termination certificate and its proof evidence.
    pub admission: crate::materialize::ChaseAdmission,
}

impl ChaseCertificate {
    /// Project this world-scoped certificate onto the shared diagnostic model.
    #[must_use]
    pub fn to_finding(&self) -> gmeow_errors::Finding {
        let mut finding = self.admission.to_finding();
        finding.message = format!("world <{}>: {}", self.world, finding.message);
        finding
    }
}

/// The decomposable derivation of one chase-invented null, re-exported so a
/// consumer of [`CertifiedReasoning`] can explain an invented individual without
/// re-running the chase.
pub use crate::physical::WitnessDerivation;

/// The content-addressed standard-RDF-reification node IRI for a head quad
/// `⟨subject predicate obj⟩`.
///
/// A thin `pub` shim over the crate-internal reifier recipe
/// ([`crate::provenance::reifier_from_strings`]) so a downstream projector (the
/// pipeline's chase-witness diagnostics fold) can address the SAME reifier node the
/// explanation plane already mints — without widening the internal helper.
/// `subject` and `predicate` are bare IRI strings (this wraps them in `<…>`);
/// `obj_n3` is the object already in canonical N3 form (`<iri>` for an IRI object,
/// `"lex"^^<dt>` for a literal) and is used verbatim.
#[must_use]
pub fn reifier_iri(subject: &str, predicate: &str, obj_n3: &str) -> String {
    crate::provenance::reifier_from_strings(subject, predicate, obj_n3)
}

/// The single production reasoning run and the existential termination evidence
/// generated while constructing its result.
#[derive(Debug, Clone, PartialEq)]
pub struct CertifiedReasoning {
    /// Typed five-axis reasoning result.
    pub result: ReasoningResult,
    /// Deterministic, deduplicated world-scoped chase certificates.
    pub chase_certificates: Vec<ChaseCertificate>,
    /// The decomposable derivation of every invented null the chase minted,
    /// sorted and deduplicated by content-addressed witness IRI. Empty when the
    /// production program has no existential obligation. Each carries the firing
    /// rule, existential ordinal, and frontier binding — the recipe an
    /// `explain(witness)` consumer decomposes.
    pub witness_derivations: Vec<WitnessDerivation>,
}

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// The content-addressed identity of the native EL/DL/RL reasoning contract —
/// the `contract_hash` every native-reason result is produced under.
///
/// The hash covers ALL source that defines the reasoning contract:
/// * the fixed typed EL/DL/RL rule sets whose change alters which axioms the
///   native chase derives;
/// * the full source of `dl.rs`, which owns the post-pass functions
///   `augment_inferred_with_dl`, `verdict_from_inferred`, `scan_coverage`, and
///   `classify_coverage` — any edit to those changes the contract semantics even
///   when the rule text is unchanged;
/// * the structured rule IR, typed adapter, plan, semi-naive evaluator, restricted
///   existential chase, and relation store that execute those rules;
/// * the canonical-program lowering, selected-view materializer, and native
///   non-monotone evaluators exposed as the forward runtime materialization surface;
/// * the source of this file (`mod.rs`), which owns the production reasoning
///   orchestration and typed-result fold.
///
/// A change to any of these files will produce a different hash, invalidating
/// cached results produced under the old contract. Public so a consumer holding a
/// shipped `graph/reasoning` verdict can refuse one minted under a different
/// contract than the engine it is about to trust it against.
const NATIVE_CONTRACT_COMPONENTS: &[(&str, &str)] = &[
    ("reason/el.rs", include_str!("el.rs")),
    ("reason/rl_rules.rs", include_str!("rl_rules.rs")),
    ("reason/dl.rs", include_str!("dl.rs")),
    ("reason/mod.rs", include_str!("mod.rs")),
    ("oracle.rs", include_str!("../oracle.rs")),
    ("certify.rs", include_str!("../certify.rs")),
    ("lower.rs", include_str!("../lower.rs")),
    ("materialize.rs", include_str!("../materialize.rs")),
    ("relational_core.rs", include_str!("../relational_core.rs")),
    ("stablemodel.rs", include_str!("../stablemodel.rs")),
    ("wellfounded.rs", include_str!("../wellfounded.rs")),
    ("rule_ir.rs", include_str!("../rule_ir.rs")),
    ("physical/plan.rs", include_str!("../physical/plan.rs")),
    (
        "physical/seminaive.rs",
        include_str!("../physical/seminaive.rs"),
    ),
    ("physical/chase.rs", include_str!("../physical/chase.rs")),
    ("physical/store.rs", include_str!("../physical/store.rs")),
];

pub fn native_contract_hash() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        // Contract source is immutable for the lifetime of a compiled binary. Frame
        // every component by name and byte length so neither path/content boundaries
        // nor concatenation ambiguity can produce the same semantic identity.
        let mut contract = String::new();
        for (name, source) in NATIVE_CONTRACT_COMPONENTS {
            use std::fmt::Write as _;
            write!(&mut contract, "{}:{name}:{}:", name.len(), source.len())
                .expect("String writes cannot fail");
            contract.push_str(source);
        }
        crate::provenance::sha1_hex(&contract)
    })
    .clone()
}

/// Run the native single-chase pipeline and return the shared
/// `(closure, DlVerdict)` it produces.
///
/// `run_reasoning → augment_inferred_with_dl → sort → verdict_from_inferred`:
/// the closure is the asserted + derived IRI-object triples; the verdict is the
/// DL consistency / unsatisfiability record read off that same closure. Both the
/// typed [`reason_all`] result and the verdict-only [`dl::dl_consistency`] entry
/// point fold from this one pipeline so they can never disagree.
///
/// # Errors
///
/// Returns `Err` if the source store cannot be loaded, native evaluation fails,
/// or coverage/consistency scanning fails.
pub(crate) fn reason_closure(
    edb: &RdfDataset,
) -> gmeow_errors::Result<(Vec<InferredAxiom>, dl::DlVerdict)> {
    let inferred = reason_closure_axioms(edb)?;
    let verdict = dl::verdict_from_inferred(&inferred, edb)?;
    Ok((inferred, verdict))
}

/// The native reasoning closure ONLY — the sorted asserted+derived axiom set — with
/// the DL consistency *verdict* left uncomputed.
///
/// This is the shared `run_reasoning → augment_inferred_with_dl → sort` half of
/// [`reason_closure`], so the returned closure is byte-identical to
/// `reason_all(edb)?.inferred()`. It exists for callers that need only the closure
/// and would otherwise pay for — and discard — [`dl::verdict_from_inferred`]'s
/// O(EDB) consistency/coverage scan. The reasoner-derived slice-quality axis is the
/// motivating caller: its leave-one-out redundancy probe re-reasons the EDB dozens of
/// times but reads only the closure's IRI-object triples, never the verdict.
///
/// # Errors
///
/// Returns `Err` if the source store cannot be loaded or native evaluation fails —
/// the same closure-side failures [`reason_closure`] surfaces (it omits only the
/// verdict-scan error path).
pub fn reason_closure_axioms(edb: &RdfDataset) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    let mut inferred = run_reasoning_rules(edb, dl::structured_dl_rules())?;
    dl::augment_inferred_with_dl(&mut inferred, edb)?;
    inferred.sort();
    Ok(inferred)
}

/// One IRI-object axiom to probe through exact leave-one-out reasoning.
///
/// The probe removes every occurrence of the triple from every RDF world, matching
/// the ontology-quality scorer's authored-axiom semantics. The result reports
/// whether the same triple is derivable in at least one world after that removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveOneOutAxiom {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

impl LeaveOneOutAxiom {
    #[must_use]
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
        }
    }
}

/// Fixed-calculus state for one RDF world during a batch of leave-one-out probes.
///
/// The settled incremental session is cheap to clone: its rule plan, fact arena,
/// EDB, and fixed-point histories are `Arc` backed. Each probe therefore pays only
/// for the signed retraction and its affected recursive frontier.
struct LeaveOneOutWorld {
    world: String,
    asserted: std::collections::BTreeSet<crate::rule_ir::FactKey>,
    session: crate::physical::IncrementalSession,
    base_axioms: Vec<InferredAxiom>,
}

fn incremental_axioms(
    session: &crate::physical::IncrementalSession,
    world: &str,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    session
        .closure()
        .into_iter()
        .map(|fact| {
            Ok(InferredAxiom {
                subject: subject_iri(&fact.subject)?,
                predicate: fact.predicate,
                object: crate::provenance::term_display(&fact.object),
                world: world.to_owned(),
                // The DL augmentation consumes only the fact surface. The reduced
                // RDF dataset below remains the authority for asserted/derived
                // provenance and supplies every surviving EDB fact again.
                is_edb: false,
                rule_name: None,
                premises: Vec::new(),
            })
        })
        .collect()
}

fn leave_one_out_worlds(edb: &RdfDataset) -> gmeow_errors::Result<Vec<LeaveOneOutWorld>> {
    let mut by_world: std::collections::BTreeMap<String, Vec<crate::rule_ir::Fact>> =
        std::collections::BTreeMap::new();
    for (world, fact) in dl::fixed_rule_resource_facts(edb) {
        by_world.entry(world).or_default().push(fact);
    }

    let rules = dl::structured_dl_rules();
    by_world
        .into_iter()
        .map(|(world, facts)| {
            let asserted = facts.iter().map(crate::rule_ir::Fact::key).collect();
            let session =
                crate::physical::IncrementalSession::new(native_contract_hash(), facts, &rules)?;
            let base_axioms = incremental_axioms(&session, &world)?;
            Ok(LeaveOneOutWorld {
                world,
                asserted,
                session,
                base_axioms,
            })
        })
        .collect()
}

fn dataset_without_axiom(
    edb: &RdfDataset,
    axiom: &LeaveOneOutAxiom,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in edb.owned_quads() {
        if quad.predicate == axiom.predicate
            && matches!(&quad.subject, RdfTerm::Iri(subject) if subject == &axiom.subject)
            && matches!(&quad.object, RdfTerm::Iri(object) if object == &axiom.object)
        {
            continue;
        }
        builder.push_owned_quad(&quad);
    }
    builder
        .freeze()
        .map_err(|error| reason_err(format!("freeze leave-one-out RDF dataset: {error}")))
}

const LOO_RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const LOO_RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const LOO_RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const LOO_RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const LOO_RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const LOO_OWL_NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const LOO_OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
const LOO_OWL_DISJOINT_UNION_OF: &str = "http://www.w3.org/2002/07/owl#disjointUnionOf";
const LOO_OWL_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#complementOf";
const LOO_OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const LOO_OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const LOO_OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";
const LOO_OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const LOO_OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const LOO_OWL_INVERSE_OF: &str = "http://www.w3.org/2002/07/owl#inverseOf";
const LOO_OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const LOO_OWL_EQUIVALENT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#equivalentProperty";
const LOO_OWL_PROPERTY_CHAIN_AXIOM: &str = "http://www.w3.org/2002/07/owl#propertyChainAxiom";

fn dataset_has_pattern(
    edb: &RdfDataset,
    subject: Option<&str>,
    predicate: &str,
    object: Option<&str>,
) -> bool {
    let Some(predicate) = edb.term_id_by_value(&TermValue::iri(predicate)) else {
        return false;
    };
    let subject = match subject {
        Some(value) => {
            let Some(value) = edb.term_id_by_value(&TermValue::iri(value)) else {
                return false;
            };
            Some(value)
        }
        None => None,
    };
    let object = match object {
        Some(value) => {
            let Some(value) = edb.term_id_by_value(&TermValue::iri(value)) else {
                return false;
            };
            Some(value)
        }
        None => None,
    };
    edb.quads_for_pattern(subject, Some(predicate), object, GraphMatch::Any)
        .next()
        .is_some()
}

/// Whether the finite DL post-pass can introduce assertions using `predicate`
/// even when the positive fixed-rule closure currently contains none.
///
/// The only open-headed DL producer is the restricted existential chase. Its
/// predicates come from `owl:onProperty`, then may travel through the ordinary
/// property schema. A direct schema mention is therefore a conservative witness
/// that the post-pass might still create the predicate.
fn dataset_may_generate_dynamic_predicate(edb: &RdfDataset, predicate: &str) -> bool {
    dataset_has_pattern(edb, None, LOO_OWL_ON_PROPERTY, Some(predicate))
        || dataset_has_pattern(edb, Some(predicate), LOO_RDFS_SUBPROPERTY_OF, None)
        || dataset_has_pattern(edb, None, LOO_RDFS_SUBPROPERTY_OF, Some(predicate))
        || dataset_has_pattern(edb, Some(predicate), LOO_OWL_INVERSE_OF, None)
        || dataset_has_pattern(edb, None, LOO_OWL_INVERSE_OF, Some(predicate))
        || dataset_has_pattern(edb, Some(predicate), LOO_OWL_EQUIVALENT_PROPERTY, None)
        || dataset_has_pattern(edb, None, LOO_OWL_EQUIVALENT_PROPERTY, Some(predicate))
        || dataset_has_pattern(edb, Some(predicate), LOO_OWL_PROPERTY_CHAIN_AXIOM, None)
}

/// A sound negative filter for the expensive finite-DL leave-one-out fallback.
///
/// Outside `owl:Nothing`, the DL post-pass introduces `rdfs:subClassOf` only by
/// expanding union lists (and then closing the new edges transitively). When the
/// reduced positive closure has no union axiom and neither union nor
/// `rdfs:subClassOf` can be introduced by an open-headed producer, a missing
/// subclass target cannot appear later. Every other target conservatively keeps
/// the full production pass.
fn finite_dl_may_rederive_after_rule_miss(
    edb: &RdfDataset,
    inferred: &[InferredAxiom],
    axiom: &LeaveOneOutAxiom,
) -> bool {
    if axiom.predicate != LOO_RDFS_SUBCLASS_OF || axiom.object == LOO_OWL_NOTHING {
        return true;
    }
    let has_union = inferred.iter().any(|fact| {
        fact.predicate == LOO_OWL_UNION_OF || fact.predicate == LOO_OWL_DISJOINT_UNION_OF
    });
    has_union
        || dataset_has_pattern(edb, None, LOO_OWL_UNION_OF, None)
        || dataset_has_pattern(edb, None, LOO_OWL_DISJOINT_UNION_OF, None)
        || dataset_may_generate_dynamic_predicate(edb, LOO_OWL_UNION_OF)
        || dataset_may_generate_dynamic_predicate(edb, LOO_OWL_DISJOINT_UNION_OF)
        || dataset_may_generate_dynamic_predicate(edb, LOO_RDFS_SUBCLASS_OF)
}

fn batch_subclass_reachability_is_exact(edb: &RdfDataset) -> bool {
    !dataset_may_generate_dynamic_predicate(edb, LOO_OWL_UNION_OF)
        && !dataset_may_generate_dynamic_predicate(edb, LOO_OWL_DISJOINT_UNION_OF)
        && !dataset_may_generate_dynamic_predicate(edb, LOO_RDFS_SUBCLASS_OF)
        && !dataset_may_generate_dynamic_predicate(edb, LOO_OWL_EQUIVALENT_CLASS)
}

fn batch_subproperty_reachability_is_exact(edb: &RdfDataset) -> bool {
    !dataset_may_generate_dynamic_predicate(edb, LOO_RDFS_SUBPROPERTY_OF)
        && !dataset_may_generate_dynamic_predicate(edb, LOO_OWL_EQUIVALENT_PROPERTY)
}

fn fixed_head_is_absent(edb: &RdfDataset, predicate: &str) -> bool {
    matches!(
        predicate,
        LOO_RDFS_DOMAIN
            | LOO_RDFS_RANGE
            | LOO_OWL_EQUIVALENT_CLASS
            | LOO_OWL_EQUIVALENT_PROPERTY
            | LOO_OWL_INVERSE_OF
    ) && !dataset_may_generate_dynamic_predicate(edb, predicate)
}

fn batch_disjoint_support_is_exact(edb: &RdfDataset) -> bool {
    [
        LOO_OWL_DISJOINT_WITH,
        LOO_OWL_COMPLEMENT_OF,
        LOO_OWL_DISJOINT_UNION_OF,
        LOO_OWL_MEMBERS,
    ]
    .into_iter()
    .all(|predicate| !dataset_may_generate_dynamic_predicate(edb, predicate))
}

fn is_property_characteristic(iri: &str) -> bool {
    matches!(
        iri,
        "http://www.w3.org/2002/07/owl#TransitiveProperty"
            | "http://www.w3.org/2002/07/owl#SymmetricProperty"
            | "http://www.w3.org/2002/07/owl#AsymmetricProperty"
            | "http://www.w3.org/2002/07/owl#ReflexiveProperty"
            | "http://www.w3.org/2002/07/owl#IrreflexiveProperty"
            | "http://www.w3.org/2002/07/owl#FunctionalProperty"
            | "http://www.w3.org/2002/07/owl#InverseFunctionalProperty"
    )
}

/// Direct relation graph for exact batched transitive-reduction probes.
///
/// The boolean edge payload records a non-removable equivalence support. A raw
/// `A rdfs:subClassOf B` edge is removed by the `(A, B)` probe, but the same edge
/// remains justified when `A owl:equivalentClass B` is also authored.
#[derive(Default)]
struct TransitiveReachability {
    worlds: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, bool>>,
    >,
}

impl TransitiveReachability {
    fn new(
        edb: &RdfDataset,
        predicate: &str,
        equivalence: &str,
        equivalence_in_finite_dl: bool,
    ) -> Self {
        let mut reachability = Self::default();
        let structured_worlds = dl::structured_rule_worlds(edb);
        for (world, fact) in dl::fixed_rule_resource_facts(edb) {
            let (TermValue::Iri(subject), TermValue::Iri(object)) = (fact.subject, fact.object)
            else {
                continue;
            };
            if fact.predicate == predicate {
                reachability.insert(&world, &subject, &object, false);
            } else if fact.predicate == equivalence
                && (equivalence_in_finite_dl || structured_worlds.contains(&world))
            {
                reachability.insert(&world, &subject, &object, true);
                reachability.insert(&world, &object, &subject, true);
            }
        }
        reachability
    }

    fn with_finite_dl_subclasses(mut self, edb: &RdfDataset) -> Self {
        for (world, subject, object) in dl::finite_dl_subclass_edges(edb) {
            self.insert(&world, &subject, &object, true);
        }
        self
    }

    fn insert(&mut self, world: &str, subject: &str, object: &str, independent: bool) {
        self.worlds
            .entry(world.to_owned())
            .or_default()
            .entry(subject.to_owned())
            .or_default()
            .entry(object.to_owned())
            .and_modify(|current| *current |= independent)
            .or_insert(independent);
    }

    fn rederived_without(&self, axiom: &LeaveOneOutAxiom) -> bool {
        self.worlds.values().any(|adjacency| {
            let mut visited = std::collections::BTreeSet::new();
            let mut frontier = vec![axiom.subject.as_str()];
            visited.insert(axiom.subject.as_str());

            while let Some(subject) = frontier.pop() {
                let Some(edges) = adjacency.get(subject) else {
                    continue;
                };
                for (object, independently_supported) in edges {
                    if subject == axiom.subject
                        && object == &axiom.object
                        && !independently_supported
                    {
                        continue;
                    }
                    if object == &axiom.object {
                        return true;
                    }
                    if visited.insert(object.as_str()) {
                        frontier.push(object.as_str());
                    }
                }
            }
            false
        })
    }

    fn has_nonself_incoming(&self, target: &str) -> bool {
        self.worlds.values().any(|adjacency| {
            adjacency
                .iter()
                .any(|(subject, edges)| subject != target && edges.contains_key(target))
        })
    }
}

struct DisjointPossibility(std::collections::BTreeSet<(String, String)>);

impl DisjointPossibility {
    fn new(edb: &RdfDataset) -> Self {
        Self(
            dl::finite_dl_disjoint_candidates(edb)
                .into_iter()
                .map(|(_, subject, object)| (subject, object))
                .collect(),
        )
    }

    fn cannot_be_rederived(&self, axiom: &LeaveOneOutAxiom) -> bool {
        !self
            .0
            .contains(&(axiom.subject.clone(), axiom.object.clone()))
    }
}

fn characteristic_type_has_no_alternative_producer(
    edb: &RdfDataset,
    class_reachability: &TransitiveReachability,
    axiom: &LeaveOneOutAxiom,
) -> bool {
    if axiom.predicate != LOO_RDF_TYPE || !is_property_characteristic(&axiom.object) {
        return false;
    }
    if class_reachability.has_nonself_incoming(&axiom.object)
        || dataset_has_pattern(edb, None, LOO_RDFS_DOMAIN, Some(&axiom.object))
        || dataset_has_pattern(edb, None, LOO_RDFS_RANGE, Some(&axiom.object))
    {
        return false;
    }
    for predicate in [
        LOO_OWL_ON_PROPERTY,
        LOO_OWL_UNION_OF,
        LOO_OWL_DISJOINT_UNION_OF,
        LOO_OWL_INTERSECTION_OF,
        LOO_OWL_ONE_OF,
    ] {
        if dataset_has_pattern(edb, Some(&axiom.object), predicate, None) {
            return false;
        }
    }
    [
        LOO_RDF_TYPE,
        LOO_RDFS_DOMAIN,
        LOO_RDFS_RANGE,
        LOO_OWL_ONE_OF,
        LOO_OWL_INTERSECTION_OF,
    ]
    .into_iter()
    .all(|predicate| !dataset_may_generate_dynamic_predicate(edb, predicate))
}

fn leave_one_out_probe(
    edb: &RdfDataset,
    worlds: &[LeaveOneOutWorld],
    axiom: &LeaveOneOutAxiom,
) -> gmeow_errors::Result<bool> {
    let candidate = crate::rule_ir::Fact {
        subject: TermValue::iri(axiom.subject.clone()),
        predicate: axiom.predicate.clone(),
        object: TermValue::iri(axiom.object.clone()),
    };
    let candidate_key = candidate.key();
    let mut inferred = Vec::new();

    for state in worlds {
        if state.asserted.contains(&candidate_key) {
            let mut fork = state.session.clone();
            fork.apply([crate::physical::SignedFact {
                fact: candidate.clone(),
                weight: -1,
            }])?;
            inferred.extend(incremental_axioms(&fork, &state.world)?);
        } else {
            inferred.extend(state.base_axioms.iter().cloned());
        }
    }

    let target_is_inferred = |inferred: &InferredAxiom| {
        inferred.subject == axiom.subject
            && inferred.predicate == axiom.predicate
            && inferred
                .object
                .trim_start_matches('<')
                .trim_end_matches('>')
                == axiom.object
    };

    // The positive rule closure is a sound subset of the complete native result.
    // Most redundant authored axioms are already witnessed here, so avoid rebuilding
    // the reduced RDF dataset and rerunning the finite DL pass when the answer is
    // irrevocably true. A miss still takes the exact production DL path below.
    if inferred.iter().any(&target_is_inferred) {
        return Ok(true);
    }
    if !finite_dl_may_rederive_after_rule_miss(edb, &inferred, axiom) {
        return Ok(false);
    }

    // The finite DL pass reads structural lists, restrictions, and raw resource
    // facts directly from the RDF dataset. Give it the exact same reduced dataset
    // as the former scratch implementation so this optimization changes cost, not
    // semantics.
    let reduced = dataset_without_axiom(edb, axiom)?;
    dl::augment_inferred_with_dl(&mut inferred, &reduced)?;
    Ok(inferred.iter().any(target_is_inferred))
}

/// Determine which authored axioms are re-derived after exact leave-one-out.
///
/// Exact class/property reachability, finite union edges, fixed-head absence, and
/// finite-disjoint impossibility are answered from shared batch indexes. Remaining
/// probes plan and settle the fixed native calculus once per RDF world, then fork
/// that immutable state and apply one signed EDB retraction. Results retain input
/// order and ambiguous finite-DL cases still use the same production augmentation
/// as [`reason_closure_axioms`].
///
/// # Errors
///
/// Returns an error if the current fixed calculus leaves the finite positive binary
/// fragment, an RDF world cannot be decoded, a retraction is invalid, or the native
/// DL augmentation fails. There is no hidden scratch fallback.
pub fn leave_one_out_rederived(
    edb: &RdfDataset,
    axioms: &[LeaveOneOutAxiom],
) -> gmeow_errors::Result<Vec<bool>> {
    use rayon::prelude::*;

    if axioms.is_empty() {
        return Ok(Vec::new());
    }
    let subclass_reachability = batch_subclass_reachability_is_exact(edb).then(|| {
        TransitiveReachability::new(edb, LOO_RDFS_SUBCLASS_OF, LOO_OWL_EQUIVALENT_CLASS, false)
            .with_finite_dl_subclasses(edb)
    });
    let subproperty_reachability = batch_subproperty_reachability_is_exact(edb).then(|| {
        TransitiveReachability::new(
            edb,
            LOO_RDFS_SUBPROPERTY_OF,
            LOO_OWL_EQUIVALENT_PROPERTY,
            true,
        )
    });
    let disjoint_possibility =
        batch_disjoint_support_is_exact(edb).then(|| DisjointPossibility::new(edb));
    let mut results = vec![false; axioms.len()];
    let mut slow = Vec::new();
    for (index, axiom) in axioms.iter().enumerate() {
        if axiom.predicate == LOO_RDFS_SUBCLASS_OF
            && axiom.object != LOO_OWL_NOTHING
            && let Some(reachability) = &subclass_reachability
        {
            results[index] = reachability.rederived_without(axiom);
        } else if axiom.predicate == LOO_RDFS_SUBPROPERTY_OF
            && let Some(reachability) = &subproperty_reachability
        {
            results[index] = reachability.rederived_without(axiom);
        } else if axiom.predicate == LOO_OWL_DISJOINT_WITH {
            if disjoint_possibility
                .as_ref()
                .is_some_and(|possibility| possibility.cannot_be_rederived(axiom))
            {
                results[index] = false;
            } else {
                slow.push((index, axiom));
            }
        } else if let Some(reachability) = &subclass_reachability
            && characteristic_type_has_no_alternative_producer(edb, reachability, axiom)
        {
            results[index] = false;
        } else if fixed_head_is_absent(edb, &axiom.predicate) {
            results[index] = false;
        } else {
            slow.push((index, axiom));
        }
    }
    if slow.is_empty() {
        return Ok(results);
    }

    let worlds = leave_one_out_worlds(edb)?;
    let slow_results = slow
        .par_iter()
        .map(|(index, axiom)| Ok((*index, leave_one_out_probe(edb, &worlds, axiom)?)))
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    for (index, value) in slow_results {
        results[index] = value;
    }
    Ok(results)
}

/// Run native predicate-as-DATA entailment + DL consistency, returning the typed
/// [`ReasoningResult`] (ME2) — the single shared result model every
/// consumer reads.
///
/// The DL verdict is folded into the result via
/// [`ReasoningResult::from_dl_verdict`]: an inconsistent verdict becomes
/// `information=both` carrying its contradiction witnesses; a consistent verdict
/// is `information=supported` (conclusively, when no construct is uncovered);
/// uncovered DL constructs surface in `preservation.unsupported_constructs` and
/// drop the completeness to `incomplete`. The DL-only diagnostics not part of the
/// shared model (the construct coverage inventory, the unsatisfiable-class set)
/// are recovered from the shared closure by [`dl::scan_coverage`] /
/// [`dl::unsatisfiable_from_inferred`] where a consumer needs them.
///
/// # Errors
///
/// Returns `Err` if the source store cannot be loaded, native evaluation fails,
/// or coverage/consistency scanning fails.
pub fn reason_all(edb: &RdfDataset) -> gmeow_errors::Result<ReasoningResult> {
    Ok(reason_all_certified(edb)?.result)
}

/// Run the same production reasoning path as [`reason_all`] while retaining the
/// existential-chase admission certificates emitted during that single pass.
///
/// # Errors
///
/// Returns the same source-loading, native-evaluation, and consistency-scanning
/// failures as [`reason_all`].
pub fn reason_all_certified(edb: &RdfDataset) -> gmeow_errors::Result<CertifiedReasoning> {
    let mut inferred = run_reasoning_rules(edb, dl::structured_dl_rules())?;
    let (chase_certificates, witness_derivations) =
        dl::augment_inferred_with_dl_certificates(&mut inferred, edb)?;
    inferred.sort();
    let verdict = dl::verdict_from_inferred(&inferred, edb)?;
    Ok(CertifiedReasoning {
        result: typed_result(inferred, &verdict),
        chase_certificates,
        witness_derivations,
    })
}

/// Run the native reasoning closure under a forward-chase step budget that CUTS the
/// semi-naive fixpoint MID-FLIGHT — the governed entry an agent-facing tool reasons
/// through so it never runs an unbudgeted Turing-complete closure over agent-influenced
/// input.
///
/// The `budget.max_steps` ceiling is threaded straight into the forward chase
/// ([`run_reasoning_rules_budgeted`] → [`crate::oracle::native_forward_eval_rules_with_frontier`]
/// → [`crate::physical::materialize_native`]), where the `StepGovernor`
/// charges one step per COMMITTED derivation at the deterministic FactKey-sorted commit
/// boundary and stops deriving new facts once the ceiling is reached.
///
/// * `budget.max_steps == None` (or a ceiling at/above the true closure size) is
///   **byte-identical to [`reason_all`]**: the governor never cuts, the full closure is
///   produced, the DL consistency post-pass runs, and the folded verdict is unchanged.
/// * A ceiling BELOW the true closure size returns the sound PARTIAL closure on a
///   non-conclusive [`EvaluationStatus::BudgetExhausted`] /
///   [`CompletenessStatus::Incomplete`] verdict whose [`InformationState`] is the honest
///   [`InformationState::Undetermined`] (the DL consistency verdict is NOT computed over a
///   truncated closure — a skipped derivation could have forced a clash — so it is never a
///   wrong `supported`/`both`). The consumed step count and declared allowance are recorded
///   on [`crate::result::BudgetUsage`].
///
/// `budget.max_answers` is not a chase-step concept and is ignored here (it is the backward
/// leg's answer cap); the forward governor bounds derivations, not answers.
///
/// # Errors
///
/// Returns the same source-loading / native-evaluation failures as [`reason_all`].
pub fn reason_all_budgeted(
    edb: &RdfDataset,
    budget: &Budget,
) -> gmeow_errors::Result<ReasoningResult> {
    let closure = run_reasoning_rules_budgeted(edb, dl::structured_dl_rules(), budget.max_steps)?;
    if closure.status == BudgetStatus::Ok {
        // Completed within budget: the fold is byte-identical to `reason_all`
        // (`augment_inferred_with_dl` is `augment_inferred_with_dl_certificates(..).map(|_|())`,
        // so the closure and folded verdict match the certified path exactly).
        let mut inferred = closure.inferred;
        dl::augment_inferred_with_dl(&mut inferred, edb)?;
        inferred.sort();
        let verdict = dl::verdict_from_inferred(&inferred, edb)?;
        Ok(typed_result(inferred, &verdict))
    } else {
        // Cut mid-chase: the DL post-pass is deliberately NOT run over the partial closure —
        // doing so would smuggle uncharged derivations past the governor. The partial closure
        // is a sound under-approximation carried on a non-conclusive budget-exhausted verdict.
        let mut inferred = closure.inferred;
        inferred.sort();
        Ok(budget_exhausted_result(
            inferred,
            PreservationClaim::exact(),
            budget.max_steps,
            closure.consumed_steps,
        ))
    }
}

/// Fold a PARTIAL forward closure — one the step governor cut mid-flight — into a
/// non-conclusive [`EvaluationStatus::BudgetExhausted`] [`ReasoningResult`].
///
/// The consistency verdict is intentionally left uncomputed: a truncated closure may have
/// skipped the very derivation that would force a clash, so claiming `supported`/`both`
/// would be unsound. The information state is therefore the honest
/// [`InformationState::Undetermined`] ("the engine has not reached a verdict"), the
/// completeness is [`CompletenessStatus::Incomplete`], and the consumed step count plus the
/// declared allowance are recorded on the provenance budget (a real measurement, never a
/// post-hoc fiction).
fn budget_exhausted_result(
    inferred: Vec<InferredAxiom>,
    preservation: PreservationClaim,
    allowance: Option<u64>,
    consumed_steps: u64,
) -> ReasoningResult {
    let mut provenance = ResultProvenance::native(native_contract_hash(), "");
    provenance.consumed_budget = BudgetUsage {
        consumed: consumed_steps,
        allowance,
        limit: Some(BudgetLimit::Inference),
    };
    provenance.projection_class = preservation.clone();
    ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::BudgetExhausted,
        CompletenessStatus::Incomplete,
        preservation,
        InformationState::Undetermined,
        provenance,
        ResultPayload::Inferred(inferred),
    )
}

/// Result of applying one ground conjecture candidate to a cached fixed-rule
/// reasoning state.
pub(crate) struct IncrementalReasoningResult {
    pub(crate) result: ReasoningResult,
    pub(crate) status: crate::seam::BudgetStatus,
    pub(crate) consumed_steps: u64,
}

/// Inputs to one fixed-calculus ground-fact incremental reasoning transaction.
pub(crate) struct GroundFactIncrementalRequest<'a> {
    pub(crate) base_edb: &'a RdfDataset,
    pub(crate) with_candidate_edb: &'a RdfDataset,
    pub(crate) base: &'a ReasoningResult,
    pub(crate) scenario_world: &'a str,
    pub(crate) subject: &'a str,
    pub(crate) predicate: &'a str,
    pub(crate) object: &'a RdfTerm,
    pub(crate) max_steps: Option<u64>,
}

/// Incrementally reason over `base_edb + candidate` for one scenario world.
///
/// The stable base result is reused byte-for-byte outside `scenario_world`. Inside
/// that world the fixed DL rule program is maintained by the signed nested-iteration
/// circuit, with newly derived facts carrying a real firing rule and immediate
/// premises. The finite DL post-pass then runs over the sound adjusted closure.
///
/// The fixed rule text only inspects resource-valued class/property axioms, but the
/// physical fact carrier is fully typed. A ground literal candidate is therefore kept
/// as an opaque asserted object in the signed session: it can make redundancy false and
/// can feed the literal-aware finite DL post-pass without fabricating a rule firing.
pub(crate) fn reason_ground_fact_insert_incremental(
    request: GroundFactIncrementalRequest<'_>,
) -> gmeow_errors::Result<IncrementalReasoningResult> {
    let GroundFactIncrementalRequest {
        base_edb,
        with_candidate_edb,
        base,
        scenario_world,
        subject,
        predicate,
        object,
        max_steps,
    } = request;
    let typed_edb = build_edb_facts(base_edb)?;
    let interner = typed_edb.interner();
    let mut world_edb = Vec::new();
    for typed in typed_edb.facts() {
        if typed.args.len() != 3 {
            return Err(reason_err(format!(
                "incremental fixed-calculus EDB row has arity {} (expected 3)",
                typed.args.len()
            )));
        }
        if world_string(interner.resolve(typed.args[2]))? != scenario_world {
            continue;
        }
        world_edb.push(crate::rule_ir::Fact {
            subject: interner.resolve(typed.args[0]).clone(),
            predicate: typed.predicate.clone(),
            object: interner.resolve(typed.args[1]).clone(),
        });
    }

    let candidate_object = ground_object_value(object)?;
    let candidate = crate::rule_ir::Fact {
        subject: TermValue::iri(subject.to_owned()),
        predicate: predicate.to_owned(),
        object: candidate_object,
    };
    let candidate_key = candidate.key();
    let candidate_axiom_key = (
        subject.to_owned(),
        predicate.to_owned(),
        crate::provenance::term_display(&candidate.object),
        scenario_world.to_owned(),
    );
    if world_edb.iter().any(|fact| fact.key() == candidate_key)
        || dataset_contains_ground_fact(
            base_edb,
            scenario_world,
            subject,
            predicate,
            &candidate.object,
        )
    {
        return Ok(IncrementalReasoningResult {
            result: base.clone(),
            status: crate::seam::BudgetStatus::Ok,
            consumed_steps: 0,
        });
    }

    let rules = dl::structured_dl_rules();
    let mut session =
        crate::physical::IncrementalSession::new(native_contract_hash(), world_edb, &rules)?;
    let adjusted = session.apply_insert_budgeted(
        [crate::physical::SignedFact {
            fact: candidate,
            weight: 1,
        }],
        max_steps,
    )?;

    type AxiomKey = (String, String, String, String);
    let base_by_key: std::collections::BTreeMap<AxiomKey, InferredAxiom> = base
        .inferred()
        .iter()
        .map(|axiom| {
            (
                (
                    axiom.subject.clone(),
                    axiom.predicate.clone(),
                    axiom.object.clone(),
                    axiom.world.clone(),
                ),
                axiom.clone(),
            )
        })
        .collect();

    // Worlds untouched by the candidate retain their existing axioms/provenance.
    let mut inferred: Vec<InferredAxiom> = base
        .inferred()
        .iter()
        .filter(|axiom| axiom.world != scenario_world)
        .cloned()
        .collect();
    for fact in adjusted.closure {
        let subject = subject_iri(&fact.subject)?;
        let object = crate::provenance::term_display(&fact.object);
        let key = (
            subject.clone(),
            fact.predicate.clone(),
            object.clone(),
            scenario_world.to_owned(),
        );
        let fact_key = fact.key();
        if fact_key == candidate_key {
            inferred.push(InferredAxiom {
                subject,
                predicate: fact.predicate,
                object,
                world: scenario_world.to_owned(),
                is_edb: true,
                rule_name: None,
                premises: Vec::new(),
            });
            continue;
        }
        if let Some(base_axiom) = base_by_key.get(&key) {
            inferred.push(base_axiom.clone());
            continue;
        }

        let witness = adjusted.delta.derivations.get(&fact_key).ok_or_else(|| {
            reason_err(format!(
                "incremental reasoning produced new derived fact {fact_key:?} without a firing witness"
            ))
        })?;
        let premises = witness
            .premises
            .iter()
            .map(|premise| {
                Ok((
                    subject_iri(&premise.subject)?,
                    premise.predicate.clone(),
                    crate::provenance::term_display(&premise.object),
                ))
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        inferred.push(InferredAxiom {
            subject,
            predicate: fact.predicate,
            object,
            world: scenario_world.to_owned(),
            is_edb: false,
            rule_name: Some(witness.rule_iri.clone()),
            premises,
        });
    }

    // The DL-only post-pass is monotone, so every base consequence remains valid and
    // is free cached state. Only run the post-pass for NEW consequences when the
    // governed recursive transaction reached its natural fixed point; doing so after a
    // cut would smuggle uncharged derivations into the partial closure.
    inferred.extend(
        base.inferred()
            .iter()
            .filter(|axiom| axiom.world == scenario_world)
            .cloned(),
    );
    if adjusted.status == crate::seam::BudgetStatus::Ok {
        dl::augment_inferred_with_dl(&mut inferred, with_candidate_edb)?;
    }
    inferred.retain(|axiom| {
        let key = (
            axiom.subject.clone(),
            axiom.predicate.clone(),
            axiom.object.clone(),
            axiom.world.clone(),
        );
        key != candidate_axiom_key || axiom.is_edb
    });
    for axiom in &mut inferred {
        axiom.premises.sort();
        axiom.premises.dedup();
    }
    inferred.sort();
    inferred.dedup();
    let verdict = dl::verdict_from_inferred(&inferred, with_candidate_edb)?;
    Ok(IncrementalReasoningResult {
        result: typed_result(inferred, &verdict),
        status: adjusted.status,
        consumed_steps: adjusted.consumed_steps,
    })
}

/// Convert the ground candidate's RDF object into the physical engine's typed term.
fn ground_object_value(object: &RdfTerm) -> gmeow_errors::Result<TermValue> {
    match object {
        RdfTerm::Iri(iri) => Ok(TermValue::iri(iri.clone())),
        RdfTerm::Literal(literal) => Ok(match literal.language.as_deref() {
            Some(language) => TermValue::lang_literal(literal.lexical_form.clone(), language),
            None => match literal.datatype.as_deref() {
                Some(datatype) => {
                    TermValue::typed_literal(literal.lexical_form.clone(), datatype.to_owned())
                }
                None => TermValue::simple_literal(literal.lexical_form.clone()),
            },
        }),
        other => Err(reason_err(format!(
            "incremental ground candidate object must be an IRI or literal, got {other:?}"
        ))),
    }
}

/// Whether the exact ground candidate is already asserted in its scenario world.
///
/// [`build_edb_facts`] intentionally omits literal objects from the fixed-calculus
/// input, so physical-session membership alone cannot recognize an already-asserted
/// literal. The source dataset remains the authority for that zero-delta case.
fn dataset_contains_ground_fact(
    edb: &RdfDataset,
    world: &str,
    subject: &str,
    predicate: &str,
    object: &TermValue,
) -> bool {
    edb.owned_quads().any(|quad| {
        matches!(&quad.subject, RdfTerm::Iri(iri) if iri == subject)
            && quad.predicate == predicate
            && ground_object_value(&quad.object)
                .ok()
                .is_some_and(|value| &value == object)
            && matches!(&quad.graph_name, Some(RdfTerm::Iri(iri)) if iri == world)
    })
}

/// Reason over a canonical [`gmeow_logic_compile::ir::LogicProgram`]'s rules AND full-FOL formulas against `edb`,
/// returning the shared typed [`ReasoningResult`].
///
/// This is the program-carrying entry the full-FOL formula layer flows through to actual
/// evaluation. The pipeline is `Formula → relational-core lowering → EvalRule →
/// native chase`, run alongside the program's own Horn rules and the fixed DL
/// calculus in one chase over `edb`:
///
/// 1. `relational_core::lower_formulas` legalizes each formula: the Horn-expressible
///    fragment becomes evaluable rules; everything beyond it (disjunctive heads,
///    `∃`-functions, sequence markers, …) is carried as flagged residue.
/// 2. The evaluable typed rules join the program rules directly in the same
///    native chase without a textual projection.
/// 3. The result's preservation claim UNIONS the lowering residue with the DL coverage gap
///    ([`ReasoningResult::from_dl_verdict_with_preservation`]): a non-evaluable formula is
///    disclosed (`{sound-under}` + `unsupported_constructs`), never silently absent.
///
/// The program's ground facts (axioms), if any, are expected in `edb` — the data graph is
/// the fact source, the program is the rule/formula source (the conformance-harness split).
///
/// # Errors
///
/// Returns `Err` if rule lowering fails (for example, a head variable is unbound
/// by every body atom), or if native evaluation fails.
pub fn reason_program(
    program: &gmeow_logic_compile::ir::LogicProgram,
    edb: &RdfDataset,
) -> gmeow_errors::Result<ReasoningResult> {
    // The unbudgeted program path is the `max_steps == None` case of the governed variant:
    // the forward chase runs to full fixpoint (`BudgetStatus::Ok`), so the returned result is
    // byte-identical to evaluating the program without any governor.
    Ok(reason_program_budgeted(program, edb, None)?.0)
}

/// Reason over `program` against `edb` under a forward-chase step budget, returning the
/// shared [`ReasoningResult`] together with the [`BudgetStatus`] and the committed step
/// count the governor observed.
///
/// This is the program-carrying analogue of [`reason_all_budgeted`]: the `max_steps`
/// ceiling is threaded into the same forward semi-naive governor, so a candidate program a
/// governed caller (e.g. [`crate::conjecture::conjecture_test`]) evaluates over
/// agent-influenced input is genuinely chase-bounded, not relabeled after a full run.
///
/// * `max_steps == None` (or a ceiling at/above the true closure) is byte-identical to the
///   ungoverned evaluation: the forward chase, the n-ary head chase, and the DL post-pass
///   all run, and the folded verdict is unchanged (`BudgetStatus::Ok`).
/// * A ceiling BELOW the true closure size cuts the forward chase mid-flight and returns the
///   sound PARTIAL closure on a non-conclusive [`EvaluationStatus::BudgetExhausted`] verdict.
///   The n-ary head chase and the DL post-pass are SKIPPED on a cut — running either over a
///   truncated closure would smuggle uncharged derivations past the governor — and any
///   formula-lowering residue is still disclosed in the preservation claim.
///
/// # Errors
///
/// Returns the same rule-lowering / native-evaluation failures as [`reason_program`].
pub(crate) fn reason_program_budgeted(
    program: &gmeow_logic_compile::ir::LogicProgram,
    edb: &RdfDataset,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<(ReasoningResult, BudgetStatus, u64)> {
    let lowering = crate::relational_core::lower_formulas(program);
    let formula_preservation = lowering.preservation.clone();
    let mut rules = dl::structured_dl_rules();
    rules.extend(crate::lower::lower_eval_rules(program)?);
    rules.extend(lowering.rules);
    let closure = run_reasoning_rules_budgeted(edb, rules, max_steps)?;

    if closure.status != BudgetStatus::Ok {
        // Cut mid-chase: carry the sound partial closure on a non-conclusive
        // budget-exhausted verdict, disclosing the formula-lowering residue. Neither the
        // n-ary head chase nor the DL post-pass runs over a truncated closure.
        let mut inferred = closure.inferred;
        inferred.sort();
        let result = budget_exhausted_result(
            inferred,
            formula_preservation,
            max_steps,
            closure.consumed_steps,
        );
        return Ok((result, closure.status, closure.consumed_steps));
    }

    let mut inferred = closure.inferred;

    // 3b. n-ary HEAD-derivation rules (`Rel(a₀..aₙ)` in a rule head) invent a shared reifier
    //     null per firing. They are evaluated through the native restricted chase,
    //     which mints the reified tuple by content identity, and the derived reified
    //     triples are folded into the same closure.
    if !lowering.nary_head_rules.is_empty() {
        inferred.extend(run_nary_head_chase(&lowering.nary_head_rules, edb)?);
    }

    dl::augment_inferred_with_dl(&mut inferred, edb)?;
    inferred.sort();
    let verdict = dl::verdict_from_inferred(&inferred, edb)?;

    // 4. Fold into the shared result, unioning the formula-lowering residue into the
    //    preservation claim.
    let provenance = ResultProvenance::native(native_contract_hash(), "");
    let result = ReasoningResult::from_dl_verdict_with_preservation(
        inferred,
        &verdict,
        &formula_preservation,
        provenance,
    );
    Ok((result, BudgetStatus::Ok, closure.consumed_steps))
}

/// Reason over `program` against `edb` and project the resulting closure (asserted +
/// derived axioms) back into a frozen [`RdfDataset`] a SPARQL consumer can query.
///
/// This is the closure→RDF bridge the native competency-question lane (`crates/slicetest`)
/// runs over: it evaluates [`reason_program`] and re-materializes every [`InferredAxiom`] as
/// a quad in its world graph, so a query sees the FULL entailment closure (the reified n-ary
/// tuples included), not just the asserted data. Both asserted (`is_edb`) and derived axioms
/// are emitted so a query over the closure sees the complete graph.
///
/// The per-axiom `subject`/`predicate` are IRIs; the `object` is the `term_display` surface
/// (`<iri>`, `_:blank`, or a literal) re-parsed via [`crate::rule_ir::surface_to_value`]. The
/// `world` string becomes the quad's graph name when it is an absolute IRI (a bodyless-rule
/// `"default"` world lands in the default graph).
///
/// # Errors
///
/// Returns `Err` if [`reason_program`] fails, if an object surface cannot be
/// re-parsed, or if the projected dataset fails the freeze-time structural contract.
/// Whether `value` is an absolute IRI (carries a `scheme:` prefix per RFC 3986). Used to
/// decide whether a reasoned axiom's non-default `world` is a genuine named graph. A robust
/// scheme check — NOT `contains("://")`, which silently misses schemeless-authority worlds
/// (`urn:`, `did:`, `tag:`, `mailto:`) and would demote them to the default graph (a
/// world-scoping / information-loss defect).
fn is_absolute_iri(value: &str) -> bool {
    match value.find(':') {
        Some(0) => false,
        Some(idx) => {
            let scheme = &value[..idx];
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        }
        None => false,
    }
}

pub fn reason_program_closure_dataset(
    program: &gmeow_logic_compile::ir::LogicProgram,
    edb: &RdfDataset,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    let result = reason_program(program, edb)?;
    let mut builder = RdfDatasetBuilder::new();
    for ax in result.inferred() {
        let subject = RdfTerm::iri(ax.subject.clone());
        let object = term_value_to_rdf_term(&crate::rule_ir::surface_to_value(&ax.object)?)?;
        let mut quad = RdfQuad::new(subject, ax.predicate.clone(), object);
        // The world travels as a plain string. The reasoner's default-world sentinel
        // ([`rl::DEFAULT_WORLD`], where un-named / default-graph EDB is reasoned) projects
        // back to the RDF DEFAULT graph, so a graph-clause-free competency query over the
        // closure sees it. A genuinely NAMED world (an absolute-IRI graph other than the
        // sentinel) is preserved as a named graph.
        if ax.world != rl::DEFAULT_WORLD && is_absolute_iri(&ax.world) {
            quad = quad.in_graph(RdfTerm::iri(ax.world.clone()));
        }
        builder.push_owned_quad(&quad);
    }
    builder.freeze().map_err(|e| reason_err(e.to_string()))
}

/// Re-materialize a native [`TermValue`] (as produced by
/// [`crate::rule_ir::surface_to_value`]) into an owned [`RdfTerm`].
///
/// `surface_to_value` only ever yields an IRI, a blank node, or a literal, so a triple
/// term is a hard error rather than a silent drop (the no-optionality discipline).
pub(crate) fn term_value_to_rdf_term(value: &TermValue) -> gmeow_errors::Result<RdfTerm> {
    Ok(match value {
        TermValue::Iri(iri) => RdfTerm::iri(iri.clone()),
        TermValue::Blank { label, .. } => RdfTerm::blank_node(label.clone()),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } => {
            let literal = match language {
                Some(lang) => RdfLiteral::language_tagged(lexical_form.clone(), lang.clone()),
                None => RdfLiteral::typed(lexical_form.clone(), datatype.clone()),
            };
            RdfTerm::literal(literal)
        }
        TermValue::Triple { s, p, o } => {
            let predicate = match p.as_ref() {
                TermValue::Iri(iri) => iri.clone(),
                other => {
                    return Err(reason_err(format!(
                        "closure→RDF: triple-term predicate must be an IRI, got {other:?}"
                    )));
                }
            };
            RdfTerm::triple(RdfTriple::new(
                term_value_to_rdf_term(s)?,
                predicate,
                term_value_to_rdf_term(o)?,
            ))
        }
    })
}

/// Evaluate n-ary conjunctive-head existential rules through the native restricted
/// chase over `edb`, returning the DERIVED reified tuples as
/// [`InferredAxiom`]s to fold into the reasoning closure.
///
/// The chase mints each reified tuple by content identity
/// ([`crate::provenance::mint_nary_reifier`]). Only chase-DERIVED rows are returned;
/// the asserted-EDB echo is dropped to avoid duplication.
///
/// # Errors
///
/// Returns `Err` if the store fails to load `edb`, or if the chase declines the
/// program (an uncertified, non-terminating existential set) — a first-class
/// declared gap, never a silent drop.
fn run_nary_head_chase(
    rules: &[crate::physical::ExistentialRule],
    edb: &RdfDataset,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    let store = WorldStore::new();
    store.load_dataset(edb)?;
    let (_admission, outcome) = crate::physical::chase_materialize(&store, rules, None)?;
    let budgeted = match outcome {
        crate::physical::NativeOutcome::Decided(budgeted) => budgeted,
        crate::physical::NativeOutcome::Unsupported(kind) => {
            return Err(reason_err(format!(
                "n-ary head derivation: the native restricted chase declined the program \
                 ({kind:?}) — an uncertified (non-terminating) existential rule set the reasoner \
                 cannot materialize"
            )));
        }
    };

    let mut out: Vec<InferredAxiom> = Vec::new();
    for row in budgeted.rows {
        // Drop the asserted-EDB echo (rule_iri == logic:assert); it is already in
        // the closure. Keep only the chase-derived reified tuples.
        if row.rule_iri == crate::provenance::ASSERT_RULE_IRI {
            continue;
        }
        out.push(InferredAxiom {
            subject: subject_iri(&row.subject)?,
            predicate: row.predicate,
            object: crate::provenance::term_display(&row.object),
            world: row.graph,
            is_edb: false,
            rule_name: Some(row.rule_iri),
            premises: Vec::new(),
        });
    }
    Ok(out)
}

/// Reason over a user-supplied data graph MERGED with the bundle's axioms, returning
/// the same shared typed [`ReasoningResult`] as [`reason_all`].
///
/// The merge is the cross-dataset re-intern
/// ([`RdfDatasetBuilder::push_dataset`](purrdf::RdfDatasetBuilder::push_dataset)),
/// so it carries the FULL RDF 1.2 statement layer of both inputs — the user's
/// reifier bindings and annotations are not dropped. The chase then runs over the
/// single merged dataset, so an inconsistency entailed only by the user's data
/// against the bundled TBox surfaces as `information=both` with its contradiction
/// witnesses, exactly as a same-graph inconsistency would.
///
/// # Errors
///
/// Returns `Err` if the merged dataset fails the freeze-time structural
/// contract, or if the chase fails to parse/validate/evaluate/decode.
pub fn reason_all_with_data(
    bundle: &RdfDataset,
    user: &RdfDataset,
) -> gmeow_errors::Result<ReasoningResult> {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(bundle);
    builder.push_dataset(user);
    let merged = builder.freeze().map_err(|e| reason_err(e.to_string()))?;
    reason_all(&merged)
}

/// Fold a `(closure, DlVerdict)` pair into the typed [`ReasoningResult`] under the
/// native reasoning contract. Shared by [`reason_all`] and the PyO3 boundary so
/// the typed result and the historical DL dict are projected from one fold.
///
/// The native consistency run spans every world in the bundle; the per-axiom
/// worlds are carried on the closure payload, so the result-level context world
/// is left unset (the aggregate run is not pinned to one world).
pub(crate) fn typed_result(
    inferred: Vec<InferredAxiom>,
    verdict: &dl::DlVerdict,
) -> ReasoningResult {
    let provenance = ResultProvenance::native(native_contract_hash(), "");
    ReasoningResult::from_dl_verdict(inferred, verdict, provenance)
}

/// The bare IRI string of a typed subject term.
///
/// A world-scoped reasoning fact never carries a literal (or triple-term)
/// subject — blanks were Skolemized to IRIs before the chase — so any other
/// shape is a hard error.
fn subject_iri(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Iri(iri) => Ok(iri.clone()),
        other => Err(reason_err(format!(
            "reasoning row subject must be an IRI (or Skolem IRI) term, got {other:?}"
        ))),
    }
}

/// The raw world string of a typed world term.
///
/// The world position of a ternary reasoning fact is always a plain string
/// literal; any other shape is a hard error.
fn world_string(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            ..
        } if datatype == "http://www.w3.org/2001/XMLSchema#string" => Ok(lexical_form.clone()),
        other => Err(reason_err(format!(
            "reasoning row world must be a plain string literal, got {other:?}"
        ))),
    }
}

/// Decode one typed antecedent row into a `(subject, predicate, object)` triple.
///
/// The antecedent rows are the same ternary shape as derived rows: subject is
/// an IRI term, object is any typed term (surfaced as its display string), and
/// the third value is the world string constant (dropped here — premises carry
/// only the triple shape).
fn decode_premise(row: &TypedRow) -> gmeow_errors::Result<(String, String, String)> {
    if row.args.len() != 3 {
        return Err(reason_err(format!(
            "antecedent row has arity {} (expected 3): {row:?}",
            row.args.len()
        )));
    }
    let subject = subject_iri(&row.args[0])?;
    let object = crate::provenance::term_display(&row.args[1]);
    Ok((subject, row.predicate.clone(), object))
}

// Structured rules are the sole forward reasoning input.
pub(crate) fn run_reasoning_rules(
    edb: &RdfDataset,
    rules: Vec<crate::rule_ir::EvalRule>,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    // The ungoverned closure is the `max_steps == None` case: the forward chase runs to full
    // fixpoint (`BudgetStatus::Ok`), so the returned axioms are byte-identical to the
    // pre-governor engine and no existing caller/golden is disturbed.
    Ok(run_reasoning_rules_budgeted(edb, rules, None)?.inferred)
}

/// The forward reasoning closure together with the step governor's cut status.
///
/// Produced by [`run_reasoning_rules_budgeted`]; `status == BudgetStatus::Ok` iff the
/// semi-naive fixpoint reached its natural end within `max_steps`, otherwise
/// `BudgetStatus::Exhausted` and `inferred` is the sound (FactKey-ordered) PARTIAL closure at
/// the deterministic cut. `consumed_steps` is the number of committed derivations (a
/// deterministic count: identical input + identical `max_steps` ⇒ identical count).
pub(crate) struct BudgetedClosure {
    /// The asserted + derived closure (full on `Ok`, partial on `Exhausted`).
    pub(crate) inferred: Vec<InferredAxiom>,
    /// Whether the forward chase ran to fixpoint or was cut by the step budget.
    pub(crate) status: BudgetStatus,
    /// Committed derivations at the point the chase stopped.
    pub(crate) consumed_steps: u64,
}

/// Run the forward reasoning chase under a step budget that CUTS the semi-naive fixpoint
/// mid-flight.
///
/// `max_steps` is threaded straight into
/// [`crate::oracle::native_forward_eval_rules_with_frontier`] →
/// [`crate::physical::materialize_native`], where the [`crate::physical::StepGovernor`]
/// charges one step per committed derivation and stops before committing the derivation that
/// would exceed the ceiling. `None` is the unbudgeted path (byte-identical to the
/// pre-governor engine); `Some(n)` admits exactly `n` committed derivations and returns the
/// sound partial closure on `BudgetStatus::Exhausted`.
///
/// # Errors
///
/// Returns `Err` if the source store cannot be loaded or native evaluation fails.
pub(crate) fn run_reasoning_rules_budgeted(
    edb: &RdfDataset,
    rules: Vec<crate::rule_ir::EvalRule>,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<BudgetedClosure> {
    let edb_facts = build_edb_facts(edb)?;
    let (chase, frontier, status) =
        crate::oracle::native_forward_eval_rules_with_frontier(&edb_facts, rules, max_steps)?;
    let inferred = chase_rows_to_inferred(&chase)?;
    Ok(BudgetedClosure {
        inferred,
        status,
        consumed_steps: frontier.consumed_steps,
    })
}

/// Build the typed EDB ([`TypedFactSet`]) for `edb` — the single native
/// fact-set construction the whole reasoning path shares.
///
/// Walks the frozen dataset directly and pushes every IRI-object quad of every
/// named-IRI world into the typed EDB. The IRI-object filter is a SEMANTIC EL/DL
/// restriction: the fixed calculi only fire on axioms whose object is an IRI
/// (subClassOf, type, disjointWith, equivalentClass, subPropertyOf), so a
/// literal-object quad (an annotation such as rdfs:comment / dc:creator) can never
/// participate in any rule, and skipping them is sound for the closure AND the
/// verdict. It is no longer a transport necessity: the typed adapter carries
/// literal objects — control characters included — losslessly through the chase.
///
/// This deliberately does not first copy the entire immutable `RdfDataset` into a
/// mutable `WorldStore` and then query every world back out. The frozen IR already
/// carries the same graph/term information; iterating it once avoids a redundant
/// full-dataset intern/index pass on every reasoning call while preserving the
/// world semantics exactly (default and blank-node graph names remain inaccessible
/// to the named-world calculus, as they are through `WorldStore::worlds`).
///
/// Factored out so benchmark seams drive the exact same fact set as the production
/// reasoning path.
///
/// # Errors
///
/// Returns `Err` if the source store cannot be loaded.
pub(crate) fn build_edb_facts(edb: &RdfDataset) -> gmeow_errors::Result<TypedFactSet> {
    let mut edb_facts = TypedFactSet::new();
    for quad in edb.quads() {
        let Some(graph) = quad.g else { continue };
        let TermRef::Iri(world) = edb.resolve(graph) else {
            continue;
        };
        let TermRef::Iri(predicate) = edb.resolve(quad.p) else {
            continue;
        };
        if !matches!(edb.resolve(quad.o), TermRef::Iri(_)) {
            continue;
        }

        // Resolve only the two fact arguments that survive the semantic filter.
        // Blank subjects/objects are Skolemized inside `push_quad`; the world
        // travels as a plain string literal exactly as before.
        let subject = edb.term_value(quad.s);
        let object = edb.term_value(quad.o);
        edb_facts.push_quad(&subject, predicate, &object, world);
    }
    Ok(edb_facts)
}

/// Coerce a typed chase result into the `Vec<InferredAxiom>` closure the DL/EL
/// post-passes and result folds consume.
///
/// Every reasoning fact is the ternary `predicate(subject, object, world)`. The
/// typed rule sets the reasoning chase runs are repo-owned and declare
/// ONLY ternary relations, so a non-ternary row indicates a rule-text bug and is
/// a hard error. (This differs from `materialize`'s explicit non-quad bucket:
/// there the rule text is caller-supplied and may legitimately declare helper
/// predicates of other arities.)
///
/// Kept as a separate fold so native evaluator and benchmark callers share the
/// same provenance-aware conversion into the public closure.
///
/// # Errors
///
/// Returns `Err` if a materialized row is not the ternary reasoning
/// shape or if a subject/world/premise term cannot be decoded.
pub(crate) fn chase_rows_to_inferred(
    chase: &crate::oracle::TypedChaseResult,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    let mut inferred: Vec<InferredAxiom> = Vec::new();
    for (row, prov) in &chase.rows {
        if row.args.len() != 3 {
            return Err(reason_err(format!(
                "reasoning chase produced a non-ternary row for predicate \
                 {:?} (arity {}): the fixed reasoning rule texts declare only \
                 ternary relations, so this is a rule-text bug",
                row.predicate,
                row.args.len()
            )));
        }

        let predicate = row.predicate.clone();
        let subject = subject_iri(&row.args[0])?;
        let object = crate::provenance::term_display(&row.args[1]);
        let world = world_string(&row.args[2])?;

        let mut premises = prov
            .antecedents
            .iter()
            .map(decode_premise)
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        premises.sort();

        inferred.push(InferredAxiom {
            subject,
            predicate,
            object,
            world,
            is_edb: prov.is_edb,
            rule_name: prov.rule_name.clone(),
            premises,
        });
    }

    Ok(inferred)
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const W: &str = "http://gmeow.example/w";
    const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";

    const A: &str = "http://gmeow.example/A";
    const B: &str = "http://gmeow.example/B";
    const C: &str = "http://gmeow.example/C";
    const X: &str = "http://gmeow.example/x";

    fn quad(s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W))
    }

    fn dataset(quads: Vec<RdfQuad>) -> std::sync::Arc<purrdf::RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in quads {
            builder.push_owned_quad(&quad);
        }
        builder.freeze().expect("valid test dataset")
    }

    fn quad_in(world: &str, s: &str, p: &str, o: &str) -> RdfQuad {
        RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o)).in_graph(RdfTerm::iri(world))
    }

    fn scratch_leave_one_out(edb: &RdfDataset, axiom: &LeaveOneOutAxiom) -> bool {
        let reduced = dataset_without_axiom(edb, axiom).expect("reduced dataset freezes");
        reason_closure_axioms(&reduced)
            .expect("scratch leave-one-out reasons")
            .iter()
            .any(|inferred| {
                inferred.subject == axiom.subject
                    && inferred.predicate == axiom.predicate
                    && inferred
                        .object
                        .trim_start_matches('<')
                        .trim_end_matches('>')
                        == axiom.object
            })
    }

    fn fact_surfaces(facts: &TypedFactSet) -> Vec<(String, Vec<String>)> {
        let mut rows = facts
            .facts()
            .map(|fact| {
                (
                    fact.predicate.clone(),
                    fact.args
                        .iter()
                        .map(|&id| facts.interner().display_of(id).to_owned())
                        .collect(),
                )
            })
            .collect::<Vec<_>>();
        rows.sort();
        rows
    }

    #[test]
    fn direct_edb_fold_is_fact_identical_to_the_world_store_adapter() {
        let p = "http://gmeow.example/p";
        let w2 = "urn:gmeow:test:world-2";
        let mut builder = RdfDatasetBuilder::new();
        for quad in [
            quad(A, p, B),
            RdfQuad::new(RdfTerm::blank_node("subject"), p, RdfTerm::iri(C))
                .in_graph(RdfTerm::iri(w2)),
            // Literal objects are outside the fixed EL/DL relation fragment.
            RdfQuad::new(
                RdfTerm::iri(A),
                p,
                RdfTerm::literal(RdfLiteral::simple("annotation")),
            )
            .in_graph(RdfTerm::iri(W)),
            // Default and blank-node graph names are not named-IRI worlds.
            RdfQuad::new(RdfTerm::iri(A), p, RdfTerm::iri(C)),
            RdfQuad::new(RdfTerm::iri(B), p, RdfTerm::iri(C))
                .in_graph(RdfTerm::blank_node("graph")),
        ] {
            builder.push_owned_quad(&quad);
        }
        let dataset = builder.freeze().expect("mixed-world fixture freezes");

        let direct = build_edb_facts(dataset.as_ref()).expect("direct frozen-IR fold");

        // The retired production shape is retained here as a semantic oracle: copy
        // through WorldStore, enumerate its named worlds, and build the same typed
        // facts. The optimized one-pass adapter must change cost, never membership.
        let store = WorldStore::new();
        store
            .load_dataset(dataset.as_ref())
            .expect("world-store oracle load");
        let mut via_store = TypedFactSet::new();
        for world in store.worlds() {
            for quad in store.quads_for_pattern_in_world(&world, None, None, None) {
                if !quad.o.is_iri() {
                    continue;
                }
                let Some(predicate) = quad.p.as_iri() else {
                    continue;
                };
                via_store.push_quad(&quad.s, predicate, &quad.o, &world);
            }
        }

        assert_eq!(
            fact_surfaces(&direct),
            fact_surfaces(&via_store),
            "the direct frozen-IR fold preserves the exact named-world fact set"
        );
    }

    #[test]
    fn incremental_leave_one_out_matches_scratch_across_worlds_and_alternative_proofs() {
        const W2: &str = "urn:gmeow:test:leave-one-out-world-2";
        const D: &str = "http://gmeow.example/D";
        const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
        const P: &str = "http://gmeow.example/p";

        let store = dataset(vec![
            // World one gives A -> C two proofs: the direct assertion and A -> B -> C.
            quad(A, SUBCLASS, B),
            quad(B, SUBCLASS, C),
            quad(A, SUBCLASS, C),
            // The same A -> B assertion exists in another world but has an independent
            // alternate proof there. Leave-one-out removes BOTH asserted occurrences.
            quad_in(W2, A, SUBCLASS, B),
            quad_in(W2, A, SUBCLASS, D),
            quad_in(W2, D, SUBCLASS, B),
            // A predicate outside the fixed rule heads stays load-bearing.
            quad(P, DOMAIN, A),
        ]);
        let probes = vec![
            LeaveOneOutAxiom::new(A, SUBCLASS, C),
            LeaveOneOutAxiom::new(A, SUBCLASS, B),
            LeaveOneOutAxiom::new(B, SUBCLASS, C),
            LeaveOneOutAxiom::new(P, DOMAIN, A),
        ];

        let incremental =
            leave_one_out_rederived(&store, &probes).expect("incremental leave-one-out reasons");
        let scratch = probes
            .iter()
            .map(|probe| scratch_leave_one_out(&store, probe))
            .collect::<Vec<_>>();
        assert_eq!(incremental, scratch);
        assert_eq!(incremental, vec![true, true, false, false]);
    }

    #[test]
    fn incremental_leave_one_out_preserves_finite_dl_union_derivation() {
        const U: &str = "http://gmeow.example/U";
        const UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
        const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
        const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
        const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

        let list = RdfTerm::blank_node("union-list");
        let store = dataset(vec![
            quad(A, SUBCLASS, U),
            RdfQuad::new(RdfTerm::iri(U), UNION_OF, list.clone()).in_graph(RdfTerm::iri(W)),
            RdfQuad::new(list.clone(), RDF_FIRST, RdfTerm::iri(A)).in_graph(RdfTerm::iri(W)),
            RdfQuad::new(list, RDF_REST, RdfTerm::iri(RDF_NIL)).in_graph(RdfTerm::iri(W)),
        ]);
        let probe = LeaveOneOutAxiom::new(A, SUBCLASS, U);

        let incremental = leave_one_out_rederived(&store, std::slice::from_ref(&probe))
            .expect("incremental union leave-one-out reasons");
        assert_eq!(incremental, vec![scratch_leave_one_out(&store, &probe)]);
        assert_eq!(incremental, vec![true]);
    }

    #[test]
    fn batched_leave_one_out_matches_scratch_for_every_fast_tbox_family() {
        const SUBPROPERTY: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
        const DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
        const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
        const EQUIVALENT: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
        const INVERSE: &str = "http://www.w3.org/2002/07/owl#inverseOf";
        const COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#complementOf";
        const FUNCTIONAL: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
        const P: &str = "http://gmeow.example/p";
        const Q: &str = "http://gmeow.example/q";
        const R: &str = "http://gmeow.example/r";
        const MARKER: &str = "http://gmeow.example/FunctionalMarker";

        let store = dataset(vec![
            quad(P, SUBPROPERTY, R),
            quad(P, SUBPROPERTY, Q),
            quad(Q, SUBPROPERTY, R),
            quad(A, EQUIVALENT, B),
            quad(P, DOMAIN, A),
            quad(P, RANGE, B),
            quad(P, INVERSE, Q),
            quad(A, DISJOINT, C),
            quad(A, COMPLEMENT, C),
            quad(B, DISJOINT, C),
            quad(P, TYPE, FUNCTIONAL),
            quad(Q, TYPE, MARKER),
            quad(MARKER, SUBCLASS, FUNCTIONAL),
            quad(Q, TYPE, FUNCTIONAL),
        ]);
        let probes = vec![
            LeaveOneOutAxiom::new(P, SUBPROPERTY, R),
            LeaveOneOutAxiom::new(Q, SUBPROPERTY, R),
            LeaveOneOutAxiom::new(A, EQUIVALENT, B),
            LeaveOneOutAxiom::new(P, DOMAIN, A),
            LeaveOneOutAxiom::new(P, RANGE, B),
            LeaveOneOutAxiom::new(P, INVERSE, Q),
            LeaveOneOutAxiom::new(A, DISJOINT, C),
            LeaveOneOutAxiom::new(B, DISJOINT, C),
            LeaveOneOutAxiom::new(P, TYPE, FUNCTIONAL),
            LeaveOneOutAxiom::new(Q, TYPE, FUNCTIONAL),
        ];

        let batched = leave_one_out_rederived(&store, &probes).expect("batch reasons");
        let scratch = probes
            .iter()
            .map(|probe| scratch_leave_one_out(&store, probe))
            .collect::<Vec<_>>();
        assert_eq!(batched, scratch);
        assert_eq!(
            batched,
            vec![
                true, false, false, false, false, false, true, false, false, true
            ]
        );
    }

    #[test]
    fn native_contract_hash_frames_every_load_bearing_engine_component() {
        let names = NATIVE_CONTRACT_COMPONENTS
            .iter()
            .map(|(name, source)| {
                assert!(!source.is_empty(), "contract component {name} is empty");
                *name
            })
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "reason/el.rs",
                "reason/rl_rules.rs",
                "reason/dl.rs",
                "reason/mod.rs",
                "oracle.rs",
                "certify.rs",
                "lower.rs",
                "materialize.rs",
                "relational_core.rs",
                "stablemodel.rs",
                "wellfounded.rs",
                "rule_ir.rs",
                "physical/plan.rs",
                "physical/seminaive.rs",
                "physical/chase.rs",
                "physical/store.rs",
            ]
        );
        assert_eq!(native_contract_hash().len(), 40, "SHA-1 hex contract id");
    }

    #[test]
    fn is_absolute_iri_recognizes_schemeless_authority_worlds() {
        // http(s) worlds — the common case — stay named.
        assert!(is_absolute_iri(
            "https://blackcatinformatics.ca/gmeow/graph/w"
        ));
        assert!(is_absolute_iri("http://example.org/g"));
        // Schemeless-authority IRIs (no `://`) are ALSO absolute named worlds — the old
        // `contains("://")` check silently demoted these to the default graph.
        assert!(is_absolute_iri(
            "urn:uuid:2c8f0a1e-0000-4000-8000-000000000001"
        ));
        assert!(is_absolute_iri("did:example:123"));
        assert!(is_absolute_iri("tag:blackcat,2026:world"));
        assert!(is_absolute_iri("mailto:someone@example.org"));
        // A bare token / relative reference is NOT absolute.
        assert!(!is_absolute_iri("c14n44"));
        assert!(!is_absolute_iri("world-1"));
        assert!(!is_absolute_iri(":no-scheme"));
        assert!(!is_absolute_iri("1http://bad-scheme")); // scheme must start with a letter
    }

    /// Production-surface    /// Production-surface antecedent guard (gap G3): the primary reasoning path
    /// (`reason_all` → `reason_closure` → `run_reasoning` → `forward_oracle().materialize`
    /// → `chase_rows_to_inferred`) must carry REAL native premises end-to-end, not
    /// just non-empty inferred facts. `forward_oracle()` funnels the binary
    /// seminaive branch here; A⊑B, B⊑C derives the transitive A⊑C, whose
    /// `InferredAxiom::premises` must be NON-EMPTY (it cites its two body facts).
    /// Falsifiable: the escaped empty-antecedents bug leaves EVERY derived
    /// `premises` empty, tripping this at the production observable.
    #[test]
    fn reason_all_derived_axioms_carry_nonempty_premises() {
        let store = dataset(vec![quad(A, SUBCLASS, B), quad(B, SUBCLASS, C)]);
        let result = reason_all(store.as_ref()).expect("native reason_all must decide the closure");

        // The transitive subClassOf(A, C) is derived (is_edb == false) and must
        // cite its immediate antecedents through `InferredAxiom::premises`.
        // `subject`/`predicate` are bare IRIs; `object` is `term_display`ed (an IRI
        // renders angle-bracketed), so match the object against its display form.
        let object_c = format!("<{C}>");
        let derived_transitive = result.inferred().iter().find(|ax| {
            !ax.is_edb && ax.predicate == SUBCLASS && ax.subject == A && ax.object == object_c
        });
        let axiom = derived_transitive.unwrap_or_else(|| {
            panic!(
                "transitive subClassOf(A, C) must be derived; got {:?}",
                result.inferred()
            )
        });
        assert!(
            !axiom.premises.is_empty(),
            "derived subClassOf(A, C) must carry NON-EMPTY premises on the production path \
             (the empty-antecedents bug fails here); got {axiom:?}"
        );
    }

    #[test]
    fn reason_all_single_chase_yields_inconsistent_and_nonempty_closure() {
        // A ⊑ B, A ⊑ C, B disjointWith C, x : A — one chase must derive both the
        // subsumption closure AND the inconsistency verdict (x forced into Nothing).
        let store = dataset(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
            quad(X, TYPE, A),
        ]);
        let result = reason_all(store.as_ref()).expect("reason_all should succeed");

        assert!(
            !result.is_consistent(),
            "x forced into owl:Nothing must make the verdict inconsistent (information=both)"
        );
        assert_eq!(
            result.information,
            crate::result::InformationState::Both,
            "an inconsistent verdict is the four-valued Belnap glut"
        );
        assert!(
            !result.inferred().is_empty(),
            "the subsumption closure must be non-empty (asserted + derived axioms)"
        );
        assert!(
            result
                .provenance
                .contradiction_witnesses
                .iter()
                .any(|w| w.individual == X),
            "x must be a contradiction witness: {:?}",
            result.provenance.contradiction_witnesses
        );
    }

    #[test]
    fn reason_all_with_data_merges_user_abox_into_bundle_tbox() {
        // The contradiction is entailed only ACROSS the two inputs: the disjointness
        // TBox lives in `bundle`, the offending individual `x : A` in `user`. Neither
        // alone is inconsistent; the merge must feed both to the chase.
        let bundle = dataset(vec![
            quad(A, SUBCLASS, B),
            quad(A, SUBCLASS, C),
            quad(B, DISJOINT, C),
        ]);
        let user = dataset(vec![quad(X, TYPE, A)]);

        // The user ABox on its own (no TBox) is consistent.
        let user_only = reason_all(user.as_ref()).expect("reason_all over user-only");
        assert!(
            user_only.is_consistent(),
            "x : A with no disjointness axioms is consistent"
        );

        // Merged with the bundle TBox, x is forced into owl:Nothing.
        let merged = reason_all_with_data(bundle.as_ref(), user.as_ref())
            .expect("reason_all_with_data should succeed");
        assert!(
            !merged.is_consistent(),
            "user data merged with the bundle TBox entails an inconsistency"
        );
        assert!(
            merged
                .provenance
                .contradiction_witnesses
                .iter()
                .any(|w| w.individual == X),
            "x must be a contradiction witness in the merged run: {:?}",
            merged.provenance.contradiction_witnesses
        );
    }

    // ── Program-carrying reason: the full-FOL formula layer actually evaluates ──

    use gmeow_logic_compile::ir::{Formula, LogicProgram, PreservationKind, Term};

    const KNOWS: &str = "http://gmeow.example/knows";
    const TRUSTS: &str = "http://gmeow.example/trusts";
    const ALICE: &str = "http://gmeow.example/alice";
    const BOB: &str = "http://gmeow.example/bob";
    const SAM: &str = "http://gmeow.example/sam";

    fn fml_atom(rel: &str, args: Vec<Term>) -> Formula {
        Formula::atom(Term::iri(rel.to_owned()).unwrap(), args).unwrap()
    }

    #[test]
    fn reason_program_evaluates_a_horn_formula_end_to_end() {
        // ∀x. (knows(x, alice) → trusts(x, bob)) is Horn-expressible, so it must lower to a
        // rule that the chase fires: given knows(sam, alice), the program must DERIVE
        // trusts(sam, bob). This is the formula layer evaluating end-to-end (not dead code).
        let formula = Formula::Forall {
            vars: vec!["x".into()],
            body: Box::new(Formula::Implies(
                Box::new(fml_atom(
                    KNOWS,
                    vec![
                        Term::var("x").unwrap(),
                        Term::iri(ALICE.to_owned()).unwrap(),
                    ],
                )),
                Box::new(fml_atom(
                    TRUSTS,
                    vec![Term::var("x").unwrap(), Term::iri(BOB.to_owned()).unwrap()],
                )),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]);
        let edb = dataset(vec![quad(SAM, KNOWS, ALICE)]);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // Objects decode to their N3 surface (`<iri>`); subjects/predicates are bare IRIs.
        let bob_obj = format!("<{BOB}>");
        assert!(
            result
                .inferred()
                .iter()
                .any(|ax| { ax.subject == SAM && ax.predicate == TRUSTS && ax.object == bob_obj }),
            "the Horn formula must derive trusts(sam, bob); closure: {:?}",
            result
                .inferred()
                .iter()
                .map(|a| (&a.subject, &a.predicate, &a.object))
                .collect::<Vec<_>>()
        );
        // The Horn formula lowers exactly — it adds no formula residue to the claim.
        assert!(
            !result
                .preservation
                .unsupported_constructs
                .iter()
                .any(|c| c.contains("formula") || c.contains("disjunct")),
            "a fully-evaluable Horn formula adds no formula residue: {:?}",
            result.preservation.unsupported_constructs
        );
    }

    /// A law with TERNARY atoms in its BODY and a BINARY head (the associativity shape,
    /// like the algebra-axioms law) evaluates end-to-end: the reified n-ary body atoms
    /// join through the chase and the binary consequent is derived. This exercises the
    /// body-reification path (no head derivation) all the way through `reason_program`.
    #[test]
    fn reason_program_evaluates_an_nary_body_law_end_to_end() {
        // ∀a b c ab bc l r. op(a,b,ab) ∧ op(ab,c,l) ∧ op(b,c,bc) ∧ op(a,bc,r) → eq(l,r)
        // Seeded on a concrete associative table so both bracketings reach the SAME value v;
        // then eq(l,r) must be derived (l = v = r).
        const OP: &str = "http://gmeow.example/op";
        const EQ: &str = "http://gmeow.example/eq";
        let v = |n: &str| Term::var(n).unwrap();
        let law = Formula::Forall {
            vars: ["a", "b", "c", "ab", "bc", "l", "r"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(OP, vec![v("a"), v("b"), v("ab")]),
                    fml_atom(OP, vec![v("ab"), v("c"), v("l")]),
                    fml_atom(OP, vec![v("b"), v("c"), v("bc")]),
                    fml_atom(OP, vec![v("a"), v("bc"), v("r")]),
                ])),
                Box::new(fml_atom(EQ, vec![v("l"), v("r")])),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

        // A concrete op table where (a·b)·c and a·(b·c) both reach `v` for a=x,b=y,c=z.
        // op is ternary → the EDB op facts are authored PRE-REIFIED (instanceOf + naryArg).
        const X: &str = "http://gmeow.example/x";
        const Y: &str = "http://gmeow.example/y";
        const Z: &str = "http://gmeow.example/z";
        const XY: &str = "http://gmeow.example/xy";
        const YZ: &str = "http://gmeow.example/yz";
        const V: &str = "http://gmeow.example/v";
        let io = "https://blackcatinformatics.ca/logic/instanceOf";
        let a0 = "https://blackcatinformatics.ca/logic/naryArg0";
        let a1 = "https://blackcatinformatics.ca/logic/naryArg1";
        let a2 = "https://blackcatinformatics.ca/logic/naryArg2";
        // Reify one op(s,t,u) tuple as instanceOf + naryArg triples on a fresh node.
        let mut quads = Vec::new();
        let mut reify = |node: &str, s: &str, t: &str, u: &str| {
            quads.push(quad(node, io, OP));
            quads.push(quad(node, a0, s));
            quads.push(quad(node, a1, t));
            quads.push(quad(node, a2, u));
        };
        reify("http://gmeow.example/r_xy", X, Y, XY); // x·y = xy
        reify("http://gmeow.example/r_xyz1", XY, Z, V); // (x·y)·z = v
        reify("http://gmeow.example/r_yz", Y, Z, YZ); // y·z = yz
        reify("http://gmeow.example/r_xyz2", X, YZ, V); // x·(y·z) = v
        let edb = dataset(quads);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // The binary consequent eq(l, r) = eq(v, v) must be derived.
        let eq_vv = result
            .inferred()
            .iter()
            .any(|ax| ax.predicate == EQ && ax.subject == V && ax.object == format!("<{V}>"));
        assert!(
            eq_vv,
            "associativity must derive eq(v, v); closure: {:?}",
            result
                .inferred()
                .iter()
                .filter(|a| a.predicate == EQ)
                .map(|a| (&a.subject, &a.object))
                .collect::<Vec<_>>()
        );
        // A fully-evaluable n-ary body law lowers exactly (no residue).
        assert!(
            !result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "an n-ary body law lowers exactly: {:?}",
            result.preservation
        );
    }

    // ── n-ary HEAD derivation: the det homomorphism law evaluates end-to-end ──

    const MATMUL: &str = "http://gmeow.example/matMul";
    const MUL: &str = "http://gmeow.example/mul";
    const DET: &str = "http://gmeow.example/det";
    const MAT_A: &str = "http://gmeow.example/A";
    const MAT_B: &str = "http://gmeow.example/B";
    const MAT_AB: &str = "http://gmeow.example/AB";
    const DET_A: &str = "http://gmeow.example/dA";
    const DET_B: &str = "http://gmeow.example/dB";
    const DET_AB: &str = "http://gmeow.example/dAB";
    const MATMUL_REIFIER: &str = "http://gmeow.example/reif/matMul-A-B-AB";
    const LOGIC_INSTANCE_OF: &str = "https://blackcatinformatics.ca/logic/instanceOf";
    const NARY_REIFIER_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/reifier/nary/";

    fn logic_nary_arg(i: usize) -> String {
        format!("https://blackcatinformatics.ca/logic/naryArg{i}")
    }

    #[test]
    fn reason_program_derives_an_nary_head_tuple_end_to_end() {
        // The determinant homomorphism law:
        //   ∀A,B,AB,dA,dB,dAB. matMul(A,B,AB) ∧ det(A,dA) ∧ det(B,dB) ∧ det(AB,dAB) → mul(dA,dB,dAB)
        // `matMul` is ternary → reified BODY atom; `mul` is ternary → reified HEAD (a derived
        // tuple). Seed a minimal deterministic pre-reified EDB (the matMul tuple as reified
        // instanceOf+naryArg triples, plus the three det facts) and assert the closure DERIVES
        // the reified `mul(dA,dB,dAB)` tuple.
        let law = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(
                        MATMUL,
                        vec![
                            Term::var("A").unwrap(),
                            Term::var("B").unwrap(),
                            Term::var("AB").unwrap(),
                        ],
                    ),
                    fml_atom(DET, vec![Term::var("A").unwrap(), Term::var("dA").unwrap()]),
                    fml_atom(DET, vec![Term::var("B").unwrap(), Term::var("dB").unwrap()]),
                    fml_atom(
                        DET,
                        vec![Term::var("AB").unwrap(), Term::var("dAB").unwrap()],
                    ),
                ])),
                Box::new(fml_atom(
                    MUL,
                    vec![
                        Term::var("dA").unwrap(),
                        Term::var("dB").unwrap(),
                        Term::var("dAB").unwrap(),
                    ],
                )),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

        // Pre-reified EDB: matMul(A,B,AB) as instanceOf + naryArg triples, plus the det facts.
        let na = logic_nary_arg(0);
        let nb = logic_nary_arg(1);
        let nab = logic_nary_arg(2);
        let edb = dataset(vec![
            quad(MATMUL_REIFIER, LOGIC_INSTANCE_OF, MATMUL),
            quad(MATMUL_REIFIER, &na, MAT_A),
            quad(MATMUL_REIFIER, &nb, MAT_B),
            quad(MATMUL_REIFIER, &nab, MAT_AB),
            quad(MAT_A, DET, DET_A),
            quad(MAT_B, DET, DET_B),
            quad(MAT_AB, DET, DET_AB),
        ]);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // Find the derived reifier R by the typing atom instanceOf(R, mul).
        let mul_obj = format!("<{MUL}>");
        let r = result
            .inferred()
            .iter()
            .find(|ax| ax.predicate == LOGIC_INSTANCE_OF && ax.object == mul_obj)
            .map(|ax| ax.subject.clone())
            .unwrap_or_else(|| {
                panic!(
                    "the law must DERIVE instanceOf(R, mul); closure: {:?}",
                    result
                        .inferred()
                        .iter()
                        .map(|a| (&a.subject, &a.predicate, &a.object))
                        .collect::<Vec<_>>()
                )
            });

        // The reifier is minted by TUPLE IDENTITY (mint_nary_reifier), not a frontier Skolem.
        assert!(
            r.starts_with(NARY_REIFIER_PREFIX),
            "R must be the content-addressed n-ary reifier IRI, got: {r}"
        );

        // Join on R: the three positional argument atoms carry the concrete det values.
        let has_arg = |i: usize, value: &str| {
            let pred = logic_nary_arg(i);
            let obj = format!("<{value}>");
            result
                .inferred()
                .iter()
                .any(|ax| ax.subject == r && ax.predicate == pred && ax.object == obj)
        };
        assert!(has_arg(0, DET_A), "naryArg0(R, dA) must be derived");
        assert!(has_arg(1, DET_B), "naryArg1(R, dB) must be derived");
        assert!(has_arg(2, DET_AB), "naryArg2(R, dAB) must be derived");

        // The law lowers exactly — no formula residue, preservation stays Exact.
        assert!(
            !result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "a range-restricted n-ary head lowers exactly (no SoundUnder): {:?}",
            result.preservation
        );
        assert!(
            !result
                .preservation
                .unsupported_constructs
                .iter()
                .any(|c| c.contains("formula") || c.contains("not bound") || c.contains("nary")),
            "no n-ary head residue disclosed: {:?}",
            result.preservation.unsupported_constructs
        );
    }

    /// The E8 group-action law `(g·h)·x = g·(h·x)`: TERNARY `comp`/`act` atoms in the
    /// BODY (reified) and a BINARY `eq` head (a plain binary tuple, not reified). Seeded on
    /// a concrete compatible action so both bracketings reach the SAME value `r`; then the
    /// binary consequent `eq(r, r)` must be derived. This is the e8-symmetry law shape
    /// evaluating end-to-end through `reason_program`.
    #[test]
    fn reason_program_closure_dataset_carries_the_derived_nary_tuple() {
        // The closure→RDF bridge (the native competency lane's substrate): the det law's
        // closure dataset, obtained via reason_program_closure_dataset, must contain the
        // DERIVED reified argument triple logic:naryArg0(R, dA) — a triple no query over the
        // asserted EDB alone could see (R is a chase-minted reifier).
        let law = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(
                        MATMUL,
                        vec![
                            Term::var("A").unwrap(),
                            Term::var("B").unwrap(),
                            Term::var("AB").unwrap(),
                        ],
                    ),
                    fml_atom(DET, vec![Term::var("A").unwrap(), Term::var("dA").unwrap()]),
                    fml_atom(DET, vec![Term::var("B").unwrap(), Term::var("dB").unwrap()]),
                    fml_atom(
                        DET,
                        vec![Term::var("AB").unwrap(), Term::var("dAB").unwrap()],
                    ),
                ])),
                Box::new(fml_atom(
                    MUL,
                    vec![
                        Term::var("dA").unwrap(),
                        Term::var("dB").unwrap(),
                        Term::var("dAB").unwrap(),
                    ],
                )),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);
        let na = logic_nary_arg(0);
        let nb = logic_nary_arg(1);
        let nab = logic_nary_arg(2);
        let edb = dataset(vec![
            quad(MATMUL_REIFIER, LOGIC_INSTANCE_OF, MATMUL),
            quad(MATMUL_REIFIER, &na, MAT_A),
            quad(MATMUL_REIFIER, &nb, MAT_B),
            quad(MATMUL_REIFIER, &nab, MAT_AB),
            quad(MAT_A, DET, DET_A),
            quad(MAT_B, DET, DET_B),
            quad(MAT_AB, DET, DET_AB),
        ]);

        let closure = reason_program_closure_dataset(&program, edb.as_ref())
            .expect("closure dataset must build");

        // Scan the projected closure for logic:naryArg0(R, dA): R is the chase-minted
        // content-addressed reifier, dA the concrete det value.
        let na0 = logic_nary_arg(0);
        let found = closure.owned_quads().any(|q| {
            q.predicate == na0
                && q.object == RdfTerm::iri(DET_A)
                && matches!(&q.subject, RdfTerm::Iri(s) if s.starts_with(NARY_REIFIER_PREFIX))
        });
        assert!(
            found,
            "the closure dataset must carry the derived logic:naryArg0(R, dA) triple; quads: {:?}",
            closure
                .owned_quads()
                .map(|q| (q.subject.clone(), q.predicate.clone(), q.object.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn reason_program_evaluates_the_group_action_law_end_to_end() {
        // ∀g,h,x,gh,r1,hx,r2. comp(g,h,gh) ∧ act(gh,x,r1) ∧ act(h,x,hx) ∧ act(g,hx,r2) → eq(r1,r2)
        const COMP: &str = "http://gmeow.example/comp";
        const ACT: &str = "http://gmeow.example/act";
        const EQ: &str = "http://gmeow.example/eq";
        let v = |n: &str| Term::var(n).unwrap();
        let law = Formula::Forall {
            vars: ["g", "h", "x", "gh", "r1", "hx", "r2"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(COMP, vec![v("g"), v("h"), v("gh")]),
                    fml_atom(ACT, vec![v("gh"), v("x"), v("r1")]),
                    fml_atom(ACT, vec![v("h"), v("x"), v("hx")]),
                    fml_atom(ACT, vec![v("g"), v("hx"), v("r2")]),
                ])),
                Box::new(fml_atom(EQ, vec![v("r1"), v("r2")])),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

        // A concrete compatible action where (g·h)·x and g·(h·x) both reach `r`.
        const G: &str = "http://gmeow.example/g";
        const H: &str = "http://gmeow.example/h";
        const XPT: &str = "http://gmeow.example/pt";
        const GH: &str = "http://gmeow.example/gh";
        const HX: &str = "http://gmeow.example/hx";
        const R: &str = "http://gmeow.example/r";
        let a0 = logic_nary_arg(0);
        let a1 = logic_nary_arg(1);
        let a2 = logic_nary_arg(2);
        // comp/act are ternary → the EDB atoms are authored PRE-REIFIED (instanceOf + naryArg).
        let mut quads = Vec::new();
        let mut reify = |node: &str, rel: &str, s: &str, t: &str, u: &str| {
            quads.push(quad(node, LOGIC_INSTANCE_OF, rel));
            quads.push(quad(node, &a0, s));
            quads.push(quad(node, &a1, t));
            quads.push(quad(node, &a2, u));
        };
        reify("http://gmeow.example/r_comp", COMP, G, H, GH); // g·h = gh
        reify("http://gmeow.example/r_act1", ACT, GH, XPT, R); // (g·h)·x = r
        reify("http://gmeow.example/r_act2", ACT, H, XPT, HX); // h·x = hx
        reify("http://gmeow.example/r_act3", ACT, G, HX, R); // g·(h·x) = r
        let edb = dataset(quads);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // The binary consequent eq(r1, r2) = eq(r, r) must be derived.
        let eq_rr = result
            .inferred()
            .iter()
            .any(|ax| ax.predicate == EQ && ax.subject == R && ax.object == format!("<{R}>"));
        assert!(
            eq_rr,
            "the group-action law must derive eq(r, r); closure: {:?}",
            result
                .inferred()
                .iter()
                .filter(|a| a.predicate == EQ)
                .map(|a| (&a.subject, &a.object))
                .collect::<Vec<_>>()
        );
        // A fully-evaluable n-ary body law lowers exactly (no residue).
        assert!(
            !result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "the group-action law lowers exactly: {:?}",
            result.preservation
        );
    }

    /// The homomorphic-encryption law `Dec(E(a) ⊗ E(b)) = a ⊕ b`: BINARY `enc`/`dec`
    /// atoms (plain body triples) plus TERNARY `ctMul`/`ptAdd` atoms (reified body atoms)
    /// and a BINARY `eq` head. Seeded on concrete values so the decrypted ciphertext
    /// product and the plaintext sum reach the SAME value `p`; then `eq(p, p)` must be
    /// derived. This is the homomorphic-encryption law shape evaluating end-to-end.
    #[test]
    fn reason_program_evaluates_the_he_law_end_to_end() {
        // ∀a,b,ea,eb,prod,decv,sum.
        //   enc(a,ea) ∧ enc(b,eb) ∧ ctMul(ea,eb,prod) ∧ dec(prod,decv) ∧ ptAdd(a,b,sum) → eq(decv,sum)
        const ENC: &str = "http://gmeow.example/enc";
        const DEC: &str = "http://gmeow.example/dec";
        const CTMUL: &str = "http://gmeow.example/ctMul";
        const PTADD: &str = "http://gmeow.example/ptAdd";
        const EQ: &str = "http://gmeow.example/eq";
        let v = |n: &str| Term::var(n).unwrap();
        let law = Formula::Forall {
            vars: ["a", "b", "ea", "eb", "prod", "decv", "sum"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            body: Box::new(Formula::Implies(
                Box::new(Formula::And(vec![
                    fml_atom(ENC, vec![v("a"), v("ea")]),
                    fml_atom(ENC, vec![v("b"), v("eb")]),
                    fml_atom(CTMUL, vec![v("ea"), v("eb"), v("prod")]),
                    fml_atom(DEC, vec![v("prod"), v("decv")]),
                    fml_atom(PTADD, vec![v("a"), v("b"), v("sum")]),
                ])),
                Box::new(fml_atom(EQ, vec![v("decv"), v("sum")])),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);

        // Concrete values: encrypt a→ea, b→eb; the ciphertext product decrypts to `p`, and
        // the plaintext sum is the SAME `p` (the homomorphic property holds on this instance).
        const A: &str = "http://gmeow.example/pa";
        const B: &str = "http://gmeow.example/pb";
        const EA: &str = "http://gmeow.example/ea";
        const EB: &str = "http://gmeow.example/eb";
        const PROD: &str = "http://gmeow.example/prod";
        const P: &str = "http://gmeow.example/p";
        let a0 = logic_nary_arg(0);
        let a1 = logic_nary_arg(1);
        let a2 = logic_nary_arg(2);
        // Binary enc/dec are PLAIN triples; ternary ctMul/ptAdd are PRE-REIFIED.
        let mut quads = vec![
            quad(A, ENC, EA),   // enc(a) = ea
            quad(B, ENC, EB),   // enc(b) = eb
            quad(PROD, DEC, P), // dec(prod) = p
        ];
        let mut reify = |node: &str, rel: &str, s: &str, t: &str, u: &str| {
            quads.push(quad(node, LOGIC_INSTANCE_OF, rel));
            quads.push(quad(node, &a0, s));
            quads.push(quad(node, &a1, t));
            quads.push(quad(node, &a2, u));
        };
        reify("http://gmeow.example/r_ctmul", CTMUL, EA, EB, PROD); // ea ⊗ eb = prod
        reify("http://gmeow.example/r_ptadd", PTADD, A, B, P); // a ⊕ b = p
        let edb = dataset(quads);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // The binary consequent eq(decv, sum) = eq(p, p) must be derived.
        let eq_pp = result
            .inferred()
            .iter()
            .any(|ax| ax.predicate == EQ && ax.subject == P && ax.object == format!("<{P}>"));
        assert!(
            eq_pp,
            "the homomorphic-encryption law must derive eq(p, p); closure: {:?}",
            result
                .inferred()
                .iter()
                .filter(|a| a.predicate == EQ)
                .map(|a| (&a.subject, &a.object))
                .collect::<Vec<_>>()
        );
        // A fully-evaluable law (binary + reified-ternary body) lowers exactly (no residue).
        assert!(
            !result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "the homomorphic-encryption law lowers exactly: {:?}",
            result.preservation
        );
    }

    #[test]
    fn reason_program_discloses_nary_head_unbound_arg_residue() {
        // A head variable the body does not bind is a non-range-restricted existential: the law
        // is carried as residue (SoundUnder) and derives NOTHING, never an unsafe tuple.
        let law = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(
                // Body binds dA, dB but NOT dAB.
                Box::new(Formula::And(vec![
                    fml_atom(
                        MATMUL,
                        vec![
                            Term::var("A").unwrap(),
                            Term::var("B").unwrap(),
                            Term::var("AB").unwrap(),
                        ],
                    ),
                    fml_atom(DET, vec![Term::var("A").unwrap(), Term::var("dA").unwrap()]),
                    fml_atom(DET, vec![Term::var("B").unwrap(), Term::var("dB").unwrap()]),
                ])),
                Box::new(fml_atom(
                    MUL,
                    vec![
                        Term::var("dA").unwrap(),
                        Term::var("dB").unwrap(),
                        Term::var("dAB").unwrap(),
                    ],
                )),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![law]);
        let na = logic_nary_arg(0);
        let nb = logic_nary_arg(1);
        let nab = logic_nary_arg(2);
        let edb = dataset(vec![
            quad(MATMUL_REIFIER, LOGIC_INSTANCE_OF, MATMUL),
            quad(MATMUL_REIFIER, &na, MAT_A),
            quad(MATMUL_REIFIER, &nb, MAT_B),
            quad(MATMUL_REIFIER, &nab, MAT_AB),
            quad(MAT_A, DET, DET_A),
            quad(MAT_B, DET, DET_B),
        ]);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        assert!(
            result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "an unsafe (non-range-restricted) head must drop the claim to SoundUnder: {:?}",
            result.preservation.polarities
        );
        assert!(
            result
                .preservation
                .unsupported_constructs
                .iter()
                .any(|c| c.contains("not bound by the body")),
            "the range-restriction residue must be disclosed: {:?}",
            result.preservation.unsupported_constructs
        );
        // Nothing of the mul tuple is materialized.
        let mul_obj = format!("<{MUL}>");
        assert!(
            !result
                .inferred()
                .iter()
                .any(|ax| ax.predicate == LOGIC_INSTANCE_OF && ax.object == mul_obj),
            "an unsafe head derives no tuple: {:?}",
            result
                .inferred()
                .iter()
                .map(|a| (&a.subject, &a.predicate, &a.object))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn reason_program_discloses_non_horn_formula_residue() {
        // ∀x. (knows(x, alice) → (trusts(x, bob) ∨ trusts(x, sam))) has a disjunctive head:
        // it does NOT lower to a rule, so it must be disclosed as residue in the result's
        // preservation claim — flagged, never silently evaluated as one disjunct.
        let formula = Formula::Forall {
            vars: vec!["x".into()],
            body: Box::new(Formula::Implies(
                Box::new(fml_atom(
                    KNOWS,
                    vec![
                        Term::var("x").unwrap(),
                        Term::iri(ALICE.to_owned()).unwrap(),
                    ],
                )),
                Box::new(Formula::Or(vec![
                    fml_atom(
                        TRUSTS,
                        vec![Term::var("x").unwrap(), Term::iri(BOB.to_owned()).unwrap()],
                    ),
                    fml_atom(
                        TRUSTS,
                        vec![Term::var("x").unwrap(), Term::iri(SAM.to_owned()).unwrap()],
                    ),
                ])),
            )),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]);
        let edb = dataset(vec![quad(SAM, KNOWS, ALICE)]);

        let result = reason_program(&program, edb.as_ref()).expect("reason_program ok");

        // Eval-path honesty: the disjunctive formula is disclosed (SoundUnder), and it does
        // NOT silently materialize either disjunct.
        assert!(
            result
                .preservation
                .polarities
                .contains(&PreservationKind::SoundUnder),
            "a non-evaluable formula must drop the claim to SoundUnder: {:?}",
            result.preservation.polarities
        );
        assert!(
            !result.preservation.unsupported_constructs.is_empty(),
            "the disjunctive residue must be disclosed, not silently absent"
        );
        assert!(
            !result.inferred().iter().any(|ax| ax.predicate == TRUSTS),
            "neither disjunct may be silently materialized: {:?}",
            result
                .inferred()
                .iter()
                .map(|a| (&a.subject, &a.predicate, &a.object))
                .collect::<Vec<_>>()
        );
    }

    // ── Mid-chase step governor: reason_all_budgeted CUTS the forward closure ──────────

    /// A subclass chain c0 ⊑ c1 ⊑ … ⊑ c(n-1) with x : c0. The native DL closure derives
    /// every transitive subsumption (O(n²)) and propagates x up the whole chain (O(n)), so
    /// the committed-derivation count grows super-linearly across many semi-naive rounds —
    /// a closure large enough that a tiny step budget must cut it mid-chase.
    fn chain_dataset(n: usize) -> std::sync::Arc<purrdf::RdfDataset> {
        let cls = |i: usize| format!("http://gmeow.example/c{i}");
        let mut quads = Vec::new();
        for i in 0..n.saturating_sub(1) {
            quads.push(quad(&cls(i), SUBCLASS, &cls(i + 1)));
        }
        quads.push(quad(X, TYPE, &cls(0)));
        dataset(quads)
    }

    #[test]
    fn reason_all_budgeted_cuts_the_chase_and_returns_a_strictly_smaller_partial_closure() {
        let store = chain_dataset(20);

        // Ground truth: the UNBUDGETED closure runs to full fixpoint.
        let full = reason_all(store.as_ref()).expect("unbudgeted reason_all decides the closure");
        let full_len = full.inferred().len();
        assert!(
            full_len > 100,
            "the chain closure must be large enough to bound meaningfully; got {full_len}"
        );

        const MAX: u64 = 5;
        let budget = Budget {
            max_answers: None,
            max_steps: Some(MAX),
        };
        let cut =
            reason_all_budgeted(store.as_ref(), &budget).expect("budgeted reason_all decides");

        // The cut is OBSERVED on the governor's own signal — the budget-exhausted status and
        // the committed step count — NOT inferred from a size comparison after a full run.
        assert_eq!(
            cut.evaluation,
            EvaluationStatus::BudgetExhausted,
            "a mid-chase cut is a non-conclusive budget-exhausted verdict"
        );
        assert_eq!(cut.completeness, CompletenessStatus::Incomplete);
        assert_eq!(
            cut.information,
            InformationState::Undetermined,
            "a truncated closure yields the honest Undetermined, never a wrong supported/both"
        );
        assert_eq!(
            cut.provenance.consumed_budget.consumed, MAX,
            "the governor admits EXACTLY max_steps committed derivations, then stops"
        );
        assert_eq!(cut.provenance.consumed_budget.allowance, Some(MAX));
        assert_eq!(
            cut.provenance.consumed_budget.limit,
            Some(crate::result::BudgetLimit::Inference)
        );

        // The materialized PARTIAL closure is STRICTLY smaller than the full closure: the
        // chase stopped deriving facts, it was not relabeled after running to completion.
        assert!(
            cut.inferred().len() < full_len,
            "partial closure ({}) must be strictly smaller than the full closure ({full_len})",
            cut.inferred().len()
        );
        assert!(
            !cut.inferred().is_empty(),
            "the partial closure still carries the EDB echo + the derivations the budget bought"
        );
    }

    #[test]
    fn reason_all_budgeted_with_ample_budget_is_byte_identical_to_unbudgeted() {
        let store = chain_dataset(8);
        let full = reason_all(store.as_ref()).expect("unbudgeted reason_all");

        // A ceiling far above the true closure never trips the governor.
        let ample = reason_all_budgeted(
            store.as_ref(),
            &Budget {
                max_answers: None,
                max_steps: Some(100_000),
            },
        )
        .expect("ample budgeted reason_all");
        assert_eq!(
            ample.evaluation,
            EvaluationStatus::Completed,
            "an ample ceiling completes normally (no spurious truncation)"
        );
        assert_eq!(
            ample, full,
            "an uncut budgeted run is byte-identical to the unbudgeted reason_all"
        );

        // The absent-budget (`None`) path is likewise byte-identical to today's reason_all.
        let none = reason_all_budgeted(
            store.as_ref(),
            &Budget {
                max_answers: None,
                max_steps: None,
            },
        )
        .expect("none-budget reason_all");
        assert_eq!(
            none, full,
            "max_steps == None is the unbudgeted path — identical to reason_all"
        );
    }

    #[test]
    fn reason_all_budgeted_partial_closure_is_deterministic() {
        let store = chain_dataset(16);
        let budget = Budget {
            max_answers: None,
            max_steps: Some(7),
        };
        let a = reason_all_budgeted(store.as_ref(), &budget).expect("budgeted run a");
        let b = reason_all_budgeted(store.as_ref(), &budget).expect("budgeted run b");
        assert_eq!(a.evaluation, EvaluationStatus::BudgetExhausted);
        assert_eq!(
            a, b,
            "the same input + the same small max_steps must yield the SAME partial closure"
        );
        assert_eq!(a.provenance.consumed_budget.consumed, 7);
    }
}
