// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The goal-directed (backward) demonstrator façade — the production surface that makes
//! the proof-carrying full-FOL backward engine non-dark.
//!
//! The proof-carrying backward engine (`crate::physical::resolve_fol`) and its
//! Curry–Howard proof checker (`crate::physical::proof::check`) are `pub(crate)` behind
//! the private `physical` module, so no other crate can reach them. This module is the
//! single thin, honest `pub` façade over them: it holds a corpus of shipped
//! *goal-directed demonstrator programs* (structured — function-symbol — logic programs
//! the flat query text-parser cannot express, so they are built directly against the
//! resolver's `TermDag`), evaluates each through [`resolve_fol`](crate::physical::resolve_fol::resolve_fol), validates every answer's
//! proof with [`check`](crate::physical::proof::check), and projects the checked answers + their content-addressed
//! derivation IRIs into RDF-serializable data the `gmeow-pipeline`
//! `stage-goal-directed` folds into `graph/goal-directed` of `gmeow.gts`.
//!
//! It is NOT a fork of the engine: it constructs programs and reads back the engine's own
//! [`FolOutcome`](crate::physical::resolve_fol::FolOutcome), never re-implementing resolution. Task 8 appends the substantial
//! demonstrators (append/member, WFS negation, math sub-sort) to
//! `shipped_demonstrators`; this module ships the minimal Peano-addition demonstrator so
//! the stage has a real, proof-checked answer to fold.

use std::collections::{BTreeMap, HashMap};

use purrdf::TermValue;

use crate::physical::id::NodeId;
use crate::physical::proof::{check, structured_derivation_iri};
use crate::physical::resolve_fol::{
    FolClause, FolControl, FolLit, FolProgram, Truth, render, resolve_fol,
};
use crate::physical::term_dag::TermDag;
use crate::physical::unify::{SortContext, SortOrder};
use crate::query_ir::Budget;

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
    /// ([`derivation_iri`](crate::physical::proof::derivation_iri) — byte-identical to the forward reasoner's rule-application id).
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
}

