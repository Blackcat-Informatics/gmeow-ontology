// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The relational-core lowering lane (`logic:RelationalCore`) — the engine-agnostic,
//! oxigraph-free Datalog±-with-stratified-negation waist between the typed logic IR and
//! the physical execution engines.
//!
//! # What this lowers
//!
//! This lane lowers a [`LogicProgram`]'s **Horn `rules`** (and the ground, binary
//! `rdf:type`-style axioms) — which are ALREADY in the Datalog±/relational-core fragment
//! — into a first-class dialect ([`RelationalCoreProgram`]) of binary atoms and Horn
//! clauses. The whole rule set is supported, so the honest preservation claim is `{exact}`.
//!
//! # Why the legalization / residue mechanism exists even when the residue is empty
//!
//! A lowering is a **total function** into `⟨ legal output ⊕ flagged residue ⟩`
//! (`slices/grounding/logic/design/LOGIC-IR.md` § IR commitments — "Lowering is legalization").
//! Any rule or axiom outside the binary-Horn fragment is **carried as flagged unsupported
//! residue** ([`RcResidue`]), never silently dropped, and the preservation claim drops to
//! `{sound-under}` naming what was carried. For the current Horn corpus the residue is
//! empty — but the mechanism (and its fixture, in the tests) MUST exist so the lane is
//! honest by construction.
//!
//! The full-FOL Formula AST lowering plugs into THIS SAME lane. Its Horn-expressible
//! fragment produces the same binary [`RcRule`]s. **Fixed-arity n-ary predication** is
//! evaluable here too: at the lowering boundary a fixed-arity atom `Rel(a₀…aₙ)` is
//! **reified** into a conjunction of binary atoms over one reifier node —
//! `logic:instanceOf(R, Rel) ∧ logic:naryArg0(R, a₀) ∧ …` — so the relational core stays
//! **binary after reification**. A body atom binds a fresh reifier variable; a head atom
//! derives a new tuple over an existential reifier (carried in [`RcRule::head_conjuncts`]),
//! whose node the restricted chase mints by content identity. Only what genuinely exceeds
//! the fragment (a disjunctive head, a Skolem *function*, a genuinely unbounded
//! sequence-marker atom, or an n-ary head argument the body does not bind) is fed through
//! [`RelationalCoreProgram::push_residue`], so the carrier, the named-graph projection, and
//! the typed handle never change shape.
//!
//! # Dual carriage
//!
//! The dialect is carried BOTH as a typed handle payload (`Arc<RelationalCoreProgram>`)
//! and as a deterministic, byte-stable RDF projection into the `graph/relational-core`
//! named graph ([`project_relational_core`]); [`parse_relational_core`] is the inverse a
//! cache hit uses to re-derive the typed payload from the backing graph. The two faces
//! share one content identity ([`RelationalCoreProgram::content_key`]).

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::Diag;
use purrdf::{RdfDataset, RdfLiteral, RdfTerm};

use crate::ir::{
    Formula, LOGIC_NAMESPACE, LogicAxiom, LogicProgram, LogicRule, PreservationKind, Term,
};

const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Whether an atom occupies the head or a body position of the clause being
/// lowered. n-ary reification differs by position: a body n-ary atom binds a fresh
/// join variable over its reified conjunction, whereas a head n-ary atom *derives*
/// a new tuple and needs a content-addressed existential reifier witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtomPosition {
    Head,
    Body,
}

/// The HiLog-reflection typing predicate: `logic:instanceOf(R, Rel)` types a
/// reified n-ary tuple `R` by its relation individual `Rel` (see the reified n-ary
/// vocabulary in the `logic:` module). Reused rather than `rdf:type` so the object
/// level stays first-order and the certifier treats it as an ordinary binary atom.
fn instance_of_iri() -> String {
    format!("{LOGIC_NAMESPACE}instanceOf")
}

/// The flat positional predicate `logic:naryArg{i}(R, aᵢ)` carrying argument `i` of
/// a reified n-ary tuple.
fn nary_arg_iri(i: usize) -> String {
    format!("{LOGIC_NAMESPACE}naryArg{i}")
}

// ── Vocabulary IRIs (the queryable `logic:` surface of the projection) ──────────

/// The singleton subject describing the whole lowered program.
fn program_iri() -> String {
    format!("{LOGIC_NAMESPACE}relational-core/program")
}
fn class_program() -> String {
    format!("{LOGIC_NAMESPACE}RelationalCore")
}
fn class_fact() -> String {
    format!("{LOGIC_NAMESPACE}RelationalCoreFact")
}
fn class_rule() -> String {
    format!("{LOGIC_NAMESPACE}RelationalCoreRule")
}
fn class_atom() -> String {
    format!("{LOGIC_NAMESPACE}RelationalCoreAtom")
}
fn p_has_preservation() -> String {
    format!("{LOGIC_NAMESPACE}hasPreservation")
}
fn p_lossy_drop() -> String {
    format!("{LOGIC_NAMESPACE}lossyDrop")
}
fn p_has_fact() -> String {
    format!("{LOGIC_NAMESPACE}hasFact")
}
fn p_has_rule() -> String {
    format!("{LOGIC_NAMESPACE}hasRule")
}
fn p_rc_subject() -> String {
    format!("{LOGIC_NAMESPACE}rcSubject")
}
fn p_rc_predicate() -> String {
    format!("{LOGIC_NAMESPACE}rcPredicate")
}
fn p_rc_object() -> String {
    format!("{LOGIC_NAMESPACE}rcObject")
}
fn p_rc_object_literal() -> String {
    format!("{LOGIC_NAMESPACE}rcObjectLiteral")
}
fn p_rc_negated() -> String {
    format!("{LOGIC_NAMESPACE}rcNegated")
}
fn p_rc_head() -> String {
    format!("{LOGIC_NAMESPACE}rcHead")
}
fn p_rc_head_conjunct() -> String {
    format!("{LOGIC_NAMESPACE}rcHeadConjunct")
}
fn p_rc_body() -> String {
    format!("{LOGIC_NAMESPACE}rcBody")
}
fn p_rc_index() -> String {
    format!("{LOGIC_NAMESPACE}rcIndex")
}
fn p_rc_distinct() -> String {
    format!("{LOGIC_NAMESPACE}rcDistinct")
}
fn p_rc_distinct_left() -> String {
    format!("{LOGIC_NAMESPACE}rcDistinctLeft")
}
fn p_rc_distinct_right() -> String {
    format!("{LOGIC_NAMESPACE}rcDistinctRight")
}
fn p_source_iri() -> String {
    format!("{LOGIC_NAMESPACE}sourceIri")
}

// --------------------------------------------------------------------------- //
// The dialect
// --------------------------------------------------------------------------- //

/// A relational-core term: a logical variable (`?x`), an IRI constant, a blank node
/// (an existential the IR carries as a canonical `c14nN` label), or a literal value
/// (object position only). The relational core is **binary** — a fact/atom is a
/// `subject predicate object` triple.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RcTerm {
    /// A logical variable carrying its `?` sigil.
    Var(String),
    /// An IRI constant (absolute IRI).
    Iri(String),
    /// A blank node carrying its canonical label (no `_:` prefix), e.g. a Skolem-style
    /// existential the lowering keeps by reference.
    Blank(String),
    /// A literal value (legal only in the object position).
    Literal(String),
}

impl RcTerm {
    fn from_value(value: &str, is_literal: bool) -> Self {
        // A declared literal MUST win before the variable-syntax heuristic: a lexical
        // form that happens to start with '?' (e.g. a string literal "?foo") must be
        // classified as Literal, not Var.
        if is_literal {
            Self::Literal(value.to_owned())
        } else if value.starts_with('?') {
            Self::Var(value.to_owned())
        } else if is_absolute_iri(value) {
            Self::Iri(value.to_owned())
        } else {
            // A non-variable, non-literal, non-absolute term is a blank node: the IR
            // carries an existential / canonicalized blank as a bare `c14nN` label.
            Self::Blank(value.trim_start_matches("_:").to_owned())
        }
    }

    /// A deterministic, type-tagged content key for this term — the single authority shared
    /// by the projection, the parse inverse, and the engine adapter's rule-IRI minting.
    pub fn key(&self) -> String {
        match self {
            Self::Var(v) => format!("V\u{1f}{v}"),
            Self::Iri(i) => format!("I\u{1f}{i}"),
            Self::Blank(b) => format!("B\u{1f}{b}"),
            Self::Literal(l) => format!("L\u{1f}{l}"),
        }
    }

    fn key_with_blanks(&self, blanks: &BTreeMap<String, String>) -> String {
        match self {
            Self::Blank(b) => {
                let canonical = blanks.get(b).map_or(b.as_str(), String::as_str);
                format!("B\u{1f}{canonical}")
            }
            _ => self.key(),
        }
    }

    fn collect_blanks(&self, blanks: &mut BTreeSet<String>) {
        if let Self::Blank(b) = self {
            blanks.insert(b.clone());
        }
    }
}

/// Whether `value` is an absolute IRI (carries a `scheme:` prefix). A bare token (a
/// canonicalized blank-node label like `c14n44`) is NOT absolute.
fn is_absolute_iri(value: &str) -> bool {
    // An absolute IRI has a scheme: an alpha followed by alnum/+/-/. then ':'.
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

/// A binary relational-core atom (`subject predicate object`), optionally negated
/// (stratified negation-as-failure in a rule body).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RcAtom {
    /// The subject term.
    pub subject: RcTerm,
    /// The predicate IRI.
    pub predicate: String,
    /// The object term.
    pub object: RcTerm,
    /// Whether this atom is a NAF body literal (always `false` for a head or a fact).
    pub negated: bool,
}

impl RcAtom {
    /// A deterministic content key for this atom (subject · predicate · object · negated),
    /// used by the projection, the parse inverse, and the engine adapter's rule-IRI minting.
    pub fn key(&self) -> String {
        format!(
            "{}\u{1e}{}\u{1e}{}\u{1e}{}",
            self.subject.key(),
            self.predicate,
            self.object.key(),
            self.negated,
        )
    }

    fn key_with_blanks(&self, blanks: &BTreeMap<String, String>) -> String {
        format!(
            "{}\u{1e}{}\u{1e}{}\u{1e}{}",
            self.subject.key_with_blanks(blanks),
            self.predicate,
            self.object.key_with_blanks(blanks),
            self.negated,
        )
    }

    fn collect_blanks(&self, blanks: &mut BTreeSet<String>) {
        self.subject.collect_blanks(blanks);
        self.object.collect_blanks(blanks);
    }
}

/// A relational-core Horn rule: a single head atom derived from a conjunctive body of
/// atoms (positive or NAF-negated) plus inequality `!=` guards.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RcRule {
    /// The derived head atom.
    pub head: RcAtom,
    /// The EXTRA reified head atoms beyond [`Self::head`] — the positional
    /// `logic:naryArg{i}(R, aᵢ)` conjunction of a derived n-ary tuple whose typing
    /// atom `logic:instanceOf(R, Rel)` is carried in [`Self::head`]. EMPTY for every
    /// ordinary binary/unary-head rule; non-empty only for an n-ary head-derivation
    /// rule (the shared existential reifier `R` is the head subject).
    pub head_conjuncts: Vec<RcAtom>,
    /// The body atoms, in canonical (sorted) order.
    pub body: Vec<RcAtom>,
    /// Inequality guards (each pair internally sorted, the set sorted).
    pub distinct_pairs: Vec<(String, String)>,
}

