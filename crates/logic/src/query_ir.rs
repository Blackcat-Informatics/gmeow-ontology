// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal IR, parser, and shared answer types for `.logic` query programs.
//!
//! # Grammar overview
//!
//! A `.logic` file is a small Prolog-ish program supporting:
//! - Comments: lines starting with `%` (and blank lines) are ignored.
//! - Prefix declarations: `:- prefix(ex, 'https://example.org/').`
//! - Rules: `head :- body1, body2, ... .` and facts `head.`
//! - Goal: exactly one `?- goalatom1, goalatom2, ... .`
//! - Atoms are binary predicates over RDF: `pred(Subject, Object)`.
//! - Negation-as-failure: a body literal `\+ pred(S, O)` or `not pred(S, O)` is parsed
//!   as a [`QBodyLit::Neg`] (stratified NAF, evaluated by the native binary core).
//! - Cut: the body literal `!` is parsed as a [`QBodyLit::Cut`] marker.
//!
//! # Canonicalization
//!
//! - Subject/object constants: IRI → `<iri>` (angle-bracket form).
//! - Predicate IRIs: bare IRI string, no angle brackets.
//! - Variables: any token starting with uppercase or `_`.

use std::collections::{BTreeMap, BTreeSet};

use crate::physical::id::NodeId;
use crate::physical::term_dag::ArenaId;
use crate::seam::BudgetStatus;

/// Wrap a query-program parse condition message as a typed diagnostic on the
/// shared substrate, preserving the authored text verbatim.
fn query_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Query { detail })
}

/// The reserved relation name of the arity-4 predicate-as-data encoding
/// `triple(subject, predicate, object, world)` — the REAL n-ary shape the binary
/// [`crate::physical::store::RelationStore`] cannot represent (the property rides in a DATA
/// position).  A goal or rule that names this bare, unqualified relation is routed to
/// the arity-generic n-ary evaluator, whose generic-triple EDB
/// ([`crate::physical::magic_generic`]) loads every world fact under this exact
/// relation name.  It is DELIBERATELY the bare symbol `triple`, distinct from any
/// prefixed predicate `ex:triple` (which resolves to a full IRI): only the
/// unqualified name reaches the generic evaluator, so the parser must accept it
/// verbatim (a bare word is otherwise unresolvable — no prefix, no angle brackets).
pub(crate) const GENERIC_TRIPLE_RELATION: &str = "triple";

// ── IR types ──────────────────────────────────────────────────────────────────

/// A term in a query atom: either a canonical constant string or a variable name.
///
/// For IRI constants the canonical form is `<iri>` (angle brackets).
/// For variables the string is the variable name as written in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QTerm {
    /// A canonical constant string.
    /// - IRI: `<https://example.org/alice>`
    Const(String),
    /// A logic variable name, e.g. `X`, `_Y`.
    Var(String),
    /// An integer literal term (arithmetic operand), e.g. `0`, `1` (G2a).
    ///
    /// Distinct from `Const("\"0\"^^<…#integer>")`: a `Num` is a *bare* arithmetic
    /// operand that the native builtin evaluator consumes, never a quoted atom.
    Num(i64),
    /// A compound (function-symbol) term interned in the structured-term DAG — the
    /// full-FOL surface a `Const`/`Var`/`Num` cannot express (e.g. `s(X)`,
    /// `cons(H, T)`).  The payload is an OPAQUE [`StructNode`] wrapping the term's
    /// [`NodeId`] in the resolver's [`crate::physical::term_dag::TermDag`]; because a
    /// `NodeId` is a crate-internal runtime handle meaningful ONLY within the DAG that
    /// minted it, it is wrapped so it never crosses the public API (the wrapper's inner
    /// handle is private to the crate). A `Struct` term always travels with that DAG (the
    /// structured backward resolver [`crate::physical::resolve_fol`] owns it). A goal, rule
    /// head, or rule body atom carrying ANY `Struct` argument is a *structured* program and
    /// is routed to the full-FOL resolver; a program with only `Const`/`Var`/`Num`
    /// arguments is *flat* and stays on the byte-identical binary magic path. The parser
    /// produces only flat terms — the `Struct` arm is constructed solely inside the crate
    /// against a live DAG.
    Struct(StructNode),
    /// A **ground** RDF 1.2 quoted-triple term used as an atom argument, written on the
    /// query surface as `<<( s p o )>>` (mirroring the [`crate::provenance::term_display`]
    /// render form). Its components are themselves ground `QTerm`s (an IRI/prefixed name,
    /// a literal, or a nested triple); the predicate must be an IRI (RDF 1.2). Unlike
    /// [`QTerm::Struct`] it is NOT a full-FOL compound term — it lowers to a flat constant
    /// [`crate::rule_ir::EvalTerm::ConstLit`] carrying a `purrdf::TermValue::Triple`, so a
    /// triple-bearing goal stays on the flat/generic path (never routed to the full-FOL
    /// resolver) and reaches an external-relation provider as a bound query term. Embedded
    /// variables (a triple *pattern*) are a distinct, unsupported semantics: the parser
    /// hard-fails on one rather than silently degrade.
    Triple {
        /// The subject term (ground).
        s: Box<QTerm>,
        /// The predicate term (ground; must be an IRI).
        p: Box<QTerm>,
        /// The object term (ground).
        o: Box<QTerm>,
    },
}

/// An opaque handle to a compound-term node in the structured-term DAG, carried by
/// [`QTerm::Struct`].
///
/// It wraps a [`crate::physical::term_dag::TermDag`] [`NodeId`] — a crate-internal runtime
/// handle — behind a PRIVATE field, so the handle stays out of the public API surface even
/// though [`QTerm`] (and hence [`QProgram`]) is `pub`. Only the crate can mint one (from a
/// live DAG) or read the wrapped node, matching the doctrine that a `NodeId` is never a
/// serialized/public identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructNode(NodeId, ArenaId);

impl StructNode {
    /// Wrap a DAG node handle as a structured-term payload (crate-internal — only a live DAG
    /// mints a `NodeId`), branded with the arena that minted it.
    ///
    /// The `arena` brand ([`crate::physical::term_dag::TermDag::arena`]) travels WITH the node
    /// so a later membership test ([`crate::physical::term_dag::TermDag::contains_node`]) is an
    /// arena-identity check, not a numeric index-range coincidence — a node from a foreign DAG
    /// is rejected even when its index happens to fall in the target arena's range.
    ///
    /// The parser produces only flat terms, so a `Struct` is constructed exclusively by
    /// crate-internal callers holding a live DAG — currently the resolver's own tests and the
    /// shipped structured demonstrators; the flat production surface never reaches it.
    #[allow(dead_code)]
    pub(crate) fn new(node: NodeId, arena: ArenaId) -> Self {
        Self(node, arena)
    }

    /// The wrapped DAG node handle.
    pub(crate) fn node(self) -> NodeId {
        self.0
    }

    /// The brand of the arena that minted [`Self::node`].
    pub(crate) fn arena(self) -> ArenaId {
        self.1
    }
}

/// A predicate atom over RDF (or an n-ary IDB predicate).
///
/// `pred(Subject, Object)` maps to the triple `(Subject, predIRI, Object)`. EDB
/// (RDF) atoms are binary; IDB predicates may have any arity ≥ 1 (e.g. `get/3`
/// for list indexing — G2a). `pred` is the bare IRI string (no angle brackets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QAtom {
    /// Bare IRI string for the predicate (no angle brackets).
    pub pred: String,
    /// The argument terms (≥ 1). Binary EDB atoms carry `[subject, object]`.
    pub args: Vec<QTerm>,
}

/// A body literal in a rule: a predicate atom, the cut marker, or an arithmetic /
/// comparison builtin (G2a, `logic:builtinArithmetic`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QBodyLit {
    /// A normal predicate atom.
    Atom(QAtom),
    /// A negated body atom (negation-as-failure), written `\+ pred(a, b)` or
    /// `not pred(a, b)` on the query surface (the Prolog-ish backward mirror of the
    /// forward rule-text `~pred` negation).
    ///
    /// Stratified NAF: the negated atom blocks the rule iff some grounding of it is
    /// PRESENT in the accumulated least model of a strictly-lower stratum. It binds no
    /// new variables (NAF is a test, not a generator), and every variable it carries
    /// must be range-restricted by a positive body atom — an unbound variable under
    /// negation flounders (an unsound NAF goal) and is a declared native gap.
    Neg(QAtom),
    /// Retired cut syntax `!`, retained only for a typed rejection diagnostic.
    Cut,
    /// An arithmetic (`X is Expr`) or comparison (`L cmp R`) builtin.
    ///
    /// Gated to `ProceduralPrologProfile` and evaluated by the native physical
    /// core. Used to compute over `rdf:first`/`rdf:rest` chains.
    Builtin(QBuiltin),
}