/// Evaluate every shipped goal-directed demonstrator: run each through the proof-carrying
/// backward resolver, [`check`] every answer's proof (a proof that does not re-derive its
/// answer atom HARD-fails — no unchecked answer ever ships), record each demonstrator's
/// probed three-valued WFS verdicts, and return the deterministic, RDF-serializable
/// evaluations. This is the pipeline stage's single entry point.
pub fn evaluate_shipped_demonstrators() -> gmeow_errors::Result<Vec<GoalDirectedEvaluation>> {
    let mut evals = Vec::new();
    for builder in shipped_demonstrators() {
        evals.push(evaluate_demonstrator(builder)?);
    }
    evals.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(evals)
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

/// A shipped demonstrator: its stable name, description, and a builder that interns the
/// structured program (and any sort context / verdict probes) into a fresh [`TermDag`].
struct Demonstrator {
    name: &'static str,
    description: &'static str,
    build: fn() -> BuiltDemonstrator,
}

/// The shipped demonstrator corpus — a SET, so appending a demonstrator is a one-line
/// addition, never a stage rewrite. Each entry makes one native backward-engine capability
/// observable in `graph/goal-directed` of `gmeow.gts`:
///
/// - `peano-add` / `member-cons` — structured (function-symbol) resolution with checkable
///   proofs (Peano successors; `member` enumeration over `cons`/`nil` lists);
/// - `win-wfs-negation` — three-valued SLG-WFS negation over a cyclic move graph (the
///   `undefined` loop verdict, alongside founded `true`/`false`);
/// - `math-subsort` / `math-subsort-control` — order-sorted unification against the authored
///   `math:` subsort tower (ℤ ⊑ ℝ accepts; an incomparable sort rejects).
fn shipped_demonstrators() -> Vec<Demonstrator> {
    vec![
        Demonstrator {
            name: "peano-add",
            description: "Peano addition by structural recursion — the minimal structured \
                          goal-directed demonstrator: one fact clause add(zero,Y,Y), one rule \
                          clause add(s(X),Y,s(Z)) :- add(X,Y,Z), and the query \
                          ?- add(s(s(zero)),s(zero),R), backward-resolved to R = s(s(s(zero))) \
                          with a Curry–Howard-checkable proof.",
            build: build_peano_add,
        },
        Demonstrator {
            name: "member-cons",
            description: "List membership over cons/nil constructors — a structured \
                          goal-directed demonstrator with a base clause member(X,cons(X,T)) \
                          and a recursive clause member(X,cons(H,T)) :- member(X,T). The query \
                          ?- member(M,cons(a,cons(b,cons(c,nil)))) enumerates M ∈ {a,b,c}, each \
                          answer carrying a Curry–Howard-checkable proof of its list position.",
            build: build_member_cons,
        },
        Demonstrator {
            name: "win-wfs-negation",
            description: "Three-valued well-founded negation (SLG-WFS) over the canonical game \
                          win(X) :- move(X,Y), not win(Y) on the move graph \
                          {move(a,b), move(b,a), move(c,d)}. The a⇄b cycle traps win(a)/win(b) \
                          in a negative loop ⇒ undefined; win(c) is a founded win (move to the \
                          lost d) ⇒ true; win(d) has no move ⇒ false. The undefined verdict is \
                          the well-founded model, never a fabricated true/false.",
            build: build_win_wfs,
        },
        Demonstrator {
            name: "math-subsort",
            description: "Order-sorted unification against the authored math: subsort tower \
                          ℕ⊑ℤ⊑ℚ⊑ℝ⊑ℂ (slices/grounding/math/module.ttl). The fact p(one) with \
                          one:Integer answers the query ?- p(X) with X:RealNumber ONLY because \
                          order-sorted unification consults ℤ⊑ℝ; the proof-checked answer is \
                          X = one.",
            build: build_math_subsort,
        },
        Demonstrator {
            name: "math-subsort-control",
            description: "The negative control for order-sorted unification: the same fact \
                          p(one) with one:Integer against the query ?- p(X) where X:Set is a \
                          sort INCOMPARABLE to Integer in the math: tower. Integer ⋢ Set, so \
                          order-sorted unification correctly refuses the binding and the query \
                          has NO answer — the observable evidence that the sort lattice gates \
                          resolution rather than being ignored.",
            build: build_math_subsort_control,
        },
    ]
}

/// Evaluate one demonstrator: resolve its goal, validate + project each answer, and record
/// each verdict probe's three-valued WFS verdict.
fn evaluate_demonstrator(demo: Demonstrator) -> gmeow_errors::Result<GoalDirectedEvaluation> {
    let BuiltDemonstrator {
        mut dag,
        program,
        ctx,
        verdict_probes,
    } = (demo.build)();
    // Render the goal template BEFORE resolution (free metavariables still present).
    let goal = render(&dag, program.goal);
    let outcome = match resolve_fol(&mut dag, &program, &ctx, &Budget::default())? {
        FolControl::Decided(outcome) => outcome,
        FolControl::Unsupported(kind) => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed demonstrator {:?} is unsupported by the backward engine: {kind:?}",
                    demo.name
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
                    "goal-directed demonstrator {:?} answer proof failed to check: {e:?}",
                    demo.name
                ),
            })
        })?;
        if checked != ans.atom {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed demonstrator {:?} proof re-derives a different atom than its answer",
                    demo.name
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
        name: demo.name.to_owned(),
        description: demo.description.to_owned(),
        goal,
        status,
        answers,
        verdicts,
    })
}

/// Intern an atomic IRI leaf under a program-local surface name.
fn leaf(dag: &mut TermDag, s: &str) -> NodeId {
    dag.intern_leaf(TermValue::iri(s.to_owned()))
}

/// Intern a function application `pred(args…)` under a program-local operator surface.
fn app(dag: &mut TermDag, pred: &str, args: Vec<NodeId>) -> NodeId {
    let op = dag.intern_leaf(TermValue::iri(pred.to_owned()));
    dag.intern_app(op, args)
}

