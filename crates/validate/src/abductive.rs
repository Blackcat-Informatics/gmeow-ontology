// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Abductive advice producer (D5) — the constructive "what to ADD" wing.
//!
//! A GENERIC producer driven entirely by the authored `logic:AbductiveSchema`
//! vocabulary (`slices/grounding/logic/module.ttl`). It discovers every schema by
//! SPARQL, reads each schema's `logic:completenessFormula` structure off the graph,
//! enumerates the minimal candidate additions that would complete an under-specified
//! subject, and asks the native conjecture engine
//! ([`gmeow_logic::conjecture::conjecture_test`]) whether a candidate addition makes
//! the discipline's completeness condition hold in an ISOLATED, consistent scenario
//! world. Only a `Corroborated` verdict warrants the emitted advisory — an `Open` or
//! `RefutedInStandpoint` answer is honest absence (no suggestion). A verdict cut short by
//! BUDGET EXHAUSTION is neither: it is a genuinely inconclusive could-not-decide, distinct
//! from a decided `Open`, so it never silently vanishes — [`abductive_advisories`] returns
//! it as its own [`AbductiveOutcome::exhausted`] diagnostic (Part B of G6), never folded
//! into the same honest absence as a real non-corroboration.
//!
//! There is NO hardcoded discipline list and NO hardcoded predicate string: the guard
//! type, the required relata predicates, and the candidate sortal types are all read
//! from the reconstructed completeness formula, so registering a further discipline of
//! an existing strategy is a data mark in the logic module, never new code here.
//!
//! # The completeness warrant is a conjunction of ground-head Horn corroborations
//!
//! The native relational-core lowering makes a **ground-head** Horn implication
//! `τ(s) → r(s, o)` a genuine discriminator: over the restricted chase the rule adds
//! `r(s, o)` when it is absent (a NEW triple ⇒ the candidate is not redundant ⇒ `Open`)
//! and adds nothing when it is already present or already entailed (redundant ⇒
//! `Supported` ⇒ `Corroborated`), and a clash with the subject's other assertions lands
//! `RefutedInStandpoint`. (An *existential*-headed implication `∃v. (τ(s) → r(s, v))` is
//! NOT a discriminator: its Skolem witness is invented outside the counted closure, so it
//! corroborates unconditionally — hence the completeness is decomposed into ground atoms,
//! never left existential.)
//!
//! So a relator-mediation completeness (a conjunction of relata) is warranted PER
//! CANDIDATE, per conjunct: each missing relatum is its own independently-warranted
//! candidate, tested against ITS OWN ground Horn `τ_guard(s) → r_i(s, value_i)` where
//! `value_i` is the fresh witness for THAT relatum — never against the whole conjunction.
//! A one-party `gmeow:Commitment` (two of three relata missing) therefore yields ONE
//! candidate per missing relatum, each warranted independently; the other relata being
//! still missing no longer blocks either candidate's warrant (per-conjunct completeness:
//! "adding this relatum is consistent with the guard type", not "all others are already
//! present"). The full completeness conjunction is still the discipline's authored TARGET
//! (`logic:relatorMediationCons` names all three relata) — only the per-candidate
//! WARRANTING is scoped to the one conjunct being added.
//!
//! A sortal specialization (a disjunction) is warranted PER SUBJECT, across every offered
//! disjunct together, never per candidate in isolation. Each disjunct `τ_Entity(s) → τ_T(s)`
//! lands one of `Corroborated` (adding `T` is consistent), `RefutedInStandpoint` (adding `T`
//! clashes with `s`'s other assertions), `Open` (honest absence), or budget-exhausted (its
//! own could-not-decide diagnostic, see above). A bare
//! subject typed nothing but its guard class corroborates EVERY offered sortal — advising a
//! "specialize to X" menu across all of them is then non-discriminating noise, not advice,
//! so it is SUPPRESSED entirely (honest absence, not a same-weight N-way menu). Only when the
//! subject's own assertions REFUTE at least one offered sortal does the completeness
//! disjunction genuinely constrain the choice; advice is then emitted for the CORROBORATED
//! remainder (∨-introduction: one true disjunct satisfies the completeness disjunction), the
//! refuted disjunct(s) excluded. The base graph is never mutated — the hypothetical lives
//! only in the borrowed scenario EDB — so nothing is auto-asserted.

use gmeow_errors::{Diag, register_code};
use gmeow_logic::conjecture::{ConjectureLifecycleState, conjecture_test};
use gmeow_logic::query_ir::Budget;
use gmeow_logic::result::EvaluationStatus;
use gmeow_logic_compile::frontend::reconstruct_formula;
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::{DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermRef};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::advisory::{Advisory, BEST_PRACTICE_STANDPOINT_IRI};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_THING: &str = "http://www.w3.org/2002/07/owl#Thing";
/// The four-boxes assertional-tier markers. Abductive advice is only for genuine,
/// under-specified A-Box INDIVIDUALS — a subject carrying `gmeow:graphBoxRole gmeow:boxABox`
/// (`crates/errors/src/abox.rs::{GRAPH_BOX_ROLE, BOX_ABOX}`) — never a TBox class/property
/// term nor an entailed phantom the guard closure otherwise sweeps in.
const GMEOW_GRAPH_BOX_ROLE: &str = "https://blackcatinformatics.ca/gmeow/graphBoxRole";
const GMEOW_BOX_ABOX: &str = "https://blackcatinformatics.ca/gmeow/boxABox";
/// The canonical source language every authored guidance literal carries; the public
/// `@en`/`@zh`/`@fr` projections are never the surfaced text (mirrors `advisory.rs`).
const ADVICE_SOURCE_LANG: &str = "x-gmeow-english";
const GMEOW_HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/howToUse";

// ── Strategy IRIs (the closed `logic:AbductiveRepairStrategy` value class) ───────────
const STRATEGY_RELATUM: &str = "https://blackcatinformatics.ca/logic/StrategyRelatumCompletion";
const STRATEGY_CHAIN: &str = "https://blackcatinformatics.ca/logic/StrategyChainCompletion";
const STRATEGY_SORTAL: &str = "https://blackcatinformatics.ca/logic/StrategySortalSpecialization";
const STRATEGY_FRAME: &str = "https://blackcatinformatics.ca/logic/StrategyFrameDeclaration";

/// Base IRI for a content-addressed witness individual (a fresh relatum / frame /
/// manifestation the candidate addition points at). Deterministic — a blake-free
/// SHA-256 digest of `subject + '\u{1f}' + predicate`, never a clock/random source.
const WITNESS_BASE: &str = "https://blackcatinformatics.ca/gmeow/abductive/witness/";
/// Base IRI for the content-addressed isolated scenario world a candidate is tested in.
const SCENARIO_WORLD_BASE: &str = "https://blackcatinformatics.ca/gmeow/abductive/scenario/";

/// The abductive engine's step ceiling — ONE named constant shared by both live callers
/// (the pipeline `stage-validate` and the `gmeow` CLI's `validate_all`), never a
/// duplicated magic literal. Each `conjecture_test` runs over an ISOLATED, per-candidate
/// scenario world (`KB ∪ {one guard atom, one candidate relatum/sortal atom}`) — the
/// SAME kernel+logic KB closure every time, so the marginal chase this ceiling bounds is
/// tiny relative to the limit: generous headroom for the worst case, never a tuned-to-fit
/// number.
pub const ABDUCTIVE_MAX_STEPS: u64 = 5_000_000;

