// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Three-consumer lowering into the shared hash-consed [`TermDag`].
//!
//! # One arena, three surfaces
//!
//! [`TermDag`](gmeow_term_arena::engine::TermDag) is the single structured-term arena.
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

#[cfg(test)]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::structural_digest;

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::{Diag, Result};
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::TermValue;

use crate::physical::id::NodeId;
use gmeow_term_arena::engine::TermDag;

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

/// The free-variable policy every `logic:` lowering entry point threads down to
/// [`lower_term_in`]: invoked ONLY when [`resolve_debruijn`] finds no enclosing binder frame
/// for a `Term::Var` occurrence — i.e. exactly the position that used to hard-code
/// `dag.intern_free(..)`. The default policy ([`lower_logic_formula`]/[`lower_logic_term`])
/// reproduces that RIGID `Free`-leaf behavior byte-for-byte; a caller that instead needs an
/// implicitly-universally-quantified variable (a `logic:ReasoningProgram` clause/query has no
/// explicit `Forall` wrapper, so every one of its variables is free from THIS lowering's point
/// of view) supplies a policy that mints/reuses a [`crate::physical::id::MetaId`] metavariable
/// via [`lower_logic_formula_with`]/[`lower_logic_term_with`] instead.
type FreeResolver<'a> = &'a mut dyn FnMut(&mut TermDag, &str) -> Result<NodeId>;

/// Lower a `logic:` [`Formula`] into `dag`, returning its node id, under the DEFAULT
/// free-variable policy: an unbound `Term::Var` interns as a RIGID [`crate::physical`]
/// `Free` leaf (never a metavariable).
///
/// Reproduces exactly the equivalences [`Formula::content_key`] decides:
/// bound-variable alpha-renaming (locally-nameless de-Bruijn), commutative
/// flatten+order-normalization of `And`/`Or`/`Iff`, and ordered `Implies`. A
/// [`Term::SequenceMarker`] is a HARD FAIL (the arena has no variadic-binder node, so a
/// sequence marker cannot be coerced to a single-term occurrence).
pub(crate) fn lower_logic_formula(dag: &mut TermDag, f: &Formula) -> Result<NodeId> {
    let mut free = default_free_resolver;
    lower_logic_formula_with(dag, f, &mut free)
}

/// Lower a `logic:` [`Term`] into `dag` under no enclosing binder and the DEFAULT
/// free-variable policy (a free variable interns as a RIGID `Free` leaf; an IRI/literal is a
/// leaf). A [`Term::SequenceMarker`] is a HARD FAIL.
pub(crate) fn lower_logic_term(dag: &mut TermDag, t: &Term) -> Result<NodeId> {
    let mut free = default_free_resolver;
    lower_logic_term_with(dag, t, &mut free)
}

/// Lower a `logic:` [`Formula`] into `dag` exactly as [`lower_logic_formula`] does, except
/// that a `Term::Var` occurrence with NO enclosing binder frame resolves through the caller's
/// own `free` policy instead of the hard-coded rigid-`Free`-leaf default — the single
/// production seam a `logic:ReasoningProgram` compiler (whose clauses/queries carry no
/// explicit `Forall` and whose variables must therefore mint/reuse a metavariable, not a
/// rigid leaf) uses. The `Bound`/de-Bruijn path is untouched: only the free-variable fallback
/// is policy-driven.
pub(crate) fn lower_logic_formula_with(
    dag: &mut TermDag,
    f: &Formula,
    free: FreeResolver<'_>,
) -> Result<NodeId> {
    let mut env: Vec<Vec<String>> = Vec::new();
    lower_formula_in(dag, f, &mut env, free)
}

/// Lower a `logic:` [`Term`] into `dag` under no enclosing binder, exactly as
/// [`lower_logic_term`] does except that a free `Term::Var` resolves through `free` (see
/// [`lower_logic_formula_with`]).
pub(crate) fn lower_logic_term_with(
    dag: &mut TermDag,
    t: &Term,
    free: FreeResolver<'_>,
) -> Result<NodeId> {
    let env: Vec<Vec<String>> = Vec::new();
    lower_term_in(dag, t, &env, free)
}

/// The default free-variable policy: an unbound `Term::Var` interns as a RIGID `Free` leaf —
/// byte-for-byte what `lower_term_in`'s `Term::Var` arm hard-coded before the policy seam
/// existed.
fn default_free_resolver(dag: &mut TermDag, name: &str) -> Result<NodeId> {
    Ok(dag.intern_free(TermValue::simple_literal(name.to_owned())))
}

fn lower_term_in(
    dag: &mut TermDag,
    term: &Term,
    env: &[Vec<String>],
    free: FreeResolver<'_>,
) -> Result<NodeId> {
    Ok(match term {
        Term::Iri(s) => dag.intern_leaf(TermValue::iri(s.clone())),
        Term::Literal(literal) => dag.intern_leaf(crate::rule_ir::literal_value(literal)),
        Term::Var(name) => match resolve_debruijn(env, name) {
            Some((distance, slot)) => intern_bound_checked(dag, distance, slot)?,
            None => free(dag, name)?,
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
                arg_nodes.push(lower_term_in(dag, a, env, free)?);
            }
            dag.intern_app(op, arg_nodes)
        }
    })
}

