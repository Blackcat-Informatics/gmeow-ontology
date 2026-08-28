// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gmn-training-corpus` stage: a rejection-sampled, PROOF-CARRYING GMN training-corpus
//! emitter (req #21 + #20). Corpus GENERATION only — it never trains a model.
//!
//! # What it is
//!
//! A *productive functor over the glyph signature*: it DETERMINISTICALLY enumerates
//! well-typed GMN terms up to a bounded depth over the carrier's operator signature (the
//! binary operator forms resolved by [`gmeow_lang_bridge::resolve_operator_forms`], joined to
//! a small fixed base of typed constants), so the emitted corpus is a REAL corpus, not merely
//! the hand-authored demonstrators in
//! `slices/grounding/lang/examples/gmn-training-demonstrators.ttl`.
//!
//! Each enumerated candidate is a small RDF term-graph (one or two operator triples over the
//! typed atoms). Every candidate is REJECTION-SAMPLED through FIVE deterministic verifiers,
//! and only a candidate that passes all five is KEPT as a paired `task` / `input` / `target`
//! training example carrying its POSITIVE ACCEPTANCE CERTIFICATE. A rejected candidate is
//! DROPPED — never silently: its TYPED rejection reason (the shipped `lang:` failure class and
//! the verifier stage) is RECORDED in the same graph.
//!
//! ## The five verifiers (rejection filter)
//!
//! 1. **Parses** — [`gmeow_lang_bridge::gmn1_write`] then [`gmeow_lang_bridge::gmn1_read`]
//!    succeed against the carrier dictionary (a codec coverage/grammar failure is the codec's
//!    own typed [`gmeow_lang_bridge::Gmn1Error`]).
//! 2. **Round-trips** — the read-back GMN-0 model is [`gmeow_lang_bridge::gmn0_canonically_equal`]
//!    to the original (RDFC-1.0 canonical N-Quads agreement — the codec's own oracle).
//! 3. **Typechecks** — the term, translated into the logic-compile IR
//!    ([`gmeow_logic_compile::ir::Formula`] / [`gmeow_logic_compile::ir::Term`] /
//!    [`gmeow_logic_compile::ir::ReasoningProgramIr`]) with its atoms' order-sorts, compiles
//!    and resolves under the native order-sorted backward engine
//!    ([`gmeow_logic::goal_directed::evaluate_reasoning_programs`], the SAME prover the
//!    `stage-compile-logic` reasoning-program lane feeds) — an ill-formed / ill-arity term
//!    fails to lower or resolve.
//! 4. **Discharges a proof obligation** — the resolution yields a proof-checked answer whose
//!    content-addressed derivation IRI (`proof_checks == true`, the Curry–Howard checked
//!    proof object) is the discharged obligation carried on the certificate. A term that
//!    resolves to zero proof-checked answers carries NO obligation and is rejected.
//! 5. **Carries no security-ring leakage** — tagging the term's atoms at a content ring and
//!    admitting them into a target ring through Task 9's
//!    [`gmeow_lang_bridge::consume_project`] over the carrier's authored ring lattice raises
//!    no [`gmeow_lang_bridge::GmnConsumeError`] (a ring leak is the shipped `lang:GmnRingLeak`).
//!
//! On every KEPT pair the stage emits the acceptance certificate: the five verdicts, the
//! discharged proof-obligation derivation IRI, and the version provenance quad
//! ([`gmeow_lang_bridge::tag_schema_version`], Task 4's graph-resolved schema major). The
//! corpus is thus proof-carrying.
//!
//! # Determinism
//!
//! Byte-deterministic: no clock, no RNG, no HashMap iteration order. The signature is resolved
//! from the authored grounding sources under `input.root` (mirroring
//! [`crate::stages::lang_projection`]'s verbalizer wiring and
//! [`crate::stages::gmn1_gate`]'s dictionary loading), the base atoms + operators + depths are
//! enumerated in a fixed sorted order, and every proof derivation IRI is content-addressed.
//! A signature-load / codec / prover failure is a HARD FAIL (no-optionality); a REJECTED
//! candidate is not a failure — it is designed corpus filtering, recorded with its typed reason.
//!
//! # Dataflow edge (the 3-place declaration)
//!
//! The stage `gmeow:dataflowConsumes` `stage-compile-logic` (the typechecker/prover lane) AND
//! `stage-mappings` (the projected GMN forms / glyph registry lane). That edge is declared
//! IDENTICALLY in three places — [`Stage::consumes`] here, `gmeow:dataflowConsumes` in
//! `slices/core/pipeline/module.ttl`, and [`crate::run::full_spec`] — and the dogfooding
//! parity gate (`tests/dag_dogfood.rs`) proves the three never diverge.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_lang_bridge::{
    ConsumeProjection, Gmn0Model, Gmn1Document, GmnConsumeError, GmnDictionary, GmnOperatorForm,
    RingLattice, consume_project, gmn0_canonically_equal, gmn1_read, gmn1_write,
    resolve_operator_forms, resolved_schema_version,
};
use gmeow_logic::goal_directed::evaluate_reasoning_programs;
use gmeow_logic_compile::ir::{EvaluationMode, Formula, ReasoningProgramIr, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// The bundle-internal named graph the enumerated + certified corpus (and the recorded
/// rejections) is folded into. A sibling of `graph/goal-directed`: a queryable projection of a
/// native generator's output that ships inside `gmeow.gts`, excluded from the object-level EDB.
pub const GRAPH_GMN_TRAINING_CORPUS: &str =
    "https://blackcatinformatics.ca/gmeow/graph/gmn-training-corpus";

/// The `gmeow:` namespace root for the corpus vocabulary and the minted example/atom IRIs.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The examples namespace the enumerated atoms + example nodes are minted under.
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/gmn-training/";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `xsd:boolean` — the datatype of the five verdict flags.
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// `gmeow:gmnContentRing` — the claim-scoped content-ring tag the consume filter reads.
const PRED_CONTENT_RING: &str = "https://blackcatinformatics.ca/gmeow/gmnContentRing";

/// The three shipped ring individuals the verifier uses (resolved from the lang lattice):
/// `gmnRingCore` (innermost, flows outward to every enclosing ring) is the clean content ring,
/// `gmnRingTrusted` the admission target, and `gmnRingRestricted` (flows nowhere but itself)
/// the injected-leak ring the negative test tags.
const RING_CORE: &str = "https://blackcatinformatics.ca/gmeow/gmnRingCore";
const RING_TRUSTED: &str = "https://blackcatinformatics.ca/gmeow/gmnRingTrusted";

/// The base of typed constants the productive functor applies operators to — a small, fixed,
/// sorted set of atoms, each carrying ONE order-sort (`rdf:type`) so the order-sorted
/// unification the typecheck runs is non-vacuous. Minted under [`EX`]; the sorts are real
/// `math:` sorts so the constant_sorts seed is a genuine lattice coordinate.
fn base_atoms() -> Vec<(String, String)> {
    let integer = "https://blackcatinformatics.ca/math/Integer";
    vec![
        (format!("{EX}a"), integer.to_owned()),
        (format!("{EX}b"), integer.to_owned()),
        (format!("{EX}c"), integer.to_owned()),
    ]
}

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-gmn-training-corpus".to_string(),
        message: message.into(),
    })
}