/// The shared abductive-engine [`Budget`] — construct it here so both call sites reference
/// the SAME named item (see [`ABDUCTIVE_MAX_STEPS`]) rather than duplicating the literal.
/// `max_answers: None` — the answer-count axis is not the discriminator for a per-candidate
/// warrant test (a single ground/Horn candidate, never an open-ended answer set); only the
/// step ceiling bounds the chase.
#[must_use]
pub fn abductive_budget() -> Budget {
    Budget {
        max_answers: None,
        max_steps: Some(ABDUCTIVE_MAX_STEPS),
    }
}

/// One abductive suggestion: the engine-corroborated warrant [`Diag`] paired with the
/// dual-projection-ready [`Advisory`]. The two carry the SAME content-addressed digest
/// in their codes, so a consumer can join the warrant to the advice it warrants.
#[derive(Debug)]
pub struct AbductiveSuggestion {
    /// The warrant note summarising the native conjecture corroboration.
    pub warrant: Diag,
    /// The best-practice advisory recommending the concrete addition.
    pub advisory: Advisory,
}

/// The full output of [`abductive_advisories`]: every engine-corroborated suggestion AND,
/// when a candidate/subject's warrant test was cut short by budget exhaustion (as opposed
/// to a genuine `Open`/`RefutedInStandpoint` verdict), an honest "could-not-decide (budget
/// exhausted)" [`Diag`] — so a subject dropped because its warrant ran out of budget stays
/// OBSERVABLE in `graph/diagnostics`, never a silent vanish indistinguishable from genuine
/// non-corroboration. Deterministic: `exhausted` is sorted by its content-addressed code,
/// exactly like `suggestions` is sorted by `(advisory.code, advisory.subject_iri)`.
#[derive(Debug, Default)]
pub struct AbductiveOutcome {
    /// Every engine-corroborated candidate addition, byte-sorted (see [`abductive_advisories`]).
    pub suggestions: Vec<AbductiveSuggestion>,
    /// One diagnostic per candidate/subject whose warrant test exhausted its budget before
    /// reaching a conclusive verdict.
    pub exhausted: Vec<Diag>,
}

/// A discovered `logic:AbductiveSchema` and the completeness structure read off its
/// `logic:completenessFormula`.
struct DiscoveredSchema {
    /// The governed term (`logic:formalizes`) — the advice's provenance.
    term: String,
    /// The `logic:repairStrategy` IRI.
    strategy: String,
    /// The completeness guard: the antecedent atom every gap subject must satisfy.
    guard: Guard,
    /// The completeness consequent, classified.
    cons: ConsShape,
}

/// The completeness guard atom `τ_guard(this)` — the antecedent of every schema's
/// `∀"this". (τ_guard(this) → Cons)`. Read off the graph as data (never a hardcoded
/// predicate switch), so a further discipline is a mark in the logic module, not new code.
enum Guard {
    /// `rdf:type(this, Class)` — the gap subject must be typed `Class` (relator-mediation,
    /// WEMI-chain, reference-frame, sortal disciplines).
    Type(String),
    /// `relation(this, ?guardVar)` (relation ≠ `rdf:type`) — the gap subject must be the
    /// subject of some `relation` triple (measurement-frame: a value carrying `logic:unit`).
    Property(String),
}

/// The consequent shape of a completeness formula.
enum ConsShape {
    /// A conjunction (or singleton) of relatum atoms `r(this, ?v)`: `(relation, objVar)`
    /// pairs. Relator-mediation, WEMI-chain, and reference-frame disciplines.
    Conjunctive(Vec<(String, String)>),
    /// A disjunction of sortal-type atoms `rdf:type(this, T)`: the fixed candidate types.
    Disjunctive(Vec<String>),
}

/// A single candidate addition read off the graph structure.
struct Candidate {
    subject: String,
    /// The predicate of the addition (a relatum relation, or `rdf:type` for a sortal).
    predicate: String,
    /// The object of the addition (a fresh witness IRI, or the sortal type IRI).
    object: String,
    /// A concrete-element description for the corrective suggestion.
    element: String,
}

/// Produce every engine-warranted abductive advisory over `reasoned`.
///
/// `reasoned` must be the graph the caller has already threaded with whatever reasoning
/// its surface can carry — the producer reads types/relata with `GraphMatch::Any`, so a
/// subject/relatum is discovered whether it is ASSERTED or ENTAILED, but only if the
/// caller actually supplies the entailed triples. The two live callers do:
///   * the pipeline `stage-validate` feeds the UNION of the authored source graph (the
///     A-Box/TBox individuals + their asserted types/relata) AND the derived closure read
///     off the consumed `stage-reason` product's typed Reasoning handle — so entailed-only
///     subjects/relata are seen (the maximal-ontological-use surface);
///   * the `gmeow` CLI feeds the dataset it built: a validated `gmeow.gts` bundle carries
///     the folded reasoned closure (entailed), while a raw-source run carries only the
///     asserted graph (no reasoner is fabricated for loose files — that surface is honestly
///     asserted-only, never a silently-degraded stand-in for entailment).
///
/// The authored half is mandatory on every surface: without the asserted A-Box the schema
/// guards match ZERO subjects, so a closure-only input would find nothing.
///
/// Deterministic: schemas are discovered in byte-sorted order, candidates are enumerated
/// byte-sorted, all witness/world IRIs are content-addressed, and the returned vector is
/// finally sorted by `(advisory.code, advisory.subject_iri)`. Same input twice ⇒ identical
/// output. `reasoned` is only READ; the base graph is never mutated.
#[must_use]
pub fn abductive_advisories(reasoned: &RdfDataset, budget: &Budget) -> AbductiveOutcome {
    let mut suggestions = Vec::new();
    // Keyed by the same content-addressed code the diag itself carries, so the final sort
    // is deterministic without re-deriving the key from the (registry-order-dependent)
    // `Diag::code()` handle.
    let mut exhausted: Vec<(String, Diag)> = Vec::new();
    for schema in discover_schemas(reasoned) {
        match schema.strategy.as_str() {
            // The sortal/disjunctive strategy is gated PER SUBJECT across all its offered
            // disjuncts together (the non-discriminating-menu suppression), so it builds its
            // own suggestions rather than going through the generic per-candidate `warrant`.
            STRATEGY_SORTAL => {
                let (subject_suggestions, subject_exhausted) =
                    sortal_suggestions(reasoned, &schema, budget);
                suggestions.extend(subject_suggestions);
                exhausted.extend(subject_exhausted);
            }
            STRATEGY_RELATUM | STRATEGY_CHAIN | STRATEGY_FRAME => {
                for candidate in relatum_candidates(reasoned, &schema) {
                    match warrant(reasoned, &schema, &candidate, budget) {
                        WarrantOutcome::Corroborated(suggestion) => suggestions.push(*suggestion),
                        WarrantOutcome::Exhausted(key, diag) => exhausted.push((key, diag)),
                        WarrantOutcome::Other => {}
                    }
                }
            }
            // An unknown strategy is honest absence, never new hidden behaviour.
            _ => {}
        }
    }
    suggestions.sort_by(|a, b| {
        a.advisory
            .code
            .cmp(&b.advisory.code)
            .then_with(|| a.advisory.subject_iri.cmp(&b.advisory.subject_iri))
    });
    exhausted.sort_by(|a, b| a.0.cmp(&b.0));
    AbductiveOutcome {
        suggestions,
        exhausted: exhausted.into_iter().map(|(_, diag)| diag).collect(),
    }
}