fn lower_formula_in(
    dag: &mut TermDag,
    f: &Formula,
    env: &mut Vec<Vec<String>>,
    free: FreeResolver<'_>,
) -> Result<NodeId> {
    Ok(match f {
        Formula::Atom { relation, args } => {
            let op = lower_term_in(dag, relation, env, free)?;
            let mut arg_nodes = Vec::with_capacity(args.len());
            for a in args {
                arg_nodes.push(lower_term_in(dag, a, env, free)?);
            }
            dag.intern_app(op, arg_nodes)
        }
        Formula::Not(b) => {
            let op = dag.intern_leaf(TermValue::iri(canon::NOT));
            let child = lower_formula_in(dag, b, env, free)?;
            dag.intern_app(op, vec![child])
        }
        Formula::And(fs) => lower_commutative(dag, canon::AND, true, fs, env, free)?,
        Formula::Or(fs) => lower_commutative(dag, canon::OR, false, fs, env, free)?,
        Formula::Implies(a, b) => {
            let op = dag.intern_leaf(TermValue::iri(canon::IMPLIES));
            let la = lower_formula_in(dag, a, env, free)?;
            let lb = lower_formula_in(dag, b, env, free)?;
            dag.intern_app(op, vec![la, lb])
        }
        Formula::Iff(a, b) => {
            let op = dag.intern_leaf(TermValue::iri(canon::IFF));
            let mut pair = [
                lower_formula_in(dag, a, env, free)?,
                lower_formula_in(dag, b, env, free)?,
            ];
            // Sort by CONTENT KEY, never NodeId: a `NodeId` is an interning-order artifact
            // (arbitrary across two separate DAGs), while `dag.key(..)` is the same
            // structural fingerprint `ir.rs` sorts the biconditional's operand keys by, so
            // the same commutative formula built in two separate fresh DAGs interns to the
            // same content key regardless of interning order.
            pair.sort_by(|&x, &y| dag.key(x).cmp(dag.key(y)));
            dag.intern_app(op, pair.to_vec())
        }
        Formula::Forall { vars, body } => {
            lower_logic_binder(dag, canon::FORALL, vars, body, env, free)?
        }
        Formula::Exists { vars, body } => {
            lower_logic_binder(dag, canon::EXISTS, vars, body, env, free)?
        }
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
    free: FreeResolver<'_>,
) -> Result<NodeId> {
    let op = dag.intern_leaf(TermValue::iri(op_iri));
    let mut operands: Vec<&Formula> = Vec::new();
    flatten_commutative(is_and, fs, &mut operands);
    let mut nodes = Vec::with_capacity(operands.len());
    for f in operands {
        nodes.push(lower_formula_in(dag, f, env, free)?);
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
    free: FreeResolver<'_>,
) -> Result<NodeId> {
    let op = dag.intern_leaf(TermValue::iri(op_iri));
    let sort = dag.intern_leaf(TermValue::iri(canon::SORT_INDIVIDUAL));
    let sorts = vec![sort; vars.len()];
    env.push(vars.to_vec());
    let body_node = lower_formula_in(dag, body, env, free);
    env.pop();
    let body_node = body_node?;
    Ok(dag.intern_binder(op, sorts, body_node))
}

// ─────────────────────────────────────────────────────────────────────────────
// math: — the RDF-authored application/binding expression vocabulary.
// ─────────────────────────────────────────────────────────────────────────────

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The `math:` namespace root, used to tell a `math:`-authored (but unrecognized) type
/// apart from a foreign/absent type when deciding whether a fallback node is a genuine
/// bare constant operand (see [`lower_math_node_dispatch`]'s trailing branch).
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";
const M_APPLICATION: &str = "https://blackcatinformatics.ca/math/ApplicationExpression";
const M_BINDING: &str = "https://blackcatinformatics.ca/math/BindingExpression";
const M_VARIABLE_EXPRESSION: &str = "https://blackcatinformatics.ca/math/VariableExpression";
const M_FREE_DECLARATION: &str = "https://blackcatinformatics.ca/math/FreeVariableDeclaration";
const M_NUMBER_LITERAL: &str = "https://blackcatinformatics.ca/math/NumberLiteral";
/// A symbol-occurrence leaf ([`math:SymbolReference`](https://blackcatinformatics.ca/math/SymbolReference)):
/// resolves through exactly one `math:hasMathematicalSymbol` edge to a local
/// `math:MathematicalSymbol` per its own class definition. This lowering does not (yet)
/// walk that edge — it interns the reference's own IRI, exactly like any other bare
/// constant leaf — so it is a RECOGNIZED constant-operand type or [`lower_math_node_dispatch`]'s
/// fallback would wrongly reject every committed `math:SymbolReference` leaf (e.g.
/// `slices/grounding/math/examples/reference-ast-act.ttl`'s `ex:leftMatrixRef`).
const M_SYMBOL_REFERENCE: &str = "https://blackcatinformatics.ca/math/SymbolReference";
/// The ABSTRACT expression base ("the abstract root", per the slice's own
/// `MATHEMATICS-EXPRESSIONS.md`). A node typed with it and nothing more concrete has no
/// concrete form for the lowering to walk and no content of its own to key on, so it is
/// `UnrecognizedExpressionType` in an expression position — see
/// [`lower_math_node_dispatch`]'s trailing branch. It stays named here because
/// [`MathGraph::expression_typed_nodes`] must still SEE such a node (it is
/// `math:structuralKey`'s declared domain) in order to report it.
const M_MATHEMATICAL_EXPRESSION: &str =
    "https://blackcatinformatics.ca/math/MathematicalExpression";
/// The edge a `math:SymbolReference` occurrence resolves through to its symbol — the
/// occurrence's ONLY content, and therefore the only thing its structural identity may
/// be keyed on.
const M_HAS_MATHEMATICAL_SYMBOL: &str = "https://blackcatinformatics.ca/math/hasMathematicalSymbol";
const M_OPERATOR: &str = "https://blackcatinformatics.ca/math/operator";
const M_ARGUMENT_SLOT: &str = "https://blackcatinformatics.ca/math/argumentSlot";
const M_SLOT_INDEX: &str = "https://blackcatinformatics.ca/math/slotIndex";
const M_SLOT_EXPRESSION: &str = "https://blackcatinformatics.ca/math/slotExpression";
const M_BOUND_VARIABLE: &str = "https://blackcatinformatics.ca/math/boundVariable";
const M_VARIABLE_OCCURRENCE: &str = "https://blackcatinformatics.ca/math/variableOccurrence";
const M_DECLARED_VARIABLE: &str = "https://blackcatinformatics.ca/math/declaredVariable";
const M_DOMAIN: &str = "https://blackcatinformatics.ca/math/domain";
const M_LITERAL_VALUE: &str = "https://blackcatinformatics.ca/math/literalValue";

/// The maximum supported `math:` expression-graph lowering recursion depth. The
/// mutual recursion `lower_math_node → lower_math_application/lower_math_binding →
/// lower_math_node` walks an AUTHORED RDF graph with no acyclicity guarantee from the
/// type system, so an unbounded depth is a stack-overflow hazard on a pathologically
/// deep (or, absent the cycle guard below, cyclic) authoring. A generous but finite
/// bound turns that hazard into a typed, catchable [`MathLoweringError`].
pub(crate) const MAX_MATH_EXPRESSION_DEPTH: usize = 500;

/// The `math:` failure-class IRIs [`MathLoweringError::failure_class`] decides. Every one
/// of the ten is authored in `slices/grounding/math/module.ttl` as an `owl:Class`
/// `logic:subClassOf math:MathConformanceFailure`, with the full annotation coat
/// (`rdfs:label`, `skos:definition`, `gmeow:useWhen`, `gmeow:avoidWhen`, `gmeow:howToUse`,
/// `skos:example`) and a row in `slices/grounding/math/design/MATHEMATICS-CONFORMANCE.md`.
///
/// `CYCLIC_EXPRESSION_GRAPH`, `EXPRESSION_DEPTH_EXCEEDED`, `UNRECOGNIZED_EXPRESSION_TYPE`,
/// and `NUMBER_LITERAL_MISSING_VALUE` are the four classes with
/// NO SHACL/OWL-derived twin — a cycle through the `math:slotExpression` graph, a
/// too-deep recursion, an unrecognized node typing, and a literal carrier with nothing to
/// carry are all decisions the lowering makes while walking, not flat relational joins the
/// SHACL/Datalog fragment can express, so they carry no `gmeow:enforcesFailureClass`
/// triple and are reachable ONLY through this Rust decision (the SAME architectural shape
/// as `math:StructuralKeyDrift` / `math:SurfaceLeakInNormalForm` /
/// `math:StructuralKeyOnRejectedExpression` in `crate::math_expression`). The other six
/// buckets are additionally SHACL-Core/SHACL-SPARQL-enforced.
mod failure_class {
    pub(super) const MALFORMED_ARGUMENT_SLOT: &str =
        "https://blackcatinformatics.ca/math/MalformedArgumentSlot";
    pub(super) const NON_CONTIGUOUS_ARGUMENT_SLOTS: &str =
        "https://blackcatinformatics.ca/math/NonContiguousArgumentSlots";
    pub(super) const DUPLICATE_ARGUMENT_SLOT_INDEX: &str =
        "https://blackcatinformatics.ca/math/DuplicateArgumentSlotIndex";
    pub(super) const APPLICATION_OPERATOR_CARDINALITY: &str =
        "https://blackcatinformatics.ca/math/ApplicationOperatorCardinality";
    pub(super) const MALFORMED_BINDING_EXPRESSION: &str =
        "https://blackcatinformatics.ca/math/MalformedBindingExpression";
    pub(super) const UNSCOPED_VARIABLE_OCCURRENCE: &str =
        "https://blackcatinformatics.ca/math/UnscopedVariableOccurrence";
    /// A node in an expression position carrying an unrecognized `math:` type.
    pub(crate) const UNRECOGNIZED_EXPRESSION_TYPE: &str =
        "https://blackcatinformatics.ca/math/UnrecognizedExpressionType";
    /// A `math:NumberLiteral` with no `math:literalValue`.
    pub(crate) const NUMBER_LITERAL_MISSING_VALUE: &str =
        "https://blackcatinformatics.ca/math/NumberLiteralMissingValue";
    pub(super) const CYCLIC_EXPRESSION_GRAPH: &str =
        "https://blackcatinformatics.ca/math/CyclicExpressionGraph";
    pub(super) const EXPRESSION_DEPTH_EXCEEDED: &str =
        "https://blackcatinformatics.ca/math/ExpressionDepthExceeded";
}

/// The typed rejection algebra of the `math:` expression-graph lowering.
///
/// Every math-specific rejection site raises exactly one of these variants (never a
/// generic string-only `Diag`), so a caller (a later reasoned-graph gate) can match on
/// the variant/fields directly instead of substring-matching a message. The one
/// exception is a Turtle PARSE failure (`MathGraph::from_turtle`): that is not a
/// conformance failure of an authored `math:` expression — it means there is no graph
/// to check at all — so it stays a plain [`Diag`], never a member of this enum.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MathLoweringError {
    /// A `math:NumberLiteral` node has no `math:literalValue`.
    NumberLiteralMissingValue { node: String },
    /// An expression node has no recognized `math:` expression type and is not a bare
    /// IRI constant: either it is a blank node (which never has an identity outside this
    /// graph to serve as a bare constant), or it is a named node carrying one or more
    /// `math:` types — `types` — none of which is a recognized expression type
    /// (`math:ApplicationExpression` / `math:BindingExpression` /
    /// `math:VariableExpression` / `math:NumberLiteral`) or the recognized
    /// constant-operand type `math:SymbolReference`. A NODE WITH NO `math:` TYPE AT ALL
    /// is not this variant — it is accepted as a bare external constant.
    UnrecognizedExpressionType { node: String, types: Vec<String> },
    /// A `math:ArgumentSlot` has no `math:slotIndex`.
    ArgumentSlotMissingIndex { slot: String },
    /// A `math:ArgumentSlot` carries more than one `math:slotIndex` value.
    ArgumentSlotMultipleIndexes { slot: String, count: usize },
    /// A `math:ArgumentSlot`'s `math:slotIndex` lexical form is not a valid integer.
    ArgumentSlotIndexNotInteger { slot: String, lexical: String },
    /// A `math:ArgumentSlot` has no `math:slotExpression`.
    ArgumentSlotMissingExpression { slot: String },
    /// A node's `math:argumentSlot` indexes have a gap in the zero-based sequence.
    NonContiguousArgumentSlots {
        node: String,
        index: i128,
        expected_position: usize,
    },
    /// A node's `math:argumentSlot` indexes carry the same index twice.
    DuplicateArgumentSlotIndex { node: String, index: i128 },
    /// A `math:ArgumentSlot`'s `math:slotIndex` is negative.
    NegativeArgumentSlotIndex {
        node: String,
        slot: String,
        index: i128,
    },
    /// A `math:ApplicationExpression` has no `math:operator`.
    ApplicationMissingOperator { node: String },
    /// A `math:ApplicationExpression` carries more than one `math:operator` value.
    ApplicationMultipleOperators { node: String, count: usize },
    /// A `math:BindingExpression` has no `math:operator`.
    BindingMissingOperator { node: String },
    /// A `math:BindingExpression` carries more than one `math:operator` value.
    BindingMultipleOperators { node: String, count: usize },
    /// A `math:BindingExpression` has no `math:boundVariable`.
    BindingMissingBoundVariable { node: String },
    /// A `math:BindingExpression` with NO `math:argumentSlot` at all: a binder that binds its
    /// variable over nothing. The slice says a binder names "its body through indexed
    /// math:argumentSlot cells" — indexed and plural, but never empty.
    BindingMissingBody { node: String },
    /// A `math:BindingExpression` carries more than one `math:boundVariable` value.
    BindingMultipleBoundVariables { node: String, count: usize },
    /// A `math:VariableExpression` has no `math:variableOccurrence`.
    VariableExpressionMissingOccurrence { node: String },
    /// A `math:VariableExpression` carries more than one `math:variableOccurrence`
    /// value.
    VariableExpressionMultipleOccurrences { node: String, count: usize },
    /// A `math:VariableOccurrence` has no `math:declaredVariable`.
    OccurrenceMissingDeclaredVariable { occurrence: String },
    /// A `math:VariableOccurrence` carries more than one `math:declaredVariable` value.
    OccurrenceMultipleDeclaredVariables { occurrence: String, count: usize },
    /// A `math:VariableOccurrence` resolves to a declaration that is neither bound by
    /// an enclosing binder nor a `math:FreeVariableDeclaration`.
    UnscopedOccurrence {
        occurrence: String,
        declaration: String,
    },
    /// A node is reached again while still being lowered — the `math:slotExpression`
    /// graph contains a cycle through it.
    CyclicExpressionGraph { node: String },
    /// Lowering recursed past the configured [depth limit](crate::math_expression::analysis::depth_limit).
    ExpressionDepthExceeded { node: String, depth: usize },
}

impl std::fmt::Display for MathLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NumberLiteralMissingValue { node } => {
                write!(f, "math:NumberLiteral {node} missing math:literalValue")
            }
            Self::UnrecognizedExpressionType { node, types } if node.starts_with("_:") => write!(
                f,
                "math expression blank node {node} has no recognized expression type \
                 (math:ApplicationExpression / math:BindingExpression / \
                 math:VariableExpression / math:NumberLiteral); types found: {types:?} — a \
                 blank node never qualifies as a bare constant operand (it has no identity \
                 outside this graph)"
            ),
            Self::UnrecognizedExpressionType { node, types } => write!(
                f,
                "math expression node {node} carries math: type(s) {types:?}, none of which \
                 is a recognized expression type (math:ApplicationExpression / \
                 math:BindingExpression / math:VariableExpression / math:NumberLiteral) or \
                 the recognized constant-operand type math:SymbolReference — and it is not a \
                 bare untyped IRI constant"
            ),
            Self::ArgumentSlotMissingIndex { slot } => {
                write!(f, "math:ArgumentSlot {slot} missing math:slotIndex")
            }
            Self::ArgumentSlotMultipleIndexes { slot, count } => write!(
                f,
                "math:ArgumentSlot {slot} carries {count} math:slotIndex values; exactly one \
                 is required"
            ),
            Self::ArgumentSlotIndexNotInteger { slot, lexical } => {
                write!(f, "math:slotIndex {lexical:?} on {slot} is not an integer")
            }
            Self::ArgumentSlotMissingExpression { slot } => {
                write!(f, "math:ArgumentSlot {slot} missing math:slotExpression")
            }
            Self::NonContiguousArgumentSlots {
                node,
                index,
                expected_position,
            } => write!(
                f,
                "math:argumentSlot indexes of {node} must be zero-based and contiguous with \
                 no gaps; got index {index} at ordered position {expected_position}"
            ),
            Self::DuplicateArgumentSlotIndex { node, index } => write!(
                f,
                "math:argumentSlot indexes of {node} contain a duplicate index {index}"
            ),
            Self::NegativeArgumentSlotIndex { node, slot, index } => write!(
                f,
                "math:ArgumentSlot {slot} of {node} declares a negative math:slotIndex \
                 {index}; indexes must be non-negative"
            ),
            Self::ApplicationMissingOperator { node } => {
                write!(f, "math:ApplicationExpression {node} missing math:operator")
            }
            Self::ApplicationMultipleOperators { node, count } => write!(
                f,
                "math:ApplicationExpression {node} carries {count} math:operator values; \
                 exactly one is required"
            ),
            Self::BindingMissingOperator { node } => {
                write!(f, "math:BindingExpression {node} missing math:operator")
            }
            Self::BindingMultipleOperators { node, count } => write!(
                f,
                "math:BindingExpression {node} carries {count} math:operator values; exactly \
                 one is required"
            ),
            Self::BindingMissingBody { node } => write!(
                f,
                "math:BindingExpression {node} carries no math:argumentSlot; a binder binds \
                 its variable over a body and must name at least one indexed operand cell"
            ),
            Self::BindingMissingBoundVariable { node } => write!(
                f,
                "math:BindingExpression {node} missing math:boundVariable"
            ),
            Self::BindingMultipleBoundVariables { node, count } => write!(
                f,
                "math:BindingExpression {node} carries {count} math:boundVariable values; \
                 exactly one is required"
            ),
            Self::VariableExpressionMissingOccurrence { node } => write!(
                f,
                "math:VariableExpression {node} missing math:variableOccurrence"
            ),
            Self::VariableExpressionMultipleOccurrences { node, count } => write!(
                f,
                "math:VariableExpression {node} carries {count} math:variableOccurrence \
                 values; exactly one is required"
            ),
            Self::OccurrenceMissingDeclaredVariable { occurrence } => write!(
                f,
                "math:VariableOccurrence {occurrence} missing math:declaredVariable"
            ),
            Self::OccurrenceMultipleDeclaredVariables { occurrence, count } => write!(
                f,
                "math:VariableOccurrence {occurrence} carries {count} math:declaredVariable \
                 values; exactly one is required"
            ),
            Self::UnscopedOccurrence {
                occurrence,
                declaration,
            } => write!(
                f,
                "math:VariableOccurrence {occurrence} resolves to declaration {declaration}, \
                 which is neither bound by an enclosing math:BindingExpression nor a \
                 math:FreeVariableDeclaration (unscoped occurrence)"
            ),
            Self::CyclicExpressionGraph { node } => write!(
                f,
                "math expression node {node} is reached while already being lowered — the \
                 math:slotExpression graph contains a cycle through this node"
            ),
            Self::ExpressionDepthExceeded { node, depth } => write!(
                f,
                "math expression node {node} exceeds the maximum supported lowering \
                 recursion depth ({depth} > {MAX_MATH_EXPRESSION_DEPTH})"
            ),
        }
    }
}

