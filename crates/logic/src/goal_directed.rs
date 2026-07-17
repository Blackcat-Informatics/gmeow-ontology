// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The goal-directed (backward) demonstrator façade — the production surface that makes
//! the proof-carrying full-FOL backward engine non-dark.
//!
//! The proof-carrying backward engine (`crate::physical::resolve_fol`) and its
//! Curry–Howard proof checker (`crate::physical::proof::check`) are `pub(crate)` behind
//! the private `physical` module, so no other crate can reach them. This module is the
//! single thin, honest `pub` façade over them: it lowers the AUTHORED
//! `logic:ReasoningProgram` corpus (structured — function-symbol — logic programs the flat
//! query text-parser cannot express) into the resolver's `TermDag` via
//! [`evaluate_reasoning_programs`], evaluates each through [`resolve_fol`], validates every
//! answer's proof with [`check`], and projects the checked answers + their
//! content-addressed derivation IRIs into RDF-serializable data the `gmeow-pipeline`
//! `stage-goal-directed` folds into `graph/goal-directed` of `gmeow.gts`.
//!
//! It is NOT a fork of the engine: it constructs programs and reads back the engine's own
//! [`FolOutcome`], never re-implementing resolution. There is exactly ONE production source
//! of goal-directed programs — the authored `logic:ReasoningProgram` cells compiled by
//! `gmeow-logic-compile` (see `slices/grounding/logic/examples/reasoning-programs.ttl`);
//! the earlier hand-interned Rust-constant demonstrator corpus has been removed
//! (GREENFIELD — no second source of goal-directed programs may remain).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gmeow_logic_compile::ir::{EvaluationMode, Formula, ReasoningProgramIr, Term};
use purrdf::TermValue;

use crate::physical::id::{MetaId, NodeId, TermId};
use crate::physical::proof::{check, structured_derivation_iri};
use crate::physical::resolve_fol::{
    FolClause, FolControl, FolLit, FolProgram, Truth, render, resolve_fol,
};
use crate::physical::term_dag::TermDag;
use crate::physical::unify::{SortContext, SortOrder};
use crate::query_ir::Budget;
use crate::rule_ir::{
    EvalAtom, EvalRule, EvalTerm, Fact, FactStore, Solution, least_model_of_reduct, match_atom,
};

/// The gmeow namespace every projected goal-directed IRI/predicate lives under.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The XSD boolean datatype IRI for the proof-checked flag.
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// One checked answer to a demonstrator's goal: the ground answer atom surface, the goal
/// variable bindings, the content-addressed derivation (proof) IRI, and whether the proof
/// [`check`]s to exactly that atom. Every field is RDF-serializable (strings), so the
/// pipeline can fold it without reaching into the engine's private term handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDirectedAnswer {
    /// The ground answer atom rendered to its functional surface, e.g.
    /// `add(s(s(zero)),s(zero),s(s(s(zero))))`.
    pub atom: String,
    /// The goal variable → resolved sub-term surface map (deterministic, sorted keys).
    pub bindings: BTreeMap<String, String>,
    /// The content-addressed derivation IRI of this answer's proof
    /// ([`derivation_iri`] — byte-identical to the forward reasoner's rule-application id).
    pub derivation_iri: String,
    /// Whether the proof [`check`]ed and re-derived exactly [`Self::atom`]. Always `true`
    /// for a shipped answer (a proof that fails to check HARD-fails the evaluation).
    pub proof_checks: bool,
}

/// One three-valued well-founded verdict of a probed ground atom under the SLG-WFS model —
/// the observable surface that makes three-valued negation a SHIPPED behaviour. Unlike a
/// two-valued proof-checked answer, a verdict can be `undefined` (an atom trapped in a
/// negative loop), so it is a plain three-valued string surface, never an `xsd:boolean`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDirectedVerdict {
    /// The probed ground atom rendered to its functional surface, e.g. `win(a)`.
    pub atom: String,
    /// The well-founded verdict: `true`, `false`, or `undefined`.
    pub verdict: String,
}

/// One evaluated goal-directed demonstrator: its stable name, prose description, rendered
/// goal template, budget status, every proof-checked answer, and any probed WFS verdicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDirectedEvaluation {
    /// The stable demonstrator name (a URI path segment; also the query IRI local part).
    pub name: String,
    /// The prose description of what the demonstrator demonstrates.
    pub description: String,
    /// The rendered goal template (free metavariables shown as `?n`), e.g.
    /// `add(s(s(zero)),s(zero),?0)`.
    pub goal: String,
    /// The budget status of the resolution (`ok` / `partial` / `exhausted`).
    pub status: String,
    /// The proof-checked answers, sorted by [`GoalDirectedAnswer::atom`] for determinism.
    pub answers: Vec<GoalDirectedAnswer>,
    /// The probed three-valued WFS verdicts, sorted by [`GoalDirectedVerdict::atom`] for
    /// determinism. Non-empty only for a negation demonstrator (e.g. `win`/`move`), where it
    /// carries the `undefined` loop atoms alongside the founded `true`/`false` atoms.
    pub verdicts: Vec<GoalDirectedVerdict>,
    /// The AUTHORED program's own clauses, each rendered to its functional surface via the
    /// SAME [`render`] helper the answers use (`"head."` for a fact, `"head :- b1, b2."` for
    /// a rule, a negation-as-failure body literal rendered `"not atom"`), in the program's own
    /// (authored, sort-key-canonical) clause order. [`project_goal_directed`] projects these
    /// alongside the evaluated answers/verdicts, so the shipped bundle carries "here is the
    /// authored program" and not only "here is its well-founded model".
    pub clauses: Vec<String>,
    /// Every `logic:verdictProbe` atom's rendered functional surface (the SAME rendering
    /// [`GoalDirectedVerdict::atom`] carries), independent of the evaluated verdict value —
    /// the authored PROGRAM structure's probe set, not its evaluation. Empty for a program
    /// with no verdict probes.
    pub verdict_probe_atoms: Vec<String>,
}

/// A fully interned demonstrator: its structured program, the order-sorted context to
/// resolve it under (`SortContext::default()` for the unsorted demonstrators), and the ground
/// atoms whose three-valued WFS verdict is projected. Building interns everything into one
/// fresh [`TermDag`] so every returned [`NodeId`] belongs to `dag`.
struct BuiltDemonstrator {
    /// The demonstrator's own term arena.
    dag: TermDag,
    /// The structured backward program (clauses + goal + goal vars + meta-sorts).
    program: FolProgram,
    /// The order-sorted lattice/tagging the resolver consults (empty ⇒ the unsorted path).
    ctx: SortContext,
    /// Ground atoms whose SLG-WFS verdict is projected (`true`/`false`/`undefined`). Empty for
    /// a purely-positive demonstrator; non-empty for the negation demonstrator so its
    /// `undefined` loop atoms and founded atoms are both observable.
    verdict_probes: Vec<NodeId>,
}

/// Evaluate one built program (lowered from a compiled `logic:ReasoningProgram` by
/// [`lower_reasoning_program`]): resolve its goal, validate + project each answer, and
/// record each verdict probe's three-valued WFS verdict. `name` / `description` are taken as
/// plain string slices (rather than folded into [`BuiltDemonstrator`] itself) because a
/// compiled program's identity is a runtime `String` (its authored IRI).
fn evaluate_demonstrator(
    name: &str,
    description: &str,
    built: BuiltDemonstrator,
) -> gmeow_errors::Result<GoalDirectedEvaluation> {
    let BuiltDemonstrator {
        mut dag,
        program,
        ctx,
        verdict_probes,
    } = built;
    // Render the goal template BEFORE resolution (free metavariables still present).
    let goal = render(&dag, program.goal);
    // U2: render the AUTHORED program structure itself (clauses + verdict-probe atoms) via
    // the SAME `render` helper the goal/answers use, so the shipped bundle carries the
    // authored program alongside its evaluated result. Rendered from `program`/`dag` BEFORE
    // resolution mutates `dag` further — the clause/probe NodeIds are unaffected either way
    // (the arena only grows), but this mirrors `goal`'s own pre-resolution rendering.
    let clauses: Vec<String> = program
        .clauses
        .iter()
        .map(|clause| render_clause(&dag, clause))
        .collect();
    let verdict_probe_atoms: Vec<String> = verdict_probes
        .iter()
        .map(|probe| render(&dag, *probe))
        .collect();
    let outcome = match resolve_fol(&mut dag, &program, &ctx, &Budget::default())? {
        FolControl::Decided(outcome) => outcome,
        FolControl::Unsupported(kind) => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed program {name:?} is unsupported by the backward engine: {kind:?}"
                ),
            }));
        }
    };
    let status = outcome.status.as_str().to_owned();
    let mut answers = Vec::with_capacity(outcome.answers.len());
    for ans in &outcome.answers {
        // Curry–Howard check: the proof MUST re-derive exactly the answer atom. A proof
        // that fails to check, or checks to a different atom, is a hard fail — the whole
        // point of shipping proof objects is that every shipped answer is proof-carrying.
        let checked = check(&mut dag, ans.proof, &outcome.rule_ctx).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed program {name:?} answer proof failed to check: {e:?}"
                ),
            })
        })?;
        if checked != ans.atom {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed program {name:?} proof re-derives a different atom than its answer"
                ),
            }));
        }
        let derivation_iri = structured_derivation_iri(&dag, ans.proof)?;
        answers.push(GoalDirectedAnswer {
            atom: render(&dag, ans.atom),
            bindings: ans.bindings.clone(),
            derivation_iri,
            proof_checks: true,
        });
    }
    answers.sort_by(|a, b| a.atom.cmp(&b.atom));
    // Record the three-valued well-founded verdict of each probed ground atom. `truth_of`
    // reads the well-founded model by content key, so a probe the grounding never founded is
    // (correctly) `false`, an atom in a negative loop is `undefined`, and a founded atom is
    // `true` — the observable SLG-WFS behaviour.
    let mut verdicts = Vec::with_capacity(verdict_probes.len());
    for probe in &verdict_probes {
        let verdict = match outcome.truth_of(&dag, *probe) {
            Truth::True => "true",
            Truth::False => "false",
            Truth::Undefined => "undefined",
        };
        verdicts.push(GoalDirectedVerdict {
            atom: render(&dag, *probe),
            verdict: verdict.to_owned(),
        });
    }
    verdicts.sort_by(|a, b| a.atom.cmp(&b.atom));
    Ok(GoalDirectedEvaluation {
        name: name.to_owned(),
        description: description.to_owned(),
        goal,
        status,
        answers,
        verdicts,
        clauses,
        verdict_probe_atoms,
    })
}

