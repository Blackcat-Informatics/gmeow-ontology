// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The proof lift tier: a parsed [`Derivation`] → `math:` structures.
//!
//! The lift map is `MATHEMATICS-BRIDGES.md`'s proof-as-process bridge, discharged edge for
//! edge against the shape `slices/grounding/math/tests/fixtures/bridges.ttl` pins:
//!
//! | TSTP | `math:` / `logic:` / `gmeow:` |
//! |---|---|
//! | the derivation as a whole | one `math:ProofDependencyGraph` and one `math:Proof`, both `gmeow:wasGeneratedBy` the run |
//! | a derived step (`plain` + `inference(…)`) | a `math:ProofStep`, reached by `math:proofStep` |
//! | an asserted leaf (`axiom`) | a `math:Axiom`, reached by `math:dependsOnAxiom` |
//! | a step's parents | `math:hasPremise` to each parent's node (and `math:dependsOnAxiom` when that parent is asserted) |
//! | a step's inference rule | the `math:operator` of the step's proof term, reached by `math:formalizesExpression` |
//! | a step's conclusion clause | a `math:MathematicalExpression` AST, reached by `math:hasConclusion` |
//! | the conclusion each step reaches | a `logic:GoalExpression` sub-goal (`logic:AchievementGoal` over a `logic:Situation`), reached by `math:provesGoal` |
//! | the QED | a `math:FormalVerificationResult` named by a `gmeow:Observation` with a `gmeow:Standpoint` vantage |
//!
//! # The rung is EARNED, not declared
//!
//! This is the only bridge claiming [`Rung::section_retraction`]
//! (`logic:SectionRetraction` / `logic:ExactPreservation` / `logic:Equiv`), and that
//! constructor's own doc makes the claim conditional: *"It is only honest if the lift
//! carries every step name, inference rule, parent edge, and rendered conclusion — i.e. if
//! the derivation genuinely reconstructs. The proof bridge owes a round-trip test for that
//! claim; without one this constructor must not be used."*
//!
//! The debt is paid by
//! `the_lift_is_a_section_the_derivation_reconstructs_from_the_graph_alone`, which
//! rebuilds every step name, role, inference rule, parent set, and rendered conclusion from
//! the emitted Turtle and NOTHING else, and requires the result to equal the parse. The
//! reader tier is the other half of the same bargain: at `ExactPreservation` a construct
//! the bridge does not structure cannot be dropped, so [`super::tstp`] hard-fails on every
//! one (a `file(…)` source, a `<useful_info>` field, a role other than `axiom`/`plain`).
//!
//! Two conventions make the reconstruction unambiguous, and both are load-bearing:
//!
//! - **`rdfs:label` on a step or axiom node is its TSTP NAME** — the identity a parent list
//!   cites. The rendered conclusion lives on the separate expression node reached by
//!   `math:hasConclusion` / `math:formalizesExpression`, never on the step.
//! - **`rdfs:label` on a `math:Operation` is the operator's RAW token** — the inference-rule
//!   IRI or the predicate symbol exactly as the derivation spells it, not a prettified form.
//!
//! # Why a step's rule rides on a proof TERM
//!
//! The `math:` proof layer declares no property from a `math:ProofStep` to the inference
//! rule that licenses it: `math:usesProofMethod` is `math:Proof`-domained and names a
//! strategy (direct, by contradiction, by induction), not a rule, and `gmeow:appliedRule` is
//! `rdfs:domain gmeow:InferenceApplication`, so using it here would entail typing a
//! `math:MathematicalObject` as a `logic:Relator`. Inventing a `math:` property is barred —
//! [`crate::ns`]: "a Rust-side IRI with no `module.ttl` declaration is a second source of
//! truth".
//!
//! What the ontology DOES declare is the thing a derivation step actually is. A TSTP step is
//! a proof term: `rule(parent-proof-terms…)`. So the step is `math:formalizesExpression`-ed
//! ("relates a mathematical concept or definition to the structured
//! math:MathematicalExpression that formalizes it — the target is an AST, never an opaque
//! display string") to a `math:ApplicationExpression` whose `math:operator` is the rule and
//! whose `math:argumentSlot`s are the parents' proof terms, carrying
//! `math:denotationKind math:denotesProof` — the kind the slice declares for exactly this
//! case ("an expression standing for a proof or proof-term … used where a proof term is
//! carried as structured content rather than as prose"). The same edge carries an asserted
//! leaf to the clause AST it states, which is what `math:Axiom`'s "never an opaque string"
//! demands.
//!
//! # Content-addressed interning
//!
//! Every expression is interned into a [`TermArena`] and its [`ContentKey`] mints its IRI,
//! so two steps concluding the same clause share ONE expression, a repeated sub-term is one
//! node, and two steps reached by an identical sub-derivation share one proof term. The step
//! and axiom nodes are keyed on the step NAME instead: a derivation's step identity is its
//! name, and two leaves stating the same clause under different names are two premises.
//!
//! # What this lift refuses rather than fakes
//!
//! - The `math:ProofDependencyGraph` carries no edge to the `math:Proof` it underlies. The
//!   class definition says it "names the math:Proof it underlies", but the slice declares no
//!   property for that edge, and the shipped fixtures (`tests/fixtures/bridges.ttl:85-86`,
//!   `examples/bridges.ttl:192-194`) both emit the DAG with only its type, its label, and
//!   `gmeow:wasGeneratedBy`. This lift emits exactly that rather than mint a term.
//! - The `math:FormalVerificationResult` claims `math:verificationPassed` from ONE named
//!   vantage — this bridge's own structural checker — and says in its standpoint's label
//!   exactly what that checker did and did not do. It never claims the inferences were
//!   re-derived: what was checked is well-foundedness, single-conclusion-ness, and that
//!   every step declares `status(thm)`. `math:verifiedByEngine`'s own definition is the rule
//!   being followed: "'verified' always answers BY WHOM, so a bare 'verified' with no engine
//!   is not a claim but a decoration."
//! - A conclusion clause is NOT typed `math:denotationKind math:denotesProposition`. A
//!   clause expression is content-addressed, so the same node may also stand in argument
//!   position inside another clause; asserting proposition-hood on a shared node would state
//!   it of a use the source never made. The proof term has no such ambiguity — its arena key
//!   is rule-headed — so `math:denotesProof` IS asserted there.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_term_arena::{Arena, ContentKey, StructNode, TermArena};
use purrdf::TermValue;

use crate::error::ProofUnliftable;
use crate::frame::{BridgeKind, Lifted, RunFrame, Rung};
use crate::ns::{gmeow, logic, math};
use crate::proof::tstp::{self, Clause, Derivation, Literal, Step, Term, render_atom};
use crate::sink::Sink;