impl RcRule {
    /// A deterministic content key for this rule (head · body atoms · distinct guards),
    /// used by the projection, the parse inverse, and the engine adapter's rule-IRI minting.
    pub fn key(&self) -> String {
        // The head segment folds the head atom then every head conjunct (in order),
        // so it is empty-suffix-stable: an ordinary rule (no conjuncts) keys exactly
        // as before, while an n-ary head-derivation rule keys distinctly.
        let mut head = self.head.key();
        for conjunct in &self.head_conjuncts {
            head.push('\u{1d}');
            head.push_str(&conjunct.key());
        }
        let body = self
            .body
            .iter()
            .map(RcAtom::key)
            .collect::<Vec<_>>()
            .join("\u{1d}");
        let distinct = self
            .distinct_pairs
            .iter()
            .map(|(a, b)| format!("{a}\u{1f}{b}"))
            .collect::<Vec<_>>()
            .join("\u{1d}");
        format!("{head}\u{1c}{body}\u{1c}{distinct}")
    }

    fn key_with_blanks(&self, blanks: &BTreeMap<String, String>) -> String {
        let mut head = self.head.key_with_blanks(blanks);
        for conjunct in &self.head_conjuncts {
            head.push('\u{1d}');
            head.push_str(&conjunct.key_with_blanks(blanks));
        }
        let body = self
            .body
            .iter()
            .map(|atom| atom.key_with_blanks(blanks))
            .collect::<Vec<_>>()
            .join("\u{1d}");
        let distinct = self
            .distinct_pairs
            .iter()
            .map(|(a, b)| format!("{a}\u{1f}{b}"))
            .collect::<Vec<_>>()
            .join("\u{1d}");
        format!("{head}\u{1c}{body}\u{1c}{distinct}")
    }

    fn collect_blanks(&self, blanks: &mut BTreeSet<String>) {
        self.head.collect_blanks(blanks);
        for atom in &self.head_conjuncts {
            atom.collect_blanks(blanks);
        }
        for atom in &self.body {
            atom.collect_blanks(blanks);
        }
    }
}

/// One flagged unsupported residue entry: a construct outside the binary-Horn fragment,
/// carried with the reason it could not be legalized (never silently dropped).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RcResidue {
    /// A stable, human-readable description of the carried-and-flagged construct.
    pub reason: String,
}

/// The lowered relational-core dialect: ground facts, Horn rules, and the honest
/// preservation claim (`{exact}` when the residue is empty, else `{sound-under}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationalCoreProgram {
    /// Ground binary facts (the program's ground axioms).
    pub facts: Vec<RcAtom>,
    /// Horn rules in canonical order.
    pub rules: Vec<RcRule>,
    /// Flagged unsupported residue, sorted and deduplicated.
    pub residue: Vec<RcResidue>,
    /// The source-graph IRI of the program this was lowered from (provenance).
    pub source_iri: Option<String>,
}

impl RelationalCoreProgram {
    /// Carry a construct that fell outside the binary-Horn fragment as flagged residue
    /// (the legalization floor). This is the seam the full-FOL formula lowering will
    /// call for every non-Horn formula it cannot evaluate.
    pub fn push_residue(&mut self, reason: impl Into<String>) {
        self.residue.push(RcResidue {
            reason: reason.into(),
        });
    }

    /// The honest preservation kind: [`PreservationKind::Exact`] when nothing was
    /// carried as residue, else [`PreservationKind::SoundUnder`] — a lane that drops a
    /// derivation it cannot evaluate is a sound *under*-approximation, never a false
    /// `{exact}`.
    pub fn preservation(&self) -> PreservationKind {
        if self.residue.is_empty() {
            PreservationKind::Exact
        } else {
            PreservationKind::SoundUnder
        }
    }

    /// A single deterministic, order-independent content key for the whole lowered
    /// program — the content identity the typed handle and the named-graph projection
    /// share.
    pub fn content_key(&self) -> String {
        let blanks = self.canonical_rule_blank_labels();
        let facts = canonical_fact_key(&self.facts);
        let rules = {
            let mut keys: Vec<String> = self
                .rules
                .iter()
                .map(|rule| rule.key_with_blanks(&blanks))
                .collect();
            keys.sort();
            keys.join("\n")
        };
        let residue = {
            let mut keys: Vec<String> = self.residue.iter().map(|r| r.reason.clone()).collect();
            keys.sort();
            keys.join("\n")
        };
        format!(
            "RELATIONAL-CORE\nFACTS\n{facts}\nRULES\n{rules}\nRESIDUE\n{residue}\nPRESERVATION\n{}\nSOURCE\n{}",
            self.preservation().as_str(),
            self.source_iri.as_deref().unwrap_or(""),
        )
    }

    fn canonical_rule_blank_labels(&self) -> BTreeMap<String, String> {
        let mut labels = BTreeSet::new();
        for rule in &self.rules {
            rule.collect_blanks(&mut labels);
        }
        labels
            .into_iter()
            .enumerate()
            .map(|(index, label)| (label, format!("b{index}")))
            .collect()
    }
}

fn canonical_fact_key(facts: &[RcAtom]) -> String {
    let mut lines = facts
        .iter()
        .map(|fact| {
            format!(
                "{} <{}> {} .",
                term_nt(&fact.subject),
                fact.predicate,
                term_nt(&fact.object)
            )
        })
        .collect::<Vec<_>>();
    lines.sort();
    lines.dedup();
    let mut nt = lines.join("\n");
    nt.push('\n');

    match purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| e.to_string())
        .and_then(|dataset| purrdf::canonical_flat_nquads(dataset.as_ref()))
    {
        Ok(canonical) => canonical,
        Err(_) => {
            let mut keys: Vec<String> = facts.iter().map(RcAtom::key).collect();
            keys.sort();
            keys.join("\n")
        }
    }
}

// --------------------------------------------------------------------------- //
// Lowering: LogicProgram → RelationalCoreProgram
// --------------------------------------------------------------------------- //

/// Lower a [`LogicProgram`]'s ground axioms and Horn `rules` into the relational core,
/// partial-converting anything outside the binary-Horn fragment to flagged unsupported
/// residue.
///
/// Main's `rules` are already Horn/Datalog±, so the whole rule set lowers and the residue
/// is empty (`{exact}`). The residue mechanism is exercised by [`lower_with_residue`] and
/// the test fixtures — it is the seam the richer full-FOL lowering plugs into.
pub fn lower_program(program: &LogicProgram) -> RelationalCoreProgram {
    let mut facts: Vec<RcAtom> = Vec::new();
    let mut rules: Vec<RcRule> = Vec::new();
    let mut residue: BTreeSet<String> = BTreeSet::new();

    for axiom in &program.axioms {
        match lower_axiom(axiom) {
            Ok(atom) => facts.push(atom),
            Err(reason) => {
                residue.insert(reason.message().to_owned());
            }
        }
    }
    for rule in &program.rules {
        match lower_rule(rule) {
            Ok(rc) => rules.push(rc),
            Err(reason) => {
                residue.insert(reason.message().to_owned());
            }
        }
    }

    finalize(facts, rules, residue, program.source_iri.clone())
}

/// Lower a program AND fold in extra flagged residue from a richer (e.g. full-FOL)
/// lowering whose non-Horn formulas were carried-and-flagged. This is the exact seam
/// the formula AST will feed: the Horn fragment lowers via [`lower_program`], the rest
/// arrives here as `extra_residue`, and the preservation claim drops to `{sound-under}`.
pub fn lower_with_residue<I, S>(program: &LogicProgram, extra_residue: I) -> RelationalCoreProgram
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut lowered = lower_program(program);
    for r in extra_residue {
        lowered.push_residue(r);
    }
    // Re-canonicalize the residue (sorted + deduplicated).
    let set: BTreeSet<String> = lowered.residue.into_iter().map(|r| r.reason).collect();
    lowered.residue = set.into_iter().map(|reason| RcResidue { reason }).collect();
    lowered
}

fn finalize(
    mut facts: Vec<RcAtom>,
    mut rules: Vec<RcRule>,
    residue: BTreeSet<String>,
    source_iri: Option<String>,
) -> RelationalCoreProgram {
    facts.sort();
    facts.dedup();
    rules.sort();
    rules.dedup();
    RelationalCoreProgram {
        facts,
        rules,
        residue: residue
            .into_iter()
            .map(|reason| RcResidue { reason })
            .collect(),
        source_iri,
    }
}

/// Lower one axiom to a ground binary fact, or flag it as residue when it is not a
/// ground binary atom (the relational core is binary; an axiom whose subject or object
/// is itself a variable-free non-IRI/non-literal would be unsupported).
fn lower_axiom(axiom: &LogicAxiom) -> gmeow_errors::Result<RcAtom> {
    let subject = RcTerm::from_value(&axiom.subject, false);
    let object = RcTerm::from_value(&axiom.obj, axiom.obj_is_literal);
    Ok(RcAtom {
        subject,
        predicate: axiom.predicate.clone(),
        object,
        negated: axiom.negated,
    })
}

/// Lower one Horn rule. Main's `LogicRule` is already a single-head Horn clause of
/// binary atoms, so this is a faithful, total lowering. The Result-returning shape is
/// the legalization seam: a future non-Horn rule (a disjunctive head, a non-binary
/// atom) would return `Err(reason)` to be carried as residue rather than mis-lowered.
fn lower_rule(rule: &LogicRule) -> gmeow_errors::Result<RcRule> {
    // A stratified aggregation (reduce) rule is outside the binary-Horn fragment the relational
    // core models: it is carried as flagged residue (never silently dropped, never mis-lowered to
    // a plain Horn rule), and projections that can express it emit it from the
    // LogicProgram directly.
    if let Some(agg) = &rule.aggregation {
        return Err(Diag::of_kind(crate::error::RelationalCore {
            detail: format!(
                "rule deriving <{}> uses aggregation ({} of {} over {}), outside the binary-Horn \
             relational core; carried in the logic: canon for aggregation-capable surfaces",
                rule.head.predicate,
                agg.function,
                agg.aggregate_var,
                if agg.group_keys.is_empty() {
                    "the whole relation".to_owned()
                } else {
                    agg.group_keys.join(", ")
                }
            ),
        }));
    }
    let head = lower_body_atom(&rule.head);
    let body: Vec<RcAtom> = rule.body.iter().map(lower_body_atom).collect();
    let distinct_pairs = rule.distinct_pairs.clone();
    Ok(RcRule {
        head: RcAtom {
            negated: false,
            ..head
        },
        head_conjuncts: Vec::new(),
        body,
        distinct_pairs,
    })
}