/// Render one program clause to its authored functional surface via the SAME [`render`]
/// helper the answer/goal surfaces use, so the projected program-structure text is
/// byte-consistent with the evaluated answers: `"head."` for a fact (empty body), and
/// `"head :- b1, b2."` for a rule, with a negation-as-failure body literal rendered
/// `"not atom"` (mirrors the authored `logic:not[atom]` surface).
fn render_clause(dag: &TermDag, clause: &FolClause) -> String {
    let head = render(dag, clause.head);
    if clause.body.is_empty() {
        return format!("{head}.");
    }
    let body: Vec<String> = clause
        .body
        .iter()
        .map(|lit| match lit {
            FolLit::Pos(node) => render(dag, *node),
            FolLit::Neg(node) => format!("not {}", render(dag, *node)),
        })
        .collect();
    format!("{head} :- {}.", body.join(", "))
}

/// Intern an atomic IRI leaf under a program-local surface name.
fn leaf(dag: &mut TermDag, s: &str) -> NodeId {
    dag.intern_leaf(TermValue::iri(s.to_owned()))
}

// ─────────────────────────────────────────────────────────────────────────────────────
// The reasoning-program compiler: `ReasoningProgramIr` → `BuiltDemonstrator` (Task 4).
// ─────────────────────────────────────────────────────────────────────────────────────
//
// This is the SOLE production source of goal-directed programs: it lowers an
// authored+compiled `logic:ReasoningProgram` (`gmeow_logic_compile::ir::ReasoningProgramIr`,
// Task 3) into the `FolProgram`/`SortContext`/verdict-probe shape [`evaluate_demonstrator`]
// resolves, proof-checks, verdict-probes, and projects — there is no second engine, and
// (as of Task 7) no second SOURCE either: the earlier hand-interned Rust-constant
// demonstrator corpus has been removed.
//
// ## One lowering, policy-parameterized on the free-variable seam
//
// `crate::physical::lower::lower_logic_term`/`lower_logic_formula` is THE single production
// `Term::Iri`/`Term::Literal`/`Term::Var`/`Term::App`/`Formula::Atom` lowering into the
// shared [`TermDag`] arena — this module never re-implements it. What differs here is only
// the FREE-VARIABLE policy: `lower_logic_term`'s default `Term::Var` fallback (an unbound
// name interns as a RIGID `NodeData::Free` leaf — `crate::physical::unify`'s unification rule
// is that `Bound`/`Leaf`/`Free` unify only by equality, never bind) is right for `logic:` text
// authored under an explicit binder, but wrong for a `logic:ReasoningProgram` clause/query,
// which is an implicitly-universally-quantified Horn clause with NO explicit `Forall`
// wrapper — every one of its variables is "free" from the lowering's point of view, yet the
// backward engine needs each to be a `NodeData::Meta` metavariable (`resolve_fol` unifies
// goal/clause atoms via `unify_sorted`, whose only bindable node kind is `NodeData::Meta`).
//
// `lower_logic_formula_with`/`lower_logic_term_with` (`crate::physical::lower`) expose exactly
// this as a policy seam: a `free: &mut dyn FnMut(&mut TermDag, &str) -> Result<NodeId>`
// closure invoked ONLY when a `Term::Var` has no enclosing `Forall`/`Exists` binder frame. This
// module supplies that closure per clause/query/probe, backed by a [`VarScope`]: the FIRST
// occurrence of a name in one scope mints a fresh [`TermDag::fresh_meta`], every LATER
// occurrence of that SAME name in the SAME scope reuses it, and a fresh [`VarScope`] per
// clause/query/probe means the SAME name in two DIFFERENT clauses mints two DIFFERENT
// metavariables. The `Bound`/de-Bruijn path in `lower.rs` is untouched — quantified
// sub-formulas inside a clause (if any) still resolve through the shared de-Bruijn machinery.

/// The per-scope variable→metavariable map a single clause/query/probe lowers under: the
/// FIRST occurrence of a name mints a fresh metavariable ([`TermDag::fresh_meta`]); every
/// later occurrence of the SAME name in the SAME scope reuses it. A fresh, empty map per
/// clause/query/probe is what keeps two clauses' same-named variables from colliding.
type VarScope = HashMap<String, (MetaId, NodeId)>;

/// Lower an atomic `logic:` [`Formula::Atom`] under `scope` into an `App` node, HARD-FAILING
/// on any other formula shape — the backward engine's clause head / body literal / query /
/// verdict-probe position all require exactly one atomic predication. The actual
/// `Term::Iri`/`Term::Literal`/`Term::Var`/`Term::App` lowering is
/// `crate::physical::lower::lower_logic_formula_with`'s (the shared production seam); this
/// wrapper supplies ONLY the free-variable policy — mint-or-reuse a metavariable in `scope` —
/// and the atomic-shape assertion the shared lowering (which also accepts compound formulas,
/// for its `math:`/`lang:` callers) does not itself enforce.
fn lower_atom(
    dag: &mut TermDag,
    formula: &Formula,
    scope: &mut VarScope,
) -> gmeow_errors::Result<NodeId> {
    if !matches!(formula, Formula::Atom { .. }) {
        return Err(reasoning_program_err(format!(
            "reasoning-program atom position requires an atomic logic:Formula (a single \
             predication); found a compound formula {formula:?}"
        )));
    }
    let mut free = |dag: &mut TermDag, name: &str| -> gmeow_errors::Result<NodeId> {
        Ok(scope
            .entry(name.to_owned())
            .or_insert_with(|| dag.fresh_meta())
            .1)
    };
    crate::physical::lower::lower_logic_formula_with(dag, formula, &mut free)
}

/// Lower a rule antecedent under `scope` into `out`'s [`FolLit`]s: a conjunction flattens
/// (mirroring `crate::physical::lower::flatten_commutative`) into its conjuncts; a bare atom
/// is a single positive literal; `logic:not[atom]` is a single negation-as-failure literal.
/// Any other body shape (a nested `And`/`Or`/`Implies`/`Iff`/quantifier, or a `Not` wrapping a
/// non-atomic formula) exceeds the backward engine's Horn+NAF fragment and is a HARD FAIL —
/// never silently dropped or approximated.
fn lower_body(
    dag: &mut TermDag,
    formula: &Formula,
    scope: &mut VarScope,
    out: &mut Vec<FolLit>,
) -> gmeow_errors::Result<()> {
    match formula {
        Formula::And(parts) => {
            for p in parts {
                lower_body(dag, p, scope, out)?;
            }
            Ok(())
        }
        Formula::Atom { .. } => {
            out.push(FolLit::Pos(lower_atom(dag, formula, scope)?));
            Ok(())
        }
        Formula::Not(inner) if matches!(inner.as_ref(), Formula::Atom { .. }) => {
            out.push(FolLit::Neg(lower_atom(dag, inner, scope)?));
            Ok(())
        }
        other => Err(reasoning_program_err(format!(
            "reasoning-program rule body literal must be an atomic logic:Formula or its \
             negation-as-failure (logic:not[atom]); found an unsupported body shape outside \
             the backward engine's Horn+NAF fragment: {other:?}"
        ))),
    }
}

/// Lower one clause [`Formula`] (a fact atom, or `Formula::Implies(antecedent, consequent)`
/// rule) under a FRESH [`VarScope`] into a [`FolClause`], and mint its content-addressed
/// `rule_iri` from the clause's own [`Formula::content_key`] — never from a [`NodeId`]/
/// [`TermId`] index, so the same authored clause always mints the same rule identity run to
/// run (pre-satisfies the authored path's content-addressing requirement independent of
/// interning order).
fn lower_clause(
    dag: &mut TermDag,
    program_iri: &str,
    clause: &Formula,
    scope: &mut VarScope,
) -> gmeow_errors::Result<FolClause> {
    let (head_formula, body) = match clause {
        Formula::Atom { .. } => (clause, Vec::new()),
        Formula::Implies(antecedent, consequent) => {
            let mut body = Vec::new();
            lower_body(dag, antecedent, scope, &mut body)?;
            (consequent.as_ref(), body)
        }
        other => {
            return Err(reasoning_program_err(format!(
                "reasoning program {program_iri} clause must be an atomic fact or a \
                 logic:antecedent/logic:consequent rule; found an unsupported clause shape: \
                 {other:?}"
            )));
        }
    };
    let head = lower_atom(dag, head_formula, scope)?;
    let rule_iri = content_addressed_rule_iri(dag, program_iri, clause);
    Ok(FolClause {
        head,
        body,
        rule_iri,
    })
}

/// Mint a content-addressed rule-IRI handle for `clause`: `blake3` over the owning program's
/// IRI plus the clause's own [`Formula::content_key`] (alpha- and order-normalized), so the
/// SAME authored clause always mints the SAME rule identity regardless of interning/mint
/// order — never a [`NodeId`]/[`TermId`] index, which is an interning-order artifact.
fn content_addressed_rule_iri(dag: &mut TermDag, program_iri: &str, clause: &Formula) -> TermId {
    let key = format!("{program_iri}\u{0}{}", clause.content_key());
    let hash = blake3::hash(key.as_bytes()).to_hex();
    dag.intern_atom(&TermValue::iri(format!("{GMEOW}goal-directed/rule/{hash}")))
}