// ── Schema discovery (SPARQL) ────────────────────────────────────────────────────────

/// Discover every `logic:AbductiveSchema` and reconstruct each completeness formula's
/// structure. Mirrors the SELECT
/// `?schema a logic:AbductiveSchema ; logic:formalizes ?term ; logic:repairStrategy
/// ?strategy ; logic:completenessFormula ?completeness` as a pattern scan (the
/// `advisory.rs` precedent — no `Arc` round-trip). Byte-sorted by schema IRI.
fn discover_schemas(reasoned: &RdfDataset) -> Vec<DiscoveredSchema> {
    let mut schema_iris = subjects_of_type(reasoned, &format!("{LOGIC}AbductiveSchema"));
    schema_iris.sort();
    schema_iris.dedup();

    let mut schemas = Vec::new();
    for schema_iri in schema_iris {
        let (Some(term), Some(strategy), Some(completeness)) = (
            first_object(reasoned, &schema_iri, &format!("{LOGIC}formalizes")),
            first_object(reasoned, &schema_iri, &format!("{LOGIC}repairStrategy")),
            first_object(
                reasoned,
                &schema_iri,
                &format!("{LOGIC}completenessFormula"),
            ),
        ) else {
            // A schema missing any required edge is honest absence, never a partial guess.
            continue;
        };
        // The completeness root is deliberately kept out of the top-level formula set; it
        // reconstructs off the reasoned RDF. A malformed / missing tree is honest absence.
        let Ok(formula) = reconstruct_formula(reasoned, &completeness) else {
            continue;
        };
        let Some((guard, cons)) = deconstruct(&formula) else {
            continue;
        };
        schemas.push(DiscoveredSchema {
            term,
            strategy,
            guard,
            cons,
        });
    }
    schemas
}

// ── Completeness-formula deconstruction ──────────────────────────────────────────────

/// Read `(Guard, ConsShape)` off a completeness formula
/// `∀"this". (τ_guard(this) → Cons)`. Returns `None` for any shape that is not the
/// authored guard→consequent form (honest absence).
fn deconstruct(formula: &Formula) -> Option<(Guard, ConsShape)> {
    let body = match formula {
        Formula::Forall { vars, body } if vars.len() == 1 && vars[0] == "this" => body.as_ref(),
        _ => return None,
    };
    let Formula::Implies(antecedent, consequent) = body else {
        return None;
    };
    let guard = guard_atom(antecedent)?;

    // The consequent is either a disjunction of sortal-type atoms, a conjunction of
    // relatum atoms, or a single relatum atom.
    let cons = match consequent.as_ref() {
        Formula::Or(disjuncts) => {
            let mut types = Vec::new();
            for disjunct in disjuncts {
                types.push(fixed_type_object(disjunct)?);
            }
            types.sort();
            types.dedup();
            ConsShape::Disjunctive(types)
        }
        Formula::And(conjuncts) => {
            let mut relata = Vec::new();
            for conjunct in conjuncts {
                relata.push(relatum_atom(conjunct)?);
            }
            ConsShape::Conjunctive(relata)
        }
        atom @ Formula::Atom { .. } => ConsShape::Conjunctive(vec![relatum_atom(atom)?]),
        _ => return None,
    };
    Some((guard, cons))
}

/// Read a guard atom off the completeness antecedent:
///   * `rdf:type(this, Class)`  → [`Guard::Type`] (relator/WEMI/frame/sortal, UNCHANGED); or
///   * `relation(this, ?var)` with `relation ≠ rdf:type` → [`Guard::Property`] (a
///     property-presence guard, e.g. `logic:unit(this, ?unitValue)` for the measurement-frame
///     discipline).
///
/// Any other shape is honest absence (`None`) — the guard reading is data, never a hardcoded
/// predicate switch.
fn guard_atom(formula: &Formula) -> Option<Guard> {
    let (relation, args) = as_binary_atom(formula)?;
    match (&args.0, &args.1) {
        (Term::Var(v), Term::Iri(class)) if v == "this" && relation == RDF_TYPE => {
            Some(Guard::Type(class.clone()))
        }
        (Term::Var(v), Term::Var(_)) if v == "this" && relation != RDF_TYPE => {
            Some(Guard::Property(relation))
        }
        _ => None,
    }
}

/// A relatum atom `r(this, ?objVar)` → `(relation, objVar)`.
fn relatum_atom(formula: &Formula) -> Option<(String, String)> {
    let (relation, args) = as_binary_atom(formula)?;
    match (&args.0, &args.1) {
        (Term::Var(s), Term::Var(obj)) if s == "this" => Some((relation, obj.clone())),
        _ => None,
    }
}

/// A sortal-type disjunct `rdf:type(this, T)` → `T`.
fn fixed_type_object(formula: &Formula) -> Option<String> {
    let (relation, args) = as_binary_atom(formula)?;
    if relation != RDF_TYPE {
        return None;
    }
    match (&args.0, &args.1) {
        (Term::Var(s), Term::Iri(t)) if s == "this" => Some(t.clone()),
        _ => None,
    }
}

/// Split a binary [`Formula::Atom`] into `(relationIri, (arg0, arg1))`.
fn as_binary_atom(formula: &Formula) -> Option<(String, (Term, Term))> {
    match formula {
        Formula::Atom { relation, args } if args.len() == 2 => {
            let Term::Iri(rel) = relation else {
                return None;
            };
            Some((rel.clone(), (args[0].clone(), args[1].clone())))
        }
        _ => None,
    }
}

// ── Candidate enumeration (read off the graph structure, byte-sorted) ─────────────────

/// Relatum-completion candidates (relator-mediation, WEMI-chain, reference-frame): a
/// gap subject typed the guard class that is MISSING at least one of the declared
/// relata. One candidate per missing relatum, a fresh content-addressed witness object.
fn relatum_candidates(reasoned: &RdfDataset, schema: &DiscoveredSchema) -> Vec<Candidate> {
    let ConsShape::Conjunctive(relata) = &schema.cons else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for subject in guard_subjects(reasoned, &schema.guard) {
        // The relata missing on this subject (byte-sorted by relation IRI).
        let mut missing: Vec<&String> = relata
            .iter()
            .map(|(relation, _)| relation)
            .filter(|relation| !has_object(reasoned, &subject, relation))
            .collect();
        missing.sort();
        missing.dedup();
        // Already complete ⇒ no candidate (honest absence).
        for relation in missing {
            let object = witness_iri(&subject, relation);
            candidates.push(Candidate {
                subject: subject.clone(),
                predicate: relation.clone(),
                object,
                element: relatum_element(relation),
            });
        }
    }
    candidates.sort_by(|a, b| {
        a.subject
            .cmp(&b.subject)
            .then_with(|| a.predicate.cmp(&b.predicate))
    });
    candidates
}