/// `rdfs:label`.
///
/// The one non-`math:`/`logic:`/`gmeow:` term this lift needs, and it is load-bearing rather
/// than decorative: a step's TSTP NAME and an operator's RAW TOKEN are two of the four facts
/// the section/retraction claim rests on, and neither has a `math:` property of its own. The
/// literal is PLAIN — [`Sink`] exposes no language-tagged constructor, because lifted graphs
/// leave through the shipped CLI where no `x-gmeow-*` private-use tag may appear.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// Lift a TSTP derivation into `math:` structures.
///
/// `mint_base` must end in `/` or `#`; every codomain IRI is minted beneath the run it
/// names, so a re-lift of the same bytes under the same base is byte-identical.
///
/// # Errors
///
/// - [`SourceNotUtf8`](crate::error::SourceNotUtf8) when `source` is not UTF-8.
/// - [`TstpParse`](crate::error::TstpParse), with a line and column, for malformed TSTP.
/// - [`ProofUnliftable`] when the document parses but carries no liftable proof: a construct
///   the reader does not structure, a dangling parent, a cycle, no derived step, or more
///   than one conclusion. See [`super::tstp`] for the complete list.
pub fn lift(source: &[u8], mint_base: &str) -> gmeow_errors::Result<Lifted> {
    let derivation = tstp::parse(source)?;

    let frame = RunFrame::mint(BridgeKind::Proof, mint_base, source);
    let mut sink = Sink::new();
    frame.emit(&mut sink, Rung::section_retraction());

    let mut lift = Lift {
        frame: &frame,
        sink,
        arena: TermArena::new(),
        emitted: BTreeSet::new(),
        step_node: BTreeMap::new(),
        clause_iri: BTreeMap::new(),
        proof_term: BTreeMap::new(),
        proof_structures: 0,
    };
    lift.derivation(&derivation);

    if lift.proof_structures == 0 {
        return Err(unliftable(format!(
            "the derivation of `{}` produced no proof-layer structure for the math: codomain: \
             no step, no axiom, and no conclusion expression. A run whose only product is the \
             verification triangle is an unliftable ingest, not a lift",
            derivation.conclusion().name
        )));
    }

    let codomain = lift.emitted.len();
    Lifted::seal(&frame, lift.sink, codomain)
}

// ── Lift state ────────────────────────────────────────────────────────────────

struct Lift<'f> {
    frame: &'f RunFrame,
    sink: Sink,
    arena: TermArena,
    emitted: BTreeSet<String>,
    /// Step name → the proof-layer node standing for it: a `math:ProofStep` for a derived
    /// step, a `math:Axiom` for an asserted leaf.
    step_node: BTreeMap<String, String>,
    /// Step name → the expression node its conclusion clause interns to.
    clause_iri: BTreeMap<String, String>,
    /// Step name → the arena node of the step's PROOF TERM. A derived step's is
    /// `rule(parents…)`; an asserted leaf's is the clause it states, because the proof of an
    /// axiom is the axiom.
    proof_term: BTreeMap<String, StructNode>,
    /// How many genuinely PROOF-layer structures the run produced — steps, axioms, and the
    /// expression ASTs they carry.
    ///
    /// Separate from `emitted.len()`, which also counts the goal/situation teleology and the
    /// verification triangle. Without it a run whose only product was the QED claim would
    /// seal: the proof bridge's job is the derivation, so the derivation is what the gate
    /// counts.
    proof_structures: usize,
}

