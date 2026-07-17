// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Three-consumer lowering into the shared hash-consed [`TermDag`].
//!
//! # One arena, three surfaces
//!
//! [`TermDag`](crate::physical::term_dag::TermDag) is the single structured-term arena.
//! Three surfaces lower INTO it, and because the arena is content-addressed and
//! locally-nameless, alpha-equivalent inputs authored in ANY of the three surfaces intern
//! to the SAME [`NodeId`] and the SAME content key:
//!
//! - **`logic:`** — the Rust [`Formula`]/[`Term`] IR of
//!   [`gmeow_logic_compile::ir`](gmeow_logic_compile::ir). Lowered directly:
//!   [`lower_logic_formula`] / [`lower_logic_term`].
//! - **`math:`** — the RDF-authored application/binding expression vocabulary
//!   (`math:ApplicationExpression`, `math:ArgumentSlot`, `math:slotIndex`,
//!   `math:BindingExpression`, `math:VariableDeclaration`, `math:VariableOccurrence`, …)
//!   materialized in `slices/grounding/math/module.ttl`. There is no typed `math:` AST in
//!   Rust — the expression tree *is* an RDF graph — so the lowering reads it out of a
//!   [`MathGraph`] (a parsed Turtle/`purrdf` dataset): [`lower_math_expression`].
//! - **`lang:`** — a `lang:` [`Form`](gmeow_lang_form::Form) paired with its ONE-WAY
//!   `lang:`→`logic:` denotation (`lang:denotationKind` / `lang:denotationTarget`, per
//!   `slices/grounding/lang/design/LANG-MEANING.md`). A form's formal meaning bottoms out
//!   in a `logic:` object, so lowering follows the denotation:
//!   [`lower_lang_form`].
//!
//! # The shared canonical vocabulary
//!
//! For alpha-equivalent inputs to collapse ACROSS surfaces, the operator/sort identities
//! must be shared: `logic:` is the canonical reasoning language and `math:`/`lang:` ground
//! INTO it, so the quantifier/connective operator IRIs and the individual-sort IRI in
//! [`canon`] are the one identity every consumer emits. A `math:BindingExpression` whose
//! `math:operator` is [`canon::FORALL`] and a `logic:` [`Formula::Forall`] therefore mint
//! the SAME binder node.
//!
//! # Locally-nameless discipline (shared)
//!
//! Every consumer resolves a bound occurrence to a de-Bruijn [`Bound`](crate::physical)
//! ref against a binder-frame environment (innermost frame last), so alpha-renaming is
//! already quotiented away by the arena. Minting a `Bound` de-Bruijn distance / slot is
//! overflow-checked ([`intern_bound_checked`]): a distance past `u32::MAX` or a slot past
//! `u16::MAX` is a HARD FAIL, never a silent wrap (a wrap is a variable-capture bug).

use std::collections::BTreeMap;

use gmeow_errors::{Diag, Result};
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::{TermRef, TermValue};

use crate::physical::id::NodeId;
use crate::physical::term_dag::TermDag;

/// The shared canonical operator / sort IRIs every consumer's lowering emits, so that
/// alpha-equivalent inputs authored in `logic:`, `math:`, or `lang:` intern to one node.
///
/// `logic:` is the canonical reasoning layer; `math:` and `lang:` ground into it, so these
/// IRIs are the single operator identity all three share (a `math:BindingExpression` whose
/// `math:operator` is [`FORALL`](canon::FORALL) denotes the same binder a `logic:`
/// `∀` does).
pub(crate) mod canon {
    /// The universal-quantifier binder operator.
    pub(crate) const FORALL: &str = "https://blackcatinformatics.ca/logic/dag/op/forall";
    /// The existential-quantifier binder operator.
    pub(crate) const EXISTS: &str = "https://blackcatinformatics.ca/logic/dag/op/exists";
    /// The strong-negation connective operator.
    pub(crate) const NOT: &str = "https://blackcatinformatics.ca/logic/dag/op/not";
    /// The conjunction connective operator (commutative + associative).
    pub(crate) const AND: &str = "https://blackcatinformatics.ca/logic/dag/op/and";
    /// The disjunction connective operator (commutative + associative).
    pub(crate) const OR: &str = "https://blackcatinformatics.ca/logic/dag/op/or";
    /// The material-implication connective operator (ordered).
    pub(crate) const IMPLIES: &str = "https://blackcatinformatics.ca/logic/dag/op/implies";
    /// The biconditional connective operator (commutative).
    pub(crate) const IFF: &str = "https://blackcatinformatics.ca/logic/dag/op/iff";
    /// The untyped individual sort — the default sort of a bound variable that carries no
    /// declared type/domain.
    pub(crate) const SORT_INDIVIDUAL: &str =
        "https://blackcatinformatics.ca/logic/dag/sort/individual";
    /// The proof constructor operator for a rule-application proof node
    /// (`by_rule(goal, rule, subproofs…)`, [`crate::physical::proof`]). A proof is itself a
    /// first-class DAG term, so its constructor is a shared `dag/op/` operator IRI — distinct
    /// from the `logic:assert` rule sentinel ([`crate::provenance::ASSERT_RULE_IRI`]), which
    /// tags an asserted fact's *derivation*, not a proof node's *shape*.
    pub(crate) const BY_RULE: &str = "https://blackcatinformatics.ca/logic/dag/op/byRule";
    /// The proof constructor operator for an assertion (EDB-membership) proof node
    /// (`assert(goal, reifier)`, [`crate::physical::proof`]).
    pub(crate) const ASSERT: &str = "https://blackcatinformatics.ca/logic/dag/op/assert";
}

/// A lowering diagnostic. Every consumer routes its hard failures through the
/// `logic-compile.ir` diagnostic kind (the IR well-formedness surface), so a lowering
/// defect surfaces as a typed [`Diag`], never a silent drop or a coercion.
fn ir_err(detail: String) -> Diag {
    Diag::of_kind(gmeow_logic_compile::error::Ir { detail })
}

/// Mint a bound-variable occurrence at de-Bruijn `distance`/`slot`, HARD-FAILING if either
/// exceeds the physical node's field width. A `Bound{debruijn: u32, slot: u16}` that
/// silently wrapped would rebind an occurrence to the wrong binder — a capture bug — so the
/// guard is where every consumer mints a bound occurrence.
fn intern_bound_checked(dag: &mut TermDag, distance: usize, slot: usize) -> Result<NodeId> {
    let debruijn = u32::try_from(distance).map_err(|_| {
        ir_err(format!(
            "binder de-Bruijn distance {distance} exceeds u32::MAX; a silent wrap would \
             rebind the occurrence to the wrong binder (variable-capture bug)"
        ))
    })?;
    let slot = u16::try_from(slot).map_err(|_| {
        ir_err(format!(
            "binder slot {slot} exceeds u16::MAX; a silent wrap would rebind the occurrence \
             to the wrong declaration slot (variable-capture bug)"
        ))
    })?;
    Ok(dag.intern_bound(debruijn, slot))
}