// ── Signature resolution (mirrors lang_projection's verbalizer wiring) ─────────────────

/// The grounding slice module surfaces whose `rdfs:label`s name the GMN denotation targets and
/// whose lang module carries the dictionary + ring lattice.
const GROUNDING_MODULES: [&str; 3] = [
    "slices/grounding/logic/module.ttl",
    "slices/grounding/lang/module.ttl",
    "slices/grounding/math/module.ttl",
];

/// The resolved generation context: the carrier dictionary, the ring lattice, the selected
/// binary operator forms, and the typed atom base.
struct GenContext {
    dict: GmnDictionary,
    lattice: RingLattice,
    /// The binary operator forms the functor applies (arity 2), sorted by `term_iri`.
    operators: Vec<GmnOperatorForm>,
    /// atom IRI → its order-sort (`rdf:type`) IRI, sorted.
    atom_sorts: BTreeMap<String, String>,
}

/// Harvest the `rdfs:label` index (`IRI → label`) from the grounding module bytes — the
/// deterministic pick mirrors [`crate::stages::lang_projection`]'s `harvest_labels` (the
/// GMEOW-English label wins; ties break to the smallest lexical form).
fn harvest_labels(modules: &[Vec<u8>]) -> Result<BTreeMap<String, String>, gmeow_errors::Diag> {
    const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
    const GMEOW_ENGLISH: &str = "x-gmeow-english";
    let mut best: BTreeMap<String, (bool, String)> = BTreeMap::new();
    for module in modules {
        let dataset = purrdf::parse_dataset(module, "text/turtle", None)
            .map_err(|e| stage_err(format!("parse grounding module for labels: {e}")))?;
        for quad in dataset.owned_quads() {
            if quad.predicate != RDFS_LABEL {
                continue;
            }
            let RdfTerm::Iri(subject) = &quad.subject else {
                continue;
            };
            let RdfTerm::Literal(literal) = &quad.object else {
                continue;
            };
            let is_english = literal.language.as_deref() == Some(GMEOW_ENGLISH);
            let candidate = (is_english, literal.lexical_form.clone());
            let better = match best.get(subject) {
                Some((cur_english, cur_lex)) => {
                    (candidate.0, std::cmp::Reverse(candidate.1.clone()))
                        > (*cur_english, std::cmp::Reverse(cur_lex.clone()))
                }
                None => true,
            };
            if better {
                best.insert(subject.clone(), candidate);
            }
        }
    }
    Ok(best.into_iter().map(|(k, (_, lex))| (k, lex)).collect())
}