/// Sortal-specialization candidates: a gap subject typed the guard class (`gmeow:Entity`)
/// that holds NONE of the completeness disjunction's sortal types. One candidate per
/// offered sortal type (byte-sorted). This is the STRUCTURAL enumeration only — whether a
/// candidate is actually warranted (and whether the subject's candidates are emitted at
/// all) is the per-subject engine gate in [`sortal_suggestions`].
fn sortal_candidates(reasoned: &RdfDataset, schema: &DiscoveredSchema) -> Vec<Candidate> {
    let ConsShape::Disjunctive(types) = &schema.cons else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for subject in guard_subjects(reasoned, &schema.guard) {
        // Bare ⇒ holds none of the offered sortal types.
        let already_specialized = types.iter().any(|t| has_type(reasoned, &subject, t));
        if already_specialized {
            continue;
        }
        for sortal in types {
            candidates.push(Candidate {
                subject: subject.clone(),
                predicate: RDF_TYPE.to_owned(),
                object: sortal.clone(),
                element: qname(sortal),
            });
        }
    }
    candidates.sort_by(|a, b| {
        a.subject
            .cmp(&b.subject)
            .then_with(|| a.object.cmp(&b.object))
    });
    candidates
}

// ── The engine warrant ───────────────────────────────────────────────────────────────

/// The three-way disposition a single candidate's warrant test lands in.
enum WarrantOutcome {
    /// The formula was `Corroborated` — the paired warrant + advisory. Boxed: at ~240 bytes
    /// this variant otherwise dwarfs its siblings (clippy::large_enum_variant).
    Corroborated(Box<AbductiveSuggestion>),
    /// The engine's budget was exhausted before it reached a conclusive verdict — the
    /// content-addressed sort key (mirrors [`Advisory::code`]) paired with the honest
    /// "could-not-decide" [`Diag`].
    Exhausted(String, Diag),
    /// `Open` / `RefutedInStandpoint` / an engine error — honest absence, no diagnostic.
    Other,
}

/// Test `candidate` against ITS OWN per-conjunct ground-head Horn (relator mediation /
/// WEMI chain / reference frame) in an isolated, content-addressed scenario world, and
/// build the paired warrant + advisory when the native engine corroborates that single
/// conjunct. A verdict cut short by budget exhaustion surfaces as
/// [`WarrantOutcome::Exhausted`] (an honest could-not-decide diagnostic, never a false
/// advisory); any other non-corroborating verdict (Open / RefutedInStandpoint) or engine
/// error is [`WarrantOutcome::Other`] — honest absence, no panic.
///
/// Per-conjunct completeness: the candidate's own predicate/object IS the ground value for
/// the relatum being added, so only `τ_guard(subject) → predicate(subject, object)` is
/// tested — never the whole completeness conjunction. A relatum that is STILL missing on
/// the subject (a different conjunct than the one this candidate adds) no longer blocks
/// this candidate's warrant: each missing relatum is its own independently-warranted
/// candidate (see the module doc "The completeness warrant …" section).
///
/// The sortal/disjunctive strategy never reaches this function — it is warranted through
/// the per-subject gate in [`sortal_suggestions`] instead, so a non-conjunctive schema
/// (which should never be dispatched here — see `abductive_advisories`) is honest absence.
fn warrant(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
    candidate: &Candidate,
    budget: &Budget,
) -> WarrantOutcome {
    let ConsShape::Conjunctive(_) = &schema.cons else {
        return WarrantOutcome::Other;
    };
    // The grounded guard antecedent for this subject. A property guard whose subject has no
    // IRI guard value cannot warrant (honest absence, no panic) — mirrors the file's other
    // None-handling.
    let Some(guard_antecedent) = guard_antecedent(reasoned, &candidate.subject, &schema.guard)
    else {
        return WarrantOutcome::Other;
    };
    let scenario_world = scenario_world_iri(candidate);
    let assume = vec![(
        candidate.subject.clone(),
        candidate.predicate.clone(),
        candidate.object.clone(),
    )];

    let formula = ground_relatum_formula(
        guard_antecedent,
        &candidate.subject,
        &candidate.predicate,
        &candidate.object,
    );
    match engine_verdict(reasoned, &scenario_world, &formula, &assume, budget) {
        EngineVerdict::Corroborated => WarrantOutcome::Corroborated(Box::new(build_suggestion(
            reasoned,
            schema,
            candidate,
            &scenario_world,
        ))),
        EngineVerdict::Exhausted => {
            let (key, diag) = exhaustion_diag(schema, candidate, &scenario_world);
            WarrantOutcome::Exhausted(key, diag)
        }
        EngineVerdict::Refuted | EngineVerdict::Other => WarrantOutcome::Other,
    }
}

/// Sortal-specialization suggestions, gated PER SUBJECT across every offered disjunct
/// together — the non-discriminating-menu suppression. For each subject with candidates
/// (from [`sortal_candidates`]), every offered sortal is tested through the native engine;
/// when NONE lands `RefutedInStandpoint` the subject's own assertions do not constrain the
/// choice at all, so a same-weight N-way "specialize to X" menu would be noise and is
/// SUPPRESSED entirely (honest absence). When AT LEAST ONE disjunct is refuted, the
/// completeness disjunction genuinely discriminates: advice is emitted for the
/// `Corroborated` remainder only (the refuted — and any merely `Open` — disjuncts are
/// excluded). A disjunct whose own warrant test was budget-exhausted ALWAYS surfaces its
/// own could-not-decide diagnostic (the second return element), independent of whether the
/// subject's menu suppression fires — an exhausted candidate must never vanish silently.
fn sortal_suggestions(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
    budget: &Budget,
) -> (Vec<AbductiveSuggestion>, Vec<(String, Diag)>) {
    let candidates = sortal_candidates(reasoned, schema);
    let mut suggestions = Vec::new();
    let mut exhausted = Vec::new();
    let mut start = 0;
    while start < candidates.len() {
        let mut end = start + 1;
        while end < candidates.len() && candidates[end].subject == candidates[start].subject {
            end += 1;
        }
        let (subject_suggestions, subject_exhausted) =
            sortal_suggestions_for_subject(reasoned, schema, &candidates[start..end], budget);
        suggestions.extend(subject_suggestions);
        exhausted.extend(subject_exhausted);
        start = end;
    }
    (suggestions, exhausted)
}