/// Intern a demonstrator clause's content-addressed rule-IRI handle.
fn rule_handle(dag: &mut TermDag, name: &str, idx: usize) -> crate::physical::id::TermId {
    dag.intern_atom(&TermValue::iri(rule_iri(name, idx)))
}

/// The Peano-addition demonstrator: `add(zero,Y,Y). add(s(X),Y,s(Z)) :- add(X,Y,Z).`
/// with the goal `?- add(s(s(zero)),s(zero),R).` interned into a fresh [`TermDag`]. The
/// function symbols (`add`/`s`/`zero`) are program-local surfaces, not dereferenceable
/// terms; the rule IRIs are gmeow-namespaced so the derivation identity is stable.
fn build_peano_add() -> BuiltDemonstrator {
    let mut dag = TermDag::new();
    let zero = leaf(&mut dag, "zero");

    // Fact clause: add(zero, Y, Y).
    let (_, y) = dag.fresh_meta();
    let fact_head = app(&mut dag, "add", vec![zero, y, y]);
    let fact_rule = rule_handle(&mut dag, "peano-add", 0);

    // Rule clause: add(s(X), Y, s(Z)) :- add(X, Y, Z).
    let (_, x) = dag.fresh_meta();
    let (_, y2) = dag.fresh_meta();
    let (_, z) = dag.fresh_meta();
    let sx = app(&mut dag, "s", vec![x]);
    let sz = app(&mut dag, "s", vec![z]);
    let rule_head = app(&mut dag, "add", vec![sx, y2, sz]);
    let rule_body = app(&mut dag, "add", vec![x, y2, z]);
    let step_rule = rule_handle(&mut dag, "peano-add", 1);

    // Goal: add(s(s(zero)), s(zero), R).
    let s_zero = app(&mut dag, "s", vec![zero]);
    let ss_zero = app(&mut dag, "s", vec![s_zero]);
    let s_zero_g = app(&mut dag, "s", vec![zero]);
    let (_, r) = dag.fresh_meta();
    let goal = app(&mut dag, "add", vec![ss_zero, s_zero_g, r]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: fact_head,
                body: vec![],
                rule_iri: fact_rule,
            },
            FolClause {
                head: rule_head,
                body: vec![FolLit::Pos(rule_body)],
                rule_iri: step_rule,
            },
        ],
        goal,
        goal_vars: vec![(r, "R".to_owned())],
        meta_sorts: HashMap::new(),
    };
    BuiltDemonstrator {
        dag,
        program,
        ctx: SortContext::default(),
        verdict_probes: Vec::new(),
    }
}

/// The list-membership demonstrator over `cons`/`nil`:
/// `member(X,cons(X,T)). member(X,cons(H,T)) :- member(X,T).` with the goal
/// `?- member(M,cons(a,cons(b,cons(c,nil))))`, enumerating M ∈ {a,b,c}. A clearly-structured
/// positive program: each answer's proof re-derives the atom by descending the cons spine.
fn build_member_cons() -> BuiltDemonstrator {
    let mut dag = TermDag::new();
    let cons = |dag: &mut TermDag, h: NodeId, t: NodeId| app(dag, "cons", vec![h, t]);
    let nil = leaf(&mut dag, "nil");
    let a = leaf(&mut dag, "a");
    let b = leaf(&mut dag, "b");
    let c = leaf(&mut dag, "c");

    // Base clause: member(X, cons(X, T)).
    let (_, x) = dag.fresh_meta();
    let (_, t) = dag.fresh_meta();
    let cons_x_t = cons(&mut dag, x, t);
    let base_head = app(&mut dag, "member", vec![x, cons_x_t]);
    let base_rule = rule_handle(&mut dag, "member-cons", 0);

    // Recursive clause: member(X, cons(H, T)) :- member(X, T).
    let (_, x2) = dag.fresh_meta();
    let (_, h2) = dag.fresh_meta();
    let (_, t2) = dag.fresh_meta();
    let cons_h_t = cons(&mut dag, h2, t2);
    let step_head = app(&mut dag, "member", vec![x2, cons_h_t]);
    let step_body = app(&mut dag, "member", vec![x2, t2]);
    let step_rule = rule_handle(&mut dag, "member-cons", 1);

    // Goal: member(M, cons(a, cons(b, cons(c, nil)))).
    let cn = cons(&mut dag, c, nil);
    let bcn = cons(&mut dag, b, cn);
    let list = cons(&mut dag, a, bcn);
    let (_, m) = dag.fresh_meta();
    let goal = app(&mut dag, "member", vec![m, list]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: base_head,
                body: vec![],
                rule_iri: base_rule,
            },
            FolClause {
                head: step_head,
                body: vec![FolLit::Pos(step_body)],
                rule_iri: step_rule,
            },
        ],
        goal,
        goal_vars: vec![(m, "M".to_owned())],
        meta_sorts: HashMap::new(),
    };
    BuiltDemonstrator {
        dag,
        program,
        ctx: SortContext::default(),
        verdict_probes: Vec::new(),
    }
}