impl GenContext {
    /// Resolve the signature from the authored grounding sources under `root`. The dictionary +
    /// ring lattice come from the lang module; the operator forms are the carrier glyph
    /// registry's bindings joined to their denotation targets' `rdfs:label`s across the three
    /// grounding modules (the SAME resolution [`crate::stages::lang_projection`] performs). A
    /// missing/malformed source is a HARD FAIL (no-optionality).
    fn resolve(root: &Path) -> Result<Self, gmeow_errors::Diag> {
        let mut module_bytes: Vec<Vec<u8>> = Vec::new();
        for rel in GROUNDING_MODULES {
            module_bytes.push(
                std::fs::read(root.join(rel)).map_err(|e| stage_err(format!("read {rel}: {e}")))?,
            );
        }
        // Index 1 is the lang module (see GROUNDING_MODULES order) — the dictionary + lattice
        // carrier.
        let lang_ds = purrdf::parse_dataset(&module_bytes[1], "text/turtle", None)
            .map_err(|e| stage_err(format!("parse lang module: {e}")))?;
        let dict = GmnDictionary::from_dataset(&lang_ds)
            .map_err(|e| stage_err(format!("load GMN dictionary: {}", e.0)))?;
        let lattice = RingLattice::from_dataset(&lang_ds);
        if lattice.is_empty() {
            return Err(stage_err(
                "the lang module resolved an empty GMN ring lattice — the consume-path \
                 verifier cannot admit any content (corrupt signature source)",
            ));
        }
        let labels = harvest_labels(&module_bytes)?;
        let all_forms = resolve_operator_forms(dict.glyph_registry(), &labels)
            .map_err(|e| stage_err(format!("resolve GMN operator forms: {e}")))?;
        // The functor's expressible fragment: binary operators. A binary operator denotes a
        // relation projectable to a single RDF triple `(subject, operator, object)` — the
        // round-trippable, first-order-binary shape the codec and the order-sorted prover both
        // cover. Higher-arity / unary operators are an explicit, documented enumeration
        // boundary, not a silent drop (Constitution: explicit feature selection is permitted).
        let mut operators: Vec<GmnOperatorForm> =
            all_forms.into_iter().filter(|f| f.arity == 2).collect();
        operators.sort();
        operators.dedup();
        if operators.is_empty() {
            return Err(stage_err(
                "the carrier glyph registry resolved zero binary (arity-2) operator forms — \
                 the productive functor has no signature to enumerate over",
            ));
        }
        Ok(Self {
            dict,
            lattice,
            operators,
            atom_sorts: base_atoms().into_iter().collect(),
        })
    }
}

// ── The enumerated candidate (a well-typed GMN term as an RDF term-graph) ───────────────

/// One enumerated candidate GMN term: an ordered set of operator triples over term nodes.
/// `(subject, operator IRI, object)` per triple; the first triple's subject/operator seed the
/// proof-obligation query. A subject/object is an [`RdfTerm`] so an ADVERSARIAL probe can carry
/// a deliberately-uncoverable node (a reserved-label blank) the round-trip verifier must reject —
/// the well-typed enumeration itself uses only IRI atoms.
#[derive(Clone, Debug)]
struct Candidate {
    /// The operator triples, in enumeration order (depth-1 = one triple, depth-2 = a chain of
    /// two).
    triples: Vec<(RdfTerm, String, RdfTerm)>,
}

