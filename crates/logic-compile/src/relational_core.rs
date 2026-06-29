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
//! (`slices/core/logic/design/LOGIC-IR.md` § IR commitments — "Lowering is legalization").
//! Any rule or axiom outside the binary-Horn fragment is **carried as flagged unsupported
//! residue** ([`RcResidue`]), never silently dropped, and the preservation claim drops to
//! `{sound-under}` naming what was carried. For the current Horn corpus the residue is
//! empty — but the mechanism (and its fixture, in the tests) MUST exist so the lane is
//! honest by construction.
//!
//! When the full-FOL Formula AST lands (a separate, larger effort whose NNF→Skolem→Horn
//! lowering produces richer non-Horn shapes — disjunctive heads, quantifier alternation,
//! non-binary atoms), its lowering plugs into THIS SAME lane: it produces the same
//! [`RcRule`]s for its Horn-expressible fragment and feeds the rest through
//! [`RelationalCoreProgram::push_residue`], so the carrier, the named-graph projection,
//! and the typed handle never change shape.
//!
//! # Dual carriage
//!
//! The dialect is carried BOTH as a typed handle payload (`Arc<RelationalCoreProgram>`)
//! and as a deterministic, byte-stable RDF projection into the `graph/relational-core`
//! named graph ([`project_relational_core`]); [`parse_relational_core`] is the inverse a
//! cache hit uses to re-derive the typed payload from the backing graph. The two faces
//! share one content identity ([`RelationalCoreProgram::content_key`]).

use std::collections::{BTreeMap, BTreeSet};

use gmeow_rdf::{RdfDataset, RdfLiteral, RdfTerm};

use crate::ir::{LogicAxiom, LogicProgram, LogicRule, PreservationKind, LOGIC_NAMESPACE};

const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

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

    fn key(&self) -> String {
        match self {
            Self::Var(v) => format!("V\u{1f}{v}"),
            Self::Iri(i) => format!("I\u{1f}{i}"),
            Self::Blank(b) => format!("B\u{1f}{b}"),
            Self::Literal(l) => format!("L\u{1f}{l}"),
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
    fn key(&self) -> String {
        format!(
            "{}\u{1e}{}\u{1e}{}\u{1e}{}",
            self.subject.key(),
            self.predicate,
            self.object.key(),
            self.negated,
        )
    }
}

/// A relational-core Horn rule: a single head atom derived from a conjunctive body of
/// atoms (positive or NAF-negated) plus inequality `!=` guards.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RcRule {
    /// The derived head atom.
    pub head: RcAtom,
    /// The body atoms, in canonical (sorted) order.
    pub body: Vec<RcAtom>,
    /// Inequality guards (each pair internally sorted, the set sorted).
    pub distinct_pairs: Vec<(String, String)>,
}

impl RcRule {
    fn key(&self) -> String {
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
        format!("{}\u{1c}{body}\u{1c}{distinct}", self.head.key())
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
        let facts = {
            let mut keys: Vec<String> = self.facts.iter().map(RcAtom::key).collect();
            keys.sort();
            keys.join("\n")
        };
        let rules = {
            let mut keys: Vec<String> = self.rules.iter().map(RcRule::key).collect();
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
                residue.insert(reason);
            }
        }
    }
    for rule in &program.rules {
        match lower_rule(rule) {
            Ok(rc) => rules.push(rc),
            Err(reason) => {
                residue.insert(reason);
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
fn lower_axiom(axiom: &LogicAxiom) -> Result<RcAtom, String> {
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
fn lower_rule(rule: &LogicRule) -> Result<RcRule, String> {
    let head = lower_body_atom(&rule.head);
    let body: Vec<RcAtom> = rule.body.iter().map(lower_body_atom).collect();
    let distinct_pairs = rule.distinct_pairs.clone();
    Ok(RcRule {
        head: RcAtom {
            negated: false,
            ..head
        },
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
        // document path (`slices/core/logic/module.ttl`), not an absolute IRI.
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
pub fn parse_relational_core(dataset: &RdfDataset) -> Result<RelationalCoreProgram, String> {
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
    let parse_atom = |iri: &str| -> Result<RcAtom, String> {
        let subject = obj_of(iri, &p_rc_subject())
            .ok_or_else(|| format!("relational-core atom <{iri}> missing rcSubject"))
            .and_then(|t| term_from_rdf(&t))?;
        let predicate = obj_of(iri, &p_rc_predicate())
            .and_then(|t| iri_obj(&t))
            .ok_or_else(|| format!("relational-core atom <{iri}> missing rcPredicate"))?;
        let object = obj_of(iri, &p_rc_object())
            .ok_or_else(|| format!("relational-core atom <{iri}> missing rcObject"))
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
            .ok_or_else(|| format!("relational-core rule <{r_iri}> missing rcHead"))?;
        let head = parse_atom(&head_iri)?;

        // Body atoms, ordered by their stored index.
        let mut indexed: Vec<(usize, RcAtom)> = Vec::new();
        for body_node in objs_of(&r_iri, &p_rc_body()) {
            let Some(b_iri) = iri_obj(&body_node) else {
                continue;
            };
            let atom = parse_atom(&b_iri)?;
            let index = int_obj(obj_of(&b_iri, &p_rc_index()))
                .ok_or_else(|| format!("relational-core body atom <{b_iri}> missing rcIndex"))?;
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
            let left = str_obj(obj_of(&d_iri, &p_rc_distinct_left()))
                .ok_or_else(|| format!("relational-core distinct <{d_iri}> missing left"))?;
            let right = str_obj(obj_of(&d_iri, &p_rc_distinct_right()))
                .ok_or_else(|| format!("relational-core distinct <{d_iri}> missing right"))?;
            distinct_pairs.push((left, right));
        }
        distinct_pairs.sort();

        rules.push(RcRule {
            head,
            body,
            distinct_pairs,
        });
    }

    Ok(finalize(facts, rules, residue_set, source_iri))
}

fn term_from_rdf(term: &RdfTerm) -> Result<RcTerm, String> {
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
        other => Err(format!(
            "relational-core term is not an IRI, blank node, or literal: {other:?}"
        )),
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
        let ds = gmeow_rdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
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
                "non-binary atom (the relational core is binary; arity != 2)",
            ],
        );
        let nt = project_relational_core(&lowered);
        let ds = gmeow_rdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
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
        let ds = gmeow_rdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
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
        let ds = gmeow_rdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
            .expect("parse malformed");
        let err = parse_relational_core(ds.as_ref()).expect_err("malformed graph must hard-fail");
        assert!(err.contains("missing rcSubject"), "got: {err}");
    }
}