/// A lowering rejection is a real error, not just something printable: a caller that works in
/// `gmeow_errors::Result` propagates it with `?`, which needs the `Diag` conversion this
/// unlocks. `Display` already carries the whole message,
/// so there is no source chain to expose.
impl std::error::Error for MathLoweringError {}

impl MathLoweringError {
    /// The full `math:` failure-class IRI this rejection decides. Exhaustive with NO
    /// wildcard arm: a variant added later without a class fails to compile, so the
    /// rejection algebra and the failure-class mapping can never silently drift apart.
    pub fn failure_class(&self) -> &'static str {
        match self {
            Self::NumberLiteralMissingValue { .. } => failure_class::NUMBER_LITERAL_MISSING_VALUE,
            Self::UnrecognizedExpressionType { .. } => failure_class::UNRECOGNIZED_EXPRESSION_TYPE,
            Self::ArgumentSlotMissingIndex { .. }
            | Self::ArgumentSlotMultipleIndexes { .. }
            | Self::ArgumentSlotIndexNotInteger { .. }
            | Self::ArgumentSlotMissingExpression { .. }
            | Self::NegativeArgumentSlotIndex { .. } => failure_class::MALFORMED_ARGUMENT_SLOT,
            Self::NonContiguousArgumentSlots { .. } => failure_class::NON_CONTIGUOUS_ARGUMENT_SLOTS,
            Self::DuplicateArgumentSlotIndex { .. } => failure_class::DUPLICATE_ARGUMENT_SLOT_INDEX,
            Self::ApplicationMissingOperator { .. } | Self::ApplicationMultipleOperators { .. } => {
                failure_class::APPLICATION_OPERATOR_CARDINALITY
            }
            Self::BindingMissingOperator { .. }
            | Self::BindingMultipleOperators { .. }
            | Self::BindingMissingBoundVariable { .. }
            | Self::BindingMissingBody { .. }
            | Self::BindingMultipleBoundVariables { .. } => {
                failure_class::MALFORMED_BINDING_EXPRESSION
            }
            Self::VariableExpressionMissingOccurrence { .. }
            | Self::VariableExpressionMultipleOccurrences { .. }
            | Self::OccurrenceMissingDeclaredVariable { .. }
            | Self::OccurrenceMultipleDeclaredVariables { .. }
            | Self::UnscopedOccurrence { .. } => failure_class::UNSCOPED_VARIABLE_OCCURRENCE,
            Self::CyclicExpressionGraph { .. } => failure_class::CYCLIC_EXPRESSION_GRAPH,
            Self::ExpressionDepthExceeded { .. } => failure_class::EXPRESSION_DEPTH_EXCEEDED,
        }
    }
}