/// The three-valued SLG-WFS negation demonstrator: `win(X) :- move(X,Y), not win(Y)` over the
/// move graph `{move(a,b), move(b,a), move(c,d)}`. The a⇄b cycle is an even negative loop, so
/// the well-founded model leaves win(a)/win(b) `undefined`; win(c) is a founded win (its only
/// move is to the lost d) so it is `true`; win(d) has no outgoing move so it is `false`. The
/// verdict probes make all four verdicts observable in `graph/goal-directed`.
fn build_win_wfs() -> BuiltDemonstrator {
    let mut dag = TermDag::new();
    let a = leaf(&mut dag, "a");
    let b = leaf(&mut dag, "b");
    let c = leaf(&mut dag, "c");
    let d = leaf(&mut dag, "d");

    // Facts: move(a,b). move(b,a). move(c,d).
    let move_ab = app(&mut dag, "move", vec![a, b]);
    let move_ba = app(&mut dag, "move", vec![b, a]);
    let move_cd = app(&mut dag, "move", vec![c, d]);

    // Rule: win(X) :- move(X, Y), not win(Y).
    let (_, x) = dag.fresh_meta();
    let (_, yv) = dag.fresh_meta();
    let win_x = app(&mut dag, "win", vec![x]);
    let move_xy = app(&mut dag, "move", vec![x, yv]);
    let win_y = app(&mut dag, "win", vec![yv]);

    // Goal: ?- win(W).
    let (_, w) = dag.fresh_meta();
    let goal = app(&mut dag, "win", vec![w]);

    let clauses = vec![
        FolClause {
            head: move_ab,
            body: vec![],
            rule_iri: rule_handle(&mut dag, "win-wfs-negation", 0),
        },
        FolClause {
            head: move_ba,
            body: vec![],
            rule_iri: rule_handle(&mut dag, "win-wfs-negation", 1),
        },
        FolClause {
            head: move_cd,
            body: vec![],
            rule_iri: rule_handle(&mut dag, "win-wfs-negation", 2),
        },
        FolClause {
            head: win_x,
            body: vec![FolLit::Pos(move_xy), FolLit::Neg(win_y)],
            rule_iri: rule_handle(&mut dag, "win-wfs-negation", 3),
        },
    ];

    // Probe every position's win-verdict so the undefined loop and the founded true/false
    // atoms are all serialized.
    let win_a = app(&mut dag, "win", vec![a]);
    let win_b = app(&mut dag, "win", vec![b]);
    let win_c = app(&mut dag, "win", vec![c]);
    let win_d = app(&mut dag, "win", vec![d]);

    let program = FolProgram {
        clauses,
        goal,
        goal_vars: vec![(w, "W".to_owned())],
        meta_sorts: HashMap::new(),
    };
    BuiltDemonstrator {
        dag,
        program,
        ctx: SortContext::default(),
        verdict_probes: vec![win_a, win_b, win_c, win_d],
    }
}