/// The per-subject sortal gate: test every disjunct in `candidates` (all for the SAME
/// subject) and emit the `Corroborated` remainder only when at least one disjunct is
/// `RefutedInStandpoint`; otherwise (nothing refuted — a non-discriminating menu, or every
/// disjunct merely `Open`) return no suggestions, honest absence. Independently, every
/// disjunct whose OWN test was budget-exhausted contributes its own could-not-decide
/// diagnostic (the second return element) regardless of the suggestion gate's outcome.
fn sortal_suggestions_for_subject(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
    candidates: &[Candidate],
    budget: &Budget,
) -> (Vec<AbductiveSuggestion>, Vec<(String, Diag)>) {
    // F1 short-circuit: a sortal subject emits advice only when >=1 offered top-sortal lands
    // `RefutedInStandpoint`; a refutation can only happen when the subject carries a reasoned
    // rdf:type DISJOINT with an offered sortal. Every offered sortal is a subclass of the
    // guard class, so the guard class itself, ITS superclasses, and owl:Thing can never be
    // disjoint with any of them — a subject whose ONLY reasoned types are those cannot refute
    // ANY disjunct, so by F1's own rule it is SUPPRESSED. Detect that cheaply off the REASONED
    // type set (domain/range- and subclass-induced types are already materialized there) and
    // return empty WITHOUT any `rehome_into_world` / `conjecture_test` call — this eliminates
    // the dominant reasoning cost of the bare-entity fan-out while staying F1-EXACT (bare ⇒
    // suppressed, identical observable result). A subject carrying an ADDITIONAL,
    // potentially-disjoint type is NOT short-circuited: it falls through to the real reasoning
    // gate below so a genuine refutation still emits the corroborated remainder.
    if let (Some(first), Guard::Type(guard_class)) = (candidates.first(), &schema.guard)
        && !subject_can_refute_a_sortal(reasoned, &first.subject, guard_class)
    {
        return (Vec::new(), Vec::new());
    }
    let mut exhausted = Vec::new();
    let verdicts: Vec<EngineVerdict> = candidates
        .iter()
        .map(|candidate| {
            let scenario_world = scenario_world_iri(candidate);
            let assume = vec![(
                candidate.subject.clone(),
                candidate.predicate.clone(),
                candidate.object.clone(),
            )];
            // The sortal guard is always a `Guard::Type` (`rdf:type(this, gmeow:Entity)`), so
            // this always yields the same `type_atom` antecedent as before — the guard reading
            // is generalized uniformly, the sortal warrant logic itself is unchanged.
            let Some(guard_atom) = guard_antecedent(reasoned, &candidate.subject, &schema.guard)
            else {
                return EngineVerdict::Other;
            };
            let formula =
                sortal_disjunct_formula(guard_atom, &candidate.subject, &candidate.object);
            // The disjunctive gate is the ONE strategy that must detect a genuine CLASH
            // between the candidate and the SUBJECT'S OWN other assertions — the
            // conjunctive strategies only ever need syntactic redundancy (decided
            // fact-locally, see `warrant`). The native engine's DL closure is WORLD-SCOPED
            // (`crates/logic/src/store.rs`: "no cross-world union is performed"), so
            // `reasoned`'s own facts (the subject's other types, the disjointness axioms)
            // live in ITS authored world, never the isolated per-candidate `scenario_world`
            // `conjecture_test` asserts the candidate into — left as-is, a clash could never
            // be seen. Re-homing the KB into that SAME world lets it actually join the
            // candidate's consistency check (mirrors `rehome_kb_into_scenario` in
            // `crates/pipeline/src/mcp.rs`'s `evaluate_conjecture`).
            let Some(rehomed) = rehome_into_world(reasoned, &scenario_world) else {
                return EngineVerdict::Other;
            };
            let verdict =
                engine_verdict(rehomed.as_ref(), &scenario_world, &formula, &assume, budget);
            if verdict == EngineVerdict::Exhausted {
                exhausted.push(exhaustion_diag(schema, candidate, &scenario_world));
            }
            verdict
        })
        .collect();

    if !verdicts.contains(&EngineVerdict::Refuted) {
        return (Vec::new(), exhausted);
    }

    let suggestions = candidates
        .iter()
        .zip(verdicts.iter())
        .filter(|(_, verdict)| **verdict == EngineVerdict::Corroborated)
        .map(|(candidate, _)| {
            let scenario_world = scenario_world_iri(candidate);
            build_suggestion(reasoned, schema, candidate, &scenario_world)
        })
        .collect();
    (suggestions, exhausted)
}

/// Copy every quad of `reasoned` into the named graph `world`, dropping each quad's
/// original graph. The native engine's DL closure is world-scoped (per-named-graph, no
/// cross-world union — `crates/logic/src/store.rs`), so a candidate's `assume_context`/`φ`
/// (asserted by `conjecture_test` into its own isolated `scenario_world`) can only join the
/// KB's disjointness axioms and the subject's other assertions for a joint consistency
/// check when the KB is re-homed into that SAME world first. Mirrors
/// `rehome_kb_into_scenario` in `crates/pipeline/src/mcp.rs`. `None` only on a freeze
/// failure — copying already-valid quads should never fail in practice, but a failure is
/// honest absence (the caller's [`EngineVerdict::Other`]), never a panic.
fn rehome_into_world(reasoned: &RdfDataset, world: &str) -> Option<Arc<RdfDataset>> {
    let world_term = RdfTerm::iri(world.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    for quad in reasoned.owned_quads() {
        let rehomed =
            RdfQuad::new(quad.subject, quad.predicate, quad.object).in_graph(world_term.clone());
        builder.push_owned_quad(&rehomed);
    }
    builder.freeze().ok()
}

/// The four-way projection of a conjecture verdict the producer needs — a bare bool loses
/// exactly the `RefutedInStandpoint` vs. `Open` vs. `BudgetExhausted` distinctions the
/// per-subject sortal gate counts refutations over and the exhaustion path (below) reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineVerdict {
    /// The formula was `Corroborated` — the candidate addition is consistent (and warrants).
    Corroborated,
    /// The formula was `RefutedInStandpoint` — the candidate addition clashes with the
    /// subject's other assertions (a genuine discriminator).
    Refuted,
    /// The engine's budget was exhausted before it reached a conclusive verdict — an honest
    /// could-not-decide, NEVER folded into `Other` (a genuine `Open`/non-corroboration):
    /// silently conflating the two would drop a completable subject with no trace.
    Exhausted,
    /// `Open` (a genuine, budget-unconstrained non-corroboration) / an engine error — honest
    /// absence, neither corroborates, refutes, nor was cut short by budget.
    Other,
}

/// Run `formula` through the native conjecture engine and project its verdict, evaluation
/// status, and lifecycle to an [`EngineVerdict`]. Budget exhaustion is read off
/// `answer.verdict.evaluation` (the axis [`gmeow_logic::conjecture::conjecture_test`]
/// actually carries it on — `lifecycle_of`'s own invariant guarantees an exhausted run is
/// ALWAYS `Open`, so checking `evaluation` first is what makes exhaustion distinguishable
/// from a genuine `Open` at all) BEFORE the lifecycle match, so it is never folded into
/// [`EngineVerdict::Other`]. An engine error is [`EngineVerdict::Other`] (honest absence, no
/// panic — a hard reasoning failure is not itself an exhaustion).
fn engine_verdict(
    reasoned: &RdfDataset,
    scenario_world: &str,
    formula: &Formula,
    assume: &[(String, String, String)],
    budget: &Budget,
) -> EngineVerdict {
    match conjecture_test(
        reasoned,
        scenario_world,
        formula,
        BEST_PRACTICE_STANDPOINT_IRI,
        assume,
        budget,
    ) {
        Ok(answer) if answer.verdict.evaluation == EvaluationStatus::BudgetExhausted => {
            EngineVerdict::Exhausted
        }
        Ok(answer) => match answer.lifecycle {
            ConjectureLifecycleState::Corroborated => EngineVerdict::Corroborated,
            ConjectureLifecycleState::RefutedInStandpoint => EngineVerdict::Refuted,
            ConjectureLifecycleState::Open | ConjectureLifecycleState::Withdrawn => {
                EngineVerdict::Other
            }
        },
        Err(_) => EngineVerdict::Other,
    }
}