/// An arithmetic or comparison builtin (bounded — no recursive expression AST).
///
/// Operands are `QTerm` (`Var` or `Num`; a `Const` IRI is invalid in arithmetic
/// and becomes a native filter failure if reached).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QBuiltin {
    /// `target is expr` — arithmetic evaluation, where `expr` is either one operand or
    /// `lhs op rhs` (`op` ∈ `+ - * //`). A single operand is canonically lowered to
    /// `operand + 0`, keeping one bounded arithmetic IR and evaluator path.
    Is {
        /// The variable (or operand) that receives the computed value.
        target: QTerm,
        /// Left arithmetic operand.
        lhs: QTerm,
        /// The arithmetic operator.
        op: ArithOp,
        /// Right arithmetic operand.
        rhs: QTerm,
    },
    /// `lhs cmp rhs` — arithmetic comparison (cmp ∈ `> < >= =< =:=`).
    Compare {
        /// Left comparison operand.
        lhs: QTerm,
        /// The comparison operator.
        op: CmpOp,
        /// Right comparison operand.
        rhs: QTerm,
    },
    /// `target is bilinearSqDist(gram, x, y)` — the exact bilinear-form squared
    /// distance `(x − y)ᵀ G (x − y)` over exact ℚ (no √; √ stays a seam).
    ///
    /// A named 3-ary math builtin. `gram` is an IRI to an authored `math:GramMatrix`,
    /// and `x`/`y` are IRIs to authored `math:` vectors (each operand is a `Const` IRI,
    /// or a `Var` bound to an IRI). The moded evaluator loads the exact-rational Gram
    /// cells and coordinate vectors from the graph and binds `target` to the exact
    /// `Value::Rat` squared distance (or filters when `target` is already bound). This
    /// is the first entry of a table-driven family of `math:` moded builtins.
    BilinearSqDist {
        /// The variable (or operand) that receives the exact squared distance.
        target: QTerm,
        /// The `math:GramMatrix` IRI operand (the symmetric bilinear form).
        gram: QTerm,
        /// The first `math:` vector IRI operand.
        x: QTerm,
        /// The second `math:` vector IRI operand.
        y: QTerm,
    },
    /// `dimEqual(d1, d2)` — the `math:dimensionEqualityRel` builtin-bound consequent of
    /// `math:dimensionalHomogeneityLaw`: exact ℚ⁷ commensurability of the two dimension
    /// IRI operands (each a `Const` IRI, or a `Var` bound to one). The moded evaluator
    /// resolves each operand's exact-rational exponent vector on demand via the
    /// [`crate::physical::CellResolver`]'s dimension probe — never a transport-literal
    /// parse — so a plain dimension IRI (never asserted as a scalar) is the operand
    /// shape. A LOWERING-ONLY builtin: never authored on the query surface, emitted
    /// solely by the `logic:Constraint` → violation-rule lowering
    /// ([`crate::relational_core::lower_constraint_violation_rules`]).
    DimEqual {
        /// The first dimension IRI operand.
        d1: QTerm,
        /// The second dimension IRI operand.
        d2: QTerm,
    },
    /// `dimProduct(dF, dM, dR)` — the `math:dimensionProductRel` builtin-bound
    /// consequent of `math:integralDimensionCompositionLaw`: `dR`'s exact ℚ⁷ exponent
    /// vector must equal `dF`'s composed (⊕, vector addition) with `dM`'s. Each operand
    /// is a dimension IRI (a `Const`, or a `Var` bound to one), resolved on demand via
    /// the [`crate::physical::CellResolver`]'s dimension probe. A LOWERING-ONLY
    /// builtin, exactly like [`Self::DimEqual`] — never authored, only emitted by the
    /// constraint lowering.
    DimProduct {
        /// The integrand's dimension IRI operand.
        d_f: QTerm,
        /// The measure's dimension IRI operand.
        d_m: QTerm,
        /// The declared result dimension IRI operand.
        d_r: QTerm,
    },
}

/// Arithmetic operators recognized in `X is Expr` builtins.
///
/// Operator identity is stable across the exact-numeric value tower: `+ - *` are
/// shared across ℤ and ℚ, [`ArithOp::Div`] (`//`) is truncating-integer division
/// (the ℤ operator), and [`ArithOp::ExactDiv`] (`/`) is exact rational division (the
/// ℚ operator). `//` and `/` are DISTINCT operators — `//` truncates toward zero on
/// integers, `/` yields the exact rational quotient — resolving the historical
/// ℤ/ℚ division overload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    /// Addition (`+`).
    Add,
    /// Subtraction (`-`).
    Sub,
    /// Multiplication (`*`).
    Mul,
    /// Truncating integer division (`//`, toward zero) — the ℤ division operator.
    Div,
    /// Exact rational division (`/`) — the ℚ division operator, distinct from `//`.
    ExactDiv,
}

impl ArithOp {
    /// The native Prolog infix token for this operator.
    pub fn token(self) -> &'static str {
        match self {
            ArithOp::Add => "+",
            ArithOp::Sub => "-",
            ArithOp::Mul => "*",
            ArithOp::Div => "//",
            ArithOp::ExactDiv => "/",
        }
    }
}

/// Comparison operators recognized in `L cmp R` builtins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    /// Greater than (`>`).
    Gt,
    /// Less than (`<`).
    Lt,
    /// Greater than or equal (`>=`).
    Ge,
    /// Less than or equal (`=<`).
    Le,
    /// Arithmetic equality (`=:=`).
    Eq,
}

impl CmpOp {
    /// The native Prolog infix token for this comparison.
    pub fn token(self) -> &'static str {
        match self {
            CmpOp::Gt => ">",
            CmpOp::Lt => "<",
            CmpOp::Ge => ">=",
            CmpOp::Le => "=<",
            CmpOp::Eq => "=:=",
        }
    }
}

/// A rule: `head :- body1, body2, ... .`  or a fact `head.` (empty body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QRule {
    /// The rule head atom.
    pub head: QAtom,
    /// Body literals (empty for facts).
    pub body: Vec<QBodyLit>,
}

/// The conjunctive goal: `?- atom1, atom2, ... .`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QGoal {
    /// Goal atoms (conjuncts), left to right.
    pub atoms: Vec<QAtom>,
}

/// A Stratum-C counterfactual declaration parsed from `.logic` directives.
///
/// A counterfactual query departs from a **base world** `W_base`, admits a
/// hypothetical **antecedent** `A` (one or more ground atoms) via a deterministic
/// AGM revision, and resolves the program's goal `φ` inside the **constructed
/// world** `W_cf` — an isolated, transient named graph that never leaks back into
/// the base store. The closeness/entrenchment ordering and the revision itself are
/// applied by [`crate::counterfactual`] at resolution time; this struct only carries
/// the declared inputs.
///
/// Surface (parsed from query-layer directives, NOT ontology terms):
/// - `:- counterfactual(<W_cf>, <W_base>).` — declare the constructed and base worlds.
/// - `:- assume(pred(S, O)).` — one antecedent atom `A` (repeatable; must be ground).
/// - `:- depth_budget(N).` — optional hard cap on nested-counterfactual depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QCounterfactual {
    /// The constructed counterfactual world IRI `W_cf` (canonical `<iri>` form).
    pub cf_world: String,
    /// The base world IRI `W_base` the revision departs from (canonical `<iri>` form).
    pub base_world: String,
    /// The antecedent `A`: ground atoms admitted into `W_cf`. Each maps to a triple.
    pub antecedent: Vec<QAtom>,
    /// Optional hard cap on nested-counterfactual depth; `None` uses the engine default.
    pub depth_budget: Option<u64>,
}

/// An independent probabilistic fact, declared by the query-layer directive
/// `:- probability(pred(S, O), p).`
///
/// The ground atom `pred(S, O)` is an independent Bernoulli variable that is true
/// with probability `prob` and false with probability `1 - prob`. The probability is
/// carried as its raw decimal token (parsed to `f64` by the evaluator) so this IR
/// stays `Eq`/hashable and preserves the source text exactly.
///
/// Surface (a query-layer directive, NOT an ontology term — exactly as
/// `:- counterfactual(...)` is the surface for `logic:counterfactualOf`, this is the
/// surface for `logic:probability` under `logic:FullIndependence`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QProbFact {
    /// The ground atom this probability annotates.
    pub atom: QAtom,
    /// Raw decimal token for the probability in `[0, 1]` (e.g. `"0.7"`).
    pub prob: String,
}

/// One row of a dependency model's explicit joint table, declared by
/// `:- joint(p, atom1, atom2, ...).`
///
/// The listed atoms are exactly those TRUE in this joint outcome; every other
/// correlated atom of the model is false in this outcome. `prob` is the joint
/// probability mass of this exact assignment (the surface for `logic:JointOutcome`
/// with `logic:jointProbability`). Outcomes must be mutually exclusive and their
/// probabilities must sum to one (checked by the evaluator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QJointOutcome {
    /// Ground atoms true in this outcome (the positive set; complement = false).
    pub true_atoms: Vec<QAtom>,
    /// Raw decimal token for this outcome's joint probability.
    pub prob: String,
}

/// The declared probability model governing probabilistic inference — the
/// surface for `logic:ProbabilityModel`.
///
/// Probabilistic inference requires one of these to be declared; with no model the
/// evaluator refuses (returns `unknown`) rather than assuming independence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QProbModel {
    /// `logic:FullIndependence`: every probabilistic fact is independent.
    FullIndependence,
    /// `logic:DependencyModel`: an explicit joint over a correlated fact set.
    Dependency {
        /// The joint table rows.
        joints: Vec<QJointOutcome>,
    },
}

/// A confidence annotation, declared by `:- confidence(pred(S, O), c).`
///
/// Carried for completeness and to make the confidence≠probability guard concrete:
/// a confidence-annotated atom is an asserted (deterministic) fact whose annotation
/// is metadata — the evaluator NEVER reads `confidence` as a probability. If a wrong
/// implementation promoted it, the conformance guard goes red.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QConfidence {
    /// The ground atom the confidence annotates (an asserted fact).
    pub atom: QAtom,
    /// Raw decimal token for the confidence in `[0, 1]` (NEVER a probability).
    pub confidence: String,
}

/// A complete parsed program: a set of rules and exactly one goal.
///
/// Prefix declarations are consumed during parsing; the resulting IRIs are
/// fully expanded in all atoms before this struct is constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QProgram {
    /// Rules and facts, in source order.
    pub rules: Vec<QRule>,
    /// The single conjunctive goal.
    pub goal: QGoal,
    /// `Some` iff this is a Stratum-C counterfactual query; `None` for a
    /// plain v4 backward goal resolved directly against the materialized world.
    pub counterfactual: Option<QCounterfactual>,
    /// Independent probabilistic facts declared by `:- probability(...)`.
    pub prob_facts: Vec<QProbFact>,
    /// The declared probability model, if any (`:- probability_model(...)`).
    pub prob_model: Option<QProbModel>,
    /// Confidence annotations declared by `:- confidence(...)` — asserted
    /// facts whose confidence is metadata, never a probability.
    pub confidences: Vec<QConfidence>,
}