impl Candidate {
    /// The GMN-0 model of this term: the operator triples as an [`Gmn0Model`] (the codec's
    /// canonical, deduped, sorted quad set). This is the `input` surface of the training pair.
    fn model(&self) -> Gmn0Model {
        let mut builder = RdfDatasetBuilder::new();
        for (s, p, o) in &self.triples {
            builder.push_owned_quad(&RdfQuad::new(s.clone(), p.clone(), o.clone()));
        }
        let ds = builder
            .freeze()
            .expect("a candidate built from valid RdfTerms freezes cleanly");
        Gmn0Model::from_dataset(&ds)
    }

    /// Every atom IRI appearing in this term (subject or object), sorted + deduped. Blank nodes
    /// (adversarial probes only) carry no order-sort and are excluded from the sort seed.
    fn atoms(&self) -> Vec<String> {
        let mut atoms: Vec<String> = Vec::new();
        for (s, _, o) in &self.triples {
            for term in [s, o] {
                if let RdfTerm::Iri(iri) = term {
                    atoms.push(iri.clone());
                }
            }
        }
        atoms.sort();
        atoms.dedup();
        atoms
    }
}

/// An IRI term node.
fn iri(iri: &str) -> RdfTerm {
    RdfTerm::Iri(iri.to_owned())
}

/// Enumerate every well-typed candidate up to bounded depth, in a fixed deterministic order,
/// then append the deterministic ADVERSARIAL probe set so the rejection filter is exercised
/// LIVE over the shipped corpus (not merely in a unit test).
///
/// * **depth 1** — for each operator `op` and each ORDERED pair of DISTINCT atoms `(x, y)`, the
///   single-triple term `x op y`.
/// * **depth 2** — for each pair of operators `(op1, op2)` and each ORDERED triple of DISTINCT
///   atoms `(x, y, z)`, the chained term `{ x op1 y, y op2 z }` (a two-hop term graph). Bounded
///   at depth 2 (no deeper nesting) so the corpus is finite and small.
///
/// Well-typedness is arity-consistent BY CONSTRUCTION (every operator is arity-2, every triple
/// binary) and RE-CHECKED independently by the typecheck verifier — the enumeration is the
/// producer, the verifier the rejection filter, never the same code.
fn enumerate(ctx: &GenContext) -> Vec<Candidate> {
    let atoms: Vec<String> = ctx.atom_sorts.keys().cloned().collect();
    let mut out: Vec<Candidate> = Vec::new();
    // depth 1
    for op in &ctx.operators {
        for x in &atoms {
            for y in &atoms {
                if x == y {
                    continue;
                }
                out.push(Candidate {
                    triples: vec![(iri(x), op.term_iri.clone(), iri(y))],
                });
            }
        }
    }
    // depth 2 — chained over a shared middle atom.
    for op1 in &ctx.operators {
        for op2 in &ctx.operators {
            for x in &atoms {
                for y in &atoms {
                    for z in &atoms {
                        if x == y || y == z || x == z {
                            continue;
                        }
                        out.push(Candidate {
                            triples: vec![
                                (iri(x), op1.term_iri.clone(), iri(y)),
                                (iri(y), op2.term_iri.clone(), iri(z)),
                            ],
                        });
                    }
                }
            }
        }
    }
    out.extend(adversarial_probes(ctx));
    out
}

/// The deterministic ADVERSARIAL probe set: a small, fixed batch of DELIBERATELY ill-formed
/// candidates whose sole purpose is to exercise the rejection filter's recording path over the
/// shipped corpus. Each is a term whose object is a blank node with a RESERVED `__` label — the
/// codec's `is_safe_token_body` rejects it, so the round-trip verifier drops it as
/// `lang:GmnUncoveredTerm` (the SAME uncovered-construct witness `stage-gmn1-gate`'s negative
/// test uses). These are NOT well-typed terms — they are the filter's live falsification, so the
/// corpus records genuine typed rejections rather than a vacuous all-accept.
fn adversarial_probes(ctx: &GenContext) -> Vec<Candidate> {
    let op = ctx.operators[0].term_iri.clone();
    ["probe-one", "probe-two"]
        .into_iter()
        .map(|label| Candidate {
            triples: vec![(
                iri(&format!("{EX}a")),
                op.clone(),
                RdfTerm::BlankNode(format!("adversarial__{label}")),
            )],
        })
        .collect()
}

// ── The five verifiers ─────────────────────────────────────────────────────────────────