/// The `math:` lowering's own result alias: math-specific rejections are the TYPED
/// [`MathLoweringError`] algebra, never the shared string-only [`Diag`].
pub(crate) type MathResult<T> = std::result::Result<T, MathLoweringError>;

/// A read-only subject → predicate → objects index over the default graph of a parsed
/// `math:` expression dataset — the substrate the `math:` lowering walks.
///
/// The `math:` expression tree has no typed Rust AST: it is RDF, so the lowering reads it
/// straight out of a [`gmeow_math::TripleIndex`] (parsed from Turtle here, identical to
/// how a shipped `.gts` bundle would present it, and shared with the `math:` dimension
/// gate's own graph substrate). Blank nodes are keyed `_:`-prefixed by label (unique
/// within one parsed default graph), so both IRI-named and blank-node expression nodes
/// resolve.
pub(crate) struct MathGraph {
    index: gmeow_math::TripleIndex,
}

impl MathGraph {
    /// Build a [`MathGraph`] from a Turtle document of the `math:` expression
    /// vocabulary. A parse failure is NOT a conformance failure of an authored
    /// expression — there is no graph to check — so it stays a plain [`Diag`], never a
    /// [`MathLoweringError`].
    pub(crate) fn from_turtle(turtle: &[u8]) -> Result<Self> {
        let dataset = purrdf::parse_dataset(turtle, "text/turtle", None)
            .map_err(|err| ir_err(format!("cannot parse math expression Turtle: {err}")))?;
        Ok(Self::from_dataset(&dataset))
    }