/// A reasoning-program-compiler diagnostic, routed through the same `logic.ir` kind
/// `crate::physical::lower` uses for its own lowering defects.
fn reasoning_program_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Ir { detail })
}

/// The local (last path-segment) name of an IRI, falling back to the whole IRI when it
/// carries no `/`/`#` separator (or ends with one) — the [`GoalDirectedEvaluation::name`]
/// surface for a compiled reasoning program.
fn local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(iri)
}

/// Lower one compiled [`ReasoningProgramIr`] into a [`BuiltDemonstrator`]: its own fresh
/// [`TermDag`], the compiled [`FolProgram`], the [`SortContext`] seeded from `subsort_edges`
/// (the caller's reasoned `rdfs:subClassOf`
/// closure, narrowed to the sorts this program's `variable_sorts` actually reference — the
/// narrowing is the caller's job, per M5/F-4), and the lowered verdict probes.
///
/// Every clause, the query, and every verdict probe lowers under its OWN fresh [`VarScope`]
/// (see the module-level note above): a variable name is shared only within the ONE
/// clause/query/probe that authored it.
fn lower_reasoning_program(
    program: &ReasoningProgramIr,
    subsort_edges: &[(String, String)],
) -> gmeow_errors::Result<BuiltDemonstrator> {
    // `EvaluationMode` is a closed, single-variant enum today — `EvaluationMode::from_local`
    // (the ONLY constructor reachable from parsed input) rejects every IRI except
    // `logic:BackwardEvaluation` at Task 3's parse stage, so a `ReasoningProgramIr` carrying
    // any other mode is already unrepresentable by construction. This irrefutable pattern
    // documents that exhaustively: it is a COMPILE ERROR (not a runtime `unreachable!`) the
    // day a second `EvaluationMode` variant lands without this dispatch being extended.
    let EvaluationMode::Backward = program.mode;

    let mut dag = TermDag::new();
    let mut meta_sorts: HashMap<MetaId, NodeId> = HashMap::new();
    let var_sort_map: HashMap<&str, &str> = program
        .variable_sorts
        .iter()
        .map(|(v, s)| (v.as_str(), s.as_str()))
        .collect();

    let mut clauses = Vec::with_capacity(program.clauses.len());
    for clause_formula in &program.clauses {
        let mut scope: VarScope = HashMap::new();
        let clause = lower_clause(&mut dag, &program.iri, clause_formula, &mut scope)?;
        for (name, (meta, _)) in &scope {
            if let Some(sort_iri) = var_sort_map.get(name.as_str()) {
                let sort_node = leaf(&mut dag, sort_iri);
                meta_sorts.insert(*meta, sort_node);
            }
        }
        clauses.push(clause);
    }

    let mut query_scope: VarScope = HashMap::new();
    let goal = lower_atom(&mut dag, &program.query, &mut query_scope)?;
    for (name, (meta, _)) in &query_scope {
        if let Some(sort_iri) = var_sort_map.get(name.as_str()) {
            let sort_node = leaf(&mut dag, sort_iri);
            meta_sorts.insert(*meta, sort_node);
        }
    }
    let mut goal_vars: Vec<(NodeId, String)> = query_scope
        .into_iter()
        .map(|(name, (_, node))| (node, name))
        .collect();
    // Deterministic order: sorted by the surface variable name (never HashMap iteration
    // order, which is not reproducible run to run).
    goal_vars.sort_by(|a, b| a.1.cmp(&b.1));

    let mut verdict_probes = Vec::with_capacity(program.verdict_probes.len());
    for probe_formula in &program.verdict_probes {
        let mut scope: VarScope = HashMap::new();
        let probe = lower_atom(&mut dag, probe_formula, &mut scope)?;
        for (name, (meta, _)) in &scope {
            if let Some(sort_iri) = var_sort_map.get(name.as_str()) {
                let sort_node = leaf(&mut dag, sort_iri);
                meta_sorts.insert(*meta, sort_node);
            }
        }
        verdict_probes.push(probe);
    }

    // The order-sorted lattice: `subsort_edges` is the caller's already-computed reasoned
    // `rdfs:subClassOf` closure (narrowed to the sorts this program references), lowered to
    // NodeIds here (hash-consing makes this idempotent with the sort leaves interned above,
    // so a shared sort IRI always resolves to the SAME node). `SortOrder::from_subclass_edges`
    // computes its own reflexive-transitive closure — nothing about the lattice is hardcoded.
    let mut edges: Vec<(NodeId, NodeId)> = Vec::with_capacity(subsort_edges.len());
    for (sub, sup) in subsort_edges {
        edges.push((leaf(&mut dag, sub), leaf(&mut dag, sup)));
    }
    let order = SortOrder::from_subclass_edges(&edges);

    // CONSTANT order-sort tagging (`SortContext::term_sorts`): `program.constant_sorts` is
    // Task 4's `(constant IRI, rdf:type IRI)` capture — the plain domain `rdf:type` triple a
    // constant like `ex:one` carries, which the stage's L3 fold otherwise drops (it is not
    // `logic:` structural vocabulary). Interning each constant/sort IRI through the SAME
    // `leaf` helper `lower_atom`'s `Term::Iri` arm uses means hash-consing resolves a
    // constant referenced both here and inside a clause/query/probe to the IDENTICAL
    // `NodeId` — this is what lets `unify_sorted` discriminate a typed constant (only
    // unifiable with a variable whose declared sort is ⊒ its own) from an untyped one
    // (order-sort top, unifies with any variable sort) instead of every constant being
    // silently untyped and unification degenerating to unsorted unification.
    let mut term_sorts: HashMap<NodeId, NodeId> = HashMap::new();
    for (const_iri, sort_iri) in &program.constant_sorts {
        let const_node = leaf(&mut dag, const_iri);
        let sort_node = leaf(&mut dag, sort_iri);
        term_sorts.insert(const_node, sort_node);
    }
    let ctx = SortContext::new(order, term_sorts, HashMap::new());

    Ok(BuiltDemonstrator {
        dag,
        program: FolProgram {
            clauses,
            goal,
            goal_vars,
            meta_sorts,
        },
        ctx,
        verdict_probes,
    })
}

/// Evaluate a compiled set of `logic:ReasoningProgram`s — the authored clause-set-plus-goal
/// surface (Tasks 1-3), the SOLE production source of goal-directed programs — against the
/// reasoned `rdfs:subClassOf` closure (`subsort_edges`, narrowed by the caller to the sorts
/// these programs actually reference). [`lower_reasoning_program`] compiles each program into
/// the exact shape [`evaluate_demonstrator`] resolves, proof-checks, verdict-probes, and
/// projects — there is no second engine. `Unsupported` stays a HARD FAIL (surfaced by
/// [`evaluate_demonstrator`]).
pub fn evaluate_reasoning_programs(
    programs: &[ReasoningProgramIr],
    subsort_edges: &[(String, String)],
) -> gmeow_errors::Result<Vec<GoalDirectedEvaluation>> {
    let mut evals = Vec::with_capacity(programs.len());
    for program in programs {
        let built = lower_reasoning_program(program, subsort_edges)?;
        let description = format!(
            "Compiled logic:ReasoningProgram {} ({} clause(s), {} evaluation).",
            program.iri,
            program.clauses.len(),
            program.mode.as_str(),
        );
        let eval = evaluate_demonstrator(local_name(&program.iri), &description, built)?;
        // T3: the cross-engine (backward-vs-forward) fixpoint-agreement oracle. Only a
        // program inside the forward-evaluable definite/function-free/binary fragment is
        // checked (see `is_definite_function_free_binary`'s doc for exactly which shipped
        // programs qualify); every other program is evaluated exactly as before.
        if is_definite_function_free_binary(program) {
            cross_check_forward_agreement(program, &eval)?;
        }
        evals.push(eval);
    }
    evals.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(evals)
}

// ─────────────────────────────────────────────────────────────────────────────────────
// T3 — the cross-engine fixpoint-agreement oracle (definite, function-free fragment).
// ─────────────────────────────────────────────────────────────────────────────────────
//
// A shipped, dogfooded soundness+completeness invariant for the fragment `rule_ir`'s forward
// chase (`least_model_of_reduct`, the same engine `relational_core::lower_formulas` feeds)
// can actually evaluate: FUNCTION-FREE definite Horn clauses with fixed-arity BINARY atoms.
// For every authored `logic:ReasoningProgram` inside that fragment, this independently
// computes the forward least model over the program's OWN lowered clauses and HARD-FAILS if
// it disagrees with the backward SLG answer set `evaluate_demonstrator` already produced —
// never a second source of truth for the shipped answers, only a cross-check that the two
// engines agree on the fragment both can evaluate.

/// Whether `program` is inside the T3 oracle's forward-evaluable fragment: no
/// `logic:verdictProbe`s, no negation-as-failure (`Formula::Not`) anywhere in its clauses, no
/// compound function-term application (`Term::App`) anywhere in its clauses/query, and every
/// atom (every clause head/body literal, plus the query) is exactly binary. This is precisely
/// the "function-free definite Horn clauses with fixed-arity (binary) atoms" fragment
/// `rule_ir::least_model_of_reduct` evaluates:
///
/// - `peano-add`/`member-cons` fail on `Term::App` (their `s(...)`/`cons(...)` function
///   symbols) — correctly SKIPPED;
/// - `win-wfs-negation` fails on both `Formula::Not` and its non-empty `verdictProbe`s —
///   correctly SKIPPED;
/// - `math-subsort`/`math-subsort-control`'s `p(X)` is UNARY, not binary — correctly SKIPPED
///   (a unary atom would need the n-ary reifier/restricted-chase lane this oracle
///   deliberately does not exercise);
/// - `reachability`'s `edge`/`reach` clauses and query are all definite, function-free,
///   binary atoms — the one shipped program the oracle actually cross-checks.
fn is_definite_function_free_binary(program: &ReasoningProgramIr) -> bool {
    if !program.verdict_probes.is_empty() {
        return false;
    }
    if !formula_is_binary_atom(&program.query) {
        return false;
    }
    program
        .clauses
        .iter()
        .all(clause_is_definite_function_free_binary)
}