fn lower_body_atom(axiom: &LogicAxiom) -> RcAtom {
    RcAtom {
        subject: RcTerm::from_value(&axiom.subject, false),
        predicate: axiom.predicate.clone(),
        object: RcTerm::from_value(&axiom.obj, axiom.obj_is_literal),
        negated: axiom.negated,
    }
}

// --------------------------------------------------------------------------- //
// Full-FOL formula lowering: Formula → RcRule + flagged residue
// --------------------------------------------------------------------------- //
//
// The seam the lane's module doc reserves: a [`LogicProgram::formulas`] entry is
// negation-normal-form rewritten, its leading existential prefix Skolemized to constants,
// and the Horn-expressible clauses extracted as [`RcRule`]s the engines run; everything
// outside the binary-Horn fragment (a disjunctive head, a quantifier alternation `∃` under
// `∀`, a non-binary or sequence-marker atom, a strong negation that is not a clause literal)
// is carried as flagged residue ([`RcResidue`]) tagged with the closed [`FormulaShape`] set,
// never silently dropped (the legalization rule of `design/LOGIC-IR.md`). This is the ONE
// place a `Formula` becomes Horn — the physical engines map these `RcRule`s onward, never
// re-clausifying.

/// Lower a program with its full-FOL formulas: the Horn-expressible formula fragment joins
/// the lane's rules, the non-Horn remainder is carried via [`RelationalCoreProgram::push_residue`]
/// so the preservation claim drops to `{sound-under}` naming what was carried. A formula-free
/// program is identical to [`lower_program`], so the historical Horn corpus is byte-unchanged.
pub fn lower_program_with_formulas(program: &LogicProgram) -> RelationalCoreProgram {
    let base = lower_program(program);
    let (formula_rules, formula_residue) = lower_formulas_to_rc(program);

    let mut rules = base.rules;
    rules.extend(formula_rules);
    let mut residue: BTreeSet<String> = base.residue.into_iter().map(|r| r.reason).collect();
    residue.extend(formula_residue);

    finalize(base.facts, rules, residue, base.source_iri)
}

/// Lower ONLY a program's full-FOL formulas to the relational core, returning the
/// Horn-expressible fragment as [`RcRule`]s and the non-Horn remainder as flagged residue
/// reason strings. The physical-engine adapter uses this to evaluate the formula layer
/// without re-lowering the program's already-Horn `rules`.
pub fn lower_formulas_to_rc(program: &LogicProgram) -> (Vec<RcRule>, Vec<String>) {
    let mut rules: Vec<RcRule> = Vec::new();
    let mut residue: BTreeSet<String> = BTreeSet::new();
    for formula in &program.formulas {
        let normalized = skolemize(nnf(formula));
        lower_formula_top(&normalized, formula, &mut rules, &mut residue);
    }
    // Dedup by content key (stable first-wins, preserving canonical formula-source order).
    // Uses the same key as `LogicRule::new` for authored rules, making the lane the single
    // dedup authority. Must NOT use `rules.sort(); rules.dedup()` — derived `Ord` orders
    // by `RcTerm` variant-declaration order (not lexically), which would reorder rules.
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    rules.retain(|rc| seen.insert(rc.key()));
    (rules, residue.into_iter().collect())
}

/// Lower a top-level (already NNF + Skolemized) formula: peel the universal closure, flatten
/// a top-level conjunction into independent clauses, and lower each clause — pushing an
/// [`RcRule`] when it is a Horn clause of binary atoms, else recording an honest residue
/// entry keyed on the ORIGINAL `source` formula (so the disclosure is stable and names the
/// `FormulaShape` constructs that exceeded the fragment).
fn lower_formula_top(
    normalized: &Formula,
    source: &Formula,
    rules: &mut Vec<RcRule>,
    residue: &mut BTreeSet<String>,
) {
    match normalized {
        // A universal binder merely closes the rule's variables.
        Formula::Forall { body, .. } => lower_formula_top(body, source, rules, residue),
        // A conjunction at the top is independent clauses / assertions.
        Formula::And(fs) => {
            for f in fs {
                lower_formula_top(f, source, rules, residue);
            }
        }
        // A clause (disjunction of literals) or a bare atom.
        clause => match lower_formula_clause(clause) {
            Ok(rule) => rules.push(rule),
            Err(reason) => {
                residue.insert(formula_residue_reason(source, reason));
            }
        },
    }
}

/// The stable, self-describing residue note for a carried (non-Horn) formula: the specific
/// reason, the closed [`FormulaShape`] tag set naming *which* first-order constructs exceed
/// the fragment, and the formula's alpha-normalized content-key digest (so two
/// alpha-equivalent formulas disclose identically and the goldens stay byte-stable).
fn formula_residue_reason(source: &Formula, reason: &str) -> String {
    let tags = source
        .shape_tags()
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("+");
    format!("{reason} [{tags}] [{}]", sha256_12(&source.content_key()))
}

/// Flatten a (possibly nested) disjunction into its flat list of clause literals. NNF
/// leaves an implication body as a nested `Or` tree, so `Or[Or[a,b],c]` must read as the
/// flat clause `a ∨ b ∨ c` for the Horn check to see one head + N body literals.
fn flatten_or<'a>(f: &'a Formula, out: &mut Vec<&'a Formula>) {
    match f {
        Formula::Or(fs) => fs.iter().for_each(|x| flatten_or(x, out)),
        other => out.push(other),
    }
}

/// A canonical lexical sort key for an [`RcAtom`] that mirrors [`crate::ir::LogicAxiom::sort_key`]
/// byte-for-byte, so that the formula lane's body order equals the order produced by
/// `lower_rule(LogicRule::new(...))` for any equivalent Horn clause.
///
/// Key shape: `subject_surface\u{0}predicate\u{0}object_surface\u{0}{obj_is_literal}` with a
/// trailing `\u{0}{negated_bool}` appended ONLY when `negated == true` — exactly matching
/// `LogicAxiom::sort_key`'s conditional-append shape.
///
/// The *surface* of a term is the inner string of the [`RcTerm`] variant (e.g. `?x` for
/// `Var("?x")`), NOT the type-tagged [`RcTerm::key`] form (no `V`/`I` prefix).  This matches
/// `LogicAxiom`'s `subject`/`obj` fields which carry the same raw string.
fn rc_atom_sort_key(atom: &RcAtom) -> String {
    // The null byte separator, matching the private `SEP` constant in `ir.rs`.
    const SEP: char = '\u{0}';
    // Python-bool rendering, matching the private `py_bool` helper in `ir.rs`.
    let py_bool = |b: bool| if b { "True" } else { "False" };

    let subject_surface = match &atom.subject {
        RcTerm::Var(v) => v.as_str(),
        RcTerm::Iri(i) => i.as_str(),
        RcTerm::Blank(b) => b.as_str(),
        RcTerm::Literal(l) => l.as_str(),
    };
    let object_surface = match &atom.object {
        RcTerm::Var(v) => v.as_str(),
        RcTerm::Iri(i) => i.as_str(),
        RcTerm::Blank(b) => b.as_str(),
        RcTerm::Literal(l) => l.as_str(),
    };
    let obj_is_literal = matches!(atom.object, RcTerm::Literal(_));
    let mut key = format!(
        "{subject_surface}{SEP}{}{SEP}{object_surface}{SEP}{}",
        atom.predicate,
        py_bool(obj_is_literal),
    );
    // Append negated flag ONLY when true — mirrors LogicAxiom::sort_key's conditional append.
    if atom.negated {
        key.push(SEP);
        key.push_str(py_bool(atom.negated));
    }
    key
}

/// Lower a single clause to a Horn [`RcRule`]. A clause is a bare atom, a strong negation of
/// an atom, or a disjunction of those; Horn requires exactly one positive literal (the head),
/// the rest negative (clause `A ∨ ¬B ∨ ¬C` ≡ rule `A ← B ∧ C`, the body atoms positive).
fn lower_formula_clause(clause: &Formula) -> Result<RcRule, &'static str> {
    // A clause may be a nested Or tree after NNF; flatten it into a flat literal list
    // before the Horn check so one head + N negated body literals are seen correctly.
    let mut literals: Vec<&Formula> = Vec::new();
    flatten_or(clause, &mut literals);

    let mut head: Option<&Formula> = None;
    let mut body_atoms: Vec<&Formula> = Vec::new();
    for lit in literals {
        match lit {
            Formula::Atom { .. } => {
                if head.is_some() {
                    // Two positive literals → not Horn (a disjunctive head).
                    return Err("disjunctive head: clause is not Horn (>1 positive literal)");
                }
                head = Some(lit);
            }
            // `¬B` in the clause becomes a positive body atom `B` in the rule.
            Formula::Not(inner) if matches!(**inner, Formula::Atom { .. }) => {
                body_atoms.push(inner);
            }
            _ => return Err("non-relational-core formula (not a Horn clause of binary atoms)"),
        }
    }

    let head = head.ok_or("headless clause (no positive literal; an integrity constraint)")?;
    // Build the reified body FIRST — a head n-ary atom's range-restriction check needs the
    // set of variables the body binds. Each body atom lowers to one (binary/unary) or
    // several (reified n-ary) atoms.
    let mut body: Vec<RcAtom> = Vec::with_capacity(body_atoms.len());
    for a in &body_atoms {
        body.extend(formula_atom_to_rc_atoms(a, AtomPosition::Body)?);
    }
    // Sort the body atoms in the canonical order that mirrors `LogicRule::new`'s
    // `sort_by_cached_key(LogicAxiom::sort_key)` so the formula lane and the Horn rule
    // lane produce identical body orderings for any equivalent clause.
    body.sort_by_cached_key(rc_atom_sort_key);

    // An n-ary head atom (arity ≥ 3, IRI relation) DERIVES a new tuple: it reifies into a
    // conjunctive-head existential rule over a fresh content-addressed reifier `R`, handled
    // here (not by the per-atom converter, whose `AtomPosition::Head` arm stays a residue
    // seam for the single-head path).
    if let Formula::Atom {
        relation: Term::Iri(rel),
        args,
    } = head
        && args.len() >= 3
    {
        return lower_nary_head_clause(rel, args, body);
    }

    // A binary/unary head reifies to exactly one head atom (a Horn rule has a single head).
    let head_atoms = formula_atom_to_rc_atoms(head, AtomPosition::Head)?;
    let [head_atom] = head_atoms.as_slice() else {
        return Err("head atom did not reduce to a single relational-core atom");
    };
    let head_atom = head_atom.clone();
    Ok(RcRule {
        head: head_atom,
        head_conjuncts: Vec::new(),
        body,
        distinct_pairs: Vec::new(),
    })
}