/// Which verifier stage a rejection fired at — the typed rejection provenance recorded on a
/// dropped candidate (never a silent drop).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RejectStage {
    Parse,
    RoundTrip,
    Typecheck,
    ProofObligation,
    RingLeak,
}

impl RejectStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "parse",
            Self::RoundTrip => "round-trip",
            Self::Typecheck => "typecheck",
            Self::ProofObligation => "proof-obligation",
            Self::RingLeak => "ring-leak",
        }
    }
}

/// A rejected candidate's typed reason: the verifier stage and the shipped `lang:` failure
/// class IRI (never a free-text reason).
#[derive(Clone, Debug)]
struct Rejection {
    stage: RejectStage,
    failure_class: String,
}

/// The positive acceptance certificate carried on a KEPT training pair.
#[derive(Clone, Debug)]
struct Certificate {
    /// The GMN-0 `input` surface (RDFC-1.0 canonical N-Quads of the term).
    input: String,
    /// The GMN-1 `target` surface (the codec's projection text).
    target: String,
    /// The discharged proof-obligation derivation IRI (the Curry–Howard checked proof object).
    derivation_iri: String,
}

/// A candidate's rejection-sampling outcome.
enum Outcome {
    Kept(Certificate),
    Rejected(Rejection),
}

/// The `lang:` class for a codec round-trip mismatch that is not itself a typed
/// [`Gmn1Error`] variant (the models differ after a clean write/read).
const CLASS_ROUNDTRIP_MISMATCH: &str = "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar";
/// The `lang:` class for a term that fails to typecheck / resolve under the prover.
const CLASS_TYPECHECK_FAIL: &str = "https://blackcatinformatics.ca/lang/GmnTypecheckFailure";
/// The `lang:` class for a term that discharges no proof obligation.
const CLASS_NO_PROOF: &str = "https://blackcatinformatics.ca/lang/GmnProofObligationUndischarged";

/// Rejection-sample one candidate through the five verifiers IN ORDER, short-circuiting at the
/// first failure with its typed reason. All five pass ⇒ [`Outcome::Kept`] with the acceptance
/// certificate. Pure + deterministic given `ctx`.
fn sample(candidate: &Candidate, ctx: &GenContext) -> Outcome {
    let model = candidate.model();

    // Verifier 1 — parses (write then read against the carrier dictionary).
    let doc: Gmn1Document = match gmn1_write(&model, &ctx.dict) {
        Ok(doc) => doc,
        Err(e) => return reject(RejectStage::Parse, e.failure_class()),
    };
    let back: Gmn0Model = match gmn1_read(&doc, &ctx.dict) {
        Ok(back) => back,
        Err(e) => return reject(RejectStage::Parse, e.failure_class()),
    };

    // Verifier 2 — round-trips (canonical N-Quads agreement).
    if !gmn0_canonically_equal(&model, &back) {
        return reject(RejectStage::RoundTrip, CLASS_ROUNDTRIP_MISMATCH);
    }

    // Verifiers 3 + 4 — typechecks and discharges a proof obligation (via the logic-compile IR
    // + the native order-sorted prover).
    let derivation_iri = match typecheck_and_prove(candidate, ctx) {
        ProofResult::Discharged(iri) => iri,
        ProofResult::TypecheckFailed => {
            return reject(RejectStage::Typecheck, CLASS_TYPECHECK_FAIL);
        }
        ProofResult::NoObligation => {
            return reject(RejectStage::ProofObligation, CLASS_NO_PROOF);
        }
    };

    // Verifier 5 — carries no security-ring leakage (Task 9's consume-path filter).
    if let Err(e) = ring_admits(&model, ctx, RING_CORE, RING_TRUSTED) {
        return reject(RejectStage::RingLeak, e.failure_class());
    }

    Outcome::Kept(Certificate {
        input: model.canonical_nquads(),
        target: doc.text,
        derivation_iri,
    })
}

fn reject(stage: RejectStage, failure_class: &str) -> Outcome {
    Outcome::Rejected(Rejection {
        stage,
        failure_class: failure_class.to_owned(),
    })
}

/// Map an RDF term node to its logic-compile IR [`Term`]: an IRI atom is a first-order
/// constant; a blank node (adversarial probes only — they never reach the typecheck, being
/// dropped by the round-trip verifier first) maps to its label as a constant, keeping the
/// translation total.
fn term_of(t: &RdfTerm) -> Term {
    match t {
        RdfTerm::Iri(s) => Term::Iri(s.clone()),
        RdfTerm::BlankNode(b) => Term::Iri(format!("_:{b}")),
        other => Term::Iri(format!("{other:?}")),
    }
}