/// A clause is inside the fragment iff its head is a binary atom and (for a rule) its
/// antecedent is a conjunction of binary atoms — never a negation, disjunction, or nested
/// quantifier.
fn clause_is_definite_function_free_binary(clause: &Formula) -> bool {
    match clause {
        Formula::Atom { .. } => formula_is_binary_atom(clause),
        Formula::Implies(antecedent, consequent) => {
            formula_is_binary_atom(consequent) && body_is_definite_function_free_binary(antecedent)
        }
        _ => false,
    }
}

/// A rule antecedent is inside the fragment iff it is a (possibly-flattened) conjunction of
/// binary atoms — no `Formula::Not` (negation-as-failure), disjunction, or quantifier.
fn body_is_definite_function_free_binary(body: &Formula) -> bool {
    match body {
        Formula::And(parts) => parts.iter().all(body_is_definite_function_free_binary),
        Formula::Atom { .. } => formula_is_binary_atom(body),
        _ => false,
    }
}

/// `true` iff `formula` is a single atomic predication with an IRI relation and exactly two
/// function-free (`Var`/`Iri`/`Literal`) arguments.
fn formula_is_binary_atom(formula: &Formula) -> bool {
    match formula {
        Formula::Atom { relation, args } => {
            matches!(relation, Term::Iri(_))
                && args.len() == 2
                && args.iter().all(term_is_function_free)
        }
        _ => false,
    }
}

/// `true` for a term the oracle's binary EDB/rule lowering can represent directly: a
/// variable, an IRI constant, or a data literal — never a compound [`Term::App`] or a
/// [`Term::SequenceMarker`].
fn term_is_function_free(term: &Term) -> bool {
    matches!(term, Term::Var(_) | Term::Iri(_) | Term::Literal { .. })
}

/// Lower one gated (definite, function-free, binary) [`Formula`] atom to an [`EvalAtom`]:
/// an IRI relation becomes the predicate, and each of its exactly-two arguments lowers via
/// [`oracle_eval_term`]. HARD-fails if `formula` is not a binary atom — a defensive re-check
/// of [`formula_is_binary_atom`], which every caller has already gated on.
fn oracle_eval_atom(formula: &Formula) -> gmeow_errors::Result<EvalAtom> {
    let Formula::Atom { relation, args } = formula else {
        return Err(reasoning_program_err(format!(
            "T3 cross-engine oracle: expected an atomic logic:Formula, found {formula:?}"
        )));
    };
    let Term::Iri(predicate) = relation else {
        return Err(reasoning_program_err(
            "T3 cross-engine oracle: an atom's relation must be a Term::Iri".to_owned(),
        ));
    };
    if args.len() != 2 {
        return Err(reasoning_program_err(format!(
            "T3 cross-engine oracle: atom {predicate} is not binary (arity {}); outside the \
             oracle's fixed-arity fragment",
            args.len()
        )));
    }
    Ok(EvalAtom {
        subject: oracle_eval_term(&args[0])?,
        predicate: predicate.clone(),
        object: oracle_eval_term(&args[1])?,
        negated: false,
    })
}

/// Lower a function-free [`Term`] to an [`EvalTerm`]: a variable stays a variable (`?`-sigil
/// prefixed, matching [`EvalTerm::Var`]'s surface convention), an IRI becomes a named
/// constant, and a literal becomes a constant literal (typed when a datatype is authored).
fn oracle_eval_term(term: &Term) -> gmeow_errors::Result<EvalTerm> {
    match term {
        Term::Var(name) => Ok(EvalTerm::Var(format!("?{name}"))),
        Term::Iri(iri) => Ok(EvalTerm::ConstNamed(iri.clone())),
        Term::Literal { lexical, datatype } => Ok(EvalTerm::ConstLit(match datatype {
            Some(dt) => TermValue::typed_literal(lexical.clone(), dt.clone()),
            None => TermValue::simple_literal(lexical.clone()),
        })),
        other => Err(reasoning_program_err(format!(
            "T3 cross-engine oracle: term {other:?} is outside the function-free binary \
             fragment (every caller gates on `term_is_function_free` before reaching this)"
        ))),
    }
}

/// Flatten a rule antecedent into its [`EvalAtom`] body, mirroring [`lower_body`]'s
/// conjunction-flattening but restricted to the DEFINITE fragment: a conjunction flattens
/// into its conjuncts, a bare atom is a single body literal, and anything else (in
/// particular a negation) is a hard fail — every caller has already gated on
/// [`body_is_definite_function_free_binary`].
fn oracle_body_atoms(formula: &Formula, out: &mut Vec<EvalAtom>) -> gmeow_errors::Result<()> {
    match formula {
        Formula::And(parts) => {
            for part in parts {
                oracle_body_atoms(part, out)?;
            }
            Ok(())
        }
        Formula::Atom { .. } => {
            out.push(oracle_eval_atom(formula)?);
            Ok(())
        }
        other => Err(reasoning_program_err(format!(
            "T3 cross-engine oracle: rule body literal must be a conjunction of atomic \
             formulas (the definite fragment); found {other:?}"
        ))),
    }
}

/// Lower one gated clause [`Formula`] (a bare fact atom, or an `Implies(antecedent,
/// consequent)` rule) into an [`EvalRule`] for the forward chase. `idx` seeds a purely
/// internal, non-content-addressed `rule_iri`: it is never projected or otherwise observed
/// outside this in-engine computation (the oracle compares the resulting FACT SET only), so
/// positional naming here carries none of the determinism risk it would for shipped bundle
/// content.
fn oracle_eval_rule(idx: usize, clause: &Formula) -> gmeow_errors::Result<EvalRule> {
    let (head_formula, body) = match clause {
        Formula::Atom { .. } => (clause, Vec::new()),
        Formula::Implies(antecedent, consequent) => {
            let mut body = Vec::new();
            oracle_body_atoms(antecedent, &mut body)?;
            (consequent.as_ref(), body)
        }
        other => {
            return Err(reasoning_program_err(format!(
                "T3 cross-engine oracle: clause must be an atomic fact or an \
                 antecedent/consequent rule; found {other:?}"
            )));
        }
    };
    Ok(EvalRule {
        head: oracle_eval_atom(head_formula)?,
        body,
        rule_iri: format!("{GMEOW}goal-directed/oracle-rule/{idx}"),
        distinct_pairs: Vec::new(),
        builtins: Vec::new(),
    })
}

/// Render one forward-derived ground [`Fact`] to the SAME functional surface
/// `render`/`GoalDirectedAnswer::atom` uses (`pred(subject,object)`, bare IRI text), so the
/// forward and backward answer sets compare as plain strings.
fn oracle_render_fact(fact: &Fact) -> String {
    format!(
        "{}({},{})",
        fact.predicate,
        oracle_term_bare(&fact.subject),
        oracle_term_bare(&fact.object)
    )
}

/// The bare (unbracketed) surface of a ground [`TermValue`]: an IRI's plain string, or (for
/// the fragment's other legal ground term, a literal) its display form.
fn oracle_term_bare(value: &TermValue) -> String {
    match value {
        TermValue::Iri(iri) => iri.clone(),
        other => crate::provenance::term_display(other),
    }
}

/// Compute the forward least model of `program`'s OWN lowered (definite, function-free,
/// binary) clauses via `rule_ir::least_model_of_reduct` — the identical forward-chase engine
/// `relational_core::lower_formulas` feeds — project it onto the query atom, and HARD-FAIL if
/// that set disagrees with the backward SLG answer set already computed into `eval.answers`.
///
/// Every clause becomes an [`EvalRule`] directly (no separate `edb`: a fact clause is simply
/// a zero-body rule, which `least_model_of_reduct`'s join fires unconditionally in round one)
/// and there is no negation to guard (the DEFINITE gate already excludes it), so `reference`
/// is passed as an empty store too.
fn cross_check_forward_agreement(
    program: &ReasoningProgramIr,
    eval: &GoalDirectedEvaluation,
) -> gmeow_errors::Result<()> {
    let rules: Vec<EvalRule> = program
        .clauses
        .iter()
        .enumerate()
        .map(|(idx, clause)| oracle_eval_rule(idx, clause))
        .collect::<gmeow_errors::Result<_>>()?;
    let empty = FactStore::new();
    let result = least_model_of_reduct(&empty, &rules, &empty)?;

    let query_atom = oracle_eval_atom(&program.query)?;
    let probe_sol = Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    };
    let mut forward: BTreeSet<String> = BTreeSet::new();
    for &i in result.store.facts_for_predicate(&query_atom.predicate) {
        let fact = &result.store.facts()[i];
        if match_atom(&query_atom, fact, &probe_sol).is_some() {
            forward.insert(oracle_render_fact(fact));
        }
    }
    let backward: BTreeSet<String> = eval.answers.iter().map(|ans| ans.atom.clone()).collect();
    if forward != backward {
        return Err(reasoning_program_err(format!(
            "goal-directed program {:?} FAILED the T3 cross-engine fixpoint-agreement oracle: \
             the backward SLG answer set {backward:?} does not equal the forward \
             relational-core least model's projection onto the query atom {forward:?}",
            program.iri
        )));
    }
    Ok(())
}

/// The query individual IRI of a demonstrator.
fn query_iri(name: &str) -> String {
    format!("{GMEOW}goal-directed/{name}")
}