// ── Answer types ──────────────────────────────────────────────────────────────

/// A single variable binding: variable name → canonical constant string.
pub type Binding = BTreeMap<String, String>;

/// The completion frontier of a native (semi-naive) evaluation — the public,
/// crate-external projection of the physical governor's `StrataProgress` plus its
/// committed step count.
///
/// The least model is built stratum-by-stratum in a fixed order.  When a step budget
/// exhausts inside stratum *k*, every predicate at a stratum `< k` has its **final**
/// least-model extension, so the run is *incomplete, never wrong*: [`Self::completed`]
/// records how many strata reached their natural fixpoint, and [`Self::saturated_preds`]
/// names the predicates whose extension is settled.  A consumer reads this to tell a
/// budget-cut partial result (`completed < total`) from a genuinely complete one.
///
/// Paths that never run the native governor (the ungoverned well-founded,
/// cautious-stable, and echo materializers)
/// carry the empty frontier ([`Self::empty`]) so the field is *always present* — a
/// consumer never has to assume "no frontier ⇒ complete".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompletionFrontier {
    /// The number of strata fully saturated.  Strata `0..completed` ran to their
    /// natural fixpoint; a stratum at index `completed`, if any, was cut mid-fixpoint.
    pub completed: usize,
    /// The total number of strata in the program.
    pub total: usize,
    /// The predicates whose extension is final: the heads of the saturated strata plus
    /// every EDB predicate.  Sorted (a `BTreeSet`), so output is deterministic.
    pub saturated_preds: BTreeSet<String>,
    /// The number of committed derivations (deterministic; a cost probe — identical
    /// inputs ⇒ identical count).
    pub consumed_steps: u64,
}

impl CompletionFrontier {
    /// The empty frontier for a non-governed / EDB-only path: nothing counted, nothing
    /// declared saturated.  Distinct from a *complete* frontier (`completed == total`),
    /// which a governed run reports when it reached its natural fixpoint within budget.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }
}

/// The result of resolving a [`QProgram`] against a world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerSet {
    /// All goal-variable bindings, in deterministic (sorted) order.
    pub bindings: Vec<Binding>,
    /// Whether resolution completed within budget.
    pub status: BudgetStatus,
    /// The preservation judgment disclosing any formulas the evaluation could not
    /// carry (downstream disclosure). Native backward dispatch is a faithful
    /// evaluator, so this is `{exact}` with an empty unsupported set;
    /// budget-incompleteness is carried on [`Self::status`], not here. The field is
    /// always present so a consumer can uniformly read the disclosure on every
    /// answer surface, never having to assume "no lowering ⇒ nothing dropped".
    pub preservation: crate::result::PreservationClaim,
    /// The completion frontier of the native evaluation: which strata / predicates the
    /// governor settled and how many derivations it committed.
    pub frontier: CompletionFrontier,
}

impl AnswerSet {
    /// Sort `bindings` deterministically so output is stable across runs.
    ///
    /// BTreeMaps are already ordered by key; we sort the `Vec` by the
    /// serialized key/value pairs of each binding map.
    pub fn canonicalize(&mut self) {
        self.bindings.sort_by(|a, b| {
            // Compare lexicographically by (key, value) pairs in order.
            let a_pairs: Vec<(&String, &String)> = a.iter().collect();
            let b_pairs: Vec<(&String, &String)> = b.iter().collect();
            a_pairs.cmp(&b_pairs)
        });
    }
}

/// Execution budget for resolution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Budget {
    /// Maximum number of answer bindings to collect before stopping with `Partial`.
    pub max_answers: Option<usize>,
    /// Maximum resolution steps before stopping with `Exhausted`.
    pub max_steps: Option<u64>,
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parse a `.logic` query program from source text.
///
/// The parser:
/// 1. Strips comment lines (`%`) and blank lines.
/// 2. Joins physical lines into logical clauses split by `.`.
/// 3. Dispatches each clause to the appropriate handler.
///
/// # Errors
///
/// Returns `Err` on any malformed input.  Exactly one `?-` goal is
/// required; zero or more than one is an error.
///
/// # Panics
///
/// Never panics — all errors are returned as a typed diagnostic.
pub fn parse_query_program(src: &str) -> gmeow_errors::Result<QProgram> {
    let mut prefixes: BTreeMap<String, String> = BTreeMap::new();
    let mut rules: Vec<QRule> = Vec::new();
    let mut goal: Option<QGoal> = None;

    // ── Stratum-C counterfactual accumulators ─────────────────────────
    // Populated by the `counterfactual(...)`, `assume(...)`, and `depth_budget(...)`
    // directives. `cf_worlds` is Some once a `counterfactual(...)` directive is seen.
    let mut cf_worlds: Option<(String, String)> = None;
    let mut cf_antecedent: Vec<QAtom> = Vec::new();
    let mut cf_depth_budget: Option<u64> = None;

    // ── Probabilistic accumulators ────────────────────────────────────
    // `prob_model_kind` is the bare keyword from `:- probability_model(...)`;
    // joints accumulate from `:- joint(...)` rows; prob_facts/confidences from
    // `:- probability(...)` / `:- confidence(...)`.
    let mut prob_model_kind: Option<String> = None;
    let mut prob_joints: Vec<QJointOutcome> = Vec::new();
    let mut prob_facts: Vec<QProbFact> = Vec::new();
    let mut confidences: Vec<QConfidence> = Vec::new();

    // ── Phase 1: collect raw logical clauses ─────────────────────────────────
    // We join continuation lines into complete clauses terminated by `.`.
    let mut pending = String::new();
    let mut clauses: Vec<String> = Vec::new();

    for line in src.lines() {
        let trimmed = line.trim();
        // Skip comments and blank lines (only at top-level, not inside a pending clause).
        if pending.is_empty() && (trimmed.is_empty() || trimmed.starts_with('%')) {
            continue;
        }
        if !pending.is_empty() {
            pending.push(' ');
        }
        pending.push_str(trimmed);

        // A clause ends at a `.` that is not inside a quoted string.
        // We collect whole clauses (terminated by `.`) from `pending`.
        while let Some(dot_pos) = find_clause_end(&pending) {
            let clause = pending[..dot_pos].trim().to_owned();
            pending = pending[dot_pos + 1..].trim().to_owned();
            // Skip empty clauses (e.g. trailing dots).
            if !clause.is_empty() {
                clauses.push(clause);
            }
        }
    }

    // If there's a non-empty pending without a terminating dot, it's a parse error.
    if !pending.trim().is_empty() {
        return Err(query_err(format!(
            "unterminated clause (missing '.'): {:?}",
            pending.trim()
        )));
    }

    // ── Phase 2: dispatch each clause ────────────────────────────────────────
    for clause in clauses {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }

        if let Some(body) = clause.strip_prefix(":-") {
            // Directive. Recognized forms: prefix / counterfactual / assume / depth_budget.
            let body = body.trim();
            if let Some(pfx) = parse_prefix_directive(body)? {
                prefixes.insert(pfx.0, pfx.1);
            } else if body.starts_with("counterfactual(") {
                if cf_worlds.is_some() {
                    return Err(query_err(
                        "program has more than one counterfactual(...) directive".to_owned(),
                    ));
                }
                cf_worlds = Some(parse_counterfactual_directive(body, &prefixes)?);
            } else if body.starts_with("assume(") {
                cf_antecedent.push(parse_assume_directive(body, &prefixes)?);
            } else if body.starts_with("depth_budget(") {
                if cf_depth_budget.is_some() {
                    return Err(query_err(
                        "program has more than one depth_budget(...) directive".to_owned(),
                    ));
                }
                cf_depth_budget = Some(parse_depth_budget_directive(body)?);
            } else if body.starts_with("probability_model(") {
                if prob_model_kind.is_some() {
                    return Err(query_err(
                        "program has more than one probability_model(...) directive".to_owned(),
                    ));
                }
                prob_model_kind = Some(parse_probability_model_directive(body)?);
            } else if body.starts_with("probability(") {
                prob_facts.push(parse_probability_directive(body, &prefixes)?);
            } else if body.starts_with("joint(") {
                prob_joints.push(parse_joint_directive(body, &prefixes)?);
            } else if body.starts_with("confidence(") {
                confidences.push(parse_confidence_directive(body, &prefixes)?);
            } else {
                // An unrecognized directive is an error, not a no-op: silently
                // ignoring one means a typo (e.g. `:- depth_buget(...)`) would
                // disable an intended guardrail without any signal.
                return Err(query_err(format!("unrecognized directive: {body:?}")));
            }
        } else if let Some(goal_body) = clause.strip_prefix("?-") {
            // Goal clause.
            if goal.is_some() {
                return Err(query_err("program has more than one ?- goal".to_owned()));
            }
            let goal_body = goal_body.trim();
            let atoms = parse_atom_list(goal_body, &prefixes)?;
            if atoms.is_empty() {
                return Err(query_err("?- goal must have at least one atom".to_owned()));
            }
            goal = Some(QGoal { atoms });
        } else {
            // Rule or fact.
            let rule = parse_rule(clause, &prefixes)?;
            rules.push(rule);
        }
    }

    let goal = goal.ok_or_else(|| query_err("program has no ?- goal".to_owned()))?;

    // ── Assemble the optional counterfactual declaration ─────────────────────
    let counterfactual = match cf_worlds {
        Some((cf_world, base_world)) => {
            // A counterfactual must admit at least one antecedent fact: an empty
            // `A` is a no-op revision (it asserts nothing hypothetical), almost
            // always a malformed query (the `assume(...)` was forgotten or typo'd).
            if cf_antecedent.is_empty() {
                return Err(query_err(
                    "counterfactual(...) directive requires at least one assume(...) antecedent"
                        .to_owned(),
                ));
            }
            // Antecedent atoms must be ground — `A` is a concrete hypothetical fact,
            // not a query pattern. Reject any variable to keep the revision deterministic.
            for atom in &cf_antecedent {
                if atom.args.iter().any(|t| matches!(t, QTerm::Var(_))) {
                    return Err(query_err(format!(
                        "assume(...) antecedent atom must be ground (no variables): {:?}",
                        atom.pred
                    )));
                }
            }
            Some(QCounterfactual {
                cf_world,
                base_world,
                antecedent: cf_antecedent,
                depth_budget: cf_depth_budget,
            })
        }
        None => {
            // `assume`/`depth_budget` without `counterfactual` is a malformed program:
            // they are meaningless outside a Stratum-C query.
            if !cf_antecedent.is_empty() {
                return Err(query_err(
                    "assume(...) directive present without a counterfactual(...) directive"
                        .to_owned(),
                ));
            }
            if cf_depth_budget.is_some() {
                return Err(query_err(
                    "depth_budget(...) directive present without a counterfactual(...) directive"
                        .to_owned(),
                ));
            }
            None
        }
    };

    // ── Assemble the optional probability model ───────────────────────
    let prob_model = match prob_model_kind.as_deref() {
        Some("full_independence") => {
            // Joints are meaningless without a dependency model: a `joint(...)`
            // under full independence is a malformed declaration, not a no-op.
            if !prob_joints.is_empty() {
                return Err(query_err(
                    "joint(...) directive present under probability_model(full_independence); \
                     joint tables require probability_model(dependency)"
                        .to_owned(),
                ));
            }
            Some(QProbModel::FullIndependence)
        }
        Some("dependency") => {
            // A dependency model must carry at least one joint outcome — an empty
            // joint table declares no distribution at all.
            if prob_joints.is_empty() {
                return Err(query_err(
                    "probability_model(dependency) requires at least one joint(...) outcome"
                        .to_owned(),
                ));
            }
            Some(QProbModel::Dependency {
                joints: prob_joints,
            })
        }
        Some(other) => {
            return Err(query_err(format!(
                "unknown probability_model kind {other:?} \
                 (expected 'full_independence' or 'dependency')"
            )));
        }
        None => {
            // A joint(...) without a declared dependency model is malformed: the
            // joints have no model to belong to. (probability(...) facts WITHOUT a
            // model are allowed here — the evaluator turns that into the required
            // `unknown` refusal, the no-model guard.)
            if !prob_joints.is_empty() {
                return Err(query_err(
                    "joint(...) directive present without a probability_model(dependency) directive"
                        .to_owned(),
                ));
            }
            None
        }
    };

    Ok(QProgram {
        rules,
        goal,
        counterfactual,
        prob_facts,
        prob_model,
        confidences,
    })
}