/// The typecheck + proof-obligation result over the logic-compile IR.
enum ProofResult {
    /// A proof-checked answer's content-addressed derivation IRI (the discharged obligation).
    Discharged(String),
    /// The term failed to lower / resolve under the order-sorted prover (ill-typed).
    TypecheckFailed,
    /// The term resolved but yielded no proof-checked answer (no obligation discharged).
    NoObligation,
}

/// Translate a candidate GMN term into the logic-compile IR and run it through the native
/// order-sorted backward prover ([`evaluate_reasoning_programs`]). Verifier 3 (typecheck) is
/// the successful lowering + order-sorted resolution of the term; verifier 4 (proof obligation)
/// is a proof-CHECKED answer, whose content-addressed derivation IRI is the discharged
/// obligation. The subsort lattice is empty (the atoms carry a single flat sort), so the
/// typecheck here is arity-consistency + first-orderness + proof-checkable resolution — the
/// well-formedness discipline a term algebra's typing judgment IS.
fn typecheck_and_prove(candidate: &Candidate, ctx: &GenContext) -> ProofResult {
    // Each operator triple `(s, p, o)` becomes a binary atomic fact `p(s, o)`.
    let clauses: Vec<Formula> = candidate
        .triples
        .iter()
        .map(|(s, p, o)| Formula::Atom {
            relation: Term::Iri(p.clone()),
            args: vec![term_of(s), term_of(o)],
        })
        .collect();
    // The proof obligation: resolve the first triple as a GROUND goal `p0(s0, o0)` against the
    // fact base. A well-typed term resolves to exactly that ground atom with a checkable proof;
    // an ill-formed one fails to lower or resolve.
    let (s0, p0, o0) = &candidate.triples[0];
    let query = Formula::Atom {
        relation: Term::Iri(p0.clone()),
        args: vec![term_of(s0), term_of(o0)],
    };
    // The atoms' order-sorts seed the order-sorted unification context (constant_sorts).
    let constant_sorts: Vec<(String, String)> = candidate
        .atoms()
        .into_iter()
        .filter_map(|a| ctx.atom_sorts.get(&a).map(|s| (a, s.clone())))
        .collect();
    let program = match ReasoningProgramIr::new(
        format!("{EX}program/{}", candidate_key(candidate)),
        EvaluationMode::Backward,
        clauses,
        query,
        Vec::new(),
        Vec::new(),
        constant_sorts,
    ) {
        Ok(program) => program,
        Err(_) => return ProofResult::TypecheckFailed,
    };
    let evals = match evaluate_reasoning_programs(std::slice::from_ref(&program), &[]) {
        Ok(evals) => evals,
        Err(_) => return ProofResult::TypecheckFailed,
    };
    // Exactly one program in, exactly one evaluation out. A proof-checked answer's derivation
    // IRI is the discharged obligation.
    let Some(eval) = evals.first() else {
        return ProofResult::TypecheckFailed;
    };
    match eval
        .answers
        .iter()
        .find(|a| a.proof_checks)
        .map(|a| a.derivation_iri.clone())
    {
        Some(iri) => ProofResult::Discharged(iri),
        None => ProofResult::NoObligation,
    }
}

/// Run Task 9's consume-path filter over the term's atoms tagged at `content_ring`, admitting
/// into `target_ring`. A clean admission is `Ok`; any ring-leak / lattice condition is the
/// typed [`GmnConsumeError`]. Used by verifier 5 (all atoms at `gmnRingCore`, admitted into
/// `gmnRingTrusted`) and by the negative test (an atom at `gmnRingRestricted`).
fn ring_admits(
    model: &Gmn0Model,
    ctx: &GenContext,
    content_ring: &str,
    target_ring: &str,
) -> Result<ConsumeProjection, GmnConsumeError> {
    consume_project(
        &tag_all_atoms(model, content_ring),
        &ctx.lattice,
        target_ring,
        None,
        &ctx.dict,
    )
}