/// The `n`-th answer individual IRI of a demonstrator.
fn answer_iri(name: &str, idx: usize) -> String {
    format!("{GMEOW}goal-directed/{name}/answer/{idx}")
}

/// The `n`-th WFS-verdict individual IRI of a demonstrator (`n` in sorted-atom order).
fn verdict_iri(name: &str, idx: usize) -> String {
    format!("{GMEOW}goal-directed/{name}/verdict/{idx}")
}

/// U2: mint a content-addressed `gmeow:GoalDirectedProgram` IRI from the evaluation's OWN
/// rendered program text (its name, clauses, query, and verdict-probe atoms) — a `blake3`
/// hash, never a [`NodeId`]/index, so the SAME authored program always mints the SAME
/// program IRI regardless of interning/evaluation order (byte-stable across independent
/// runs, exactly like [`content_addressed_rule_iri`]).
fn program_iri_for(eval: &GoalDirectedEvaluation) -> String {
    let mut key = String::new();
    key.push_str(&eval.name);
    for clause in &eval.clauses {
        key.push('\u{0}');
        key.push_str(clause);
    }
    key.push('\u{0}');
    key.push_str(&eval.goal);
    for probe in &eval.verdict_probe_atoms {
        key.push('\u{0}');
        key.push_str(probe);
    }
    let hash = blake3::hash(key.as_bytes()).to_hex();
    format!("{GMEOW}goal-directed/program/{hash}")
}