/// Build the "could-not-decide (budget exhausted)" diagnostic for a candidate whose warrant
/// test was cut short by the engine's budget before reaching a conclusive verdict — an
/// HONEST could-not-decide note (Note severity, mirrors the warrant [`Diag`]'s own grade),
/// never a false advisory and never a silent drop indistinguishable from a genuine
/// `Open`/non-corroboration. Returns the content-addressed sort key alongside the diag (see
/// [`AbductiveOutcome::exhausted`]'s determinism note).
fn exhaustion_diag(
    schema: &DiscoveredSchema,
    candidate: &Candidate,
    scenario_world: &str,
) -> (String, Diag) {
    let discipline = code_local(&schema.term);
    let digest = candidate_digest(candidate);
    let code = format!(
        "{}abductive.exhausted.{discipline}.{digest}",
        crate::codes::ADVICE_FAMILY
    );
    let subject_q = qname(&candidate.subject);
    let predicate_q = qname(&candidate.predicate);
    let object_q = qname(&candidate.object);
    let diag = Diag::note(
        register_code(&code),
        format!(
            "abductive warrant inconclusive: the native conjecture engine's budget was \
             exhausted before it could decide whether adding {predicate_q} {object_q} to \
             {subject_q} completes the {} completeness formula ({discipline}) in a consistent, \
             isolated scenario world <{scenario_world}> from standpoint \
             <{BEST_PRACTICE_STANDPOINT_IRI}> — this is a could-not-decide (budget exhausted), \
             NOT a genuine non-corroboration; no advisory is emitted for this candidate.",
            qname(&schema.term),
        ),
    );
    (code, diag)
}

/// The engine-evaluable ground-head Horn `guard_antecedent → relation(subject, value)` —
/// the restricted chase adds the head only when `relation(subject, value)` is absent, so
/// a present (or added) relatum is redundant (Supported) and a clash is Refuted. The
/// `guard_antecedent` is the subject's grounded guard atom (a `rdf:type(subject, class)` for
/// a type guard, or the subject's own `relation(subject, guardValue)` for a property guard).
fn ground_relatum_formula(
    guard_antecedent: Formula,
    subject: &str,
    relation: &str,
    value: &str,
) -> Formula {
    Formula::Implies(
        Box::new(guard_antecedent),
        Box::new(Formula::Atom {
            relation: Term::Iri(relation.to_owned()),
            args: vec![Term::Iri(subject.to_owned()), Term::Iri(value.to_owned())],
        }),
    )
}

/// The Horn per-disjunct completeness formula `guard_antecedent → τ_T(subject)`.
fn sortal_disjunct_formula(guard_antecedent: Formula, subject: &str, sortal: &str) -> Formula {
    Formula::Implies(
        Box::new(guard_antecedent),
        Box::new(type_atom(subject, sortal)),
    )
}

/// `rdf:type(subject, class)`.
fn type_atom(subject: &str, class: &str) -> Formula {
    Formula::Atom {
        relation: Term::Iri(RDF_TYPE.to_owned()),
        args: vec![Term::Iri(subject.to_owned()), Term::Iri(class.to_owned())],
    }
}

// ── Suggestion construction ──────────────────────────────────────────────────────────

/// Build the paired warrant [`Diag`] + [`Advisory`] for a corroborated candidate.
fn build_suggestion(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
    candidate: &Candidate,
    scenario_world: &str,
) -> AbductiveSuggestion {
    let discipline = code_local(&schema.term);
    let digest = candidate_digest(candidate);
    let advice_code = format!(
        "{}abductive.{discipline}.{digest}",
        crate::codes::ADVICE_FAMILY
    );
    let warrant_code = format!(
        "{}abductive.warrant.{discipline}.{digest}",
        crate::codes::ADVICE_FAMILY
    );
    let conjecture_id = format!("{discipline}.{digest}");

    let subject_q = qname(&candidate.subject);
    let object_q = qname(&candidate.object);
    let predicate_q = qname(&candidate.predicate);

    // The advisory message is the governed term's OWN live `gmeow:howToUse` prose (source
    // language), read from the reasoned graph — never a paraphrase. Honest fallback prose
    // when the term authors none.
    let message = term_how_to_use(reasoned, &schema.term).unwrap_or_else(|| {
        format!(
            "Complete the under-specified {} — the native conjecture engine corroborated a \
             minimal addition that satisfies its declared modeling discipline.",
            qname(&schema.term)
        )
    });

    let suggestion = format!(
        "Add {} to {subject_q} — {}. The native conjecture engine corroborated that this \
         addition completes {}'s discipline in a consistent scenario world.",
        candidate.element,
        candidate_reason(candidate, &predicate_q, &object_q),
        qname(&schema.term),
    );

    let advisory = Advisory::note(advice_code, message)
        .with_suggestion(suggestion)
        .with_subject_iri(candidate.subject.clone())
        .with_tag("abductive")
        .with_tag(format!("formalizes:{}", schema.term))
        .with_tag(format!("warrant:{conjecture_id}"));

    let warrant = Diag::note(
        register_code(&warrant_code),
        format!(
            "abductive warrant: the native conjecture engine corroborated that adding {predicate_q} \
             {object_q} to {subject_q} completes the {} completeness formula ({}) in a consistent, \
             isolated scenario world <{scenario_world}> from standpoint <{BEST_PRACTICE_STANDPOINT_IRI}>.",
            qname(&schema.term),
            discipline,
        ),
    );

    AbductiveSuggestion { warrant, advisory }
}

/// The concrete-element phrase describing the candidate reason (kind-specific but derived
/// from the predicate, never a hardcoded discipline switch).
fn candidate_reason(candidate: &Candidate, predicate_q: &str, object_q: &str) -> String {
    if candidate.predicate == RDF_TYPE {
        format!("specialize the bare sortal to {object_q}")
    } else if candidate.predicate.ends_with("exemplifies") {
        format!("declare {predicate_q} to a fresh gmeow:Manifestation witness ({object_q})")
    } else {
        format!("supply the missing {predicate_q} relatum (witness {object_q})")
    }
}

/// The element phrase for a relatum addition (`a gmeow:hasReferenceFrame`, etc.).
fn relatum_element(relation: &str) -> String {
    if relation.ends_with("exemplifies") {
        format!("{} + a gmeow:Manifestation", qname(relation))
    } else {
        format!("a {} relatum", qname(relation))
    }
}

// ── Graph readers ────────────────────────────────────────────────────────────────────

