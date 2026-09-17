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
//! | the derivation as a theory context | one `math:MathematicalTheory`, the scope every statement role holds in |
//! | a step's FORMULA ROLE | a `math:MathematicalStatement` carrying `math:statementRole` under `math:roleInTheory` |
//! | a derived step (an `inference(…)` or a bare DAG parent) | a `math:ProofStep`, reached by `math:proofStep` |
//! | an asserted leaf in a FOUNDATION role | a `math:Axiom`, reached by `math:dependsOnAxiom` |
//! | an asserted leaf in any other role | a `math:MathematicalObject` premise, reached by `math:hasPremise` |
//! | a `file(…)`/`theory(…)`/`introduced(…)`/`creator(…)` source | a `math:externalWarrant` naming the reference |
//! | a step's parents | `math:hasPremise` to each parent's node (and `math:dependsOnAxiom` when that parent is a law) |
//! | a step's inference rule | `math:usesInferenceRule`, and the `math:operator` of the step's proof term |
//! | a step's conclusion | a `math:MathematicalExpression` AST — a flat clause, or a real binder tree for a `fof` formula |
//! | a refuted negated conjecture | the proof's `math:usesProofMethod`, a refutation `math:ProofMethod` |
//! | the conclusion each step reaches | a `logic:GoalExpression` sub-goal (`logic:AchievementGoal` over a `logic:Situation`), reached by `math:provesGoal` |
//! | the QED | a `math:FormalVerificationResult` named by a `gmeow:Observation` with a `gmeow:Standpoint` vantage |
//!
//! # The rung is EARNED, not declared
//!
//! [`Rung::section_retraction`] (`logic:SectionRetraction` / `logic:ExactPreservation` /
//! `logic:Equiv`) is claimed **per run**, not per bridge, and only when the run has NOTHING
//! to enumerate as residue. [`residue`] walks the derivation first; anything the `math:`
//! codomain has no property for is recorded on the source witness through
//! `RunFrame::record_unmapped`, and a run with any residue travels at
//! [`Rung::lossy_crisp_with_witness`] instead. Three constructs do that:
//!
//! - **an SZS inference status other than `status(thm)`** — `status(cth)` and `status(esa)`
//!   are real claims about what a step preserves (counter-theorem, equisatisfiability), and
//!   the `math:` proof layer declares no property from a `math:ProofStep` to an SZS status.
//!   Reading them as if they were `thm` would state that every step preserved theoremhood.
//! - **the role `unknown`** — `math:StatementRole` is a closed six-value set with no
//!   unknown, and picking one of the six for a formula whose status the source declines to
//!   state would invent an epistemic commitment.
//! - **a `<useful_info>` 5th field** — prover bookkeeping (`[proof]`, `[iquote('…')]`) with
//!   no `math:` codomain.
//!
//! Everything else the reader structures DOES cross, and
//! `the_lift_is_a_section_the_derivation_reconstructs_from_the_graph_alone` proves it by
//! rebuilding every step name, ROLE, inference rule, parent set, and rendered conclusion
//! from the emitted Turtle and nothing else.
//!
//! Three `rdfs:label` conventions make that reconstruction unambiguous, and all three are
//! load-bearing:
//!
//! - **on a proof-layer node (a `math:ProofStep`, a `math:Axiom`, a premise object) the
//!   label is the step's TSTP NAME** — the identity a parent list cites.
//! - **on a `math:MathematicalStatement` the label is the step's RAW TPTP ROLE WORD.**
//!   Several TPTP roles share one `math:StatementRole` value (`axiom`, `hypothesis` and
//!   `assumption` are all `math:roleAxiom`), so the mapped value alone does not recover
//!   what the prover wrote.
//! - **on a `math:Operation` the label is the operator's RAW TOKEN** — the inference-rule
//!   IRI, the predicate symbol, or the connective's own name.
//!
//! # How a step, its role, and its conclusion are joined
//!
//! `math:hasConclusion`'s range is "an expression or STATEMENT", so the layering is:
//!
//! ```text
//! math:ProofStep --hasConclusion--> math:MathematicalStatement --hasConclusion--> expression
//! ```
//!
//! A `math:Axiom` is not in that property's domain, so an asserted leaf is joined the other
//! way round — `statement --math:dependsOnAxiom--> axiom`, exactly the shape the shipped
//! worked example `slices/grounding/math/examples/theorem-proof-claim.ttl` uses
//! (`ex:pythagoreanStatement math:dependsOnAxiom ex:euclidPostulate5`). A leaf in a
//! non-foundation role is joined by `math:hasPremise` instead, because it is a premise the
//! derivation takes as given and NOT a law it may be typed with.
//!
//! # Why `negated_conjecture` is not flattened
//!
//! A negated conjecture is asserted so that deriving a contradiction from it REFUTES it and
//! thereby establishes the conjecture. Typing it `math:Axiom` would assert its content as a
//! law of the theory — the exact opposite of what the derivation claims — so it never is.
//! It carries `math:statementRole math:roleConjecture` (the candidate under active test),
//! the proof that contains one carries `math:usesProofMethod` naming refutation, and the
//! `math:Proof` never reaches it through `math:dependsOnAxiom`.
//!
//! # Why a step's rule ALSO rides on a proof TERM
//!
//! A TSTP step is a proof term: `rule(parent-proof-terms…)`. So the step is
//! `math:formalizesExpression`-ed to a `math:ApplicationExpression` whose `math:operator` is
//! the rule and whose `math:argumentSlot`s are the parents' proof terms, carrying
//! `math:denotationKind math:denotesProof` — the kind the slice declares for exactly this
//! case. `math:usesInferenceRule` carries the same rule on the step itself, so a consumer
//! asking "which steps fired resolution?" need not walk into a proof term. A bare `<name>`
//! DAG source declares NO rule, so it gets neither: its proof term is its parent's, and
//! minting a rule token the source never wrote would be fabrication.
//!
//! # Content-addressed interning
//!
//! Every expression is interned into a [`TermArena`] and its [`ContentKey`] mints its IRI,
//! so two steps concluding the same clause share ONE expression, a repeated sub-term is one
//! node, and two steps reached by an identical sub-derivation share one proof term.
//!
//! Under a `fof` binder the IRI is additionally qualified by the ENCLOSING BINDER CHAIN.
//! It has to be: `math:VariableDeclaration`'s own definition says "two binders that reuse
//! the glyph i introduce two distinct declarations", and a `math:VariableOccurrence` names
//! at most one declaration, so a bound-variable leaf that collapsed across binders would
//! resolve to several. Bound variables intern by de-Bruijn distance, so α-equivalent
//! formulas still share one arena term; the chain only separates their RDF identities. A
//! CNF clause has no binders, so its chain is empty and its IRIs are unchanged.
//!
//! # What this lift refuses rather than fakes
//!
//! - The `math:FormalVerificationResult` is NOT accompanied by a `math:ProofCheckActivity`.
//!   This bridge parses and structurally checks a derivation; it does not run a proof
//!   assistant, so claiming the process node would name an activity that never occurred.
//! - The verdict is `math:verificationPassed` ONLY when every `inference(…)` in the
//!   derivation declares `status(thm)`. When one does not — an `esa` Skolemization, a `cth`
//!   negation, an empty status list — the result carries `math:verificationUnknown`, whose
//!   own definition covers exactly this ("the check neither accepted nor rejected … never
//!   collapsed into a bare failure, because 'not proved' is not 'refuted'").
//! - The result claims `math:verificationPassed` from ONE named vantage — this bridge's own
//!   structural checker — and says in its standpoint's label exactly what that checker did
//!   and did not do. It never claims the inferences were re-derived.
//! - A conclusion expression is NOT typed `math:denotationKind math:denotesProposition`. It
//!   is content-addressed, so the same node may also stand in argument position inside
//!   another expression; asserting proposition-hood on a shared node would state it of a use
//!   the source never made. The proof term has no such ambiguity — its arena key is
//!   rule-headed — so `math:denotesProof` IS asserted there.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_term_arena::{Arena, ContentKey, StructNode, TermArena};
use purrdf::TermValue;