/// Resolve `name` against the binder-frame environment (innermost frame last) to a
/// de-Bruijn `(distance, slot)`, or `None` if it is free. Shared by every consumer — the
/// frames hold `logic:` variable names, `math:` declaration IRIs, or whatever token the
/// surface uses to identify a binding site.
fn resolve_debruijn(env: &[Vec<String>], name: &str) -> Option<(usize, usize)> {
    for (back, frame) in env.iter().rev().enumerate() {
        if let Some(slot) = frame.iter().position(|v| v == name) {
            return Some((back, slot));
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// logic: — the Rust `ir::Formula`/`Term` IR, lowered directly.
// ─────────────────────────────────────────────────────────────────────────────

/// Lower a `logic:` [`Formula`] into `dag`, returning its node id.
///
/// Reproduces exactly the equivalences [`Formula::content_key`] decides:
/// bound-variable alpha-renaming (locally-nameless de-Bruijn), commutative
/// flatten+order-normalization of `And`/`Or`/`Iff`, and ordered `Implies`. A
/// [`Term::SequenceMarker`] is a HARD FAIL (the arena has no variadic-binder node, so a
/// sequence marker cannot be coerced to a single-term occurrence).
pub(crate) fn lower_logic_formula(dag: &mut TermDag, f: &Formula) -> Result<NodeId> {
    let mut env: Vec<Vec<String>> = Vec::new();
    lower_formula_in(dag, f, &mut env)
}

/// Lower a `logic:` [`Term`] into `dag` under no enclosing binder (a free variable stays
/// free, an IRI/literal is a leaf). A [`Term::SequenceMarker`] is a HARD FAIL.
pub(crate) fn lower_logic_term(dag: &mut TermDag, t: &Term) -> Result<NodeId> {
    let env: Vec<Vec<String>> = Vec::new();
    lower_term_in(dag, t, &env)
}

fn lower_term_in(dag: &mut TermDag, term: &Term, env: &[Vec<String>]) -> Result<NodeId> {
    Ok(match term {
        Term::Iri(s) => dag.intern_leaf(TermValue::iri(s.clone())),
        Term::Literal { lexical, datatype } => {
            let tv = match datatype {
                None => TermValue::simple_literal(lexical.clone()),
                Some(dt) => TermValue::typed_literal(lexical.clone(), dt.clone()),
            };
            dag.intern_leaf(tv)
        }
        Term::Var(name) => match resolve_debruijn(env, name) {
            Some((distance, slot)) => intern_bound_checked(dag, distance, slot)?,
            None => dag.intern_free(TermValue::simple_literal(name.clone())),
        },
        Term::SequenceMarker(name) => {
            return Err(ir_err(format!(
                "sequence marker {name:?} binds a variable-length sequence, not a single term; \
                 the fixed-arity term DAG has no variadic-binder node, so lowering it is a hard \
                 fail rather than a silent single-term coercion"
            )));
        }
        Term::App { symbol, args } => {
            // A compound function-term application `f(t0, .., tn)`: mirror how
            // `Formula::Atom` lowers its relation (a reified leaf op applied to its lowered
            // argument carriers) — the reified function-symbol IRI becomes the `App` node's
            // `op` child, and each argument is lowered recursively through this same
            // function, so a nested application (`cons(H, cons(1, nil))`) round-trips.
            let op = dag.intern_leaf(TermValue::iri(symbol.clone()));
            let mut arg_nodes = Vec::with_capacity(args.len());
            for a in args {
                arg_nodes.push(lower_term_in(dag, a, env)?);
            }
            dag.intern_app(op, arg_nodes)
        }
    })
}

fn lower_formula_in(dag: &mut TermDag, f: &Formula, env: &mut Vec<Vec<String>>) -> Result<NodeId> {
    Ok(match f {
        Formula::Atom { relation, args } => {
            let op = lower_term_in(dag, relation, env)?;
            let mut arg_nodes = Vec::with_capacity(args.len());
            for a in args {
                arg_nodes.push(lower_term_in(dag, a, env)?);
            }
            dag.intern_app(op, arg_nodes)
        }
        Formula::Not(b) => {
            let op = dag.intern_leaf(TermValue::iri(canon::NOT));
            let child = lower_formula_in(dag, b, env)?;
            dag.intern_app(op, vec![child])
        }
        Formula::And(fs) => lower_commutative(dag, canon::AND, true, fs, env)?,
        Formula::Or(fs) => lower_commutative(dag, canon::OR, false, fs, env)?,
        Formula::Implies(a, b) => {
            let op = dag.intern_leaf(TermValue::iri(canon::IMPLIES));
            let la = lower_formula_in(dag, a, env)?;
            let lb = lower_formula_in(dag, b, env)?;
            dag.intern_app(op, vec![la, lb])
        }
        Formula::Iff(a, b) => {
            let op = dag.intern_leaf(TermValue::iri(canon::IFF));
            let mut pair = [
                lower_formula_in(dag, a, env)?,
                lower_formula_in(dag, b, env)?,
            ];
            // Sort by CONTENT KEY, never NodeId: a `NodeId` is an interning-order artifact
            // (arbitrary across two separate DAGs), while `dag.key(..)` is the same
            // structural fingerprint `ir.rs` sorts the biconditional's operand keys by, so
            // the same commutative formula built in two separate fresh DAGs interns to the
            // same content key regardless of interning order.
            pair.sort_by(|&x, &y| dag.key(x).cmp(dag.key(y)));
            dag.intern_app(op, pair.to_vec())
        }
        Formula::Forall { vars, body } => lower_logic_binder(dag, canon::FORALL, vars, body, env)?,
        Formula::Exists { vars, body } => lower_logic_binder(dag, canon::EXISTS, vars, body, env)?,
    })
}

/// Flatten a commutative connective's same-tag operands, mirroring `ir.rs`'s
/// `flatten_commutative`, so `And[And[a,b],c] ≡ And[a,b,c]`.
fn flatten_commutative<'a>(is_and: bool, fs: &'a [Formula], out: &mut Vec<&'a Formula>) {
    for f in fs {
        match (is_and, f) {
            (true, Formula::And(inner)) => flatten_commutative(is_and, inner, out),
            (false, Formula::Or(inner)) => flatten_commutative(is_and, inner, out),
            _ => out.push(f),
        }
    }
}

/// Lower a flattened, order-normalized commutative connective. Sorting the interned
/// operands by CONTENT KEY (never `NodeId`, which is only meaningful within the DAG that
/// minted it) canonicalizes operand order exactly as `ir.rs` sorts operand keys (duplicates
/// preserved), while the DAG `App` stays strictly positional.
fn lower_commutative(
    dag: &mut TermDag,
    op_iri: &str,
    is_and: bool,
    fs: &[Formula],
    env: &mut Vec<Vec<String>>,
) -> Result<NodeId> {
    let op = dag.intern_leaf(TermValue::iri(op_iri));
    let mut operands: Vec<&Formula> = Vec::new();
    flatten_commutative(is_and, fs, &mut operands);
    let mut nodes = Vec::with_capacity(operands.len());
    for f in operands {
        nodes.push(lower_formula_in(dag, f, env)?);
    }
    // Sort by CONTENT KEY, never NodeId (see the `Iff` arm's comment above): a `NodeId` is
    // interning-order-dependent and not comparable across two separate DAGs, while
    // `dag.key(..)` is the structural fingerprint, so this matches `ir.rs`'s own operand-key
    // sort and is deterministic regardless of which DAG / interning order built the operands.
    nodes.sort_by(|&x, &y| dag.key(x).cmp(dag.key(y)));
    Ok(dag.intern_app(op, nodes))
}

/// Lower a quantifier binder. Each bound variable becomes a slot with the untyped
/// individual sort (so the binder's arity is captured), and the body is lowered one
/// binder-depth deeper via a pushed frame — the bound names become de-Bruijn occurrences.
fn lower_logic_binder(
    dag: &mut TermDag,
    op_iri: &str,
    vars: &[String],
    body: &Formula,
    env: &mut Vec<Vec<String>>,
) -> Result<NodeId> {
    let op = dag.intern_leaf(TermValue::iri(op_iri));
    let sort = dag.intern_leaf(TermValue::iri(canon::SORT_INDIVIDUAL));
    let sorts = vec![sort; vars.len()];
    env.push(vars.to_vec());
    let body_node = lower_formula_in(dag, body, env);
    env.pop();
    let body_node = body_node?;
    Ok(dag.intern_binder(op, sorts, body_node))
}

// ─────────────────────────────────────────────────────────────────────────────
// math: — the RDF-authored application/binding expression vocabulary.
// ─────────────────────────────────────────────────────────────────────────────

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const M_APPLICATION: &str = "https://blackcatinformatics.ca/math/ApplicationExpression";
const M_BINDING: &str = "https://blackcatinformatics.ca/math/BindingExpression";
const M_VARIABLE_EXPRESSION: &str = "https://blackcatinformatics.ca/math/VariableExpression";
const M_FREE_DECLARATION: &str = "https://blackcatinformatics.ca/math/FreeVariableDeclaration";
const M_NUMBER_LITERAL: &str = "https://blackcatinformatics.ca/math/NumberLiteral";
const M_OPERATOR: &str = "https://blackcatinformatics.ca/math/operator";
const M_ARGUMENT_SLOT: &str = "https://blackcatinformatics.ca/math/argumentSlot";
const M_SLOT_INDEX: &str = "https://blackcatinformatics.ca/math/slotIndex";
const M_SLOT_EXPRESSION: &str = "https://blackcatinformatics.ca/math/slotExpression";
const M_BOUND_VARIABLE: &str = "https://blackcatinformatics.ca/math/boundVariable";
const M_VARIABLE_OCCURRENCE: &str = "https://blackcatinformatics.ca/math/variableOccurrence";
const M_DECLARED_VARIABLE: &str = "https://blackcatinformatics.ca/math/declaredVariable";
const M_DOMAIN: &str = "https://blackcatinformatics.ca/math/domain";
const M_LITERAL_VALUE: &str = "https://blackcatinformatics.ca/math/literalValue";

/// One resolved object of a `(subject, predicate, ?)` edge in a [`MathGraph`].
#[derive(Debug, Clone)]
enum Obj {
    /// An IRI or blank-node reference — a node the lowering can follow.
    Ref(String),
    /// A literal (e.g. a `math:slotIndex` integer or a `math:literalValue` number), carrying
    /// its lexical form AND its datatype/language — a `math:NumberLiteral` must lower with
    /// its datatype/language intact, never discarded.
    Lit {
        /// The lexical form, byte-for-byte as authored.
        lexical: String,
        /// The datatype IRI (never empty — the parser expands the RDF default).
        datatype: String,
        /// The (lowercased) language tag, for a language-tagged literal.
        language: Option<String>,
    },
}

/// A read-only subject → predicate → objects index over the default graph of a parsed
/// `math:` expression dataset — the substrate the `math:` lowering walks.
///
/// The `math:` expression tree has no typed Rust AST: it is RDF, so the lowering reads it
/// straight out of a `purrdf` dataset (parsed from Turtle here, identical to how a shipped
/// `.gts` bundle would present it). Blank nodes are keyed by label (unique within one
/// parsed default graph), so both IRI-named and blank-node expression nodes resolve.
pub(crate) struct MathGraph {
    index: BTreeMap<String, BTreeMap<String, Vec<Obj>>>,
}

impl MathGraph {
    /// Build a [`MathGraph`] from a Turtle document of the `math:` expression vocabulary.
    pub(crate) fn from_turtle(turtle: &[u8]) -> Result<Self> {
        let dataset = purrdf::parse_dataset(turtle, "text/turtle", None)
            .map_err(|err| ir_err(format!("cannot parse math expression Turtle: {err}")))?;
        let mut index: BTreeMap<String, BTreeMap<String, Vec<Obj>>> = BTreeMap::new();
        for quad in dataset.quad_refs() {
            // The expression vocab is authored in the default graph.
            if quad.g.is_some() {
                continue;
            }
            let (Some(subject), TermRef::Iri(predicate), Some(object)) =
                (node_key(&quad.s), quad.p, obj_of(&quad.o, &dataset))
            else {
                continue;
            };
            index
                .entry(subject)
                .or_default()
                .entry(predicate.to_owned())
                .or_default()
                .push(object);
        }
        Ok(Self { index })
    }

    fn objects(&self, subject: &str, predicate: &str) -> &[Obj] {
        self.index
            .get(subject)
            .and_then(|preds| preds.get(predicate))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The first IRI/blank object of `(subject, predicate, ?)`, if any.
    fn first_ref(&self, subject: &str, predicate: &str) -> Option<&str> {
        self.objects(subject, predicate)
            .iter()
            .find_map(|o| match o {
                Obj::Ref(value) => Some(value.as_str()),
                Obj::Lit { .. } => None,
            })
    }

    /// Every IRI/blank object of `(subject, predicate, ?)`, in index order.
    fn refs(&self, subject: &str, predicate: &str) -> impl Iterator<Item = &str> {
        self.objects(subject, predicate)
            .iter()
            .filter_map(|o| match o {
                Obj::Ref(value) => Some(value.as_str()),
                Obj::Lit { .. } => None,
            })
    }

    /// The first literal lexical form of `(subject, predicate, ?)`, if any. Datatype/language
    /// fidelity is dropped here deliberately — callers that need full literal identity (a
    /// `math:NumberLiteral`'s `math:literalValue`) use [`Self::first_lit_typed`] instead.
    fn first_lit(&self, subject: &str, predicate: &str) -> Option<&str> {
        self.first_lit_typed(subject, predicate)
            .map(|(lexical, _, _)| lexical)
    }

    /// The first literal object of `(subject, predicate, ?)`, if any, as
    /// `(lexical, datatype, language)` — full fidelity, never discarding the datatype/language
    /// a `math:NumberLiteral`'s `math:literalValue` carries.
    fn first_lit_typed(
        &self,
        subject: &str,
        predicate: &str,
    ) -> Option<(&str, &str, Option<&str>)> {
        self.objects(subject, predicate)
            .iter()
            .find_map(|o| match o {
                Obj::Lit {
                    lexical,
                    datatype,
                    language,
                } => Some((lexical.as_str(), datatype.as_str(), language.as_deref())),
                Obj::Ref(_) => None,
            })
    }

    /// The `rdf:type` IRIs of `subject`.
    fn types(&self, subject: &str) -> Vec<&str> {
        self.refs(subject, RDF_TYPE).collect()
    }

    /// Whether `subject` carries `rdf:type` `class`.
    fn has_type(&self, subject: &str, class: &str) -> bool {
        self.refs(subject, RDF_TYPE).any(|t| t == class)
    }
}

/// The followable-node key of a subject `TermRef` (an IRI or blank node), or `None` for a
/// literal / triple term (neither can be the subject of an expression edge).
fn node_key(term: &TermRef<'_>) -> Option<String> {
    match term {
        TermRef::Iri(iri) => Some((*iri).to_owned()),
        TermRef::Blank { label, .. } => Some(format!("_:{label}")),
        TermRef::Literal { .. } | TermRef::Triple { .. } => None,
    }
}

/// The object of an expression edge: an IRI/blank reference, or a literal carrying its
/// lexical form AND its datatype/language (resolved against `dataset`, since a borrowed
/// [`TermRef`]'s datatype is a dataset-local [`purrdf::TermId`], not a portable IRI string —
/// never dropped, so a `math:NumberLiteral`'s datatype survives into the [`MathGraph`]
/// index).
fn obj_of(term: &TermRef<'_>, dataset: &purrdf::RdfDataset) -> Option<Obj> {
    match term {
        TermRef::Iri(iri) => Some(Obj::Ref((*iri).to_owned())),
        TermRef::Blank { label, .. } => Some(Obj::Ref(format!("_:{label}"))),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            ..
        } => {
            let datatype = match dataset.resolve(*datatype) {
                TermRef::Iri(dt) => dt.to_owned(),
                other => unreachable!(
                    "a literal's datatype term resolves to an IRI by RDF construction, got \
                     {other:?}"
                ),
            };
            Some(Obj::Lit {
                lexical: (*lexical).to_owned(),
                datatype,
                language: language.map(|l| (*l).to_owned()),
            })
        }
        TermRef::Triple { .. } => None,
    }
}

/// Lower the `math:` expression rooted at `root` in `graph` into `dag`, returning its node
/// id.
///
/// - `math:ApplicationExpression` → `App` with `math:slotIndex`-ordered args (validated
///   zero-based, contiguous, and duplicate-free — a violated slot sequence is a HARD FAIL).
/// - `math:BindingExpression` → `Binder`; its `math:boundVariable` declaration's declared
///   `math:domain` becomes the binder's sort child (defaulting to the untyped individual
///   sort — the declared type is NEVER dropped), and its body is the single indexed slot.
/// - `math:VariableExpression` → `Bound` if its occurrence's `math:declaredVariable`
///   resolves to an enclosing binder, else `Free` iff the declaration is a
///   `math:FreeVariableDeclaration` (an occurrence bound to nothing is a HARD FAIL).
/// - `math:NumberLiteral` → a `Leaf` of its `math:literalValue`, a TYPED (or
///   language-tagged) RDF literal — the datatype/language is NEVER dropped; a bare IRI
///   operand → a `Leaf` of that IRI.
pub(crate) fn lower_math_expression(
    dag: &mut TermDag,
    graph: &MathGraph,
    root: &str,
) -> Result<NodeId> {
    let mut env: Vec<Vec<String>> = Vec::new();
    lower_math_node(dag, graph, root, &mut env)
}

fn lower_math_node(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &mut Vec<Vec<String>>,
) -> Result<NodeId> {
    let types = graph.types(node);
    if types.contains(&M_APPLICATION) {
        lower_math_application(dag, graph, node, env)
    } else if types.contains(&M_BINDING) {
        lower_math_binding(dag, graph, node, env)
    } else if types.contains(&M_VARIABLE_EXPRESSION) {
        lower_math_variable(dag, graph, node, env)
    } else if types.contains(&M_NUMBER_LITERAL) {
        // A `math:literalValue` is an RDF literal (e.g. `"42"^^xsd:integer`), never an
        // IRI/blank reference, so it is read with `first_lit_typed` — and its datatype/
        // language is preserved into the interned leaf, never discarded (a bare
        // `TermValue::iri` here would silently coerce a typed number to an untyped
        // constant).
        let (lexical, datatype, language) = graph
            .first_lit_typed(node, M_LITERAL_VALUE)
            .ok_or_else(|| {
                ir_err(format!(
                    "math:NumberLiteral {node} missing math:literalValue"
                ))
            })?;
        let tv = match language {
            Some(lang) => TermValue::lang_literal(lexical.to_owned(), lang),
            None => TermValue::typed_literal(lexical.to_owned(), datatype.to_owned()),
        };
        Ok(dag.intern_leaf(tv))
    } else if !node.starts_with("_:") {
        // A bare IRI constant operand (a mathematical symbol / individual): a leaf.
        Ok(dag.intern_leaf(TermValue::iri(node.to_owned())))
    } else {
        Err(ir_err(format!(
            "math expression node {node} has no recognized expression type \
             (math:ApplicationExpression / math:BindingExpression / math:VariableExpression / \
             math:NumberLiteral) and is not an IRI constant"
        )))
    }
}

/// Collect a node's `math:argumentSlot` slot expressions in `math:slotIndex` order,
/// HARD-FAILING unless the indexes are zero-based, contiguous, and duplicate-free.
fn collect_slots(graph: &MathGraph, node: &str) -> Result<Vec<String>> {
    let mut indexed: Vec<(i128, String)> = Vec::new();
    for slot in graph.refs(node, M_ARGUMENT_SLOT) {
        let index_lex = graph
            .first_lit(slot, M_SLOT_INDEX)
            .ok_or_else(|| ir_err(format!("math:ArgumentSlot {slot} missing math:slotIndex")))?;
        let index: i128 = index_lex.trim().parse().map_err(|_| {
            ir_err(format!(
                "math:slotIndex {index_lex:?} on {slot} is not an integer"
            ))
        })?;
        let expr = graph.first_ref(slot, M_SLOT_EXPRESSION).ok_or_else(|| {
            ir_err(format!(
                "math:ArgumentSlot {slot} missing math:slotExpression"
            ))
        })?;
        indexed.push((index, expr.to_owned()));
    }
    indexed.sort_by_key(|(index, _)| *index);
    for (expected, (index, _)) in indexed.iter().enumerate() {
        if *index != expected as i128 {
            return Err(ir_err(format!(
                "math:argumentSlot indexes of {node} must be zero-based and contiguous with no \
                 gaps or duplicates; got index {index} at ordered position {expected}"
            )));
        }
    }
    Ok(indexed.into_iter().map(|(_, expr)| expr).collect())
}

fn lower_math_application(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &mut Vec<Vec<String>>,
) -> Result<NodeId> {
    let operator = graph.first_ref(node, M_OPERATOR).ok_or_else(|| {
        ir_err(format!(
            "math:ApplicationExpression {node} missing math:operator"
        ))
    })?;
    let op = dag.intern_leaf(TermValue::iri(operator.to_owned()));
    let slot_exprs = collect_slots(graph, node)?;
    let mut args = Vec::with_capacity(slot_exprs.len());
    for expr in &slot_exprs {
        args.push(lower_math_node(dag, graph, expr, env)?);
    }
    Ok(dag.intern_app(op, args))
}

fn lower_math_binding(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &mut Vec<Vec<String>>,
) -> Result<NodeId> {
    let operator = graph.first_ref(node, M_OPERATOR).ok_or_else(|| {
        ir_err(format!(
            "math:BindingExpression {node} missing math:operator"
        ))
    })?;
    let op = dag.intern_leaf(TermValue::iri(operator.to_owned()));
    let declaration = graph
        .first_ref(node, M_BOUND_VARIABLE)
        .ok_or_else(|| {
            ir_err(format!(
                "math:BindingExpression {node} missing math:boundVariable"
            ))
        })?
        .to_owned();
    // The bound variable's declared type/domain becomes the binder's sort child and is
    // never dropped; an undeclared domain defaults to the untyped individual sort (so an
    // undeclared `math:` binder collapses with an untyped `logic:` quantifier).
    let sort_iri = graph
        .first_ref(&declaration, M_DOMAIN)
        .unwrap_or(canon::SORT_INDIVIDUAL);
    let sort = dag.intern_leaf(TermValue::iri(sort_iri.to_owned()));
    // A binder binds over exactly one body, carried as its single index-0 argument slot.
    let body_slots = collect_slots(graph, node)?;
    if body_slots.len() != 1 {
        return Err(ir_err(format!(
            "math:BindingExpression {node} must carry exactly one body slot (math:slotIndex 0); \
             found {} slot(s)",
            body_slots.len()
        )));
    }
    env.push(vec![declaration]);
    let body = lower_math_node(dag, graph, &body_slots[0], env);
    env.pop();
    let body = body?;
    Ok(dag.intern_binder(op, vec![sort], body))
}

fn lower_math_variable(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &[Vec<String>],
) -> Result<NodeId> {
    let occurrence = graph
        .first_ref(node, M_VARIABLE_OCCURRENCE)
        .ok_or_else(|| {
            ir_err(format!(
                "math:VariableExpression {node} missing math:variableOccurrence"
            ))
        })?;
    let declaration = graph
        .first_ref(occurrence, M_DECLARED_VARIABLE)
        .ok_or_else(|| {
            ir_err(format!(
                "math:VariableOccurrence {occurrence} missing math:declaredVariable"
            ))
        })?;
    if let Some((distance, slot)) = resolve_debruijn(env, declaration) {
        return intern_bound_checked(dag, distance, slot);
    }
    if graph.has_type(declaration, M_FREE_DECLARATION) {
        return Ok(dag.intern_free(TermValue::iri(declaration.to_owned())));
    }
    Err(ir_err(format!(
        "math:VariableOccurrence {occurrence} resolves to declaration {declaration}, which is \
         neither bound by an enclosing math:BindingExpression nor a \
         math:FreeVariableDeclaration (unscoped occurrence)"
    )))
}

// ─────────────────────────────────────────────────────────────────────────────
// lang: — a form paired with its one-way lang:→logic: denotation.
// ─────────────────────────────────────────────────────────────────────────────

/// A `lang:` denotation target, typed by its `lang:denotationKind`.
///
/// Formal meaning bottoms out in a `logic:` object (`LANG-MEANING.md`, the one-way
/// `lang:`→`logic:` bridge): a declarative sentence denotes a `logic:` formula, a referring
/// expression a `logic:` term, and a common noun / entity reference a `logic:` type / IRI.
/// The form's meaning IS its denotation target, so lowering a form is lowering its target.
pub(crate) enum LangDenotation {
    /// `lang:denotesLogicFormula` — the target is a `logic:` [`Formula`].
    LogicFormula(Formula),
    /// `lang:denotesLogicTerm` — the target is a `logic:` [`Term`].
    LogicTerm(Term),
    /// `lang:denotesEntity` — the target is a GMEOW individual, by IRI.
    Entity(String),
    /// `lang:denotesClass` — the target is a class, by IRI.
    Class(String),
}

/// A `lang:` [`Form`](gmeow_lang_form::Form) carrying its `lang:`→`logic:` denotation — the
/// meaning-record pair the `lang:` lowering consumes.
pub(crate) struct LangDenotedForm {
    /// The denoted form (never a surface form — meaning attaches above the byte level).
    pub(crate) form: gmeow_lang_form::Form,
    /// Its denotation target, typed by kind.
    pub(crate) denotation: LangDenotation,
}

/// Lower a `lang:` denoted form into `dag`, returning its node id.
///
/// The bridge is ONE-WAY (`lang:` → `logic:`): a form's formal meaning is its denotation
/// target, so the lowering dispatches on the denotation kind and reuses the `logic:`
/// lowering for a formula/term target (an alpha-equivalent `logic:`, `math:`, or `lang:`
/// input therefore all intern to one node). A form carrying a denotation must name a
/// non-empty sign system; a denotation on an ill-formed form is a HARD FAIL.
pub(crate) fn lower_lang_form(dag: &mut TermDag, denoted: &LangDenotedForm) -> Result<NodeId> {
    if denoted.form.sign_system().trim().is_empty() {
        return Err(ir_err(
            "a lang: form carrying a denotation must name a non-empty sign system".to_owned(),
        ));
    }
    lower_lang_denotation(dag, &denoted.denotation)
}

/// Lower a `lang:` denotation target into `dag`, dispatching on its kind.
pub(crate) fn lower_lang_denotation(
    dag: &mut TermDag,
    denotation: &LangDenotation,
) -> Result<NodeId> {
    match denotation {
        LangDenotation::LogicFormula(f) => lower_logic_formula(dag, f),
        LangDenotation::LogicTerm(t) => lower_logic_term(dag, t),
        LangDenotation::Entity(iri) | LangDenotation::Class(iri) => {
            if iri.trim().is_empty() {
                return Err(ir_err(
                    "lang: denotation target IRI must be non-empty".to_owned(),
                ));
            }
            Ok(dag.intern_leaf(TermValue::iri(iri.clone())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::physical::term_dag::TermDag;

    fn forall_p_x() -> Formula {
        // ∀x. p(x)
        Formula::Forall {
            vars: vec!["x".to_owned()],
            body: Box::new(
                Formula::atom(
                    Term::iri("https://example.org/p").expect("iri"),
                    vec![Term::var("x").expect("var")],
                )
                .expect("atom"),
            ),
        }
    }

    /// A `lang:` form (an English sentence) whose meaning IS a `logic:` formula.
    fn sentence_form() -> gmeow_lang_form::Form {
        gmeow_lang_form::Form::Composed {
            sign_system: "https://example.org/english".to_owned(),
            level: "sentence".to_owned(),
            analysis: None,
            head: None,
            slots: Vec::new(),
        }
    }

    // ── THE acceptance: one arena, three surfaces, alpha-equivalent ⇒ one node ──────────

    #[test]
    fn cross_consumer_alpha_equivalent_interns_to_one_node_and_key() {
        const P: &str = "https://example.org/p";
        let mut dag = TermDag::new();

        // (1) logic: ∀x. p(x) as an ir::Formula, lowered directly.
        let logic_formula = forall_p_x();
        let logic_node = lower_logic_formula(&mut dag, &logic_formula).expect("logic lowering");

        // (2) math: the SAME shape as a BindingExpression (∀-operator) binding one
        //     occurrence of p applied to the bound variable, authored as RDF.
        let math_ttl = format!(
            "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix op: <https://blackcatinformatics.ca/logic/dag/op/> .\n\
             ex:binder a math:BindingExpression ;\n\
             \x20 math:operator op:forall ;\n\
             \x20 math:boundVariable ex:xDecl ;\n\
             \x20 math:argumentSlot ex:bodySlot .\n\
             ex:xDecl a math:VariableDeclaration .\n\
             ex:bodySlot a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:app .\n\
             ex:app a math:ApplicationExpression ;\n\
             \x20 math:operator <{P}> ;\n\
             \x20 math:argumentSlot ex:appSlot0 .\n\
             ex:appSlot0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:xOcc .\n\
             ex:xOcc a math:VariableExpression ; math:variableOccurrence ex:xOccurrence .\n\
             ex:xOccurrence a math:VariableOccurrence ; math:declaredVariable ex:xDecl .\n"
        );
        let math_graph = MathGraph::from_turtle(math_ttl.as_bytes()).expect("math parse");
        let math_node = lower_math_expression(&mut dag, &math_graph, "https://example.org/binder")
            .expect("math lowering");

        // (3) lang: a form whose denotation IS the same logic formula.
        let denoted = LangDenotedForm {
            form: sentence_form(),
            denotation: LangDenotation::LogicFormula(forall_p_x()),
        };
        let lang_node = lower_lang_form(&mut dag, &denoted).expect("lang lowering");

        // All three alpha-equivalent inputs collapse to ONE NodeId and ONE content key.
        assert_eq!(
            logic_node, math_node,
            "logic: and math: alpha-equivalent inputs must intern to one NodeId"
        );
        assert_eq!(
            logic_node, lang_node,
            "lang: denotation must intern to the same NodeId as its logic: target"
        );
        assert_eq!(
            dag.key(logic_node),
            dag.key(math_node),
            "logic: and math: must share a byte-identical content key"
        );
        assert_eq!(
            dag.key(logic_node),
            dag.key(lang_node),
            "lang: must share the byte-identical content key"
        );

        // Guard against a vacuous acceptance: the shared node really is a binder over an
        // application over a bound-variable occurrence (BIND / APP / B kind tags present).
        let key = dag.key(logic_node);
        assert!(key.starts_with("BIND"), "shared node is a binder: {key}");
        assert!(key.contains("APP"), "binder body is an application: {key}");
        assert!(
            key.contains('B'),
            "application argument is a bound occurrence: {key}"
        );
    }

    // ── math: slotIndex ordering is respected; a slot gap hard-fails ────────────────────

    fn application_ttl(slots: &[(i64, &str)]) -> String {
        let mut ttl = String::from(
            "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ",
        );
        for (i, _) in slots.iter().enumerate() {
            ttl.push_str(&format!("; math:argumentSlot ex:s{i} "));
        }
        ttl.push_str(".\n");
        for (i, (index, expr)) in slots.iter().enumerate() {
            ttl.push_str(&format!(
                "ex:s{i} a math:ArgumentSlot ; math:slotIndex {index} ; math:slotExpression {expr} .\n"
            ));
        }
        ttl
    }

    #[test]
    fn math_slot_index_orders_operands_regardless_of_authoring_order() {
        let mut dag = TermDag::new();
        // p(a, b) authored slots-forward and slots-reversed must intern identically.
        let forward =
            MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (1, "ex:b")]).as_bytes())
                .expect("parse");
        let reversed =
            MathGraph::from_turtle(application_ttl(&[(1, "ex:b"), (0, "ex:a")]).as_bytes())
                .expect("parse");
        let n_forward =
            lower_math_expression(&mut dag, &forward, "https://example.org/app").expect("forward");
        let n_reversed = lower_math_expression(&mut dag, &reversed, "https://example.org/app")
            .expect("reversed");
        assert_eq!(
            n_forward, n_reversed,
            "operand order is carried by slotIndex, not authoring order"
        );

        // It matches the logic: atom p(a, b) — cross-consumer, same arena.
        let logic_pab = lower_logic_formula(
            &mut dag,
            &Formula::atom(
                Term::iri("https://example.org/p").unwrap(),
                vec![
                    Term::iri("https://example.org/a").unwrap(),
                    Term::iri("https://example.org/b").unwrap(),
                ],
            )
            .unwrap(),
        )
        .expect("logic p(a,b)");
        assert_eq!(n_forward, logic_pab, "math p(a,b) == logic p(a,b)");

        // Swapping the operands (p(b, a)) is a DISTINCT node — order is identity-bearing.
        let swapped =
            MathGraph::from_turtle(application_ttl(&[(0, "ex:b"), (1, "ex:a")]).as_bytes())
                .expect("parse");
        let n_swapped =
            lower_math_expression(&mut dag, &swapped, "https://example.org/app").expect("swapped");
        assert_ne!(n_forward, n_swapped, "p(a,b) and p(b,a) are distinct");
    }

    #[test]
    fn math_slot_gap_hard_fails() {
        let mut dag = TermDag::new();
        // Indexes {0, 2} are not contiguous — a hard fail, never a silent renumber.
        let graph = MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (2, "ex:b")]).as_bytes())
            .expect("parse");
        let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
            .expect_err("slot gap must hard-fail");
        assert!(
            err.message().contains("contiguous"),
            "gap diagnostic names contiguity: {}",
            err.message()
        );
    }

    // ── math: a declared bound-variable domain becomes a distinct sort child ────────────

    fn binder_ttl(domain: Option<&str>) -> String {
        let domain_line = match domain {
            Some(d) => format!("ex:xDecl a math:VariableDeclaration ; math:domain {d} .\n"),
            None => "ex:xDecl a math:VariableDeclaration .\n".to_owned(),
        };
        format!(
            "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix op: <https://blackcatinformatics.ca/logic/dag/op/> .\n\
             ex:binder a math:BindingExpression ;\n\
             \x20 math:operator op:forall ;\n\
             \x20 math:boundVariable ex:xDecl ;\n\
             \x20 math:argumentSlot ex:bodySlot .\n\
             {domain_line}\
             ex:bodySlot a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:app .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:xOcc .\n\
             ex:xOcc a math:VariableExpression ; math:variableOccurrence ex:xOccurrence .\n\
             ex:xOccurrence a math:VariableOccurrence ; math:declaredVariable ex:xDecl .\n"
        )
    }

    #[test]
    fn math_declared_domain_changes_binder_sort_child() {
        let mut dag = TermDag::new();
        let untyped = MathGraph::from_turtle(binder_ttl(None).as_bytes()).expect("parse");
        let typed = MathGraph::from_turtle(binder_ttl(Some("ex:Reals")).as_bytes()).expect("parse");
        let n_untyped = lower_math_expression(&mut dag, &untyped, "https://example.org/binder")
            .expect("untyped");
        let n_typed =
            lower_math_expression(&mut dag, &typed, "https://example.org/binder").expect("typed");
        assert_ne!(
            n_untyped, n_typed,
            "a declared bound-variable domain is a distinct sort child (not lost)"
        );
        assert_ne!(
            dag.key(n_untyped),
            dag.key(n_typed),
            "distinct content keys"
        );

        // The untyped math binder collapses with the untyped logic ∀ (default sort shared).
        let logic_node = lower_logic_formula(&mut dag, &forall_p_x()).expect("logic");
        assert_eq!(
            n_untyped, logic_node,
            "an undeclared math: domain defaults to the untyped individual sort"
        );
    }

    // ── math: free vs unscoped occurrences ──────────────────────────────────────────────

    #[test]
    fn math_free_declaration_lowers_to_free_and_unscoped_hard_fails() {
        // A free occurrence (declaration is a math:FreeVariableDeclaration) → a Free node.
        let free_ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:yOcc .\n\
             ex:yOcc a math:VariableExpression ; math:variableOccurrence ex:yOccurrence .\n\
             ex:yOccurrence a math:VariableOccurrence ; math:declaredVariable ex:yDecl .\n\
             ex:yDecl a math:FreeVariableDeclaration .\n";
        let mut dag = TermDag::new();
        let graph = MathGraph::from_turtle(free_ttl.as_bytes()).expect("parse");
        let node = lower_math_expression(&mut dag, &graph, "https://example.org/app")
            .expect("free occurrence lowers");
        // p(free y): the argument is a Free node (kind tag `V`), not a Bound one.
        assert!(
            dag.key(node).contains('V'),
            "free var → Free node: {}",
            dag.key(node)
        );

        // An occurrence whose declaration is neither bound nor free-declared is a hard fail.
        let unscoped_ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:zOcc .\n\
             ex:zOcc a math:VariableExpression ; math:variableOccurrence ex:zOccurrence .\n\
             ex:zOccurrence a math:VariableOccurrence ; math:declaredVariable ex:zDecl .\n\
             ex:zDecl a math:VariableDeclaration .\n";
        let graph = MathGraph::from_turtle(unscoped_ttl.as_bytes()).expect("parse");
        let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
            .expect_err("unscoped occurrence hard-fails");
        assert!(
            err.message().contains("unscoped"),
            "diagnostic names the unscoped occurrence: {}",
            err.message()
        );
    }

    // ── logic: a sequence marker is a hard fail, never a silent single-term coercion ────

    #[test]
    fn logic_sequence_marker_hard_fails() {
        let mut dag = TermDag::new();
        let formula = Formula::atom(
            Term::iri("https://example.org/p").unwrap(),
            vec![Term::sequence_marker("xs").unwrap()],
        )
        .unwrap();
        let err = lower_logic_formula(&mut dag, &formula).expect_err("sequence marker hard-fails");
        assert!(
            err.message().contains("sequence marker"),
            "diagnostic names the sequence marker: {}",
            err.message()
        );
    }

    // ── overflow guard: minting a Bound occurrence past the field width hard-fails ──────

    #[test]
    fn bound_slot_overflow_hard_fails() {
        // A binder with u16::MAX + 2 slots, whose body references the last (slot 65536):
        // minting Bound{slot: 65536} must hard-fail rather than silently wrap a u16.
        let mut dag = TermDag::new();
        let count: usize = u16::MAX as usize + 2; // 65537
        let vars: Vec<String> = (0..count).map(|i| format!("v{i}")).collect();
        let last = format!("v{}", count - 1); // resolves to slot 65536
        let formula = Formula::Forall {
            vars,
            body: Box::new(
                Formula::atom(
                    Term::iri("https://example.org/p").unwrap(),
                    vec![Term::var(last).unwrap()],
                )
                .unwrap(),
            ),
        };
        let err = lower_logic_formula(&mut dag, &formula).expect_err("slot overflow hard-fails");
        assert!(
            err.message().contains("u16::MAX") && err.message().contains("slot"),
            "diagnostic names the slot overflow: {}",
            err.message()
        );
    }

    // ── lang: an ill-formed form (empty sign system) carrying a denotation hard-fails ──

    #[test]
    fn lang_empty_sign_system_hard_fails() {
        let mut dag = TermDag::new();
        let denoted = LangDenotedForm {
            form: gmeow_lang_form::Form::Composed {
                sign_system: String::new(),
                level: "sentence".to_owned(),
                analysis: None,
                head: None,
                slots: Vec::new(),
            },
            denotation: LangDenotation::LogicFormula(forall_p_x()),
        };
        let err = lower_lang_form(&mut dag, &denoted).expect_err("empty sign system hard-fails");
        assert!(
            err.message().contains("sign system"),
            "diagnostic names the sign system: {}",
            err.message()
        );
    }

    #[test]
    fn lang_entity_and_class_denotations_lower_to_leaf() {
        let mut dag = TermDag::new();
        let entity = lower_lang_denotation(
            &mut dag,
            &LangDenotation::Entity("https://example.org/venus".to_owned()),
        )
        .expect("entity");
        // The same IRI as a bare logic: term leaf must intern to the same node.
        let term_leaf =
            lower_logic_term(&mut dag, &Term::iri("https://example.org/venus").unwrap())
                .expect("term");
        assert_eq!(
            entity, term_leaf,
            "lang: entity IRI and logic: IRI leaf coincide"
        );

        let empty = lower_lang_denotation(&mut dag, &LangDenotation::Class(String::new()))
            .expect_err("empty class IRI hard-fails");
        assert!(empty.message().contains("non-empty"), "{}", empty.message());
    }

    // ── logic: Term::App lowers into a real App node (the former hard-fail seam) ───────

    #[test]
    fn term_app_lowers_matching_hand_built_intern_app() {
        let mut dag = TermDag::new();
        let term = Term::app(
            "https://example.org/f",
            vec![
                Term::iri("https://example.org/a").unwrap(),
                Term::iri("https://example.org/b").unwrap(),
            ],
        )
        .unwrap();
        let lowered = lower_logic_term(&mut dag, &term).expect("Term::App lowers");

        // By hand, exactly mirroring `Formula::Atom`'s own lowering shape: a reified leaf
        // op applied to its lowered argument carriers.
        let op = dag.intern_leaf(TermValue::iri("https://example.org/f"));
        let a = dag.intern_leaf(TermValue::iri("https://example.org/a"));
        let b = dag.intern_leaf(TermValue::iri("https://example.org/b"));
        let hand_built = dag.intern_app(op, vec![a, b]);

        // Hash-consing means a matching by-hand build interns to the SAME NodeId, not
        // merely an equal content key.
        assert_eq!(
            lowered, hand_built,
            "Term::App lowering interns to the same node as a by-hand intern_app build"
        );
        assert_eq!(dag.key(lowered), dag.key(hand_built));
        assert!(
            dag.key(lowered).starts_with("APP"),
            "lowered node is an application: {}",
            dag.key(lowered)
        );
    }

    #[test]
    fn nested_application_lowers_and_round_trips() {
        // cons(H, cons(1, nil)): the second argument is itself an application, so a nested
        // `Term::App` must round-trip through the lowering, not just a flat one.
        fn cons_h_cons_one_nil() -> Term {
            Term::app(
                "https://example.org/cons",
                vec![
                    Term::var("H").unwrap(),
                    Term::app(
                        "https://example.org/cons",
                        vec![
                            Term::literal("1", None).unwrap(),
                            Term::iri("https://example.org/nil").unwrap(),
                        ],
                    )
                    .unwrap(),
                ],
            )
            .unwrap()
        }

        // Built and lowered in two SEPARATE fresh arenas: the nested shape must fold to
        // the identical content key regardless of which arena minted it — hash-consing
        // determinism for a NESTED application, not just a flat one.
        let mut dag_a = TermDag::new();
        let node_a =
            lower_logic_term(&mut dag_a, &cons_h_cons_one_nil()).expect("nested lowers (a)");
        let mut dag_b = TermDag::new();
        let node_b =
            lower_logic_term(&mut dag_b, &cons_h_cons_one_nil()).expect("nested lowers (b)");
        assert_eq!(
            dag_a.key(node_a),
            dag_b.key(node_b),
            "the same nested-application shape interns to the same content key in a \
             separate arena"
        );

        // Structural sanity: TWO application nodes (outer cons, inner cons) and the free
        // occurrence of H both survive lowering.
        let key = dag_a.key(node_a);
        assert_eq!(
            key.matches("APP").count(),
            2,
            "outer and inner cons applications both lowered: {key}"
        );
        assert!(
            key.contains("free_\"H\""),
            "H lowers as a free occurrence: {key}"
        );
        assert!(key.contains("nil"), "the nil constant survives: {key}");
    }

    // ── G4: commutative sort key is CONTENT KEY, never NodeId ───────────────────────────

    #[test]
    fn g4_and_operand_order_content_key_stable_across_separate_dags() {
        fn atom(name: &str) -> Formula {
            Formula::atom(
                Term::iri(format!("https://example.org/{name}")).unwrap(),
                Vec::new(),
            )
            .unwrap()
        }
        let pq = Formula::And(vec![atom("p"), atom("q")]);
        let qp = Formula::And(vec![atom("q"), atom("p")]);

        // Built and interned in two SEPARATE fresh DAGs, so `p`/`q`'s NodeIds are minted
        // in the OPPOSITE order between the two arenas — a NodeId-keyed sort would then
        // disagree on operand order between the two DAGs, while a content-key-keyed sort
        // agrees regardless.
        let mut dag1 = TermDag::new();
        let node_pq = lower_logic_formula(&mut dag1, &pq).expect("And[p,q] lowers");
        let mut dag2 = TermDag::new();
        let node_qp = lower_logic_formula(&mut dag2, &qp).expect("And[q,p] lowers");

        assert_eq!(
            dag1.key(node_pq),
            dag2.key(node_qp),
            "And[p,q] and And[q,p], each built in a SEPARATE fresh DAG, must intern to the \
             same content key regardless of interning order (sorted by content key, never \
             NodeId)"
        );
    }

    // ── G7: a math:NumberLiteral's typed literalValue lowers, datatype preserved ────────

    #[test]
    fn g7_math_number_literal_preserves_typed_datatype() {
        // `math:literalValue` as a genuine RDF typed literal (`"42"^^xsd:integer`) must
        // lower to a TYPED leaf — not hard-fail, and not silently drop the datatype.
        let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:lit a math:NumberLiteral ; math:literalValue \"42\"^^xsd:integer .\n";
        let mut dag = TermDag::new();
        let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse");
        let node = lower_math_expression(&mut dag, &graph, "https://example.org/lit")
            .expect("typed math:NumberLiteral lowers, not a hard fail");

        // The lowered leaf's key must carry BOTH the lexical form and the datatype IRI — a
        // dropped datatype would collapse a typed number to an untyped constant.
        let key = dag.key(node);
        assert!(key.contains("42"), "lexical form survives: {key}");
        assert!(
            key.contains("XMLSchema#integer"),
            "datatype IRI survives (not silently dropped): {key}"
        );

        // It interns to the SAME node as a by-hand `typed_literal` build through the arena.
        let hand_built = dag.intern_leaf(TermValue::typed_literal(
            "42",
            "http://www.w3.org/2001/XMLSchema#integer",
        ));
        assert_eq!(
            node, hand_built,
            "math:NumberLiteral lowering interns to the SAME node as a by-hand typed literal"
        );
    }
}
