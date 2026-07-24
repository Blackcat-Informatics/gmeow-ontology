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
//! world. Only a `Corroborated` verdict warrants the emitted advisory — an `Open`,
//! `RefutedInStandpoint`, or budget-exhausted answer is honest absence (no suggestion).
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
//! So a relator-mediation completeness (a conjunction of relata) is warranted by testing,
//! for EVERY declared relatum, the ground Horn `τ_guard(s) → r_i(s, value_i)` where
//! `value_i` is the fresh witness for the relatum being ADDED and the subject's existing
//! object for a relatum already present — and requiring every conjunct to corroborate.
//! A relatum that is still missing has no ground value, so the candidate cannot be
//! warranted (this is exactly "adding the last relatum corroborates only when the others
//! are already present"). A sortal specialization (a disjunction) is warranted per
//! candidate disjunct: `τ_Entity(s) → τ_T(s)` corroborates exactly when adding `T` is
//! consistent with the subject's other assertions (∨-introduction: one true disjunct
//! satisfies the completeness disjunction). The base graph is never mutated — the
//! hypothetical lives only in the borrowed scenario EDB — so nothing is auto-asserted.

use gmeow_errors::{Diag, register_code};
use gmeow_logic::conjecture::{ConjectureLifecycleState, conjecture_test};
use gmeow_logic::query_ir::Budget;
use gmeow_logic_compile::frontend::reconstruct_formula;
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef};
use sha2::{Digest, Sha256};

use crate::advisory::{Advisory, BEST_PRACTICE_STANDPOINT_IRI};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
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

/// A discovered `logic:AbductiveSchema` and the completeness structure read off its
/// `logic:completenessFormula`.
struct DiscoveredSchema {
    /// The governed term (`logic:formalizes`) — the advice's provenance.
    term: String,
    /// The `logic:repairStrategy` IRI.
    strategy: String,
    /// The guard class every gap subject must be typed (the completeness guard atom's
    /// fixed object, e.g. `gmeow:Commitment` / `gmeow:Entity`).
    guard_type: String,
    /// The completeness consequent, classified.
    cons: ConsShape,
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
pub fn abductive_advisories(reasoned: &RdfDataset, budget: &Budget) -> Vec<AbductiveSuggestion> {
    let mut out = Vec::new();
    for schema in discover_schemas(reasoned) {
        let candidates = match schema.strategy.as_str() {
            STRATEGY_SORTAL => sortal_candidates(reasoned, &schema),
            STRATEGY_RELATUM | STRATEGY_CHAIN | STRATEGY_FRAME => {
                relatum_candidates(reasoned, &schema)
            }
            // An unknown strategy is honest absence, never new hidden behaviour.
            _ => Vec::new(),
        };
        for candidate in candidates {
            if let Some(suggestion) = warrant(reasoned, &schema, &candidate, budget) {
                out.push(suggestion);
            }
        }
    }
    out.sort_by(|a, b| {
        a.advisory
            .code
            .cmp(&b.advisory.code)
            .then_with(|| a.advisory.subject_iri.cmp(&b.advisory.subject_iri))
    });
    out
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
        let Some((guard_type, cons)) = deconstruct(&formula) else {
            continue;
        };
        schemas.push(DiscoveredSchema {
            term,
            strategy,
            guard_type,
            cons,
        });
    }
    schemas
}

// ── Completeness-formula deconstruction ──────────────────────────────────────────────

/// Read `(guard_type, ConsShape)` off a completeness formula
/// `∀"this". (τ_guard(this) → Cons)`. Returns `None` for any shape that is not the
/// authored guard→consequent form (honest absence).
fn deconstruct(formula: &Formula) -> Option<(String, ConsShape)> {
    let body = match formula {
        Formula::Forall { vars, body } if vars.len() == 1 && vars[0] == "this" => body.as_ref(),
        _ => return None,
    };
    let Formula::Implies(antecedent, consequent) = body else {
        return None;
    };
    let guard_type = guard_class(antecedent)?;

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
    Some((guard_type, cons))
}