/// The math IRIs of the authored subsort tower `ℕ ⊑ ℤ ⊑ ℚ ⊑ ℝ ⊑ ℂ`, sourced from
/// `slices/grounding/math/module.ttl` (`math:NaturalNumber rdfs:subClassOf math:Integer`, …).
/// These are the load-bearing sort surfaces — the constant/predicate/rule surfaces of the
/// subsort demonstrators are program-local, but the sorts cite the canonical `math:` tower so
/// the order-sorted lattice is not a second source of truth.
mod math_sort {
    pub(super) const NATURAL: &str = "https://blackcatinformatics.ca/math/NaturalNumber";
    pub(super) const INTEGER: &str = "https://blackcatinformatics.ca/math/Integer";
    pub(super) const RATIONAL: &str = "https://blackcatinformatics.ca/math/RationalNumber";
    pub(super) const REAL: &str = "https://blackcatinformatics.ca/math/RealNumber";
    pub(super) const COMPLEX: &str = "https://blackcatinformatics.ca/math/ComplexNumber";
    /// A sort deliberately INCOMPARABLE to the numeric tower (`math:Set`), for the control.
    pub(super) const SET: &str = "https://blackcatinformatics.ca/math/Set";
}

/// Build the shared parts of an order-sorted subsort demonstrator: the fact `p(one)` with
/// `one : Integer`, the goal `?- p(X)` with `X : x_sort`, and the [`SortContext`] carrying the
/// authored `ℕ⊑ℤ⊑ℚ⊑ℝ⊑ℂ` covering edges. `x_sort` is `math:RealNumber` for the positive case
/// (ℤ⊑ℝ, so the Integer constant binds) and `math:Set` for the incomparable control (Integer
/// ⋢ Set, so the binding is refused). Returns everything as a [`BuiltDemonstrator`].
fn build_math_subsort_with(name: &str, x_sort_iri: &str) -> BuiltDemonstrator {
    let mut dag = TermDag::new();

    // The authored subsort tower ℕ⊑ℤ⊑ℚ⊑ℝ⊑ℂ (covering edges from math/module.ttl). The
    // reflexive-transitive closure gives ℤ⊑ℝ, which is what makes the positive query resolve.
    let natural = leaf(&mut dag, math_sort::NATURAL);
    let integer = leaf(&mut dag, math_sort::INTEGER);
    let rational = leaf(&mut dag, math_sort::RATIONAL);
    let real = leaf(&mut dag, math_sort::REAL);
    let complex = leaf(&mut dag, math_sort::COMPLEX);
    let edges = [
        (natural, integer),
        (integer, rational),
        (rational, real),
        (real, complex),
    ];
    let order = SortOrder::from_subclass_edges(&edges);

    // Constant `one` tagged as an Integer (a program-local individual, Integer-sorted).
    let one = leaf(&mut dag, "one");
    let mut term_sorts: HashMap<NodeId, NodeId> = HashMap::new();
    term_sorts.insert(one, integer);
    let ctx = SortContext::new(order, term_sorts, HashMap::new());

    // Fact: p(one).
    let fact = app(&mut dag, "p", vec![one]);
    let fact_rule = rule_handle(&mut dag, name, 0);

    // Goal: ?- p(X), with X declared at the requested sort.
    let (xm, x) = dag.fresh_meta();
    let goal = app(&mut dag, "p", vec![x]);
    let x_sort = leaf(&mut dag, x_sort_iri);
    let mut meta_sorts: HashMap<crate::physical::id::MetaId, NodeId> = HashMap::new();
    meta_sorts.insert(xm, x_sort);

    let program = FolProgram {
        clauses: vec![FolClause {
            head: fact,
            body: vec![],
            rule_iri: fact_rule,
        }],
        goal,
        goal_vars: vec![(x, "X".to_owned())],
        meta_sorts,
    };
    BuiltDemonstrator {
        dag,
        program,
        ctx,
        verdict_probes: Vec::new(),
    }
}

