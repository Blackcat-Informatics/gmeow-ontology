// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The goal-directed (backward) demonstrator façade — the production surface that makes
//! the proof-carrying full-FOL backward engine non-dark.
//!
//! The proof-carrying backward engine is PurRDF's order-sorted
//! [`resolve_fol`](purrdf::datalog::resolve_fol::resolve_fol), paired with its
//! Curry–Howard [`check_fol_proof`](purrdf::datalog::resolve_fol::check_fol_proof).
//! This module is the single thin, honest `pub` façade over that shared substrate: it
//! lowers the AUTHORED `logic:ReasoningProgram` corpus (structured — function-symbol —
//! logic programs the flat query text-parser cannot express) into PurRDF's
//! [`TermDag`](purrdf::datalog::term::TermDag), evaluates each program through
//! [`evaluate_reasoning_programs`](crate::goal_directed::evaluate_reasoning_programs),
//! validates every answer's proof, and projects the checked answers plus their
//! content-addressed derivation IRIs into RDF-serializable data the `gmeow-pipeline`
//! `stage-goal-directed` folds into `graph/goal-directed` of `gmeow.gts`.
//!
//! It is NOT a fork of the engine: it constructs PurRDF [`FolProgram`](purrdf::datalog::resolve_fol::FolProgram)s, reads back
//! PurRDF [`FolProof`](purrdf::datalog::resolve_fol::FolProof)s, and never re-implements
//! resolution or proof checking. There is
//! exactly ONE production source of goal-directed programs — the authored
//! `logic:ReasoningProgram` cells compiled by `gmeow-logic-compile` (see
//! `slices/grounding/logic/examples/reasoning-programs.ttl`); the earlier hand-interned
//! Rust-constant demonstrator corpus has been removed (GREENFIELD — no second source of
//! goal-directed programs may remain).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gmeow_logic_compile::ir::{
    EvaluationMode, Formula, ReasoningProgramIr, Term, VariableSortScope,
};
use purrdf::TermValue;

// The goal-directed backward lane resolves on the SHARED PurRDF datalog SLG-WFS substrate
// (order-sorted `resolve_fol`), NOT the native `crate::physical::` backward engine. Independent
// native forward/DL-EL and non-goal-directed query surfaces remain separate. The RDF-authored
// `logic:ReasoningProgram` corpus-lowering stays here (PurRDF mints no `logic:` vocabulary); it
// now lowers into PurRDF's `TermDag`/`FolProgram` and reads back PurRDF's `FolProof`. gmeow keeps
// its OWN derivation-IRI scheme
// (`provenance::DERIVATION_PREFIX` over PurRDF's content digest) — PurRDF mints no IRIs
// (`docs/CUTOVER.md`: "deriving an IRI from the digest is caller vocabulary").
use purrdf::datalog::id::{MetaId, NodeId};
use purrdf::datalog::resolve_fol::{
    FolBudget, FolClause, FolControl, FolLit, FolProgram, FolStatus, Truth, check_fol_proof,
    derivation_id, render, resolve_fol,
};
use purrdf::datalog::term::TermDag;
use purrdf::datalog::unify::{SortContext, SortOrder};
// The native forward-chase differential oracle (below) is UNCHANGED by the backward-lane cutover:
// it builds its own `rule_ir` least-model over the SAME authored program to cross-check PurRDF's
// backward answers, and is arena-independent (its own `EvalAtom`/`Fact` representation, never the
// resolver's `TermDag`), so it keeps using the native `rule_ir` forward evaluator.
use crate::rule_ir::{
    EvalAtom, EvalRule, EvalTerm, Fact, FactStore, Solution, least_model_of_reduct, match_atom,
};

