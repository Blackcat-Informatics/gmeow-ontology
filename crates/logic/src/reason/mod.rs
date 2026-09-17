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
pub(crate) mod builtin_gap;
pub(crate) mod dataset;
mod session;
pub(crate) use session::NativeReasoningSession;
pub mod dl;
pub mod el;
pub(crate) mod enactment;
mod leave_one_out;
pub mod ledger;
pub mod math_gate;
pub mod perf_ledger;
mod program;
pub use program::{PreparedReasoningInput, prepare_reasoning_input};
pub mod refute;
pub mod rl;
mod schema;
pub(crate) use schema::laws as schema_laws;
pub(crate) mod source_existentials;
#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::reason_closure;
pub(crate) mod value;

pub use dl::{DlGap, DlVerdict, InconsistencyWitness, UnsatClass, dl_consistency};
pub use el::{ElClosure, InferredAxiom, el_closure};
pub use ledger::{
    DivergenceKind, DivergenceLedger, ExternalComparison, LedgerRow, LedgerVerdict, build_ledger,
    compare_external_corpus, divergence_diag_ledger, divergence_findings, dl_gap_rows, enforce,
};
pub use refute::{NativeFragmentRegistry, native_fragment_registry};
pub use rl::{RlClosure, RlTriple, rl_closure};

use crate::facts::TypedFactSet;
use crate::modal::ModalFact;
use crate::oracle::TypedRow;
use crate::query_ir::Budget;
use crate::result::{BudgetLimit, BudgetUsage, ReasoningResult};
use crate::seam::BudgetStatus;
use purrdf::{
    RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTriple, TermRef, TermValue,
};

/// One production existential-program admission certificate, scoped to the RDF
/// world whose obligations were chased.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChaseCertificate {
    /// Named-graph world whose existential program was certified and evaluated.
    pub world: String,
    /// Native input-shape and immutable-definition contract used by a joint
    /// source-bound plan. This is admission scope, never an unrestricted rewrite
    /// certificate or a digest of an entire corpus.
    pub input_contract: [u8; 32],
    /// The native chase termination certificate and its proof evidence.
    pub admission: crate::materialize::ChaseAdmission,
}

impl ChaseCertificate {
    /// Project this world-scoped certificate onto the shared diagnostic model.
    #[must_use]
    pub fn to_finding(&self) -> gmeow_errors::Finding {
        let mut finding = self.admission.to_finding();
        finding.message = format!("world <{}>: {}", self.world, finding.message);
        finding.message = format!(
            "input contract {}: {}",
            blake3::Hash::from(self.input_contract).to_hex(),
            finding.message,
        );
        finding
    }
}

/// Exact finite capacity of an intrinsic datatype, using the same native authority
/// as compiled datatype plans. Infinite or incompletely bounded spaces return None.
/// Authenticated math observations compare their authored counts with this result.
#[must_use]
pub fn finite_named_datatype_capacity(iri: &str) -> Option<u128> {
    crate::physical::finite_named_capacity(iri)
}

/// The decomposable derivation of one chase-invented null, re-exported so a
/// consumer of [`ReasoningResult`] can explain an invented individual without
/// re-running the chase.
pub use crate::physical::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, WitnessDerivation,
    WitnessHead, WitnessPosition, WitnessScope, WitnessStatement,
};

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

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// Complete whole-file commitments of the canonical native kernel. The shared
/// module owner follows production declarations across the logic, compiler,
/// namespace and term-arena crates, independent of host/profile. External tests
/// and doc-only modules never participate. PurRDF's own substrate identity is
/// folded separately by [`native_contract_hash`].
const NATIVE_CONTRACT_COMPONENTS: &[(&str, &str)] = crate::runtime::NATIVE_SOURCE_FILES;

/// The framed concatenation of every native source component, BEFORE the purrdf
/// substrate identity is folded in.
///
/// Split out from [`native_contract_hash`] so the participation test
/// (`native_contract_hash_folds_the_purrdf_substrate_identity`) can prove the purrdf
/// identity genuinely moves the final hash: it recomputes the digest from this base
/// alone and asserts it differs from the folded value.
fn framed_native_component_source() -> String {
    // Contract source is immutable for the lifetime of a compiled binary. Frame
    // every component by name and byte length so neither path/content boundaries
    // nor concatenation ambiguity can produce the same semantic identity.
    let source_contract = crate::runtime::NATIVE_SOURCE_CONTRACT;
    let mut contract = format!(
        "native-semantic-kernel-v1:{}:{source_contract}:",
        source_contract.len()
    );
    for (name, source) in NATIVE_CONTRACT_COMPONENTS {
        use std::fmt::Write as _;
        write!(&mut contract, "{}:{name}:{}:", name.len(), source.len())
            .expect("String writes cannot fail");
        contract.push_str(source);
    }
    contract
}