impl Lift<'_> {
    /// Mint (and back-link) a codomain node, reporting whether it is new.
    ///
    /// The back edge `gmeow:wasGeneratedBy` is what the native `math:UnliftableIngest` lint
    /// enumerates, so it is attached HERE, once, for every node this lift creates.
    fn mint(&mut self, role: &str, key: &str) -> (String, bool) {
        let iri = self.frame.node(role, key);
        let fresh = self.emitted.insert(iri.clone());
        if fresh {
            self.frame.generated(&mut self.sink, &iri);
        }
        (iri, fresh)
    }

    fn label(&mut self, subject: &str, text: &str) {
        self.sink.string(subject, RDFS_LABEL, text);
    }

    fn key_of(&self, node: StructNode) -> ContentKey {
        self.arena
            .key(node)
            .expect("every node was minted by this lift's own arena")
    }

    fn atom(&mut self, text: &str) -> StructNode {
        self.arena.intern_leaf(TermValue::simple_literal(text))
    }

    fn app(&mut self, head: &str, args: &[StructNode]) -> StructNode {
        let head = self.atom(head);
        self.arena
            .intern_app(head, args)
            .expect("every node was minted by this lift's own arena")
    }

    /// Mint an expression's IRI from its CONTENT, never its position.
    fn expression(&mut self, node: StructNode) -> (String, bool) {
        let key = self.key_of(node).into_string();
        self.mint("expr", &key)
    }

    // -- the whole derivation -------------------------------------------------

    fn derivation(&mut self, derivation: &Derivation) {
        // Dependency order, not source order: a derived step's proof term is built from its
        // parents', and TSTP does not require a step to be written after the steps it cites.
        for index in derivation.dependency_order() {
            self.step(derivation, &derivation.steps()[index]);
        }

        let key = derivation.render();
        let conclusion = derivation.conclusion();
        let rendered = conclusion.conclusion.render();

        let (dag, _) = self.mint("dag", &key);
        self.sink.typed(&dag, &math("ProofDependencyGraph"));
        self.label(
            &dag,
            &format!(
                "TSTP proof dependency DAG: {} steps, {} inferences",
                derivation.steps().len(),
                derivation.steps().iter().filter(|s| s.is_derived()).count()
            ),
        );

        let (proof, _) = self.mint("proof", &key);
        self.sink.typed(&proof, &math("Proof"));
        self.label(&proof, &format!("TSTP derivation of {rendered}"));

        // The proof discharges the goal its terminal step reaches, and decomposes into the
        // derived steps and the axioms they rest on.
        let goal = self.emit_goal(&rendered);
        self.sink.iri(&proof, &math("provesGoal"), &goal);
        let statement = self.clause_iri[&conclusion.name].clone();
        self.sink.iri(&proof, &math("provesStatement"), &statement);
        for step in derivation.steps() {
            let node = self.step_node[&step.name].clone();
            if step.is_derived() {
                self.sink.iri(&proof, &math("proofStep"), &node);
            } else {
                self.sink.iri(&proof, &math("dependsOnAxiom"), &node);
            }
        }

        self.emit_verification(&key, &proof, &rendered);
    }

    // -- one step --------------------------------------------------------------

    fn step(&mut self, derivation: &Derivation, step: &Step) {
        let (clause_node, clause_iri) = self.emit_clause(&step.conclusion);
        self.clause_iri
            .insert(step.name.clone(), clause_iri.clone());

        let Some(rule) = step.rule.clone() else {
            // An asserted leaf: the derivation's stated law. math:Axiom's own definition
            // rejects the amnesic reading ("never an opaque string"), so the axiom names the
            // clause AST it states rather than a rendering of it.
            let (axiom, _) = self.mint("axiom", &step.name);
            self.sink.typed(&axiom, &math("Axiom"));
            self.label(&axiom, &step.name);
            self.sink
                .iri(&axiom, &math("formalizesExpression"), &clause_iri);
            self.step_node.insert(step.name.clone(), axiom);
            self.proof_term.insert(step.name.clone(), clause_node);
            self.proof_structures += 1;
            return;
        };

        // The proof term: rule(parent-proof-terms…). Content-addressed over the WHOLE
        // sub-derivation, so two steps reached by an identical sub-proof share one term.
        let parents: Vec<StructNode> = step
            .parents
            .iter()
            .map(|parent| self.proof_term[parent])
            .collect();
        let term_node = self.app(&format!("tstp:rule:{rule}"), &parents);
        let (term_iri, fresh) = self.expression(term_node);
        if fresh {
            self.sink.typed(&term_iri, &math("ApplicationExpression"));
            let operation = self.emit_operation("rule", &rule, &rule);
            self.sink.iri(&term_iri, &math("operator"), &operation);
            // The kind the slice declares for exactly this case: an expression standing for
            // a proof-term, carried as structured content rather than as prose.
            self.sink
                .iri(&term_iri, &math("denotationKind"), &math("denotesProof"));
            let operands: Vec<String> = parents
                .iter()
                .map(|&node| {
                    let key = self.key_of(node).into_string();
                    self.frame.node("expr", &key)
                })
                .collect();
            self.emit_slots(&term_iri, &operands);
            self.proof_structures += 1;
        }
        self.proof_term.insert(step.name.clone(), term_node);

        let (node, _) = self.mint("step", &step.name);
        self.sink.typed(&node, &math("ProofStep"));
        // The step's NAME — the identity a parent list cites, and one of the four facts the
        // section/retraction claim rests on.
        self.label(&node, &step.name);
        self.sink.iri(&node, &math("hasConclusion"), &clause_iri);
        self.sink
            .iri(&node, &math("formalizesExpression"), &term_iri);
        for parent in &step.parents {
            let parent_node = self.step_node[parent].clone();
            self.sink.iri(&node, &math("hasPremise"), &parent_node);
            // An asserted parent is a foundation the step rests on, which is precisely what
            // a proof dependency DAG records.
            if derivation.step(parent).is_some_and(|p| !p.is_derived()) {
                self.sink.iri(&node, &math("dependsOnAxiom"), &parent_node);
            }
        }
        let goal = self.emit_goal(&step.conclusion.render());
        self.sink.iri(&node, &math("provesGoal"), &goal);
        self.step_node.insert(step.name.clone(), node);
        self.proof_structures += 1;
    }

    // -- the teleology seam ----------------------------------------------------

    /// The `logic:GoalExpression` sub-goal a step discharges.
    ///
    /// Keyed on the RENDERED CONCLUSION rather than on the step, because
    /// `logic:GoalExpression`'s identity is structural by its own definition: "two
    /// expressions with the same node kind, operands, and bound situation type are the same
    /// term". Two steps reaching the same conclusion reach the same goal.
    ///
    /// The kind is `logic:AchievementGoal` — the target FIRST obtains along the path, which
    /// is exactly what deriving a conclusion is, and is the kind `math:provesGoal`'s own
    /// definition names. It is a VALUE carried by `logic:goalExpressionKind`, never a class
    /// the goal is typed with.
    fn emit_goal(&mut self, rendered: &str) -> String {
        let (goal, fresh) = self.mint("goal", rendered);
        if fresh {
            self.sink.typed(&goal, &logic("GoalExpression"));
            self.sink.iri(
                &goal,
                &logic("goalExpressionKind"),
                &logic("AchievementGoal"),
            );
            self.label(&goal, &format!("derive {rendered}"));
            let (situation, _) = self.mint("situation", rendered);
            self.sink.typed(&situation, &logic("Situation"));
            self.label(&situation, &format!("{rendered} is derived"));
            self.sink
                .iri(&goal, &logic("boundSituationType"), &situation);
        }
        goal
    }

    // -- the QED triangle ------------------------------------------------------

    /// The process / result / claim separation, discharged.
    ///
    /// `math:FormalVerificationResult` carries
    /// `gmeow:enforcesFailureClass math:UngroundedVerificationResult`, so the grounding
    /// observation is MANDATORY: a result stated without a vantage-held claim is ill-formed,
    /// not merely under-decorated. The three nodes stay distinct — the run is the process
    /// (`gmeow:wasGeneratedBy`), the result object carries the verdict, and the
    /// `gmeow:Observation` holds it from the checker's `gmeow:vantage`.
    fn emit_verification(&mut self, key: &str, proof: &str, rendered: &str) {
        let (standpoint, fresh) = self.mint("checker", "tstp-structural-checker");
        if fresh {
            self.sink.typed(&standpoint, &gmeow("Standpoint"));
            self.label(
                &standpoint,
                "the gmeow-math-lift TSTP derivation checker: it accepts a derivation whose \
                 dependency graph is well-founded (every cited parent introduced, no cycle, one \
                 terminal conclusion) and whose every inference declares status(thm); it does \
                 NOT re-derive the inferences",
            );
        }

        let (result, _) = self.mint("verification", key);
        self.sink.typed(&result, &math("FormalVerificationResult"));
        self.label(
            &result,
            &format!("{rendered}: accepted as a well-founded TSTP derivation"),
        );
        self.sink.iri(
            &result,
            &math("verificationResult"),
            &math("verificationPassed"),
        );
        // "Verified" always answers BY WHOM: the engine named on the result object is the
        // same standpoint the grounding observation holds the verdict from.
        self.sink
            .iri(&result, &math("verifiedByEngine"), &standpoint);

        let (observation, _) = self.mint("observation", key);
        self.sink.typed(&observation, &gmeow("Observation"));
        self.sink
            .iri(&observation, &gmeow("observedFeature"), proof);
        self.sink
            .iri(&observation, &gmeow("observationResult"), &result);
        self.sink.iri(&observation, &gmeow("vantage"), &standpoint);
    }

    // -- expressions -----------------------------------------------------------

    /// A conclusion clause, as a `math:MathematicalExpression` AST.
    fn emit_clause(&mut self, clause: &Clause) -> (StructNode, String) {
        let [single] = clause.literals.as_slice() else {
            let parts: Vec<(StructNode, String)> = clause
                .literals
                .iter()
                .map(|literal| self.emit_literal(literal))
                .collect();
            let nodes: Vec<StructNode> = parts.iter().map(|(node, _)| *node).collect();
            let node = self.app("tstp:connective:or", &nodes);
            let (iri, fresh) = self.expression(node);
            if fresh {
                self.sink.typed(&iri, &math("ApplicationExpression"));
                let operation = self.emit_operation("connective", "or", "logical disjunction (|)");
                self.sink.iri(&iri, &math("operator"), &operation);
                let operands: Vec<String> = parts.iter().map(|(_, iri)| iri.clone()).collect();
                self.emit_slots(&iri, &operands);
                self.label(&iri, &clause.render());
                self.proof_structures += 1;
            }
            return (node, iri);
        };
        self.emit_literal(single)
    }

    fn emit_literal(&mut self, literal: &Literal) -> (StructNode, String) {
        let (atom, atom_iri) = self.emit_term(&literal.atom);
        if !literal.negated {
            return (atom, atom_iri);
        }
        let node = self.app("tstp:connective:not", &[atom]);
        let (iri, fresh) = self.expression(node);
        if fresh {
            self.sink.typed(&iri, &math("ApplicationExpression"));
            let operation = self.emit_operation("connective", "not", "logical negation (~)");
            self.sink.iri(&iri, &math("operator"), &operation);
            self.emit_slots(&iri, &[atom_iri]);
            self.label(&iri, &literal.render());
            self.proof_structures += 1;
        }
        (node, iri)
    }

    fn emit_term(&mut self, term: &Term) -> (StructNode, String) {
        match term {
            Term::Variable(name) => self.emit_variable(name),
            Term::Apply { functor, args } if args.is_empty() => self.emit_constant(functor),
            Term::Apply { functor, args } => {
                let parts: Vec<(StructNode, String)> =
                    args.iter().map(|arg| self.emit_term(arg)).collect();
                let nodes: Vec<StructNode> = parts.iter().map(|(node, _)| *node).collect();
                let node = self.app(&format!("tstp:sym:{functor}"), &nodes);
                let (iri, fresh) = self.expression(node);
                if fresh {
                    self.sink.typed(&iri, &math("ApplicationExpression"));
                    let operation = self.emit_operation("functor", functor, functor);
                    self.sink.iri(&iri, &math("operator"), &operation);
                    let operands: Vec<String> = parts.iter().map(|(_, iri)| iri.clone()).collect();
                    self.emit_slots(&iri, &operands);
                    self.label(&iri, &term.render());
                    self.proof_structures += 1;
                }
                (node, iri)
            }
        }
    }

    /// A nullary functor — a constant, or a defined atom such as the empty clause `$false`.
    ///
    /// A `math:SymbolReference` rather than a zero-operand application: the slice's own
    /// reading is that a symbol-occurrence leaf "resolves through exactly one
    /// math:hasMathematicalSymbol edge to a local math:MathematicalSymbol", which is what a
    /// TPTP constant is. Emitting it as an application with no operands would claim a
    /// computation the source does not state.
    fn emit_constant(&mut self, functor: &str) -> (StructNode, String) {
        let node = self.atom(&format!("tstp:sym:{functor}"));
        let (iri, fresh) = self.expression(node);
        if fresh {
            self.sink.typed(&iri, &math("SymbolReference"));
            let (symbol, symbol_fresh) = self.mint("symbol", functor);
            if symbol_fresh {
                self.sink.typed(&symbol, &math("MathematicalSymbol"));
                self.label(&symbol, functor);
            }
            self.sink.iri(&iri, &math("hasMathematicalSymbol"), &symbol);
            self.label(&iri, &render_atom(functor));
            self.proof_structures += 1;
        }
        (node, iri)
    }

    /// A clause variable.
    ///
    /// Modelled exactly as the R and ONNX bridges model one — a `math:VariableExpression`
    /// over a `math:VariableOccurrence` resolving to a `math:FreeVariableDeclaration` —
    /// because `math:VariableExpression`'s own definition insists "there is no implicit free
    /// variable": an occurrence resolving to no declaration is
    /// `math:UnscopedVariableOccurrence`. The declaration is FREE and not a binder: a CNF
    /// clause's universal closure is implicit in the source, and minting a
    /// `math:BindingExpression` for a binder the derivation never writes would state a scope
    /// it does not.
    fn emit_variable(&mut self, name: &str) -> (StructNode, String) {
        let node = self
            .arena
            .intern_free(TermValue::simple_literal(format!("tstp:var:{name}")));
        let (iri, fresh) = self.expression(node);
        if fresh {
            let key = self.key_of(node).into_string();
            self.sink.typed(&iri, &math("VariableExpression"));
            self.label(&iri, name);
            let (occurrence, _) = self.mint("occurrence", &key);
            self.sink.typed(&occurrence, &math("VariableOccurrence"));
            let (declaration, _) = self.mint("declaration", &key);
            self.sink
                .typed(&declaration, &math("FreeVariableDeclaration"));
            self.label(&declaration, name);
            self.sink
                .iri(&occurrence, &math("declaredVariable"), &declaration);
            self.sink
                .iri(&iri, &math("variableOccurrence"), &occurrence);
            self.proof_structures += 1;
        }
        (node, iri)
    }

    /// The `math:Operation` an application applies.
    ///
    /// The label is the operator's RAW token — an inference rule's content-addressed firing
    /// IRI, or a predicate's own symbol — because that token is what a reconstruction of the
    /// derivation has to recover verbatim. `role` keeps the three operator families apart,
    /// so a rule and a predicate that happen to share a spelling are never one individual.
    fn emit_operation(&mut self, role: &str, token: &str, label: &str) -> String {
        let (iri, fresh) = self.mint("operation", &format!("{role}|{token}"));
        if fresh {
            self.sink.typed(&iri, &math("Operation"));
            self.label(&iri, label);
        }
        iri
    }

    /// Ordered, contiguous, zero-based `math:ArgumentSlot` cells over an application.
    ///
    /// Contiguity is an OWL obligation, not a convention: a gap is
    /// `math:NonContiguousArgumentSlots`. The slots are also what preserve TSTP's operand
    /// ORDER, which the premise EDGES (a set) do not.
    fn emit_slots(&mut self, application: &str, operands: &[String]) {
        for (index, operand) in operands.iter().enumerate() {
            let (slot, _) = self.mint("slot", &format!("{application}#{index}"));
            self.sink.typed(&slot, &math("ArgumentSlot"));
            let position = i64::try_from(index).unwrap_or(i64::MAX);
            self.sink.integer(&slot, &math("slotIndex"), position);
            self.sink.iri(&slot, &math("slotExpression"), operand);
            self.sink.iri(application, &math("argumentSlot"), &slot);
        }
    }
}