use crate::error::ProofUnliftable;
use crate::frame::{BridgeKind, Lifted, RunFrame, Rung};
use crate::ns::{gmeow, logic, math};
use crate::proof::tstp::{
    self, Clause, Conclusion, Connective, Derivation, Formula, Literal, Quantifier, Role, Source,
    Step, Term, render_atom,
};
use crate::sink::Sink;

/// `rdfs:label`.
///
/// The one non-`math:`/`logic:`/`gmeow:` term this lift needs, and it is load-bearing rather
/// than decorative: a step's TSTP NAME, a statement's RAW ROLE WORD, and an operator's RAW
/// TOKEN are three of the five facts the section/retraction claim rests on, and none has a
/// `math:` property of its own. The literal is PLAIN — [`Sink`] exposes no language-tagged
/// constructor, because lifted graphs leave through the shipped CLI where no `x-gmeow-*`
/// private-use tag may appear.
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// The single sort a `fof` binder quantifies over.
///
/// FOF is untyped: every quantifier ranges over the one universe of individuals, TPTP's
/// `$i`. Naming it explicitly keeps the arena's binder blocks well-formed without inventing
/// a type system the dialect does not have.
const INDIVIDUAL_SORT: &str = "tstp:sort:$i";

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

    // The rung is decided BEFORE a triple is written: a run that has anything to enumerate
    // as residue is not an exact lift, and `RunFrame::emit` writes the law spine once.
    let mut frame = RunFrame::mint(BridgeKind::Proof, mint_base, source);
    residue(&mut frame, &derivation);
    let rung = if frame.unmapped.is_empty() {
        Rung::section_retraction()
    } else {
        Rung::lossy_crisp_with_witness()
    };
    let frame = frame;

    let mut sink = Sink::new();
    frame.emit(&mut sink, rung);

    let mut lift = Lift {
        frame: &frame,
        sink,
        arena: TermArena::new(),
        emitted: BTreeSet::new(),
        step_node: BTreeMap::new(),
        statement_node: BTreeMap::new(),
        law_names: BTreeSet::new(),
        conclusion_iri: BTreeMap::new(),
        proof_term: BTreeMap::new(),
        proof_term_iri: BTreeMap::new(),
        theory: String::new(),
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

/// Enumerate, on the run's source witness, everything the `math:` codomain cannot carry.
///
/// A lift that declared a rung weaker than `logic:ExactPreservation` and enumerated nothing
/// would be asserting a loss it cannot name; a lift that enumerated nothing and declared the
/// STRONGER rung while dropping a stated fact would be worse. This is the one place that
/// decides which of the two a given derivation is.
fn residue(frame: &mut RunFrame, derivation: &Derivation) {
    for step in derivation.steps() {
        if step.role == Role::Unknown {
            frame.record_unmapped(format!(
                "the formula role `unknown` on step `{}`: the source declines to state an \
                 epistemic status, math:StatementRole is a closed six-value set with no unknown, \
                 and choosing one of the six would invent a commitment the derivation refused to \
                 make — so the step carries no math:MathematicalStatement",
                step.name
            ));
        }
        for term in step.status() {
            if term != "status(thm)" {
                frame.record_unmapped(format!(
                    "the inference status term `{term}` on step `{}`: the math: proof layer \
                     declares no property from a math:ProofStep to an SZS inference status, and \
                     reading a non-thm status as thm would assert the step preserved theoremhood \
                     when the prover said it did not",
                    step.name
                ));
            }
        }
        for info in &step.useful_info {
            frame.record_unmapped(format!(
                "the <useful_info> term `{info}` on step `{}`: prover bookkeeping with no math: \
                 codomain, enumerated here rather than silently dropped",
                step.name
            ));
        }
    }
}

/// The `math:StatementRole` a TPTP formula role holds, or `None` for `unknown`.
///
/// The map is many-to-one because `math:StatementRole` is a closed six-value set and TPTP
/// has fifteen roles. That is why the RAW WORD rides on the statement's `rdfs:label`: the
/// mapped value says how the theory HOLDS the formula, the word says what the prover wrote,
/// and neither substitutes for the other.
fn statement_role(role: Role) -> Option<&'static str> {
    Some(match role {
        // A foundation of the theory rather than something derived within it.
        Role::Axiom | Role::Hypothesis | Role::Assumption => "roleAxiom",
        // True by stipulation: a definition, a symbol's type declaration, and the
        // finite-interpretation formulas that FIX a model's domain, functions and
        // predicates rather than assert anything provable about them.
        Role::Definition | Role::Type | Role::FiDomain | Role::FiFunctors | Role::FiPredicates => {
            "roleDefinition"
        }
        // An auxiliary result proved en route: a declared lemma, and a prover's `plain`
        // working step, which is precisely an intermediate proved formula with no
        // user-facing status of its own.
        Role::Lemma | Role::Plain => "roleLemma",
        Role::Theorem => "roleTheorem",
        Role::Corollary => "roleCorollary",
        // The candidate under active epistemic test. A negated conjecture is the SAME
        // candidate, asserted in negated form so that refuting it settles the test; it is
        // never a law, which is what keeps it out of math:Axiom.
        Role::Conjecture | Role::NegatedConjecture => "roleConjecture",
        Role::Unknown => return None,
    })
}