    /// Build a [`MathGraph`] over an already-parsed dataset (e.g. the native reasoned
    /// graph) — the seam [`math_expression_structural_keys`] uses, since its caller
    /// already holds a parsed [`purrdf::RdfDataset`] and re-parsing would be a
    /// redundant second parse of the same bytes.
    pub(crate) fn from_dataset(dataset: &purrdf::RdfDataset) -> Self {
        Self {
            index: gmeow_math::index_dataset(dataset),
        }
    }

    /// The first IRI/blank object of `(subject, predicate, ?)`, if any.
    fn first_ref(&self, subject: &str, predicate: &str) -> Option<String> {
        gmeow_math::first_iri(&self.index, subject, predicate)
    }

    /// Every IRI/blank object of `(subject, predicate, ?)`, in index order.
    fn refs(&self, subject: &str, predicate: &str) -> Vec<String> {
        gmeow_math::all_iris(&self.index, subject, predicate)
    }

    /// Every literal lexical form of `(subject, predicate, ?)`, in index order —
    /// datatype/language dropped deliberately (only used to COUNT/read a
    /// `math:slotIndex`, which is always a plain integer lexical).
    fn all_lit(&self, subject: &str, predicate: &str) -> Vec<String> {
        gmeow_math::all_literals_typed(&self.index, subject, predicate)
            .into_iter()
            .map(|(lexical, _, _)| lexical.to_owned())
            .collect()
    }

    /// The first literal object of `(subject, predicate, ?)`, if any, as
    /// `(lexical, datatype, language)` — full fidelity, never discarding the datatype/
    /// language a `math:NumberLiteral`'s `math:literalValue` carries.
    fn first_lit_typed(
        &self,
        subject: &str,
        predicate: &str,
    ) -> Option<(&str, &str, Option<&str>)> {
        gmeow_math::first_literal_typed(&self.index, subject, predicate)
    }

    /// The `rdf:type` IRIs of `subject`.
    fn types(&self, subject: &str) -> Vec<String> {
        self.refs(subject, RDF_TYPE)
    }

    /// Whether `subject` carries `rdf:type` `class`.
    fn has_type(&self, subject: &str, class: &str) -> bool {
        gmeow_math::has_type(&self.index, subject, class)
    }

    /// The IRIs of EVERY node typed `math:ApplicationExpression` / `math:BindingExpression`
    /// / `math:VariableExpression` / `math:NumberLiteral` in this graph, referenced or
    /// not — the full candidate population [`expression_roots`](Self::expression_roots)
    /// filters down to the unreferenced ones, and
    /// [`math_expression_structural_keys`] walks again (against the roots' combined
    /// reachability) to seed the rootless nodes a purely referenced-based filter can never
    /// find: a fully closed cyclic component (every member typed here AND referenced by
    /// another member of the SAME component) has no unreferenced member at all.
    fn expression_typed_nodes(&self) -> BTreeSet<String> {
        gmeow_math::subjects(&self.index)
            .filter(|subject| {
                self.has_type(subject, M_APPLICATION)
                    || self.has_type(subject, M_BINDING)
                    || self.has_type(subject, M_VARIABLE_EXPRESSION)
                    || self.has_type(subject, M_NUMBER_LITERAL)
                    // The abstract base too. It is `math:structuralKey`'s DECLARED domain, and
                    // leaving it out of the root population meant an authored key on such a node
                    // reached no digest to compare against: `check_structural_key_drift` found no
                    // entry and skipped it, so a hand-guessed digest — the exact thing the
                    // property's own `gmeow:avoidWhen` forbids — passed the gate in silence.
                    // Every such node is REJECTED by the lowering (it names no concrete form, so
                    // it has no structural identity), and being in this population is what turns
                    // that rejection into a reported `math:UnrecognizedExpressionType` instead of
                    // an unexamined node.
                    || self.has_type(subject, M_MATHEMATICAL_EXPRESSION)
            })
            .cloned()
            .collect()
    }

    /// The IRIs of every node this graph's `math:slotExpression` edges name as SOME
    /// node's operand or binder body (an `math:ArgumentSlot`'s `math:slotExpression`
    /// object) — the "has an incoming reference" half of
    /// [`expression_roots`](Self::expression_roots)'s root test.
    fn slot_expression_referenced_nodes(&self) -> BTreeSet<String> {
        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for subject in gmeow_math::subjects(&self.index) {
            for object in self.refs(subject, M_SLOT_EXPRESSION) {
                referenced.insert(object);
            }
        }
        referenced
    }

    /// The IRIs of every "root" expression node in this graph: a node typed
    /// `math:ApplicationExpression` / `math:BindingExpression` / `math:VariableExpression`
    /// / `math:NumberLiteral` that is not itself referenced as any other node's
    /// `math:slotExpression` object (an operand or a binder's body — the ONE edge a
    /// child expression node is reached through). [`math_expression_structural_keys`]
    /// lowers each independently, so one bad root's error never blinds another root's
    /// result.
    ///
    /// This is NOT the full expression-typed population: a node in a fully closed cyclic
    /// component (every member is BOTH candidate-typed and referenced by another member
    /// of the SAME component) has no unreferenced member and is invisible here by
    /// construction — [`math_expression_structural_keys`] seeds it separately from
    /// [`expression_typed_nodes`](Self::expression_typed_nodes).
    fn expression_roots(&self) -> BTreeSet<String> {
        let referenced = self.slot_expression_referenced_nodes();
        let mut candidates = self.expression_typed_nodes();
        candidates.retain(|node| !referenced.contains(node));
        candidates
    }
}

/// The IRIs of every node transitively reachable from `start` via the
/// `math:argumentSlot` → `math:slotExpression` edge (INCLUDING `start` itself) — the
/// structural edge [`lower_math_node`]'s cycle guard walks. Used purely to compute
/// coverage (which nodes a root's lowering attempt already visits), so a malformed slot
/// family never aborts the walk early: unlike the real lowering, a missing/duplicate
/// `math:slotIndex` is simply skipped rather than raised, and a cycle terminates the walk
/// through the same insert-returns-false check the real cycle guard uses, never looping
/// forever.
fn reachable_expression_nodes(graph: &MathGraph, start: &str) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut stack = vec![start.to_owned()];
    while let Some(node) = stack.pop() {
        if !seen.insert(node.clone()) {
            continue;
        }
        for slot in graph.refs(&node, M_ARGUMENT_SLOT) {
            if let Some(child) = graph.first_ref(&slot, M_SLOT_EXPRESSION) {
                stack.push(child);
            }
        }
    }
    seen
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
///
/// Guarded against a cyclic or pathologically deep `math:slotExpression` graph: a node
/// reached while it is still being lowered raises [`MathLoweringError::CyclicExpressionGraph`],
/// and a recursion depth past [`MAX_MATH_EXPRESSION_DEPTH`] raises
/// [`MathLoweringError::ExpressionDepthExceeded`] — never an unbounded stack dive.
pub(crate) fn lower_math_expression(
    dag: &mut TermDag,
    graph: &MathGraph,
    root: &str,
) -> MathResult<NodeId> {
    let mut env: Vec<Vec<String>> = Vec::new();
    let mut visiting: BTreeSet<String> = BTreeSet::new();
    lower_math_node(dag, graph, root, &mut env, &mut visiting, 0)
}