/// The gmeow namespace every projected goal-directed IRI/predicate lives under.
pub(crate) const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The grounding step budget every authored `logic:ReasoningProgram` resolves under.
///
/// The retired native backward engine ran these with an *unbounded* budget (its `Budget`'s
/// `max_steps` defaulted to `None`); PurRDF's [`FolBudget`] makes the bound mandatory, so the
/// faithful port is `u64::MAX` — high enough that the authored, terminating corpus always
/// grounds to [`FolStatus::Complete`], never demoted to [`FolStatus::Partial`] (which would
/// distrust negation and silently change verdicts). This is a determinism-preserving constant,
/// not a capability degradation: the same program grounds to the same fixpoint every run.
pub(crate) const GROUNDING_BUDGET: FolBudget = FolBudget {
    max_steps: u64::MAX,
};
/// The XSD boolean datatype IRI for the proof-checked flag.
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// One checked answer to a demonstrator's goal: the ground answer atom surface, the goal
/// variable bindings, the content-addressed derivation (proof) IRI, and whether the proof
/// proof checker re-derives as exactly that atom. Every field is RDF-serializable
/// (strings), so the
/// pipeline can fold it without reaching into the engine's private term handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDirectedAnswer {
    /// The ground answer atom rendered to its functional surface, e.g.
    /// `add(s(s(zero)),s(zero),s(s(s(zero))))`.
    pub atom: String,
    /// The goal variable → resolved sub-term surface map (deterministic, sorted keys).
    pub bindings: BTreeMap<String, String>,
    /// The content-addressed derivation IRI of this answer's proof
    /// (the GMEOW derivation namespace over PurRDF's content-addressed
    /// [`derivation_id`]).
    pub derivation_iri: String,
    /// Whether PurRDF's [`check_fol_proof`] re-derived exactly [`Self::atom`]. Always
    /// `true` for a shipped answer (a proof that fails to check HARD-fails the evaluation).
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
    /// The FULL authored `logic:ReasoningProgram` IRI — the demonstrator's collision-free
    /// identity. Every minted resource IRI (query / answer / verdict / program node) folds
    /// this, NOT [`Self::name`]: two programs authored under distinct IRIs that happen to share
    /// a local name (`https://a.example/prog` and `https://b.example/prog`) must never collapse
    /// to the same projected nodes.
    pub iri: String,
    /// The demonstrator's local (last path-segment) name — HUMAN DISPLAY TEXT only (the
    /// `goalDirectedName` literal), never a resource-IRI identity (see [`Self::iri`]).
    pub name: String,
    /// The prose description of what the demonstrator demonstrates.
    pub description: String,
    /// The rendered goal template (free metavariables shown as `?n`), e.g.
    /// `add(s(s(zero)),s(zero),?0)`.
    pub goal: String,
    /// The grounding status of the resolution (`ok` / `partial`).
    pub status: String,
    /// The proof-checked answers, in a TOTAL order over `(atom, bindings, derivation_iri)`
    /// for determinism (G12) — atom alone is not a total order, since two answers may share
    /// the identical ground conclusion via distinct derivations.
    pub answers: Vec<GoalDirectedAnswer>,
    /// The probed three-valued WFS verdicts, in a TOTAL order over `(atom, verdict)` for
    /// determinism (G12). Non-empty only for a negation demonstrator (e.g. `win`/`move`),
    /// where it carries the `undefined` loop atoms alongside the founded `true`/`false`
    /// atoms.
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
///
/// `pub(crate)` so [`crate::proof_tree`]'s structured proof view resolves a compiled
/// `logic:ReasoningProgram` through THIS lowering rather than forking a second one — there is
/// exactly one `ReasoningProgramIr` → `FolProgram` compiler and it lives here.
pub(crate) struct BuiltDemonstrator {
    /// The demonstrator's own term arena.
    pub(crate) dag: TermDag,
    /// The structured backward program (clauses + goal + goal vars + meta-sorts).
    pub(crate) program: FolProgram,
    /// The order-sorted lattice/tagging the resolver consults (empty ⇒ the unsorted path).
    pub(crate) ctx: SortContext,
    /// Ground atoms whose SLG-WFS verdict is projected (`true`/`false`/`undefined`). Empty for
    /// a purely-positive demonstrator; non-empty for the negation demonstrator so its
    /// `undefined` loop atoms and founded atoms are both observable.
    pub(crate) verdict_probes: Vec<NodeId>,
}