// ── Lift state ────────────────────────────────────────────────────────────────

/// One enclosing `fof` binder, as the RDF identities its bound variable resolves through.
#[derive(Debug, Clone)]
struct Binding {
    /// The bound variable's glyph.
    name: String,
    /// The binder expression's arena content key — what qualifies every IRI beneath it.
    key: String,
    /// The `math:VariableOccurrence` the binder binds.
    ///
    /// The binder's `math:VariableDeclaration` is reached from here through
    /// `math:declaredVariable`, so a bound leaf names the occurrence and the occurrence
    /// names the declaration — the one-declaration-per-occurrence discipline
    /// `math:UnscopedVariableOccurrence` enforces.
    occurrence: String,
}

/// The enclosing binder chain, as the key qualifier every IRI beneath it carries.
fn scope_chain(scope: &[Binding]) -> String {
    let keys: Vec<&str> = scope.iter().map(|b| b.key.as_str()).collect();
    keys.join("/")
}

struct Lift<'f> {
    frame: &'f RunFrame,
    sink: Sink,
    arena: TermArena,
    emitted: BTreeSet<String>,
    /// Step name → the proof-layer node standing for it: a `math:ProofStep` for a derived
    /// step, a `math:Axiom` for an asserted law, a premise object for any other leaf.
    step_node: BTreeMap<String, String>,
    /// Step name → the `math:MathematicalStatement` carrying its role, when it has one.
    statement_node: BTreeMap<String, String>,
    /// The names of the steps whose proof-layer node is typed `math:Axiom`.
    law_names: BTreeSet<String>,
    /// Step name → the expression node its conclusion interns to.
    conclusion_iri: BTreeMap<String, String>,
    /// Step name → the arena node of the step's PROOF TERM. A step justified by a rule has
    /// `rule(parents…)`; a leaf's is the conclusion it states, because the proof of an
    /// asserted formula is the formula; a bare DAG source's is its parent's, unchanged.
    proof_term: BTreeMap<String, StructNode>,
    /// Step name → the IRI of that proof term.
    proof_term_iri: BTreeMap<String, String>,
    /// The `math:MathematicalTheory` every statement role in this derivation holds under.
    theory: String,
    /// How many genuinely PROOF-layer structures the run produced — steps, statements,
    /// axioms, and the expression ASTs they carry.
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

    /// Mint an expression's IRI from its CONTENT and its enclosing binder chain.
    ///
    /// At top level (a CNF clause, or a `fof` formula outside any binder) the chain is empty
    /// and the IRI is the pure content address, exactly as before binders existed.
    fn scoped_expression(&mut self, node: StructNode, scope: &[Binding]) -> (String, bool) {
        let key = self.key_of(node).into_string();
        if scope.is_empty() {
            self.mint("expr", &key)
        } else {
            self.mint("expr", &format!("{key}@{}", scope_chain(scope)))
        }
    }

    // -- the whole derivation -------------------------------------------------

    fn derivation(&mut self, derivation: &Derivation) {
        let key = derivation.render();

        // The theory context every statement role is scoped to. math:roleInTheory's own
        // definition makes it mandatory: "'this is a theorem' is always 'a theorem IN this
        // theory version', never an unconditional global fact". The derivation IS that
        // context, and it is versioned by its content address.
        let (theory, _) = self.mint("theory", &key);
        self.sink.typed(&theory, &math("MathematicalTheory"));
        self.label(
            &theory,
            &format!(
                "the theory the TSTP derivation states its formula roles in: {} annotated \
                 formulas, content-addressed",
                derivation.steps().len()
            ),
        );
        self.theory = theory;

        // Dependency order, not source order: a derived step's proof term is built from its
        // parents', and TSTP does not require a step to be written after the steps it cites.
        let terminal = derivation.conclusion().name.clone();
        for index in derivation.dependency_order() {
            let step = &derivation.steps()[index];
            let is_terminal = step.name == terminal;
            self.step(step, is_terminal);
        }

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

        // The DAG names the proof it underlies. Without this edge the two would be related
        // only by co-generation from the same run, which is a run-scoped coincidence rather
        // than the structural fact the class definition asserts.
        self.sink.iri(&dag, &math("dependencyGraphOf"), &proof);

        // The proof discharges the goal its terminal step reaches, and decomposes into the
        // derived steps and the laws they rest on.
        let goal = self.emit_goal(&rendered);
        self.sink.iri(&proof, &math("provesGoal"), &goal);
        // What the proof ESTABLISHES is the terminal step's role-bearing statement — the
        // node math:provesStatement's range names. A terminal in the `unknown` role has no
        // statement, so the proof names the conclusion expression instead rather than
        // silently naming nothing.
        let statement = self
            .statement_node
            .get(&conclusion.name)
            .or_else(|| self.conclusion_iri.get(&conclusion.name))
            .cloned();
        if let Some(statement) = statement {
            self.sink.iri(&proof, &math("provesStatement"), &statement);
        }
        for step in derivation.steps() {
            let node = self.step_node[&step.name].clone();
            if step.is_derived() {
                self.sink.iri(&proof, &math("proofStep"), &node);
            } else if self.law_names.contains(&step.name) {
                self.sink.iri(&proof, &math("dependsOnAxiom"), &node);
            }
        }

        // A derivation that asserts a negated conjecture and reaches a contradiction is a
        // refutation, and that IS its proof strategy. Naming it is the structural way to
        // carry what `negated_conjecture` means; leaving it out would make the role a bare
        // word rather than a claim about how the proof works.
        if derivation
            .steps()
            .iter()
            .any(|s| s.role == Role::NegatedConjecture)
        {
            let (method, fresh) = self.mint("method", "refutation");
            if fresh {
                self.sink.typed(&method, &math("ProofMethod"));
                self.label(
                    &method,
                    "refutation (proof by contradiction): the derivation asserts the negation of \
                     the conjecture and derives a contradiction from it, which refutes the \
                     negation and thereby establishes the conjecture",
                );
            }
            self.sink.iri(&proof, &math("usesProofMethod"), &method);
        }

        let thm_verified = derivation
            .steps()
            .iter()
            .filter(|s| matches!(s.source, Source::Inference { .. }))
            .all(Step::declares_thm_status);
        self.emit_verification(&key, &proof, &rendered, thm_verified);
    }

    // -- one step --------------------------------------------------------------

    fn step(&mut self, step: &Step, is_terminal: bool) {
        let (conclusion_node, conclusion_iri) = self.emit_conclusion(&step.conclusion);
        self.conclusion_iri
            .insert(step.name.clone(), conclusion_iri.clone());
        let statement = self.emit_statement(step, &conclusion_iri, is_terminal);
        if let Some(statement) = &statement {
            self.statement_node
                .insert(step.name.clone(), statement.clone());
        }

        if step.is_derived() {
            self.emit_derived(step, statement.as_deref());
        } else {
            self.emit_leaf(step, conclusion_node, &conclusion_iri, statement.as_deref());
        }
    }

    /// The statement layer: the step's ROLE, held under the derivation's theory.
    ///
    /// Returns `None` for the `unknown` role — the one TPTP role with no `math:StatementRole`
    /// image. Its absence is enumerated as residue by [`residue`], so the run declares the
    /// loss rather than minting a role-less statement that "names exactly one" role.
    fn emit_statement(
        &mut self,
        step: &Step,
        conclusion_iri: &str,
        is_terminal: bool,
    ) -> Option<String> {
        let role = statement_role(step.role)?;
        let (statement, _) = self.mint("statement", &step.name);
        self.sink.typed(&statement, &math("MathematicalStatement"));
        // The RAW TPTP role word: `hypothesis` and `assumption` both map to math:roleAxiom,
        // so the mapped value alone does not recover what the prover wrote.
        self.label(&statement, step.role.as_str());
        self.sink
            .iri(&statement, &math("statementRole"), &math(role));
        let theory = self.theory.clone();
        self.sink.iri(&statement, &math("roleInTheory"), &theory);
        self.sink
            .iri(&statement, &math("hasConclusion"), conclusion_iri);

        // An `<external_source>` cited in the INFERENCE's parent list — `theory(equality)`
        // on E's every equality step, `file('SET001-1.p', ax7)` on an imported premise. It
        // warrants the step without being a step, so it rides as a warrant exactly as a
        // source-position external does. A step's source is either an inference or an
        // external, never both, so these can never be conflated with the arm below.
        for external in &step.external_parents {
            let rendered = external.rendered.clone();
            let (warrant, fresh) = self.mint("warrant", &rendered);
            if fresh {
                self.sink.typed(&warrant, &math("MathematicalObject"));
                self.label(&warrant, &rendered);
                self.proof_structures += 1;
            }
            self.sink
                .iri(&statement, &math("externalWarrant"), &warrant);
        }

        if let Source::External(external) = &step.source {
            // The premise came from OUTSIDE this derivation. math:externalWarrant is exactly
            // "a declared external warrant that stands in for an in-graph math:Proof", and
            // the reference rides verbatim because `file`, `theory`, `introduced` and
            // `creator` have different argument shapes with no common normal form.
            let rendered = external.rendered.clone();
            let (warrant, fresh) = self.mint("warrant", &rendered);
            if fresh {
                self.sink.typed(&warrant, &math("MathematicalObject"));
                self.label(&warrant, &rendered);
                self.proof_structures += 1;
            }
            self.sink
                .iri(&statement, &math("externalWarrant"), &warrant);
        } else if role == "roleTheorem" && !is_terminal {
            // math:UngroundedTheoremClaim: a math:roleTheorem statement needs a theory
            // context AND either a math:Proof through math:provesStatement or a declared
            // external warrant. The terminal step gets the former; any other step declared
            // a theorem by the document is warranted by the document itself, which the run
            // retains in band as its math:parseSource witness.
            let witness = self.frame.source_witness_iri.clone();
            self.sink
                .iri(&statement, &math("externalWarrant"), &witness);
        }

        self.proof_structures += 1;
        Some(statement)
    }

    /// A step justified from inside the derivation: an `inference(…)` or a bare DAG parent.
    fn emit_derived(&mut self, step: &Step, statement: Option<&str>) {
        let parents: Vec<StructNode> = step
            .parents
            .iter()
            .map(|parent| self.proof_term[parent])
            .collect();

        let (term_node, term_iri) = match step.rule() {
            Some(rule) => {
                // The proof term: rule(parent-proof-terms…). Content-addressed over the
                // WHOLE sub-derivation, so two steps reached by an identical sub-proof share
                // one term.
                let rule = rule.to_owned();
                let node = self.app(&format!("tstp:rule:{rule}"), &parents);
                let (iri, fresh) = self.scoped_expression(node, &[]);
                if fresh {
                    self.sink.typed(&iri, &math("ApplicationExpression"));
                    let operation = self.emit_operation("rule", &rule, &rule);
                    self.sink.iri(&iri, &math("operator"), &operation);
                    // The kind the slice declares for exactly this case: an expression
                    // standing for a proof-term, carried as structured content.
                    self.sink
                        .iri(&iri, &math("denotationKind"), &math("denotesProof"));
                    let operands: Vec<String> = parents
                        .iter()
                        .map(|&node| {
                            let key = self.key_of(node).into_string();
                            self.frame.node("expr", &key)
                        })
                        .collect();
                    self.emit_slots(&iri, &operands);
                    self.proof_structures += 1;
                }
                (node, iri)
            }
            None => {
                // A bare `<name>` DAG source: the step restates its parent under a new name.
                // Its proof term IS the parent's — there is no rule to head a new one, and
                // minting `tstp:rule:<something>` would put a token in the graph the source
                // never wrote.
                let parent = step
                    .parents
                    .first()
                    .expect("a bare DAG source names exactly one parent");
                (self.proof_term[parent], self.proof_term_iri[parent].clone())
            }
        };
        self.proof_term.insert(step.name.clone(), term_node);
        self.proof_term_iri
            .insert(step.name.clone(), term_iri.clone());

        let (node, _) = self.mint("step", &step.name);
        self.sink.typed(&node, &math("ProofStep"));
        // The step's NAME — the identity a parent list cites.
        self.label(&node, &step.name);
        if let Some(statement) = statement {
            // math:hasConclusion's range is "an expression or STATEMENT"; a step draws its
            // conclusion as the role-bearing statement, which in turn draws the expression.
            self.sink.iri(&node, &math("hasConclusion"), statement);
        } else {
            let conclusion = self.conclusion_iri[&step.name].clone();
            self.sink.iri(&node, &math("hasConclusion"), &conclusion);
        }
        self.sink
            .iri(&node, &math("formalizesExpression"), &term_iri);
        if let Some(rule) = step.rule() {
            let rule = rule.to_owned();
            let rule_iri = self.emit_operation("rule", &rule, &rule);
            self.sink.iri(&node, &math("usesInferenceRule"), &rule_iri);
        }
        for parent in &step.parents {
            let parent_node = self.step_node[parent].clone();
            self.sink.iri(&node, &math("hasPremise"), &parent_node);
            // A parent asserted as a LAW is a foundation the step rests on, which is
            // precisely what a proof dependency DAG records. A parent asserted in any other
            // role — a negated conjecture above all — is a premise and never a law.
            if self.law_names.contains(parent) {
                self.sink.iri(&node, &math("dependsOnAxiom"), &parent_node);
            }
        }
        let goal = self.emit_goal(&step.conclusion.render());
        self.sink.iri(&node, &math("provesGoal"), &goal);
        self.step_node.insert(step.name.clone(), node);
        self.proof_structures += 1;
    }

    /// A leaf: a formula the derivation asserts outright or imports from outside itself.
    fn emit_leaf(
        &mut self,
        step: &Step,
        conclusion_node: StructNode,
        conclusion_iri: &str,
        statement: Option<&str>,
    ) {
        let law = step.role.is_foundational();
        let (node, _) = if law {
            self.mint("axiom", &step.name)
        } else {
            self.mint("premise", &step.name)
        };
        if law {
            // math:Axiom's own definition rejects the amnesic reading ("never an opaque
            // string"), so the law names the AST it states rather than a rendering of it.
            self.sink.typed(&node, &math("Axiom"));
            self.law_names.insert(step.name.clone());
        } else {
            // A conjecture, a negated conjecture, a definition, a finite-interpretation
            // formula: the derivation takes it as given without holding it as a law of the
            // theory, so it is a mathematical object the proof draws on, not an axiom.
            self.sink.typed(&node, &math("MathematicalObject"));
        }
        self.label(&node, &step.name);
        self.sink
            .iri(&node, &math("formalizesExpression"), conclusion_iri);
        if let Some(statement) = statement {
            // math:Axiom is not in math:hasConclusion's domain, so the statement/leaf join
            // runs the other way — the shape theorem-proof-claim.ttl uses.
            let join = if law {
                math("dependsOnAxiom")
            } else {
                math("hasPremise")
            };
            self.sink.iri(statement, &join, &node);
        }
        self.step_node.insert(step.name.clone(), node);
        self.proof_term.insert(step.name.clone(), conclusion_node);
        self.proof_term_iri
            .insert(step.name.clone(), conclusion_iri.to_owned());
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
    fn emit_verification(&mut self, key: &str, proof: &str, rendered: &str, thm_verified: bool) {
        let (standpoint, fresh) = self.mint("checker", "tstp-structural-checker");
        if fresh {
            self.sink.typed(&standpoint, &gmeow("Standpoint"));
            self.label(
                &standpoint,
                "the gmeow-math-lift TSTP derivation checker: it accepts a derivation whose \
                 dependency graph is well-founded (every cited parent introduced, no cycle, one \
                 terminal conclusion), and reports it as verified only when every inference \
                 declares status(thm); it does NOT re-derive the inferences",
            );
        }

        let (result, _) = self.mint("verification", key);
        self.sink.typed(&result, &math("FormalVerificationResult"));
        let (outcome, verdict) = if thm_verified {
            (
                "verificationPassed",
                format!(
                    "{rendered}: accepted as a well-founded TSTP derivation whose every \
                         inference declares status(thm)"
                ),
            )
        } else {
            (
                "verificationUnknown",
                format!(
                    "{rendered}: a well-founded TSTP derivation, but at least one inference \
                     declares an SZS status other than thm (or none at all), so this structural \
                     checker neither accepts nor rejects it — not proved is not refuted"
                ),
            )
        };
        self.label(&result, &verdict);
        self.sink
            .iri(&result, &math("verificationResult"), &math(outcome));
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

    // -- conclusions -----------------------------------------------------------

    /// A step's conclusion, as a `math:MathematicalExpression` AST.
    fn emit_conclusion(&mut self, conclusion: &Conclusion) -> (StructNode, String) {
        match conclusion {
            Conclusion::Clause(clause) => self.emit_clause(clause),
            Conclusion::Formula(formula) => {
                let mut scope = Vec::new();
                self.emit_formula(formula, &mut scope)
            }
        }
    }

    /// A CNF clause: a flat disjunction, with no binder — its universal closure is implicit.
    fn emit_clause(&mut self, clause: &Clause) -> (StructNode, String) {
        let [single] = clause.literals.as_slice() else {
            let parts: Vec<(StructNode, String)> = clause
                .literals
                .iter()
                .map(|literal| self.emit_literal(literal))
                .collect();
            let nodes: Vec<StructNode> = parts.iter().map(|(node, _)| *node).collect();
            let node = self.app("tstp:connective:or", &nodes);
            let (iri, fresh) = self.scoped_expression(node, &[]);
            if fresh {
                self.sink.typed(&iri, &math("ApplicationExpression"));
                let operation = self.emit_operation("connective", "or", Connective::Or.label());
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
        let (base, base_iri) = match &literal.equated {
            None => self.emit_term(&literal.atom, &[]),
            Some(right) => {
                let rendered = format!("{} = {}", literal.atom.render(), right.render());
                self.emit_equation(&literal.atom, right, &rendered, &[])
            }
        };
        if !literal.negated {
            return (base, base_iri);
        }
        self.emit_negation(base, &base_iri, &literal.render(), &[])
    }

    /// `l = r`, as an application of the equality operator.
    ///
    /// Equality is a first-class literal shape in TPTP, not a predicate spelt `=`, so it
    /// gets its own operator identity rather than riding as a functor named `=` in
    /// argument position.
    fn emit_equation(
        &mut self,
        left: &Term,
        right: &Term,
        rendered: &str,
        scope: &[Binding],
    ) -> (StructNode, String) {
        let (left_node, left_iri) = self.emit_term(left, scope);
        let (right_node, right_iri) = self.emit_term(right, scope);
        let node = self.app("tstp:connective:equal", &[left_node, right_node]);
        let (iri, fresh) = self.scoped_expression(node, scope);
        if fresh {
            self.sink.typed(&iri, &math("ApplicationExpression"));
            let operation = self.emit_operation("connective", "equal", "equality (=)");
            self.sink.iri(&iri, &math("operator"), &operation);
            self.emit_slots(&iri, &[left_iri, right_iri]);
            self.label(&iri, rendered);
            self.proof_structures += 1;
        }
        (node, iri)
    }

    fn emit_negation(
        &mut self,
        inner: StructNode,
        inner_iri: &str,
        rendered: &str,
        scope: &[Binding],
    ) -> (StructNode, String) {
        let node = self.app("tstp:connective:not", &[inner]);
        let (iri, fresh) = self.scoped_expression(node, scope);
        if fresh {
            self.sink.typed(&iri, &math("ApplicationExpression"));
            let operation = self.emit_operation("connective", "not", "logical negation (~)");
            self.sink.iri(&iri, &math("operator"), &operation);
            self.emit_slots(&iri, &[inner_iri.to_owned()]);
            self.label(&iri, rendered);
            self.proof_structures += 1;
        }
        (node, iri)
    }

    // -- first-order formulas --------------------------------------------------

    fn emit_formula(
        &mut self,
        formula: &Formula,
        scope: &mut Vec<Binding>,
    ) -> (StructNode, String) {
        match formula {
            Formula::Atom(term) => self.emit_term(term, scope),
            Formula::Equation {
                negated,
                left,
                right,
            } => {
                let rendered = format!("{} = {}", left.render(), right.render());
                let (node, iri) = self.emit_equation(left, right, &rendered, scope);
                if !*negated {
                    return (node, iri);
                }
                self.emit_negation(node, &iri, &formula.render(), scope)
            }
            Formula::Not(inner) => {
                let (node, iri) = self.emit_formula(inner, scope);
                let rendered = formula.render();
                self.emit_negation(node, &iri, &rendered, scope)
            }
            Formula::Binary {
                connective,
                left,
                right,
            } => {
                let (left_node, left_iri) = self.emit_formula(left, scope);
                let (right_node, right_iri) = self.emit_formula(right, scope);
                let node = self.app(
                    &format!("tstp:connective:{}", connective.slug()),
                    &[left_node, right_node],
                );
                let (iri, fresh) = self.scoped_expression(node, scope);
                if fresh {
                    self.sink.typed(&iri, &math("ApplicationExpression"));
                    let operation =
                        self.emit_operation("connective", connective.slug(), connective.label());
                    self.sink.iri(&iri, &math("operator"), &operation);
                    self.emit_slots(&iri, &[left_iri, right_iri]);
                    self.label(&iri, &formula.render());
                    self.proof_structures += 1;
                }
                (node, iri)
            }
            Formula::Quantified {
                quantifier,
                variables,
                body,
            } => self.emit_binder(*quantifier, variables, body, scope),
        }
    }

    /// One `math:BindingExpression` per bound variable.
    ///
    /// `math:BindingExpression` names at most one `math:boundVariable`, so `! [X, Y] : F`
    /// nests: the outer binder's body is the inner binder. Each level's declaration and
    /// occurrence are keyed on THAT binder's content key, which is why the binder's arena
    /// node is built (bottom-up) before its body's RDF is emitted (top-down).
    fn emit_binder(
        &mut self,
        quantifier: Quantifier,
        variables: &[String],
        body: &Formula,
        scope: &mut Vec<Binding>,
    ) -> (StructNode, String) {
        let Some((first, rest)) = variables.split_first() else {
            return self.emit_formula(body, scope);
        };

        let mut depth: Vec<String> = scope.iter().map(|b| b.name.clone()).collect();
        depth.push(first.clone());
        let inner = if rest.is_empty() {
            self.formula_node(body, &mut depth)
        } else {
            self.quantified_node(quantifier, rest, body, &mut depth)
        };
        depth.pop();
        let node = self.binder_node(quantifier, inner);

        let (iri, fresh) = self.scoped_expression(node, scope);
        if !fresh {
            return (node, iri);
        }

        let key = self.key_of(node).into_string();
        let chain = if scope.is_empty() {
            key.clone()
        } else {
            format!("{}/{key}", scope_chain(scope))
        };
        let (declaration, _) = self.mint("binder-declaration", &chain);
        let (occurrence, _) = self.mint("binder-occurrence", &chain);

        self.sink.typed(&iri, &math("BindingExpression"));
        self.label(
            &iri,
            &format!(
                "{} [{}] : {}",
                quantifier.as_str(),
                variables.join(", "),
                body.render()
            ),
        );
        let operation = self.emit_operation("quantifier", quantifier.slug(), quantifier.label());
        self.sink.iri(&iri, &math("operator"), &operation);
        // A binder introduces its variable's scoped identity and binds the occurrences of
        // it, never the glyph — which is what makes shadowing resolvable.
        self.sink.typed(&declaration, &math("VariableDeclaration"));
        self.label(&declaration, first);
        self.sink.typed(&occurrence, &math("VariableOccurrence"));
        self.sink
            .iri(&occurrence, &math("declaredVariable"), &declaration);
        self.sink.iri(&occurrence, &math("occursInScope"), &iri);
        self.sink.iri(&iri, &math("boundVariable"), &declaration);
        self.sink.iri(&iri, &math("bindsOccurrence"), &occurrence);

        scope.push(Binding {
            name: first.clone(),
            key,

            occurrence,
        });
        let (_, body_iri) = if rest.is_empty() {
            self.emit_formula(body, scope)
        } else {
            self.emit_binder(quantifier, rest, body, scope)
        };
        scope.pop();
        self.emit_slots(&iri, &[body_iri]);
        self.proof_structures += 1;
        (node, iri)
    }

    /// The arena node of a binder over `body`, with no RDF.
    fn binder_node(&mut self, quantifier: Quantifier, body: StructNode) -> StructNode {
        let operator = self.atom(&format!("tstp:quantifier:{}", quantifier.slug()));
        let sort = self.atom(INDIVIDUAL_SORT);
        self.arena
            .intern_binder(operator, &[sort], body)
            .expect("every node was minted by this lift's own arena")
    }

    /// The arena node of a formula, with no RDF.
    ///
    /// The RDF walk needs a binder's own content key BEFORE it may mint that binder's
    /// declaration and occurrence, and the key depends on the body — so the arena term is
    /// built bottom-up here and the graph is written top-down by [`Lift::emit_formula`].
    fn formula_node(&mut self, formula: &Formula, depth: &mut Vec<String>) -> StructNode {
        match formula {
            Formula::Atom(term) => self.term_node(term, depth),
            Formula::Equation {
                negated,
                left,
                right,
            } => {
                let left_node = self.term_node(left, depth);
                let right_node = self.term_node(right, depth);
                let equation = self.app("tstp:connective:equal", &[left_node, right_node]);
                if *negated {
                    self.app("tstp:connective:not", &[equation])
                } else {
                    equation
                }
            }
            Formula::Not(inner) => {
                let node = self.formula_node(inner, depth);
                self.app("tstp:connective:not", &[node])
            }
            Formula::Binary {
                connective,
                left,
                right,
            } => {
                let left_node = self.formula_node(left, depth);
                let right_node = self.formula_node(right, depth);
                self.app(
                    &format!("tstp:connective:{}", connective.slug()),
                    &[left_node, right_node],
                )
            }
            Formula::Quantified {
                quantifier,
                variables,
                body,
            } => self.quantified_node(*quantifier, variables, body, depth),
        }
    }

    fn quantified_node(
        &mut self,
        quantifier: Quantifier,
        variables: &[String],
        body: &Formula,
        depth: &mut Vec<String>,
    ) -> StructNode {
        let Some((first, rest)) = variables.split_first() else {
            return self.formula_node(body, depth);
        };
        depth.push(first.clone());
        let inner = if rest.is_empty() {
            self.formula_node(body, depth)
        } else {
            self.quantified_node(quantifier, rest, body, depth)
        };
        depth.pop();
        self.binder_node(quantifier, inner)
    }

    /// The arena node of a term, with no RDF.
    ///
    /// A variable the enclosing binders bind interns at its de-Bruijn distance, so
    /// α-equivalent formulas are ONE arena term; a variable no binder binds interns by name,
    /// which is how a CNF clause's implicitly-closed variables have always been carried.
    fn term_node(&mut self, term: &Term, depth: &[String]) -> StructNode {
        match term {
            Term::Variable(name) => match depth.iter().rposition(|bound| bound == name) {
                Some(index) => {
                    let distance = u32::try_from(depth.len() - 1 - index).unwrap_or(u32::MAX);
                    self.arena.intern_bound(distance, 0)
                }
                None => self
                    .arena
                    .intern_free(TermValue::simple_literal(format!("tstp:var:{name}"))),
            },
            Term::Apply { functor, args } if args.is_empty() => {
                self.atom(&format!("tstp:sym:{functor}"))
            }
            Term::Apply { functor, args } => {
                let nodes: Vec<StructNode> =
                    args.iter().map(|arg| self.term_node(arg, depth)).collect();
                self.app(&format!("tstp:sym:{functor}"), &nodes)
            }
        }
    }

    // -- terms -----------------------------------------------------------------

    fn emit_term(&mut self, term: &Term, scope: &[Binding]) -> (StructNode, String) {
        match term {
            Term::Variable(name) => match scope.iter().rposition(|b| b.name == *name) {
                Some(index) => self.emit_bound_variable(name, scope.len() - 1 - index, scope),
                None => self.emit_free_variable(name, scope),
            },
            Term::Apply { functor, args } if args.is_empty() => self.emit_constant(functor, scope),
            Term::Apply { functor, args } => {
                let parts: Vec<(StructNode, String)> =
                    args.iter().map(|arg| self.emit_term(arg, scope)).collect();
                let nodes: Vec<StructNode> = parts.iter().map(|(node, _)| *node).collect();
                let node = self.app(&format!("tstp:sym:{functor}"), &nodes);
                let (iri, fresh) = self.scoped_expression(node, scope);
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
    fn emit_constant(&mut self, functor: &str, scope: &[Binding]) -> (StructNode, String) {
        let node = self.atom(&format!("tstp:sym:{functor}"));
        let (iri, fresh) = self.scoped_expression(node, scope);
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

    /// A variable no enclosing binder binds.
    ///
    /// Modelled exactly as the R and ONNX bridges model one — a `math:VariableExpression`
    /// over a `math:VariableOccurrence` resolving to a `math:FreeVariableDeclaration` —
    /// because `math:VariableExpression`'s own definition insists "there is no implicit free
    /// variable": an occurrence resolving to no declaration is
    /// `math:UnscopedVariableOccurrence`. The declaration is FREE and not a binder: a CNF
    /// clause's universal closure is implicit in the source, and minting a
    /// `math:BindingExpression` for a binder the derivation never writes would state a scope
    /// it does not.
    fn emit_free_variable(&mut self, name: &str, scope: &[Binding]) -> (StructNode, String) {
        let node = self
            .arena
            .intern_free(TermValue::simple_literal(format!("tstp:var:{name}")));
        let (iri, fresh) = self.scoped_expression(node, scope);
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

    /// A variable an enclosing binder binds.
    ///
    /// It resolves to THAT binder's declaration through THAT binder's occurrence — the
    /// distance is what picks the binder, so a shadowed glyph resolves to the innermost one.
    fn emit_bound_variable(
        &mut self,
        name: &str,
        distance: usize,
        scope: &[Binding],
    ) -> (StructNode, String) {
        let node = self
            .arena
            .intern_bound(u32::try_from(distance).unwrap_or(u32::MAX), 0);
        let (iri, fresh) = self.scoped_expression(node, scope);
        if fresh {
            let occurrence = scope[scope.len() - 1 - distance].occurrence.clone();
            self.sink.typed(&iri, &math("VariableExpression"));
            self.label(&iri, name);
            self.sink
                .iri(&iri, &math("variableOccurrence"), &occurrence);
            self.proof_structures += 1;
        }
        (node, iri)
    }

    /// The `math:Operation` an application applies.
    ///
    /// The label is the operator's RAW token — an inference rule's content-addressed firing
    /// IRI, a predicate's own symbol, or a connective's name — because that token is what a
    /// reconstruction of the derivation has to recover verbatim. `role` keeps the operator
    /// families apart, so a rule and a predicate that happen to share a spelling are never
    /// one individual.
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

#[path = "lift.tests.rs"]
#[cfg(test)]
mod tests;