/// A deterministic, `purrdf`-PROVIDED identity of the external substrate the moved
/// reasoning lanes now stand on — the value [`native_contract_hash`] folds in so a
/// purrdf pin bump that changes those lanes is detected even though no native source
/// byte moved.
///
/// It is composed of three purrdf-owned, side-effect-free identities, all pure
/// functions of the pinned purrdf rule tables and version constant (no I/O, no git
/// shell-out, no wall-clock):
///
/// * [`purrdf::datalog::cache::CALCULUS_VERSION`] — the datalog evaluator SEMANTICS
///   (code, not data), the shared substrate under both the RL chase and every DL entail
///   service that runs the datalog kernel. purrdf bumps it exactly when the evaluator's
///   answers can change.
/// * the OWL 2 RL calculus `contract_hash` — `purrdf`'s own BLAKE3 over the OWL-RL rule
///   program PLUS the version PLUS the three evaluation ceilings
///   (`contract_hash(&calculus_program(Regime::OwlRl))`, the exact identity
///   `purrdf::entail`'s report carries for the RL lane the cutover moved to). Any change
///   to the RL rule set the RL lane closes under moves this digest.
/// * the `D` (datatype-entailment) calculus `contract_hash` — the finest purrdf-provided
///   identity for the datatype rule surface the value-space decider
///   ([`crate::reason::refute::datatype`]) sits beside. (`purrdf::xsd`'s pure value-space
///   algebra exposes no independent version/contract const of its own, so the whole
///   `purrdf::xsd` surface is pinned transitively by the git rev in `Cargo.lock`; this D
///   digest is the closest purrdf-owned rule-surface identity available.)
///
/// This is NOT purrdf's per-result `contract_hash` re-badged as the native identity: it
/// is one framed INPUT to [`native_contract_hash`]'s own SHA-1 fold. The two identities
/// stay distinct — `native_contract_hash` remains a SHA-1 over native source plus this
/// input; purrdf's `contract_hash` remains a BLAKE3 over a purrdf calculus — and are
/// never assumed interchangeable.
fn purrdf_substrate_identity() -> String {
    let owl_rl = purrdf::datalog::cache::contract_hash(&purrdf::entail::calculus_program(
        purrdf::entail::Regime::OwlRl,
    ));
    let datatype = purrdf::datalog::cache::contract_hash(&purrdf::entail::calculus_program(
        purrdf::entail::Regime::D,
    ));
    format!(
        "purrdf-substrate-v1|calculus:{}|owl-rl:{}|datatype:{}",
        purrdf::datalog::cache::CALCULUS_VERSION,
        owl_rl.to_hex(),
        datatype.to_hex(),
    )
}

pub fn native_contract_hash() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut contract = framed_native_component_source();
        // Fold the purrdf substrate identity under its OWN framing tag, length-prefixed
        // the same way as a source component so it cannot alias any file boundary. A
        // purrdf pin bump that changes the moved lanes (RL chase, DL services, datatype
        // value space) moves this segment and therefore the native contract hash.
        let purrdf = purrdf_substrate_identity();
        const PURRDF_TAG: &str = "purrdf-substrate";
        use std::fmt::Write as _;
        write!(
            &mut contract,
            "{}:{PURRDF_TAG}:{}:",
            PURRDF_TAG.len(),
            purrdf.len()
        )
        .expect("String writes cannot fail");
        contract.push_str(&purrdf);
        crate::provenance::sha1_hex(&contract)
    })
    .clone()
}

/// Return the complete native closure under the caller's explicit world selection.
/// All selected producers execute together; this projection never runs a second
/// DL or modal engine.
///
/// # Errors
/// Returns the same admission and execution failures as [`reason_all`].
pub fn reason_closure_axioms(
    input: PreparedReasoningInput,
    domains: &SelectedDomains,
) -> gmeow_errors::Result<Vec<InferredAxiom>> {
    Ok(reason_all(input, domains)?.inferred().to_vec())
}