/// Lower an n-ary (arity ≥ 3) HEAD atom `Rel(a₀,…,aₙ)` into a conjunctive-head existential
/// rule: `logic:instanceOf(R, Rel) ∧ logic:naryArg0(R, a₀) ∧ … :- <reified body>`, where `R`
/// is a fresh existential reifier variable the chase mints once per firing and shares across
/// the whole head conjunction (the reified tuple's node). `R` is keyed on the atom's raw
/// syntactic form (relation + ordered authored arg keys) so re-authoring the same head atom
/// reuses one reifier while a distinct head atom gets a distinct one.
///
/// # Range restriction (the one head hard-fail)
///
/// Every head argument that is a variable MUST be bound by the body: a head variable the
/// body does not bind is a non-range-restricted (unsafe) existential, so the clause is
/// carried as residue (`Err`) rather than lowered to an unsafe rule. The reifier `R` is the
/// existential and is NOT expected to be body-bound.
fn lower_nary_head_clause(
    rel: &str,
    args: &[Term],
    body: Vec<RcAtom>,
) -> Result<RcRule, &'static str> {
    // Reified argument terms (object position: variables, IRIs, literals all legal; a
    // sequence-marker argument rejects, keeping a genuinely-variadic head as residue).
    let arg_terms: Vec<RcTerm> = args
        .iter()
        .map(|a| formula_term_to_rc(a, true))
        .collect::<Result<_, _>>()?;

    // Range-restriction hard-fail: every variable head argument must be bound by the body.
    let body_bound = body_bound_vars(&body);
    for t in &arg_terms {
        if let RcTerm::Var(v) = t
            && !body_bound.contains(v)
        {
            return Err(
                "n-ary head argument not bound by the body (a non-range-restricted existential \
                 is unsafe)",
            );
        }
    }

    // Fresh existential reifier keyed on the raw syntactic head atom (relation + ordered
    // authored argument term keys — NOT the alpha-normalized content_key, which would
    // unsoundly collapse distinct head atoms).
    let mut syntactic_key = rel.to_owned();
    for t in &arg_terms {
        syntactic_key.push('\u{1f}');
        syntactic_key.push_str(&t.key());
    }
    // Underscore-free, letter-first for portability across text projections; `naryH` = the
    // reified HEAD reifier, a distinct namespace from the `naryB` body reifier.
    let reifier = RcTerm::Var(format!("?naryH{}", sha256_12(&syntactic_key)));

    let head = RcAtom {
        subject: reifier.clone(),
        predicate: instance_of_iri(),
        object: RcTerm::Iri(rel.to_owned()),
        negated: false,
    };
    let head_conjuncts: Vec<RcAtom> = arg_terms
        .into_iter()
        .enumerate()
        .map(|(i, obj)| RcAtom {
            subject: reifier.clone(),
            predicate: nary_arg_iri(i),
            object: obj,
            negated: false,
        })
        .collect();

    Ok(RcRule {
        head,
        head_conjuncts,
        body,
        distinct_pairs: Vec::new(),
    })
}

/// The set of variable names the body binds (either subject or object position of any body
/// atom) — the frontier available to a head atom's range-restriction check.
fn body_bound_vars(body: &[RcAtom]) -> BTreeSet<String> {
    let mut vars = BTreeSet::new();
    for atom in body {
        if let RcTerm::Var(v) = &atom.subject {
            vars.insert(v.clone());
        }
        if let RcTerm::Var(v) = &atom.object {
            vars.insert(v.clone());
        }
    }
    vars
}

/// Lower a [`Formula::Atom`] to one or more binary [`RcAtom`]s. A binary atom is a
/// direct triple; every other fixed arity is **reified** into a conjunction of
/// ordinary binary atoms over a single reifier node `R` (the flat-binary n-ary
/// encoding): `logic:instanceOf(R, Rel) ∧ logic:naryArg0(R, a₀) ∧ …`. A clause
/// literal is always positive in the rule (strong negation `¬B` was peeled into a
/// positive body atom by [`lower_formula_clause`]).
///
/// - arity 2 → the direct `subject predicate object` triple (unchanged binary path).
/// - arity 1 → `logic:instanceOf(a₀, Rel)` — unary predication under the HiLog reflection.
/// - arity ≥ 3, **body** → the reified conjunction over a fresh join variable `R`.
///   `R` is keyed on the atom's *raw syntactic form* (relation + ordered argument
///   term keys, with authored variable names intact) via [`sha256_12`], so distinct
///   atoms `rel(?x,?y,?z)` and `rel(?u,?v,?w)` get distinct reifier vars (their raw
///   keys differ) while two occurrences of the *same* atom share one `R`. This keying
///   is order-independent of body position, so the canonical body sort in
///   [`lower_formula_clause`] keeps rule identity stable regardless of authored order —
///   and it is NOT the alpha-normalized `content_key`, which would unsoundly collapse
///   `rel(?x,?y,?z)` and `rel(?u,?v,?w)` into one variable.
/// - arity ≥ 3, **head** → deriving a new n-ary tuple; handled by the head-derivation
///   lowering (a conjunctive existential rule), not this per-atom converter.
fn formula_atom_to_rc_atoms(
    atom: &Formula,
    pos: AtomPosition,
) -> Result<Vec<RcAtom>, &'static str> {
    let Formula::Atom { relation, args } = atom else {
        return Err("clause literal is not an atom");
    };
    let Term::Iri(rel) = relation else {
        return Err("non-IRI relation in atom");
    };
    match args.len() {
        2 => {
            let subject = formula_term_to_rc(&args[0], false)?;
            let object = formula_term_to_rc(&args[1], true)?;
            Ok(vec![RcAtom {
                subject,
                predicate: rel.clone(),
                object,
                negated: false,
            }])
        }
        1 => {
            // Unary predication Rel(a) ≡ instanceOf(a, Rel) under the HiLog reflection.
            let subject = formula_term_to_rc(&args[0], false)?;
            Ok(vec![RcAtom {
                subject,
                predicate: instance_of_iri(),
                object: RcTerm::Iri(rel.clone()),
                negated: false,
            }])
        }
        0 => {
            Err("nullary atom (a proposition constant is not representable in the relational core)")
        }
        _ => match pos {
            AtomPosition::Body => {
                // Reified argument terms (object position: variables, IRIs, literals
                // are all legal). A sequence-marker argument rejects here, keeping a
                // genuinely-variadic atom as residue.
                let arg_terms: Vec<RcTerm> = args
                    .iter()
                    .map(|a| formula_term_to_rc(a, true))
                    .collect::<Result<_, _>>()?;
                // Reifier join variable keyed on the raw syntactic atom (relation +
                // ordered argument term keys). Order-independent of body position;
                // distinct atoms → distinct vars; identical atoms → shared var.
                let mut syntactic_key = rel.clone();
                for t in &arg_terms {
                    syntactic_key.push('\u{1f}');
                    syntactic_key.push_str(&t.key());
                }
                // The reifier name is deliberately underscore-free and letter-first for
                // portability across text projections. `naryB` = the reified BODY join reifier
                // (distinct namespace from the `naryH` head reifier).
                let reifier = RcTerm::Var(format!("?naryB{}", sha256_12(&syntactic_key)));
                let mut out = Vec::with_capacity(arg_terms.len() + 1);
                out.push(RcAtom {
                    subject: reifier.clone(),
                    predicate: instance_of_iri(),
                    object: RcTerm::Iri(rel.clone()),
                    negated: false,
                });
                for (i, obj) in arg_terms.into_iter().enumerate() {
                    out.push(RcAtom {
                        subject: reifier.clone(),
                        predicate: nary_arg_iri(i),
                        object: obj,
                        negated: false,
                    });
                }
                Ok(out)
            }
            // An n-ary atom deriving a new tuple in the head is handled by the
            // head-derivation lowering, which [`lower_formula_clause`] dispatches
            // before reaching this per-atom converter.
            AtomPosition::Head => Err("n-ary atom in rule head (deriving a new tuple)"),
        },
    }
}

/// Convert an IR [`Term`] to an [`RcTerm`]. `is_object` gates a literal to the object slot.
/// The relational core's `RcTerm::Literal` carries the lexical form only (the lane is
/// datatype-agnostic, matching `RcTerm::from_value`).
fn formula_term_to_rc(term: &Term, is_object: bool) -> Result<RcTerm, &'static str> {
    match term {
        // RcTerm::Var carries the `?` sigil (the surface convention Term drops).
        Term::Var(name) => Ok(RcTerm::Var(format!("?{name}"))),
        Term::Iri(iri) => Ok(RcTerm::Iri(iri.clone())),
        Term::Literal { lexical, .. } => {
            if !is_object {
                return Err("literal in subject position (only an object may be a literal)");
            }
            Ok(RcTerm::Literal(lexical.clone()))
        }
        Term::SequenceMarker(_) => {
            Err("sequence marker (variadic) is not representable in the relational core")
        }
    }
}

// --------------------------------------------------------------------------- //
// Negation-normal form
// --------------------------------------------------------------------------- //

/// Rewrite a formula into negation-normal form: eliminate `→` and `↔`, then push every
/// negation inward (De Morgan + quantifier duality) until it sits only on atoms.
fn nnf(formula: &Formula) -> Formula {
    nnf_inner(formula, false)
}

