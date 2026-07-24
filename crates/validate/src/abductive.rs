// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Abductive advice producer (D5) — the constructive "what to ADD" wing. ENGINE-FREE.
//!
//! A GENERIC producer driven entirely by the authored `logic:AbductiveSchema`
//! vocabulary (`slices/grounding/logic/module.ttl`). It discovers every schema by
//! SPARQL, reads each schema's `logic:completenessFormula` structure off the graph, and
//! enumerates the minimal candidate additions that would complete an under-specified
//! subject. How each candidate is WARRANTED depends on the completeness SHAPE — and NEITHER
//! path calls a reasoner: the producer is entirely engine-free.
//!   * a CONJUNCTIVE (relatum) completeness — relator-mediation, WEMI-chain,
//!     reference-frame, and measurement-frame — is warranted BY CONSTRUCTION: one candidate
//!     per MISSING relatum, filled by a FRESH content-addressed witness that is not in the
//!     graph. A fresh untyped individual on an absent property can never trigger a
//!     cardinality or disjointness clash, so the addition is consistent by construction and
//!     completes that conjunct's per-conjunct completeness.
//!   * a DISJUNCTIVE (sortal) completeness is warranted by a SOUND CLASS-DISJOINTNESS LOOKUP,
//!     not a reasoner. The offered disjuncts are the top sortals `T`; adding a single bare
//!     `rdf:type(s, T)` to an already-reasoned subject `s` is inconsistent IFF `s` already
//!     holds a type disjoint with `T` — a bare top-sortal type-assertion has no OTHER
//!     inconsistency source (no cardinality, no property-range clash: the top sortals carry
//!     only `owl:disjointWith` negative axioms). So refutation is an O(1) set-membership test
//!     against a per-sortal CLASH SET precomputed once per run, replacing the old
//!     per-candidate rehome-the-whole-KB-and-reason `conjecture_test`. See
//!     [`sortal_suggestions`] for the soundness argument and the clash-set construction.
//!
//! There is NO hardcoded discipline list and NO hardcoded predicate string: the guard
//! type, the required relata predicates, and the candidate sortal types are all read
//! from the reconstructed completeness formula, so registering a further discipline of
//! an existing strategy is a data mark in the logic module, never new code here.
//!
//! # The conjunctive (relatum) completeness warrant is BY CONSTRUCTION
//!
//! A relator-mediation completeness (a conjunction of relata) is warranted PER CANDIDATE,
//! per conjunct, BY CONSTRUCTION: each missing relatum is its own independently-warranted
//! candidate, filled by a FRESH content-addressed witness `value_i` that is NOT in the
//! graph. Adding `r_i(s, value_i)` for an absent relation `r_i` and a fresh, untyped `value_i`
//! is a consistent addition — a fresh untyped object on an absent property can never trigger
//! a cardinality or disjointness clash — so it completes that conjunct's per-conjunct
//! completeness WITHOUT any engine check. The warrant is the by-construction argument itself.
//!
//! A one-party `gmeow:Commitment` (two of three relata missing) therefore yields ONE
//! candidate per missing relatum, each warranted independently by construction; the other
//! relata being still missing no longer blocks either candidate's warrant (per-conjunct
//! completeness: "adding this relatum with a fresh witness is a consistent completion", not
//! "all others are already present"). The full completeness conjunction is still the
//! discipline's authored TARGET (`logic:relatorMediationCons` names all three relata) — only
//! the per-candidate WARRANTING is scoped to the one conjunct being added.
//!
//! # The disjunctive (sortal) completeness warrant is a SOUND CLASS-DISJOINTNESS LOOKUP
//!
//! A sortal specialization (a disjunction of top sortals) is warranted PER SUBJECT, across
//! every offered disjunct together, never per candidate in isolation. For each offered sortal
//! `T`, refutation of `rdf:type(s, T)` is decided by a lookup, not a reasoning pass:
//!   `Refuted(s, T)  ⟺  reasoned_types(s) ∩ clash_set(T) ≠ ∅`
//! where `reasoned_types(s)` are `s`'s `rdf:type` objects on the REASONED graph (already
//! carrying materialized superclasses) and `clash_set(T)` is every class disjoint with `T` or
//! with any reasoned superclass of `T`. This is the SOUND equivalent of the old engine
//! consistency check for exactly this case: a bare top-sortal type-assertion is inconsistent
//! only through a class-disjointness clash, and disjointness propagates down subclasses, so
//! `∃ A ⊇ D, B ⊇ T. A ⊥ B` (the engine's refutation condition for `s:D`) is captured exactly
//! by intersecting `s`'s upward-closed types with `T`'s upward-anchored clash set. Disjointness
//! is read in EVERY form that occurs in the reasoned object-level graph — `owl:disjointWith`
//! (symmetric), `owl:AllDisjointClasses`/`owl:members` (expanded pairwise), and the canonical
//! `logic:disjointWith` grounding form — so no clash the engine would find is missed.
//!
//! A bare subject typed nothing but its guard class refutes NO offered sortal — advising a
//! "specialize to X" menu across all of them is non-discriminating noise, not advice, so it is
//! SUPPRESSED entirely (honest absence, not a same-weight N-way menu). Only when the subject's
//! own assertions REFUTE at least one offered sortal does the completeness disjunction
//! genuinely constrain the choice; advice is then emitted for the CORROBORATED remainder
//! (∨-introduction: one true disjunct satisfies the completeness disjunction), the refuted
//! disjunct(s) excluded. The base graph is never mutated — the lookup only reads it — so
//! nothing is auto-asserted.