/// Compute the structural digest ([`arena_structural_key`]) of every "root" `math:`
/// expression in `ds` ([`MathGraph::expression_roots`]) — the seam a later reasoned-
/// graph gate calls to derive a content-stable α-equivalence identity per authored
/// expression. Each root is lowered independently (a fresh [`TermDag`] and a fresh
/// recursion-guard state per root), so ONE root's rejection is recorded against ONLY
/// that root's entry — it never blinds any other root's `Ok` result.
pub(crate) fn math_expression_structural_keys(
    ds: &purrdf::RdfDataset,
) -> BTreeMap<String, MathResult<String>> {
    let graph = MathGraph::from_dataset(ds);
    let mut out = BTreeMap::new();
    // `visited` accumulates every node any processed root's expression graph already
    // covers, so the rootless pass below seeds a component AT MOST once — never re-
    // reports the same closed cycle once via one of its own members.
    let mut visited: BTreeSet<String> = BTreeSet::new();
    for root in graph.expression_roots() {
        visited.extend(reachable_expression_nodes(&graph, &root));
        out.insert(root.clone(), arena_structural_key(&graph, &root));
    }
    // A fully closed cyclic component (every member typed as a `math:` expression AND
    // referenced by another member of the SAME component through `math:slotExpression`)
    // has no member `expression_roots()` can find — its root-seeded traversal above
    // never touches it, `lower_math_expression` never runs over it, and
    // `math:CyclicExpressionGraph` never fires. Seed any STILL-unvisited expression-typed
    // node (sorted — `expression_typed_nodes` returns a `BTreeSet` — so which member
    // represents the component is deterministic) as an orphan root, so every closed
    // component is reached and its cycle guard actually fires at least once.
    for node in graph.expression_typed_nodes() {
        if visited.contains(&node) {
            continue;
        }
        visited.extend(reachable_expression_nodes(&graph, &node));
        out.insert(node.clone(), arena_structural_key(&graph, &node));
    }
    out
}

/// Domain-separation tag for [`fold_content_key`]'s framed `blake3` hash — mirrors the
/// length-prefixed, domain-tagged framing `crates/errors/src/ledger.rs`'s `feed` uses for
/// its own content-address fingerprints (never a bare-concatenation hash, so a field-
/// boundary shift can never collide two structurally-distinct keys).
const STRUCTURAL_KEY_TAG: &[u8] = b"gmeow-math-structural-key-v1";