/// `neg` = an odd number of negations encloses this node; carry it inward.
fn nnf_inner(f: &Formula, neg: bool) -> Formula {
    match f {
        Formula::Atom { .. } => {
            if neg {
                Formula::Not(Box::new(f.clone()))
            } else {
                f.clone()
            }
        }
        Formula::Not(inner) => nnf_inner(inner, !neg),
        Formula::And(fs) => {
            let parts: Vec<Formula> = fs.iter().map(|x| nnf_inner(x, neg)).collect();
            if neg {
                Formula::Or(parts) // ¬(φ ∧ ψ) ≡ ¬φ ∨ ¬ψ
            } else {
                Formula::And(parts)
            }
        }
        Formula::Or(fs) => {
            let parts: Vec<Formula> = fs.iter().map(|x| nnf_inner(x, neg)).collect();
            if neg {
                Formula::And(parts) // ¬(φ ∨ ψ) ≡ ¬φ ∧ ¬ψ
            } else {
                Formula::Or(parts)
            }
        }
        Formula::Implies(a, b) => {
            // φ → ψ ≡ ¬φ ∨ ψ
            let rewritten = Formula::Or(vec![Formula::Not(a.clone()), (**b).clone()]);
            nnf_inner(&rewritten, neg)
        }
        Formula::Iff(a, b) => {
            // φ ↔ ψ ≡ (φ → ψ) ∧ (ψ → φ)
            let rewritten = Formula::And(vec![
                Formula::Implies(a.clone(), b.clone()),
                Formula::Implies(b.clone(), a.clone()),
            ]);
            nnf_inner(&rewritten, neg)
        }
        Formula::Forall { vars, body } => {
            let inner = Box::new(nnf_inner(body, neg));
            if neg {
                Formula::Exists {
                    vars: vars.clone(),
                    body: inner, // ¬∀x.φ ≡ ∃x.¬φ
                }
            } else {
                Formula::Forall {
                    vars: vars.clone(),
                    body: inner,
                }
            }
        }
        Formula::Exists { vars, body } => {
            let inner = Box::new(nnf_inner(body, neg));
            if neg {
                Formula::Forall {
                    vars: vars.clone(),
                    body: inner, // ¬∃x.φ ≡ ∀x.¬φ
                }
            } else {
                Formula::Exists {
                    vars: vars.clone(),
                    body: inner,
                }
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// Existential Skolemization (constants only)
// --------------------------------------------------------------------------- //

/// Skolemize a leading existential prefix over a quantifier-free matrix, replacing each
/// `∃`-bound variable with a fresh Skolem-constant IRI derived deterministically from the
/// formula's alpha-normalized content key (so two alpha-equivalent formulas, however
/// constructed, get identical witnesses). A formula with no leading `∃`, or whose matrix
/// still holds a quantifier (`∃` under `∀` ⇒ a Skolem *function*; or an inner binder ⇒ a
/// capture hazard), is returned unchanged — the lowering then flags the surviving `∃`.
fn skolemize(formula: Formula) -> Formula {
    if !matches!(formula, Formula::Exists { .. }) {
        return formula;
    }
    let seed = sha256_12(&formula.content_key());
    let mut names: Vec<String> = Vec::new();
    let matrix = peel_exists(formula.clone(), &mut names);
    if has_quantifier(&matrix) {
        return formula;
    }
    let subs: Vec<(String, String)> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), format!("{LOGIC_NAMESPACE}skolem/{seed}-{i}")))
        .collect();
    subst_formula(matrix, &subs)
}

/// Peel a leading existential prefix, collecting its bound-variable names in order.
fn peel_exists(f: Formula, names: &mut Vec<String>) -> Formula {
    match f {
        Formula::Exists { vars, body } => {
            names.extend(vars);
            peel_exists(*body, names)
        }
        other => other,
    }
}

/// `true` if any quantifier appears anywhere in `f`.
fn has_quantifier(f: &Formula) -> bool {
    match f {
        Formula::Forall { .. } | Formula::Exists { .. } => true,
        Formula::Atom { .. } => false,
        Formula::Not(b) => has_quantifier(b),
        Formula::Implies(a, b) | Formula::Iff(a, b) => has_quantifier(a) || has_quantifier(b),
        Formula::And(fs) | Formula::Or(fs) => fs.iter().any(has_quantifier),
    }
}

/// Substitute each `(var → IRI)` binding into every atom term of a quantifier-free matrix.
fn subst_formula(f: Formula, subs: &[(String, String)]) -> Formula {
    match f {
        Formula::Atom { relation, args } => Formula::Atom {
            relation,
            args: args.into_iter().map(|t| subst_term(t, subs)).collect(),
        },
        Formula::Not(b) => Formula::Not(Box::new(subst_formula(*b, subs))),
        Formula::And(fs) => Formula::And(fs.into_iter().map(|x| subst_formula(x, subs)).collect()),
        Formula::Or(fs) => Formula::Or(fs.into_iter().map(|x| subst_formula(x, subs)).collect()),
        Formula::Implies(a, b) => Formula::Implies(
            Box::new(subst_formula(*a, subs)),
            Box::new(subst_formula(*b, subs)),
        ),
        Formula::Iff(a, b) => Formula::Iff(
            Box::new(subst_formula(*a, subs)),
            Box::new(subst_formula(*b, subs)),
        ),
        // Unreachable for a quantifier-free matrix, but total for safety.
        other => other,
    }
}

/// Replace a variable term with its Skolem IRI if bound by `subs`; else leave it. A shadowed
/// name appears more than once in `subs`; the matrix occurrence is bound by the *innermost*
/// enclosing quantifier, so the search runs in reverse — innermost binding wins.
fn subst_term(t: Term, subs: &[(String, String)]) -> Term {
    if let Term::Var(name) = &t {
        for (var, iri) in subs.iter().rev() {
            if var == name {
                return Term::Iri(iri.clone());
            }
        }
    }
    t
}

// --------------------------------------------------------------------------- //
// Projection: RelationalCoreProgram → deterministic N-Triples RDF graph
// --------------------------------------------------------------------------- //

/// The first 12 hex chars of SHA-256 of `s` — a content-stable node-IRI hash, matching
/// the `sha256(key)[:12]` convention the canonical-rdf12 projection uses for reifiers.
fn sha256_12(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(s.as_bytes());
    let mut out = String::with_capacity(12);
    for b in digest.iter().take(6) {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn fact_iri(atom: &RcAtom) -> String {
    format!(
        "{LOGIC_NAMESPACE}relational-core/fact/{}",
        sha256_12(&atom.key())
    )
}
fn rule_iri(rule: &RcRule) -> String {
    format!(
        "{LOGIC_NAMESPACE}relational-core/rule/{}",
        sha256_12(&rule.key())
    )
}
fn atom_node_iri(prefix: &str, key: &str) -> String {
    format!(
        "{LOGIC_NAMESPACE}relational-core/{prefix}/{}",
        sha256_12(key)
    )
}

/// N-Triples escape for a literal lexical value.
fn nt_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// Render an [`RcTerm`] as an N-Triples object (IRI, plain literal, or a typed
/// variable literal so a variable survives the round-trip distinctly from an IRI).
fn term_nt(term: &RcTerm) -> String {
    match term {
        RcTerm::Iri(iri) => format!("<{iri}>"),
        RcTerm::Blank(label) => format!("_:{label}"),
        RcTerm::Literal(lex) => format!("\"{}\"", nt_escape(lex)),
        RcTerm::Var(v) => format!(
            "\"{}\"^^<{LOGIC_NAMESPACE}Variable>",
            nt_escape(v.trim_start_matches('?'))
        ),
    }
}

fn triple(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> {object} .")
}
fn triple_iri(subject: &str, predicate: &str, object: &str) -> String {
    format!("<{subject}> <{predicate}> <{object}> .")
}
fn triple_str(subject: &str, predicate: &str, lexical: &str) -> String {
    format!("<{subject}> <{predicate}> \"{}\" .", nt_escape(lexical))
}
fn triple_bool(subject: &str, predicate: &str, value: bool) -> String {
    format!("<{subject}> <{predicate}> \"{value}\"^^<{XSD_BOOLEAN}> .")
}
fn triple_int(subject: &str, predicate: &str, value: usize) -> String {
    format!("<{subject}> <{predicate}> \"{value}\"^^<http://www.w3.org/2001/XMLSchema#integer> .")
}

/// Emit the atom-node triples (subject/predicate/object/negated) for one atom under
/// `atom_iri`, returning that IRI for linking.
fn emit_atom(lines: &mut Vec<String>, atom: &RcAtom, prefix: &str) -> String {
    let iri = atom_node_iri(prefix, &atom.key());
    lines.push(triple_iri(&iri, RDF_TYPE, &class_atom()));
    lines.push(triple(&iri, &p_rc_subject(), &term_nt(&atom.subject)));
    lines.push(triple_iri(&iri, &p_rc_predicate(), &atom.predicate));
    lines.push(triple(&iri, &p_rc_object(), &term_nt(&atom.object)));
    if matches!(atom.object, RcTerm::Literal(_)) {
        lines.push(triple_bool(&iri, &p_rc_object_literal(), true));
    }
    if atom.negated {
        lines.push(triple_bool(&iri, &p_rc_negated(), true));
    }
    iri
}

/// Project a [`RelationalCoreProgram`] into a deterministic, sorted, byte-stable
/// N-Triples graph — the content folded into the `graph/relational-core` named graph.
pub fn project_relational_core(program: &RelationalCoreProgram) -> String {
    let prog = program_iri();
    let mut lines: Vec<String> = Vec::new();

    lines.push(triple_iri(&prog, RDF_TYPE, &class_program()));
    lines.push(triple_iri(
        &prog,
        &p_has_preservation(),
        &program.preservation().iri(),
    ));
    if let Some(src) = &program.source_iri {
        // The source provenance is carried as a plain literal: it may be a relative
        // document path (`slices/grounding/logic/module.ttl`), not an absolute IRI.
        lines.push(triple_str(&prog, &p_source_iri(), src));
    }
    for residue in &program.residue {
        lines.push(triple_str(&prog, &p_lossy_drop(), &residue.reason));
    }

    for fact in &program.facts {
        let iri = fact_iri(fact);
        lines.push(triple_iri(&prog, &p_has_fact(), &iri));
        lines.push(triple_iri(&iri, RDF_TYPE, &class_fact()));
        lines.push(triple(&iri, &p_rc_subject(), &term_nt(&fact.subject)));
        lines.push(triple_iri(&iri, &p_rc_predicate(), &fact.predicate));
        lines.push(triple(&iri, &p_rc_object(), &term_nt(&fact.object)));
        if matches!(fact.object, RcTerm::Literal(_)) {
            lines.push(triple_bool(&iri, &p_rc_object_literal(), true));
        }
    }

    for rule in &program.rules {
        let r_iri = rule_iri(rule);
        lines.push(triple_iri(&prog, &p_has_rule(), &r_iri));
        lines.push(triple_iri(&r_iri, RDF_TYPE, &class_rule()));
        let head_iri = emit_atom(&mut lines, &rule.head, "head");
        lines.push(triple_iri(&r_iri, &p_rc_head(), &head_iri));
        // The reified n-ary head conjunction (empty for an ordinary rule): each extra head
        // atom is emitted as a positional, rcIndex-ordered node so the parse inverse
        // reconstructs `head_conjuncts` in order. Scoped by rule key + `hc{index}` so a
        // repeated conjunct at different positions gets a distinct node (no rcIndex collision).
        for (index, hc) in rule.head_conjuncts.iter().enumerate() {
            let hc_scope_key = format!("{}\u{1c}hc{index}\u{1c}{}", rule.key(), hc.key());
            let hc_iri = atom_node_iri("headconjunct", &hc_scope_key);
            lines.push(triple_iri(&hc_iri, RDF_TYPE, &class_atom()));
            lines.push(triple(&hc_iri, &p_rc_subject(), &term_nt(&hc.subject)));
            lines.push(triple_iri(&hc_iri, &p_rc_predicate(), &hc.predicate));
            lines.push(triple(&hc_iri, &p_rc_object(), &term_nt(&hc.object)));
            if matches!(hc.object, RcTerm::Literal(_)) {
                lines.push(triple_bool(&hc_iri, &p_rc_object_literal(), true));
            }
            lines.push(triple_iri(&r_iri, &p_rc_head_conjunct(), &hc_iri));
            lines.push(triple_int(&hc_iri, &p_rc_index(), index));
        }
        for (index, body_atom) in rule.body.iter().enumerate() {
            // Scope the body-atom node IRI by rule key AND atom index so that the same
            // atom appearing at different positions (or in different rules) gets a
            // distinct node — avoiding rcIndex collision on a shared content-keyed node.
            let b_scope_key = format!("{}\u{1c}{index}\u{1c}{}", rule.key(), body_atom.key());
            let b_iri = atom_node_iri("body", &b_scope_key);
            // Emit atom-node triples directly (emit_atom is content-keyed; here we need
            // the scope-keyed IRI instead).
            lines.push(triple_iri(&b_iri, RDF_TYPE, &class_atom()));
            lines.push(triple(
                &b_iri,
                &p_rc_subject(),
                &term_nt(&body_atom.subject),
            ));
            lines.push(triple_iri(&b_iri, &p_rc_predicate(), &body_atom.predicate));
            lines.push(triple(&b_iri, &p_rc_object(), &term_nt(&body_atom.object)));
            if matches!(body_atom.object, RcTerm::Literal(_)) {
                lines.push(triple_bool(&b_iri, &p_rc_object_literal(), true));
            }
            if body_atom.negated {
                lines.push(triple_bool(&b_iri, &p_rc_negated(), true));
            }
            lines.push(triple_iri(&r_iri, &p_rc_body(), &b_iri));
            // The body order is stored so the rule re-derives identically.
            lines.push(triple_int(&b_iri, &p_rc_index(), index));
        }
        for (a, b) in &rule.distinct_pairs {
            let d_iri = atom_node_iri("distinct", &format!("{}\u{1f}{a}\u{1f}{b}", rule.key()));
            lines.push(triple_iri(&r_iri, &p_rc_distinct(), &d_iri));
            lines.push(triple_str(&d_iri, &p_rc_distinct_left(), a));
            lines.push(triple_str(&d_iri, &p_rc_distinct_right(), b));
        }
    }

    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

// --------------------------------------------------------------------------- //
// Reverse: graph → RelationalCoreProgram (the cache-hit re-derivation)
// --------------------------------------------------------------------------- //

/// Parse a `graph/relational-core` projection (already loaded into an [`RdfDataset`])
/// back into the typed [`RelationalCoreProgram`] — the inverse of
/// [`project_relational_core`] the cache uses to re-derive the typed handle on a hit.
///
/// # Errors
///
/// Returns `Err` when the graph is structurally malformed (a missing required edge of a
/// fact/atom/rule node) — a corrupt projection HARD-fails, never re-derives a partial
/// program (no-optionality).
pub fn parse_relational_core(dataset: &RdfDataset) -> gmeow_errors::Result<RelationalCoreProgram> {
    // Index every (subject, predicate) once so reverse parsing is linear in graph
    // size, not repeated scans over the full quad list for every atom/rule edge.
    let mut by_sp: BTreeMap<(String, String), Vec<RdfTerm>> = BTreeMap::new();
    for quad in dataset.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        by_sp
            .entry((subject.clone(), quad.predicate.clone()))
            .or_default()
            .push(quad.object.clone());
    }

    let obj_of = |s: &str, p: &str| -> Option<RdfTerm> {
        by_sp
            .get(&(s.to_owned(), p.to_owned()))
            .and_then(|objects| objects.first().cloned())
    };
    let objs_of = |s: &str, p: &str| -> Vec<RdfTerm> {
        by_sp
            .get(&(s.to_owned(), p.to_owned()))
            .cloned()
            .unwrap_or_default()
    };
    let iri_obj = |t: &RdfTerm| -> Option<String> {
        match t {
            RdfTerm::Iri(i) => Some(i.clone()),
            _ => None,
        }
    };

    let prog = program_iri();

    // Source provenance (a literal: may be a relative document path) and residue.
    let source_iri = str_obj(obj_of(&prog, &p_source_iri()));
    let mut residue_set: BTreeSet<String> = BTreeSet::new();
    for t in objs_of(&prog, &p_lossy_drop()) {
        if let RdfTerm::Literal(lit) = t {
            residue_set.insert(lit.lexical_form);
        }
    }

    // Re-parse an atom node into an RcAtom.
    let parse_atom = |iri: &str| -> gmeow_errors::Result<RcAtom> {
        let subject = obj_of(iri, &p_rc_subject())
            .ok_or_else(|| {
                Diag::of_kind(crate::error::RelationalCore {
                    detail: format!("relational-core atom <{iri}> missing rcSubject"),
                })
            })
            .and_then(|t| term_from_rdf(&t))?;
        let predicate = obj_of(iri, &p_rc_predicate())
            .and_then(|t| iri_obj(&t))
            .ok_or_else(|| {
                Diag::of_kind(crate::error::RelationalCore {
                    detail: format!("relational-core atom <{iri}> missing rcPredicate"),
                })
            })?;
        let object = obj_of(iri, &p_rc_object())
            .ok_or_else(|| {
                Diag::of_kind(crate::error::RelationalCore {
                    detail: format!("relational-core atom <{iri}> missing rcObject"),
                })
            })
            .and_then(|t| term_from_rdf(&t))?;
        let negated = bool_obj(obj_of(iri, &p_rc_negated()));
        Ok(RcAtom {
            subject,
            predicate,
            object,
            negated,
        })
    };

    // Facts.
    let mut facts: Vec<RcAtom> = Vec::new();
    for fact_node in objs_of(&prog, &p_has_fact()) {
        if let Some(iri) = iri_obj(&fact_node) {
            facts.push(parse_atom(&iri)?);
        }
    }

    // Rules.
    let mut rules: Vec<RcRule> = Vec::new();
    for rule_node in objs_of(&prog, &p_has_rule()) {
        let Some(r_iri) = iri_obj(&rule_node) else {
            continue;
        };
        let head_iri = obj_of(&r_iri, &p_rc_head())
            .and_then(|t| iri_obj(&t))
            .ok_or_else(|| {
                Diag::of_kind(crate::error::RelationalCore {
                    detail: format!("relational-core rule <{r_iri}> missing rcHead"),
                })
            })?;
        let head = parse_atom(&head_iri)?;

        // Head conjuncts (the reified n-ary tail), ordered by their stored index. Empty for
        // an ordinary rule.
        let mut hc_indexed: Vec<(usize, RcAtom)> = Vec::new();
        for hc_node in objs_of(&r_iri, &p_rc_head_conjunct()) {
            let Some(hc_iri) = iri_obj(&hc_node) else {
                continue;
            };
            let atom = parse_atom(&hc_iri)?;
            let index = int_obj(obj_of(&hc_iri, &p_rc_index())).ok_or_else(|| {
                Diag::of_kind(crate::error::RelationalCore {
                    detail: format!(
                        "relational-core head-conjunct atom <{hc_iri}> missing rcIndex"
                    ),
                })
            })?;
            hc_indexed.push((index, atom));
        }
        hc_indexed.sort_by_key(|(i, _)| *i);
        // The reified n-ary head conjuncts carry positional `rcIndex` values that map to the
        // `naryArg{i}` slots the runtime reifier is content-addressed over. This path re-reads
        // an externally serializable relational-core projection, so a duplicate or gapped index
        // must HARD-FAIL — a silently mis-positioned conjunction would mint a wrong reifier
        // downstream (no-optionality).
        for (position, (i, _)) in hc_indexed.iter().enumerate() {
            if *i != position {
                return Err(Diag::of_kind(crate::error::RelationalCore {
                    detail: format!(
                        "relational-core rule <{r_iri}> has non-contiguous or duplicate head-conjunct \
                     rcIndex values {:?} (expected 0..{})",
                        hc_indexed.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
                        hc_indexed.len()
                    ),
                }));
            }
        }
        let head_conjuncts: Vec<RcAtom> = hc_indexed.into_iter().map(|(_, a)| a).collect();

        // Body atoms, ordered by their stored index.
        let mut indexed: Vec<(usize, RcAtom)> = Vec::new();
        for body_node in objs_of(&r_iri, &p_rc_body()) {
            let Some(b_iri) = iri_obj(&body_node) else {
                continue;
            };
            let atom = parse_atom(&b_iri)?;
            let index = int_obj(obj_of(&b_iri, &p_rc_index())).ok_or_else(|| {
                Diag::of_kind(crate::error::RelationalCore {
                    detail: format!("relational-core body atom <{b_iri}> missing rcIndex"),
                })
            })?;
            indexed.push((index, atom));
        }
        indexed.sort_by_key(|(i, _)| *i);
        let body: Vec<RcAtom> = indexed.into_iter().map(|(_, a)| a).collect();

        // Distinct guards.
        let mut distinct_pairs: Vec<(String, String)> = Vec::new();
        for d_node in objs_of(&r_iri, &p_rc_distinct()) {
            let Some(d_iri) = iri_obj(&d_node) else {
                continue;
            };
            let left = str_obj(obj_of(&d_iri, &p_rc_distinct_left())).ok_or_else(|| {
                Diag::of_kind(crate::error::RelationalCore {
                    detail: format!("relational-core distinct <{d_iri}> missing left"),
                })
            })?;
            let right = str_obj(obj_of(&d_iri, &p_rc_distinct_right())).ok_or_else(|| {
                Diag::of_kind(crate::error::RelationalCore {
                    detail: format!("relational-core distinct <{d_iri}> missing right"),
                })
            })?;
            distinct_pairs.push((left, right));
        }
        distinct_pairs.sort();

        rules.push(RcRule {
            head,
            head_conjuncts,
            body,
            distinct_pairs,
        });
    }

    Ok(finalize(facts, rules, residue_set, source_iri))
}

fn term_from_rdf(term: &RdfTerm) -> gmeow_errors::Result<RcTerm> {
    match term {
        RdfTerm::Iri(iri) => Ok(RcTerm::Iri(iri.clone())),
        RdfTerm::BlankNode(label) => Ok(RcTerm::Blank(label.trim_start_matches("_:").to_owned())),
        RdfTerm::Literal(lit) => {
            if lit.datatype.as_deref() == Some(&format!("{LOGIC_NAMESPACE}Variable")) {
                Ok(RcTerm::Var(format!("?{}", lit.lexical_form)))
            } else {
                Ok(RcTerm::Literal(lit.lexical_form.clone()))
            }
        }
        other => Err(Diag::of_kind(crate::error::RelationalCore {
            detail: format!(
                "relational-core term is not an IRI, blank node, or literal: {other:?}"
            ),
        })),
    }
}

fn literal_lexical(term: &RdfTerm) -> Option<&RdfLiteral> {
    match term {
        RdfTerm::Literal(lit) => Some(lit),
        _ => None,
    }
}

fn bool_obj(term: Option<RdfTerm>) -> bool {
    term.as_ref()
        .and_then(literal_lexical)
        .map(|l| l.lexical_form == "true")
        .unwrap_or(false)
}
fn int_obj(term: Option<RdfTerm>) -> Option<usize> {
    term.as_ref()
        .and_then(literal_lexical)
        .and_then(|l| l.lexical_form.parse().ok())
}
fn str_obj(term: Option<RdfTerm>) -> Option<String> {
    term.as_ref()
        .and_then(literal_lexical)
        .map(|l| l.lexical_form.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ContextualScope, LogicAxiom, LogicRule};

    fn ax(s: &str, p: &str, o: &str, o_lit: bool, negated: bool) -> LogicAxiom {
        LogicAxiom::new(s, p, o, o_lit, negated, ContextualScope::default()).expect("axiom")
    }

    /// A clean Horn program: two ground type axioms + one transitive-style rule. The
    /// whole rule set is in the binary-Horn fragment, so the lowering is `{exact}`.
    fn horn_program() -> LogicProgram {
        let animal = "https://blackcatinformatics.ca/gmeow/Animal";
        let cat = "https://blackcatinformatics.ca/gmeow/Cat";
        let kind = "https://blackcatinformatics.ca/logic/Kind";
        let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        // ?x sc ?z :- ?x sc ?y, ?y sc ?z .
        let rule = LogicRule::new(
            ax("?x", sc, "?z", false, false),
            vec![
                ax("?x", sc, "?y", false, false),
                ax("?y", sc, "?z", false, false),
            ],
            vec![],
            ContextualScope::default(),
        );
        LogicProgram::new(
            vec![
                ax(animal, RDF_TYPE, kind, false, false),
                ax(cat, sc, animal, false, false),
            ],
            vec![rule],
            vec![],
            Some("https://blackcatinformatics.ca/logic/test".to_owned()),
        )
    }

    #[test]
    fn horn_floor_lowers_exactly() {
        let lowered = lower_program(&horn_program());
        assert!(
            lowered.residue.is_empty(),
            "main's Horn rules lower with no residue: {:?}",
            lowered.residue
        );
        assert_eq!(
            lowered.preservation(),
            PreservationKind::Exact,
            "a fully-supported lowering is {{exact}}"
        );
        assert_eq!(lowered.facts.len(), 2, "both ground axioms lower to facts");
        assert_eq!(lowered.rules.len(), 1, "the Horn rule lowers");
        // The rule is a binary clause with a 2-atom body.
        assert_eq!(lowered.rules[0].body.len(), 2);
    }

    #[test]
    fn unsupported_construct_becomes_flagged_residue_and_sound_under() {
        // The full-FOL lowering's seam: a non-Horn construct (e.g. a disjunctive head)
        // arrives as flagged residue. Assert it is CARRIED (not dropped) and the claim
        // drops to {sound-under}.
        let lowered = lower_with_residue(
            &horn_program(),
            ["disjunctive head: clause is not Horn (>1 positive literal)"],
        );
        assert_eq!(
            lowered.preservation(),
            PreservationKind::SoundUnder,
            "a carried residue makes the claim sound-under, not a false exact"
        );
        assert!(
            lowered
                .residue
                .iter()
                .any(|r| r.reason.contains("disjunctive head")),
            "the unsupported construct is carried and flagged, never dropped"
        );
        // The Horn fragment still lowered alongside the residue.
        assert_eq!(lowered.rules.len(), 1, "the legal fragment still lowers");
    }

    #[test]
    fn projection_round_trips_through_the_graph() {
        // Dual carriage: the typed program → graph → typed program is identity (the
        // cache-hit re-derivation the handle relies on).
        let lowered = lower_program(&horn_program());
        let nt = project_relational_core(&lowered);
        let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("parse projection");
        let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
        assert_eq!(
            re_derived.content_key(),
            lowered.content_key(),
            "the graph round-trips to a content-key-equal program"
        );
        assert_eq!(re_derived, lowered, "the round-trip is value-identical");
    }

    #[test]
    fn residue_round_trips_through_the_graph() {
        // A {sound-under} program (carrying residue) also round-trips: the residue
        // survives the projection and the preservation claim is re-derived correctly.
        let lowered = lower_with_residue(
            &horn_program(),
            [
                "disjunctive head: clause is not Horn (>1 positive literal)",
                "sequence marker (variadic) is not representable in the relational core",
            ],
        );
        let nt = project_relational_core(&lowered);
        let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("parse projection");
        let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
        assert_eq!(re_derived.preservation(), PreservationKind::SoundUnder);
        assert_eq!(re_derived.residue.len(), 2, "both residue rows survive");
        assert_eq!(re_derived, lowered);
    }

    #[test]
    fn projection_is_byte_deterministic() {
        let lowered = lower_program(&horn_program());
        let a = project_relational_core(&lowered);
        let b = project_relational_core(&lower_program(&horn_program()));
        assert_eq!(
            a, b,
            "the projection is a byte-stable function of the program"
        );
        // Sorted N-Triples: every non-empty line ends with " ." and the whole is sorted.
        let lines: Vec<&str> = a.lines().collect();
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        assert_eq!(lines, sorted, "projection lines are sorted (deterministic)");
    }

    /// Finding 1 regression: a literal whose lexical form begins with '?' must be
    /// classified as a Literal, not as a Var, regardless of variable-syntax heuristic.
    #[test]
    fn declared_literal_starting_with_question_mark_is_not_a_var() {
        let term = RcTerm::from_value("?sparql-like-literal", true);
        assert!(
            matches!(term, RcTerm::Literal(_)),
            "is_literal=true MUST win over variable-syntax heuristic; got {term:?}"
        );
        // Countercheck: without is_literal=true, '?' still produces Var.
        let var_term = RcTerm::from_value("?x", false);
        assert!(
            matches!(var_term, RcTerm::Var(_)),
            "without is_literal flag, '?' prefix still produces Var"
        );
    }

    /// Finding 2 regression: a rule with a repeated body atom (same content at two
    /// positions) must round-trip without rcIndex collision on a shared node IRI.
    #[test]
    fn repeated_body_atom_round_trips_with_distinct_indices() {
        let sc = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
        // ?x sc ?z :- ?x sc ?y, ?x sc ?y .   (same atom twice — pathological but legal)
        let rule = LogicRule::new(
            ax("?x", sc, "?z", false, false),
            vec![
                ax("?x", sc, "?y", false, false),
                ax("?x", sc, "?y", false, false), // duplicate
            ],
            vec![],
            ContextualScope::default(),
        );
        let program = LogicProgram::new(vec![], vec![rule], vec![], None);
        let lowered = lower_program(&program);
        // Project then re-derive: must reconstruct 2 body atoms, not 1.
        let nt = project_relational_core(&lowered);
        let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("parse projection");
        let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
        assert_eq!(
            re_derived.rules[0].body.len(),
            2,
            "both body occurrences must survive the round-trip (no rcIndex collision)"
        );
        assert_eq!(re_derived, lowered, "full round-trip value equality");
    }

    #[test]
    fn malformed_graph_hard_fails() {
        // A fact node missing its rcPredicate edge is a corrupt projection — the
        // reverse parser HARD-fails rather than re-deriving a partial program.
        let prog = program_iri();
        let fact = format!("{LOGIC_NAMESPACE}relational-core/fact/deadbeef");
        let nt = format!(
            "<{prog}> <{}> <{}> .\n<{prog}> <{}> <{fact}> .\n<{fact}> <{}> <{}> .\n",
            RDF_TYPE,
            class_program(),
            p_has_fact(),
            RDF_TYPE,
            class_fact(),
        );
        let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("parse malformed");
        let err = parse_relational_core(ds.as_ref()).expect_err("malformed graph must hard-fail");
        assert!(err.message().contains("missing rcSubject"), "got: {err}");
    }

    // ── Full-FOL formula lowering: the seam wiring the clausifier into the lane ──

    fn firi(s: &str) -> String {
        format!("https://blackcatinformatics.ca/gmeow/{s}")
    }
    fn fatom(pred: &str, args: Vec<Term>) -> Formula {
        Formula::atom(Term::Iri(firi(pred)), args).expect("first-order atom")
    }
    fn fvar(n: &str) -> Term {
        Term::Var(n.to_owned())
    }
    fn transitivity_formula() -> Formula {
        // ∀x y z. (sc(x,y) ∧ sc(y,z)) → sc(x,z)
        let body = Formula::And(vec![
            fatom("scA", vec![fvar("x"), fvar("y")]),
            fatom("scA", vec![fvar("y"), fvar("z")]),
        ]);
        let head = fatom("scA", vec![fvar("x"), fvar("z")]);
        Formula::Forall {
            vars: vec!["x".into(), "y".into(), "z".into()],
            body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
        }
    }

    /// A Horn-expressible formula (a universally-closed implication with a conjunctive body)
    /// lowers to exactly one Horn RcRule, leaves no residue, and keeps the lane {exact}.
    #[test]
    fn horn_expressible_formula_lowers_to_rcrule_exact() {
        let program = LogicProgram::new(vec![], vec![], vec![], None)
            .with_formulas(vec![transitivity_formula()]);
        let lowered = lower_program_with_formulas(&program);
        assert!(
            lowered.residue.is_empty(),
            "a Horn-expressible formula leaves no residue: {:?}",
            lowered.residue
        );
        assert_eq!(lowered.preservation(), PreservationKind::Exact);
        assert_eq!(
            lowered.rules.len(),
            1,
            "the implication lowers to one Horn rule"
        );
        assert_eq!(lowered.rules[0].body.len(), 2, "both body atoms survive");
    }

    /// A disjunctive head is beyond Horn: carried + flagged, preservation drops to sound-under,
    /// and the residue note names BOTH the reason and the Disjunctive FormulaShape tag.
    #[test]
    fn disjunctive_head_is_carried_and_named() {
        let f = Formula::Forall {
            vars: vec!["x".into()],
            body: Box::new(Formula::Or(vec![
                fatom("pA", vec![fvar("x"), Term::Iri(firi("a"))]),
                fatom("qA", vec![fvar("x"), Term::Iri(firi("b"))]),
            ])),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
        assert!(lowered.rules.is_empty(), "nothing Horn-expressible here");
        assert_eq!(lowered.residue.len(), 1);
        let r = &lowered.residue[0].reason;
        assert!(r.contains("disjunctive head"), "names the reason: {r}");
        assert!(r.contains("Disjunctive"), "names the FormulaShape tag: {r}");
    }

    /// An existential under a universal needs a Skolem *function* the relational term algebra
    /// cannot hold; it is carried (not mis-lowered to a constant) and tagged Quantified.
    #[test]
    fn exists_under_forall_is_carried_and_named() {
        let f = Formula::Forall {
            vars: vec!["x".into()],
            body: Box::new(Formula::Exists {
                vars: vec!["y".into()],
                body: Box::new(fatom("rA", vec![fvar("x"), fvar("y")])),
            }),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
        assert!(lowered.rules.is_empty());
        assert_eq!(lowered.residue.len(), 1);
        assert!(
            lowered.residue[0].reason.contains("Quantified"),
            "names the Quantified tag: {}",
            lowered.residue[0].reason
        );
    }

    /// A fixed-arity ternary atom in a rule BODY is now evaluable: it reifies into a
    /// conjunction of binary atoms over a fresh reifier variable, so the rule is
    /// carried (not residue) and preservation is Exact.
    #[test]
    fn nary_body_atom_lowers_to_reified_rules() {
        // ∀x y z. relA(x,y,z) → pA(x,z)
        let body = fatom("relA", vec![fvar("x"), fvar("y"), fvar("z")]);
        let head = fatom("pA", vec![fvar("x"), fvar("z")]);
        let f = Formula::Forall {
            vars: vec!["x".into(), "y".into(), "z".into()],
            body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        assert!(
            lowered.residue.is_empty(),
            "a fixed-arity n-ary body atom is evaluable, not residue: {:?}",
            lowered.residue
        );
        assert_eq!(lowered.preservation(), PreservationKind::Exact);
        assert_eq!(lowered.rules.len(), 1, "the implication lowers to one rule");
        // The ternary body atom reifies into instanceOf + naryArg0 + naryArg1 + naryArg2.
        assert_eq!(
            lowered.rules[0].body.len(),
            4,
            "ternary body atom → 4 reified binary atoms"
        );
        let preds: Vec<&str> = lowered.rules[0]
            .body
            .iter()
            .map(|a| a.predicate.as_str())
            .collect();
        assert!(preds.iter().any(|p| p.ends_with("instanceOf")), "{preds:?}");
        for i in 0..3 {
            assert!(
                preds.iter().any(|p| p.ends_with(&format!("naryArg{i}"))),
                "missing naryArg{i}: {preds:?}"
            );
        }
        // The reifier variable is shared across the whole reified conjunction.
        let reifier_subjects: BTreeSet<&RcTerm> =
            lowered.rules[0].body.iter().map(|a| &a.subject).collect();
        assert_eq!(
            reifier_subjects.len(),
            1,
            "all reified atoms share one reifier variable: {reifier_subjects:?}"
        );
    }

    /// Two DISTINCT same-relation ternary body atoms must NOT be unified: their reifier
    /// variables are keyed on the raw syntactic atom (authored variable names intact),
    /// so `rel(?a,?b,?c)` and `rel(?d,?e,?f)` get different reifiers. Keying on the
    /// alpha-normalized content_key would unsoundly collapse them into one tuple.
    #[test]
    fn distinct_nary_body_atoms_do_not_collide() {
        // ∀ a b c d e f. relA(a,b,c) ∧ relA(d,e,f) → pA(a,d)
        let body = Formula::And(vec![
            fatom("relA", vec![fvar("a"), fvar("b"), fvar("c")]),
            fatom("relA", vec![fvar("d"), fvar("e"), fvar("f")]),
        ]);
        let head = fatom("pA", vec![fvar("a"), fvar("d")]);
        let f = Formula::Forall {
            vars: vec![
                "a".into(),
                "b".into(),
                "c".into(),
                "d".into(),
                "e".into(),
                "f".into(),
            ],
            body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        assert!(lowered.residue.is_empty(), "{:?}", lowered.residue);
        assert_eq!(lowered.rules.len(), 1);
        // Two distinct ternary atoms → two distinct reifier variables (2 × 4 = 8 atoms).
        assert_eq!(
            lowered.rules[0].body.len(),
            8,
            "two distinct ternary atoms reify without collision"
        );
        let reifier_vars: BTreeSet<&RcTerm> = lowered.rules[0]
            .body
            .iter()
            .filter(|a| a.predicate.ends_with("instanceOf"))
            .map(|a| &a.subject)
            .collect();
        assert_eq!(
            reifier_vars.len(),
            2,
            "distinct atoms must NOT share a reifier variable: {reifier_vars:?}"
        );
    }

    /// A genuine sequence-marker atom (`rA(x, ...rest)`) is truly variadic — it stays
    /// carried as residue and tagged Variadic; only *fixed*-arity atoms reify.
    #[test]
    fn sequence_marker_atom_is_carried_and_named() {
        let f = fatom(
            "rA",
            vec![fvar("x"), Term::SequenceMarker("rest".to_owned())],
        );
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
        assert!(lowered.rules.is_empty());
        assert_eq!(lowered.residue.len(), 1);
        let r = &lowered.residue[0].reason;
        assert!(r.contains("sequence marker"), "names the reason: {r}");
        assert!(r.contains("Variadic"), "names the Variadic tag: {r}");
    }

    /// A fixed-arity ternary atom in the rule HEAD now DERIVES a reified tuple: it lowers to
    /// a conjunctive-head existential rule (`instanceOf(R, Rel)` head + `naryArg{i}(R, aᵢ)`
    /// conjuncts over a fresh existential reifier `R`), carried (not residue), preservation
    /// Exact. `matMul` is ternary in the BODY (reified) and `mul` ternary in the HEAD.
    #[test]
    fn nary_head_atom_derives_a_reified_tuple() {
        // ∀A B AB dA dB dAB. matMul(A,B,AB) ∧ det(A,dA) ∧ det(B,dB) ∧ det(AB,dAB) → mul(dA,dB,dAB)
        let body = Formula::And(vec![
            fatom("matMul", vec![fvar("A"), fvar("B"), fvar("AB")]),
            fatom("det", vec![fvar("A"), fvar("dA")]),
            fatom("det", vec![fvar("B"), fvar("dB")]),
            fatom("det", vec![fvar("AB"), fvar("dAB")]),
        ]);
        let head = fatom("mul", vec![fvar("dA"), fvar("dB"), fvar("dAB")]);
        let f = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        assert!(
            lowered.residue.is_empty(),
            "a range-restricted n-ary head is evaluable, not residue: {:?}",
            lowered.residue
        );
        assert_eq!(lowered.preservation(), PreservationKind::Exact);
        assert_eq!(lowered.rules.len(), 1, "the implication lowers to one rule");
        let rule = &lowered.rules[0];
        // Head is instanceOf(R, mul); the tail is naryArg0..2(R, dA/dB/dAB).
        assert!(
            rule.head.predicate.ends_with("instanceOf"),
            "head types the reifier: {}",
            rule.head.predicate
        );
        assert!(
            matches!(&rule.head.object, RcTerm::Iri(i) if i.ends_with("mul")),
            "head instanceOf object is the relation IRI: {:?}",
            rule.head.object
        );
        assert_eq!(
            rule.head_conjuncts.len(),
            3,
            "ternary head → 3 naryArg conjuncts: {:?}",
            rule.head_conjuncts
        );
        for i in 0..3 {
            assert!(
                rule.head_conjuncts[i]
                    .predicate
                    .ends_with(&format!("naryArg{i}")),
                "conjunct {i} predicate: {}",
                rule.head_conjuncts[i].predicate
            );
        }
        // The reifier `R` is one fresh existential var shared across head + every conjunct,
        // and it is NOT bound by the body (it is the invented tuple node).
        let reifier = &rule.head.subject;
        assert!(matches!(reifier, RcTerm::Var(v) if v.starts_with("?naryH")));
        assert!(
            rule.head_conjuncts.iter().all(|c| &c.subject == reifier),
            "all conjuncts share the reifier subject"
        );
        let body_bound = body_bound_vars(&rule.body);
        if let RcTerm::Var(v) = reifier {
            assert!(
                !body_bound.contains(v),
                "the existential reifier is not body-bound"
            );
        }
    }

    /// A head variable the body does not bind is a non-range-restricted existential (unsafe):
    /// the clause is carried as residue tagged with the "not bound by the body" reason, never
    /// lowered to an unsafe rule.
    #[test]
    fn nary_head_with_unbound_arg_is_residue() {
        // ∀A B AB dA dB. matMul(A,B,AB) ∧ det(A,dA) ∧ det(B,dB) → mul(dA, dB, dAB)
        // `dAB` appears only in the head — the body binds nothing to it.
        let body = Formula::And(vec![
            fatom("matMul", vec![fvar("A"), fvar("B"), fvar("AB")]),
            fatom("det", vec![fvar("A"), fvar("dA")]),
            fatom("det", vec![fvar("B"), fvar("dB")]),
        ]);
        let head = fatom("mul", vec![fvar("dA"), fvar("dB"), fvar("dAB")]);
        let f = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        assert_eq!(lowered.preservation(), PreservationKind::SoundUnder);
        assert!(lowered.rules.is_empty(), "an unsafe head does not lower");
        assert_eq!(lowered.residue.len(), 1);
        assert!(
            lowered.residue[0].reason.contains("not bound by the body"),
            "the residue names the range-restriction failure: {}",
            lowered.residue[0].reason
        );
    }

    /// An n-ary head-derivation rule (carrying `head_conjuncts`) survives the lane's RDF
    /// projection round-trip value-identically — the conjuncts re-derive in order.
    #[test]
    fn nary_head_rule_round_trips_through_the_graph() {
        let body = Formula::And(vec![
            fatom("matMul", vec![fvar("A"), fvar("B"), fvar("AB")]),
            fatom("det", vec![fvar("A"), fvar("dA")]),
            fatom("det", vec![fvar("B"), fvar("dB")]),
            fatom("det", vec![fvar("AB"), fvar("dAB")]),
        ]);
        let head = fatom("mul", vec![fvar("dA"), fvar("dB"), fvar("dAB")]);
        let f = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        assert_eq!(lowered.rules.len(), 1);
        assert_eq!(lowered.rules[0].head_conjuncts.len(), 3);
        let nt = project_relational_core(&lowered);
        let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("parse projection");
        let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
        assert_eq!(
            re_derived, lowered,
            "an n-ary head-derivation rule round-trips value-identically"
        );
    }

    /// A relational-core projection whose reified head-conjunct positional `rcIndex` values are
    /// corrupted into a duplicate (non-contiguous) set must HARD-FAIL on re-derivation: this
    /// path re-reads an externally serializable projection, and a silently mis-positioned
    /// conjunction would mint a wrong content-addressed reifier at chase time (no-optionality).
    #[test]
    fn nary_head_rule_with_duplicate_rc_index_is_rejected() {
        let body = Formula::And(vec![
            fatom("matMul", vec![fvar("A"), fvar("B"), fvar("AB")]),
            fatom("det", vec![fvar("A"), fvar("dA")]),
            fatom("det", vec![fvar("B"), fvar("dB")]),
            fatom("det", vec![fvar("AB"), fvar("dAB")]),
        ]);
        let head = fatom("mul", vec![fvar("dA"), fvar("dB"), fvar("dAB")]);
        let f = Formula::Forall {
            vars: vec![
                "A".into(),
                "B".into(),
                "AB".into(),
                "dA".into(),
                "dB".into(),
                "dAB".into(),
            ],
            body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
        };
        let program = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
        let lowered = lower_program_with_formulas(&program);
        let nt = project_relational_core(&lowered);
        // Rewrite the head conjunct at positional index 2 to a DUPLICATE of index 0. Only the
        // one `/headconjunct/` node carries rcIndex "2" (body atoms live under `/body/`).
        let corrupted: String = nt
            .lines()
            .map(|line| {
                if line.contains("/headconjunct/")
                    && line.contains("rcIndex")
                    && line.contains("\"2\"^^")
                {
                    line.replace("\"2\"^^", "\"0\"^^")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_ne!(
            corrupted, nt,
            "the corruption must have hit a head-conjunct rcIndex line"
        );
        let ds = purrdf::parse_dataset(corrupted.as_bytes(), "application/n-triples", None)
            .expect("parse corrupted projection");
        let err =
            parse_relational_core(ds.as_ref()).expect_err("duplicate rcIndex must be rejected");
        assert!(
            err.message()
                .contains("non-contiguous or duplicate head-conjunct"),
            "the error names the malformed head-conjunct indices: {err}"
        );
    }

    /// The Horn-expressible formula fragment survives the lane's RDF projection round-trip,
    /// so the carrier and the typed handle stay value-identical (dual carriage holds).
    #[test]
    fn formula_derived_rules_round_trip_through_the_graph() {
        let program = LogicProgram::new(vec![], vec![], vec![], None)
            .with_formulas(vec![transitivity_formula()]);
        let lowered = lower_program_with_formulas(&program);
        let nt = project_relational_core(&lowered);
        let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("parse projection");
        let re_derived = parse_relational_core(ds.as_ref()).expect("re-derive");
        assert_eq!(
            re_derived, lowered,
            "formula-derived rules round-trip value-identically"
        );
    }
}