/// Project evaluated demonstrators into deterministic (sorted) N-Triples for the
/// `graph/goal-directed` fold. Each demonstrator is a `gmeow:GoalDirectedQuery` carrying
/// its description, goal template, and status; each answer is a `gmeow:GoalDirectedAnswer`
/// carrying its ground atom, bindings, the proof-derivation IRI, and the proof-checked
/// flag. No new predicate is invented beyond this small self-consistent set; the goal /
/// atom / binding surfaces ride as plain string literals, the derivation as an IRI.
pub fn project_goal_directed(evals: &[GoalDirectedEvaluation]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let p = |pred: &str| format!("{GMEOW}{pred}");
    for eval in evals {
        let q = query_iri(&eval.name);
        lines.push(triple_iri(&q, RDF_TYPE, &p("GoalDirectedQuery")));
        lines.push(triple_lit(&q, &p("goalDirectedName"), &eval.name));
        lines.push(triple_lit(
            &q,
            &p("goalDirectedDescription"),
            &eval.description,
        ));
        lines.push(triple_lit(&q, &p("goalDirectedGoal"), &eval.goal));
        lines.push(triple_lit(&q, &p("goalDirectedStatus"), &eval.status));
        for (idx, ans) in eval.answers.iter().enumerate() {
            let a = answer_iri(&eval.name, idx);
            lines.push(triple_iri(&q, &p("hasGoalDirectedAnswer"), &a));
            lines.push(triple_iri(&a, RDF_TYPE, &p("GoalDirectedAnswer")));
            lines.push(triple_lit(&a, &p("goalDirectedAtom"), &ans.atom));
            for (var, surface) in &ans.bindings {
                lines.push(triple_lit(
                    &a,
                    &p("goalDirectedBinding"),
                    &format!("{var} = {surface}"),
                ));
            }
            lines.push(triple_iri(
                &a,
                &p("goalDirectedDerivation"),
                &ans.derivation_iri,
            ));
            lines.push(triple_typed(
                &a,
                &p("goalDirectedProofChecked"),
                if ans.proof_checks { "true" } else { "false" },
                XSD_BOOLEAN,
            ));
        }
        // Three-valued SLG-WFS verdicts: each carries its ground atom surface and a
        // `true`/`false`/`undefined` value. The `undefined` verdict is what makes well-founded
        // negation a SHIPPED (non-dark) behaviour — it cannot be an `xsd:boolean`, so it rides
        // as a plain three-valued string literal.
        for (idx, v) in eval.verdicts.iter().enumerate() {
            let vi = verdict_iri(&eval.name, idx);
            lines.push(triple_iri(&q, &p("hasGoalDirectedVerdict"), &vi));
            lines.push(triple_iri(&vi, RDF_TYPE, &p("GoalDirectedVerdict")));
            lines.push(triple_lit(&vi, &p("goalDirectedVerdictAtom"), &v.atom));
            lines.push(triple_lit(&vi, &p("goalDirectedVerdict"), &v.verdict));
        }
        // U2: project the AUTHORED program structure itself — its own clauses, query, and
        // verdict-probe atoms — into the SAME graph/goal-directed, content-addressed and
        // linked from the existing query node, so the shipped bundle carries the authored
        // program alongside its evaluated well-founded model, not only the answers.
        let prog = program_iri_for(eval);
        lines.push(triple_iri(&q, &p("hasGoalDirectedProgram"), &prog));
        lines.push(triple_iri(&prog, RDF_TYPE, &p("GoalDirectedProgram")));
        lines.push(triple_lit(&prog, &p("goalDirectedProgramName"), &eval.name));
        for clause in &eval.clauses {
            lines.push(triple_lit(&prog, &p("goalDirectedClause"), clause));
        }
        lines.push(triple_lit(
            &prog,
            &p("goalDirectedProgramQuery"),
            &eval.goal,
        ));
        for probe in &eval.verdict_probe_atoms {
            lines.push(triple_lit(
                &prog,
                &p("goalDirectedProgramVerdictProbe"),
                probe,
            ));
        }
    }
    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// One `<s> <p> <o> .` IRI-object triple line.
fn triple_iri(s: &str, p: &str, o: &str) -> String {
    format!("<{s}> <{p}> <{o}> .")
}

/// One `<s> <p> "lit" .` plain-string-literal triple line (with N-Triples escaping).
fn triple_lit(s: &str, p: &str, lit: &str) -> String {
    format!("<{s}> <{p}> \"{}\" .", escape_literal(lit))
}

/// One `<s> <p> "lex"^^<dt> .` typed-literal triple line.
fn triple_typed(s: &str, p: &str, lex: &str, dt: &str) -> String {
    format!("<{s}> <{p}> \"{}\"^^<{dt}> .", escape_literal(lex))
}

/// Escape a string for an N-Triples literal (backslash, quote, and the C0 controls that
/// have canonical escapes).
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `math:` subsort tower IRIs the order-sorted demonstrator tests below reason
    /// over — literal references to the AUTHORED grounding vocabulary
    /// (`slices/grounding/math/module.ttl`), not a second source of the tower itself (the
    /// tower's edges are always supplied as `subsort_edges`, never hardcoded here).
    const TEST_MATH_INTEGER: &str = "https://blackcatinformatics.ca/math/Integer";
    const TEST_MATH_REAL: &str = "https://blackcatinformatics.ca/math/RealNumber";

    // ── Task 4: compiled `logic:ReasoningProgram` → `FolProgram`, via `lower_reasoning_program` ──
    //
    // These parse a `logic:ReasoningProgram` from a Turtle fixture (the SAME authoring
    // vocabulary `crates/logic-compile`'s own frontend tests exercise), compile it to
    // `ReasoningProgramIr` (Task 3), then run it through `evaluate_reasoning_programs` — the
    // SOLE production path for goal-directed programs.

    /// `add(zero,Y,Y). add(s(X),Y,s(Z)) :- add(X,Y,Z).` with goal
    /// `?- add(s(s(zero)),s(zero),R)`, authored as a `logic:ReasoningProgram`.
    const PEANO_ADD_REASONING_PROGRAM_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        \n\
        ex:peanoAdd a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:add ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termApplication ex:ssZero ] ,\n\
                               [ logic:termIndex 1 ; logic:termApplication ex:sZero ] ,\n\
                               [ logic:termIndex 2 ; logic:termVariable \"R\" ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:add ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:zero ] ,\n\
                               [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,\n\
                               [ logic:termIndex 2 ; logic:termVariable \"Y\" ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:antecedent [ a logic:Formula ;\n\
                    logic:relation ex:add ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,\n\
                                   [ logic:termIndex 2 ; logic:termVariable \"Z\" ]\n\
                ] ;\n\
                logic:consequent [ a logic:Formula ;\n\
                    logic:relation ex:add ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termApplication ex:sX ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Y\" ] ,\n\
                                   [ logic:termIndex 2 ; logic:termApplication ex:sZ ]\n\
                ]\n\
            ] .\n\
        ex:sZero a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:s ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:zero ] .\n\
        ex:ssZero a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:s ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termApplication ex:sZero ] .\n\
        ex:sX a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:s ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] .\n\
        ex:sZ a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:s ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"Z\" ] .\n\
    ";

    #[test]
    fn compiled_peano_add_reasoning_program_resolves_and_proof_checks() {
        let (prog, diags) =
            gmeow_logic_compile::frontend::parse_logic_str(PEANO_ADD_REASONING_PROGRAM_TTL, None)
                .expect("parse succeeds");
        assert!(
            diags
                .iter()
                .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
            "unexpected error diagnostics: {diags:#?}"
        );
        assert_eq!(
            prog.reasoning_programs.len(),
            1,
            "exactly one logic:ReasoningProgram parsed"
        );

        let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
            .expect("evaluate the compiled reasoning program");
        assert_eq!(evals.len(), 1);
        let peano = &evals[0];
        assert_eq!(peano.status, "ok");
        assert_eq!(peano.answers.len(), 1, "2 + 1 has exactly one answer");
        let ans = &peano.answers[0];
        // Every constant/function-symbol in the compiled path is a REAL RDF IRI (rendered in
        // full) — so the expected surfaces are built from the same `ex:` namespace the
        // fixture authors its symbols under.
        const EX: &str = "https://example.org/goal-directed-test/";
        let zero = format!("{EX}zero");
        let s = |inner: &str| format!("{EX}s({inner})");
        let s_zero = s(&zero); // s(zero)
        let ss_zero = s(&s_zero); // s(s(zero))
        let sss_zero = s(&ss_zero); // s(s(s(zero))) = R
        assert_eq!(
            ans.bindings.get("R").map(String::as_str),
            Some(sss_zero.as_str()),
            "2 + 1 = 3 in Peano successors"
        );
        assert_eq!(ans.atom, format!("{EX}add({ss_zero},{s_zero},{sss_zero})"));
        assert!(ans.proof_checks, "the compiled answer is proof-checked");
        assert!(
            ans.derivation_iri.starts_with("https://"),
            "the answer carries a content-addressed derivation IRI: {}",
            ans.derivation_iri
        );

        // Two independent evaluations of the SAME parsed program mint the SAME derivation
        // IRI — content-addressing (`content_addressed_rule_iri`), not mint-order.
        let evals2 =
            evaluate_reasoning_programs(&prog.reasoning_programs, &[]).expect("second evaluation");
        assert_eq!(
            evals2[0].answers[0].derivation_iri, ans.derivation_iri,
            "the compiled program's rule identity is content-addressed, not interning-order \
             dependent"
        );
    }

    /// `member(X,cons(X,T)). member(X,cons(H,T)) :- member(X,T).` with goal
    /// `?- member(M,cons(a,cons(b,cons(c,nil))))`. The base clause and the recursive
    /// clause's antecedent/consequent deliberately REUSE the variable names `X`/`T` — this
    /// is the exact scenario that proves per-clause [`VarScope`] freshness: if the compiler
    /// accidentally shared one metavariable per NAME across clauses (instead of per NAME
    /// WITHIN one clause), this program would resolve incorrectly.
    const MEMBER_CONS_REASONING_PROGRAM_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        \n\
        ex:memberCons a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:member ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"M\" ] ,\n\
                               [ logic:termIndex 1 ; logic:termApplication ex:list1 ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:member ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                               [ logic:termIndex 1 ; logic:termApplication ex:consXT ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:antecedent [ a logic:Formula ;\n\
                    logic:relation ex:member ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"T\" ]\n\
                ] ;\n\
                logic:consequent [ a logic:Formula ;\n\
                    logic:relation ex:member ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termApplication ex:consHT ]\n\
                ]\n\
            ] .\n\
        ex:consXT a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                           [ logic:termIndex 1 ; logic:termVariable \"T\" ] .\n\
        ex:consHT a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termVariable \"H\" ] ,\n\
                           [ logic:termIndex 1 ; logic:termVariable \"T\" ] .\n\
        ex:list1 a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,\n\
                           [ logic:termIndex 1 ; logic:termApplication ex:list2 ] .\n\
        ex:list2 a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:b ] ,\n\
                           [ logic:termIndex 1 ; logic:termApplication ex:list3 ] .\n\
        ex:list3 a logic:FunctionTerm ;\n\
            logic:functionSymbol ex:cons ;\n\
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:c ] ,\n\
                           [ logic:termIndex 1 ; logic:termIri ex:nil ] .\n\
    ";

    #[test]
    fn compiled_member_cons_reasoning_program_enumerates_with_reused_variable_names() {
        let (prog, diags) =
            gmeow_logic_compile::frontend::parse_logic_str(MEMBER_CONS_REASONING_PROGRAM_TTL, None)
                .expect("parse succeeds");
        assert!(
            diags
                .iter()
                .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
            "unexpected error diagnostics: {diags:#?}"
        );
        assert_eq!(prog.reasoning_programs.len(), 1);

        let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
            .expect("evaluate the compiled reasoning program");
        assert_eq!(evals.len(), 1);
        let member = &evals[0];
        assert_eq!(member.status, "ok");
        let mut bound: Vec<String> = member
            .answers
            .iter()
            .map(|a| a.bindings["M"].clone())
            .collect();
        bound.sort();
        // Every constant is a REAL RDF IRI (rendered in full) — see the peano-add test above.
        const EX: &str = "https://example.org/goal-directed-test/";
        assert_eq!(
            bound,
            vec![format!("{EX}a"), format!("{EX}b"), format!("{EX}c"),],
            "the SAME variable names (X, T) reused across the base and recursive clauses must \
             NOT collide across clause scopes: {bound:?}"
        );
        for ans in &member.answers {
            assert!(ans.proof_checks, "every member answer is proof-checked");
        }
    }

    // ── Task 4 M5/F-4: compiled math-subsort + incomparable control, term_sorts seeded ──
    //
    // `ex:one` is an ordinary domain individual, typed `math:Integer` by a plain
    // `rdf:type` triple (never `logic:` structural vocabulary, so the stage's L3 fold drops
    // it — `ReasoningProgramIr::constant_sorts`, Task 4's fix, is what recovers it). Program
    // A's query variable is declared `math:RealNumber`; program B's (the control) is
    // declared the INCOMPARABLE `math:Set`. Both share the SAME fact `p(one)` and the SAME
    // constant `ex:one`, so the ONLY difference between A's answer and B's empty answer set
    // is the order-sorted lattice discriminating `Integer ⊑ RealNumber` from `Integer ⋢
    // Set` — proving `SortContext::term_sorts` (not just `meta_sorts`) is actually seeded
    // from the compiled IR's `constant_sorts`, not left empty (which would make every
    // constant order-sort top and erase the F-4 differential).
    const MATH_SUBSORT_REASONING_PROGRAMS_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        @prefix math: <https://blackcatinformatics.ca/math/> .\n\
        \n\
        ex:one a math:Integer .\n\
        \n\
        ex:subsortPositive a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ;\n\
                                  logic:variableSort math:RealNumber ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:one ]\n\
            ] .\n\
        \n\
        ex:subsortControl a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ;\n\
                                  logic:variableSort math:Set ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:p ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:one ]\n\
            ] .\n\
    ";

    #[test]
    fn compiled_math_subsort_reasoning_program_seeds_term_sorts_from_constant_sorts() {
        let (prog, diags) = gmeow_logic_compile::frontend::parse_logic_str(
            MATH_SUBSORT_REASONING_PROGRAMS_TTL,
            None,
        )
        .expect("parse succeeds");
        assert!(
            diags
                .iter()
                .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
            "unexpected error diagnostics: {diags:#?}"
        );
        assert_eq!(prog.reasoning_programs.len(), 2);

        const EX: &str = "https://example.org/goal-directed-test/";
        let subsort_edges = [(TEST_MATH_INTEGER.to_owned(), TEST_MATH_REAL.to_owned())];

        // Under the ℤ⊑ℝ reasoned edge: program A (RealNumber-sorted X) resolves to exactly
        // the Integer constant `one`; program B (the Set-sorted control) resolves to NOTHING
        // — status ok, empty answer set, never an error.
        let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &subsort_edges)
            .expect("evaluate the compiled reasoning programs");
        assert_eq!(evals.len(), 2);
        let positive = evals
            .iter()
            .find(|e| e.name == "subsortPositive")
            .expect("the positive program is present");
        assert_eq!(positive.status, "ok");
        assert_eq!(
            positive.answers.len(),
            1,
            "an Integer constant binds a RealNumber variable (ℤ ⊑ ℝ) under the reasoned \
             edge: {:?}",
            positive.answers
        );
        let ans = &positive.answers[0];
        assert_eq!(
            ans.bindings.get("X").map(String::as_str),
            Some(format!("{EX}one").as_str()),
            "the subsort-unified answer binds X = ex:one"
        );
        assert_eq!(ans.atom, format!("{EX}p({EX}one)"));
        assert!(ans.proof_checks, "the subsort answer is proof-checked");

        let control = evals
            .iter()
            .find(|e| e.name == "subsortControl")
            .expect("the control program is present");
        assert_eq!(control.status, "ok");
        assert!(
            control.answers.is_empty(),
            "an Integer constant does NOT bind an incomparable-sort (Set) variable, status \
             ok, empty answer set: {:?}",
            control.answers
        );

        // M5/F-4: with EMPTY subsort_edges, program A ALSO returns ZERO answers — the
        // Integer/RealNumber unification comes from the REASONED edge, never a hardcoded
        // tower baked into the compiler.
        let evals_no_edges = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
            .expect("evaluate with no subsort edges");
        let positive_no_edges = evals_no_edges
            .iter()
            .find(|e| e.name == "subsortPositive")
            .expect("the positive program is present");
        assert_eq!(positive_no_edges.status, "ok");
        assert!(
            positive_no_edges.answers.is_empty(),
            "without the reasoned ℤ⊑ℝ edge, an Integer constant does NOT bind a RealNumber \
             variable — order-sortedness comes from subsort_edges, not a hardcoded tower: {:?}",
            positive_no_edges.answers
        );
    }

    // ── T3: cross-engine (backward vs. forward) fixpoint-agreement oracle ───────────────
    //
    // `ex:reachability`: `edge(a,b). edge(b,c). reach(X,Y):-edge(X,Y). reach(X,Z):-edge(X,Y),
    // reach(Y,Z).` with goal `?- reach(a,W)`. Definite, function-free, and every atom binary
    // — squarely inside `is_definite_function_free_binary`'s fragment, so
    // `evaluate_reasoning_programs` runs the T3 oracle over it.

    const REACHABILITY_REASONING_PROGRAM_TTL: &str = "\
        @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
        @prefix ex: <https://example.org/goal-directed-test/> .\n\
        \n\
        ex:reachability a logic:ReasoningProgram ;\n\
            logic:evaluationMode logic:BackwardEvaluation ;\n\
            logic:programQuery [ a logic:Formula ;\n\
                logic:relation ex:reach ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,\n\
                               [ logic:termIndex 1 ; logic:termVariable \"W\" ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:edge ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ,\n\
                               [ logic:termIndex 1 ; logic:termIri ex:b ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:relation ex:edge ;\n\
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:b ] ,\n\
                               [ logic:termIndex 1 ; logic:termIri ex:c ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:antecedent [ a logic:Formula ;\n\
                    logic:relation ex:edge ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Y\" ]\n\
                ] ;\n\
                logic:consequent [ a logic:Formula ;\n\
                    logic:relation ex:reach ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Y\" ]\n\
                ]\n\
            ] ;\n\
            logic:clause [ a logic:Formula ;\n\
                logic:antecedent [ a logic:Formula ;\n\
                    logic:and [ a logic:Formula ;\n\
                            logic:relation ex:edge ;\n\
                            logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                           [ logic:termIndex 1 ; logic:termVariable \"Y\" ]\n\
                        ] ,\n\
                        [ a logic:Formula ;\n\
                            logic:relation ex:reach ;\n\
                            logic:argument [ logic:termIndex 0 ; logic:termVariable \"Y\" ] ,\n\
                                           [ logic:termIndex 1 ; logic:termVariable \"Z\" ]\n\
                        ]\n\
                ] ;\n\
                logic:consequent [ a logic:Formula ;\n\
                    logic:relation ex:reach ;\n\
                    logic:argument [ logic:termIndex 0 ; logic:termVariable \"X\" ] ,\n\
                                   [ logic:termIndex 1 ; logic:termVariable \"Z\" ]\n\
                ]\n\
            ] .\n\
    ";

    #[test]
    fn reachability_program_is_gated_into_the_oracle_fragment_and_passes_it() {
        let (prog, diags) = gmeow_logic_compile::frontend::parse_logic_str(
            REACHABILITY_REASONING_PROGRAM_TTL,
            None,
        )
        .expect("parse succeeds");
        assert!(
            diags
                .iter()
                .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
            "unexpected error diagnostics: {diags:#?}"
        );
        assert_eq!(prog.reasoning_programs.len(), 1);
        assert!(
            is_definite_function_free_binary(&prog.reasoning_programs[0]),
            "reachability is definite, function-free, and every atom is binary — squarely \
             inside the T3 oracle's fragment"
        );

        // `evaluate_reasoning_programs` runs the oracle inline; a mismatch would HARD-FAIL
        // here, so success itself proves backward == forward for this program.
        let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
            .expect("evaluate + cross-check the reachability program");
        assert_eq!(evals.len(), 1);
        let reach = &evals[0];
        assert_eq!(reach.status, "ok");
        const EX: &str = "https://example.org/goal-directed-test/";
        let mut bound: Vec<String> = reach
            .answers
            .iter()
            .map(|a| a.bindings["W"].clone())
            .collect();
        bound.sort();
        assert_eq!(
            bound,
            vec![format!("{EX}b"), format!("{EX}c")],
            "backward resolution of reach(a,W) enumerates W ∈ {{b,c}}"
        );
    }

    #[test]
    fn programs_with_function_symbols_negation_or_non_binary_atoms_are_excluded_from_the_oracle() {
        // Peano add carries `s(...)` function-term applications: NOT function-free.
        let (peano, _) =
            gmeow_logic_compile::frontend::parse_logic_str(PEANO_ADD_REASONING_PROGRAM_TTL, None)
                .expect("parse succeeds");
        assert!(
            !is_definite_function_free_binary(&peano.reasoning_programs[0]),
            "peano-add's s(...) function terms exclude it from the oracle's fragment"
        );

        // Math-subsort's `p(X)` is unary: NOT binary.
        let (subsort, _) = gmeow_logic_compile::frontend::parse_logic_str(
            MATH_SUBSORT_REASONING_PROGRAMS_TTL,
            None,
        )
        .expect("parse succeeds");
        for program in &subsort.reasoning_programs {
            assert!(
                !is_definite_function_free_binary(program),
                "{}'s unary p(X) atom excludes it from the oracle's binary-atom fragment",
                program.iri
            );
        }
    }

    #[test]
    fn the_cross_engine_oracle_hard_fails_a_deliberately_wrong_answer_set() {
        // Proves the oracle is not vacuous: corrupt a REAL, oracle-passing evaluation's
        // answer set and confirm `cross_check_forward_agreement` actually detects and
        // rejects the mismatch, rather than trivially succeeding for any input.
        let (prog, _) = gmeow_logic_compile::frontend::parse_logic_str(
            REACHABILITY_REASONING_PROGRAM_TTL,
            None,
        )
        .expect("parse succeeds");
        let program = &prog.reasoning_programs[0];
        let evals = evaluate_reasoning_programs(std::slice::from_ref(program), &[])
            .expect("the real program passes the oracle");
        let real_eval = &evals[0];
        assert!(
            !real_eval.answers.is_empty(),
            "the real program has at least one answer to corrupt"
        );

        let mut wrong = real_eval.clone();
        wrong.answers.pop();
        wrong.answers.push(GoalDirectedAnswer {
            atom: "https://example.org/goal-directed-test/reach(https://example.org/\
                   goal-directed-test/a,https://example.org/goal-directed-test/nonexistent)"
                .to_owned(),
            bindings: BTreeMap::new(),
            derivation_iri: "https://blackcatinformatics.ca/gmeow/derivation/bogus".to_owned(),
            proof_checks: true,
        });

        let result = cross_check_forward_agreement(program, &wrong);
        assert!(
            result.is_err(),
            "the oracle must HARD-FAIL when the (corrupted) backward answer set disagrees \
             with the forward least model — proving the check is not vacuous"
        );
    }

    // ── Task 7: the authored/compiled path is now the SOLE source — every demonstrator
    // behavior below is asserted directly against `evaluate_reasoning_programs` over a
    // parsed `logic:ReasoningProgram` fixture, never a hand-interned Rust-constant corpus.

    #[test]
    fn compiled_peano_add_projection_carries_answer_atom_and_derivation_iri() {
        let (prog, _) =
            gmeow_logic_compile::frontend::parse_logic_str(PEANO_ADD_REASONING_PROGRAM_TTL, None)
                .expect("parse succeeds");
        let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
            .expect("evaluate the compiled reasoning program");
        let nt = project_goal_directed(&evals);
        assert!(
            nt.contains("GoalDirectedQuery"),
            "the projection types the query"
        );
        const EX: &str = "https://example.org/goal-directed-test/";
        let expected_atom = format!(
            "{EX}add({EX}s({EX}s({EX}zero)),{EX}s({EX}zero),{EX}s({EX}s({EX}s({EX}zero))))"
        );
        assert!(
            nt.contains(&expected_atom),
            "the projection carries the ground answer atom:\n{nt}"
        );
        assert!(
            nt.contains("goalDirectedDerivation"),
            "the projection carries the proof-derivation IRI predicate"
        );
        // Deterministic: a second projection of the SAME evals is byte-identical.
        let nt2 = project_goal_directed(&evals);
        assert_eq!(nt, nt2, "the projection is byte-stable");
    }

    // ── U2: the authored PROGRAM STRUCTURE itself is projected, not only its answers ────

    #[test]
    fn compiled_peano_add_projection_carries_the_authored_program_structure_and_is_byte_stable_across_runs()
     {
        let (prog, _) =
            gmeow_logic_compile::frontend::parse_logic_str(PEANO_ADD_REASONING_PROGRAM_TTL, None)
                .expect("parse succeeds");
        let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
            .expect("evaluate the compiled reasoning program");
        let nt = project_goal_directed(&evals);
        assert!(
            nt.contains("GoalDirectedProgram"),
            "the projection types the authored program node:\n{nt}"
        );
        assert!(
            nt.contains("hasGoalDirectedProgram"),
            "the query node links to its authored program:\n{nt}"
        );
        assert!(
            nt.contains("goalDirectedClause"),
            "the projection carries the authored clauses:\n{nt}"
        );
        assert!(
            nt.contains("goalDirectedProgramQuery"),
            "the projection carries the authored program's query:\n{nt}"
        );
        // The Peano program's own fact clause and the recursive rule's body both surface as
        // rendered `goalDirectedClause` literals.
        const EX: &str = "https://example.org/goal-directed-test/";
        assert!(
            nt.contains(&format!("{EX}add({EX}zero,")),
            "the Peano fact clause add(zero,Y,Y). is projected:\n{nt}"
        );
        assert!(
            nt.contains(&format!(" :- {EX}add(")),
            "the Peano recursive rule's antecedent is projected:\n{nt}"
        );
        // The query linkage: the peano-add query node's `hasGoalDirectedProgram` object is a
        // `GoalDirectedProgram` individual carrying that SAME program's `goalDirectedProgramQuery`
        // literal, equal to the query node's own `goalDirectedGoal` literal (the SAME `render`
        // surface, reused rather than re-derived).
        let peano = &evals[0];
        let expected_query_triple =
            format!("<{GMEOW}goalDirectedProgramQuery> \"{}\" .", peano.goal);
        assert!(
            nt.lines().any(|l| l.ends_with(&expected_query_triple)),
            "the program node's goalDirectedProgramQuery literal equals the query node's own \
             rendered goal:\n{nt}"
        );

        // Byte-stability ACROSS two independent evaluations (not merely two projections of
        // the same `evals`): content-addressed, never interning/mint-order dependent.
        let evals2 =
            evaluate_reasoning_programs(&prog.reasoning_programs, &[]).expect("second evaluation");
        let nt2 = project_goal_directed(&evals2);
        assert_eq!(
            nt, nt2,
            "the authored-program projection is byte-identical across independent evaluations"
        );
    }

    // ── Positive structured demonstrator: member over cons/nil ──────────────────────────

    #[test]
    fn compiled_member_cons_projection_carries_structured_answers_and_derivation() {
        let (prog, _) =
            gmeow_logic_compile::frontend::parse_logic_str(MEMBER_CONS_REASONING_PROGRAM_TTL, None)
                .expect("parse succeeds");
        let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &[])
            .expect("evaluate the compiled reasoning program");
        let member = &evals[0];
        assert_eq!(member.status, "ok");
        const EX: &str = "https://example.org/goal-directed-test/";
        // Each answer is proof-checked and carries a content-addressed derivation IRI over the
        // cons spine (a genuine structured atom, not a flat binary one).
        for ans in &member.answers {
            assert!(ans.proof_checks, "every member answer is proof-checked");
            assert!(
                ans.derivation_iri
                    .starts_with("https://blackcatinformatics.ca/gmeow/derivation/"),
                "the answer carries a content-addressed derivation IRI: {}",
                ans.derivation_iri
            );
            assert!(
                ans.atom.starts_with(&format!("{EX}member("))
                    && ans.atom.contains(&format!("{EX}cons(")),
                "the answer atom is a structured cons-list membership: {}",
                ans.atom
            );
        }

        let nt = project_goal_directed(&evals);
        let expected_atom =
            format!("{EX}member({EX}a,{EX}cons({EX}a,{EX}cons({EX}b,{EX}cons({EX}c,{EX}nil))))");
        assert!(
            nt.contains(&expected_atom),
            "the projection carries a structured member answer atom:\n{nt}"
        );
        // Every member answer surfaces a derivation IRI triple.
        assert!(
            nt.contains(
                "<https://blackcatinformatics.ca/gmeow/goalDirectedDerivation> \
                 <https://blackcatinformatics.ca/gmeow/derivation/"
            ),
            "the projection carries the member answers' derivation IRIs:\n{nt}"
        );
    }

    // ── WFS negation demonstrator: three-valued verdicts including undefined ─────────────
    //
    // No authored-path test above exercises `logic:verdictProbe`s, so this test is what
    // proves the compiled path carries the three-valued SLG-WFS verdict surface end to end.
    // Unlike the fixtures above, this parses the REAL committed corpus
    // (`slices/grounding/logic/examples/reasoning-programs.ttl`) via [`authored_reasoning_programs`]
    // rather than an inline TTL literal: a standalone inline copy of `ex:winWfs`'s
    // `logic:and`-conjoined positive+negative body was found (empirically, while authoring
    // this test) to compile to a DIFFERENT literal order than the SAME text does inside the
    // full corpus file — `logic:and`/`logic:or` carry no `logic:conjunctIndex` analogous to
    // `logic:argument`'s `logic:termIndex`, so `crate::physical::lower`'s conjunct order is
    // only as stable as the frontend's per-document blank-node interning, not the authored
    // text order. That is a pre-existing frontend/vocabulary gap (never introduced or fixed
    // by Task 7's GREENFIELD removal), so this test sidesteps it by exercising the ACTUAL
    // shipped fixture rather than reproducing an order-sensitive fragment out of context.

    /// Parse the REAL authored demonstrator corpus
    /// (`slices/grounding/logic/examples/reasoning-programs.ttl`) through the exact same
    /// production frontend entry point `gmeow-pipeline`'s `stage-compile-logic` uses.
    fn authored_reasoning_programs() -> Vec<ReasoningProgramIr> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("slices/grounding/logic/examples/reasoning-programs.ttl");
        let (prog, diags) = gmeow_logic_compile::frontend::parse_logic_path(&path, None)
            .expect("parse the authored reasoning-programs cell");
        assert!(
            diags
                .iter()
                .all(|d| d.severity != gmeow_logic_compile::frontend::Severity::Error),
            "unexpected error diagnostics: {diags:#?}"
        );
        assert!(
            !prog.reasoning_programs.is_empty(),
            "the authored cell carries at least one logic:ReasoningProgram"
        );
        prog.reasoning_programs
    }

    #[test]
    fn compiled_win_wfs_reasoning_program_carries_three_valued_verdicts() {
        let programs = authored_reasoning_programs();
        let win_program = programs
            .iter()
            .find(|p| p.iri.ends_with("winWfs"))
            .cloned()
            .expect("ex:winWfs is authored in the reasoning-programs cell");

        let evals = evaluate_reasoning_programs(std::slice::from_ref(&win_program), &[])
            .expect("evaluate the compiled win-wfs reasoning program");
        assert_eq!(evals.len(), 1);
        let win = &evals[0];
        assert_eq!(win.status, "ok");

        const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";
        // The only well-founded-TRUE goal answer is win(c).
        let ws: Vec<String> = win
            .answers
            .iter()
            .map(|a| a.bindings["W"].clone())
            .collect();
        assert_eq!(
            ws,
            vec![format!("{EX}c")],
            "only c is a founded win: {ws:?}"
        );
        for ans in &win.answers {
            assert!(ans.proof_checks, "the win answer is proof-checked");
        }

        let atom_of = |local: &str| format!("{EX}win({EX}{local})");
        let verdict_of = |atom: &str| {
            win.verdicts
                .iter()
                .find(|v| v.atom == atom)
                .unwrap_or_else(|| panic!("verdict for {atom} present: {:?}", win.verdicts))
                .verdict
                .as_str()
        };
        // The a⇄b negative loop is well-founded UNDEFINED (never a fabricated true/false).
        assert_eq!(
            verdict_of(&atom_of("a")),
            "undefined",
            "even cycle ⇒ undefined"
        );
        assert_eq!(
            verdict_of(&atom_of("b")),
            "undefined",
            "even cycle ⇒ undefined"
        );
        // The founded positions are a definite true/false.
        assert_eq!(verdict_of(&atom_of("c")), "true", "move to lost d ⇒ won");
        assert_eq!(verdict_of(&atom_of("d")), "false", "no move ⇒ lost");

        // The distinctive SLG-WFS surface projects: an undefined verdict AND both founded
        // verdicts.
        let nt = project_goal_directed(&evals);
        let has_verdict = |atom: &str, verdict: &str| {
            nt.lines()
                .any(|l| l.contains("goalDirectedVerdictAtom") && l.contains(atom))
                && nt.lines().any(|l| {
                    l.contains("goalDirectedVerdict>") && l.contains(&format!("\"{verdict}\""))
                })
        };
        assert!(
            nt.contains("\"undefined\""),
            "the projection carries at least one undefined WFS verdict (SLG-WFS is non-dark):\n{nt}"
        );
        assert!(
            has_verdict(&atom_of("a"), "undefined"),
            "win(a) is serialized as undefined:\n{nt}"
        );
        assert!(
            has_verdict(&atom_of("c"), "true"),
            "win(c) is serialized as a founded true:\n{nt}"
        );
        assert!(
            has_verdict(&atom_of("d"), "false"),
            "win(d) is serialized as a founded false:\n{nt}"
        );

        // Byte-stability across two independent evaluations.
        let evals2 = evaluate_reasoning_programs(std::slice::from_ref(&win_program), &[])
            .expect("second evaluation");
        let nt2 = project_goal_directed(&evals2);
        assert_eq!(
            nt, nt2,
            "the win-wfs projection is byte-identical across independent evaluations"
        );
    }

    // ── Math sub-sort demonstrator (order-sorted ℤ ⊑ ℝ) + incomparable control ───────────

    #[test]
    fn compiled_math_subsort_projection_carries_the_subsort_unified_answer() {
        let (prog, _) = gmeow_logic_compile::frontend::parse_logic_str(
            MATH_SUBSORT_REASONING_PROGRAMS_TTL,
            None,
        )
        .expect("parse succeeds");
        let subsort_edges = [(TEST_MATH_INTEGER.to_owned(), TEST_MATH_REAL.to_owned())];
        let evals = evaluate_reasoning_programs(&prog.reasoning_programs, &subsort_edges)
            .expect("evaluate the compiled reasoning programs");
        let nt = project_goal_directed(&evals);
        const EX: &str = "https://example.org/goal-directed-test/";
        assert!(
            nt.contains(&format!(
                "<https://blackcatinformatics.ca/gmeow/goalDirectedAtom> \"{EX}p({EX}one)\""
            )),
            "the projection carries the subsort-unified answer atom p(one):\n{nt}"
        );
        assert!(
            nt.contains(&format!("\"X = {EX}one\"")),
            "the projection carries the subsort-unified binding X = one:\n{nt}"
        );
    }

    // ── Every compiled reasoning-program answer proof-checks; whole projection is
    // non-vacuous and byte-stable — the authored-path equivalent of the retired
    // `evaluate_shipped_demonstrators()` corpus sweep, over the SAME six programs
    // `slices/grounding/logic/examples/reasoning-programs.ttl` ships. ──────────────────────

    #[test]
    fn every_compiled_reasoning_program_answer_proof_checks_and_projection_is_deterministic() {
        // The REAL committed corpus — all six authored programs (peano-add, member-cons, the
        // math-subsort positive/control pair, reachability, win-wfs) in ONE parse — the
        // authored-path equivalent of the retired `evaluate_shipped_demonstrators()` corpus
        // sweep.
        let programs = authored_reasoning_programs();
        let subsort_edges = [(TEST_MATH_INTEGER.to_owned(), TEST_MATH_REAL.to_owned())];
        let evals = evaluate_reasoning_programs(&programs, &subsort_edges)
            .expect("evaluate the merged compiled corpus");
        let mut total_answers = 0usize;
        for eval in &evals {
            for ans in &eval.answers {
                assert!(
                    ans.proof_checks,
                    "demonstrator {} answer {} must be proof-checked",
                    eval.name, ans.atom
                );
                total_answers += 1;
            }
        }
        assert!(
            total_answers >= 5,
            "the corpus ships several proof-checked answers (peano + 3 members + subsort + \
             reachability + win-wfs): got {total_answers}"
        );

        // Two evaluations produce byte-identical serialization (no hash-iteration order).
        let nt_first = project_goal_directed(&evals);
        assert!(!nt_first.is_empty(), "the projection is non-empty");
        let evals2 =
            evaluate_reasoning_programs(&programs, &subsort_edges).expect("second evaluation");
        let nt_second = project_goal_directed(&evals2);
        assert_eq!(
            nt_first, nt_second,
            "two independent evaluations serialize byte-identically (deterministic)"
        );
    }
}