/// The subject IRIs typed `class` (asserted or entailed), byte-sorted & deduplicated.
fn subjects_of_type(reasoned: &RdfDataset, class: &str) -> Vec<String> {
    let (Some(type_id), Some(class_id)) = (term_id(reasoned, RDF_TYPE), term_id(reasoned, class))
    else {
        return Vec::new();
    };
    let mut subjects: Vec<String> = reasoned
        .quads_for_pattern(None, Some(type_id), Some(class_id), GraphMatch::Any)
        .filter_map(|q| match reasoned.resolve(q.s) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    subjects.sort();
    subjects.dedup();
    subjects
}

/// The gap subject set a schema's guard admits: the subjects typed the guard class
/// ([`Guard::Type`]) or the subjects of some guard-relation triple ([`Guard::Property`]),
/// RESTRICTED to genuine A-Box individuals (`gmeow:graphBoxRole gmeow:boxABox`). Byte-sorted
/// & deduplicated (both underlying readers are; `retain` preserves that order).
///
/// The restriction is applied in this ONE place so ALL guard paths (sortal, relator, WEMI,
/// Expression-frame, measurement) are scoped uniformly: abductive advice is only for real,
/// under-specified A-Box individuals, never a TBox class/property term nor an entailed
/// phantom the type/property closure otherwise sweeps in. The subject's TYPE/RELATA are
/// still read off the REASONED graph (entailed-type awareness is preserved, G1) — only the
/// SUBJECT SET is narrowed to declared A-Box individuals.
fn guard_subjects(reasoned: &RdfDataset, guard: &Guard) -> Vec<String> {
    let mut subjects = match guard {
        Guard::Type(class) => subjects_of_type(reasoned, class),
        Guard::Property(relation) => subjects_with_property(reasoned, relation),
    };
    subjects.retain(|subject| is_abox(reasoned, subject));
    subjects
}

/// `true` iff `subject` carries `gmeow:graphBoxRole gmeow:boxABox` on the reasoned graph —
/// the assertional-tier marker every genuine A-Box individual carries. The abductive guard
/// enumeration is scoped to these subjects only.
fn is_abox(reasoned: &RdfDataset, subject: &str) -> bool {
    let (Some(s), Some(p), Some(o)) = (
        term_id(reasoned, subject),
        term_id(reasoned, GMEOW_GRAPH_BOX_ROLE),
        term_id(reasoned, GMEOW_BOX_ABOX),
    ) else {
        return false;
    };
    reasoned
        .quads_for_pattern(Some(s), Some(p), Some(o), GraphMatch::Any)
        .next()
        .is_some()
}

/// The IRI subjects that are the subject of some `relation` triple (asserted or entailed),
/// byte-sorted & deduplicated. IRI-only: a property guard keys on an IRI-valued witness (e.g.
/// `logic:unit`), so a literal/blank subject is never a gap subject — mirroring the file's
/// existing IRI-only reader discipline ([`subjects_of_type`]).
fn subjects_with_property(reasoned: &RdfDataset, relation: &str) -> Vec<String> {
    let Some(rel_id) = term_id(reasoned, relation) else {
        return Vec::new();
    };
    let mut subjects: Vec<String> = reasoned
        .quads_for_pattern(None, Some(rel_id), None, GraphMatch::Any)
        .filter_map(|q| match reasoned.resolve(q.s) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    subjects.sort();
    subjects.dedup();
    subjects
}

/// The grounded guard antecedent atom for `subject` under `guard`:
///   * [`Guard::Type`]     → `rdf:type(subject, class)` (always `Some`); or
///   * [`Guard::Property`] → `relation(subject, guardValue)` where `guardValue` is the
///     subject's byte-first IRI object for the guard relation, so the antecedent is grounded
///     and non-vacuous. `None` when the subject has no IRI guard value — that candidate
///     cannot warrant (honest absence), never a vacuous or guessed antecedent.
fn guard_antecedent(reasoned: &RdfDataset, subject: &str, guard: &Guard) -> Option<Formula> {
    match guard {
        Guard::Type(class) => Some(type_atom(subject, class)),
        Guard::Property(relation) => {
            let value = first_object(reasoned, subject, relation)?;
            Some(Formula::Atom {
                relation: Term::Iri(relation.clone()),
                args: vec![Term::Iri(subject.to_owned()), Term::Iri(value)],
            })
        }
    }
}

/// `true` iff `subject predicate _` holds for some object.
fn has_object(reasoned: &RdfDataset, subject: &str, predicate: &str) -> bool {
    let (Some(s), Some(p)) = (term_id(reasoned, subject), term_id(reasoned, predicate)) else {
        return false;
    };
    reasoned
        .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Any)
        .next()
        .is_some()
}

/// The subject's byte-first IRI object for `predicate`, or `None` when it has none.
/// Byte-first keeps the ground value deterministic for a non-functional relatum.
fn first_object(reasoned: &RdfDataset, subject: &str, predicate: &str) -> Option<String> {
    let (Some(s), Some(p)) = (term_id(reasoned, subject), term_id(reasoned, predicate)) else {
        return None;
    };
    let mut objects: Vec<String> = reasoned
        .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Any)
        .filter_map(|q| match reasoned.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    objects.sort();
    objects.into_iter().next()
}

/// `true` iff `subject rdf:type class` holds.
fn has_type(reasoned: &RdfDataset, subject: &str, class: &str) -> bool {
    let (Some(s), Some(p), Some(o)) = (
        term_id(reasoned, subject),
        term_id(reasoned, RDF_TYPE),
        term_id(reasoned, class),
    ) else {
        return false;
    };
    reasoned
        .quads_for_pattern(Some(s), Some(p), Some(o), GraphMatch::Any)
        .next()
        .is_some()
}

/// `true` iff `subject` carries some reasoned rdf:type that could REFUTE an offered top
/// sortal — i.e. a type that is NOT the guard class, NOT a superclass of the guard class, and
/// NOT `owl:Thing`. Every offered sortal is a subclass of `guard_class`, so those "benign"
/// types are all consistent with every sortal (a superclass of the guard can never be
/// disjoint with a subclass of the guard). A subject with only benign types therefore cannot
/// land any disjunct `RefutedInStandpoint`, and F1 suppresses it (see
/// [`sortal_suggestions_for_subject`]'s short-circuit). SOUND, never over-suppressing: a type
/// disjoint with a sortal can never itself be a superclass of the guard class, so a subject
/// carrying a genuinely-discriminating type always returns `true` and reaches the reasoning
/// gate. Reads the REASONED graph, where subclass/domain/range-induced types are materialized.
fn subject_can_refute_a_sortal(reasoned: &RdfDataset, subject: &str, guard_class: &str) -> bool {
    let mut benign = superclasses(reasoned, guard_class);
    benign.insert(guard_class.to_owned());
    benign.insert(OWL_THING.to_owned());
    types_of(reasoned, subject)
        .into_iter()
        .any(|t| !benign.contains(&t))
}

/// The reasoned rdf:type IRIs of `subject` (byte-sorted & deduplicated).
fn types_of(reasoned: &RdfDataset, subject: &str) -> Vec<String> {
    let (Some(s), Some(p)) = (term_id(reasoned, subject), term_id(reasoned, RDF_TYPE)) else {
        return Vec::new();
    };
    let mut types: Vec<String> = reasoned
        .quads_for_pattern(Some(s), Some(p), None, GraphMatch::Any)
        .filter_map(|q| match reasoned.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
        .collect();
    types.sort();
    types.dedup();
    types
}

/// The transitive `rdfs:subClassOf` ancestors of `class` on the reasoned graph (`class`
/// itself excluded). A cycle-safe BFS — a `subClassOf` cycle (e.g. two equivalent classes)
/// terminates because a class is pushed only on first insertion into the visited set.
fn superclasses(reasoned: &RdfDataset, class: &str) -> BTreeSet<String> {
    let Some(subclass_pred) = term_id(reasoned, RDFS_SUBCLASS_OF) else {
        return BTreeSet::new();
    };
    let mut ancestors: BTreeSet<String> = BTreeSet::new();
    let mut stack = vec![class.to_owned()];
    while let Some(current) = stack.pop() {
        let Some(current_id) = term_id(reasoned, &current) else {
            continue;
        };
        for quad in
            reasoned.quads_for_pattern(Some(current_id), Some(subclass_pred), None, GraphMatch::Any)
        {
            if let TermRef::Iri(iri) = reasoned.resolve(quad.o) {
                let ancestor = iri.to_owned();
                if ancestor != class && ancestors.insert(ancestor.clone()) {
                    stack.push(ancestor);
                }
            }
        }
    }
    ancestors
}

/// The governed term's canonical source-language (`@x-gmeow-english`) `gmeow:howToUse`
/// prose, read from `reasoned`. `None` when the term authors none — an `@en`/`@zh`/`@fr`
/// projection is never returned (mirrors `advisory::term_source_prose`). Byte-first keeps
/// the surfaced message deterministic when a term carries more than one source-language
/// `howToUse` literal (mirrors [`first_object`]'s collect-sort-take-first pattern).
fn term_how_to_use(reasoned: &RdfDataset, term: &str) -> Option<String> {
    let (Some(subj), Some(pred)) = (term_id(reasoned, term), term_id(reasoned, GMEOW_HOW_TO_USE))
    else {
        return None;
    };
    let mut messages: Vec<String> = reasoned
        .quads_for_pattern(Some(subj), Some(pred), None, GraphMatch::Any)
        .filter_map(|q| match reasoned.resolve(q.o) {
            TermRef::Literal {
                lexical,
                language: Some(lang),
                ..
            } if lang == ADVICE_SOURCE_LANG => Some(lexical.to_owned()),
            _ => None,
        })
        .collect();
    messages.sort();
    messages.into_iter().next()
}

/// Resolve an IRI to its interned dataset id.
fn term_id(reasoned: &RdfDataset, iri: &str) -> Option<purrdf::TermId> {
    reasoned.term_id_by_value(&purrdf::TermValue::iri(iri))
}

// ── Content-addressing & naming ──────────────────────────────────────────────────────

/// A content-addressed witness IRI for a `(subject, predicate)` addition — a stable
/// SHA-256 digest, never a clock/random source.
fn witness_iri(subject: &str, predicate: &str) -> String {
    format!(
        "{WITNESS_BASE}{}",
        hex_digest(&format!("{subject}\u{1f}{predicate}"), 16)
    )
}

/// A content-addressed isolated scenario-world IRI for a candidate.
fn scenario_world_iri(candidate: &Candidate) -> String {
    format!(
        "{SCENARIO_WORLD_BASE}{}",
        hex_digest(&candidate_key(candidate), 16)
    )
}

/// A stable 12-hex digest of the full candidate (subject · predicate · object) — the
/// injective code fragment so distinct candidates never collide onto one advisory code.
fn candidate_digest(candidate: &Candidate) -> String {
    hex_digest(&candidate_key(candidate), 12)
}

/// The content-addressing key of a candidate.
fn candidate_key(candidate: &Candidate) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}",
        candidate.subject, candidate.predicate, candidate.object
    )
}