use gmeow_errors::{Diag, register_code};
use gmeow_logic_compile::frontend::reconstruct_formula;
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::advisory::Advisory;

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
/// The three disjointness forms that occur in the reasoned object-level graph. The sortal
/// clash-set lookup ([`sortal_suggestions`]) reads ALL of them so no refutation the old
/// native engine would find is missed (soundness against the engine it replaces).
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
const OWL_ALL_DISJOINT_CLASSES: &str = "http://www.w3.org/2002/07/owl#AllDisjointClasses";
const OWL_MEMBERS: &str = "http://www.w3.org/2002/07/owl#members";
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

/// One abductive suggestion: the warrant [`Diag`] paired with the dual-projection-ready
/// [`Advisory`]. The two carry the SAME content-addressed digest in their codes, so a
/// consumer can join the warrant to the advice it warrants. The warrant is either a
/// by-construction argument (conjunctive/relatum path — a fresh witness for a missing relatum
/// is a consistent addition) or a class-disjointness corroboration (sortal path).
#[derive(Debug)]
pub struct AbductiveSuggestion {
    /// The warrant note: a by-construction relatum argument, or a class-disjointness
    /// corroboration for the sortal path (see [`build_suggestion`]).
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

/// Produce every warranted abductive advisory over `reasoned`. ENGINE-FREE: the relatum
/// path warrants by construction and the sortal path by a class-disjointness lookup, so no
/// reasoner is invoked at all.
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
/// byte-sorted, all witness IRIs are content-addressed, and the returned vector is finally
/// sorted by `(advisory.code, advisory.subject_iri)`. Same input twice ⇒ identical output.
/// `reasoned` is only READ; the base graph is never mutated.
#[must_use]
pub fn abductive_advisories(reasoned: &RdfDataset) -> Vec<AbductiveSuggestion> {
    let mut suggestions = Vec::new();
    for schema in discover_schemas(reasoned) {
        match schema.strategy.as_str() {
            // The sortal/disjunctive strategy is gated PER SUBJECT across all its offered
            // disjuncts together (the non-discriminating-menu suppression), so it builds its
            // own suggestions rather than going through the generic per-candidate `warrant`.
            STRATEGY_SORTAL => {
                suggestions.extend(sortal_suggestions(reasoned, &schema));
            }
            STRATEGY_RELATUM | STRATEGY_CHAIN | STRATEGY_FRAME => {
                // The conjunctive/relatum warrant is BY CONSTRUCTION (a fresh witness for a
                // missing relatum is a consistent addition), so it yields a suggestion or
                // honest absence.
                for candidate in relatum_candidates(reasoned, &schema) {
                    match warrant(reasoned, &schema, &candidate) {
                        WarrantOutcome::Corroborated(suggestion) => suggestions.push(*suggestion),
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
    suggestions
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
/// all) is the per-subject class-disjointness gate in [`sortal_suggestions`].
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

// ── The relatum by-construction warrant ──────────────────────────────────────────────

/// The two-way disposition a single conjunctive/relatum candidate lands in. The relatum
/// warrant is BY CONSTRUCTION (no engine call), so it either warrants a suggestion or is
/// honest absence.
enum WarrantOutcome {
    /// The candidate is warranted by construction — the paired warrant + advisory. Boxed: at
    /// ~240 bytes this variant otherwise dwarfs its unit sibling (clippy::large_enum_variant).
    Corroborated(Box<AbductiveSuggestion>),
    /// A non-conjunctive schema mis-dispatched here, or a property guard whose subject has no
    /// IRI guard value — honest absence, no suggestion.
    Other,
}

/// Warrant `candidate` (relator mediation / WEMI chain / reference frame / measurement frame)
/// BY CONSTRUCTION and build the paired warrant + advisory. The candidate fills a MISSING
/// relatum with a FRESH content-addressed witness (`candidate.object`, never in the graph):
/// adding `predicate(subject, freshWitness)` for an absent `predicate` and a fresh, untyped
/// object can NEVER trigger a cardinality or disjointness clash, so it is a consistent
/// addition that completes that conjunct's per-conjunct completeness. This is warranted by the
/// by-construction argument itself — NO native conjecture-engine call (the old per-candidate
/// `conjecture_test` there was tautological — always `Corroborated` for a fresh witness — and
/// expensive; it is gone).
///
/// Per-conjunct completeness: each missing relatum is its own independently-warranted
/// candidate. A relatum that is STILL missing on the subject (a different conjunct than the
/// one this candidate adds) does not block this candidate's warrant (see the module doc "The
/// conjunctive (relatum) completeness warrant is BY CONSTRUCTION" section).
///
/// The sortal/disjunctive strategy never reaches this function — it is warranted through the
/// per-subject class-disjointness gate in [`sortal_suggestions`], so a non-conjunctive schema
/// mis-dispatched here is [`WarrantOutcome::Other`] (honest absence). A
/// property guard whose subject has no IRI guard value is likewise honest absence — the guard
/// does not actually ground on that subject, so no advice is emitted, no panic.
fn warrant(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
    candidate: &Candidate,
) -> WarrantOutcome {
    let ConsShape::Conjunctive(_) = &schema.cons else {
        return WarrantOutcome::Other;
    };
    // The guard must actually ground on this subject (a type guard always does; a property
    // guard needs an IRI guard value). Absent one, the guard does not admit the subject —
    // honest absence, no panic. (`relatum_candidates` already scopes to guard subjects, so
    // this only excludes a property-guard subject whose guard value is a literal/blank.)
    if guard_antecedent(reasoned, &candidate.subject, &schema.guard).is_none() {
        return WarrantOutcome::Other;
    }
    WarrantOutcome::Corroborated(Box::new(build_suggestion(
        reasoned,
        schema,
        candidate,
        WarrantProse::ByConstruction,
    )))
}

/// Sortal-specialization suggestions, gated PER SUBJECT across every offered disjunct
/// together — the non-discriminating-menu suppression — decided by a SOUND CLASS-DISJOINTNESS
/// LOOKUP, no reasoner. Each offered sortal's CLASH SET (every class whose presence on a
/// subject refutes adding that sortal) is precomputed ONCE per run (there are only ~4 offered
/// sortals), so refutation is an O(1) set-membership test per candidate rather than a full
/// per-candidate KB rehome + conjecture reasoning pass. Byte-sorted candidate order is
/// preserved, so the output stays deterministic.
fn sortal_suggestions(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
) -> Vec<AbductiveSuggestion> {
    let ConsShape::Disjunctive(types) = &schema.cons else {
        return Vec::new();
    };
    // Precompute the clash set of each offered sortal ONCE. `disjointness_index` is built
    // once and shared across all ~4 offered sortals.
    let disjointness = disjointness_index(reasoned);
    let clash_sets: BTreeMap<String, BTreeSet<String>> = types
        .iter()
        .map(|sortal| (sortal.clone(), clash_set(reasoned, sortal, &disjointness)))
        .collect();

    let candidates = sortal_candidates(reasoned, schema);
    let mut suggestions = Vec::new();
    let mut start = 0;
    while start < candidates.len() {
        let mut end = start + 1;
        while end < candidates.len() && candidates[end].subject == candidates[start].subject {
            end += 1;
        }
        suggestions.extend(sortal_suggestions_for_subject(
            reasoned,
            schema,
            &candidates[start..end],
            &clash_sets,
        ));
        start = end;
    }
    suggestions
}

/// The per-subject sortal gate, by class-disjointness lookup. All of `candidates` are for the
/// SAME subject `s`. For each offered sortal `T`:
///   `Refuted(s, T)  ⟺  reasoned_types(s) ∩ clash_set(T) ≠ ∅`
/// (a bare top-sortal type-assertion is inconsistent ONLY through a class-disjointness clash;
/// see the module doc and [`clash_set`] for the soundness argument against the old engine).
/// Emit the `Corroborated` (= not-refuted) remainder ONLY when at least one offered sortal is
/// refuted — the model then genuinely discriminates; nothing refuted ⇒ a non-discriminating
/// N-way menu ⇒ SUPPRESSED entirely (honest absence). This is byte-identical to the old
/// per-subject engine gate's F1 semantics, now O(1) per candidate.
fn sortal_suggestions_for_subject(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
    candidates: &[Candidate],
    clash_sets: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<AbductiveSuggestion> {
    let Some(first) = candidates.first() else {
        return Vec::new();
    };
    // The subject's reasoned rdf:type objects — already carrying materialized superclasses on
    // a reasoned graph, so a subclass-induced disjoint type is seen here. All candidates in
    // the slice share this subject.
    let subject_types: BTreeSet<String> = types_of(reasoned, &first.subject).into_iter().collect();

    let mut any_refuted = false;
    let mut corroborated: Vec<&Candidate> = Vec::new();
    for candidate in candidates {
        // Refuted iff the subject already holds a type in the offered sortal's clash set.
        let refuted = clash_sets
            .get(&candidate.object)
            .is_some_and(|clash| subject_types.iter().any(|t| clash.contains(t)));
        if refuted {
            any_refuted = true;
        } else {
            corroborated.push(candidate);
        }
    }
    // F1 gate: a discriminating model (≥1 offered sortal ruled out) warrants the corroborated
    // remainder; a non-discriminating one (nothing ruled out) is suppressed.
    if !any_refuted {
        return Vec::new();
    }
    corroborated
        .into_iter()
        .map(|candidate| {
            build_suggestion(reasoned, schema, candidate, WarrantProse::ClassDisjointness)
        })
        .collect()
}

// ── The sortal class-disjointness lookup ─────────────────────────────────────────────

/// The clash set of an offered sortal `T`: every class `c` such that a subject already typed
/// `c` cannot ALSO be consistently typed `T` — i.e. `c` is disjoint with `T` or with any
/// reasoned superclass of `T`. Anchored on `{T} ∪ reasoned_superclasses(T)` because
/// disjointness propagates DOWN subclasses (`A ⊥ B ⇒ every subclass of A ⊥ every subclass of
/// B`), so a clash with a superclass of `T` is a clash with `T`. Read against the symmetric
/// `disjointness` index (every disjointness form; see [`disjointness_index`]).
///
/// SOUNDNESS vs. the old native engine: the engine refuted `rdf:type(s, T)` against `s:D` iff
/// `∃ A ⊇ D, B ⊇ T. A ⊥ B`. Intersecting `s`'s upward-closed reasoned types (`{D} ∪ super(D)`,
/// materialized on the reasoned graph) with this `{T} ∪ super(T)`-anchored clash set decides
/// exactly that condition — no over- or under-refutation, provided disjointness is read in
/// every form the engine reads (it is; see [`disjointness_index`]). A bare top-sortal
/// type-assertion has NO other inconsistency source, so this fully replaces the engine here.
fn clash_set(
    reasoned: &RdfDataset,
    sortal: &str,
    disjointness: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut anchors = superclasses(reasoned, sortal);
    anchors.insert(sortal.to_owned());
    let mut clash = BTreeSet::new();
    for anchor in &anchors {
        if let Some(disjoint_with_anchor) = disjointness.get(anchor) {
            clash.extend(disjoint_with_anchor.iter().cloned());
        }
    }
    clash
}

/// The SYMMETRIC class-disjointness index of the reasoned graph — `class → { classes disjoint
/// with it }` — built once and read by every offered sortal's [`clash_set`]. Captures EVERY
/// disjointness form that occurs in the reasoned object-level graph, so no refutation the old
/// native engine would find is missed:
///   * `owl:disjointWith` — the top-sortal form, read in BOTH directions (it is symmetric);
///   * `logic:disjointWith` — the canonical grounding form (Principle 17), likewise symmetric;
///   * `owl:AllDisjointClasses` + `owl:members` RDF list — expanded to every distinct pair of
///     members (pairwise disjointness).
///
/// IRI classes only: a disjointness edge onto a literal/blank is never a class-clash source.
fn disjointness_index(reasoned: &RdfDataset) -> BTreeMap<String, BTreeSet<String>> {
    let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // The two symmetric binary forms.
    for predicate in [OWL_DISJOINT_WITH.to_owned(), format!("{LOGIC}disjointWith")] {
        for (a, b) in binary_disjoint_pairs(reasoned, &predicate) {
            insert_symmetric(&mut index, a, b);
        }
    }
    // owl:AllDisjointClasses membership lists, expanded pairwise.
    for members in all_disjoint_member_lists(reasoned) {
        for (i, a) in members.iter().enumerate() {
            for b in &members[i + 1..] {
                insert_symmetric(&mut index, a.clone(), b.clone());
            }
        }
    }
    index
}

/// Record `a ⊥ b` symmetrically in the disjointness index (`owl:disjointWith` and
/// `logic:disjointWith` are symmetric; an `owl:AllDisjointClasses` membership makes every
/// distinct member pair mutually disjoint).
fn insert_symmetric(index: &mut BTreeMap<String, BTreeSet<String>>, a: String, b: String) {
    index.entry(a.clone()).or_default().insert(b.clone());
    index.entry(b).or_default().insert(a);
}

/// Every `(a, b)` IRI pair asserted `a <predicate> b` on the reasoned graph, for a binary
/// class-disjointness `predicate` (`owl:disjointWith` / `logic:disjointWith`). Order-as-read;
/// the caller symmetrizes. IRI-only (a disjointness onto a literal/blank is not a clash).
fn binary_disjoint_pairs(reasoned: &RdfDataset, predicate: &str) -> Vec<(String, String)> {
    let Some(pred_id) = term_id(reasoned, predicate) else {
        return Vec::new();
    };
    reasoned
        .quads_for_pattern(None, Some(pred_id), None, GraphMatch::Any)
        .filter_map(|q| match (reasoned.resolve(q.s), reasoned.resolve(q.o)) {
            (TermRef::Iri(a), TermRef::Iri(b)) => Some((a.to_owned(), b.to_owned())),
            _ => None,
        })
        .collect()
}

/// The `owl:members` IRI list of every `owl:AllDisjointClasses` node in the reasoned graph.
/// The AllDisjointClasses node is typically a BLANK node, so it is matched by TYPE (not by
/// IRI); each returned inner Vec is one membership, its elements the class IRIs. The caller
/// expands each to pairwise disjointness.
fn all_disjoint_member_lists(reasoned: &RdfDataset) -> Vec<Vec<String>> {
    let (Some(type_id), Some(add_id), Some(members_id)) = (
        term_id(reasoned, RDF_TYPE),
        term_id(reasoned, OWL_ALL_DISJOINT_CLASSES),
        term_id(reasoned, OWL_MEMBERS),
    ) else {
        return Vec::new();
    };
    // Collect the AllDisjointClasses node ids first so each list walk opens a fresh,
    // non-overlapping pattern scan.
    let nodes: Vec<TermId> = reasoned
        .quads_for_pattern(None, Some(type_id), Some(add_id), GraphMatch::Any)
        .map(|q| q.s)
        .collect();
    let mut lists = Vec::new();
    for node in nodes {
        let heads: Vec<TermId> = reasoned
            .quads_for_pattern(Some(node), Some(members_id), None, GraphMatch::Any)
            .map(|q| q.o)
            .collect();
        for head in heads {
            let members = list_iri_members(reasoned, head);
            if !members.is_empty() {
                lists.push(members);
            }
        }
    }
    lists
}

/// Walk an `rdf:first`/`rdf:rest`/`rdf:nil` list from `head`, collecting its IRI members in
/// order. Cycle-safe (each cell visited once) and tolerant of a malformed list (terminates on
/// `rdf:nil`, a missing `rdf:rest`, or a non-IRI cell), mirroring the frontend's list reader.
fn list_iri_members(reasoned: &RdfDataset, head: TermId) -> Vec<String> {
    let (Some(first_id), Some(rest_id)) =
        (term_id(reasoned, RDF_FIRST), term_id(reasoned, RDF_REST))
    else {
        return Vec::new();
    };
    let nil_id = term_id(reasoned, RDF_NIL);
    let mut out = Vec::new();
    let mut seen: BTreeSet<TermId> = BTreeSet::new();
    let mut cursor = head;
    loop {
        if Some(cursor) == nil_id || !seen.insert(cursor) {
            break;
        }
        if let Some(q) = reasoned
            .quads_for_pattern(Some(cursor), Some(first_id), None, GraphMatch::Any)
            .next()
            && let TermRef::Iri(iri) = reasoned.resolve(q.o)
        {
            out.push(iri.to_owned());
        }
        match reasoned
            .quads_for_pattern(Some(cursor), Some(rest_id), None, GraphMatch::Any)
            .next()
        {
            Some(q) => cursor = q.o,
            None => break,
        }
    }
    out
}

/// `rdf:type(subject, class)`.
fn type_atom(subject: &str, class: &str) -> Formula {
    Formula::Atom {
        relation: Term::Iri(RDF_TYPE.to_owned()),
        args: vec![Term::Iri(subject.to_owned()), Term::Iri(class.to_owned())],
    }
}

// ── Suggestion construction ──────────────────────────────────────────────────────────

/// The warrant discipline for a suggestion — selects the HONEST warrant + advisory prose.
/// The conjunctive/relatum path is warranted BY CONSTRUCTION (a fresh witness for a missing
/// relatum is a consistent addition); the sortal path is warranted by a sound
/// class-disjointness check (the subject holds no type disjoint with the offered sortal).
/// NEITHER path calls a reasoner — wording each truthfully is why this flag is threaded here.
enum WarrantProse {
    /// A relatum/conjunctive completion — warranted by construction, no engine call.
    ByConstruction,
    /// A sortal specialization — warranted by a class-disjointness lookup: the subject holds
    /// no reasoned type disjoint with the offered sortal, so adding it is consistent.
    ClassDisjointness,
}

/// Build the paired warrant [`Diag`] + [`Advisory`] for a warranted candidate, wording the
/// warrant HONESTLY per its `prose` discipline: a by-construction relatum argument, or a
/// class-disjointness argument for the sortal path. Neither claims a reasoner was run.
fn build_suggestion(
    reasoned: &RdfDataset,
    schema: &DiscoveredSchema,
    candidate: &Candidate,
    prose: WarrantProse,
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
    let warrant_id = format!("{discipline}.{digest}");

    let subject_q = qname(&candidate.subject);
    let object_q = qname(&candidate.object);
    let predicate_q = qname(&candidate.predicate);
    let term_q = qname(&schema.term);

    // The advisory message is the governed term's OWN live `gmeow:howToUse` prose (source
    // language), read from the reasoned graph — never a paraphrase. Honest fallback prose
    // when the term authors none, worded per the warrant discipline.
    let message = term_how_to_use(reasoned, &schema.term).unwrap_or_else(|| match &prose {
        WarrantProse::ByConstruction => format!(
            "Complete the under-specified {term_q} — a fresh witness for the missing relatum is \
             a consistent addition that satisfies its declared modeling discipline by construction."
        ),
        WarrantProse::ClassDisjointness => format!(
            "Complete the under-specified {term_q} — specializing to this top sortal is consistent \
             (the subject holds no type disjoint with it), a minimal addition satisfying its \
             declared modeling discipline."
        ),
    });

    let suggestion = match &prose {
        WarrantProse::ByConstruction => format!(
            "Add {} to {subject_q} — {}. A fresh witness for this missing relatum is a consistent \
             addition that completes {term_q}'s discipline by construction (a fresh untyped object \
             on an absent property can introduce no cardinality or disjointness clash).",
            candidate.element,
            candidate_reason(candidate, &predicate_q, &object_q),
        ),
        WarrantProse::ClassDisjointness => format!(
            "Add {} to {subject_q} — {}. {subject_q} holds no type disjoint with {object_q}, so this \
             specialization is consistent (a sound class-disjointness check, not a reasoning pass).",
            candidate.element,
            candidate_reason(candidate, &predicate_q, &object_q),
        ),
    };

    let advisory = Advisory::note(advice_code, message)
        .with_suggestion(suggestion)
        .with_subject_iri(candidate.subject.clone())
        .with_tag("abductive")
        .with_tag(format!("formalizes:{}", schema.term))
        .with_tag(format!("warrant:{warrant_id}"));

    let warrant_message = match &prose {
        WarrantProse::ByConstruction => format!(
            "abductive warrant (by construction): adding {predicate_q} {object_q} to {subject_q} \
             completes the {term_q} per-conjunct completeness formula ({discipline}) BY CONSTRUCTION \
             — {object_q} is a fresh content-addressed witness for a missing relatum, and adding a \
             fresh untyped object on an absent property can introduce no cardinality or disjointness \
             clash, so the addition is consistent with no native conjecture-engine call required."
        ),
        WarrantProse::ClassDisjointness => format!(
            "abductive warrant (class disjointness): adding {predicate_q} {object_q} to {subject_q} \
             completes the {term_q} sortal-specialization completeness formula ({discipline}) — \
             {subject_q} holds NO reasoned type disjoint with {object_q}, so adding this top sortal \
             is consistent. This is a sound class-disjointness lookup (a bare top-sortal \
             type-assertion is inconsistent only if the subject already holds a disjoint type); the \
             native conjecture engine is not used."
        ),
    };
    let warrant = Diag::note(register_code(&warrant_code), warrant_message);

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
    use purrdf::RdfDatasetBuilder;
    use std::sync::Arc;

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