/// Execute the selected native operation and project its exact graph identities.
///
/// # Errors
/// Returns source admission, execution, evidence, or RDF construction failures.
pub fn reason_closure_dataset(
    input: PreparedReasoningInput,
    domains: &SelectedDomains,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    native_closure_to_dataset(&reason_all(input, domains)?)
}

/// Project an admitted native closure without re-running any producer.
/// Retained graph identities preserve scoped blanks and declared empty worlds;
/// world-key strings are never reinterpreted as RDF graph names.
///
/// # Errors
/// Rejects missing or invalid execution evidence, unknown row contexts, or RDF
/// terms that violate the output dataset's structural contract.
pub fn native_closure_to_dataset(
    result: &ReasoningResult,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    result.validate_native_closure()?;
    let execution = result.native_execution()?;
    let mut builder = RdfDatasetBuilder::new();
    let mut graphs = std::collections::BTreeMap::new();
    for (world, graph) in &execution.worlds {
        let term = graph.graph().map(term_value_to_rdf_term).transpose()?;
        if let Some(term) = &term {
            let id = builder.intern_owned_term(term);
            builder.declare_named_graph(id);
        }
        graphs.insert(world.clone(), term);
    }
    for axiom in result.inferred() {
        let graph = graphs.get(&axiom.world).ok_or_else(|| {
            reason_err(format!(
                "native closure row has no retained context {:?}",
                axiom.world
            ))
        })?;
        let mut quad = RdfQuad::new(
            RdfTerm::iri(&axiom.subject),
            &axiom.predicate,
            term_value_to_rdf_term(&axiom.object)?,
        );
        quad.graph_name = graph.clone();
        builder.push_owned_quad(&quad);
    }
    builder
        .freeze()
        .map_err(|error| reason_err(format!("freeze native closure: {error}")))
}

/// Materialize an already-computed closure ([`InferredAxiom`]s) into a frozen
/// [`RdfDataset`] of the inferred triples.
///
/// Split out of [`reason_closure_dataset`] because there are two ways to GET a closure and
/// only one right way to lower it to RDF. [`reason_closure_dataset`] runs the UNBUDGETED
/// chase, which is correct for an in-process caller that owns its own input; the agent-facing
/// MCP `reason_graph` tool must run [`reason_all_budgeted`] instead (R4 forbids exposing an
/// unbudgeted Turing-complete evaluation to an agent loop) and then lower
/// [`ReasoningResult::inferred`](crate::result::ReasoningResult::inferred). Both lower through
/// THIS function, so a budgeted closure and an unbudgeted one of the same size serialize to
/// byte-identical RDF — which is what lets the browser's live entailment panel and the native
/// reasoner be compared byte-for-byte.
///
/// # Errors
///
/// Returns `Err` if an inferred term cannot be lowered to RDF or the dataset cannot freeze.
pub fn inferred_axioms_to_dataset<'a>(
    inferred: impl IntoIterator<Item = &'a InferredAxiom>,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for ax in inferred {
        let subject = RdfTerm::iri(ax.subject.clone());
        let object = term_value_to_rdf_term(&ax.object)?;
        let mut quad = RdfQuad::new(subject, ax.predicate.clone(), object);
        if ax.world != rl::DEFAULT_WORLD && ax.world != "default" && !ax.world.is_empty() {
            let graph = if let Some(label) = ax.world.strip_prefix("_:") {
                RdfTerm::blank_node(label)
            } else if is_absolute_iri(&ax.world) {
                RdfTerm::iri(ax.world.clone())
            } else {
                return Err(reason_err(format!(
                    "invalid contextual closure world {:?}",
                    ax.world
                )));
            };
            quad = quad.in_graph(graph);
        }
        builder.push_owned_quad(&quad);
    }
    builder
        .freeze()
        .map_err(|e| reason_err(format!("freeze reasoned closure dataset: {e}")))
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