/// Tag every atom (subject AND object) of a model with a single `gmeow:gmnContentRing` at
/// `ring`, returning the tagged model the consume filter classifies. The tags are the content
/// classification the ring filter reads; they ride only the admissibility check, never the
/// certificate's `input` surface.
fn tag_all_atoms(model: &Gmn0Model, ring: &str) -> Gmn0Model {
    let mut atoms: Vec<String> = Vec::new();
    for quad in &model.quads {
        if let RdfTerm::Iri(s) = &quad.subject {
            atoms.push(s.clone());
        }
        if let RdfTerm::Iri(o) = &quad.object {
            atoms.push(o.clone());
        }
    }
    atoms.sort();
    atoms.dedup();
    let mut builder = RdfDatasetBuilder::new();
    for quad in &model.quads {
        builder.push_owned_quad(quad);
    }
    for atom in atoms {
        builder.push_owned_quad(&RdfQuad::new(
            RdfTerm::Iri(atom),
            PRED_CONTENT_RING,
            RdfTerm::Iri(ring.to_owned()),
        ));
    }
    let ds = builder.freeze().expect("tagged model freezes cleanly");
    Gmn0Model::from_dataset(&ds)
}

/// A stable, filesystem-safe key for a candidate (its triples joined) — the suffix of every
/// minted per-term IRI, so two distinct terms never collide and the corpus is deterministic.
fn candidate_key(candidate: &Candidate) -> String {
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    for (s, p, o) in &candidate.triples {
        sha2::Digest::update(&mut hasher, term_bytes(s).as_bytes());
        sha2::Digest::update(&mut hasher, b"\x1f");
        sha2::Digest::update(&mut hasher, p.as_bytes());
        sha2::Digest::update(&mut hasher, b"\x1f");
        sha2::Digest::update(&mut hasher, term_bytes(o).as_bytes());
        sha2::Digest::update(&mut hasher, b"\x1e");
    }
    let digest = sha2::Digest::finalize(hasher);
    hex_lower(&digest[..8])
}