/// The positive order-sorted demonstrator: `X : RealNumber` accepts the `Integer`-sorted
/// constant `one` because order-sorted unification consults `ℤ ⊑ ℝ`.
fn build_math_subsort() -> BuiltDemonstrator {
    build_math_subsort_with("math-subsort", math_sort::REAL)
}

/// The negative control: `X : Set` is incomparable to `Integer`, so the binding is refused
/// and the query has no answer — the observable evidence the lattice gates resolution.
fn build_math_subsort_control() -> BuiltDemonstrator {
    build_math_subsort_with("math-subsort-control", math_sort::SET)
}

/// The gmeow-namespaced content-addressing anchor for a demonstrator clause's rule IRI.
fn rule_iri(name: &str, idx: usize) -> String {
    format!("{GMEOW}goal-directed/{name}/rule/{idx}")
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

    #[test]
    fn peano_add_demonstrator_resolves_and_proof_checks() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let peano = evals
            .iter()
            .find(|e| e.name == "peano-add")
            .expect("the peano-add demonstrator is shipped");
        assert_eq!(peano.status, "ok");
        assert_eq!(peano.answers.len(), 1, "2 + 1 has exactly one answer");
        let ans = &peano.answers[0];
        assert_eq!(
            ans.bindings.get("R").map(String::as_str),
            Some("s(s(s(zero)))"),
            "2 + 1 = 3 in Peano successors"
        );
        assert_eq!(ans.atom, "add(s(s(zero)),s(zero),s(s(s(zero))))");
        assert!(ans.proof_checks, "the shipped answer is proof-checked");
        assert!(
            ans.derivation_iri.starts_with("https://"),
            "the answer carries a content-addressed derivation IRI: {}",
            ans.derivation_iri
        );
    }

    #[test]
    fn projection_carries_answer_atom_and_derivation_iri() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let nt = project_goal_directed(&evals);
        assert!(
            nt.contains("GoalDirectedQuery"),
            "the projection types the query"
        );
        assert!(
            nt.contains("add(s(s(zero)),s(zero),s(s(s(zero))))"),
            "the projection carries the ground answer atom:\n{nt}"
        );
        assert!(
            nt.contains("goalDirectedDerivation"),
            "the projection carries the proof-derivation IRI predicate"
        );
        // Deterministic: a second projection is byte-identical.
        let nt2 = project_goal_directed(&evals);
        assert_eq!(nt, nt2, "the projection is byte-stable");
    }

    // ── Positive structured demonstrator: member over cons/nil ──────────────────────────

    #[test]
    fn member_cons_demonstrator_enumerates_structured_answers_with_proofs() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let member = evals
            .iter()
            .find(|e| e.name == "member-cons")
            .expect("the member-cons demonstrator is shipped");
        assert_eq!(member.status, "ok");
        // The three list elements are enumerated as structured answers.
        let mut bound: Vec<String> = member
            .answers
            .iter()
            .map(|a| a.bindings["M"].clone())
            .collect();
        bound.sort();
        assert_eq!(bound, vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
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
                ans.atom.starts_with("member(") && ans.atom.contains("cons("),
                "the answer atom is a structured cons-list membership: {}",
                ans.atom
            );
        }
    }

    #[test]
    fn projection_carries_structured_member_answers_and_derivation() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let nt = project_goal_directed(&evals);
        assert!(
            nt.contains("member(a,cons(a,cons(b,cons(c,nil))))"),
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

    #[test]
    fn win_wfs_demonstrator_carries_three_valued_verdicts() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let win = evals
            .iter()
            .find(|e| e.name == "win-wfs-negation")
            .expect("the win-wfs-negation demonstrator is shipped");
        assert_eq!(win.status, "ok");
        // The only well-founded-TRUE goal answer is win(c).
        let ws: Vec<String> = win
            .answers
            .iter()
            .map(|a| a.bindings["W"].clone())
            .collect();
        assert_eq!(ws, vec!["c".to_owned()], "only c is a founded win: {ws:?}");

        let verdict_of = |atom: &str| {
            win.verdicts
                .iter()
                .find(|v| v.atom == atom)
                .unwrap_or_else(|| panic!("verdict for {atom} present: {:?}", win.verdicts))
                .verdict
                .as_str()
        };
        // The a⇄b negative loop is well-founded UNDEFINED (never a fabricated true/false).
        assert_eq!(verdict_of("win(a)"), "undefined", "even cycle ⇒ undefined");
        assert_eq!(verdict_of("win(b)"), "undefined", "even cycle ⇒ undefined");
        // The founded positions are a definite true/false.
        assert_eq!(verdict_of("win(c)"), "true", "move to lost d ⇒ won");
        assert_eq!(verdict_of("win(d)"), "false", "no move ⇒ lost");
    }

    #[test]
    fn projection_carries_undefined_and_founded_wfs_verdicts() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let nt = project_goal_directed(&evals);
        // The distinctive SLG-WFS surface: an undefined verdict AND both founded verdicts.
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
            has_verdict("win(a)", "undefined"),
            "win(a) is serialized as undefined:\n{nt}"
        );
        assert!(
            has_verdict("win(c)", "true"),
            "win(c) is serialized as a founded true:\n{nt}"
        );
        assert!(
            has_verdict("win(d)", "false"),
            "win(d) is serialized as a founded false:\n{nt}"
        );
    }

    // ── Math sub-sort demonstrator (order-sorted ℤ ⊑ ℝ) + incomparable control ───────────

    #[test]
    fn math_subsort_demonstrator_resolves_only_via_the_lattice() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let subsort = evals
            .iter()
            .find(|e| e.name == "math-subsort")
            .expect("the math-subsort demonstrator is shipped");
        assert_eq!(subsort.status, "ok");
        assert_eq!(
            subsort.answers.len(),
            1,
            "an Integer constant binds a RealNumber variable (ℤ ⊑ ℝ): {:?}",
            subsort.answers
        );
        let ans = &subsort.answers[0];
        assert_eq!(
            ans.bindings.get("X").map(String::as_str),
            Some("one"),
            "the subsort-unified answer binds X = one"
        );
        assert_eq!(ans.atom, "p(one)", "the answer atom is p(one)");
        assert!(ans.proof_checks, "the subsort answer is proof-checked");

        // The incomparable control yields NO answer (Integer ⋢ Set).
        let control = evals
            .iter()
            .find(|e| e.name == "math-subsort-control")
            .expect("the math-subsort-control demonstrator is shipped");
        assert_eq!(control.status, "ok");
        assert!(
            control.answers.is_empty(),
            "an Integer constant does NOT bind an incomparable-sort (Set) variable: {:?}",
            control.answers
        );
    }

    #[test]
    fn projection_carries_the_subsort_unified_answer() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let nt = project_goal_directed(&evals);
        assert!(
            nt.contains("<https://blackcatinformatics.ca/gmeow/goalDirectedAtom> \"p(one)\""),
            "the projection carries the subsort-unified answer atom p(one):\n{nt}"
        );
        assert!(
            nt.contains("\"X = one\""),
            "the projection carries the subsort-unified binding X = one:\n{nt}"
        );
    }

    // ── Every shipped demonstrator answer proof-checks; whole projection is non-vacuous ──

    #[test]
    fn every_shipped_answer_proof_checks_and_projection_is_deterministic() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
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
            "the corpus ships several proof-checked answers (peano + 3 members + subsort): \
             got {total_answers}"
        );

        // Two evaluations produce byte-identical serialization (no hash-iteration order).
        let nt_first = project_goal_directed(&evals);
        assert!(!nt_first.is_empty(), "the projection is non-empty");
        let evals2 = evaluate_shipped_demonstrators().expect("second evaluation");
        let nt_second = project_goal_directed(&evals2);
        assert_eq!(
            nt_first, nt_second,
            "two independent evaluations serialize byte-identically (deterministic)"
        );
    }
}