/// Determine which authored axioms remain derivable after exact source retraction.
/// One native theory is prepared and settled for the batch. Every isolated probe
/// shares that preparation and the immutable base proof state; affected completed
/// reads and unsupported proof paths are invalidated by the native producer graph.
/// No fixed-DL side engine or dataset reconstruction participates.
///
/// # Errors
/// Returns native source admission, execution, or evidence failures.
pub fn leave_one_out_rederived(
    input: PreparedReasoningInput,
    domains: &SelectedDomains,
    axioms: &[LeaveOneOutAxiom],
) -> gmeow_errors::Result<Vec<bool>> {
    use rayon::prelude::*;
    if axioms.is_empty() {
        return Ok(Vec::new());
    }
    let analysis = leave_one_out::BatchAnalysis::new(&input);
    let mut answers = vec![false; axioms.len()];
    let slow = axioms
        .iter()
        .enumerate()
        .filter_map(|(index, axiom)| match analysis.answer(axiom) {
            Some(answer) => {
                answers[index] = answer;
                None
            }
            None => Some((index, axiom)),
        })
        .collect::<Vec<_>>();
    if slow.is_empty() {
        return Ok(answers);
    }
    let potential = input
        .facts
        .iter()
        .flat_map(|(world, facts)| facts.iter().cloned().map(|fact| (world.clone(), fact)))
        .collect();
    let session = NativeReasoningSession::new(input, domains, potential)?;
    let resolved =
        slow.par_iter()
            .map(|(index, axiom)| {
                let result = session.retract(axiom)?;
                let answer = result.inferred().iter().any(|row| {
                    calculus_term(&row.subject) == calculus_term(&axiom.subject)
                        && calculus_term(&row.predicate) == calculus_term(&axiom.predicate)
                        && row.object.as_iri().is_some_and(|object| {
                            calculus_term(object) == calculus_term(&axiom.object)
                        })
                });
                Ok((*index, answer))
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
    for (index, answer) in resolved {
        answers[index] = answer;
    }
    Ok(answers)
}

/// Execute the complete selected native calculus over the caller's logical worlds.
/// Rule, class, datatype and modal producers share the same admitted input,
/// governor and completed-read dependencies. Every actual witness and source
/// boundary is retained on the returned result.
///
/// # Errors
/// Returns source-role admission, native execution, or evidence validation failures.
pub fn reason_all(
    input: PreparedReasoningInput,
    domains: &crate::physical::SelectedDomains,
) -> gmeow_errors::Result<ReasoningResult> {
    reason_program(
        &gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None),
        input,
        domains,
    )
}

/// Execute the same complete operation under the explicit derivation allowance.
/// A cut retains committed facts and supported conflict evidence while withholding
/// completeness-dependent consumers. Answer-count caps are query-only contracts.
///
/// # Errors
/// Returns the same admission and execution failures as [`reason_all`].
pub fn reason_all_budgeted(
    input: PreparedReasoningInput,
    domains: &crate::physical::SelectedDomains,
    budget: &Budget,
) -> gmeow_errors::Result<ReasoningResult> {
    Ok(reason_program_budgeted(
        &gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None),
        input,
        domains,
        budget.max_steps,
    )?
    .0)
}

impl ModalFact for InferredAxiom {
    fn graph(&self) -> &str {
        &self.world
    }

    fn subject(&self) -> &str {
        &self.subject
    }

    fn predicate(&self) -> &str {
        &self.predicate
    }

    fn object(&self) -> std::borrow::Cow<'_, str> {
        modal_object(&self.object)
    }
}

/// Modal frames name resource atoms. Borrow their IRIs directly; render a
/// non-resource only at the diagnostic admission boundary that rejects it.
fn modal_object(value: &TermValue) -> std::borrow::Cow<'_, str> {
    match value.as_iri() {
        Some(iri) => std::borrow::Cow::Borrowed(iri),
        None => std::borrow::Cow::Owned(crate::provenance::term_display(value)),
    }
}

/// Result of applying one ground conjecture candidate to a cached fixed-rule
/// reasoning state.
pub(crate) struct IncrementalReasoningResult {
    pub(crate) result: ReasoningResult,
    pub(crate) status: crate::seam::BudgetStatus,
    pub(crate) consumed_steps: u64,
}

pub(crate) fn ground_object_value(object: &RdfTerm) -> gmeow_errors::Result<TermValue> {
    match object {
        RdfTerm::Iri(_) | RdfTerm::Literal(_) => Ok(dataset::value(object)),
        other => Err(reason_err(format!(
            "incremental ground candidate object must be an IRI or literal, got {other:?}"
        ))),
    }
}

/// Execute the selected canonical rules and formulas together with the complete
/// native calculus. Lowering happens once, and every unimplemented construct
/// remains explicit in the result's preservation and completion evidence.
///
/// The caller supplies logical-domain authority independently of transport graph
/// presence. The borrowed source and all native statement tables stay intact.
///
/// # Errors
/// Returns lowering, source-role admission, native execution, or evidence failures.
pub fn reason_program(
    program: &gmeow_logic_compile::ir::LogicProgram,
    input: PreparedReasoningInput,
    domains: &crate::physical::SelectedDomains,
) -> gmeow_errors::Result<ReasoningResult> {
    Ok(reason_program_budgeted(program, input, domains, None)?.0)
}