/// A term's stable string form for the candidate key (an IRI by itself; a blank node with a
/// `_:` prefix), so two distinct candidates never share a key.
fn term_bytes(t: &RdfTerm) -> String {
    match t {
        RdfTerm::Iri(s) => s.clone(),
        RdfTerm::BlankNode(b) => format!("_:{b}"),
        other => format!("{other:?}"),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ── Corpus assembly (the certificate graph) ────────────────────────────────────────────

/// Build the corpus named graph: iterate the enumerated candidates in order, rejection-sample
/// each, and emit — for a KEPT pair — a `gmeow:GmnTrainingExample` carrying task/input/target,
/// the five verdicts, the discharged proof-obligation derivation IRI, and the version quad;
/// for a REJECTED candidate — a `gmeow:GmnRejectedCandidate` carrying its typed reason. Returns
/// the quads (all in [`GRAPH_GMN_TRAINING_CORPUS`]) plus the kept/rejected counts.
fn build_corpus(ctx: &GenContext) -> (Vec<RdfQuad>, usize, usize) {
    let schema_version = resolved_schema_version(&ctx.dict);
    let mut quads: Vec<RdfQuad> = Vec::new();
    let mut kept = 0usize;
    let mut rejected = 0usize;
    for (index, candidate) in enumerate(ctx).into_iter().enumerate() {
        match sample(&candidate, ctx) {
            Outcome::Kept(cert) => {
                emit_kept(&mut quads, index, &cert, &schema_version);
                kept += 1;
            }
            Outcome::Rejected(reason) => {
                emit_rejected(&mut quads, index, &reason);
                rejected += 1;
            }
        }
    }
    (quads, kept, rejected)
}

fn iri_quad(subject: &str, predicate: &str, object: &str) -> RdfQuad {
    let mut quad = RdfQuad::new(
        RdfTerm::Iri(subject.to_owned()),
        predicate.to_owned(),
        RdfTerm::Iri(object.to_owned()),
    );
    quad.graph_name = Some(RdfTerm::Iri(GRAPH_GMN_TRAINING_CORPUS.to_owned()));
    quad
}

fn str_quad(subject: &str, predicate: &str, lexical: &str, datatype: Option<&str>) -> RdfQuad {
    let mut quad = RdfQuad::new(
        RdfTerm::Iri(subject.to_owned()),
        predicate.to_owned(),
        RdfTerm::Literal(RdfLiteral {
            lexical_form: lexical.to_owned(),
            datatype: datatype.map(str::to_owned),
            language: None,
            direction: None,
        }),
    );
    quad.graph_name = Some(RdfTerm::Iri(GRAPH_GMN_TRAINING_CORPUS.to_owned()));
    quad
}

/// Emit the acceptance certificate for one kept training pair.
fn emit_kept(quads: &mut Vec<RdfQuad>, index: usize, cert: &Certificate, schema_version: &str) {
    let subject = format!("{EX}example-{index}");
    quads.push(iri_quad(
        &subject,
        RDF_TYPE,
        &format!("{GMEOW}GmnTrainingExample"),
    ));
    // task / input / target — the paired training example.
    quads.push(str_quad(
        &subject,
        &format!("{GMEOW}gmnTrainingTask"),
        "verbalize",
        None,
    ));
    quads.push(str_quad(
        &subject,
        &format!("{GMEOW}gmnTrainingInput"),
        &cert.input,
        None,
    ));
    quads.push(str_quad(
        &subject,
        &format!("{GMEOW}gmnTrainingTarget"),
        &cert.target,
        None,
    ));
    // The five verdicts (all true on a kept pair).
    for verdict in [
        "gmnVerdictParsed",
        "gmnVerdictRoundTrip",
        "gmnVerdictTypechecked",
        "gmnVerdictProofObligation",
        "gmnVerdictRingClean",
    ] {
        quads.push(str_quad(
            &subject,
            &format!("{GMEOW}{verdict}"),
            "true",
            Some(XSD_BOOLEAN),
        ));
    }
    // The discharged proof obligation (the Curry–Howard checked derivation).
    quads.push(iri_quad(
        &subject,
        &format!("{GMEOW}gmnTrainingProofDerivation"),
        &cert.derivation_iri,
    ));
    // Version provenance (Task 4): exactly one gmnSchemaVersion per kept pair.
    quads.push(str_quad(
        &subject,
        gmeow_lang_bridge::PRED_GMN_SCHEMA_VERSION,
        schema_version,
        Some("http://www.w3.org/2001/XMLSchema#string"),
    ));
}

/// Record one rejected candidate with its TYPED reason (never a silent drop).
fn emit_rejected(quads: &mut Vec<RdfQuad>, index: usize, reason: &Rejection) {
    let subject = format!("{EX}rejected-{index}");
    quads.push(iri_quad(
        &subject,
        RDF_TYPE,
        &format!("{GMEOW}GmnRejectedCandidate"),
    ));
    quads.push(str_quad(
        &subject,
        &format!("{GMEOW}gmnRejectionStage"),
        reason.stage.as_str(),
        None,
    ));
    quads.push(iri_quad(
        &subject,
        &format!("{GMEOW}gmnRejectionReason"),
        &reason.failure_class,
    ));
}

// ── The stage ──────────────────────────────────────────────────────────────────────────

/// The `gmn-training-corpus` pipeline stage.
pub struct GmnTrainingCorpusStage {
    consumes: Vec<String>,
}

impl GmnTrainingCorpusStage {
    /// Construct the stage. It consumes `stage-compile-logic` (the typechecker/prover lane) AND
    /// `stage-mappings` (the projected GMN forms / glyph registry lane) — the two producers
    /// whose products the corpus is a function of. The edge is declared identically here, in
    /// `slices/core/pipeline/module.ttl`, and in [`crate::run::full_spec`].
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-mappings".to_string(),
            ],
        }
    }
}

impl Default for GmnTrainingCorpusStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for GmnTrainingCorpusStage {
    fn id(&self) -> &str {
        "stage-gmn-training-corpus"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        // v1: the rejection-sampled, proof-carrying GMN training-corpus emitter.
        "gmn-training-corpus.v1"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // The signature is resolved from the three grounding module surfaces; declare them for
        // byte-level cache soundness (a glyph/label edit re-keys the corpus).
        Ok(GROUNDING_MODULES.iter().map(|rel| root.join(rel)).collect())
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let ctx = GenContext::resolve(input.root)?;
        let (quads, kept, _rejected) = build_corpus(&ctx);
        if kept == 0 {
            return Err(stage_err(
                "the productive functor kept ZERO training pairs — every enumerated well-typed \
                 term was rejected, so the emitted corpus would be vacuous (corrupt signature \
                 or codec/prover regression)",
            ));
        }
        let mut builder = RdfDatasetBuilder::new();
        for quad in &quads {
            builder.push_owned_quad(quad);
        }
        let dataset = builder
            .freeze()
            .map_err(|e| stage_err(format!("freeze corpus dataset: {e}")))?;
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            dataset,
            BTreeMap::new(),
        )))
    }
}