/// The fixed class of a guard atom `rdf:type(this, Class)`.
fn guard_class(formula: &Formula) -> Option<String> {
    let (relation, args) = as_binary_atom(formula)?;
    if relation != RDF_TYPE {
        return None;
    }
    match (&args.0, &args.1) {
        (Term::Var(v), Term::Iri(class)) if v == "this" => Some(class.clone()),
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
    for subject in subjects_of_type(reasoned, &schema.guard_type) {
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
/// offered sortal type (byte-sorted).
fn sortal_candidates(reasoned: &RdfDataset, schema: &DiscoveredSchema) -> Vec<Candidate> {
    let ConsShape::Disjunctive(types) = &schema.cons else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for subject in subjects_of_type(reasoned, &schema.guard_type) {
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

/// Test `candidate` against the schema's completeness condition in an isolated,
/// content-addressed scenario world, and build the paired warrant + advisory when the
/// native engine corroborates. `None` on any non-corroborating verdict (Open /
/// RefutedInStandpoint / BudgetExhausted) or engine error — honest absence, no panic.
fn warrant(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
    candidate: &Candidate,
    budget: &Budget,
) -> Option<AbductiveSuggestion> {
    let scenario_world = scenario_world_iri(candidate);
    let assume = vec![(
        candidate.subject.clone(),
        candidate.predicate.clone(),
        candidate.object.clone(),
    )];

    let corroborated = match &schema.cons {
        // Conjunctive completeness: build the ground value of EVERY declared relatum — the
        // fresh witness for the one being added, the subject's existing object for one
        // already present — and require every relatum's ground Horn to corroborate. A
        // still-missing relatum has no ground value, so the candidate cannot warrant.
        ConsShape::Conjunctive(relata) => relata.iter().all(|(relation, _)| {
            let Some(value) = ground_value(reasoned, candidate, relation) else {
                return false;
            };
            let formula =
                ground_relatum_formula(&candidate.subject, &schema.guard_type, relation, &value);
            corroborates(reasoned, &scenario_world, &formula, &assume, budget)
        }),
        // Disjunctive completeness: the Horn implication for the candidate's own disjunct
        // must corroborate (adding the type is consistent → ∨-introduction).
        ConsShape::Disjunctive(_) => {
            let formula =
                sortal_disjunct_formula(&candidate.subject, &schema.guard_type, &candidate.object);
            corroborates(reasoned, &scenario_world, &formula, &assume, budget)
        }
    };
    if !corroborated {
        return None;
    }

    Some(build_suggestion(
        reasoned,
        schema,
        candidate,
        &scenario_world,
    ))
}

/// `true` iff the native conjecture engine lands the formula in
/// [`ConjectureLifecycleState::Corroborated`]. Any other lifecycle or an engine error is
/// `false` (honest absence).
fn corroborates(
    reasoned: &RdfDataset,
    scenario_world: &str,
    formula: &Formula,
    assume: &[(String, String, String)],
    budget: &Budget,
) -> bool {
    matches!(
        conjecture_test(
            reasoned,
            scenario_world,
            formula,
            BEST_PRACTICE_STANDPOINT_IRI,
            assume,
            budget,
        ),
        Ok(answer) if answer.lifecycle == ConjectureLifecycleState::Corroborated
    )
}

/// The ground value of `relation` on the candidate's subject: the fresh witness object
/// when `relation` is the relatum being ADDED, else the subject's existing (byte-first)
/// object. `None` when the relatum is neither the addition nor already present — a
/// still-missing relatum has no ground value, so no warrant is possible.
fn ground_value(reasoned: &RdfDataset, candidate: &Candidate, relation: &str) -> Option<String> {
    if relation == candidate.predicate {
        Some(candidate.object.clone())
    } else {
        first_object(reasoned, &candidate.subject, relation)
    }
}

/// The engine-evaluable ground-head Horn `τ_guard(subject) → relation(subject, value)` —
/// the restricted chase adds the head only when `relation(subject, value)` is absent, so
/// a present (or added) relatum is redundant (Supported) and a clash is Refuted.
fn ground_relatum_formula(subject: &str, guard_type: &str, relation: &str, value: &str) -> Formula {
    Formula::Implies(
        Box::new(type_atom(subject, guard_type)),
        Box::new(Formula::Atom {
            relation: Term::Iri(relation.to_owned()),
            args: vec![Term::Iri(subject.to_owned()), Term::Iri(value.to_owned())],
        }),
    )
}

/// The Horn per-disjunct completeness formula `τ_guard(subject) → τ_T(subject)`.
fn sortal_disjunct_formula(subject: &str, guard_type: &str, sortal: &str) -> Formula {
    Formula::Implies(
        Box::new(type_atom(subject, guard_type)),
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

/// The governed term's canonical source-language (`@x-gmeow-english`) `gmeow:howToUse`
/// prose, read from `reasoned`. `None` when the term authors none — an `@en`/`@zh`/`@fr`
/// projection is never returned (mirrors `advisory::term_source_prose`).
fn term_how_to_use(reasoned: &RdfDataset, term: &str) -> Option<String> {
    let (Some(subj), Some(pred)) = (term_id(reasoned, term), term_id(reasoned, GMEOW_HOW_TO_USE))
    else {
        return None;
    };
    reasoned
        .quads_for_pattern(Some(subj), Some(pred), None, GraphMatch::Any)
        .find_map(|q| match reasoned.resolve(q.o) {
            TermRef::Literal {
                lexical,
                language: Some(lang),
                ..
            } if lang == ADVICE_SOURCE_LANG => Some(lexical.to_owned()),
            _ => None,
        })
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