/// The sole producer-to-result fold for the governed native operation. The typed
/// result consumes its execution evidence; no parallel certificate or witness
/// vectors survive under a second owner.
///
/// # Errors
/// Returns preparation, execution, scope, proof, or budget admission failures.
pub(crate) fn reason_program_budgeted(
    program: &gmeow_logic_compile::ir::LogicProgram,
    input: PreparedReasoningInput,
    domains: &crate::physical::SelectedDomains,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<(ReasoningResult, BudgetStatus, u64)> {
    let prepared = crate::program_analysis::prepare_program(program)?;
    let closure = program::execute(&prepared, input, domains, max_steps)?;
    result_from_closure(&prepared, closure, max_steps)
}

fn result_from_closure(
    prepared: &crate::program_analysis::PreparedProgram,
    closure: program::ProgramClosure,
    max_steps: Option<u64>,
) -> gmeow_errors::Result<(ReasoningResult, BudgetStatus, u64)> {
    let consumed = closure.consumed_steps;
    let status = closure.status;
    let budget = BudgetUsage {
        consumed,
        allowance: max_steps,
        limit: closure
            .inference_exhausted
            .then_some(BudgetLimit::Inference),
    };
    let evidence = crate::result::NativeExecutionEvidence {
        input_contract: closure.input_contract,
        worlds: closure
            .graphs
            .into_iter()
            .map(|(world, graph)| (world, crate::physical::LogicalGraph::from_graph(graph)))
            .collect(),
        selected_domains: closure.selected_domains,
        frontier: closure.frontier,
        status: closure.native_status,
        families: closure.native_families,
        class_admission: closure.class_admission,
        source_coverage: closure.source_coverage,
        classes: closure.classes,
        chase_certificates: closure.certificates,
        witness_derivations: closure.witnesses,
    };
    let result = ReasoningResult::from_native_closure(
        closure.inferred,
        evidence,
        &prepared.preservation,
        budget,
    )?;
    Ok((result, status, consumed))
}

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

/// Execute the selected canonical program and project its complete native closure.
/// Original context bindings, scoped blanks and empty named worlds come from the
/// retained execution evidence, without re-parsing or running another producer.
///
/// # Errors
/// Returns execution, native evidence validation, or RDF construction failures.
pub fn reason_program_closure_dataset(
    program: &gmeow_logic_compile::ir::LogicProgram,
    input: PreparedReasoningInput,
    domains: &SelectedDomains,
) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    native_closure_to_dataset(&reason_program(program, input, domains)?)
}