// ── Clause-end detector ───────────────────────────────────────────────────────

/// Find the position of the first clause-terminating `.` in `s`.
///
/// Skips `.` inside single-quoted strings (`'...'`) to avoid splitting on
/// IRIs with dots.  Returns the byte index of the `.` or `None`.
fn find_clause_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                // Skip single-quoted string.
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1; // skip escaped char
                    }
                    i += 1;
                }
                i += 1; // skip closing quote
            }
            b'.' => {
                // A clause-terminating `.` is followed by whitespace or end-of-input
                // (the standard Prolog convention). A `.` that is part of a decimal
                // literal (`0.5`) or an angle-bracketed IRI (`<…/a.b>`) is followed by
                // a non-space character and is NOT a terminator. Dots inside
                // single-quoted IRIs are already skipped by the quote branch above.
                match bytes.get(i + 1) {
                    None => return Some(i),
                    Some(c) if c.is_ascii_whitespace() => return Some(i),
                    _ => {
                        i += 1;
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

// ── Prefix directive parser ───────────────────────────────────────────────────

/// Parse a `prefix(alias, 'iri')` body (the part after `:-`).
///
/// Returns `Some((alias, iri))` on match, `None` if it's a different directive.
fn parse_prefix_directive(body: &str) -> gmeow_errors::Result<Option<(String, String)>> {
    // Expected form: `prefix(alias, 'https://...')`
    let body = body.trim();
    if !body.starts_with("prefix(") {
        return Ok(None);
    }
    let inner = body
        .strip_prefix("prefix(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| query_err(format!("malformed prefix directive: {body:?}")))?;

    let comma = inner
        .find(',')
        .ok_or_else(|| query_err(format!("prefix directive missing comma: {body:?}")))?;
    let alias = inner[..comma].trim().to_owned();
    let iri_part = inner[comma + 1..].trim();

    // IRI must be single-quoted. The `len() < 2` guard rejects a lone `'`: it
    // satisfies both `starts_with`/`ends_with`, but `iri_part[1..len-1]` would be
    // `[1..0]` and panic on the out-of-bounds slice.
    if !iri_part.starts_with('\'') || !iri_part.ends_with('\'') || iri_part.len() < 2 {
        return Err(query_err(format!(
            "prefix IRI must be single-quoted in: {body:?}"
        )));
    }
    let iri = iri_part[1..iri_part.len() - 1].to_owned();

    if alias.is_empty() {
        return Err(query_err(format!("prefix alias is empty in: {body:?}")));
    }
    if iri.is_empty() {
        return Err(query_err(format!("prefix IRI is empty in: {body:?}")));
    }

    Ok(Some((alias, iri)))
}

// ── Counterfactual directive parsers ──────────────────────────────────

/// Parse a `counterfactual(<W_cf>, <W_base>)` directive body (the part after `:-`).
///
/// Both arguments are IRI references (prefixed name, single-quoted IRI, or
/// angle-bracketed IRI), resolved to the canonical `<iri>` constant form.
///
/// Returns `(cf_world, base_world)` in canonical `<iri>` form.
fn parse_counterfactual_directive(
    body: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<(String, String)> {
    let inner = body
        .strip_prefix("counterfactual(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| query_err(format!("malformed counterfactual directive: {body:?}")))?;
    let args = split_comma_top(inner);
    if args.len() != 2 {
        return Err(query_err(format!(
            "counterfactual(...) takes exactly 2 world IRIs (W_cf, W_base); got {} in {body:?}",
            args.len()
        )));
    }
    let cf_world = resolve_iri(args[0].trim(), prefixes).ok_or_else(|| {
        query_err(format!(
            "cannot resolve W_cf IRI {:?} in {body:?}",
            args[0].trim()
        ))
    })?;
    let base_world = resolve_iri(args[1].trim(), prefixes).ok_or_else(|| {
        query_err(format!(
            "cannot resolve W_base IRI {:?} in {body:?}",
            args[1].trim()
        ))
    })?;
    if cf_world == base_world {
        return Err(query_err(format!(
            "counterfactual W_cf and W_base must differ (got both = {cf_world})"
        )));
    }
    Ok((cf_world, base_world))
}

/// Parse an `assume(pred(S, O))` directive body (the part after `:-`).
///
/// The inner `pred(S, O)` is parsed as an ordinary binary atom; ground-ness is
/// enforced by the caller once the whole program is assembled.
fn parse_assume_directive(
    body: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<QAtom> {
    let inner = body
        .strip_prefix("assume(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| query_err(format!("malformed assume directive: {body:?}")))?;
    parse_atom(inner.trim(), prefixes)
}

/// Parse a `depth_budget(N)` directive body (the part after `:-`).
///
/// `N` is a non-negative integer — the hard cap on nested-counterfactual depth.
fn parse_depth_budget_directive(body: &str) -> gmeow_errors::Result<u64> {
    let inner = body
        .strip_prefix("depth_budget(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| query_err(format!("malformed depth_budget directive: {body:?}")))?;
    inner.trim().parse::<u64>().map_err(|e| {
        query_err(format!(
            "depth_budget(...) must be a non-negative integer in {body:?}: {e}"
        ))
    })
}

// ── Probabilistic directive parsers ───────────────────────────────────

/// Validate a probability/confidence decimal token: must parse as `f64` and lie
/// in `[0, 1]`. Returns the trimmed token verbatim (the IR keeps the raw text).
fn validate_unit_decimal(tok: &str, what: &str) -> gmeow_errors::Result<String> {
    let t = tok.trim();
    let v: f64 = t
        .parse::<f64>()
        .map_err(|e| query_err(format!("{what} value {t:?} is not a decimal: {e}")))?;
    if !(0.0..=1.0).contains(&v) || v.is_nan() {
        return Err(query_err(format!("{what} value {t:?} must be in [0, 1]")));
    }
    Ok(t.to_owned())
}

/// Require every term of `atom` to be a constant (no variables) — probabilistic
/// facts, joint outcomes, and confidence annotations are over concrete facts.
fn require_ground(atom: &QAtom, what: &str) -> gmeow_errors::Result<()> {
    if atom.args.iter().any(|t| matches!(t, QTerm::Var(_))) {
        return Err(query_err(format!(
            "{what} atom must be ground (no variables): {:?}",
            atom.pred
        )));
    }
    Ok(())
}

/// Parse a `probability_model(kind)` directive body (the part after `:-`).
///
/// `kind` is a bare keyword: `full_independence` or `dependency`.
fn parse_probability_model_directive(body: &str) -> gmeow_errors::Result<String> {
    let inner = body
        .strip_prefix("probability_model(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| query_err(format!("malformed probability_model directive: {body:?}")))?;
    let kind = inner.trim();
    if kind.is_empty() {
        return Err(query_err(format!(
            "probability_model(...) kind is empty in: {body:?}"
        )));
    }
    Ok(kind.to_owned())
}

/// Parse a `probability(pred(S, O), p)` directive body (the part after `:-`).
fn parse_probability_directive(
    body: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<QProbFact> {
    let inner = body
        .strip_prefix("probability(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| query_err(format!("malformed probability directive: {body:?}")))?;
    let parts = split_comma_top(inner);
    if parts.len() != 2 {
        return Err(query_err(format!(
            "probability(...) takes exactly an atom and a probability; got {} parts in {body:?}",
            parts.len()
        )));
    }
    let atom = parse_atom(parts[0].trim(), prefixes)?;
    require_ground(&atom, "probability")?;
    let prob = validate_unit_decimal(parts[1], "probability")?;
    Ok(QProbFact { atom, prob })
}

/// Parse a `joint(p, atom1, atom2, ...)` directive body (the part after `:-`).
///
/// The first argument is the joint probability; the rest are the atoms TRUE in
/// this outcome (possibly none — an all-false outcome). Atoms must be ground.
fn parse_joint_directive(
    body: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<QJointOutcome> {
    let inner = body
        .strip_prefix("joint(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| query_err(format!("malformed joint directive: {body:?}")))?;
    let parts = split_comma_top(inner);
    if parts.is_empty() {
        return Err(query_err(format!(
            "joint(...) requires a probability in {body:?}"
        )));
    }
    let prob = validate_unit_decimal(parts[0], "joint")?;
    let mut true_atoms = Vec::new();
    for tok in &parts[1..] {
        let atom = parse_atom(tok.trim(), prefixes)?;
        require_ground(&atom, "joint")?;
        true_atoms.push(atom);
    }
    Ok(QJointOutcome { true_atoms, prob })
}

/// Parse a `confidence(pred(S, O), c)` directive body (the part after `:-`).
fn parse_confidence_directive(
    body: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<QConfidence> {
    let inner = body
        .strip_prefix("confidence(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| query_err(format!("malformed confidence directive: {body:?}")))?;
    let parts = split_comma_top(inner);
    if parts.len() != 2 {
        return Err(query_err(format!(
            "confidence(...) takes exactly an atom and a confidence; got {} parts in {body:?}",
            parts.len()
        )));
    }
    let atom = parse_atom(parts[0].trim(), prefixes)?;
    require_ground(&atom, "confidence")?;
    let confidence = validate_unit_decimal(parts[1], "confidence")?;
    Ok(QConfidence { atom, confidence })
}

// ── Rule parser ───────────────────────────────────────────────────────────────

/// Parse a rule clause `head :- body1, body2, ... ` or a fact `head`.
fn parse_rule(clause: &str, prefixes: &BTreeMap<String, String>) -> gmeow_errors::Result<QRule> {
    if let Some(idx) = find_neck(clause) {
        let head_str = clause[..idx].trim();
        let body_str = clause[idx + 2..].trim();
        let head = parse_atom(head_str, prefixes)?;
        let body_lits = parse_body_lit_list(body_str, prefixes)?;
        Ok(QRule {
            head,
            body: body_lits,
        })
    } else {
        // Fact: no `:-` neck.
        let head = parse_atom(clause.trim(), prefixes)?;
        Ok(QRule { head, body: vec![] })
    }
}

/// Find the position of the `:-` neck that is not inside parentheses or quotes.
fn find_neck(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b':' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                return Some(i);
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

// ── Body literal list ─────────────────────────────────────────────────────────

/// Parse a comma-separated list of body literals (atoms, `!`, or builtins).
///
/// Builtins are detected BEFORE the `parse_atom` fallback. A builtin token has no
/// outer `pred(...)` parens; an atom does — that disambiguates robustly:
/// - `!`                              → [`QBodyLit::Cut`].
/// - `target is expr`                 → [`QBuiltin::Is`] (one operand or `lhs op rhs`).
/// - `lhs cmp rhs` (no `name(...)`)   → [`QBuiltin::Compare`] (cmp ∈ `> < >= =< =:=`).
/// - otherwise                        → [`QBodyLit::Atom`].
fn parse_body_lit_list(
    s: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<Vec<QBodyLit>> {
    split_comma_top(s)
        .into_iter()
        .map(|tok| {
            let tok = tok.trim();
            if tok == "!" {
                Ok(QBodyLit::Cut)
            } else if let Some(inner) = strip_negation(tok) {
                // A negated body literal `\+ pred(..)` / `not pred(..)`: the inner form
                // is always an ordinary predicate atom (NAF over a builtin/cut is not a
                // meaningful goal), so parse it as an atom and wrap it as `Neg`.
                parse_atom(inner.trim(), prefixes).map(QBodyLit::Neg)
            } else if let Some(builtin) = try_parse_builtin(tok, prefixes)? {
                Ok(QBodyLit::Builtin(builtin))
            } else {
                parse_atom(tok, prefixes).map(QBodyLit::Atom)
            }
        })
        .collect()
}

/// Strip a leading negation-as-failure operator (`\+` or the `not` keyword) from a body
/// literal token, returning the inner (still-unparsed) atom text.
///
/// Recognizes the two Prolog-ish query-surface forms: `\+ pred(..)` (with or without a
/// space after `\+`) and `not pred(..)` (the keyword form, requiring a following space so
/// a predicate whose local name merely starts with `not`, e.g. `notation(..)`, is NOT
/// mistaken for a negation). Returns `None` when `tok` carries no negation operator.
fn strip_negation(tok: &str) -> Option<&str> {
    if let Some(rest) = tok.strip_prefix("\\+") {
        return Some(rest);
    }
    if let Some(rest) = tok.strip_prefix("not")
        && rest.starts_with(char::is_whitespace)
    {
        return Some(rest);
    }
    None
}

/// Attempt to parse `tok` as an arithmetic or comparison builtin.
///
/// Returns `Ok(Some(_))` if `tok` is a builtin, `Ok(None)` if it is an ordinary
/// atom (a `pred(...)` form), or `Err` if it looks like a builtin but is malformed.
fn try_parse_builtin(
    tok: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<Option<QBuiltin>> {
    // `X is Expr` — the ` is ` infix (with surrounding spaces) is unambiguous.
    if let Some(is_pos) = find_infix_top(tok, " is ") {
        let target_str = tok[..is_pos].trim();
        let rhs_str = tok[is_pos + 4..].trim();
        let target = parse_term(target_str, prefixes)?;
        // A named n-ary math function on the RHS (e.g. `bilinearSqDist(G, X, Y)`) is a
        // moded math builtin, detected BEFORE the arithmetic split so its parenthesized
        // argument list (which may contain `/` inside `<iri>`s) is never mistaken for a
        // binary arithmetic operator. The table is greenfield-extensible: more math
        // builtins register a name here.
        if let Some((name, args)) = parse_named_function(rhs_str)
            && let Some(builtin) = try_parse_math_function(name, &args, &target, prefixes)?
        {
            return Ok(Some(builtin));
        }
        // Split a binary RHS on its arithmetic operator (multi-char `//` checked
        // first). A single operand is the arithmetic-expression base case; lower it
        // canonically to `operand + 0` so parsing, hashing, mode analysis, and execution
        // retain one bounded `QBuiltin::Is` representation.
        let (lhs_str, op, rhs_op_str) =
            split_arith(rhs_str).unwrap_or((rhs_str, ArithOp::Add, "0"));
        let lhs = parse_term(lhs_str.trim(), prefixes)?;
        let rhs = parse_term(rhs_op_str.trim(), prefixes)?;
        return Ok(Some(QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        }));
    }

    // `L cmp R` — only when `tok` is NOT a `pred(...)` atom (atoms own outer parens).
    // Multi-char comparison operators are checked FIRST so `>=`/`=<`/`=:=` are not
    // mistaken for `>`/`<`/`=`.
    if !is_atom_shaped(tok)
        && let Some((lhs_str, op, rhs_str)) = split_compare(tok)
    {
        let lhs = parse_term(lhs_str.trim(), prefixes)?;
        let rhs = parse_term(rhs_str.trim(), prefixes)?;
        return Ok(Some(QBuiltin::Compare { lhs, op, rhs }));
    }

    Ok(None)
}

/// Detect an RHS shaped as a named n-ary function call `name(arg0, arg1, ...)`.
///
/// Returns `(name, arg_tokens)` when `s` is `name(...)` with a bare alphanumeric
/// name and a `)`-terminated argument list, splitting the arguments at top-level
/// commas (parens/quotes respected, exactly like [`split_comma_top`]). Returns
/// `None` for any non-function shape (a bare operand, an arithmetic expression), so
/// the caller falls through to the arithmetic split.
fn parse_named_function(s: &str) -> Option<(&str, Vec<&str>)> {
    let s = s.trim();
    if !s.ends_with(')') {
        return None;
    }
    let open = s.find('(')?;
    let name = s[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let inner = &s[open + 1..s.len() - 1];
    Some((name, split_comma_top(inner)))
}

/// Table-driven parse of a named math builtin from its `name(args)` surface.
///
/// `Ok(Some(_))` for a recognized math function (currently only `bilinearSqDist/3`),
/// `Ok(None)` for an unrecognized name (the caller falls through to arithmetic), or
/// `Err` for a recognized name with the wrong arity — a malformed builtin, never a
/// silent fallthrough.
fn try_parse_math_function(
    name: &str,
    args: &[&str],
    target: &QTerm,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<Option<QBuiltin>> {
    match name {
        "bilinearSqDist" => {
            if args.len() != 3 {
                return Err(query_err(format!(
                    "bilinearSqDist(...) takes exactly 3 arguments (gram, x, y); got {}",
                    args.len()
                )));
            }
            let gram = parse_term(args[0].trim(), prefixes)?;
            let x = parse_term(args[1].trim(), prefixes)?;
            let y = parse_term(args[2].trim(), prefixes)?;
            Ok(Some(QBuiltin::BilinearSqDist {
                target: target.clone(),
                gram,
                x,
                y,
            }))
        }
        _ => Ok(None),
    }
}

/// Find a top-level (not inside parens/quotes) occurrence of `needle` in `s`.
fn find_infix_top(s: &str, needle: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ if depth == 0 && bytes[i..].starts_with(needle_bytes) => {
                return Some(i);
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

/// `true` if `tok` is shaped like an atom `name(...)` — has an opening paren and
/// ends with `)`. Builtins never have an outer `pred(...)` wrapper.
fn is_atom_shaped(tok: &str) -> bool {
    let tok = tok.trim();
    tok.ends_with(')') && tok.contains('(')
}

/// Split an arithmetic RHS `lhs op rhs` on the first top-level arithmetic operator.
///
/// The multi-char `//` (truncating integer division, [`ArithOp::Div`]) is checked
/// FIRST so it always wins over the single-char `/` (exact rational division,
/// [`ArithOp::ExactDiv`]) when both could match. The single-char pass then adds `/`
/// alongside `+ * -`, each skipping a leading-sign occurrence at position 0.
/// Returns `(lhs, op, rhs)`.
fn split_arith(s: &str) -> Option<(&str, ArithOp, &str)> {
    // Multi-char first: `//` must bind before the lone `/`.
    if let Some(pos) = find_infix_top(s, "//") {
        return Some((&s[..pos], ArithOp::Div, &s[pos + 2..]));
    }
    // Single-char operators. A leading `-`/`+`/`/` at position 0 is a sign or a
    // dangling operator (no LHS), not a binary split, so it is skipped.
    for (tok, op) in [
        ("+", ArithOp::Add),
        ("*", ArithOp::Mul),
        ("-", ArithOp::Sub),
        ("/", ArithOp::ExactDiv),
    ] {
        if let Some(pos) = find_infix_top(s, tok) {
            // Skip a leading-sign `-`/`+`/`/` (operator at position 0 with no LHS).
            if pos == 0 {
                continue;
            }
            return Some((&s[..pos], op, &s[pos + tok.len()..]));
        }
    }
    None
}

/// Split a comparison `lhs cmp rhs` on the first top-level comparison operator.
/// Multi-char operators (`>=`, `=<`, `=:=`) are checked before single chars.
fn split_compare(s: &str) -> Option<(&str, CmpOp, &str)> {
    for (tok, op) in [
        ("=:=", CmpOp::Eq),
        (">=", CmpOp::Ge),
        ("=<", CmpOp::Le),
        (">", CmpOp::Gt),
        ("<", CmpOp::Lt),
    ] {
        if let Some(pos) = find_infix_top(s, tok) {
            return Some((&s[..pos], op, &s[pos + tok.len()..]));
        }
    }
    None
}

/// Parse a comma-separated list of atoms (for `?-` goal).
fn parse_atom_list(
    s: &str,
    prefixes: &BTreeMap<String, String>,
) -> gmeow_errors::Result<Vec<QAtom>> {
    split_comma_top(s)
        .into_iter()
        .map(|tok| parse_atom(tok.trim(), prefixes))
        .collect()
}

/// Split `s` on commas that are not inside parentheses or quotes.
fn split_comma_top(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b',' if depth == 0 => {
                parts.push(&s[start..i]);
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }
    if start <= s.len() {
        parts.push(&s[start..]);
    }
    parts
}

/// Split on top-level ASCII whitespace, respecting bracket depth (`<`/`(` open,
/// `>`/`)` close) and `'`/`"` quotes so a nested quoted-triple `<<( … )>>`, an
/// angle-bracketed `<iri>`, or a spaced literal stays one component. Used to split the
/// three components of a `<<( s p o )>>` term.
fn split_ws_top(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut in_token = false;
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'\'' | b'"') => {
                if !in_token {
                    start = i;
                    in_token = true;
                }
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'<' | b'(' => {
                if !in_token {
                    start = i;
                    in_token = true;
                }
                depth += 1;
                i += 1;
            }
            b'>' | b')' => {
                depth -= 1;
                i += 1;
            }
            b if b.is_ascii_whitespace() && depth == 0 => {
                if in_token {
                    parts.push(&s[start..i]);
                    in_token = false;
                }
                i += 1;
            }
            _ => {
                if !in_token {
                    start = i;
                    in_token = true;
                }
                i += 1;
            }
        }
    }
    if in_token {
        parts.push(&s[start..]);
    }
    parts
}

/// Whether a parsed term is ground — carries no [`QTerm::Var`] at any depth. A quoted
/// triple with an embedded variable is a triple *pattern* (unsupported as a goal
/// argument), rejected at parse time.
fn qterm_is_ground(t: &QTerm) -> bool {
    match t {
        QTerm::Var(_) => false,
        QTerm::Const(_) | QTerm::Num(_) | QTerm::Struct(_) => true,
        QTerm::Triple { s, p, o } => qterm_is_ground(s) && qterm_is_ground(p) && qterm_is_ground(o),
    }
}

/// Whether a parsed term denotes an IRI — a canonical `<iri>` constant (from a prefixed
/// name or single-quoted/angle-bracketed form). A double-quoted literal `Const` (`"…"`)
/// and every non-`Const` shape are not IRIs.
fn qterm_is_iri(t: &QTerm) -> bool {
    matches!(t, QTerm::Const(c) if c.starts_with('<') && c.ends_with('>'))
}

/// Canonical surface string of a parsed term: a `Const` verbatim, a `Num` as its decimal
/// text, a `Var` as its name, a `Struct` as its `#struct<idx>` handle, and a ground
/// quoted-triple as `<<( s p o )>>` (recursively). Used where a term must be rendered to a
/// comparison/memo surface outside the physical evaluator (the declarative reference oracle
/// and the probabilistic path).
pub(crate) fn qterm_display(t: &QTerm) -> String {
    match t {
        QTerm::Const(c) => c.clone(),
        QTerm::Var(v) => v.clone(),
        QTerm::Num(n) => n.to_string(),
        QTerm::Struct(sn) => format!("#struct{}", sn.node().index()),
        QTerm::Triple { s, p, o } => format!(
            "<<( {} {} {} )>>",
            qterm_display(s),
            qterm_display(p),
            qterm_display(o)
        ),
    }
}

// ── Atom parser ───────────────────────────────────────────────────────────────

/// Parse a single atom `pred(Arg0, Arg1, ...)`.
///
/// The predicate may be a prefixed name (`ex:foo`) or a single-quoted IRI
/// (`'https://...'`). Args are one or more terms (binary EDB RDF atoms carry two;
/// n-ary IDB predicates like `get/3` carry more — G2a).
fn parse_atom(s: &str, prefixes: &BTreeMap<String, String>) -> gmeow_errors::Result<QAtom> {
    let s = s.trim();
    // Find the opening paren.
    let open = s
        .find('(')
        .ok_or_else(|| query_err(format!("atom missing '(': {s:?}")))?;
    if !s.ends_with(')') {
        return Err(query_err(format!("atom missing closing ')': {s:?}")));
    }
    let pred_str = s[..open].trim();
    let args_str = s[open + 1..s.len() - 1].trim();

    // Reserved generic relation: the bare, unqualified `triple` symbol names the
    // arity-4 predicate-as-data encoding `triple(s, p, o, w)` and is carried VERBATIM
    // (no IRI resolution — it is a program-local relation symbol, not an IRI), so the
    // parsed predicate agrees exactly with the generic-triple EDB's
    // `push_fact(GENERIC_TRIPLE_RELATION, …)`.  Everything else resolves as an IRI.
    let pred = if pred_str == GENERIC_TRIPLE_RELATION {
        GENERIC_TRIPLE_RELATION.to_owned()
    } else {
        let pred = resolve_iri(pred_str, prefixes)
            .ok_or_else(|| query_err(format!("cannot resolve predicate IRI {pred_str:?}")))?;
        // Predicate: bare IRI string (strip angle brackets if present).
        strip_angle_brackets(&pred)
    };

    let arg_tokens = split_comma_top(args_str);
    if arg_tokens.is_empty() {
        return Err(query_err(format!(
            "atom {s:?} has no args; expected at least 1"
        )));
    }

    let args: Vec<QTerm> = arg_tokens
        .into_iter()
        .map(|tok| parse_term(tok.trim(), prefixes))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(QAtom { pred, args })
}

// ── Term parser ───────────────────────────────────────────────────────────────

/// Parse a single term (variable or constant).
fn parse_term(s: &str, prefixes: &BTreeMap<String, String>) -> gmeow_errors::Result<QTerm> {
    let s = s.trim();
    if s.is_empty() {
        return Err(query_err("empty term".to_owned()));
    }

    // Variable: starts with uppercase ASCII letter or `_`.
    let first = s.chars().next().unwrap();
    if first.is_uppercase() || first == '_' {
        return Ok(QTerm::Var(s.to_owned()));
    }

    // Integer literal: a bare `i64` arithmetic operand (G2a). Checked before
    // the prefixed-name branch so `0`/`1`/`-1` are numbers, not failed IRIs.
    if let Ok(n) = s.parse::<i64>() {
        return Ok(QTerm::Num(n));
    }

    // RDF 1.2 quoted-triple term: `<<( s p o )>>` (the render form in
    // `provenance::term_display`). Checked BEFORE `resolve_iri`: `<<( … )>>` starts with
    // `<` and ends with `>`, so `resolve_iri` would otherwise mis-capture the whole span
    // as an opaque IRI. Components are parsed recursively; the term must be ground (a
    // triple *pattern* with variables is a distinct, unsupported semantics) and its
    // predicate must be an IRI.
    if let Some(inner) = s
        .strip_prefix("<<(")
        .and_then(|rest| rest.strip_suffix(")>>"))
    {
        let components = split_ws_top(inner.trim());
        if components.len() != 3 {
            return Err(query_err(format!(
                "quoted-triple term must have exactly 3 components (subject predicate object), \
                 got {}: {s:?}",
                components.len()
            )));
        }
        let subject = parse_term(components[0], prefixes)?;
        let predicate = parse_term(components[1], prefixes)?;
        let object = parse_term(components[2], prefixes)?;
        for (role, term) in [
            ("subject", &subject),
            ("predicate", &predicate),
            ("object", &object),
        ] {
            if !qterm_is_ground(term) {
                return Err(query_err(format!(
                    "quoted-triple {role} must be ground (a variable makes it a triple \
                     pattern, which is not supported as a goal argument): {s:?}"
                )));
            }
        }
        // RDF 1.2: a triple-term predicate is always an IRI.
        if !qterm_is_iri(&predicate) {
            return Err(query_err(format!(
                "quoted-triple predicate must be an IRI (RDF 1.2): {s:?}"
            )));
        }
        return Ok(QTerm::Triple {
            s: Box::new(subject),
            p: Box::new(predicate),
            o: Box::new(object),
        });
    }

    // Single-quoted full IRI: `'https://...'`
    if s.starts_with('\'') {
        if !s.ends_with('\'') || s.len() < 2 {
            return Err(query_err(format!("unterminated single-quoted IRI: {s:?}")));
        }
        let iri = &s[1..s.len() - 1];
        return Ok(QTerm::Const(format!("<{}>", iri)));
    }

    // Double-quoted literal: `"foo"` — canonicalize as n3 string literal.
    if s.starts_with('"') {
        if !s.ends_with('"') || s.len() < 2 {
            return Err(query_err(format!(
                "unterminated double-quoted literal: {s:?}"
            )));
        }
        // Keep it verbatim in canonical n3 form.
        return Ok(QTerm::Const(s.to_owned()));
    }

    // Prefixed name: `ex:alice`.
    if let Some(iri) = resolve_iri(s, prefixes) {
        return Ok(QTerm::Const(iri));
    }

    Err(query_err(format!(
        "cannot parse term {s:?} (not a variable, single-quoted IRI, or prefixed name)"
    )))
}

// ── IRI resolution helpers ────────────────────────────────────────────────────

/// Resolve a predicate/constant string to a canonical `<iri>` form.
///
/// Returns `Some("<iri>")` on success, `None` if the input cannot be resolved
/// (e.g. an unknown prefix).
fn resolve_iri(s: &str, prefixes: &BTreeMap<String, String>) -> Option<String> {
    let s = s.trim();

    // Already angle-bracketed: `<https://...>` — pass through.
    if s.starts_with('<') && s.ends_with('>') {
        return Some(s.to_owned());
    }

    // Single-quoted IRI.
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        let iri = &s[1..s.len() - 1];
        return Some(format!("<{}>", iri));
    }

    // Prefixed name: `alias:local`.
    if let Some(colon) = s.find(':') {
        let alias = &s[..colon];
        let local = &s[colon + 1..];
        if let Some(base) = prefixes.get(alias) {
            return Some(format!("<{}{}>", base, local));
        }
    }

    None
}

/// Strip angle brackets from `<iri>` → `iri`.
fn strip_angle_brackets(s: &str) -> String {
    if s.starts_with('<') && s.ends_with('>') {
        s[1..s.len() - 1].to_owned()
    } else {
        s.to_owned()
    }
}

// ── Utility impls ─────────────────────────────────────────────────────────────

impl QBodyLit {
    /// Extract the inner `QAtom` if this is a `QBodyLit::Atom`, else `None`.
    pub fn into_atom(self) -> Option<QAtom> {
        match self {
            QBodyLit::Atom(a) => Some(a),
            QBodyLit::Neg(_) | QBodyLit::Cut | QBodyLit::Builtin(_) => None,
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Variable vs constant classification ───────────────────────────────────

    #[test]
    fn variable_uppercase_first() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, ex:a).\n\
             ?- ex:p(X, Y).\n",
        )
        .unwrap();
        let fact = &prog.rules[0];
        assert_eq!(fact.head.args[0], QTerm::Var("X".to_owned()));
        assert_eq!(
            fact.head.args[1],
            QTerm::Const("<https://example.org/a>".to_owned())
        );
    }

    // ── RDF 1.2 quoted-triple goal argument ───────────────────────────────────

    #[test]
    fn quoted_triple_goal_argument_parses_ground_components() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( ex:s0 ex:p ex:o0 )>>, C).\n",
        )
        .unwrap();
        let arg = &prog.goal.atoms[0].args[0];
        let QTerm::Triple { s, p, o } = arg else {
            panic!("expected a quoted-triple term, got {arg:?}");
        };
        assert_eq!(**s, QTerm::Const("<https://example.org/s0>".to_owned()));
        assert_eq!(**p, QTerm::Const("<https://example.org/p>".to_owned()));
        assert_eq!(**o, QTerm::Const("<https://example.org/o0>".to_owned()));
        // The unbound candidate stays a variable.
        assert_eq!(prog.goal.atoms[0].args[1], QTerm::Var("C".to_owned()));
    }

    #[test]
    fn quoted_triple_nested_object_parses() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( ex:s0 ex:p <<( ex:a ex:q ex:b )>> )>>, C).\n",
        )
        .unwrap();
        let QTerm::Triple { o, .. } = &prog.goal.atoms[0].args[0] else {
            panic!("expected a quoted-triple term");
        };
        assert!(matches!(**o, QTerm::Triple { .. }), "nested object triple");
    }

    #[test]
    fn quoted_triple_with_embedded_variable_is_rejected() {
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( X ex:p ex:o0 )>>, C).\n",
        )
        .unwrap_err();
        assert!(
            format!("{err:?}").contains("ground"),
            "embedded variable must be rejected as non-ground: {err:?}"
        );
    }

    #[test]
    fn quoted_triple_with_non_iri_predicate_is_rejected() {
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( ex:s0 \"lit\" ex:o0 )>>, C).\n",
        )
        .unwrap_err();
        assert!(
            format!("{err:?}").contains("predicate must be an IRI"),
            "non-IRI predicate must be rejected: {err:?}"
        );
    }

    #[test]
    fn quoted_triple_wrong_arity_is_rejected() {
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( ex:s0 ex:p )>>, C).\n",
        )
        .unwrap_err();
        assert!(
            format!("{err:?}").contains("exactly 3 components"),
            "a 2-component quoted triple must be rejected: {err:?}"
        );
    }

    #[test]
    fn variable_underscore_first() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(_Z, ex:b).\n\
             ?- ex:p(_Z, ex:b).\n",
        )
        .unwrap();
        assert_eq!(prog.rules[0].head.args[0], QTerm::Var("_Z".to_owned()));
    }

    // ── Prefix expansion correctness ──────────────────────────────────────────

    #[test]
    fn prefix_expansion_is_correct() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/profiles/positive-horn/').\n\
             ex:parentOf(ex:alice, ex:bob).\n\
             ?- ex:parentOf(ex:alice, Y).\n",
        )
        .unwrap();
        let fact = &prog.rules[0];
        assert_eq!(
            fact.head.pred,
            "https://example.org/profiles/positive-horn/parentOf"
        );
        assert_eq!(
            fact.head.args[0],
            QTerm::Const("<https://example.org/profiles/positive-horn/alice>".to_owned())
        );
        assert_eq!(
            fact.head.args[1],
            QTerm::Const("<https://example.org/profiles/positive-horn/bob>".to_owned())
        );
    }

    // ── Malformed prefix IRI: must Err, never panic on the quote-strip slice ───

    #[test]
    fn prefix_lone_single_quote_errs_not_panics() {
        // A lone `'` satisfies both starts_with/ends_with; without the len guard
        // the `iri_part[1..len-1]` strip is `[1..0]` and panics. Must be a clean Err.
        let err = parse_prefix_directive("prefix(ex, ')").unwrap_err();
        assert!(
            err.message().contains("single-quoted"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn prefix_empty_quotes_errs() {
        // `''` strips to the empty IRI — caught by the empty-IRI check, also an Err.
        let err = parse_prefix_directive("prefix(ex, '')").unwrap_err();
        assert!(err.message().contains("empty"), "unexpected error: {err}");
    }

    // ── Prefix + 2 rules + goal parse ─────────────────────────────────────────

    #[test]
    fn parse_prefix_two_rules_and_goal() {
        let src = "\
:- prefix(ex, 'https://example.org/').\
\n\
ex:parentOf(ex:alice, ex:bob).\
\n\
ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\
\n\
ex:ancestorOf(X, Y) :- ex:parentOf(X, Z), ex:ancestorOf(Z, Y).\
\n\
?- ex:ancestorOf(ex:alice, Y).\
";
        let prog = parse_query_program(src).unwrap();
        assert_eq!(prog.rules.len(), 3, "1 fact + 2 rules");
        assert_eq!(prog.goal.atoms.len(), 1);
        assert_eq!(prog.goal.atoms[0].pred, "https://example.org/ancestorOf");
    }

    // ── Fact parse ────────────────────────────────────────────────────────────

    #[test]
    fn parse_fact_no_body() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b).\n\
             ?- ex:p(ex:a, ex:b).\n",
        )
        .unwrap();
        assert_eq!(prog.rules.len(), 1);
        let fact = &prog.rules[0];
        assert!(fact.body.is_empty(), "fact must have empty body");
        assert_eq!(fact.head.pred, "https://example.org/p");
    }

    // ── Cut in body ───────────────────────────────────────────────────────────

    #[test]
    fn parse_cut_in_body() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- ex:q(X, Y), !, ex:r(X, Y).\n\
             ?- ex:p(X, Y).\n",
        )
        .unwrap();
        let rule = &prog.rules[0];
        assert_eq!(rule.body.len(), 3);
        assert_eq!(
            rule.body[0],
            QBodyLit::Atom(rule.body[0].clone().into_atom().unwrap())
        );
        assert_eq!(rule.body[1], QBodyLit::Cut);
        assert_eq!(
            rule.body[2],
            QBodyLit::Atom(rule.body[2].clone().into_atom().unwrap())
        );
    }

    // ── Negation-as-failure in body ───────────────────────────────────────────

    #[test]
    fn parse_backslash_plus_and_not_negation() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- ex:q(X, Y), \\+ ex:r(X, Y), not ex:s(X, Y).\n\
             ?- ex:p(X, Y).\n",
        )
        .unwrap();
        let body = &prog.rules[0].body;
        assert_eq!(body.len(), 3, "one positive + two negated literals");
        assert!(matches!(body[0], QBodyLit::Atom(_)), "q is positive");
        match &body[1] {
            QBodyLit::Neg(a) => assert_eq!(a.pred, "https://example.org/r"),
            other => panic!("expected Neg(r), got {other:?}"),
        }
        match &body[2] {
            QBodyLit::Neg(a) => assert_eq!(a.pred, "https://example.org/s"),
            other => panic!("expected Neg(s), got {other:?}"),
        }
    }

    #[test]
    fn parse_not_prefixed_predicate_is_not_mistaken_for_negation() {
        // A predicate whose local name merely starts with `not` (here `notation`) must NOT
        // be parsed as a negation operator — the `not` keyword requires a following space.
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- ex:notation(X, Y).\n\
             ?- ex:p(X, Y).\n",
        )
        .unwrap();
        match &prog.rules[0].body[0] {
            QBodyLit::Atom(a) => assert_eq!(a.pred, "https://example.org/notation"),
            other => panic!("expected a positive notation atom, got {other:?}"),
        }
    }

    // ── Reject: no goal ───────────────────────────────────────────────────────

    #[test]
    fn reject_program_with_no_goal() {
        let result = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b).\n",
        );
        assert!(result.is_err(), "must reject program with no goal");
        assert!(result.unwrap_err().message().contains("no ?- goal"));
    }

    // ── Reject: malformed clause ──────────────────────────────────────────────

    #[test]
    fn reject_malformed_atom_missing_parens() {
        let result = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p ex:a ex:b.\n\
             ?- ex:p(ex:a, ex:b).\n",
        );
        assert!(result.is_err(), "must reject atom missing parentheses");
    }

    #[test]
    fn parse_ternary_atom_is_accepted() {
        // Arity is now arbitrary (≥1): n-ary IDB predicates like get/3 are valid
        // (G2a). EDB RDF atoms remain binary naturally.
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b, ex:c).\n\
             ?- ex:p(ex:a, ex:b, ex:c).\n",
        )
        .unwrap();
        assert_eq!(prog.rules[0].head.args.len(), 3);
        assert_eq!(prog.goal.atoms[0].args.len(), 3);
    }

    // ── Arithmetic / comparison builtin parsing (G2a) ───────────────────

    #[test]
    fn parse_is_arith_builtin_roundtrip() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:len(L, N) :- ex:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n",
        )
        .unwrap();
        let body = &prog.rules[0].body;
        assert_eq!(body.len(), 3, "two atoms + one builtin");
        match &body[2] {
            QBodyLit::Builtin(QBuiltin::Is {
                target,
                lhs,
                op,
                rhs,
            }) => {
                assert_eq!(*target, QTerm::Var("N".to_owned()));
                assert_eq!(*lhs, QTerm::Var("M".to_owned()));
                assert_eq!(*op, ArithOp::Add);
                assert_eq!(*rhs, QTerm::Num(1));
            }
            other => panic!("expected Is builtin, got {other:?}"),
        }
    }

    #[test]
    fn parse_distinguishes_truncating_and_exact_division_operators() {
        // `//` parses to the truncating-integer operator; a lone `/` to exact ℚ
        // division. `//` (multi-char, checked first) always wins over `/`.
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(A, B) :- A is 6 // 4, B is 6 / 4.\n\
             ?- ex:p(A, B).\n",
        )
        .unwrap();
        let body = &prog.rules[0].body;
        assert_eq!(body.len(), 2);
        assert_eq!(
            body[0],
            QBodyLit::Builtin(QBuiltin::Is {
                target: QTerm::Var("A".to_owned()),
                lhs: QTerm::Num(6),
                op: ArithOp::Div,
                rhs: QTerm::Num(4),
            })
        );
        assert_eq!(
            body[1],
            QBodyLit::Builtin(QBuiltin::Is {
                target: QTerm::Var("B".to_owned()),
                lhs: QTerm::Num(6),
                op: ArithOp::ExactDiv,
                rhs: QTerm::Num(4),
            })
        );
        assert_eq!(ArithOp::Div.token(), "//");
        assert_eq!(ArithOp::ExactDiv.token(), "/");
    }

    #[test]
    fn parse_is_single_operand_lowers_to_canonical_additive_identity() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- X is 1, Y is 2.\n\
             ?- ex:p(X, Y).\n",
        )
        .unwrap();
        let body = &prog.rules[0].body;
        assert_eq!(body.len(), 2);
        assert_eq!(
            body[0],
            QBodyLit::Builtin(QBuiltin::Is {
                target: QTerm::Var("X".to_owned()),
                lhs: QTerm::Num(1),
                op: ArithOp::Add,
                rhs: QTerm::Num(0),
            })
        );
        assert_eq!(
            body[1],
            QBodyLit::Builtin(QBuiltin::Is {
                target: QTerm::Var("Y".to_owned()),
                lhs: QTerm::Num(2),
                op: ArithOp::Add,
                rhs: QTerm::Num(0),
            })
        );
    }

    #[test]
    fn parse_compare_builtin_roundtrip() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:pos(L, N, X) :- N > 0, ex:rest(L, X).\n\
             ?- ex:pos(ex:l0, 1, X).\n",
        )
        .unwrap();
        let body = &prog.rules[0].body;
        assert_eq!(body.len(), 2, "one builtin + one atom");
        match &body[0] {
            QBodyLit::Builtin(QBuiltin::Compare { lhs, op, rhs }) => {
                assert_eq!(*lhs, QTerm::Var("N".to_owned()));
                assert_eq!(*op, CmpOp::Gt);
                assert_eq!(*rhs, QTerm::Num(0));
            }
            other => panic!("expected Compare builtin, got {other:?}"),
        }
    }

    #[test]
    fn parse_combined_builtins_split_on_top_comma() {
        // `N is M + 1, N > 0` must split into two body literals.
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:r(M, N) :- N is M + 1, N > 0.\n\
             ?- ex:r(ex:a, N).\n",
        )
        .unwrap();
        let body = &prog.rules[0].body;
        assert_eq!(body.len(), 2);
        assert!(matches!(body[0], QBodyLit::Builtin(QBuiltin::Is { .. })));
        assert!(matches!(
            body[1],
            QBodyLit::Builtin(QBuiltin::Compare { .. })
        ));
    }

    // ── Answer-set canonicalization ───────────────────────────────────────────

    // ── Counterfactual directive parsing ───────────────────────────────

    #[test]
    fn plain_program_has_no_counterfactual() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:p(ex:a, Y).\n",
        )
        .unwrap();
        assert!(
            prog.counterfactual.is_none(),
            "a plain v4 goal must not be a counterfactual"
        );
    }

    #[test]
    fn parse_counterfactual_with_assume_and_depth() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/wc/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:mitigation(ex:x, ex:failed)).\n\
             :- assume(ex:control(ex:y, ex:absent)).\n\
             :- depth_budget(3).\n\
             ?- ex:harm(ex:y, Z).\n",
        )
        .unwrap();
        let cf = prog.counterfactual.expect("counterfactual must be parsed");
        assert_eq!(cf.cf_world, "<http://world/cf>");
        assert_eq!(cf.base_world, "<http://world/base>");
        assert_eq!(cf.antecedent.len(), 2, "two assume(...) atoms");
        assert_eq!(cf.antecedent[0].pred, "https://example.org/wc/mitigation");
        assert_eq!(
            cf.antecedent[0].args[1],
            QTerm::Const("<https://example.org/wc/failed>".to_owned())
        );
        assert_eq!(cf.depth_budget, Some(3));
        // The goal φ is an ordinary atom resolved inside W_cf.
        assert_eq!(prog.goal.atoms[0].pred, "https://example.org/wc/harm");
    }

    #[test]
    fn counterfactual_defaults_depth_budget_to_none() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- assume(ex:a(ex:s, ex:o)).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap();
        let cf = prog.counterfactual.unwrap();
        assert_eq!(cf.depth_budget, None);
        assert_eq!(cf.cf_world, "<https://example.org/cf>");
    }

    #[test]
    fn reject_variable_in_antecedent() {
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- assume(ex:a(ex:s, O)).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap_err();
        assert!(err.message().contains("ground"), "unexpected error: {err}");
    }

    #[test]
    fn reject_assume_without_counterfactual() {
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- assume(ex:a(ex:s, ex:o)).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap_err();
        assert!(
            err.message().contains("without a counterfactual"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reject_depth_budget_without_counterfactual() {
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- depth_budget(2).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap_err();
        assert!(
            err.message().contains("without a counterfactual"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reject_duplicate_counterfactual_directive() {
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- counterfactual(ex:cf2, ex:base).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap_err();
        assert!(
            err.message().contains("more than one"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reject_counterfactual_same_world() {
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:w, ex:w).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap_err();
        assert!(
            err.message().contains("must differ"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reject_unrecognized_directive() {
        // A typo'd directive (here `depth_buget`) must fail loudly rather than be
        // silently dropped — otherwise the intended guardrail vanishes unnoticed.
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- assume(ex:a(ex:s, ex:o)).\n\
             :- depth_buget(2).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap_err();
        assert!(
            err.message().contains("unrecognized directive"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn reject_counterfactual_with_empty_antecedent() {
        // A counterfactual with no assume(...) admits nothing hypothetical: a
        // no-op revision, rejected as malformed.
        let err = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             ?- ex:p(ex:s, Y).\n",
        )
        .unwrap_err();
        assert!(
            err.message().contains("at least one assume"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn answer_set_canonicalize_sorts_bindings() {
        let mut b1 = BTreeMap::new();
        b1.insert("Y".to_owned(), "<https://example.org/c>".to_owned());
        let mut b2 = BTreeMap::new();
        b2.insert("Y".to_owned(), "<https://example.org/a>".to_owned());
        let mut b3 = BTreeMap::new();
        b3.insert("Y".to_owned(), "<https://example.org/b>".to_owned());

        let mut ans = AnswerSet {
            bindings: vec![b1.clone(), b3.clone(), b2.clone()],
            status: BudgetStatus::Ok,
            preservation: crate::result::PreservationClaim::exact(),
            frontier: CompletionFrontier::empty(),
        };
        ans.canonicalize();
        assert_eq!(ans.bindings[0]["Y"], "<https://example.org/a>");
        assert_eq!(ans.bindings[1]["Y"], "<https://example.org/b>");
        assert_eq!(ans.bindings[2]["Y"], "<https://example.org/c>");
    }
}
