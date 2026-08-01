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
        Term::Literal { lexical, datatype } => {
            let tv = match datatype {
                None => TermValue::simple_literal(lexical.clone()),
                Some(dt) => TermValue::typed_literal(lexical.clone(), dt.clone()),
            };
            dag.intern_leaf(tv)
        }
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
/// The ABSTRACT expression base. A node typed with it and nothing more concrete is an
/// expression whose FORM is deliberately unspecified — the shipped tensor/learning examples
/// use it for an operand they name but do not decompose. It has no structure to walk, so it
/// interns on its own IRI: that is the only content it has, and two distinct unspecified
/// operands are genuinely distinct terms.
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
const MAX_MATH_EXPRESSION_DEPTH: usize = 500;

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
/// exception is a Turtle PARSE failure ([`MathGraph::from_turtle`]): that is not a
/// conformance failure of an authored `math:` expression — it means there is no graph
/// to check at all — so it stays a plain [`Diag`], never a member of this enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MathLoweringError {
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
    /// A `math:BindingExpression` carries more than one `math:boundVariable` value.
    BindingMultipleBoundVariables { node: String, count: usize },
    /// A `math:BindingExpression`'s body slot family is not exactly `{index 0}`.
    BindingBodyNotSingleSlot { node: String, slot_count: usize },
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
    /// Lowering recursed past [`MAX_MATH_EXPRESSION_DEPTH`].
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
            Self::BindingMissingBoundVariable { node } => write!(
                f,
                "math:BindingExpression {node} missing math:boundVariable"
            ),
            Self::BindingMultipleBoundVariables { node, count } => write!(
                f,
                "math:BindingExpression {node} carries {count} math:boundVariable values; \
                 exactly one is required"
            ),
            Self::BindingBodyNotSingleSlot { node, slot_count } => write!(
                f,
                "math:BindingExpression {node} must carry exactly one body slot \
                 (math:slotIndex 0); found {slot_count} slot(s)"
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
    pub(crate) fn failure_class(&self) -> &'static str {
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
            | Self::BindingMultipleBoundVariables { .. }
            | Self::BindingBodyNotSingleSlot { .. } => failure_class::MALFORMED_BINDING_EXPRESSION,
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
                    // reached no digest to be compared against: `check_structural_key_drift`
                    // found no entry and skipped it, so a hand-guessed digest — the exact thing
                    // the property's own `gmeow:avoidWhen` forbids — passed the gate silently.
                    // Included here, an undecomposed one lowers to its IRI leaf and its key is
                    // checked like any other; a decomposed one is rejected and reported.
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

/// Fold a `TermDag` node's content key into the published digest — TEST SCAFFOLDING ONLY.
///
/// Production never calls this: the shipped `math:structuralKey` is computed by
/// [`arena_structural_key`], through the arena facade, and both routes end at
/// [`fold_content_key`] over the same bytes ([`gmeow_term_arena::Arena::key`] returns
/// `TermDag::key` verbatim). It exists because several invariant tests below intern SEVERAL
/// nodes into ONE shared `TermDag` to check hash-consing, which the graph-and-root production
/// entry point cannot express. `#[cfg(test)]` so it can never become a second production
/// surface — the duplicate-entry-point condition the arena facade was deleted for.
#[cfg(test)]
pub(crate) fn structural_digest(dag: &TermDag, id: NodeId) -> String {
    fold_content_key(dag.key(id))
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
        // `math:VariableOccurrence` used where an expression belongs, ... — is a HARD
        // FAIL rather than a silently-degraded opaque leaf: letting an ill-typed AST
        // through here would mean `math:StructuralKeyOnRejectedExpression` never fires
        // and `math:StructuralKeyDrift` compares a declared key against a digest computed
        // over garbage.
        let math_types: Vec<&String> = types.iter().filter(|t| t.starts_with(MATH_NS)).collect();
        // `math:MathematicalExpression` alone is the abstract base — an operand named but not
        // decomposed. That is a POSITIVE typing meaning "unspecified form", not the unknown
        // typing the hard fail exists for, so it interns on its own IRI. Rejecting it reported
        // the slice's own conforming examples as ill-typed.
        //
        // ONLY when it really is undecomposed. A node carrying one of the four structured-child
        // edges — the same four `math:StringOnlyComputableExpression` names — HAS structure the
        // abstract type gives the lowering no production to walk. Interning it on its IRI would
        // silently DROP that subtree, and two expressions differing only inside it would share
        // one digest and one `math:AlphaEquivalenceClass`: a content key computed over content
        // the lowering refused to read. Structure present with no concrete form to interpret it
        // is exactly `UnrecognizedExpressionType`.
        const STRUCTURED_CHILD_EDGES: [&str; 4] = [
            M_ARGUMENT_SLOT,
            M_BOUND_VARIABLE,
            M_HAS_MATHEMATICAL_SYMBOL,
            M_LITERAL_VALUE,
        ];
        let is_abstract_expression = !math_types.is_empty()
            && math_types
                .iter()
                .all(|t| t.as_str() == M_MATHEMATICAL_EXPRESSION)
            && STRUCTURED_CHILD_EDGES
                .iter()
                .all(|edge| graph.refs(node, edge).is_empty());
        let is_constant_operand = !node.starts_with("_:")
            && (math_types.is_empty()
                || is_abstract_expression
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
    // A binder binds over exactly one body, carried as its single index-0 argument slot.
    let body_slots = collect_slots(graph, node)?;
    if body_slots.len() != 1 {
        return Err(MathLoweringError::BindingBodyNotSingleSlot {
            node: node.to_owned(),
            slot_count: body_slots.len(),
        });
    }
    env.push(vec![declaration]);
    let body = lower_math_node(dag, graph, &body_slots[0], env, visiting, depth + 1);
    env.pop();
    let body = body?;
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

#[cfg(test)]
mod tests {
    use super::*;

    use gmeow_term_arena::engine::TermDag;

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
        assert_eq!(
            err,
            MathLoweringError::NonContiguousArgumentSlots {
                node: "https://example.org/app".to_owned(),
                index: 2,
                expected_position: 1,
            },
            "gap diagnostic names the non-contiguous slot family: {err:?}"
        );
        assert_eq!(
            err.failure_class(),
            "https://blackcatinformatics.ca/math/NonContiguousArgumentSlots"
        );
    }

    #[test]
    fn math_duplicate_slot_index_hard_fails_distinctly_from_a_gap() {
        // Indexes {0, 0} are a DUPLICATE, not a gap — a distinct rejection variant/class.
        let mut dag = TermDag::new();
        let graph = MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (0, "ex:b")]).as_bytes())
            .expect("parse");
        let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
            .expect_err("duplicate slot index must hard-fail");
        assert_eq!(
            err,
            MathLoweringError::DuplicateArgumentSlotIndex {
                node: "https://example.org/app".to_owned(),
                index: 0,
            }
        );
        assert_eq!(
            err.failure_class(),
            "https://blackcatinformatics.ca/math/DuplicateArgumentSlotIndex"
        );
    }

    #[test]
    fn math_negative_slot_index_hard_fails_as_malformed() {
        let mut dag = TermDag::new();
        let graph =
            MathGraph::from_turtle(application_ttl(&[(-1, "ex:a")]).as_bytes()).expect("parse");
        let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
            .expect_err("negative slot index must hard-fail");
        assert!(
            matches!(
                err,
                MathLoweringError::NegativeArgumentSlotIndex { index: -1, .. }
            ),
            "{err:?}"
        );
        assert_eq!(
            err.failure_class(),
            "https://blackcatinformatics.ca/math/MalformedArgumentSlot"
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
            matches!(err, MathLoweringError::UnscopedOccurrence { .. }),
            "diagnostic names the unscoped occurrence: {err:?}"
        );
        assert_eq!(
            err.failure_class(),
            "https://blackcatinformatics.ca/math/UnscopedVariableOccurrence"
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

    // ── commutative sort key is CONTENT KEY, never NodeId ───────────────────────────

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

    // ── a math:NumberLiteral's typed literalValue lowers, datatype preserved ────────

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

    // ── math: a cyclic slotExpression graph hard-fails, never stack-overflows ──────────

    #[test]
    fn math_cyclic_expression_graph_hard_fails() {
        // ex:a's argument slot points at ex:b, whose argument slot points back at ex:a —
        // a two-triple cycle through `math:slotExpression`.
        let cyclic_ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:a a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:sa .\n\
             ex:sa a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:b .\n\
             ex:b a math:ApplicationExpression ; math:operator ex:q ; math:argumentSlot ex:sb .\n\
             ex:sb a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:a .\n";
        let mut dag = TermDag::new();
        let graph = MathGraph::from_turtle(cyclic_ttl.as_bytes()).expect("parse");
        let err = lower_math_expression(&mut dag, &graph, "https://example.org/a")
            .expect_err("a cyclic slotExpression graph must hard-fail, not stack-overflow");
        assert!(
            matches!(err, MathLoweringError::CyclicExpressionGraph { .. }),
            "{err:?}"
        );
        assert_eq!(
            err.failure_class(),
            "https://blackcatinformatics.ca/math/CyclicExpressionGraph"
        );
    }

    // ── a ROOTLESS (fully closed) cyclic component is still reached and rejected ──

    /// A fully closed 2-node cycle — `ex:a`'s argument slot points at `ex:b`, whose
    /// argument slot points back at `ex:a`, and NEITHER is referenced from outside the
    /// cycle — has NO node satisfying [`MathGraph::expression_roots`]'s "not referenced"
    /// test: EVERY member is both candidate-typed AND referenced by the OTHER member of
    /// the SAME component. Before the `expression_typed_nodes` orphan-seeding pass,
    /// [`math_expression_structural_keys`] therefore NEVER lowered either node at all —
    /// `lower_math_expression` was never called, so `math:CyclicExpressionGraph` could
    /// never fire and this case was silently invisible (zero entries, zero findings, ZERO
    /// coverage) rather than a rejected root. This asserts the CAPABILITY: the closed
    /// component is discovered and its cycle guard actually fires.
    #[test]
    fn math_expression_structural_keys_reaches_a_rootless_cyclic_component() {
        let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:a a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:sa .\n\
             ex:sa a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:b .\n\
             ex:b a math:ApplicationExpression ; math:operator ex:q ; math:argumentSlot ex:sb .\n\
             ex:sb a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:a .\n";
        let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");

        // Confirm the premise: NEITHER ex:a nor ex:b is a root under the pure
        // "not referenced" test — the closed cycle has no unreferenced member.
        let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse graph");
        assert!(
            graph.expression_roots().is_empty(),
            "a fully closed cycle has NO unreferenced member: {:?}",
            graph.expression_roots()
        );

        let results = math_expression_structural_keys(&dataset);
        assert!(
            !results.is_empty(),
            "the closed cyclic component must still be SEEDED and lowered at least once, \
             not silently skipped: {results:?}"
        );
        // Every entry must be the SAME typed rejection: the cycle guard actually fires,
        // never a silent Ok (accepting a cyclic node as an opaque leaf) and never some
        // OTHER unrelated rejection reason.
        for (node, result) in &results {
            match result {
                Err(MathLoweringError::CyclicExpressionGraph { .. }) => {}
                other => panic!("{node}: expected CyclicExpressionGraph, got {other:?}"),
            }
        }
    }

    // ── math: a pathologically deep application chain hard-fails on depth ─────────────

    #[test]
    fn math_expression_depth_exceeded_hard_fails() {
        // A chain of NESTED single-argument applications, several times deeper than
        // MAX_MATH_EXPRESSION_DEPTH, generated programmatically (not hand-authored) —
        // ex:app0(ex:app1(ex:app2(...(ex:leaf)...))).
        const CHAIN_LEN: usize = 2_000;
        let mut ttl = String::from(
            "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:leaf a math:NumberLiteral ; math:literalValue \"1\" .\n",
        );
        for i in 0..CHAIN_LEN {
            let child = if i + 1 == CHAIN_LEN {
                "ex:leaf".to_owned()
            } else {
                format!("ex:app{}", i + 1)
            };
            ttl.push_str(&format!(
                "ex:app{i} a math:ApplicationExpression ; math:operator ex:p ; \
                 math:argumentSlot ex:s{i} .\n\
                 ex:s{i} a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression {child} .\n"
            ));
        }
        let mut dag = TermDag::new();
        let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse");
        let err = lower_math_expression(&mut dag, &graph, "https://example.org/app0")
            .expect_err("a chain far deeper than the recursion bound must hard-fail");
        assert!(
            matches!(err, MathLoweringError::ExpressionDepthExceeded { .. }),
            "{err:?}"
        );
        assert_eq!(
            err.failure_class(),
            "https://blackcatinformatics.ca/math/ExpressionDepthExceeded"
        );
    }

    // ── math: per-root structural keys isolate one bad root from the rest ─────────────

    #[test]
    fn math_expression_structural_keys_isolates_a_bad_root_from_good_roots() {
        // Two INDEPENDENT root expressions in one dataset: ex:good is well-formed,
        // ex:bad has a slot gap. Neither is referenced as any other node's
        // math:slotExpression, so both are candidate roots.
        let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:good a math:ApplicationExpression ; math:operator ex:p ; \
             math:argumentSlot ex:gs0 .\n\
             ex:gs0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:a .\n\
             ex:bad a math:ApplicationExpression ; math:operator ex:q ; \
             math:argumentSlot ex:bs0 , ex:bs2 .\n\
             ex:bs0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:b .\n\
             ex:bs2 a math:ArgumentSlot ; math:slotIndex 2 ; math:slotExpression ex:c .\n";
        let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
        let results = math_expression_structural_keys(&dataset);

        assert_eq!(results.len(), 2, "both roots are candidates: {results:?}");
        let good = results
            .get("https://example.org/good")
            .expect("ex:good present");
        assert!(good.is_ok(), "the well-formed root still lowers: {good:?}");

        let bad = results
            .get("https://example.org/bad")
            .expect("ex:bad present");
        let err = bad.as_ref().expect_err("the malformed root still fails");
        assert!(
            matches!(err, MathLoweringError::NonContiguousArgumentSlots { .. }),
            "{err:?}"
        );
    }

    // ── math: structural_digest / alpha_class_iri_for_digest are deterministic ─────────

    #[test]
    fn structural_digest_matches_for_alpha_equivalent_roots_and_differs_otherwise() {
        // p(a, b) authored slots-forward vs slots-reversed: the SAME expression, so the
        // SAME structural digest (they already intern to the same NodeId — this checks
        // the digest built on top of that identity agrees too).
        let forward =
            MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (1, "ex:b")]).as_bytes())
                .expect("parse");
        let reversed =
            MathGraph::from_turtle(application_ttl(&[(1, "ex:b"), (0, "ex:a")]).as_bytes())
                .expect("parse");

        let digest_a =
            arena_structural_key(&forward, "https://example.org/app").expect("forward lowers");
        let digest_b =
            arena_structural_key(&reversed, "https://example.org/app").expect("reversed lowers");

        assert_eq!(
            digest_a, digest_b,
            "alpha-equivalent expressions share one structural digest"
        );
        // Deterministic: computing it again from the same graph is byte-identical.
        assert_eq!(
            digest_a,
            arena_structural_key(&forward, "https://example.org/app").expect("forward lowers")
        );

        // p(b, a) is a DISTINCT expression (operand order is identity-bearing) — a
        // DIFFERENT digest.
        let swapped =
            MathGraph::from_turtle(application_ttl(&[(0, "ex:b"), (1, "ex:a")]).as_bytes())
                .expect("parse");

        let digest_c =
            arena_structural_key(&swapped, "https://example.org/app").expect("swapped lowers");
        assert_ne!(digest_a, digest_c, "p(a,b) and p(b,a) get distinct digests");

        // The live minting entry point — the SAME one production calls — is content-stable
        // over the digest and injective across distinct digests.
        let iri_a = alpha_class_iri_for_digest(&digest_a);
        assert_eq!(
            iri_a,
            alpha_class_iri_for_digest(&digest_a),
            "minting is deterministic"
        );
        assert_ne!(
            iri_a,
            alpha_class_iri_for_digest(&digest_c),
            "distinct digests mint distinct alpha-class IRIs"
        );
    }

    // ── math: failure_class is exhaustive, non-empty, and groups variants coherently ──

    /// One concrete sample of EVERY [`MathLoweringError`] variant — the single committed
    /// enumeration of "all variants" both
    /// [`math_lowering_error_failure_class_is_exhaustive_and_non_empty`] (the failure-class
    /// bucketing property) and
    /// [`every_math_lowering_error_variant_is_produced_by_a_committed_fixture`] (the
    /// variant-liveness property) drive from, so the two properties can never silently
    /// diverge on "how many variants there are" — exactly one list, never two.
    fn sample_variants() -> Vec<MathLoweringError> {
        vec![
            MathLoweringError::NumberLiteralMissingValue {
                node: "n".to_owned(),
            },
            MathLoweringError::UnrecognizedExpressionType {
                node: "n".to_owned(),
                types: vec!["https://blackcatinformatics.ca/math/SomeUnrecognizedType".to_owned()],
            },
            MathLoweringError::ArgumentSlotMissingIndex {
                slot: "s".to_owned(),
            },
            MathLoweringError::ArgumentSlotMultipleIndexes {
                slot: "s".to_owned(),
                count: 2,
            },
            MathLoweringError::ArgumentSlotIndexNotInteger {
                slot: "s".to_owned(),
                lexical: "x".to_owned(),
            },
            MathLoweringError::ArgumentSlotMissingExpression {
                slot: "s".to_owned(),
            },
            MathLoweringError::NonContiguousArgumentSlots {
                node: "n".to_owned(),
                index: 2,
                expected_position: 1,
            },
            MathLoweringError::DuplicateArgumentSlotIndex {
                node: "n".to_owned(),
                index: 0,
            },
            MathLoweringError::NegativeArgumentSlotIndex {
                node: "n".to_owned(),
                slot: "s".to_owned(),
                index: -1,
            },
            MathLoweringError::ApplicationMissingOperator {
                node: "n".to_owned(),
            },
            MathLoweringError::ApplicationMultipleOperators {
                node: "n".to_owned(),
                count: 2,
            },
            MathLoweringError::BindingMissingOperator {
                node: "n".to_owned(),
            },
            MathLoweringError::BindingMultipleOperators {
                node: "n".to_owned(),
                count: 2,
            },
            MathLoweringError::BindingMissingBoundVariable {
                node: "n".to_owned(),
            },
            MathLoweringError::BindingMultipleBoundVariables {
                node: "n".to_owned(),
                count: 2,
            },
            MathLoweringError::BindingBodyNotSingleSlot {
                node: "n".to_owned(),
                slot_count: 2,
            },
            MathLoweringError::VariableExpressionMissingOccurrence {
                node: "n".to_owned(),
            },
            MathLoweringError::VariableExpressionMultipleOccurrences {
                node: "n".to_owned(),
                count: 2,
            },
            MathLoweringError::OccurrenceMissingDeclaredVariable {
                occurrence: "o".to_owned(),
            },
            MathLoweringError::OccurrenceMultipleDeclaredVariables {
                occurrence: "o".to_owned(),
                count: 2,
            },
            MathLoweringError::UnscopedOccurrence {
                occurrence: "o".to_owned(),
                declaration: "d".to_owned(),
            },
            MathLoweringError::CyclicExpressionGraph {
                node: "n".to_owned(),
            },
            MathLoweringError::ExpressionDepthExceeded {
                node: "n".to_owned(),
                depth: 501,
            },
        ]
    }

    /// The variant name of a [`MathLoweringError`] sample, for a human-readable liveness
    /// report ONLY (never used to decide equality — that is
    /// [`std::mem::discriminant`]'s job). Exhaustive with NO wildcard arm for the SAME
    /// reason [`MathLoweringError::failure_class`] has none: a variant added later without
    /// a label fails to compile rather than silently reporting as unnamed.
    fn variant_label(v: &MathLoweringError) -> &'static str {
        match v {
            MathLoweringError::NumberLiteralMissingValue { .. } => "NumberLiteralMissingValue",
            MathLoweringError::UnrecognizedExpressionType { .. } => "UnrecognizedExpressionType",
            MathLoweringError::ArgumentSlotMissingIndex { .. } => "ArgumentSlotMissingIndex",
            MathLoweringError::ArgumentSlotMultipleIndexes { .. } => "ArgumentSlotMultipleIndexes",
            MathLoweringError::ArgumentSlotIndexNotInteger { .. } => "ArgumentSlotIndexNotInteger",
            MathLoweringError::ArgumentSlotMissingExpression { .. } => {
                "ArgumentSlotMissingExpression"
            }
            MathLoweringError::NonContiguousArgumentSlots { .. } => "NonContiguousArgumentSlots",
            MathLoweringError::DuplicateArgumentSlotIndex { .. } => "DuplicateArgumentSlotIndex",
            MathLoweringError::NegativeArgumentSlotIndex { .. } => "NegativeArgumentSlotIndex",
            MathLoweringError::ApplicationMissingOperator { .. } => "ApplicationMissingOperator",
            MathLoweringError::ApplicationMultipleOperators { .. } => {
                "ApplicationMultipleOperators"
            }
            MathLoweringError::BindingMissingOperator { .. } => "BindingMissingOperator",
            MathLoweringError::BindingMultipleOperators { .. } => "BindingMultipleOperators",
            MathLoweringError::BindingMissingBoundVariable { .. } => "BindingMissingBoundVariable",
            MathLoweringError::BindingMultipleBoundVariables { .. } => {
                "BindingMultipleBoundVariables"
            }
            MathLoweringError::BindingBodyNotSingleSlot { .. } => "BindingBodyNotSingleSlot",
            MathLoweringError::VariableExpressionMissingOccurrence { .. } => {
                "VariableExpressionMissingOccurrence"
            }
            MathLoweringError::VariableExpressionMultipleOccurrences { .. } => {
                "VariableExpressionMultipleOccurrences"
            }
            MathLoweringError::OccurrenceMissingDeclaredVariable { .. } => {
                "OccurrenceMissingDeclaredVariable"
            }
            MathLoweringError::OccurrenceMultipleDeclaredVariables { .. } => {
                "OccurrenceMultipleDeclaredVariables"
            }
            MathLoweringError::UnscopedOccurrence { .. } => "UnscopedOccurrence",
            MathLoweringError::CyclicExpressionGraph { .. } => "CyclicExpressionGraph",
            MathLoweringError::ExpressionDepthExceeded { .. } => "ExpressionDepthExceeded",
        }
    }

    /// Every committed `math:` counter-example fixture, parsed as its own standalone
    /// dataset — mirrors [`MathGraph::from_turtle`]'s own parse call exactly, since a
    /// fixture is a self-contained Turtle document (its own `@prefix` declarations, no
    /// dependency on `module.ttl` schema triples: [`MathGraph`] walks asserted
    /// `math:`/`rdf:type` edges directly, it consults no OWL/RDFS axiom).
    fn counter_example_fixtures() -> Vec<(String, std::sync::Arc<purrdf::RdfDataset>)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("slices/grounding/math/tests/counter-examples");
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read counter-examples dir {}: {e}", dir.display()))
            .map(|e| e.expect("dir entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "ttl"))
            .collect();
        entries.sort();
        entries
            .into_iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .expect("utf8 fixture name")
                    .to_owned();
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
                let ds = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()));
                (name, ds)
            })
            .collect()
    }

    /// The committed depth fixture is TIED to [`MAX_MATH_EXPRESSION_DEPTH`], not to a number
    /// someone typed once.
    ///
    /// It is a hand-committed chain rather than a generated one, so raising the depth bound
    /// would leave it quietly UNDER the limit: it would stop exceeding anything, the fixture
    /// would lower cleanly, and `math:ExpressionDepthExceeded` would go unexercised while every
    /// assertion still passed. Pin the relationship so the constant and the fixture cannot
    /// drift apart silently — if this fails, regenerate the chain to the new bound.
    #[test]
    fn the_depth_fixture_chain_is_pinned_to_the_depth_bound() {
        const FIXTURE: &str = include_str!(
            "../../../../slices/grounding/math/tests/counter-examples/expression-depth-exceeded.ttl"
        );
        let nodes = FIXTURE
            .lines()
            .filter(|l| l.contains("a math:ApplicationExpression"))
            .count();
        assert!(
            nodes > MAX_MATH_EXPRESSION_DEPTH,
            "the depth fixture must EXCEED the bound it exercises: {nodes} application nodes \
             vs MAX_MATH_EXPRESSION_DEPTH {MAX_MATH_EXPRESSION_DEPTH} — regenerate the chain"
        );
    }

    /// **The variant-liveness test.** For every [`MathLoweringError`] variant,
    /// at least one committed `slices/grounding/math/tests/counter-examples/*.ttl` fixture
    /// must actually produce it through the REAL production entry point
    /// [`math_expression_structural_keys`] — the SAME function
    /// [`crate::math_expression::check_math_expression_findings`] calls over the frozen
    /// reasoned graph. A hand-built [`MathLoweringError`] sample (as
    /// [`sample_variants`] provides for the failure-class bucketing property) proves the
    /// variant COMPILES and has a class; it does NOT prove the variant is REACHABLE from
    /// authored data. Without this test, an unreachable variant is a phantom failure class
    /// the charter would report as enforced when no fixture on disk can ever raise it — the
    /// gap this test exists to close (the classes minted in Rust but authored nowhere, `CyclicExpressionGraph` /
    /// `ExpressionDepthExceeded`, shipped with exactly this gap until their fixtures landed).
    ///
    /// No allowlist, no skip-set: a variant absent from EVERY fixture's produced errors
    /// fails the test by name, never silently passes.
    #[test]
    fn every_math_lowering_error_variant_is_produced_by_a_committed_fixture() {
        let fixtures = counter_example_fixtures();
        assert!(
            !fixtures.is_empty(),
            "tests/counter-examples has no fixtures to drive this liveness test"
        );

        // discriminant -> (label, fixtures that produced it) — built by ACTUALLY RUNNING the
        // real production lowering entry point over every committed fixture, never by
        // hand-constructing a variant.
        let mut produced: std::collections::HashMap<
            std::mem::Discriminant<MathLoweringError>,
            (&'static str, Vec<String>),
        > = std::collections::HashMap::new();
        for (name, ds) in &fixtures {
            for result in math_expression_structural_keys(ds).into_values() {
                if let Err(err) = result {
                    let discriminant = std::mem::discriminant(&err);
                    produced
                        .entry(discriminant)
                        .or_insert_with(|| (variant_label(&err), Vec::new()))
                        .1
                        .push(name.clone());
                }
            }
        }

        let samples = sample_variants();
        assert_eq!(
            samples.len(),
            23,
            "sample_variants() must enumerate every MathLoweringError variant exactly once \
             (update this count alongside the enum and both variant lists when a variant is \
             added or removed)"
        );
        // The count alone would still pass if one variant were listed twice and another
        // dropped — the two errors cancel. Pin DISTINCTNESS by discriminant so a duplicate
        // is caught as itself rather than hiding an omission.
        let distinct: std::collections::BTreeSet<_> = samples
            .iter()
            .map(|v| format!("{:?}", std::mem::discriminant(v)))
            .collect();
        assert_eq!(
            distinct.len(),
            samples.len(),
            "sample_variants() must list each variant ONCE; a duplicate silently masks a \
             missing one because only the total is checked"
        );
        let missing: Vec<&'static str> = samples
            .iter()
            .filter(|v| !produced.contains_key(&std::mem::discriminant(*v)))
            .map(variant_label)
            .collect();
        assert!(
            missing.is_empty(),
            "the following MathLoweringError variant(s) are produced by NO committed \
             counter-example fixture through the real production lowering entry point — each \
             is a phantom failure class (reachable-by-name in Rust, unreachable from any \
             authored data): {missing:?}. Fixtures that DID trip a variant: {:#?}",
            produced
                .values()
                .map(|(label, files)| (*label, files.clone()))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
    }

    #[test]
    fn math_lowering_error_failure_class_is_exhaustive_and_non_empty() {
        let variants = sample_variants();

        // Every variant must decide a non-empty, properly-namespaced `math:` IRI, and the
        // Display impl must not panic (each variant is exercised through `{}`).
        for variant in &variants {
            let class = variant.failure_class();
            assert!(!class.is_empty(), "{variant:?} has an empty failure class");
            assert!(
                class.starts_with("https://blackcatinformatics.ca/math/"),
                "{variant:?} failure class {class} is not `math:`-namespaced"
            );
            let _ = format!("{variant}");
        }

        // Check the mapping against the ONTOLOGY, never against a copy of the mapping.
        //
        // This assertion used to be a second, hand-maintained `match` restating
        // `failure_class()` arm for arm. That form cannot fail for a WRONG bucket — it asserts
        // the function equals itself — and it is why two variants sat mis-typed against the
        // target class's own definition without any test noticing. Reading the authored
        // module.ttl instead means a class this code decides but the slice never authored, or
        // deletes, fails here.
        const MODULE: &str = include_str!("../../../../slices/grounding/math/module.ttl");
        for variant in &variants {
            let class = variant.failure_class();
            let local = class
                .rsplit('/')
                .next()
                .expect("class IRI has a local name");
            assert!(
                MODULE.contains(&format!("\nmath:{local}\n")),
                "{variant:?} decides math:{local}, which module.ttl does not author"
            );
        }

        // Distinct buckets, derived rather than hardcoded: the count follows the mapping, and
        // the assertion above is what pins each one to an authored class.
        let distinct_classes: std::collections::BTreeSet<&'static str> = variants
            .iter()
            .map(MathLoweringError::failure_class)
            .collect();
        assert_eq!(
            distinct_classes.len(),
            10,
            "the rejection algebra must keep EXACTLY its ten distinct failure-class buckets — \
             an inequality here would let two buckets silently collapse into one: \
             {distinct_classes:?}"
        );
    }

    // ── structural_digest: alpha-equivalence, injectivity, and interning properties ────
    //
    // These properties exercise `structural_digest`/`lower_math_expression` at the
    // property level: alpha-equivalent inputs
    // (differing ONLY in a bound-variable declaration's IRI/label) must share one
    // digest; structurally distinct inputs must never collide; and interning one
    // expression's alpha-variants into a SHARED dag must add nodes for the distinct
    // structure only, never once per variant. The final property reconciles the
    // `math:structuralKey` values authored in `reference-ast-act.ttl` against the
    // real, recomputed digest.

    const ALPHA_PAIR_A: &str = include_str!(
        "../../../../slices/grounding/math/tests/conformance-fixtures/alpha-equivalent-pair-a.ttl"
    );
    const ALPHA_PAIR_B: &str = include_str!(
        "../../../../slices/grounding/math/tests/conformance-fixtures/alpha-equivalent-pair-b.ttl"
    );
    const ALPHA_SHADOWING: &str = include_str!(
        "../../../../slices/grounding/math/tests/conformance-fixtures/alpha-equivalent-shadowing.ttl"
    );
    const REFERENCE_AST_ACT: &str =
        include_str!("../../../../slices/grounding/math/examples/reference-ast-act.ttl");

    /// `math:structuralKey`'s full predicate IRI — read-only in these tests (this
    /// module never authors it; [`crate::math_expression`] owns the reasoned-graph
    /// drift gate that DOES).
    const M_STRUCTURAL_KEY: &str = "https://blackcatinformatics.ca/math/structuralKey";

    /// Build a one-variable-binder `math:` expression `∀x. p(x)` whose every
    /// declaration/occurrence/slot subject is suffixed by `suffix` — a family of
    /// alpha-variants differing ONLY in the bound-variable declaration's IRI (and its
    /// `rdfs:label`) can therefore be generated programmatically instead of
    /// hand-authoring near-duplicate fixtures.
    fn one_var_binder_variant_ttl(suffix: &str) -> String {
        format!(
            "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix op: <https://blackcatinformatics.ca/logic/dag/op/> .\n\
             ex:binder{suffix} a math:BindingExpression ;\n\
             \x20 math:operator op:forall ;\n\
             \x20 math:boundVariable ex:decl{suffix} ;\n\
             \x20 math:argumentSlot ex:bodySlot{suffix} .\n\
             ex:decl{suffix} a math:VariableDeclaration ; rdfs:label \"{suffix}\"@en .\n\
             ex:bodySlot{suffix} a math:ArgumentSlot ; math:slotIndex 0 ; \
             math:slotExpression ex:app{suffix} .\n\
             ex:app{suffix} a math:ApplicationExpression ; math:operator ex:p ; \
             math:argumentSlot ex:s0{suffix} .\n\
             ex:s0{suffix} a math:ArgumentSlot ; math:slotIndex 0 ; \
             math:slotExpression ex:occ{suffix} .\n\
             ex:occ{suffix} a math:VariableExpression ; math:variableOccurrence ex:o{suffix} .\n\
             ex:o{suffix} a math:VariableOccurrence ; math:declaredVariable ex:decl{suffix} .\n"
        )
    }

    fn one_var_binder_variant_root(suffix: &str) -> String {
        format!("https://example.org/binder{suffix}")
    }

    // ── 1. Alpha-equivalence: see the `interning` property-test module below ─────────
    //
    // The hand-enumerated "a handful of hardcoded suffixes share one digest" case that
    // used to live here is now STRICTLY SUBSUMED by
    // `interning::bound_variable_renaming_does_not_change_digest` (arbitrary generated
    // renamings, not five fixed strings) and `interning::shadowing_changes_binder_resolution_and_digest`
    // (the nested-shadowing case, generated rather than hand-authored) — both drive the
    // REAL `MathGraph`/`lower_math_expression` pipeline the same way this test did, over
    // a generated rather than enumerated input space. Deleted per the no-duplicate-of-
    // record standing instruction rather than kept alongside its own superset.

    /// The three committed `tests/conformance-fixtures/alpha-equivalent-*.ttl` fixtures
    /// use the `math:VariableExpression`-wrapped occurrence shape: a `math:slotExpression`
    /// pointed directly at a bare `math:VariableOccurrence` is a type error (a
    /// `math:VariableOccurrence` is not itself a `math:MathematicalExpression` per its own
    /// class definition — it enters the tree only through a `math:VariableExpression`'s
    /// `math:variableOccurrence` edge), so the fallback leaf branch now HARD-FAILS with
    /// [`MathLoweringError::UnrecognizedExpressionType`] on that shape rather than
    /// silently degrading it to an opaque IRI leaf. This asserts the REAL properties the
    /// fixtures document in their own header comments: pair-a and pair-b are genuinely
    /// α-equivalent (share one digest), and the shadowing fixture is deterministic and
    /// distinct from a bare one-variable binder.
    #[test]
    fn committed_alpha_equivalence_fixtures_are_genuinely_alpha_equivalent() {
        let sum_root = "http://example.org/math/sumBinder";
        let graph_a = MathGraph::from_turtle(ALPHA_PAIR_A.as_bytes()).expect("pair-a parses");
        let digest_a = arena_structural_key(&graph_a, sum_root).expect("pair-a lowers");

        let graph_b = MathGraph::from_turtle(ALPHA_PAIR_B.as_bytes()).expect("pair-b parses");
        let digest_b = arena_structural_key(&graph_b, sum_root).expect("pair-b lowers");

        assert_eq!(
            digest_a, digest_b,
            "alpha-equivalent-pair-a.ttl and alpha-equivalent-pair-b.ttl differ only in \
             their bound-variable declaration IRI and must share one structural digest"
        );

        let shadowing_root = "http://example.org/math/outerSum";
        let graph_shadow =
            MathGraph::from_turtle(ALPHA_SHADOWING.as_bytes()).expect("shadowing parses");
        let mut dag_shadow = TermDag::new();
        let node_shadow = lower_math_expression(&mut dag_shadow, &graph_shadow, shadowing_root)
            .expect("shadowing fixture lowers");
        let digest_shadow_1 = structural_digest(&dag_shadow, node_shadow);
        let digest_shadow_2 = arena_structural_key(&graph_shadow, shadowing_root)
            .expect("shadowing fixture lowers (second pass)");
        assert_eq!(
            digest_shadow_1, digest_shadow_2,
            "the committed shadowing fixture's digest is deterministic across separate lowerings"
        );
        assert_ne!(
            digest_shadow_1, digest_a,
            "the shadowing (nested binder) fixture is not alpha-equivalent to the bare \
             one-variable summation of pair-a/pair-b"
        );
    }

    // ── the leaf fallback HARD-FAILS an ill-typed node, never degrades it ─────────

    /// A `math:slotExpression` pointed directly at a bare `math:VariableOccurrence` (the
    /// SHAPE the committed alpha-equivalence fixtures originally used, before they were
    /// corrected to the `math:VariableExpression`-wrapped shape) is a genuine type error —
    /// `math:VariableOccurrence` is not itself a `math:MathematicalExpression`. It must be
    /// REJECTED, never silently accepted as an opaque IRI leaf keyed on the occurrence's
    /// own subject (which would let two non-alpha-equivalent expressions collide, or let
    /// an authored `math:structuralKey` claim an identity for a thing the grammar itself
    /// refutes).
    #[test]
    fn bare_variable_occurrence_as_slot_expression_hard_fails() {
        let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; \
             math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:occ .\n\
             ex:occ a math:VariableOccurrence ; math:declaredVariable ex:decl .\n\
             ex:decl a math:VariableDeclaration .\n";
        let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse");
        let mut dag = TermDag::new();
        let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
            .expect_err("a bare math:VariableOccurrence slot target must hard-fail");
        match &err {
            MathLoweringError::UnrecognizedExpressionType { node, types } => {
                assert_eq!(node, "https://example.org/occ");
                assert_eq!(
                    types,
                    &vec!["https://blackcatinformatics.ca/math/VariableOccurrence".to_owned()]
                );
            }
            other => panic!("expected UnrecognizedExpressionType, got {other:?}"),
        }
        assert_eq!(
            err.failure_class(),
            "https://blackcatinformatics.ca/math/UnrecognizedExpressionType"
        );
    }

    /// A node carrying a genuinely UNKNOWN/typo'd `math:` type (never authored anywhere
    /// in the `math:` vocabulary) used as a slot operand must hard-fail exactly the same
    /// way — the fallback is not merely a denylist of the classes this file happens to
    /// know about.
    #[test]
    fn typo_math_type_as_slot_expression_hard_fails() {
        let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; \
             math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:bogus .\n\
             ex:bogus a math:MathematicalSttaement .\n";
        let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse");
        let mut dag = TermDag::new();
        let err = lower_math_expression(&mut dag, &graph, "https://example.org/app").expect_err(
            "a typo'd math: class on a slot target must hard-fail, never silently \
                         degrade to an opaque leaf",
        );
        assert!(
            matches!(err, MathLoweringError::UnrecognizedExpressionType { .. }),
            "{err:?}"
        );
    }

    /// A `math:SymbolReference` leaf (the RECOGNIZED constant-operand type, e.g.
    /// `slices/grounding/math/examples/reference-ast-act.ttl`'s `ex:leftMatrixRef`) is
    /// still accepted through the fallback, interning its own IRI exactly like an
    /// untyped external constant — the stricter fallback rejects UNRECOGNIZED `math:`
    /// types, never every `math:` type whatsoever.
    #[test]
    fn symbol_reference_leaf_interns_on_its_symbol_not_its_own_iri() {
        // TWO independently authored copies of the same expression over the SAME symbols,
        // differing only in their occurrence-wrapper IRIs. This is the case the shipped
        // reference example cannot express, because it reuses one pair of occurrence nodes
        // across both of its expressions — holding constant the very IRIs the defect moved
        // with, which is why a digest keyed on the wrapper looked correct there.
        let ttl = |refl: &str, refr: &str, app: &str, s0: &str, s1: &str| {
            format!(
                "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:{app} a math:ApplicationExpression ; math:operator ex:p ; \
                 math:argumentSlot ex:{s0} , ex:{s1} .\n\
                 ex:{s0} a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:{refl} .\n\
                 ex:{s1} a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:{refr} .\n\
                 ex:{refl} a math:SymbolReference ; math:hasMathematicalSymbol ex:symL .\n\
                 ex:{refr} a math:SymbolReference ; math:hasMathematicalSymbol ex:symR .\n\
                 ex:symL a math:MathematicalSymbol .\n\
                 ex:symR a math:MathematicalSymbol .\n"
            )
        };
        let digest_of = |text: &str, root: &str| {
            let graph = MathGraph::from_turtle(text.as_bytes()).expect("parse");
            let mut dag = TermDag::new();
            let node = lower_math_expression(&mut dag, &graph, root).expect("lowers");
            structural_digest(&dag, node)
        };
        let a = digest_of(
            &ttl("refA0", "refA1", "appA", "sA0", "sA1"),
            "https://example.org/appA",
        );
        let b = digest_of(
            &ttl("refB0", "refB1", "appB", "sB0", "sB1"),
            "https://example.org/appB",
        );
        assert_eq!(
            a, b,
            "two independently authored copies of one expression over the SAME symbols must \
             intern to ONE key; a digest that moves with the occurrence-wrapper IRI is a label, \
             not a content key, and the alpha-equivalence contract is false"
        );

        // Different SYMBOLS must still separate — the fix must not collapse distinct content.
        let other = digest_of(
            &ttl("refA0", "refA1", "appA", "sA0", "sA1").replace("ex:symR", "ex:symZ"),
            "https://example.org/appA",
        );
        assert_ne!(
            a, other,
            "expressions over DIFFERENT symbols must not collide"
        );
    }

    // ── 2. Injectivity: structurally DISTINCT expressions get DIFFERENT digests ───────

    #[test]
    fn structurally_distinct_expressions_get_distinct_digests() {
        let mut dag = TermDag::new();

        // f(a, b) vs f(b, a): swapped slot indexes at the same operator — operand order
        // is always identity-bearing for a `math:ApplicationExpression`.
        let fab = MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (1, "ex:b")]).as_bytes())
            .expect("parse f(a,b)");
        let fba = MathGraph::from_turtle(application_ttl(&[(0, "ex:b"), (1, "ex:a")]).as_bytes())
            .expect("parse f(b,a)");
        let n_fab =
            lower_math_expression(&mut dag, &fab, "https://example.org/app").expect("f(a,b)");
        let n_fba =
            lower_math_expression(&mut dag, &fba, "https://example.org/app").expect("f(b,a)");

        // A DIFFERENT operator entirely, over the SAME operands/shape as f(a,b).
        let ttl_g = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:g ; \
             math:argumentSlot ex:s0 , ex:s1 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:a .\n\
             ex:s1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:b .\n";
        let g_graph = MathGraph::from_turtle(ttl_g.as_bytes()).expect("parse g(a,b)");
        let n_g =
            lower_math_expression(&mut dag, &g_graph, "https://example.org/app").expect("g(a,b)");

        // A different binder sort/domain over an otherwise identical binder shape.
        let untyped = MathGraph::from_turtle(binder_ttl(None).as_bytes()).expect("parse untyped");
        let typed =
            MathGraph::from_turtle(binder_ttl(Some("ex:Reals")).as_bytes()).expect("parse typed");
        let n_untyped = lower_math_expression(&mut dag, &untyped, "https://example.org/binder")
            .expect("untyped binder");
        let n_typed = lower_math_expression(&mut dag, &typed, "https://example.org/binder")
            .expect("typed binder");

        let labeled_digests = [
            ("f(a,b)", structural_digest(&dag, n_fab)),
            ("f(b,a)", structural_digest(&dag, n_fba)),
            ("g(a,b)", structural_digest(&dag, n_g)),
            ("untyped binder", structural_digest(&dag, n_untyped)),
            ("typed binder", structural_digest(&dag, n_typed)),
        ];
        for i in 0..labeled_digests.len() {
            for j in (i + 1)..labeled_digests.len() {
                let (name_i, digest_i) = &labeled_digests[i];
                let (name_j, digest_j) = &labeled_digests[j];
                assert_ne!(
                    digest_i, digest_j,
                    "{name_i} and {name_j} are structurally distinct and must get distinct \
                     digests"
                );
            }
        }
    }

    // ── 3. Interning: α-variants of ONE expression add nodes for the distinct ─────────
    // ──    structure only, never once per variant ─────────────────────────────────────

    #[test]
    fn alpha_variants_of_one_expression_intern_to_a_fixed_node_count() {
        let mut dag = TermDag::new();
        let len_before = dag.len();

        // A single one-variable-binder expression is built from exactly 6 distinct
        // constituent nodes (its `forall` op leaf, the untyped-individual sort leaf,
        // `p`'s op leaf, the bound occurrence, the `App` node, the `Binder` node
        // itself) — so this family must be STRICTLY larger than 6 for "grew by far
        // fewer nodes than variants lowered" to be a meaningful (non-vacuous) claim.
        let variants = [
            "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa",
        ];
        let mut nodes = Vec::new();
        for suffix in variants {
            let ttl = one_var_binder_variant_ttl(suffix);
            let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("variant parses");
            let root = one_var_binder_variant_root(suffix);
            let node = lower_math_expression(&mut dag, &graph, &root).expect("variant lowers");
            nodes.push(node);
        }

        // Every variant interns to the SAME node (hash-consing under alpha-equivalence).
        assert!(
            nodes.windows(2).all(|w| w[0] == w[1]),
            "all α-variants intern to one NodeId: {nodes:?}"
        );

        // The dag grew by the FIXED, small number of distinct constituent nodes a single
        // one-variable-binder expression is built from (its `forall` op leaf, the
        // untyped-individual sort leaf, `p`'s op leaf, the bound occurrence, the App
        // node, the Binder node) — strictly fewer nodes than the number of α-variants
        // lowered, never one new node per variant.
        let distinct_nodes_added = dag.len() - len_before;
        assert!(
            distinct_nodes_added > 0,
            "the dag actually grew by lowering the first variant"
        );
        assert!(
            distinct_nodes_added < variants.len(),
            "{distinct_nodes_added} distinct nodes added for {} α-variants of ONE expression — \
             must intern to far fewer nodes than variants lowered, never grow linearly with N",
            variants.len()
        );

        // Re-lowering the SAME shapes a second time must add ZERO new nodes at all (pure
        // re-interning) — proving the count above was not an accident of some
        // sub-structure not being fully shared.
        let stable_len = dag.len();
        for suffix in variants {
            let ttl = one_var_binder_variant_ttl(suffix);
            let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("variant re-parses");
            let root = one_var_binder_variant_root(suffix);
            let _ = lower_math_expression(&mut dag, &graph, &root).expect("variant re-lowers");
        }
        assert_eq!(
            dag.len(),
            stable_len,
            "re-lowering the same α-variants a second time adds ZERO new nodes"
        );
    }

    // ── 4. Contract-unchanged: the authored math:structuralKey is the REAL digest ─────

    #[test]
    fn reference_ast_act_structural_key_matches_recomputed_digest() {
        const NS: &str = "https://blackcatinformatics.ca/gmeow/examples/math/reference-act/";
        let dataset = purrdf::parse_dataset(REFERENCE_AST_ACT.as_bytes(), "text/turtle", None)
            .expect("parse");
        let keys = math_expression_structural_keys(&dataset);

        let ast_root = format!("{NS}matrixProductAst");
        let normal_root = format!("{NS}matrixProductNormalForm");
        let ast_digest = keys
            .get(&ast_root)
            .expect("matrixProductAst is a root")
            .as_ref()
            .expect("matrixProductAst lowers")
            .clone();
        let normal_digest = keys
            .get(&normal_root)
            .expect("matrixProductNormalForm is a root")
            .as_ref()
            .expect("matrixProductNormalForm lowers")
            .clone();

        // The two are declared `math:structuralNormalization`-equivalent (same
        // operator, same operand slots) — they really are the same structure, so the
        // same digest.
        assert_eq!(
            ast_digest, normal_digest,
            "matrixProductAst and matrixProductNormalForm must share one structural digest"
        );

        // The authored `math:structuralKey` on BOTH expressions must be the REAL,
        // recomputed digest — never the known placeholder.
        let graph = MathGraph::from_turtle(REFERENCE_AST_ACT.as_bytes()).expect("parse graph");
        let authored_ast_key = graph
            .first_lit_typed(&ast_root, M_STRUCTURAL_KEY)
            .expect("matrixProductAst carries a math:structuralKey")
            .0
            .to_owned();
        let authored_normal_key = graph
            .first_lit_typed(&normal_root, M_STRUCTURAL_KEY)
            .expect("matrixProductNormalForm carries a math:structuralKey")
            .0
            .to_owned();

        assert_ne!(
            authored_ast_key, "placeholder-alpha-equivalent-digest-v1",
            "the authored math:structuralKey must be the REAL digest, not the known placeholder"
        );
        assert_eq!(
            authored_ast_key, ast_digest,
            "the authored math:structuralKey on matrixProductAst must match the recomputed \
             digest"
        );
        assert_eq!(
            authored_normal_key, normal_digest,
            "the authored math:structuralKey on matrixProductNormalForm must match the \
             recomputed digest"
        );
    }
}