/// The first `hex_len` hex chars of `SHA-256(input)`.
fn hex_digest(input: &str, hex_len: usize) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut hex = String::with_capacity(hex_len);
    for byte in digest {
        use std::fmt::Write;
        if hex.len() >= hex_len {
            break;
        }
        let _ = write!(hex, "{byte:02x}");
    }
    hex.truncate(hex_len);
    hex
}

/// The IRI-safe local name (after the last `/` or `#`), sanitised to the code alphabet
/// (mirrors `advisory::code_local`).
fn code_local(iri: &str) -> String {
    let raw = iri.rsplit(['/', '#']).next().unwrap_or(iri);
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// A compact display QName for an IRI: `gmeow:local` / `logic:local` / `rdf:type`, else
/// the angle-bracketed IRI. Presentation only — never a code key.
fn qname(iri: &str) -> String {
    if let Some(local) = iri.strip_prefix(GMEOW) {
        format!("gmeow:{local}")
    } else if let Some(local) = iri.strip_prefix(LOGIC) {
        format!("logic:{local}")
    } else if iri == RDF_TYPE {
        "rdf:type".to_owned()
    } else {
        format!("<{iri}>")
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse `ttl` (with the `gmeow:` prefix predeclared) into a frozen [`RdfDataset`] —
    /// the minimal build helper this file's `term_how_to_use` unit test needs.
    fn dataset(ttl: &str) -> Arc<RdfDataset> {
        let parsed =
            purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("ttl parses");
        let mut builder = RdfDatasetBuilder::new();
        builder.push_dataset(parsed.as_ref());
        builder.freeze().expect("freeze")
    }

    /// A term carrying TWO `@x-gmeow-english gmeow:howToUse` literals resolves to the
    /// byte-first one deterministically — same result regardless of which literal was
    /// authored first (G7: `term_how_to_use` must not be quad-iteration-order dependent,
    /// mirroring `first_object`'s collect-sort-take-first pattern).
    #[test]
    fn term_how_to_use_picks_the_byte_first_literal_regardless_of_insertion_order() {
        let term = format!("{GMEOW}TestAbductiveTerm");
        let ttl_zebra_first = format!(
            "@prefix gmeow: <{GMEOW}> .\n<{term}> gmeow:howToUse \"Zebra message\"@x-gmeow-english , \"Alpha message\"@x-gmeow-english .\n"
        );
        let ttl_alpha_first = format!(
            "@prefix gmeow: <{GMEOW}> .\n<{term}> gmeow:howToUse \"Alpha message\"@x-gmeow-english , \"Zebra message\"@x-gmeow-english .\n"
        );

        let from_zebra_first = term_how_to_use(dataset(&ttl_zebra_first).as_ref(), &term);
        let from_alpha_first = term_how_to_use(dataset(&ttl_alpha_first).as_ref(), &term);

        assert_eq!(
            from_zebra_first, from_alpha_first,
            "the surfaced message must not depend on authoring/insertion order"
        );
        assert_eq!(
            from_zebra_first.as_deref(),
            Some("Alpha message"),
            "the byte-first literal (\"Alpha message\" < \"Zebra message\") wins"
        );
    }

    /// A term with no `@x-gmeow-english gmeow:howToUse` literal at all is honest absence,
    /// not a panic or a fallback to another language.
    #[test]
    fn term_how_to_use_is_none_when_the_term_authors_no_source_language_literal() {
        let term = format!("{GMEOW}TestAbductiveTermNoProse");
        let ttl =
            format!("@prefix gmeow: <{GMEOW}> .\n<{term}> gmeow:howToUse \"English prose\"@en .\n");
        assert_eq!(term_how_to_use(dataset(&ttl).as_ref(), &term), None);
    }
}