/// Project a native [`TermValue`] into an owned [`RdfTerm`] at an RDF output boundary.
///
/// Preserve scoped blanks, complete literal identity and recursive triple terms.
/// A non-IRI triple predicate fails rather than weakening the statement.
pub(crate) fn term_value_to_rdf_term(value: &TermValue) -> gmeow_errors::Result<RdfTerm> {
    Ok(match value {
        TermValue::Iri(iri) => {
            let parsed = purrdf::iri::parse(iri)
                .map_err(|error| reason_err(format!("closure RDF IRI: {error}")))?;
            if !parsed.has_scheme() {
                return Err(reason_err("closure RDF IRI must be absolute".into()));
            }
            RdfTerm::iri(iri.clone())
        }
        TermValue::Blank { label, scope } => RdfTerm::blank_node(scope.qualify_label(label)),
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            direction,
        } => RdfTerm::literal(RdfLiteral {
            lexical_form: lexical_form.clone(),
            datatype: Some(datatype.clone()),
            language: language.clone(),
            direction: *direction,
        }),
        TermValue::Triple { s, p, o } => {
            let predicate = match term_value_to_rdf_term(p)? {
                RdfTerm::Iri(iri) => iri.as_str().to_owned(),
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
    domains: &SelectedDomains,
) -> gmeow_errors::Result<ReasoningResult> {
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(bundle);
    builder.push_dataset(user);
    let merged = builder.freeze().map_err(|e| reason_err(e.to_string()))?;
    reason_all(prepare_reasoning_input(&merged)?, domains)
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
    let edb_facts = build_edb_facts(edb)?;
    let (chase, _, status) =
        crate::oracle::native_forward_eval_rules_with_frontier(&edb_facts, rules, None)?;
    if status != BudgetStatus::Ok {
        return Err(reason_err(
            "unbounded structured-rule closure did not complete".to_owned(),
        ));
    }
    chase_rows_to_inferred(&chase)
}

/// The ONE table mapping the canonical `logic:` axiom vocabulary onto the W3C spelling
/// the FIXED calculi match — the single definition of that correspondence for the whole
/// reasoner. Nothing under `reason/` may re-spell a `logic:` IRI outside it.
///
/// Two groups, and both are needed for one authored axiom to reach the closure:
///
/// * the **class-expression body** — `logic:Restriction` and its slots. Their local
///   names are shared by BOTH authoring surfaces (the same fact the compiler's
///   `RestrictionVocab` is parameterized on: only the namespace differs between a
///   `logic:`-authored restriction and its `owl:` projection), so each entry is built
///   from the local alone.
/// * the **anchors** that attach a body to the class it constrains — `logic:subClassOf`
///   / `logic:equivalentClass`, the two `RestrictionVocab` names as well, plus
///   `logic:subPropertyOf` for the property hierarchy. A body without its anchor is
///   still dark: `dl:type-propagation` and `cax-sco` reach a restriction node only
///   along a subsumption edge, so lowering the slots and not the anchor would read the
///   restriction and never apply it to an individual.
/// * the **typing + class-axiom vocabulary** the DL post-pass and the counting/case-split
///   refuters read BY NAME off the raw dataset (`owl:Class`/`owl:ObjectProperty`/… type
///   markers, the property-characteristic types, `owl:disjointWith`/`owl:inverseOf`/
///   `owl:unionOf`/`owl:oneOf`/…, and the `owl:Thing`/`owl:Nothing` top/bottom the clash
///   readers compare against). Once the `owl:` authoring spelling is retired, a slice
///   authors these as `logic:`; without the row the normalized read never matches the
///   hardcoded `owl:` constant and the axiom goes dark exactly as a bare `logic:Restriction`
///   would. Every entry here is a slice-authorable construct the reasoner already reads —
///   see the matching arms in `dl.rs` and `refute/counting.rs`.
static CALCULUS_VOCABULARY: [(&str, &str); 52] = {
    macro_rules! owl {
        ($local:literal) => {
            (
                concat!("https://blackcatinformatics.ca/logic/", $local),
                concat!("http://www.w3.org/2002/07/owl#", $local),
            )
        };
    }
    // The property-characteristic markers are NOT a pure namespace swap: the canonical `logic:`
    // spelling is lower-camel (`logic:transitiveProperty`), the `owl:` view upper-camel
    // (`owl:TransitiveProperty`) — the exact map `adapter::OWL_CHARACTERISTIC_TO_LOGIC` and
    // `rdf::owl_for_char` use. A slice authors `?P a logic:transitiveProperty` once `owl:` is
    // retired as an authoring vocabulary, so the
    // reasoner must lower THAT onto the upper-camel spelling the fixed RL characteristic rules match.
    macro_rules! owl_char {
        ($logic_local:literal, $owl_local:literal) => {
            (
                concat!("https://blackcatinformatics.ca/logic/", $logic_local),
                concat!("http://www.w3.org/2002/07/owl#", $owl_local),
            )
        };
    }
    [
        // Anchors.
        (gmeow_ns::LOGIC_SUB_CLASS_OF, gmeow_ns::RDFS_SUB_CLASS_OF),
        (
            gmeow_ns::LOGIC_SUB_PROPERTY_OF,
            gmeow_ns::RDFS_SUB_PROPERTY_OF,
        ),
        // Property domain/range — canonical `logic:` lowered to the fixed `rdfs:` calculus
        // spelling the DL/RL domain-range rules match (a slice authors `?P logic:domain C`
        // / `?P logic:range C`, e.g. the `logic:` grounding slice's own reasoning axioms).
        (gmeow_ns::LOGIC_DOMAIN, gmeow_ns::RDFS_DOMAIN),
        (gmeow_ns::LOGIC_RANGE, gmeow_ns::RDFS_RANGE),
        owl!("equivalentClass"),
        // Class-expression body.
        owl!("Restriction"),
        owl!("onProperty"),
        owl!("someValuesFrom"),
        owl!("allValuesFrom"),
        owl!("hasValue"),
        owl!("onClass"),
        owl!("onDataRange"),
        owl!("onDatatype"),
        owl!("withRestrictions"),
        owl!("cardinality"),
        owl!("minCardinality"),
        owl!("maxCardinality"),
        owl!("qualifiedCardinality"),
        owl!("minQualifiedCardinality"),
        owl!("maxQualifiedCardinality"),
        // Typing markers (rdf:type objects the refuters read by name).
        owl!("Class"),
        owl!("ObjectProperty"),
        owl!("DatatypeProperty"),
        owl!("NamedIndividual"),
        owl!("AnnotationProperty"),
        owl!("Ontology"),
        owl!("Thing"),
        owl!("Nothing"),
        // Property-characteristic types — canonical lower-camel `logic:` → upper-camel `owl:` view.
        owl_char!("functionalProperty", "FunctionalProperty"),
        owl_char!("inverseFunctionalProperty", "InverseFunctionalProperty"),
        owl_char!("transitiveProperty", "TransitiveProperty"),
        owl_char!("symmetricProperty", "SymmetricProperty"),
        owl_char!("asymmetricProperty", "AsymmetricProperty"),
        owl_char!("reflexiveProperty", "ReflexiveProperty"),
        owl_char!("irreflexiveProperty", "IrreflexiveProperty"),
        // Class + property axiom vocabulary.
        owl!("disjointWith"),
        owl!("complementOf"),
        owl!("inverseOf"),
        owl!("unionOf"),
        owl!("oneOf"),
        owl!("intersectionOf"),
        owl!("disjointUnionOf"),
        owl!("sameAs"),
        owl!("differentFrom"),
        owl!("equivalentProperty"),
        owl!("propertyChainAxiom"),
        owl!("propertyDisjointWith"),
        owl!("hasKey"),
        owl!("members"),
        owl!("hasSelf"),
        owl!("AllDisjointClasses"),
        owl!("AllDisjointProperties"),
    ]
};

/// The reasoner's fixed-calculus lowering table: each `(canonical logic: IRI, projected
/// W3C OWL/RDFS IRI)` pair the EDB boundary normalizes (Principle 17). Exposed so the
/// grounding-law cross-check (`crates/validate`) can pin the shipped
/// `logic:GroundingCorrespondence` corpus against THIS table directly, rather than a
/// hand-retyped mirror that can silently drift from it.
pub fn calculus_vocabulary() -> &'static [(&'static str, &'static str)] {
    &CALCULUS_VOCABULARY
}

/// The fixed-calculus spelling of a canonical `logic:` axiom term, or `None` when `iri`
/// is not one — the single lookup behind both lowerings below.
fn calculus_projection(iri: &str) -> Option<&'static str> {
    if !iri.starts_with(gmeow_ns::LOGIC_NS) {
        return None;
    }
    CALCULUS_VOCABULARY
        .iter()
        .find(|(canonical, _)| *canonical == iri)
        .map(|(_, projected)| *projected)
}

/// Normalize one term — a predicate, or an `rdf:type` object — onto the vocabulary the
/// fixed calculi match: a canonical `logic:` axiom term becomes its `rdfs:`/`owl:`
/// spelling, every other IRI passes through untouched.
///
/// This is the RAW-DATASET twin of [`edb_predicate_spellings`], and the two differ only
/// in how they carry the projection. The typed EDB holds a quad under two spellings at
/// once, so there the projection ADDS. A raw scan instead folds each quad into a struct
/// field keyed by predicate, where a second spelling would double-count (two
/// `Restriction` entries for one node, a doubled `predicates` completeness set), so here
/// it REPLACES. The direction is the same one in both: canonical → the fixed calculi's
/// W3C vocabulary, never the reverse (Principle 17 — `owl:`/`rdfs:` are the lossy
/// projections, and the reasoner reads them because the rules it implements are
/// specified in them).
///
/// # Why the raw waists need their own lowering at all
///
/// [`build_edb_facts`] keeps only IRI-object quads, so a restriction body — which is
/// anchored through a BLANK node (`C logic:subClassOf [ a logic:Restriction ; … ]`) —
/// never reaches the typed EDB in the first place. The DL post-pass reads those bodies
/// straight off the frozen dataset instead, which is exactly where an unlowered
/// canonical spelling goes dark.
pub(crate) fn calculus_term(iri: &str) -> &str {
    calculus_projection(iri).unwrap_or(iri)
}

/// The predicate spellings one authored quad contributes to a fixed-calculus EDB:
/// the authored predicate itself, plus — for a canonical `logic:` subsumption edge or a
/// canonical `logic:` restriction slot — the `rdfs:`/`owl:` spelling every fixed rule set
/// matches on.
///
/// # Why the lowering lives at the EDB boundary
///
/// The native EL/DL calculi ([`el::structured_el_rules`], [`dl::structured_dl_rules`])
/// and the RL lane's OWL 2 RL chase ([`rl::rl_closure`]) are FIXED and largely
/// W3C-specified: every subsumption rule
/// (`el:subClassOf-transitive`, `el:type-propagation`, `cax-sco`, `scm-sco`,
/// `scm-spo`, `prp-spo1`, …) matches the `rdfs:subClassOf` / `rdfs:subPropertyOf`
/// spelling *by specification*, and is not GMEOW's to re-author. But GMEOW's
/// authored surface is the CANONICAL `logic:` vocabulary (Principle 17: `rdfs:` is
/// one of its lossy projections), so a chase fed authored `module.ttl` sources sees
/// a taxonomy spelled `logic:subClassOf` and would derive nothing from it —
/// silently, with a taxonomy-free closure rather than an error. A consumer of the
/// shipped artifacts could not then answer "is this class a `math:MathConformanceFailure`?"
/// for any class whose parent edge was authored canonically.
///
/// So the projection is materialized at the EDB boundary, once, for every fixed
/// calculus: a canonical `logic:subClassOf` / `logic:subPropertyOf` quad is encoded a
/// second time under its `rdfs:` spelling ([`gmeow_ns::SUB_CLASS_OF`] /
/// [`gmeow_ns::SUB_PROPERTY_OF`], canonical first, projected second), in the same
/// world. The canonical quad is kept too — the projection ADDS the RDFS view, it
/// never replaces the authored edge — and both are asserted (`is_edb`), because a
/// projection of an asserted axiom is asserted, not derived.
///
/// When the `rdfs:` projection ceilings reach zero corpus-wide and the `rdfs:` arm of
/// the [`gmeow_ns`] doctrine is deleted, this stays: the direction of the lowering
/// (canonical → the fixed calculi's RDFS vocabulary) is what those calculi need, not
/// a transitional read of two authored spellings.
///
/// # The same lowering carries the class-expression (restriction) vocabulary
///
/// The restriction slots are the identical situation one construct over. `cls-svf1`,
/// `cls-avf`, `cls-hv1`/`cls-hv2` and the DL existential/counting readers all name
/// `owl:onProperty` / `owl:someValuesFrom` / `owl:allValuesFrom` *by specification*,
/// while a slice authors `[ a logic:Restriction ; logic:onProperty … ]`. Without the
/// projection an authored restriction body reaches the SHACL surface and contributes
/// nothing to the DL/EL closure: a mandatory-value axiom would enforce in validation
/// and be invisible to `gmeow entails`. So [`CLASS_EXPRESSION_VOCABULARY`] is projected
/// here on the same terms as the subsumption edge — canonical first, projected second,
/// both asserted, the authored quad never replaced.
pub(crate) fn edb_predicate_spellings(predicate: &str) -> impl Iterator<Item = &str> {
    std::iter::once(predicate).chain(calculus_projection(predicate))
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
/// Each surviving quad is pushed under every spelling
/// [`edb_predicate_spellings`] gives it, so a canonically-spelled
/// `logic:subClassOf` / `logic:subPropertyOf` taxonomy drives the fixed
/// RDFS-vocabulary calculus instead of sitting inert in the EDB. The selected
/// program input adapter shares that spelling policy while retaining literals,
/// default graphs and native statement annotations for authored rule execution.
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
    // RDF 1.2 annotation assertions live outside the base quad table. They are
    // ordinary facts about a reifier and must reach the same native calculus;
    // the reified proposition itself is never asserted by this admission.
    for quad in edb.quads().chain(edb.annotation_quads()) {
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
        for spelling in edb_predicate_spellings(predicate) {
            edb_facts.push_quad(&subject, spelling, &object, world);
        }
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
        let object = row.args[1].clone();
        let world = world_string(&row.args[2])?;

        let premises = prov
            .antecedents
            .iter()
            .map(decode_premise)
            .collect::<gmeow_errors::Result<Vec<_>>>()?;

        inferred.push(InferredAxiom {
            modal_evaluation: None,
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

#[path = "mod.tests.rs"]
#[cfg(test)]
mod tests;