/// Length-prefixed, domain-separated field feed (mirrors `ledger.rs`'s `feed`): a length
/// prefix before both the tag and the payload makes a delimiter-injection collision
/// between the two impossible, whatever bytes either carries.
fn feed_structural(hasher: &mut blake3::Hasher, tag: &[u8], bytes: &[u8]) {
    hasher.update(&(tag.len() as u64).to_le_bytes());
    hasher.update(tag);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// Fold an arena content key into the published fixed-width digest.
///
/// Split out so the [`TermDag`]-facing (test-only) and
/// [`crate::term_arena::TermArena`]-facing routes cannot drift: both end here, over the same
/// bytes ([`gmeow_term_arena::Arena::key`] returns `dag.key` verbatim).
fn fold_content_key(content_key: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    feed_structural(&mut hasher, STRUCTURAL_KEY_TAG, content_key.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Lower one root through the arena seam ([`crate::term_arena::intern_math_root`]) and fold the
/// [`gmeow_term_arena::ContentKey`] the arena returns into the published digest.
///
/// The shipped structural key is therefore computed by the very seam a downstream consumer
/// of the structured-term arena calls — not by a parallel in-house lowering that merely
/// happens to agree with it. A fresh arena per root preserves the isolation the caller
/// documents: one root's typed rejection can never blind another root's `Ok`.
pub(crate) fn arena_structural_key(graph: &MathGraph, root: &str) -> MathResult<String> {
    let mut arena = crate::term_arena::TermArena::new();
    crate::term_arena::intern_math_root(&mut arena, graph, root)
        .map(|(_, key)| fold_content_key(key.as_str()))
}

/// Namespace segment under which [`alpha_class_iri_for_digest`] mints
/// one content-addressed IRI per distinct [`arena_structural_key`] — the individual every
/// α-equivalent expression's authored `math:alphaEquivalenceClass` edge
/// (`slices/grounding/math/module.ttl`) resolves to. Minted directly under the slice's
/// OWN `math:` namespace (never the generic n-ary-reifier convention: `mint_nary_reifier`
/// types its reifier as `logic:instanceOf(R, relation)`, a reified-TUPLE typing that is
/// not what an α-equivalence-class individual is), mirroring how
/// `crates/logic/src/entail.rs`'s `Minter::witness`/`complement` mint fresh individuals
/// directly from a `blake3` digest under their own reserved namespace. Kept textually
/// distinct from the `math:alphaEquivalenceClass` PROPERTY IRI itself (that IRI has no
/// trailing path segment) so a class individual and the property that names it are never
/// visually conflated.
const ALPHA_CLASS_NS: &str = "https://blackcatinformatics.ca/math/alphaClass/";

/// Mint the content-stable IRI naming the α-equivalence class identified by an
/// already-computed [`arena_structural_key`] — the entry point
/// [`crate::math_expression::check_math_expression_findings`] uses, since
/// [`math_expression_structural_keys`] already folds each root down to its digest
/// string before this is ever called (no [`TermDag`]/[`NodeId`] survives that far).
/// Two equal digests (α-equivalent expressions, by [`arena_structural_key`]'s own
/// contract) mint the IDENTICAL IRI — the whole point: a consumer of the reasoned
/// graph can JOIN on it rather than compare opaque digest literals.
pub(crate) fn alpha_class_iri_for_digest(digest: &str) -> String {
    format!("{ALPHA_CLASS_NS}{digest}")
}

fn lower_math_node(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &mut Vec<Vec<String>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
) -> MathResult<NodeId> {
    if depth > MAX_MATH_EXPRESSION_DEPTH {
        return Err(MathLoweringError::ExpressionDepthExceeded {
            node: node.to_owned(),
            depth,
        });
    }
    // Insert on entry, remove on exit (mirrors how `env` is pushed/popped around a
    // binder's body): a node already present means an ANCESTOR of this call is still
    // being lowered — the `math:slotExpression` graph closes a cycle through `node`.
    if !visiting.insert(node.to_owned()) {
        return Err(MathLoweringError::CyclicExpressionGraph {
            node: node.to_owned(),
        });
    }
    let result = lower_math_node_dispatch(dag, graph, node, env, visiting, depth);
    visiting.remove(node);
    result
}

fn lower_math_node_dispatch(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &mut Vec<Vec<String>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
) -> MathResult<NodeId> {
    let types = graph.types(node);
    if types.iter().any(|t| t == M_APPLICATION) {
        lower_math_application(dag, graph, node, env, visiting, depth)
    } else if types.iter().any(|t| t == M_BINDING) {
        lower_math_binding(dag, graph, node, env, visiting, depth)
    } else if types.iter().any(|t| t == M_VARIABLE_EXPRESSION) {
        lower_math_variable(dag, graph, node, env)
    } else if types.iter().any(|t| t == M_NUMBER_LITERAL) {
        // `math:literalValue` carries the number in EITHER of the two idioms this slice
        // authors: an RDF literal (`"42"^^xsd:integer`) or a reference to a number
        // INDIVIDUAL (`math:RealNumber` with `math:inNumberSystem`/`math:isExact`), which is
        // what the shipped closed-form and learning examples use. A literal keeps its
        // datatype/language into the interned leaf (a bare `TermValue::iri` would silently
        // coerce a typed number to an untyped constant); an individual is interned on its own
        // IRI, which is the only content it has. Only a literalValue-less node is rejected —
        // demanding the literal form alone reported the slice's own conforming examples as
        // "missing" the value they plainly carry.
        if let Some((lexical, datatype, language)) = graph.first_lit_typed(node, M_LITERAL_VALUE) {
            let tv = match language {
                Some(lang) => TermValue::lang_literal(lexical.to_owned(), lang),
                None => TermValue::typed_literal(lexical.to_owned(), datatype.to_owned()),
            };
            return Ok(dag.intern_leaf(tv));
        }
        let individual = graph.first_ref(node, M_LITERAL_VALUE).ok_or_else(|| {
            MathLoweringError::NumberLiteralMissingValue {
                node: node.to_owned(),
            }
        })?;
        Ok(dag.intern_leaf(TermValue::iri(individual)))
    } else {
        // Neither a blank node nor a named node carrying one of the four expression
        // types dispatched above. A leaf is accepted here ONLY when `node` is POSITIVELY
        // a constant operand:
        //   - it carries NO `math:`-namespaced type at all (a bare external constant/
        //     individual referenced from an operator or symbol position — e.g. an
        //     arithmetic-operator IRI filling `math:operator`, or an untyped external
        //     constant such as a Wikidata-anchored individual), or
        //   - it carries the recognized `math:SymbolReference` constant-operand type (a
        //     symbol-occurrence leaf, interned on the SYMBOL its
        //     `math:hasMathematicalSymbol` edge resolves to — the occurrence wrapper's own
        //     IRI is not content, and keying on it would make the digest a label).
        //
        // A blank node NEVER qualifies (it has no identity outside this graph to serve as
        // a bare constant), and a named node carrying one or more `math:` types NONE of
        // which is recognized — a typo'd class, a `math:MathematicalStatement`, a bare
        // `math:VariableOccurrence` used where an expression belongs, the ABSTRACT
        // `math:MathematicalExpression` base with no concrete form beneath it, ... — is a
        // HARD FAIL rather than a silently-degraded opaque leaf: letting an ill-typed AST
        // through here would mean `math:StructuralKeyOnRejectedExpression` never fires
        // and `math:StructuralKeyDrift` compares a declared key against a digest computed
        // over garbage.
        let math_types: Vec<&String> = types.iter().filter(|t| t.starts_with(MATH_NS)).collect();
        // `math:MathematicalExpression` alone — the ABSTRACT base, with no concrete form
        // beneath it — is NOT a constant operand. It is an AST node of this language (that is
        // what the typing asserts) whose form the author declined to give, and an AST node's
        // own IRI is never content: an `ApplicationExpression`'s subject IRI does not enter the
        // digest, and neither may this one. Interning it on its IRI is the SAME defect the
        // `math:SymbolReference` branch below names — it makes the digest a LABEL, so two
        // independently authored copies of one expression over undecomposed operands never
        // intern to one key and never share a `math:AlphaEquivalenceClass`. Interning it on a
        // shared opaque constant instead is no better: every undecomposed operand would then be
        // interchangeable, so `App(op, [a, b])` and `App(op, [b, a])` over two DIFFERENT named
        // operands would collapse into one class, and the key would identify expressions the
        // author distinguished.
        //
        // Neither reading is a content key, so the abstract base carries no structural identity
        // at all and is `UnrecognizedExpressionType` — with or without structured children.
        // Structure present with no concrete form to interpret it is uninterpretable; structure
        // ABSENT with no concrete form is equally so, and only fails more quietly. The slice
        // already ships the honest way to name an operand without decomposing it: a
        // `math:SymbolReference` to a `math:MathematicalSymbol`, whose identity is the SYMBOL —
        // real content, shared by every author who names the same symbol.
        let is_constant_operand = !node.starts_with("_:")
            && (math_types.is_empty()
                || math_types.iter().any(|t| t.as_str() == M_SYMBOL_REFERENCE));
        if is_constant_operand {
            // A `math:SymbolReference` is an OCCURRENCE wrapper: its identity is the symbol it
            // resolves to, never its own node IRI. Interning the wrapper made the structural
            // digest a LABEL rather than a content key — two independently authored copies of
            // the same expression over the same symbols produced different digests, so they
            // never interned to one key and never shared a math:AlphaEquivalenceClass. The
            // slice says as much: a reference occurrence "has exactly one local symbol
            // identity", and `math:UnresolvedSymbolReference` is the failure for zero, many,
            // or off-class. So walk the edge, and HARD FAIL where that class says to.
            if math_types.iter().any(|t| t.as_str() == M_SYMBOL_REFERENCE) {
                let symbols = graph.refs(node, M_HAS_MATHEMATICAL_SYMBOL);
                return match symbols.as_slice() {
                    [symbol] => Ok(dag.intern_leaf(TermValue::iri(symbol.clone()))),
                    _ => Err(MathLoweringError::UnrecognizedExpressionType {
                        node: node.to_owned(),
                        types: types.clone(),
                    }),
                };
            }
            Ok(dag.intern_leaf(TermValue::iri(node.to_owned())))
        } else {
            Err(MathLoweringError::UnrecognizedExpressionType {
                node: node.to_owned(),
                types,
            })
        }
    }
}

/// Collect a node's `math:argumentSlot` slot expressions in `math:slotIndex` order,
/// HARD-FAILING unless the indexes are non-negative, zero-based, contiguous, and
/// duplicate-free — each distinctly typed ([`MathLoweringError::NegativeArgumentSlotIndex`],
/// [`MathLoweringError::DuplicateArgumentSlotIndex`],
/// [`MathLoweringError::NonContiguousArgumentSlots`]), never conflated into one message.
fn collect_slots(graph: &MathGraph, node: &str) -> MathResult<Vec<String>> {
    let mut indexed: Vec<(i128, String)> = Vec::new();
    for slot in graph.refs(node, M_ARGUMENT_SLOT) {
        let index_lexicals = graph.all_lit(&slot, M_SLOT_INDEX);
        let index_lex = match index_lexicals.as_slice() {
            [] => {
                return Err(MathLoweringError::ArgumentSlotMissingIndex { slot });
            }
            [one] => one.clone(),
            _ => {
                return Err(MathLoweringError::ArgumentSlotMultipleIndexes {
                    slot,
                    count: index_lexicals.len(),
                });
            }
        };
        let index: i128 = index_lex.trim().parse().map_err(|_| {
            MathLoweringError::ArgumentSlotIndexNotInteger {
                slot: slot.clone(),
                lexical: index_lex.clone(),
            }
        })?;
        if index < 0 {
            return Err(MathLoweringError::NegativeArgumentSlotIndex {
                node: node.to_owned(),
                slot,
                index,
            });
        }
        let expr = graph
            .first_ref(&slot, M_SLOT_EXPRESSION)
            .ok_or(MathLoweringError::ArgumentSlotMissingExpression { slot })?;
        indexed.push((index, expr));
    }
    indexed.sort_by_key(|(index, _)| *index);
    // Duplicate check BEFORE contiguity: a duplicate index makes the "expected
    // position" walk below meaningless (two slots would both claim one position).
    for pair in indexed.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(MathLoweringError::DuplicateArgumentSlotIndex {
                node: node.to_owned(),
                index: pair[0].0,
            });
        }
    }
    for (expected, (index, _)) in indexed.iter().enumerate() {
        if *index != expected as i128 {
            return Err(MathLoweringError::NonContiguousArgumentSlots {
                node: node.to_owned(),
                index: *index,
                expected_position: expected,
            });
        }
    }
    Ok(indexed.into_iter().map(|(_, expr)| expr).collect())
}

fn lower_math_application(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &mut Vec<Vec<String>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
) -> MathResult<NodeId> {
    let operators = graph.refs(node, M_OPERATOR);
    let operator = match operators.as_slice() {
        [] => {
            return Err(MathLoweringError::ApplicationMissingOperator {
                node: node.to_owned(),
            });
        }
        [one] => one.clone(),
        _ => {
            return Err(MathLoweringError::ApplicationMultipleOperators {
                node: node.to_owned(),
                count: operators.len(),
            });
        }
    };
    let op = dag.intern_leaf(TermValue::iri(operator));
    let slot_exprs = collect_slots(graph, node)?;
    let mut args = Vec::with_capacity(slot_exprs.len());
    for expr in &slot_exprs {
        args.push(lower_math_node(dag, graph, expr, env, visiting, depth + 1)?);
    }
    Ok(dag.intern_app(op, args))
}

fn lower_math_binding(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &mut Vec<Vec<String>>,
    visiting: &mut BTreeSet<String>,
    depth: usize,
) -> MathResult<NodeId> {
    let operators = graph.refs(node, M_OPERATOR);
    let operator = match operators.as_slice() {
        [] => {
            return Err(MathLoweringError::BindingMissingOperator {
                node: node.to_owned(),
            });
        }
        [one] => one.clone(),
        _ => {
            return Err(MathLoweringError::BindingMultipleOperators {
                node: node.to_owned(),
                count: operators.len(),
            });
        }
    };
    let op = dag.intern_leaf(TermValue::iri(operator));

    let declarations = graph.refs(node, M_BOUND_VARIABLE);
    let declaration = match declarations.as_slice() {
        [] => {
            return Err(MathLoweringError::BindingMissingBoundVariable {
                node: node.to_owned(),
            });
        }
        [one] => one.clone(),
        _ => {
            return Err(MathLoweringError::BindingMultipleBoundVariables {
                node: node.to_owned(),
                count: declarations.len(),
            });
        }
    };
    // The bound variable's declared type/domain becomes the binder's sort child and is
    // never dropped; an undeclared domain defaults to the untyped individual sort (so an
    // undeclared `math:` binder collapses with an untyped `logic:` quantifier).
    let sort_iri = graph
        .first_ref(&declaration, M_DOMAIN)
        .unwrap_or_else(|| canon::SORT_INDIVIDUAL.to_owned());
    let sort = dag.intern_leaf(TermValue::iri(sort_iri));
    // A binder binds ONE variable over its indexed operand sequence — the shape this slice
    // authors: `math:BindingExpression` "names its body through indexed math:argumentSlot
    // cells", and `math:ModelFormula` is "a binder over indexed math:ArgumentSlot operands"
    // (an R `y ~ x1 + x2` lifts to exactly that). Demanding a single index-0 slot made the
    // lowering stricter than the vocabulary it serves, so the shipped `gmeow lift r`
    // producer's own output failed the shipped validator.
    //
    // ONE operand is the body itself; several are the binder's operator applied to them.
    // That asymmetry is load-bearing, not a convenience: the arena's whole point is that a
    // `math:` binder and an alpha-equivalent `logic:` quantifier intern to the SAME node, and
    // a `logic:` quantifier is `bind(op, [sort], body)` with the body bare. Wrapping a
    // single operand in a vacuous `apply(op, [x])` would break that cross-surface collapse
    // and move every existing binder digest. For several operands there is no `logic:` twin
    // to agree with, and the operator is exactly what combines them.
    let body_slots = collect_slots(graph, node)?;
    if body_slots.is_empty() {
        return Err(MathLoweringError::BindingMissingBody {
            node: node.to_owned(),
        });
    }
    env.push(vec![declaration]);
    let mut lowered: Vec<NodeId> = Vec::with_capacity(body_slots.len());
    let mut failure = None;
    for slot in &body_slots {
        match lower_math_node(dag, graph, slot, env, visiting, depth + 1) {
            Ok(id) => lowered.push(id),
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    env.pop();
    if let Some(e) = failure {
        return Err(e);
    }
    let body = match lowered.as_slice() {
        [single] => *single,
        _ => dag.intern_app(op, lowered),
    };
    Ok(dag.intern_binder(op, vec![sort], body))
}

fn lower_math_variable(
    dag: &mut TermDag,
    graph: &MathGraph,
    node: &str,
    env: &[Vec<String>],
) -> MathResult<NodeId> {
    let occurrences = graph.refs(node, M_VARIABLE_OCCURRENCE);
    let occurrence = match occurrences.as_slice() {
        [] => {
            return Err(MathLoweringError::VariableExpressionMissingOccurrence {
                node: node.to_owned(),
            });
        }
        [one] => one.clone(),
        _ => {
            return Err(MathLoweringError::VariableExpressionMultipleOccurrences {
                node: node.to_owned(),
                count: occurrences.len(),
            });
        }
    };

    let declarations = graph.refs(&occurrence, M_DECLARED_VARIABLE);
    let declaration = match declarations.as_slice() {
        [] => {
            return Err(MathLoweringError::OccurrenceMissingDeclaredVariable { occurrence });
        }
        [one] => one.clone(),
        _ => {
            return Err(MathLoweringError::OccurrenceMultipleDeclaredVariables {
                occurrence,
                count: declarations.len(),
            });
        }
    };

    if let Some((distance, slot)) = resolve_debruijn(env, &declaration) {
        // `intern_bound_checked`'s two overflow modes are provably unreachable on this
        // path: `lower_math_binding` pushes exactly one declaration per binder frame
        // (`env.push(vec![declaration])`), so `resolve_debruijn`'s returned `slot` is
        // always `0` and `u16::try_from` cannot fail; and every recursive descent is
        // gated by `depth > MAX_MATH_EXPRESSION_DEPTH` (500) before it proceeds, so
        // `env.len()` — and therefore any `distance < env.len()` this lookup can return —
        // never exceeds 500, far inside `u32`. An `Err` here would mean that invariant
        // broke, which is an internal defect in this lowering, never a `math:`
        // conformance failure of the authored data — so it is a hard panic, not a
        // laundered `MathLoweringError`.
        return Ok(
            intern_bound_checked(dag, distance, slot).unwrap_or_else(|e| {
                panic!(
                    "math: binder frames carry exactly one declaration each and recursion is \
                 depth-bounded by MAX_MATH_EXPRESSION_DEPTH, so a de-Bruijn distance/slot \
                 computed here can never overflow u32/u16; got {e:?}"
                )
            }),
        );
    }
    if graph.has_type(&declaration, M_FREE_DECLARATION) {
        return Ok(dag.intern_free(TermValue::iri(declaration)));
    }
    Err(MathLoweringError::UnscopedOccurrence {
        occurrence,
        declaration,
    })
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

#[path = "lower.tests.rs"]
#[cfg(test)]
mod tests;