/// Evaluate one built program (lowered from a compiled `logic:ReasoningProgram` by
/// [`lower_reasoning_program`]): resolve its goal, validate + project each answer, and
/// record each verdict probe's three-valued WFS verdict. `name` / `description` are taken as
/// plain string slices (rather than folded into [`BuiltDemonstrator`] itself) because a
/// compiled program's identity is a runtime `String` (its authored IRI).
fn evaluate_demonstrator(
    iri: &str,
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
    // `resolve_fol` returns a `FolControl` directly (no `Result`): an `Unsupported` decision is a
    // structural refusal of the backward engine, mapped here to a hard `Physical` diagnostic.
    let outcome = match resolve_fol(&mut dag, &program, &ctx, &GROUNDING_BUDGET) {
        FolControl::Decided(outcome) => outcome,
        FolControl::Unsupported(kind) => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed program {iri:?} is unsupported by the backward engine: {kind:?}"
                ),
            }));
        }
    };
    // Grounding-completion status. PurRDF's `FolStatus` is `{Complete, Partial}` (a budget-cut
    // grounding is `Partial`, and its negation is no longer trusted); project it to the SAME
    // stable surface the retired native engine shipped (`ok`/`partial`). With `GROUNDING_BUDGET`
    // unbounded the authored corpus always grounds `Complete` ⇒ `ok`.
    let status = match outcome.status {
        FolStatus::Complete => "ok",
        FolStatus::Partial => "partial",
    }
    .to_owned();
    // The `not_false` predicate `check_fol_proof` charges each negative literal against: an atom
    // is admissible as "not false" iff the well-founded model does not place it in the false set.
    // Read straight from THIS outcome's three-valued model (`truth_of`), so the proof checker and
    // the shipped verdicts agree by construction. Captures `&outcome` only; `dag` is passed in.
    let not_false = |d: &TermDag, n: NodeId| outcome.truth_of(d, n) != Truth::False;
    let mut answers = Vec::with_capacity(outcome.answers.len());
    for ans in &outcome.answers {
        // Curry–Howard check: the proof MUST re-derive exactly the answer atom. A proof
        // that fails to check, or checks to a different atom, is a hard fail — the whole
        // point of shipping proof objects is that every shipped answer is proof-carrying.
        let checked = check_fol_proof(
            &mut dag,
            &ans.proof,
            &program.clauses,
            &program.meta_sorts,
            &ctx,
            &not_false,
        )
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed program {iri:?} answer proof failed to check: {e:?}"
                ),
            })
        })?;
        if checked != ans.atom {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed program {iri:?} proof re-derives a different atom than its answer"
                ),
            }));
        }
        // gmeow's derivation IRI = its OWN vocabulary (`DERIVATION_PREFIX`) over PurRDF's
        // content-addressed proof digest (`derivation_id`, a SHA-1 over `FolProof::rule_identity`
        // and the recursive sub-proof digests). PurRDF mints no IRIs; the digest is byte-stable
        // run-to-run, so the derived IRI is too (`docs/CUTOVER.md` §4).
        let derivation_iri = format!(
            "{}{}",
            crate::provenance::DERIVATION_PREFIX,
            derivation_id(&dag, &ans.proof)
        );
        answers.push(GoalDirectedAnswer {
            atom: render(&dag, ans.atom),
            bindings: ans.bindings.clone(),
            derivation_iri,
            proof_checks: true,
        });
    }
    // G12: a TOTAL order, not merely atom-order — two answers can share the same ground
    // atom (distinct derivations of the identical conclusion), and `sort_by` is stable, so
    // sorting on the atom alone would let a tie's relative order depend on whatever order
    // the engine happened to produce them in, which is NOT guaranteed byte-stable across
    // independent evaluations. Folding in `bindings` (a `BTreeMap`, itself totally ordered)
    // and `derivation_iri` makes the comparator a genuine total order over the answer's own
    // content, so the sorted sequence — and hence [`project_goal_directed`]'s output — is
    // deterministic regardless of internal evaluation order.
    answers.sort_by(|a, b| {
        a.atom
            .cmp(&b.atom)
            .then_with(|| a.bindings.cmp(&b.bindings))
            .then_with(|| a.derivation_iri.cmp(&b.derivation_iri))
    });
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
    // G12: same total-order rationale as `answers` above — fold in `verdict` so a tie on
    // `atom` alone (which cannot occur for a single probe set today, but is not an
    // invariant this function should silently depend on) still sorts deterministically.
    verdicts.sort_by(|a, b| a.atom.cmp(&b.atom).then_with(|| a.verdict.cmp(&b.verdict)));
    Ok(GoalDirectedEvaluation {
        iri: iri.to_owned(),
        name: local_name(iri).to_owned(),
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
    // `clause.body` is already in canonical (`content_key`) order — `lower_body` flattens and
    // sorts the authored conjunction before lowering — so publishing it in `Vec` order here is
    // byte-stable regardless of the authored RDF object order.
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
    dag.intern_leaf(s)
}

// ─────────────────────────────────────────────────────────────────────────────────────
// The reasoning-program compiler: `ReasoningProgramIr` → `BuiltDemonstrator`.
// ─────────────────────────────────────────────────────────────────────────────────────
//
// This is the SOLE production source of goal-directed programs: it lowers an
// authored+compiled `logic:ReasoningProgram`
// (`gmeow_logic_compile::ir::ReasoningProgramIr`) into the
// `FolProgram`/`SortContext`/verdict-probe shape [`evaluate_demonstrator`]
// resolves, proof-checks, verdict-probes, and projects — there is no second engine, and
// no second SOURCE; the earlier hand-interned Rust-constant demonstrator corpus has
// been removed.
//
// ## One lowering onto the shared PurRDF arena
//
// This module owns the vocabulary-boundary lowering from compiled `logic:` IR into
// PurRDF's vocabulary-neutral [`TermDag`]. An IRI/function symbol becomes a rigid leaf, a
// literal becomes an injectively quoted leaf, and an application becomes an app node.
// Reasoning-program clauses and queries are implicitly universally quantified and carry no
// explicit binder, so each `Term::Var` must become a bindable `NodeData::Meta`, not a rigid
// leaf. A [`VarScope`] implements that policy: the FIRST occurrence of a name in one scope
// mints a fresh [`TermDag::fresh_meta`], every LATER occurrence of the SAME name in the SAME
// scope reuses it, and a fresh scope per clause/query/probe keeps same-named variables in
// different clauses distinct. Unsupported sequence markers and non-atomic clause/query
// positions HARD-FAIL at this boundary; they are never weakened into a different program.

/// The per-scope variable→metavariable map a single clause/query/probe lowers under: the
/// FIRST occurrence of a name mints a fresh metavariable ([`TermDag::fresh_meta`]); every
/// later occurrence of the SAME name in the SAME scope reuses it. A fresh, empty map per
/// clause/query/probe is what keeps two clauses' same-named variables from colliding.
type VarScope = HashMap<String, (MetaId, NodeId)>;

/// Encode a `logic:` literal as an injective PurRDF leaf-symbol string.
///
/// PurRDF-datalog's `TermDag::intern_leaf` interns a bare `&str` symbol (the gmeow-term-arena
/// `TermValue` coupling was stripped at the substrate boundary — `docs/CUTOVER.md` §4), so a
/// literal is carried as its N-Triples-quoted lexical form (`"lex"` or `"lex"^^dt`) with the
/// lexical form backslash/quote-escaped. The surrounding quotes keep a literal leaf injectively
/// DISTINCT from an IRI/function-symbol leaf (which is interned as its bare IRI text), and the
/// `^^dt` suffix keeps two same-lexical literals of different datatype distinct — the two
/// discriminations `unify`/`canon` need to keep interned leaves that denote different terms apart.
fn encode_literal(literal: &purrdf::RdfLiteral) -> String {
    purrdf::RdfTerm::literal(literal.clone()).to_string()
}

/// Lower a `logic:` [`Term`] into PurRDF's [`TermDag`] under `scope`'s free-variable policy: a
/// `Term::Var` mints a fresh [`TermDag::fresh_meta`] on FIRST occurrence in this scope and reuses
/// it on every later occurrence (a `logic:ReasoningProgram` clause/query carries no explicit
/// binder, so each of its variables is a resolver metavariable). An IRI/function-symbol interns as
/// a bare-symbol leaf, a literal as its injective encoded form, and a compound application recurses
/// through this same function. Mirrors `crate::physical::lower::lower_term_in`'s rules, re-targeted
/// onto PurRDF's arena. A [`Term::SequenceMarker`] is a HARD FAIL (no variadic node).
fn lower_term(
    dag: &mut TermDag,
    term: &Term,
    scope: &mut VarScope,
) -> gmeow_errors::Result<NodeId> {
    Ok(match term {
        Term::Iri(s) => dag.intern_leaf(s),
        Term::Literal(literal) => dag.intern_leaf(&encode_literal(literal)),
        Term::Var(name) => {
            if let Some((_, node)) = scope.get(name) {
                *node
            } else {
                let (meta, node) = dag.fresh_meta();
                scope.insert(name.clone(), (meta, node));
                node
            }
        }
        Term::App { symbol, args } => {
            let op = dag.intern_leaf(symbol);
            let mut arg_nodes = Vec::with_capacity(args.len());
            for a in args {
                arg_nodes.push(lower_term(dag, a, scope)?);
            }
            dag.intern_app(op, arg_nodes)
        }
        Term::SequenceMarker(name) => {
            return Err(reasoning_program_err(format!(
                "sequence marker {name:?} binds a variable-length sequence, not a single term; the \
                 fixed-arity backward engine has no variadic-binder node, so lowering it is a hard \
                 fail rather than a silent single-term coercion"
            )));
        }
    })
}

/// Lower an atomic `logic:` [`Formula::Atom`] under `scope` into a PurRDF `App` node (the relation
/// IRI as the operator, the arguments as children), HARD-FAILING on any other formula shape — the
/// backward engine's clause head / body literal / query / verdict-probe positions each require
/// exactly one atomic predication.
fn lower_atom(
    dag: &mut TermDag,
    formula: &Formula,
    scope: &mut VarScope,
) -> gmeow_errors::Result<NodeId> {
    match formula {
        Formula::Atom { relation, args } => {
            let op = lower_term(dag, relation, scope)?;
            let mut arg_nodes = Vec::with_capacity(args.len());
            for a in args {
                arg_nodes.push(lower_term(dag, a, scope)?);
            }
            Ok(dag.intern_app(op, arg_nodes))
        }
        other => Err(reasoning_program_err(format!(
            "reasoning-program atom position requires an atomic logic:Formula (a single \
             predication); found a compound formula {other:?}"
        ))),
    }
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
        Formula::And(_) => {
            // `Formula::And` has ORDER-NORMALIZED identity (its `content_key` sorts operands),
            // but the antecedent is authored as RDF whose `logic:and` operands carry no explicit
            // index, so their stored order is only as stable as per-document blank-node
            // interning. Flatten nested conjunctions to their leaf conjuncts and lower them in
            // the SAME canonical order `content_key` imposes, so equivalent programs yield
            // byte-identical clause text, metavariable numbering, and content-addressed IRIs
            // regardless of RDF object order. This changes NO shipped answer (a conjunctive body
            // is commutative for resolution) — only the deterministic surface.
            let mut conjuncts: Vec<&Formula> = Vec::new();
            flatten_and_conjuncts(formula, &mut conjuncts);
            conjuncts.sort_by_key(|f| f.content_key());
            for c in conjuncts {
                lower_body(dag, c, scope, out)?;
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

/// Flatten a conjunction to its leaf conjuncts, mirroring `Formula`'s own
/// `flatten_commutative`: a nested `Formula::And` is spliced into its parent, everything else
/// is a leaf. The result is the operand list `Formula::And`'s order-normalized `content_key`
/// keys over, so sorting it by `content_key` reproduces that canonical order at lowering time.
fn flatten_and_conjuncts<'a>(formula: &'a Formula, out: &mut Vec<&'a Formula>) {
    match formula {
        Formula::And(parts) => {
            for p in parts {
                flatten_and_conjuncts(p, out);
            }
        }
        other => out.push(other),
    }
}

/// Lower one clause [`Formula`] (a fact atom, or `Formula::Implies(antecedent, consequent)`
/// rule) under a FRESH [`VarScope`] into a [`FolClause`]. `rule_index` is the clause's position
/// in AUTHORED program order (`program.clauses` is already the stable post-`ReasoningProgramIr`
/// order), which is exactly what PurRDF's [`FolClause::rule`] addresses.
///
/// PurRDF (unlike the retired native engine) does NOT mint a digest-addressed rule-firing IRI
/// on the clause: the content-addressed identity gmeow ships is now the PROOF digest — PurRDF's
/// [`derivation_id`] hashes `FolProof::rule_identity` (which folds in the fired clause's content),
/// and [`evaluate_demonstrator`] derives the answer's `derivation_iri` from it under
/// [`crate::provenance::DERIVATION_PREFIX`]. So the run-to-run-stable content-addressing
/// requirement is met at the proof layer, not on the clause; the clause carries only its
/// authored-order index. See `docs/CUTOVER.md` §4 ("deriving an IRI from the digest is caller
/// vocabulary").
fn lower_clause(
    dag: &mut TermDag,
    program_iri: &str,
    clause: &Formula,
    scope: &mut VarScope,
    rule_index: usize,
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
    Ok(FolClause {
        head,
        body,
        rule: rule_index,
    })
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
pub(crate) fn lower_reasoning_program(
    program: &ReasoningProgramIr,
    subsort_edges: &[(String, String)],
) -> gmeow_errors::Result<BuiltDemonstrator> {
    // `EvaluationMode` is a closed, single-variant enum today — `EvaluationMode::from_local`
    // (the ONLY constructor reachable from parsed input) rejects every IRI except
    // `logic:BackwardEvaluation` at the parse stage, so a `ReasoningProgramIr` carrying
    // any other mode is already unrepresentable by construction. This irrefutable pattern
    // documents that exhaustively: it is a COMPILE ERROR (not a runtime `unreachable!`) the
    // day a second `EvaluationMode` variant lands without this dispatch being extended.
    let EvaluationMode::Backward = program.mode;

    let mut dag = TermDag::new();
    let mut meta_sorts: BTreeMap<MetaId, NodeId> = BTreeMap::new();
    // Per-SCOPE variable-sort lookup: `program.variable_sorts` carries the owning scope with
    // every `(name → sort)` declaration, because each clause / the query is a FRESH variable
    // scope (fresh metavariables). Applying one program-global name→sort map to every scope
    // would (a) let a sort on `X` in clause 1 wrongly constrain an unrelated `X` in clause 2 and
    // (b) force two clauses' legitimately-different `X` sorts into one entry. So each scope
    // draws only its OWN declarations, keyed by the owning clause's `content_key`.
    let sorts_in_scope = |scope: &VariableSortScope| -> HashMap<&str, &str> {
        program
            .variable_sorts
            .iter()
            .filter(|(s, _, _)| s == scope)
            .map(|(_, v, srt)| (v.as_str(), srt.as_str()))
            .collect()
    };

    let mut clauses = Vec::with_capacity(program.clauses.len());
    for (idx, clause_formula) in program.clauses.iter().enumerate() {
        let mut scope: VarScope = HashMap::new();
        let clause = lower_clause(&mut dag, &program.iri, clause_formula, &mut scope, idx)?;
        // The scope is keyed by `content_key` PLUS the clause's occurrence index among clauses
        // sharing that key (the number of prior such clauses). `program.clauses` is the
        // post-`ReasoningProgramIr::new` order; because `new` sorts clauses STABLY and two
        // clauses with an identical `content_key` have an identical `sort_key`, this occurrence
        // index matches the one the frontend recorded — so two structurally-identical clauses
        // with different `logic:variableSort` declarations each draw their OWN scope's sorts.
        let key = clause_formula.content_key();
        let occurrence = program.clauses[..idx]
            .iter()
            .filter(|prior| prior.content_key() == key)
            .count();
        let clause_sorts = sorts_in_scope(&VariableSortScope::Clause { key, occurrence });
        for (name, (meta, _)) in &scope {
            if let Some(sort_iri) = clause_sorts.get(name.as_str()) {
                let sort_node = leaf(&mut dag, sort_iri);
                meta_sorts.insert(*meta, sort_node);
            }
        }
        clauses.push(clause);
    }

    let mut query_scope: VarScope = HashMap::new();
    let goal = lower_atom(&mut dag, &program.query, &mut query_scope)?;
    let query_sorts = sorts_in_scope(&VariableSortScope::Query);
    for (name, (meta, _)) in &query_scope {
        if let Some(sort_iri) = query_sorts.get(name.as_str()) {
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
        // A verdict probe is a GROUND atom (enforced by `ReasoningProgramIr::new`), so its
        // lowering mints no metavariables and carries no per-scope sort obligations.
        let mut scope: VarScope = HashMap::new();
        let probe = lower_atom(&mut dag, probe_formula, &mut scope)?;
        debug_assert!(
            scope.is_empty(),
            "a verdict probe must be ground (no metavariables): {probe_formula:?}"
        );
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

    // CONSTANT order-sort tagging (`SortContext::term_sorts`): `program.constant_sorts` is
    // the `(constant IRI, rdf:type IRI)` capture — the plain domain `rdf:type` triple a
    // constant like `ex:one` carries, which the stage's L3 fold otherwise drops (it is not
    // `logic:` structural vocabulary). Interning each constant/sort IRI through the SAME
    // `leaf` helper `lower_atom`'s `Term::Iri` arm uses means hash-consing resolves a
    // constant referenced both here and inside a clause/query/probe to the IDENTICAL
    // `NodeId` — this is what lets `unify_sorted` discriminate a typed constant (only
    // unifiable with a variable whose declared sort is ⊒ its own) from an untyped one
    // (order-sort top, unifies with any variable sort) instead of every constant being
    // silently untyped and unification degenerating to unsorted unification.
    //
    // A constant may carry SEVERAL asserted `rdf:type` sorts at once (`ex:c a math:Set,
    // math:Integer`), and the IR deliberately RETAINS EVERY `(constant, type)` pair. Collapsing
    // them with a last-write-wins map would keep only the lexically-last type and WRONGLY reject
    // a binding a dropped type would license (if `Set` won and the query variable is
    // `RealNumber`-sorted, the valid `Integer ⊑ Real` binding is lost). So a multiply-typed
    // constant is tagged with a SYNTHETIC meet sort that is a subsort of EVERY asserted type:
    // `synth ⊑ S` then holds (via `synth ⊑ Tᵢ ⊑ S`) exactly when SOME asserted type `Tᵢ ⊑ S`,
    // matching the engine's order-sort semantics — a constant binds a variable of sort `S` when
    // ANY of its types satisfies `S`. A single-typed constant is tagged directly (unchanged).
    let mut const_types: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (const_iri, sort_iri) in &program.constant_sorts {
        const_types
            .entry(const_iri.as_str())
            .or_default()
            .push(sort_iri.as_str());
    }
    let mut term_sorts: BTreeMap<NodeId, NodeId> = BTreeMap::new();
    for (const_iri, sort_iris) in &const_types {
        let const_node = leaf(&mut dag, const_iri);
        let tagged = if let [single] = sort_iris.as_slice() {
            leaf(&mut dag, single)
        } else {
            // A synthetic bottom sort, content-addressed on the constant IRI (deterministic and
            // never colliding with an authored sort), made a subsort of every asserted type.
            let synth_iri = format!(
                "{GMEOW}goal-directed/const-meet-sort/{}",
                blake3::hash(const_iri.as_bytes()).to_hex()
            );
            let synth_node = leaf(&mut dag, &synth_iri);
            for sort_iri in sort_iris {
                let sort_node = leaf(&mut dag, sort_iri);
                edges.push((synth_node, sort_node));
            }
            synth_node
        };
        term_sorts.insert(const_node, tagged);
    }

    // Built AFTER the synthetic constant-meet edges are appended, so their reflexive-transitive
    // closure is part of the order `unify_sorted` consults.
    let order = SortOrder::from_subclass_edges(&edges);
    let ctx = SortContext::new(order, term_sorts, BTreeMap::new());

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
/// surface, the SOLE production source of goal-directed programs — against the
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
        let eval = evaluate_demonstrator(&program.iri, &description, built)?;
        // T3: the cross-engine (backward-vs-forward) fixpoint-agreement oracle. Only a
        // program inside the forward-evaluable definite/function-free/binary fragment is
        // checked (see `is_definite_function_free_binary`'s doc for exactly which shipped
        // programs qualify); every other program is evaluated exactly as before.
        if is_definite_function_free_binary(program) {
            cross_check_forward_agreement(program, &eval)?;
        }
        evals.push(eval);
    }
    // Sort by the FULL authored IRI (the collision-free identity), falling back to name only
    // for byte-stability of the human-facing order; two distinct programs are never merged.
    evals.sort_by(|a, b| a.iri.cmp(&b.iri).then_with(|| a.name.cmp(&b.name)));
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
    // The forward chase runs UNSORTED (no `SortContext`): it admits any constant regardless of
    // the order-sort lattice, so an order-sorted program's forward least model can legitimately
    // include bindings the sorted backward answer set correctly rejects. Cross-checking the two
    // would then HARD-FAIL spuriously. An order-sorted program (any `variable_sorts`) is
    // therefore outside this unsorted oracle's fragment — never cross-checked.
    if !program.variable_sorts.is_empty() {
        return false;
    }
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
    matches!(term, Term::Var(_) | Term::Iri(_) | Term::Literal(_))
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
        Term::Literal(literal) => Ok(EvalTerm::ConstLit(crate::rule_ir::literal_value(literal))),
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
        numeric: Vec::new(),
        head: oracle_eval_atom(head_formula)?,
        body,
        rule_iri: format!("{GMEOW}goal-directed/oracle-rule/{idx}"),
        distinct_pairs: Vec::new(),
        builtins: Vec::new(),
        reduction: None,
        constraint_tag: None,
    })
}

/// Render one forward-derived ground [`Fact`] to the SAME functional surface PurRDF's
/// [`render`]/[`GoalDirectedAnswer::atom`] uses — `pred(subject, object)` with PurRDF's `", "`
/// argument separator (`resolve_fol::render` joins app arguments with `", "`), bare IRI text —
/// so the forward and backward answer sets compare as plain strings.
fn oracle_render_fact(fact: &Fact) -> String {
    format!(
        "{}({}, {})",
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

/// The query individual IRI of a demonstrator — the primary projected node every answer /
/// verdict / program triple hangs off.
///
/// Collision-safety: the node folds the FULL authored program IRI (a `blake3` of it), never
/// the bare local name. Two programs authored under distinct IRIs that share a local name
/// (`https://a.example/prog` vs `https://b.example/prog`) therefore mint DISTINCT query nodes
/// and never merge their projected triples. The local name rides only as a readable path
/// segment (human orientation), never as the identity.
fn query_iri(eval: &GoalDirectedEvaluation) -> String {
    let hash = blake3::hash(eval.iri.as_bytes()).to_hex();
    format!("{GMEOW}goal-directed/{}/{hash}", eval.name)
}

/// The content-addressed answer individual IRI of one checked [`GoalDirectedAnswer`].
///
/// G12: a `blake3` hash folding the demonstrator's FULL authored IRI, the ground answer atom,
/// its sorted variable bindings, and its derivation-proof IRI — NEVER a positional
/// `/answer/{idx}`, and NEVER the bare local name (collision-safety, see [`query_iri`]). A
/// positional index is only stable if the caller's `Vec<GoalDirectedAnswer>` is visited in
/// the exact same order on every run; [`evaluate_demonstrator`]'s sort is a total order over
/// this SAME content, but minting the IRI from the content directly (rather than from
/// wherever it lands after sorting) makes the projection byte-stable and
/// order-independent by construction, exactly like [`program_iri_for`].
fn answer_iri(eval: &GoalDirectedEvaluation, ans: &GoalDirectedAnswer) -> String {
    let mut key = String::new();
    key.push_str(&eval.iri);
    key.push('\u{0}');
    key.push_str(&ans.atom);
    for (var, surface) in &ans.bindings {
        key.push('\u{0}');
        key.push_str(var);
        key.push('=');
        key.push_str(surface);
    }
    key.push('\u{0}');
    key.push_str(&ans.derivation_iri);
    let hash = blake3::hash(key.as_bytes()).to_hex();
    format!("{}/answer/{hash}", query_iri(eval))
}

/// The content-addressed WFS-verdict individual IRI of one probed [`GoalDirectedVerdict`].
///
/// G12: a `blake3` hash folding the demonstrator's FULL authored IRI, the probed ground atom,
/// and its three-valued verdict — never a positional `/verdict/{idx}`, and never the bare
/// local name, for the same byte-stability/order-independence/collision-safety reason as
/// [`answer_iri`].
fn verdict_iri(eval: &GoalDirectedEvaluation, v: &GoalDirectedVerdict) -> String {
    let mut key = String::new();
    key.push_str(&eval.iri);
    key.push('\u{0}');
    key.push_str(&v.atom);
    key.push('\u{0}');
    key.push_str(&v.verdict);
    let hash = blake3::hash(key.as_bytes()).to_hex();
    format!("{}/verdict/{hash}", query_iri(eval))
}

/// U2: mint a content-addressed `gmeow:GoalDirectedProgram` IRI from the evaluation's OWN
/// rendered program text (its name, clauses, query, and verdict-probe atoms) — a `blake3`
/// hash, never a [`NodeId`]/index, so the SAME authored program always mints the SAME
/// program IRI regardless of interning/evaluation order (byte-stable across independent
/// runs, exactly like [`answer_iri`]).
fn program_iri_for(eval: &GoalDirectedEvaluation) -> String {
    let mut key = String::new();
    key.push_str(&eval.iri);
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
        let q = query_iri(eval);
        lines.push(triple_iri(&q, RDF_TYPE, &p("GoalDirectedQuery")));
        lines.push(triple_lit(&q, &p("goalDirectedName"), &eval.name));
        lines.push(triple_lit(
            &q,
            &p("goalDirectedDescription"),
            &eval.description,
        ));
        lines.push(triple_lit(&q, &p("goalDirectedGoal"), &eval.goal));
        lines.push(triple_lit(&q, &p("goalDirectedStatus"), &eval.status));
        for ans in &eval.answers {
            let a = answer_iri(eval, ans);
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
        for v in &eval.verdicts {
            let vi = verdict_iri(eval, v);
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

#[path = "goal_directed.tests.rs"]
#[cfg(test)]
mod tests;