fn unliftable(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(ProofUnliftable { detail })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TstpParse;
    use crate::proof::tstp::Role;

    const BASE: &str = "https://blackcatinformatics.ca/gmeow/examples/math/lift/";

    /// The flagship fixture: a derivation OUR OWN reasoner produced, byte-pinned against
    /// `ProofTree::to_tstp` by `gmeow_conformance::external::tptp::lower_fol`.
    const FIXTURE: &[u8] = include_bytes!("../../fixtures/theorem-subclass.tstp");

    /// A synthetic derivation exercising what the fixture does not: several parents in one
    /// inference, a disjunctive and a negated conclusion, a variable, a nested term, a
    /// repeated sub-term, and a shared sub-proof.
    const RICH: &[u8] = b"cnf(ax_p, axiom, p(f(a), X)).\n\
                          cnf(ax_q, axiom, q(f(a))).\n\
                          cnf(d_left, plain, ( ~r(f(a)) | s(b) ), \
                              inference(res, [status(thm)], [ax_p, ax_q])).\n\
                          cnf(d_right, plain, t(b), inference(res, [status(thm)], [ax_q])).\n\
                          cnf(d_top, plain, $false, \
                              inference(unit, [status(thm)], [d_left, d_right])).\n";

    const RDF_TYPE_LINE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    const LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

    fn turtle(source: &[u8]) -> String {
        lift(source, BASE)
            .unwrap_or_else(|e| panic!("the derivation must lift: {e}"))
            .turtle
    }

    fn count(ttl: &str, predicate: &str) -> usize {
        ttl.matches(&format!("<{predicate}>")).count()
    }

    /// How many subjects the graph types as `math:{class}`.
    ///
    /// Exact rather than substring: `math:Proof` must not be counted by `math:ProofStep`,
    /// nor `math:Axiom` by anything sharing the word.
    fn typed(ttl: &str, class: &str) -> usize {
        typed_as(ttl, &math(class))
    }

    fn typed_as(ttl: &str, class: &str) -> usize {
        let suffix = format!("{RDF_TYPE_LINE} <{class}> .");
        ttl.lines().filter(|line| line.ends_with(&suffix)).count()
    }

    // -- a tiny reader over the emitted Turtle ---------------------------------

    /// One triple of the canonical, one-triple-per-line Turtle the [`Sink`] serializes.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct Triple {
        subject: String,
        predicate: String,
        object: String,
        literal: bool,
    }

    /// Read the emitted graph back as triples — the ONLY channel the reconstruction below
    /// is allowed to use. No lift state, no parser, just the bytes a consumer receives.
    fn triples(ttl: &str) -> Vec<Triple> {
        let mut out = Vec::new();
        for line in ttl.lines() {
            if line.is_empty() || line.starts_with('@') {
                continue;
            }
            let rest = line
                .strip_suffix(" .")
                .unwrap_or_else(|| panic!("a Turtle line ends in ` .`: {line}"));
            let (subject, rest) = rest.split_once(' ').expect("subject then predicate");
            let (predicate, object) = rest.split_once(' ').expect("predicate then object");
            let literal = object.starts_with('"');
            out.push(Triple {
                subject: unwrap_iri(subject),
                predicate: unwrap_iri(predicate),
                object: if literal {
                    unwrap_literal(object)
                } else {
                    unwrap_iri(object)
                },
                literal,
            });
        }
        out
    }

    fn unwrap_iri(term: &str) -> String {
        term.strip_prefix('<')
            .and_then(|t| t.strip_suffix('>'))
            .unwrap_or_else(|| panic!("an IRI term is angle-bracketed: {term}"))
            .to_owned()
    }

    /// The lexical form of a Turtle literal, with the escapes the codec writes undone.
    fn unwrap_literal(term: &str) -> String {
        let body = term
            .strip_prefix('"')
            .expect("a literal starts with a quote");
        let end = {
            let bytes = body.as_bytes();
            let mut cursor = 0;
            loop {
                assert!(cursor < bytes.len(), "an unterminated literal: {term}");
                match bytes[cursor] {
                    b'\\' => cursor += 2,
                    b'"' => break cursor,
                    _ => cursor += 1,
                }
            }
        };
        let mut out = String::with_capacity(end);
        let mut chars = body[..end].chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => panic!("a dangling escape in {term}"),
            }
        }
        out
    }

    /// A tiny read-only index over the emitted graph.
    struct Graph {
        triples: Vec<Triple>,
    }

    impl Graph {
        fn of(ttl: &str) -> Self {
            Self {
                triples: triples(ttl),
            }
        }

        fn objects(&self, subject: &str, predicate: &str) -> Vec<String> {
            self.triples
                .iter()
                .filter(|t| t.subject == subject && t.predicate == predicate)
                .map(|t| t.object.clone())
                .collect()
        }

        fn object(&self, subject: &str, predicate: &str) -> String {
            let found = self.objects(subject, predicate);
            let [one] = found.as_slice() else {
                panic!("expected exactly one <{predicate}> on <{subject}>, found {found:?}");
            };
            one.clone()
        }

        fn label(&self, subject: &str) -> String {
            self.object(subject, LABEL)
        }

        fn subjects_typed(&self, class: &str) -> BTreeSet<String> {
            self.triples
                .iter()
                .filter(|t| t.predicate == crate::ns::RDF_TYPE && t.object == class)
                .map(|t| t.subject.clone())
                .collect()
        }
    }

    /// One derivation step, as rebuilt from the emitted graph ALONE.
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct Rebuilt {
        name: String,
        derived: bool,
        rule: Option<String>,
        parents: BTreeSet<String>,
        conclusion: String,
    }

    /// Reconstruct the derivation from the lifted Turtle — the section/retraction claim,
    /// executed.
    ///
    /// Reads nothing but the graph: derived steps are the `math:ProofStep` subjects, their
    /// names their `rdfs:label`s, their rules the `rdfs:label` of the `math:operator` of the
    /// proof term reached by `math:formalizesExpression`, their parents the `rdfs:label`s of
    /// their `math:hasPremise` targets, and their conclusions the `rdfs:label` of the
    /// expression reached by `math:hasConclusion`. Asserted leaves are the `math:Axiom`
    /// subjects, reached the same way through `math:formalizesExpression`.
    fn reconstruct(ttl: &str) -> BTreeSet<Rebuilt> {
        let graph = Graph::of(ttl);
        let mut out = BTreeSet::new();
        for step in graph.subjects_typed(&math("ProofStep")) {
            let term = graph.object(&step, &math("formalizesExpression"));
            let operation = graph.object(&term, &math("operator"));
            out.insert(Rebuilt {
                name: graph.label(&step),
                derived: true,
                rule: Some(graph.label(&operation)),
                parents: graph
                    .objects(&step, &math("hasPremise"))
                    .iter()
                    .map(|parent| graph.label(parent))
                    .collect(),
                conclusion: graph.label(&graph.object(&step, &math("hasConclusion"))),
            });
        }
        for axiom in graph.subjects_typed(&math("Axiom")) {
            out.insert(Rebuilt {
                name: graph.label(&axiom),
                derived: false,
                rule: None,
                parents: BTreeSet::new(),
                conclusion: graph.label(&graph.object(&axiom, &math("formalizesExpression"))),
            });
        }
        out
    }

    /// The same view, taken from the PARSE rather than from the graph.
    fn expected(source: &[u8]) -> BTreeSet<Rebuilt> {
        tstp::parse(source)
            .expect("the fixture parses")
            .steps()
            .iter()
            .map(|step| Rebuilt {
                name: step.name.clone(),
                derived: step.is_derived(),
                rule: step.rule.clone(),
                parents: step.parents.iter().cloned().collect(),
                conclusion: step.conclusion.render(),
            })
            .collect()
    }

    // -- THE RUNG: the round-trip that earns the section/retraction claim ------

    #[test]
    fn the_lift_is_a_section_the_derivation_reconstructs_from_the_graph_alone() {
        // `Rung::section_retraction`'s own doc: "It is only honest if the lift carries every
        // step name, inference rule, parent edge, and rendered conclusion — i.e. if the
        // derivation genuinely reconstructs. The proof bridge owes a round-trip test for
        // that claim; without one this constructor must not be used." This is that test.
        for source in [FIXTURE, RICH] {
            let rebuilt = reconstruct(&turtle(source));
            assert_eq!(
                rebuilt,
                expected(source),
                "the derivation did not reconstruct from the lifted graph"
            );
        }
    }

    #[test]
    fn the_reconstruction_would_notice_a_dropped_step_or_a_wrong_rule() {
        // A round-trip test is only evidence if it can FAIL, so pin that the comparison is
        // sensitive to each of the four facts the rung rests on.
        let mut rebuilt = reconstruct(&turtle(RICH));
        let full = rebuilt.clone();
        assert_eq!(full.len(), 5, "five steps: 2 axioms + 3 inferences");

        let victim = full.iter().find(|s| s.derived).expect("a derived step");
        rebuilt.remove(victim);
        assert_ne!(rebuilt, full, "a dropped step must change the rebuild");

        let mut altered = victim.clone();
        altered.rule = Some("a-different-rule".to_owned());
        rebuilt.insert(altered);
        assert_ne!(rebuilt, full, "a changed rule must change the rebuild");
    }

    #[test]
    fn the_rebuilt_conclusion_is_the_exact_tstp_surface_of_the_source() {
        let graph = Graph::of(&turtle(FIXTURE));
        let steps = graph.subjects_typed(&math("ProofStep"));
        let conclusions: BTreeSet<String> = steps
            .iter()
            .map(|step| graph.label(&graph.object(step, &math("hasConclusion"))))
            .collect();
        assert!(
            conclusions.contains(
                "'https://blackcatinformatics.ca/gmeow/tptp#c'\
                 ('https://blackcatinformatics.ca/logic/entail/reserved#witness-\
                 d4a1e02579180296')"
            ),
            "the rendered conclusion keeps the full IRIs unshortened: {conclusions:?}"
        );
    }

    // -- the shape bridges.ttl pins -------------------------------------------

    #[test]
    fn the_committed_fixture_lifts_every_expected_codomain_class() {
        let lifted = lift(FIXTURE, BASE).expect("the flagship fixture lifts");
        let ttl = &lifted.turtle;
        for (class, expected) in [
            ("ProofIngestRun", 1),
            ("ProofDependencyGraph", 1),
            ("Proof", 1),
            ("ProofStep", 2),
            ("Axiom", 1),
            ("FormalVerificationResult", 1),
            ("MathematicalObject", 1),
        ] {
            assert_eq!(typed(ttl, class), expected, "math:{class} count:\n{ttl}");
        }
        for class in [
            "ApplicationExpression",
            "SymbolReference",
            "MathematicalSymbol",
            "Operation",
            "ArgumentSlot",
        ] {
            assert!(
                typed(ttl, class) > 0,
                "the fixture must produce math:{class}"
            );
        }
        for (class, expected) in [("GoalExpression", 2), ("Situation", 2)] {
            assert_eq!(
                typed_as(ttl, &logic(class)),
                expected,
                "one sub-goal per derived step: logic:{class}"
            );
        }
        assert_eq!(typed_as(ttl, &gmeow("Observation")), 1);
        assert_eq!(typed_as(ttl, &gmeow("Standpoint")), 1);
        assert!(lifted.run_iri.contains("proof-run-"));
        assert!(lifted.codomain_nodes > 15, "a real derivation is dense");
    }

    #[test]
    fn the_proof_decomposes_into_its_steps_its_axioms_and_its_goal() {
        let ttl = turtle(FIXTURE);
        let graph = Graph::of(&ttl);
        let proof = graph
            .subjects_typed(&math("Proof"))
            .iter()
            .next()
            .expect("one math:Proof")
            .clone();

        assert_eq!(
            graph.objects(&proof, &math("proofStep")).len(),
            2,
            "one math:proofStep per DERIVED step, and never one per axiom"
        );
        assert_eq!(
            graph.objects(&proof, &math("dependsOnAxiom")).len(),
            1,
            "the asserted leaf is a foundation the proof depends on"
        );
        let goal = graph.object(&proof, &math("provesGoal"));
        assert_eq!(
            graph.object(&goal, &logic("goalExpressionKind")),
            logic("AchievementGoal"),
            "the kind is a VALUE on the property, never a class the goal is typed with"
        );
        let situation = graph.object(&goal, &logic("boundSituationType"));
        assert!(
            graph
                .subjects_typed(&logic("Situation"))
                .contains(&situation),
            "the goal's bound situation type is a logic:Situation"
        );
        // The proof also names what it establishes: the terminal step's conclusion AST.
        let statement = graph.object(&proof, &math("provesStatement"));
        assert!(
            graph.label(&statement).contains("tptp#c"),
            "the proof establishes its terminal conclusion"
        );
    }

    #[test]
    fn each_step_carries_its_premises_and_the_axioms_beneath_it() {
        let ttl = turtle(FIXTURE);
        let graph = Graph::of(&ttl);
        let axiom = graph
            .subjects_typed(&math("Axiom"))
            .iter()
            .next()
            .expect("one axiom")
            .clone();
        let citing: Vec<&Triple> = graph
            .triples
            .iter()
            .filter(|t| t.predicate == math("dependsOnAxiom") && t.object == axiom)
            .collect();
        assert_eq!(
            citing.len(),
            2,
            "the axiom is a foundation of BOTH the step that cites it and the proof: {citing:?}"
        );
        assert_eq!(
            count(&ttl, &math("hasPremise")),
            2,
            "one premise edge per cited parent"
        );
    }

    #[test]
    fn the_qed_is_a_result_object_held_by_an_observation_from_a_named_vantage() {
        // math:FormalVerificationResult carries
        // gmeow:enforcesFailureClass math:UngroundedVerificationResult, so the grounding
        // observation is mandatory rather than decorative.
        let ttl = turtle(FIXTURE);
        let graph = Graph::of(&ttl);
        let result = graph
            .subjects_typed(&math("FormalVerificationResult"))
            .iter()
            .next()
            .expect("one result object")
            .clone();
        let observation = graph
            .subjects_typed(&gmeow("Observation"))
            .iter()
            .next()
            .expect("one observation")
            .clone();
        let proof = graph
            .subjects_typed(&math("Proof"))
            .iter()
            .next()
            .expect("one proof")
            .clone();

        assert_eq!(
            graph.object(&observation, &gmeow("observationResult")),
            result
        );
        assert_eq!(graph.object(&observation, &gmeow("observedFeature")), proof);
        let vantage = graph.object(&observation, &gmeow("vantage"));
        assert!(
            graph
                .subjects_typed(&gmeow("Standpoint"))
                .contains(&vantage),
            "the vantage is a gmeow:Standpoint"
        );
        assert_eq!(
            graph.object(&result, &math("verifiedByEngine")),
            vantage,
            "'verified' answers BY WHOM: the engine on the result IS the observation's vantage"
        );
        assert_eq!(
            graph.object(&result, &math("verificationResult")),
            math("verificationPassed")
        );
        // The three roles stay distinct: no node is typed as two of process / result /
        // claim (math:ResultRoleConflation, math:ProcessObservationConflation).
        assert_ne!(result, observation);
        assert!(
            !graph
                .subjects_typed(&gmeow("Observation"))
                .contains(&result),
            "a result object roled as its own claim is ill-formed"
        );
        assert_eq!(
            typed(&ttl, "ProofCheckActivity"),
            0,
            "this lift ran no proof-assistant check, so it claims no math:ProofCheckActivity"
        );
    }

    #[test]
    fn the_verification_claim_says_exactly_what_the_checker_did_not_do() {
        let ttl = turtle(FIXTURE);
        let graph = Graph::of(&ttl);
        let standpoint = graph
            .subjects_typed(&gmeow("Standpoint"))
            .iter()
            .next()
            .expect("a standpoint")
            .clone();
        let label = graph.label(&standpoint);
        assert!(label.contains("well-founded"), "{label}");
        assert!(label.contains("status(thm)"), "{label}");
        assert!(
            label.contains("does NOT re-derive the inferences"),
            "the vantage must not imply a soundness check it never ran: {label}"
        );
    }

    #[test]
    fn the_dependency_graph_is_emitted_with_exactly_the_shape_the_fixture_pins() {
        // math:ProofDependencyGraph has no declared property naming the math:Proof it
        // underlies, so the lift emits the type, a label, and the generation back-edge —
        // the same three the shipped fixtures carry — and mints no term of its own.
        let ttl = turtle(FIXTURE);
        let graph = Graph::of(&ttl);
        let dag = graph
            .subjects_typed(&math("ProofDependencyGraph"))
            .iter()
            .next()
            .expect("one DAG")
            .clone();
        let predicates: BTreeSet<String> = graph
            .triples
            .iter()
            .filter(|t| t.subject == dag)
            .map(|t| t.predicate.clone())
            .collect();
        assert_eq!(
            predicates,
            BTreeSet::from([
                crate::ns::RDF_TYPE.to_owned(),
                LABEL.to_owned(),
                gmeow("wasGeneratedBy"),
            ])
        );
        assert!(graph.label(&dag).contains("3 steps, 2 inferences"));
    }

    // -- the competency questions ---------------------------------------------

    #[test]
    fn the_graph_answers_the_proof_dependency_and_subgoal_competency_question() {
        // The BGP of `slices/grounding/math/queries/competency/
        // proof-dependency-graph-and-subgoals.rq`, walked by hand: ?run a
        // math:ProofIngestRun; ?dag wasGeneratedBy ?run, a math:ProofDependencyGraph;
        // ?proof wasGeneratedBy ?run, a math:Proof, math:provesGoal ?goal, math:proofStep
        // ?step.
        let ttl = turtle(FIXTURE);
        let graph = Graph::of(&ttl);
        let mut rows = 0;
        for run in graph.subjects_typed(&math("ProofIngestRun")) {
            for dag in graph.subjects_typed(&math("ProofDependencyGraph")) {
                if !graph.objects(&dag, &gmeow("wasGeneratedBy")).contains(&run) {
                    continue;
                }
                for proof in graph.subjects_typed(&math("Proof")) {
                    if !graph
                        .objects(&proof, &gmeow("wasGeneratedBy"))
                        .contains(&run)
                    {
                        continue;
                    }
                    for _goal in graph.objects(&proof, &math("provesGoal")) {
                        for _step in graph.objects(&proof, &math("proofStep")) {
                            rows += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(
            rows, 2,
            "one row per (goal, step) pair — the join must exist"
        );
    }

    #[test]
    fn the_graph_answers_the_bridge_results_as_observations_competency_question() {
        // The BGP of `bridge-results-as-observations.rq`: ?obs a gmeow:Observation,
        // gmeow:observationResult ?result, gmeow:vantage ?vantage, gmeow:wasGeneratedBy
        // ?activity; ?activity a math:ProofIngestRun.
        let ttl = turtle(FIXTURE);
        let graph = Graph::of(&ttl);
        let runs = graph.subjects_typed(&math("ProofIngestRun"));
        let mut rows = 0;
        for observation in graph.subjects_typed(&gmeow("Observation")) {
            let _result = graph.object(&observation, &gmeow("observationResult"));
            let _vantage = graph.object(&observation, &gmeow("vantage"));
            for activity in graph.objects(&observation, &gmeow("wasGeneratedBy")) {
                if runs.contains(&activity) {
                    rows += 1;
                }
            }
        }
        assert_eq!(rows, 1, "the held claim joins back to the bridge run");
    }

    // -- the doctrines ---------------------------------------------------------

    #[test]
    fn the_run_frame_travels_at_the_section_retraction_rung() {
        let ttl = turtle(FIXTURE);
        for required in [
            math("ProofIngestRun"),
            math("parseSource"),
            logic("instantiatesSchema"),
            logic("instantiatesPlan"),
            math("ingestCorrespondence"),
            logic("SectionRetraction"),
            logic("ExactPreservation"),
            logic("Equiv"),
            logic("Crisp"),
            logic("mnemomorphic"),
        ] {
            assert!(ttl.contains(&required), "the frame is missing `{required}`");
        }
        assert!(
            !ttl.contains(&logic("LossyLens")),
            "the proof rung is a section/retraction, not the lossy lens the other two claim"
        );
    }

    #[test]
    fn a_relift_of_the_same_derivation_is_byte_identical() {
        let a = turtle(FIXTURE);
        let b = turtle(FIXTURE);
        assert_eq!(a, b, "the lift is idempotent: no clock, no counter");
    }

    #[test]
    fn the_lift_is_independent_of_the_order_the_steps_are_written_in() {
        // Source order is not dependency order; the lift walks the DAG, so writing the
        // conclusion first must produce the same codomain (a different run IRI, since the
        // run is content-addressed on the SOURCE BYTES, but the same reconstruction).
        let forward = b"cnf(a0, axiom, p(a)).\n\
                        cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n";
        let backward = b"cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n\
                         cnf(a0, axiom, p(a)).\n";
        assert_eq!(
            reconstruct(&turtle(forward)),
            reconstruct(&turtle(backward)),
            "the lifted derivation is the DAG, not the file layout"
        );
    }

    #[test]
    fn a_different_derivation_mints_a_different_run() {
        let a = lift(FIXTURE, BASE).expect("lifts");
        let b = lift(RICH, BASE).expect("lifts");
        assert_ne!(a.run_iri, b.run_iri);
    }

    #[test]
    fn every_codomain_node_carries_the_back_edge_the_native_lint_reads() {
        let lifted = lift(RICH, BASE).expect("lifts");
        assert_eq!(
            count(&lifted.turtle, &gmeow("wasGeneratedBy")),
            lifted.codomain_nodes,
            "exactly one gmeow:wasGeneratedBy per generated node"
        );
    }

    #[test]
    fn a_lifted_graph_carries_no_private_use_language_tag() {
        for source in [FIXTURE, RICH] {
            assert!(
                !turtle(source).contains("x-gmeow-"),
                "consumer output must not leak a private-use tag"
            );
        }
    }

    // -- content addressing ----------------------------------------------------

    #[test]
    fn a_repeated_conclusion_collapses_to_one_content_addressed_expression() {
        // Two steps concluding the SAME clause are one conclusion, reached twice.
        let source = b"cnf(a0, axiom, p(a)).\n\
                       cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n\
                       cnf(d2, plain, q(a), inference(s, [status(thm)], [d1])).\n";
        let ttl = turtle(source);
        let graph = Graph::of(&ttl);
        let conclusions: BTreeSet<String> = graph
            .subjects_typed(&math("ProofStep"))
            .iter()
            .map(|step| graph.object(step, &math("hasConclusion")))
            .collect();
        assert_eq!(
            conclusions.len(),
            1,
            "`q(a)` is ONE expression however many steps conclude it:\n{ttl}"
        );
        // …and so is the goal, whose identity logic:GoalExpression declares to be
        // structural: same kind, same operands, same bound situation type.
        assert_eq!(typed_as(&ttl, &logic("GoalExpression")), 1);
        assert_eq!(typed(&ttl, "ProofStep"), 2, "but the STEPS stay distinct");
    }

    #[test]
    fn a_repeated_sub_term_is_one_node_and_distinct_structure_grows_the_graph() {
        let shared = lift(
            b"cnf(a0, axiom, p(f(a), f(a))).\n\
              cnf(d1, plain, q(f(a)), inference(r, [status(thm)], [a0])).\n",
            BASE,
        )
        .expect("lifts");
        let distinct = lift(
            b"cnf(a0, axiom, p(f(a), f(b))).\n\
              cnf(d1, plain, q(f(c)), inference(r, [status(thm)], [a0])).\n",
            BASE,
        )
        .expect("lifts");
        assert!(
            distinct.codomain_nodes > shared.codomain_nodes,
            "the fact count grows with DISTINCT structure, not with textual repetition"
        );
        let graph = Graph::of(&shared.turtle);
        assert_eq!(
            graph
                .triples
                .iter()
                .filter(|t| t.predicate == LABEL && t.object == "f(a)")
                .count(),
            1,
            "the repeated `f(a)` is one interned expression:\n{}",
            shared.turtle
        );
    }

    #[test]
    fn an_identical_sub_derivation_collapses_to_one_proof_term() {
        // Two steps reached by the same rule over the same sub-proof ARE the same proof
        // term, even though they remain two named steps.
        let source = b"cnf(a0, axiom, p(a)).\n\
                       cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n\
                       cnf(d2, plain, s(a), inference(r, [status(thm)], [a0])).\n\
                       cnf(d3, plain, t(a), inference(u, [status(thm)], [d1, d2])).\n";
        let ttl = turtle(source);
        let graph = Graph::of(&ttl);
        let terms: BTreeSet<String> = ["d1", "d2"]
            .iter()
            .map(|name| {
                let step = graph
                    .triples
                    .iter()
                    .find(|t| t.predicate == LABEL && t.object == **name)
                    .map(|t| t.subject.clone())
                    .expect("the step is labelled with its TSTP name");
                graph.object(&step, &math("formalizesExpression"))
            })
            .collect();
        assert_eq!(
            terms.len(),
            1,
            "`r(p(a))` is one proof term however many steps it justifies:\n{ttl}"
        );
        // The steps themselves do NOT collapse: a step's identity is its NAME.
        assert_eq!(typed(&ttl, "ProofStep"), 3);
        assert_eq!(reconstruct(&ttl).len(), 4, "and all four steps rebuild");
    }

    #[test]
    fn a_proof_terms_operands_are_ordered_contiguous_zero_based_slots() {
        let ttl = turtle(RICH);
        let graph = Graph::of(&ttl);
        let top = graph
            .triples
            .iter()
            .find(|t| t.predicate == LABEL && t.object == "d_top")
            .map(|t| t.subject.clone())
            .expect("the terminal step");
        let term = graph.object(&top, &math("formalizesExpression"));
        let mut indexes: Vec<String> = graph
            .objects(&term, &math("argumentSlot"))
            .iter()
            .map(|slot| graph.object(slot, &math("slotIndex")))
            .collect();
        indexes.sort();
        assert_eq!(
            indexes,
            vec!["0".to_owned(), "1".to_owned()],
            "TSTP's operand ORDER survives as contiguous zero-based slots"
        );
        assert_eq!(
            graph.object(&term, &math("denotationKind")),
            math("denotesProof"),
            "a proof term declares the denotation kind the slice mints for exactly this case"
        );
    }

    #[test]
    fn a_clause_lifts_into_a_typed_expression_ast_not_a_string() {
        let ttl = turtle(RICH);
        let graph = Graph::of(&ttl);
        // `~r(f(a)) | s(b)` is a disjunction of a negation and an atom, all structured.
        let disjunction = graph
            .triples
            .iter()
            .find(|t| t.predicate == LABEL && t.object == "~r(f(a)) | s(b)")
            .map(|t| t.subject.clone())
            .expect("the disjunctive clause is an expression node");
        assert_eq!(graph.objects(&disjunction, &math("argumentSlot")).len(), 2);
        let operation = graph.object(&disjunction, &math("operator"));
        assert_eq!(graph.label(&operation), "logical disjunction (|)");
        assert!(
            ttl.contains("logical negation (~)"),
            "the negated literal is an application of negation, not a prefixed string"
        );
        // A clause variable resolves to a declaration: there is no implicit free variable.
        assert_eq!(typed(&ttl, "VariableExpression"), 1);
        assert_eq!(typed(&ttl, "VariableOccurrence"), 1);
        assert_eq!(typed(&ttl, "FreeVariableDeclaration"), 1);
        // The empty clause rides as the defined atom, resolved through one symbol.
        assert_eq!(
            graph
                .triples
                .iter()
                .filter(|t| t.predicate == LABEL && t.object == "$false")
                .count(),
            2,
            "the `$false` symbol and the reference that resolves to it"
        );
    }

    #[test]
    fn an_inference_rule_and_a_predicate_that_share_a_spelling_are_two_operations() {
        let ttl = turtle(
            b"cnf(a0, axiom, r(a)).\n\
              cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n",
        );
        let graph = Graph::of(&ttl);
        let named_r: BTreeSet<String> = graph
            .triples
            .iter()
            .filter(|t| t.predicate == LABEL && t.object == "r")
            .map(|t| t.subject.clone())
            .collect();
        assert_eq!(
            named_r.len(),
            2,
            "the inference rule `r` and the predicate `r` are different operators:\n{ttl}"
        );
    }

    // -- hard failures ---------------------------------------------------------

    #[test]
    fn a_malformed_derivation_is_a_typed_parse_failure_with_a_position() {
        let err = lift(b"cnf(a0, axiom, p(a)\n", BASE).expect_err("malformed TSTP must not lift");
        assert!(
            err.is::<TstpParse>(),
            "expected math.lift.proof.parse: {err}"
        );
        assert!(format!("{err}").contains("line "), "{err}");
    }

    #[test]
    fn a_dangling_parent_is_a_typed_unliftable_failure() {
        let err = lift(
            b"cnf(a0, axiom, p(a)).\n\
              cnf(d1, plain, q(a), inference(r, [status(thm)], [ghost])).\n",
            BASE,
        )
        .expect_err("a dangling parent must not lift");
        assert!(
            err.is::<ProofUnliftable>(),
            "expected math.lift.proof.unliftable: {err}"
        );
        assert!(format!("{err}").contains("`ghost`"), "{err}");
    }

    #[test]
    fn a_cyclic_dependency_graph_is_a_typed_unliftable_failure() {
        let err = lift(
            b"cnf(d1, plain, p(a), inference(r, [status(thm)], [d2])).\n\
              cnf(d2, plain, q(a), inference(r, [status(thm)], [d1])).\n",
            BASE,
        )
        .expect_err("a cycle must not lift");
        assert!(
            err.is::<ProofUnliftable>(),
            "expected math.lift.proof.unliftable: {err}"
        );
        assert!(format!("{err}").contains("cycle"), "{err}");
    }

    #[test]
    fn an_out_of_fragment_construct_never_reaches_the_sink() {
        // The whole-or-nothing rule: an unliftable derivation produces NO triples at all.
        for source in [
            &b"cnf(a0, negated_conjecture, p(a)).\n"[..],
            &b"cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), file('problem.p')).\n"[..],
        ] {
            assert!(
                lift(source, BASE).is_err(),
                "an unstructured construct must hard-fail rather than lift partially"
            );
        }
    }

    #[test]
    fn the_committed_fixture_is_the_reasoners_own_derivation() {
        // The fixture is a PRODUCT of `gmeow_logic::proof_tree::ProofTree::to_tstp`,
        // byte-pinned by `gmeow_conformance::external::tptp::lower_fol`'s
        // `the_committed_tstp_fixture_is_exactly_what_our_reasoner_produces`. Both crates
        // include_ the same file, so a drift on either side is caught; this end asserts the
        // shape that pin guarantees.
        let text = std::str::from_utf8(FIXTURE).expect("utf-8");
        assert!(
            text.contains("produced by OUR OWN native reasoner"),
            "the fixture's provenance header"
        );
        let derivation = tstp::parse(FIXTURE).expect("parses");
        assert_eq!(derivation.steps().len(), 3);
        assert_eq!(derivation.steps()[0].role, Role::Axiom);
        assert!(
            derivation
                .conclusion()
                .rule
                .as_deref()
                .expect("a rule")
                .starts_with("https://blackcatinformatics.ca/logic/dag/firing/"),
            "the rule names the content-addressed ground-instance firing"
        );
    }
}
