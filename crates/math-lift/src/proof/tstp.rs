// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The TSTP parse tier: derivation bytes → a typed [`Derivation`], no RDF.
//!
//! The grammar is the TSTP *solution* (derivation) fragment of TPTP, as a real prover
//! writes it — E, Vampire, SPASS and Z3 all emit within it:
//!
//! ```text
//! derivation      ::= annotated*
//! annotated       ::= dialect '(' name ',' role ',' body [ ',' source [ ',' useful-info ] ] ')' '.'
//! dialect         ::= 'cnf' | 'fof'
//! body(cnf)       ::= [ '(' ] literal { '|' literal } [ ')' ]
//! body(fof)       ::= the full first-order formula grammar (see [`Formula`])
//! literal         ::= [ '~' ] term [ ( '=' | '!=' ) term ]
//! term            ::= UPPER_WORD | functor [ '(' term { ',' term } ')' ]
//! source          ::= inference-record | 'file' '(' … ')' | 'theory' '(' … ')'
//!                   | 'introduced' '(' … ')' | 'creator' '(' … ')' | 'unknown' | name
//! useful-info     ::= '[' general-term { ',' general-term } ']'
//! ```
//!
//! A functor is a lower word, a `$`-word, an integer, or a **single-quoted atom** — which
//! is how a full IRI rides through TPTP without being lossily shortened, and is exactly
//! what our own reasoner emits (`'https://…/tptp#a'('https://…/reserved#witness-…')`).
//!
//! # A conclusion is a clause OR a formula, never a coerced clause
//!
//! [`Conclusion`] is a sum, not a clause with a flag. A `cnf` step concludes a
//! [`Clause`] — a flat disjunction of literals under an implicit universal closure — and a
//! `fof` step concludes a [`Formula`], which may quantify, imply, and equate. Reading
//! `! [X] : (p(X) => q(X))` as a clause would say the step concluded a disjunction of two
//! literals, which is not what it concluded, so the two shapes stay apart all the way into
//! the lift (a clause becomes a flat AST; a quantifier becomes a real binder).
//!
//! # Roles are carried, never flattened
//!
//! All fifteen TPTP formula roles parse ([`Role`]). The role is EPISTEMIC — `axiom`,
//! `negated_conjecture` and `plain` say different things about how the derivation holds
//! its formula — so the reader keeps the raw word and the lift maps it onto the `math:`
//! statement-role layer. Nothing is collapsed onto "axiom".
//!
//! # A source may point outside the derivation
//!
//! [`Source`] distinguishes the four provenance shapes TSTP actually uses: no source at
//! all, an `inference(…)` record, an EXTERNAL reference (`file(…)`, `theory(…)`,
//! `introduced(…)`, `creator(…)`, `unknown`), and a bare `<name>` DAG parent. An external
//! reference names a premise imported from outside this document; the lift carries the
//! reference itself rather than pretending the premise was derived here.
//!
//! # What still hard-fails
//!
//! | construct | outcome | why |
//! |---|---|---|
//! | `tff`/`thf`/`tcf` | [`ProofUnliftable`] | a typed or higher-order body is not a first-order formula, and reading it as one would misstate the step's conclusion and its sorts |
//! | `include` | [`ProofUnliftable`] | the included document is not here; a missing dependency is a hard fail, never a licence to lift a partial proof |
//! | a `<sources>` LIST | [`ProofUnliftable`] | it declares several independent provenances for one formula, and picking one would drop the others |
//! | a nested `inference(…)` in a parent list | [`ProofUnliftable`] | an inline sub-derivation is a second, anonymous step identity this AST does not mint |
//! | an `<external_source>` in a parent list | LIFTED as [`Step::external_parents`] | `theory(equality)` rides on every E equality inference; it warrants the step without being one, and carries no sub-proof to flatten |
//! | an unrecognised source functor | [`ProofUnliftable`] | a source form the TPTP grammar does not define is not provenance this reader may guess at |
//!
//! Malformed *syntax* — an unterminated quoted atom or block comment, a missing `.`, an
//! unexpected token, a stray character, a duplicate formula name, `&` mixed with `|` at
//! one level — is [`TstpParse`], always with a line and column.
//!
//! # Well-foundedness is a parse-tier obligation
//!
//! A derivation whose dependency graph is not a well-founded DAG is not a proof, so
//! [`parse`] refuses it rather than handing the lift a graph to discover the problem in:
//! a parent name the document never introduces, a cycle, a document with no derived step,
//! and a document that does not end in exactly one terminal derived step are all
//! [`ProofUnliftable`]. What [`parse`] returns is therefore always a proof of ONE
//! conclusion, which is why [`Derivation::conclusion`] is infallible.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{ProofUnliftable, SourceNotUtf8, TstpParse};

// ── The derivation AST ────────────────────────────────────────────────────────

/// A term in a step's conclusion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// An upper-initial variable occurrence, e.g. `X`.
    Variable(String),
    /// A functor applied to zero or more argument terms. Arity 0 is a constant.
    ///
    /// `functor` is the atom's UNQUOTED text, so a single-quoted IRI atom holds the IRI
    /// itself rather than the quoted surface.
    Apply {
        /// The functor's unquoted text.
        functor: String,
        /// The argument terms, in source order.
        args: Vec<Term>,
    },
}

impl Term {
    /// The canonical TSTP surface of this term.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Variable(name) => name.clone(),
            Self::Apply { functor, args } => {
                let head = render_atom(functor);
                if args.is_empty() {
                    head
                } else {
                    let rendered: Vec<String> = args.iter().map(Term::render).collect();
                    format!("{head}({})", rendered.join(", "))
                }
            }
        }
    }
}

/// One literal of a clause: a predicate atom or an equation, optionally negated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    /// Whether the literal's polarity is negative.
    pub negated: bool,
    /// The literal's atom — the predicate application, or an equation's LEFT term.
    pub atom: Term,
    /// The right-hand term when the literal is an equation, `None` for a predicate atom.
    ///
    /// CNF equality is a first-class literal shape in TPTP (`f(X) = X`, `a != b`), not a
    /// predicate named `=`, so it is held apart: a lift that read `=` as an ordinary
    /// functor would put an equality symbol in argument position and lose the equation.
    pub equated: Option<Term>,
}

impl Literal {
    /// The canonical TSTP surface of this literal.
    ///
    /// A negated equation renders in TPTP's infix `!=` form rather than as `~(l = r)`; the
    /// two parse to the same literal, and rendering one of them keeps the surface canonical.
    #[must_use]
    pub fn render(&self) -> String {
        match (&self.equated, self.negated) {
            (Some(right), false) => format!("{} = {}", self.atom.render(), right.render()),
            (Some(right), true) => format!("{} != {}", self.atom.render(), right.render()),
            (None, false) => self.atom.render(),
            (None, true) => format!("~{}", self.atom.render()),
        }
    }
}

/// A CNF clause: a non-empty disjunction of literals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    /// The clause's literals, in source order. Never empty.
    pub literals: Vec<Literal>,
}

impl Clause {
    /// The canonical TSTP surface of this clause.
    #[must_use]
    pub fn render(&self) -> String {
        let rendered: Vec<String> = self.literals.iter().map(Literal::render).collect();
        rendered.join(" | ")
    }
}

/// A first-order quantifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantifier {
    /// `!` — universal.
    ForAll,
    /// `?` — existential.
    Exists,
}

impl Quantifier {
    /// The quantifier's TPTP surface token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ForAll => "!",
            Self::Exists => "?",
        }
    }

    /// A stable, word-shaped slug used to key the lift's operator identities.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::ForAll => "forall",
            Self::Exists => "exists",
        }
    }

    /// A human-readable name for the binder this quantifier introduces.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ForAll => "universal quantification (!)",
            Self::Exists => "existential quantification (?)",
        }
    }
}

/// A binary first-order connective.
///
/// Every TPTP binary connective is here: the two associative ones (`&`, `|`) and the six
/// non-associative ones (`=>`, `<=`, `<=>`, `<~>`, `~|`, `~&`). None is rewritten into
/// another — `A <~> B` is exclusive disjunction, not `~(A <=> B)` — because a derivation
/// step that concluded one of them concluded THAT one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connective {
    /// `&` — conjunction.
    And,
    /// `|` — disjunction.
    Or,
    /// `=>` — material implication.
    Imply,
    /// `<=` — converse implication.
    RevImply,
    /// `<=>` — equivalence.
    Iff,
    /// `<~>` — exclusive disjunction.
    Xor,
    /// `~|` — joint denial (NOR).
    Nor,
    /// `~&` — alternative denial (NAND).
    Nand,
}

impl Connective {
    /// The connective's TPTP surface token.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::And => "&",
            Self::Or => "|",
            Self::Imply => "=>",
            Self::RevImply => "<=",
            Self::Iff => "<=>",
            Self::Xor => "<~>",
            Self::Nor => "~|",
            Self::Nand => "~&",
        }
    }

    /// A stable, word-shaped slug used to key the lift's operator identities.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::And => "and",
            Self::Or => "or",
            Self::Imply => "imply",
            Self::RevImply => "rev-imply",
            Self::Iff => "iff",
            Self::Xor => "xor",
            Self::Nor => "nor",
            Self::Nand => "nand",
        }
    }

    /// A human-readable name for the operation this connective applies.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::And => "logical conjunction (&)",
            Self::Or => "logical disjunction (|)",
            Self::Imply => "material implication (=>)",
            Self::RevImply => "converse implication (<=)",
            Self::Iff => "logical equivalence (<=>)",
            Self::Xor => "exclusive disjunction (<~>)",
            Self::Nor => "joint denial (~|)",
            Self::Nand => "alternative denial (~&)",
        }
    }
}

/// A general first-order formula — the body of a `fof` annotated formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Formula {
    /// A predicate application or a defined atom (`$true`, `$false`).
    Atom(Term),
    /// An equation `s = t`, or a disequation `s != t` when `negated`.
    Equation {
        /// Whether the source wrote `!=` rather than `=`.
        negated: bool,
        /// The left-hand term.
        left: Term,
        /// The right-hand term.
        right: Term,
    },
    /// `~F`.
    Not(Box<Formula>),
    /// A binary connective applied to two formulas.
    Binary {
        /// The connective.
        connective: Connective,
        /// The left operand.
        left: Box<Formula>,
        /// The right operand.
        right: Box<Formula>,
    },
    /// A quantifier binding a non-empty variable list over a body.
    Quantified {
        /// The quantifier.
        quantifier: Quantifier,
        /// The bound variable names, in source order. Never empty.
        variables: Vec<String>,
        /// The quantifier's body.
        body: Box<Formula>,
    },
}

impl Formula {
    /// The canonical TSTP surface of this formula.
    ///
    /// Every binary node is parenthesized, so the surface re-parses to an equal AST
    /// without depending on precedence or on TPTP's ban on mixing `&` with `|`. That
    /// fidelity is what lets the lift carry the rendered conclusion as the fact a
    /// reconstruction reads back.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Atom(term) => term.render(),
            Self::Equation {
                negated,
                left,
                right,
            } => format!(
                "{} {} {}",
                left.render(),
                if *negated { "!=" } else { "=" },
                right.render()
            ),
            Self::Not(inner) => format!("~{}", inner.render()),
            Self::Binary {
                connective,
                left,
                right,
            } => format!(
                "({} {} {})",
                left.render(),
                connective.as_str(),
                right.render()
            ),
            Self::Quantified {
                quantifier,
                variables,
                body,
            } => format!(
                "{} [{}] : {}",
                quantifier.as_str(),
                variables.join(", "),
                body.render()
            ),
        }
    }
}

/// What a derivation step concludes.
///
/// A sum rather than one coerced shape: a `cnf` step concludes a flat disjunction of
/// literals whose universal closure is implicit, and a `fof` step concludes a formula that
/// may carry its own binders and connectives. Which of the two it is also fixes the
/// dialect keyword the step renders under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conclusion {
    /// A CNF clause, from a `cnf(…)` annotated formula.
    Clause(Clause),
    /// A general first-order formula, from a `fof(…)` annotated formula.
    Formula(Formula),
}

impl Conclusion {
    /// The canonical TSTP surface of the conclusion.
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::Clause(clause) => clause.render(),
            Self::Formula(formula) => formula.render(),
        }
    }

    /// The TPTP dialect keyword an annotated formula with this conclusion is written under.
    #[must_use]
    pub fn dialect(&self) -> &'static str {
        match self {
            Self::Clause(_) => "cnf",
            Self::Formula(_) => "fof",
        }
    }
}

/// The formula role of a derivation step — the full TPTP set.
///
/// A role is EPISTEMIC: it says how the derivation holds the formula, not what the formula
/// says. All fifteen are read and kept as themselves; the lift maps each onto the `math:`
/// statement-role layer and carries the raw word alongside, because several TPTP roles
/// share one `math:StatementRole` value and flattening them would lose the distinction the
/// prover drew.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// `axiom` — a stated law of the problem's theory.
    Axiom,
    /// `hypothesis` — a problem-specific assertion taken as given.
    Hypothesis,
    /// `definition` — a stipulation introducing a symbol.
    Definition,
    /// `assumption` — an assertion taken for the derivation, to be discharged.
    Assumption,
    /// `lemma` — an auxiliary result proved en route.
    Lemma,
    /// `theorem` — a proved consequence of the axioms.
    Theorem,
    /// `corollary` — a result following readily from a theorem.
    Corollary,
    /// `conjecture` — the goal, held under test.
    Conjecture,
    /// `negated_conjecture` — the goal's negation, asserted so that deriving a
    /// contradiction from it refutes it and thereby establishes the conjecture.
    NegatedConjecture,
    /// `plain` — a formula with no declared user semantics; a prover's working step.
    Plain,
    /// `type` — a symbol's type declaration.
    Type,
    /// `fi_domain` — a finite-interpretation formula fixing the model's domain.
    FiDomain,
    /// `fi_functors` — a finite-interpretation formula fixing the model's functions.
    FiFunctors,
    /// `fi_predicates` — a finite-interpretation formula fixing the model's predicates.
    FiPredicates,
    /// `unknown` — the source declares no role.
    Unknown,
}

impl Role {
    /// The role's TPTP surface word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Axiom => "axiom",
            Self::Hypothesis => "hypothesis",
            Self::Definition => "definition",
            Self::Assumption => "assumption",
            Self::Lemma => "lemma",
            Self::Theorem => "theorem",
            Self::Corollary => "corollary",
            Self::Conjecture => "conjecture",
            Self::NegatedConjecture => "negated_conjecture",
            Self::Plain => "plain",
            Self::Type => "type",
            Self::FiDomain => "fi_domain",
            Self::FiFunctors => "fi_functors",
            Self::FiPredicates => "fi_predicates",
            Self::Unknown => "unknown",
        }
    }

    /// The role named by a TPTP word, or `None` when the word is not a TPTP role.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Some(match word {
            "axiom" => Self::Axiom,
            "hypothesis" => Self::Hypothesis,
            "definition" => Self::Definition,
            "assumption" => Self::Assumption,
            "lemma" => Self::Lemma,
            "theorem" => Self::Theorem,
            "corollary" => Self::Corollary,
            "conjecture" => Self::Conjecture,
            "negated_conjecture" => Self::NegatedConjecture,
            "plain" => Self::Plain,
            "type" => Self::Type,
            "fi_domain" => Self::FiDomain,
            "fi_functors" => Self::FiFunctors,
            "fi_predicates" => Self::FiPredicates,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }

    /// Whether the role holds its formula as a FOUNDATION of the theory — a law the
    /// derivation rests on rather than a claim it is testing or a step it derived.
    ///
    /// Only these three become a `math:Axiom` in the lift. `negated_conjecture` in
    /// particular is deliberately excluded: it is asserted so that refuting it establishes
    /// the conjecture, and typing it as a law would state the opposite of what the
    /// derivation claims.
    #[must_use]
    pub fn is_foundational(self) -> bool {
        matches!(self, Self::Axiom | Self::Hypothesis | Self::Assumption)
    }
}

/// An external provenance form: the source names a premise from OUTSIDE the derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSource {
    /// The source functor — `file`, `theory`, `introduced`, `creator`, or `unknown`.
    pub functor: String,
    /// The source's exact rendered TSTP surface, e.g. `file('SET001-1.p', ax7)`.
    ///
    /// The reference is carried VERBATIM rather than decomposed: `file`, `theory`,
    /// `introduced` and `creator` each take their own argument shapes, and a lift that
    /// reduced them to one normalized pair would be inventing a common structure the
    /// grammar does not have.
    pub rendered: String,
}

/// How a step's `<source>` field justifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// No `<source>` field at all: the formula is asserted with no stated provenance.
    Asserted,
    /// An `inference(rule, [status…], [parent…])` record.
    Inference {
        /// The inference rule's unquoted name.
        rule: String,
        /// The rendered status terms, in source order — e.g. `["status(thm)"]`.
        status: Vec<String>,
    },
    /// An external reference: `file(…)`, `theory(…)`, `introduced(…)`, `creator(…)`, or
    /// the bare word `unknown`.
    External(ExternalSource),
    /// A bare `<name>` DAG source: the formula comes from the named formula with no
    /// declared rule (a rename, a copy, or a re-statement).
    Parent,
}

/// One annotated formula of a derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The step's name — its identity within the derivation, and what a parent list cites.
    pub name: String,
    /// The step's role.
    pub role: Role,
    /// What the step concludes.
    pub conclusion: Conclusion,
    /// How the step is justified.
    pub source: Source,
    /// The parent step names the source cites, in source order.
    ///
    /// Non-empty only for [`Source::Inference`] (its parent list) and [`Source::Parent`]
    /// (the single named formula). Held as a field rather than inside [`Source`] because
    /// the well-foundedness checks and the dependency walk read it uniformly.
    pub parents: Vec<String>,
    /// The EXTERNAL references cited in the inference's parent list, in source order.
    ///
    /// TPTP's `<parent_info>` admits an `<external_source>`, and E writes one on every
    /// equality-using inference (`rw`, `spm`, `sr`, `cn` all cite `theory(equality)`).
    /// Such a citation names a warrant the derivation did not derive — it is NOT a step,
    /// so it never enters [`Step::parents`], which must resolve against step names for the
    /// well-foundedness walk.
    pub external_parents: Vec<ExternalSource>,
    /// The rendered terms of the `<useful_info>` 5th field, in source order. Empty when
    /// the field is absent.
    pub useful_info: Vec<String>,
}

impl Step {
    /// Whether this step is justified by something INSIDE the derivation.
    ///
    /// True for an `inference(…)` record and for a bare `<name>` DAG source. An asserted
    /// formula and one whose source points outside the document are both leaves: nothing
    /// in this derivation derived them.
    #[must_use]
    pub fn is_derived(&self) -> bool {
        matches!(self.source, Source::Inference { .. } | Source::Parent)
    }

    /// The inference rule that licenses the step, or `None` when it declares none.
    ///
    /// A bare `<name>` DAG source is derived but names no rule, and so is `None` here: the
    /// source states a parent, not a calculus step, and inventing a rule name for it would
    /// put a token in the graph the derivation never wrote.
    #[must_use]
    pub fn rule(&self) -> Option<&str> {
        match &self.source {
            Source::Inference { rule, .. } => Some(rule),
            _ => None,
        }
    }

    /// The rendered status terms of the step's `inference(…)`, or an empty slice.
    #[must_use]
    pub fn status(&self) -> &[String] {
        match &self.source {
            Source::Inference { status, .. } => status,
            _ => &[],
        }
    }

    /// Whether the step's inference declares the SZS theorem status.
    ///
    /// The whole-derivation verification claim rests on this: a step declaring `esa`
    /// (equisatisfiable) or `cth` (counter-theorem) has not been asserted to preserve
    /// theoremhood, so a checker may not report the derivation as accepted on its account.
    #[must_use]
    pub fn declares_thm_status(&self) -> bool {
        self.status().iter().any(|s| s == "status(thm)")
    }

    /// The step's canonical TSTP surface, one full annotated formula.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!(
            "{}({}, {}, {}",
            self.conclusion.dialect(),
            render_atom(&self.name),
            self.role.as_str(),
            self.conclusion.render()
        );
        match &self.source {
            Source::Asserted => {}
            Source::Inference { rule, status } => {
                // Step citations first, then the external warrants — the order E writes
                // them, and the order this reader split them apart in. Re-emitting the
                // externals is what keeps render ∘ parse the identity: dropping them here
                // would make the round-trip that defends the SectionRetraction rung a lie.
                let mut parents: Vec<String> =
                    self.parents.iter().map(|p| render_atom(p)).collect();
                parents.extend(self.external_parents.iter().map(|x| x.rendered.clone()));
                out.push_str(&format!(
                    ", inference({}, [{}], [{}])",
                    render_atom(rule),
                    status.join(", "),
                    parents.join(", ")
                ));
            }
            Source::External(external) => {
                out.push_str(&format!(", {}", external.rendered));
            }
            Source::Parent => {
                let parent = self
                    .parents
                    .first()
                    .map_or(String::new(), |p| render_atom(p));
                out.push_str(&format!(", {parent}"));
            }
        }
        if !self.useful_info.is_empty() {
            out.push_str(&format!(", [{}]", self.useful_info.join(", ")));
        }
        out.push_str(").");
        out
    }
}

/// A parsed, well-founded TSTP derivation.
///
/// Every invariant the lift depends on is established by [`parse`] and holds for the whole
/// lifetime of the value: names are unique, every cited parent is introduced, the dependency
/// graph is acyclic, at least one step is derived, and exactly one step is terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derivation {
    steps: Vec<Step>,
    index: BTreeMap<String, usize>,
    conclusion: usize,
}

impl Derivation {
    /// Every step, in source order.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The step a name cites, if the derivation introduces it.
    #[must_use]
    pub fn step(&self, name: &str) -> Option<&Step> {
        self.index.get(name).map(|&i| &self.steps[i])
    }

    /// The derivation's single terminal step — the conclusion it proves.
    ///
    /// Infallible: uniqueness and derivedness are checked in [`parse`].
    #[must_use]
    pub fn conclusion(&self) -> &Step {
        &self.steps[self.conclusion]
    }

    /// Every step index in DEPENDENCY order: a step never precedes one of its parents.
    ///
    /// Source order is not dependency order — TSTP does not require a step to be written
    /// after the steps it cites — so a consumer that folds over the derivation (the lift
    /// building each step's proof term from its parents') walks this instead. Deterministic:
    /// a depth-first post-order rooted at each step in source order, which is a pure
    /// function of the AST, so the lift stays idempotent.
    #[must_use]
    pub fn dependency_order(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.steps.len());
        let mut placed = vec![false; self.steps.len()];
        // Explicit stack: the derivation is untrusted input, and a deep chain must not be
        // able to blow the caller's stack.
        let mut work: Vec<(usize, usize)> = Vec::new();
        for start in 0..self.steps.len() {
            if placed[start] {
                continue;
            }
            work.push((start, 0));
            while let Some((node, cursor)) = work.pop() {
                if let Some(parent) = self.steps[node].parents.get(cursor) {
                    work.push((node, cursor + 1));
                    let next = self.index[parent];
                    if !placed[next] {
                        work.push((next, 0));
                    }
                } else if !placed[node] {
                    placed[node] = true;
                    order.push(node);
                }
            }
        }
        order
    }

    /// The canonical text of the whole derivation — one rendered annotated formula per
    /// line, in source order.
    ///
    /// A pure function of the AST, so it is the content address the lift mints the proof,
    /// the dependency graph, and the verification triangle under.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for step in &self.steps {
            out.push_str(&step.render());
            out.push('\n');
        }
        out
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Parse a TSTP derivation.
///
/// # Errors
///
/// - [`SourceNotUtf8`] when `source` is not valid UTF-8.
/// - [`TstpParse`], with a line and column, for malformed syntax: an unterminated quoted
///   atom or block comment, a stray character, an unexpected token, a missing terminator,
///   or two formulas sharing one name.
/// - [`ProofUnliftable`] for a well-formed document that carries no liftable proof: a
///   construct this reader does not structure (see the module doc), a parent name the
///   document never introduces, a cycle, no derived step, or more than one terminal step.
pub fn parse(source: &[u8]) -> gmeow_errors::Result<Derivation> {
    let text = std::str::from_utf8(source).map_err(|e| {
        gmeow_errors::Diag::of_kind(SourceNotUtf8 {
            detail: format!(
                "the TSTP derivation is not valid UTF-8 (invalid byte sequence at offset {}); a \
                 TPTP document is text, and this bridge will not guess an encoding",
                e.valid_up_to()
            ),
        })
    })?;
    let tokens = lex(text)?;
    let mut parser = Parser {
        toks: &tokens,
        pos: 0,
        end: end_position(text),
    };
    let mut steps = Vec::new();
    while !parser.at_end() {
        steps.push(parser.annotated_formula()?);
    }
    seal(steps)
}

/// Check the whole-document obligations and freeze the derivation.
fn seal(steps: Vec<Step>) -> gmeow_errors::Result<Derivation> {
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, step) in steps.iter().enumerate() {
        if index.insert(step.name.clone(), i).is_some() {
            return Err(gmeow_errors::Diag::of_kind(TstpParse {
                detail: format!(
                    "the derivation introduces the formula name `{}` twice; a TSTP name IS the \
                     step's identity, so a second definition leaves every parent citing it \
                     ambiguous",
                    step.name
                ),
            }));
        }
    }

    // Every cited parent must be introduced. A dangling parent is not a syntax slip: the
    // step it names carries the premises the inference consumed, so the derivation has no
    // well-founded proof at all.
    for step in &steps {
        for parent in &step.parents {
            if !index.contains_key(parent) {
                return Err(unliftable(format!(
                    "step `{}` cites the parent `{parent}`, which the derivation never \
                     introduces; a proof step whose premise is absent is not a well-founded \
                     derivation, and the lift will not mint a placeholder premise for it",
                    step.name
                )));
            }
        }
    }

    if let Some(cycle) = find_cycle(&steps, &index) {
        return Err(unliftable(format!(
            "the derivation's dependency graph contains the cycle {}; a proof is a well-founded \
             DAG, and a step that (transitively) depends on itself proves nothing",
            cycle.join(" → ")
        )));
    }

    if !steps.iter().any(Step::is_derived) {
        return Err(unliftable(
            "the derivation contains no derived step: every formula is a leaf the document \
             asserts or imports, so there is no inference to lift into the math: proof layer \
             and no proof to hold a math:FormalVerificationResult about"
                .to_owned(),
        ));
    }

    let cited: BTreeSet<&str> = steps
        .iter()
        .flat_map(|s| s.parents.iter().map(String::as_str))
        .collect();
    let terminals: Vec<&Step> = steps
        .iter()
        .filter(|s| !cited.contains(s.name.as_str()))
        .collect();
    let [terminal] = terminals.as_slice() else {
        let names: Vec<&str> = terminals.iter().map(|s| s.name.as_str()).collect();
        return Err(unliftable(format!(
            "the derivation has {} terminal steps ({}); a math:Proof proves ONE goal through \
             math:provesGoal, so a document holding several independent conclusions is several \
             proofs and must be lifted as several derivations",
            terminals.len(),
            names.join(", ")
        )));
    };
    // The single terminal is necessarily DERIVED, and that is a theorem about the checks
    // above rather than a case to handle: the derived steps form a finite acyclic
    // sub-graph, so at least one of them is cited by nothing; were the sole terminal an
    // asserted leaf, every derived step would be cited by another, and following the
    // citations through a finite set would close a cycle the acyclicity check already
    // refused. The `no derived step` check supplies the "at least one" half.
    debug_assert!(
        terminal.is_derived(),
        "the unique terminal of an acyclic derivation with a derived step is derived"
    );
    let conclusion = index[&terminal.name];

    Ok(Derivation {
        steps,
        index,
        conclusion,
    })
}

/// The first dependency cycle, as the step names along it, or `None` when the graph is a DAG.
fn find_cycle(steps: &[Step], index: &BTreeMap<String, usize>) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Unvisited,
        OnStack,
        Done,
    }
    let mut mark = vec![Mark::Unvisited; steps.len()];
    let mut path: Vec<usize> = Vec::new();
    // An explicit stack rather than recursion: a derivation is untrusted input, and a deep
    // chain must not be able to blow the parser's own stack.
    let mut work: Vec<(usize, usize)> = Vec::new();
    for start in 0..steps.len() {
        if mark[start] != Mark::Unvisited {
            continue;
        }
        work.push((start, 0));
        mark[start] = Mark::OnStack;
        path.push(start);
        while let Some((node, cursor)) = work.pop() {
            if let Some(parent) = steps[node].parents.get(cursor) {
                work.push((node, cursor + 1));
                let next = index[parent];
                match mark[next] {
                    Mark::OnStack => {
                        // `next` is on the stack, so it IS in `path`; `unwrap_or(0)` keeps
                        // the walk total rather than turning a found cycle into "no cycle".
                        let from = path.iter().position(|&n| n == next).unwrap_or(0);
                        let mut cycle: Vec<String> = path[from..]
                            .iter()
                            .map(|&n| steps[n].name.clone())
                            .collect();
                        cycle.push(steps[next].name.clone());
                        return Some(cycle);
                    }
                    Mark::Done => {}
                    Mark::Unvisited => {
                        mark[next] = Mark::OnStack;
                        path.push(next);
                        work.push((next, 0));
                    }
                }
            } else {
                mark[node] = Mark::Done;
                path.pop();
            }
        }
    }
    None
}

// ── Rendering helpers ─────────────────────────────────────────────────────────

/// Whether an atom's text is a bare TPTP word needing no quoting.
fn is_bare_word(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first.is_ascii_digit() {
        return text.chars().all(|c| c.is_ascii_digit());
    }
    if first == '$' {
        let Some(second) = chars.next() else {
            return false;
        };
        return (second.is_ascii_lowercase() || second == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    }
    first.is_ascii_lowercase() && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// An atom's canonical TPTP surface: bare when it is a word, single-quoted otherwise.
///
/// The inverse of the lexer's unescaping, so a rendered atom re-lexes to the same text.
#[must_use]
pub fn render_atom(text: &str) -> String {
    if is_bare_word(text) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        if c == '\\' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

// ── Lexer ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Colon,
    Tilde,
    Pipe,
    Amp,
    Bang,
    Query,
    Eq,
    NotEq,
    Arrow,
    RevArrow,
    Iff,
    Xor,
    Nor,
    Nand,
    /// A lower word or a `$`-word.
    Lower(String),
    /// A single-quoted atom, unescaped.
    Quoted(String),
    /// An upper-initial word — a variable.
    Upper(String),
    /// An unsigned integer.
    Number(String),
    /// Any other character.
    ///
    /// Lexing is TOTAL so that an out-of-fragment dialect body (a `tff` type's `$i > $o`)
    /// survives to the dialect keyword the parser refuses BY NAME. A lexer that stopped at
    /// the first `>` would report a stray character where the real answer is "this bridge
    /// reads `cnf` and `fof` derivation steps".
    Other(char),
}

impl Tok {
    /// How the token reads back in a diagnostic.
    fn describe(&self) -> String {
        let punct = match self {
            Self::LParen => "(",
            Self::RParen => ")",
            Self::LBracket => "[",
            Self::RBracket => "]",
            Self::Comma => ",",
            Self::Dot => ".",
            Self::Colon => ":",
            Self::Tilde => "~",
            Self::Pipe => "|",
            Self::Amp => "&",
            Self::Bang => "!",
            Self::Query => "?",
            Self::Eq => "=",
            Self::NotEq => "!=",
            Self::Arrow => "=>",
            Self::RevArrow => "<=",
            Self::Iff => "<=>",
            Self::Xor => "<~>",
            Self::Nor => "~|",
            Self::Nand => "~&",
            Self::Lower(w) | Self::Number(w) => return format!("`{w}`"),
            Self::Quoted(w) => return format!("the quoted atom `{w}`"),
            Self::Upper(w) => return format!("the variable `{w}`"),
            Self::Other(c) => return format!("`{c}`"),
        };
        format!("`{punct}`")
    }

    /// The binary connective this token spells, if it spells one.
    fn connective(&self) -> Option<Connective> {
        Some(match self {
            Self::Amp => Connective::And,
            Self::Pipe => Connective::Or,
            Self::Arrow => Connective::Imply,
            Self::RevArrow => Connective::RevImply,
            Self::Iff => Connective::Iff,
            Self::Xor => Connective::Xor,
            Self::Nor => Connective::Nor,
            Self::Nand => Connective::Nand,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
struct Token {
    tok: Tok,
    line: u32,
    col: u32,
}

/// A `(line, column)` position, 1-based, for the end of the document.
fn end_position(text: &str) -> (u32, u32) {
    let mut line = 1u32;
    let mut col = 1u32;
    for c in text.chars() {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn syntax(line: u32, col: u32, detail: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(TstpParse {
        detail: format!("line {line}, column {col}: {detail}"),
    })
}

fn unliftable(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(ProofUnliftable { detail })
}

fn lex(src: &str) -> gmeow_errors::Result<Vec<Token>> {
    let chars: Vec<char> = src.chars().collect();
    let n = chars.len();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut col = 1u32;
    let mut out: Vec<Token> = Vec::new();

    // One place that advances the cursor, so line/column can never drift from the index.
    macro_rules! step {
        () => {{
            if chars[i] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
        }};
    }

    while i < n {
        let c = chars[i];
        if c.is_whitespace() {
            step!();
            continue;
        }
        // `%` is TPTP's own line comment. `#` is not in the grammar, but it is what E
        // actually writes: `eprover --proof-object` frames its derivation in
        // `# SZS status Theorem`, `# SZS output start CNFRefutation`, and
        // `# Proof object total steps : 12`. Refusing them meant the bridge could not read
        // an unedited E proof at all — and the committed eprover fixtures were written
        // WITHOUT those lines, so they passed a parser that could not read the tool they
        // are named for. Skipping them is not leniency about the grammar; it is reading the
        // file the tool emits.
        if c == '%' || c == '#' {
            while i < n && chars[i] != '\n' {
                step!();
            }
            continue;
        }
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            let (open_line, open_col) = (line, col);
            step!();
            step!();
            loop {
                if i + 1 >= n {
                    return Err(syntax(
                        open_line,
                        open_col,
                        "unterminated block comment: the `/*` opened here is never closed by `*/`",
                    ));
                }
                if chars[i] == '*' && chars[i + 1] == '/' {
                    step!();
                    step!();
                    break;
                }
                step!();
            }
            continue;
        }

        let (start_line, start_col) = (line, col);
        let next = |offset: usize| chars.get(i + offset).copied();

        // Multi-character operators first: `<=>` must not lex as `<` then `=>`.
        let operator: Option<(Tok, usize)> = match c {
            '(' => Some((Tok::LParen, 1)),
            ')' => Some((Tok::RParen, 1)),
            '[' => Some((Tok::LBracket, 1)),
            ']' => Some((Tok::RBracket, 1)),
            ',' => Some((Tok::Comma, 1)),
            '.' => Some((Tok::Dot, 1)),
            ':' => Some((Tok::Colon, 1)),
            '&' => Some((Tok::Amp, 1)),
            '?' => Some((Tok::Query, 1)),
            '|' => Some((Tok::Pipe, 1)),
            '~' => match next(1) {
                Some('|') => Some((Tok::Nor, 2)),
                Some('&') => Some((Tok::Nand, 2)),
                _ => Some((Tok::Tilde, 1)),
            },
            '!' => match next(1) {
                Some('=') => Some((Tok::NotEq, 2)),
                _ => Some((Tok::Bang, 1)),
            },
            '=' => match next(1) {
                Some('>') => Some((Tok::Arrow, 2)),
                _ => Some((Tok::Eq, 1)),
            },
            '<' => match (next(1), next(2)) {
                (Some('='), Some('>')) => Some((Tok::Iff, 3)),
                (Some('~'), Some('>')) => Some((Tok::Xor, 3)),
                (Some('='), _) => Some((Tok::RevArrow, 2)),
                _ => None,
            },
            _ => None,
        };
        if let Some((tok, width)) = operator {
            for _ in 0..width {
                step!();
            }
            out.push(Token {
                tok,
                line: start_line,
                col: start_col,
            });
            continue;
        }

        if c == '\'' {
            step!();
            let mut text = String::new();
            loop {
                if i >= n {
                    return Err(syntax(
                        start_line,
                        start_col,
                        "unterminated single-quoted atom: the `'` opened here is never closed",
                    ));
                }
                let ch = chars[i];
                if ch == '\\' {
                    step!();
                    if i >= n {
                        return Err(syntax(
                            start_line,
                            start_col,
                            "unterminated single-quoted atom: the escape `\\` has no character \
                             after it",
                        ));
                    }
                    text.push(chars[i]);
                    step!();
                    continue;
                }
                if ch == '\'' {
                    step!();
                    break;
                }
                if ch == '\n' {
                    return Err(syntax(
                        start_line,
                        start_col,
                        "unterminated single-quoted atom: a newline reached before the closing `'`",
                    ));
                }
                text.push(ch);
                step!();
            }
            if text.is_empty() {
                return Err(syntax(
                    start_line,
                    start_col,
                    "an empty single-quoted atom `''` names nothing",
                ));
            }
            out.push(Token {
                tok: Tok::Quoted(text),
                line: start_line,
                col: start_col,
            });
            continue;
        }

        if c == '_' {
            return Err(syntax(
                start_line,
                start_col,
                "an identifier starting with `_` is neither a TPTP variable (`[A-Z]…`) nor a \
                 functor (`[a-z]…`); admitting it as a constant would change what the clause says",
            ));
        }

        if c == '$' || c.is_ascii_alphabetic() {
            let mut text = String::new();
            text.push(c);
            step!();
            while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                text.push(chars[i]);
                step!();
            }
            let tok = if c.is_ascii_uppercase() {
                Tok::Upper(text)
            } else {
                Tok::Lower(text)
            };
            out.push(Token {
                tok,
                line: start_line,
                col: start_col,
            });
            continue;
        }

        if c.is_ascii_digit() {
            let mut text = String::new();
            while i < n && chars[i].is_ascii_digit() {
                text.push(chars[i]);
                step!();
            }
            out.push(Token {
                tok: Tok::Number(text),
                line: start_line,
                col: start_col,
            });
            continue;
        }

        step!();
        out.push(Token {
            tok: Tok::Other(c),
            line: start_line,
            col: start_col,
        });
    }
    Ok(out)
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// A general TSTP annotation term — the shape a `<source>` field is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Annotation {
    Name(String),
    Func(String, Vec<Annotation>),
    List(Vec<Annotation>),
}

impl Annotation {
    fn render(&self) -> String {
        match self {
            Self::Name(name) => render_atom(name),
            Self::Func(functor, args) => {
                let rendered: Vec<String> = args.iter().map(Annotation::render).collect();
                format!("{}({})", render_atom(functor), rendered.join(", "))
            }
            Self::List(items) => {
                let rendered: Vec<String> = items.iter().map(Annotation::render).collect();
                format!("[{}]", rendered.join(", "))
            }
        }
    }
}

struct Parser<'t> {
    toks: &'t [Token],
    pos: usize,
    end: (u32, u32),
}

impl Parser<'_> {
    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    /// The position to blame for an error at the cursor — the current token's, or the end
    /// of the document when the input simply ran out.
    fn here(&self) -> (u32, u32) {
        self.toks
            .get(self.pos)
            .map_or(self.end, |t| (t.line, t.col))
    }

    fn bump(&mut self) -> gmeow_errors::Result<&Token> {
        let (line, col) = self.here();
        let token = self
            .toks
            .get(self.pos)
            .ok_or_else(|| syntax(line, col, "unexpected end of the derivation"))?;
        self.pos += 1;
        Ok(token)
    }

    fn expect(&mut self, want: &Tok) -> gmeow_errors::Result<()> {
        let wanted = want.describe();
        let token = self.bump()?;
        if &token.tok == want {
            return Ok(());
        }
        Err(syntax(
            token.line,
            token.col,
            &format!("expected {wanted}, found {}", token.tok.describe()),
        ))
    }

    /// An atomic word in name position: a lower word, a quoted atom, or an integer.
    fn atomic_word(&mut self, role: &str) -> gmeow_errors::Result<String> {
        let token = self.bump()?;
        match &token.tok {
            Tok::Lower(w) | Tok::Quoted(w) | Tok::Number(w) => Ok(w.clone()),
            other => Err(syntax(
                token.line,
                token.col,
                &format!("expected {role}, found {}", other.describe()),
            )),
        }
    }

    /// `cnf(name, role, clause[, source[, useful-info]]).`
    /// `fof(name, role, formula[, source[, useful-info]]).`
    fn annotated_formula(&mut self) -> gmeow_errors::Result<Step> {
        let (keyword, line, col) = {
            let token = self.bump()?;
            match &token.tok {
                Tok::Lower(word) => (word.clone(), token.line, token.col),
                other => {
                    return Err(syntax(
                        token.line,
                        token.col,
                        &format!(
                            "expected a `cnf(…)` or `fof(…)` annotated formula, found {}",
                            other.describe()
                        ),
                    ));
                }
            }
        };
        let first_order = match keyword.as_str() {
            "cnf" => false,
            "fof" => true,
            "tff" | "thf" | "tcf" => {
                return Err(unliftable(format!(
                    "line {line}, column {col}: the derivation uses the TYPED TPTP dialect \
                     `{keyword}`; its body carries sorts and, for `thf`, higher-order terms that \
                     the untyped first-order AST this bridge builds cannot hold, and reading it \
                     as an untyped formula would drop the typing the step depends on"
                )));
            }
            "include" => {
                return Err(unliftable(format!(
                    "line {line}, column {col}: the derivation uses an `include` directive; a \
                     proof this bridge lifts must be self-contained, and the included document \
                     is not here to be read"
                )));
            }
            other => {
                return Err(syntax(
                    line,
                    col,
                    &format!("expected `cnf` or `fof`, found `{other}`"),
                ));
            }
        };

        self.expect(&Tok::LParen)?;
        let name = self.atomic_word("a formula name")?;
        self.expect(&Tok::Comma)?;
        let role = self.role()?;
        self.expect(&Tok::Comma)?;
        let conclusion = if first_order {
            Conclusion::Formula(self.formula()?)
        } else {
            Conclusion::Clause(self.clause()?)
        };

        let mut source = Source::Asserted;
        let mut parents = Vec::new();
        let mut external_parents = Vec::new();
        let mut useful_info = Vec::new();
        if matches!(self.peek(), Some(Tok::Comma)) {
            self.bump()?;
            let (source_line, source_col) = self.here();
            let annotation = self.annotation()?;
            let (s, p, xp) = recognize_source(annotation, source_line, source_col)?;
            source = s;
            parents = p;
            external_parents = xp;
            if matches!(self.peek(), Some(Tok::Comma)) {
                self.bump()?;
                let (info_line, info_col) = self.here();
                let info = self.annotation()?;
                let Annotation::List(items) = info else {
                    return Err(syntax(
                        info_line,
                        info_col,
                        "a <useful_info> 5th field is a bracketed general list",
                    ));
                };
                useful_info = items.iter().map(Annotation::render).collect();
            }
        }
        self.expect(&Tok::RParen)?;
        self.expect(&Tok::Dot)?;

        Ok(Step {
            name,
            role,
            conclusion,
            source,
            parents,
            external_parents,
            useful_info,
        })
    }

    fn role(&mut self) -> gmeow_errors::Result<Role> {
        let token = self.bump()?;
        let (line, col) = (token.line, token.col);
        let Tok::Lower(word) = &token.tok else {
            return Err(syntax(
                line,
                col,
                &format!("expected a formula role, found {}", token.tok.describe()),
            ));
        };
        Role::from_word(word)
            .ok_or_else(|| syntax(line, col, &format!("`{word}` is not a TPTP formula role")))
    }

    // -- the CNF body ---------------------------------------------------------

    /// `[ '(' ] literal { '|' literal } [ ')' ]`
    fn clause(&mut self) -> gmeow_errors::Result<Clause> {
        let parenthesized = matches!(self.peek(), Some(Tok::LParen));
        if parenthesized {
            self.bump()?;
        }
        let mut literals = vec![self.literal()?];
        while matches!(self.peek(), Some(Tok::Pipe)) {
            self.bump()?;
            literals.push(self.literal()?);
        }
        if parenthesized {
            self.expect(&Tok::RParen)?;
        }
        Ok(Clause { literals })
    }

    fn literal(&mut self) -> gmeow_errors::Result<Literal> {
        let mut negated = matches!(self.peek(), Some(Tok::Tilde));
        if negated {
            self.bump()?;
        }
        let atom = self.term()?;
        let equated = match self.peek() {
            Some(Tok::Eq) => {
                self.bump()?;
                Some(self.term()?)
            }
            Some(Tok::NotEq) => {
                self.bump()?;
                negated = true;
                Some(self.term()?)
            }
            _ => None,
        };
        Ok(Literal {
            negated,
            atom,
            equated,
        })
    }

    // -- the FOF body ---------------------------------------------------------

    /// A full first-order formula.
    ///
    /// `&` and `|` chain left-associatively; the six non-associative connectives take
    /// exactly two unitary operands. Mixing `&` with `|` at one level without parentheses
    /// is a SYNTAX error, exactly as the TPTP grammar says — silently choosing a precedence
    /// would make this reader accept a document whose meaning it invented.
    fn formula(&mut self) -> gmeow_errors::Result<Formula> {
        let mut left = self.unitary_formula()?;
        let Some(connective) = self.peek().and_then(Tok::connective) else {
            return Ok(left);
        };
        if matches!(connective, Connective::And | Connective::Or) {
            let token = if connective == Connective::And {
                Tok::Amp
            } else {
                Tok::Pipe
            };
            while matches!(self.peek(), Some(t) if *t == token) {
                self.bump()?;
                let right = self.unitary_formula()?;
                left = Formula::Binary {
                    connective,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            }
            if let Some(next) = self.peek().and_then(Tok::connective) {
                let (line, col) = self.here();
                return Err(syntax(
                    line,
                    col,
                    &format!(
                        "`{}` may not follow a `{}` chain without parentheses; TPTP's associative \
                         connectives do not mix, and choosing a precedence here would invent a \
                         reading the source did not write",
                        next.as_str(),
                        connective.as_str()
                    ),
                ));
            }
            return Ok(left);
        }
        self.bump()?;
        let right = self.unitary_formula()?;
        if let Some(next) = self.peek().and_then(Tok::connective) {
            let (line, col) = self.here();
            return Err(syntax(
                line,
                col,
                &format!(
                    "the non-associative connective `{}` takes exactly two unitary operands, so \
                     the trailing `{}` needs parentheses",
                    connective.as_str(),
                    next.as_str()
                ),
            ));
        }
        Ok(Formula::Binary {
            connective,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    /// A unitary formula: a quantification, a negation, a parenthesized formula, or an atom.
    fn unitary_formula(&mut self) -> gmeow_errors::Result<Formula> {
        match self.peek() {
            Some(Tok::LParen) => {
                self.bump()?;
                let inner = self.formula()?;
                self.expect(&Tok::RParen)?;
                Ok(inner)
            }
            Some(Tok::Bang) => self.quantified(Quantifier::ForAll),
            Some(Tok::Query) => self.quantified(Quantifier::Exists),
            Some(Tok::Tilde) => {
                self.bump()?;
                Ok(Formula::Not(Box::new(self.unitary_formula()?)))
            }
            _ => self.atomic_formula(),
        }
    }

    /// `('!' | '?') '[' variable { ',' variable } ']' ':' unitary-formula`
    fn quantified(&mut self, quantifier: Quantifier) -> gmeow_errors::Result<Formula> {
        self.bump()?;
        self.expect(&Tok::LBracket)?;
        let mut variables = vec![self.variable_name()?];
        while matches!(self.peek(), Some(Tok::Comma)) {
            self.bump()?;
            variables.push(self.variable_name()?);
        }
        self.expect(&Tok::RBracket)?;
        self.expect(&Tok::Colon)?;
        Ok(Formula::Quantified {
            quantifier,
            variables,
            body: Box::new(self.unitary_formula()?),
        })
    }

    fn variable_name(&mut self) -> gmeow_errors::Result<String> {
        let token = self.bump()?;
        match &token.tok {
            Tok::Upper(name) => Ok(name.clone()),
            other => Err(syntax(
                token.line,
                token.col,
                &format!(
                    "expected a quantified variable (`[A-Z]…`), found {}",
                    other.describe()
                ),
            )),
        }
    }

    /// A predicate application, a defined atom, or an (in)equation.
    fn atomic_formula(&mut self) -> gmeow_errors::Result<Formula> {
        let left = self.term()?;
        let negated = match self.peek() {
            Some(Tok::Eq) => false,
            Some(Tok::NotEq) => true,
            _ => return Ok(Formula::Atom(left)),
        };
        self.bump()?;
        let right = self.term()?;
        Ok(Formula::Equation {
            negated,
            left,
            right,
        })
    }

    // -- terms and annotations ------------------------------------------------

    fn term(&mut self) -> gmeow_errors::Result<Term> {
        let token = self.bump()?;
        let (line, col) = (token.line, token.col);
        let functor = match &token.tok {
            Tok::Upper(name) => return Ok(Term::Variable(name.clone())),
            Tok::Lower(w) | Tok::Quoted(w) | Tok::Number(w) => w.clone(),
            other => {
                return Err(syntax(
                    line,
                    col,
                    &format!("expected a term, found {}", other.describe()),
                ));
            }
        };
        if !matches!(self.peek(), Some(Tok::LParen)) {
            return Ok(Term::Apply {
                functor,
                args: Vec::new(),
            });
        }
        self.bump()?;
        let mut args = vec![self.term()?];
        while matches!(self.peek(), Some(Tok::Comma)) {
            self.bump()?;
            args.push(self.term()?);
        }
        self.expect(&Tok::RParen)?;
        Ok(Term::Apply { functor, args })
    }

    /// A general annotation term: a bracketed list, or a word optionally applied.
    fn annotation(&mut self) -> gmeow_errors::Result<Annotation> {
        if matches!(self.peek(), Some(Tok::LBracket)) {
            self.bump()?;
            if matches!(self.peek(), Some(Tok::RBracket)) {
                self.bump()?;
                return Ok(Annotation::List(Vec::new()));
            }
            let mut items = vec![self.annotation()?];
            while matches!(self.peek(), Some(Tok::Comma)) {
                self.bump()?;
                items.push(self.annotation()?);
            }
            self.expect(&Tok::RBracket)?;
            return Ok(Annotation::List(items));
        }
        let token = self.bump()?;
        let (line, col) = (token.line, token.col);
        let word = match &token.tok {
            Tok::Lower(w) | Tok::Quoted(w) | Tok::Number(w) | Tok::Upper(w) => w.clone(),
            other => {
                return Err(syntax(
                    line,
                    col,
                    &format!("expected an annotation term, found {}", other.describe()),
                ));
            }
        };
        if !matches!(self.peek(), Some(Tok::LParen)) {
            return Ok(Annotation::Name(word));
        }
        self.bump()?;
        let mut args = vec![self.annotation()?];
        while matches!(self.peek(), Some(Tok::Comma)) {
            self.bump()?;
            args.push(self.annotation()?);
        }
        self.expect(&Tok::RParen)?;
        Ok(Annotation::Func(word, args))
    }
}

/// The TPTP `<source>` functors this reader recognises as an EXTERNAL reference.
///
/// `file`/`theory`/`creator` are the `<external_source>` forms and `introduced` is the
/// `<internal_source>` form; all four name a premise the derivation did not derive, so the
/// lift carries the reference itself and never mints a fictitious inference for it.
const EXTERNAL_SOURCE_FUNCTORS: &[&str] = &["file", "theory", "introduced", "creator"];

/// Recognize a `<source>` annotation as one of the four shapes TSTP actually writes.
fn recognize_source(
    source: Annotation,
    line: u32,
    col: u32,
) -> gmeow_errors::Result<(Source, Vec<String>, Vec<ExternalSource>)> {
    match source {
        // `<source> ::= unknown` — the document declares that it does not know. That is a
        // stated absence, not a parent name, so it never becomes a dangling citation.
        Annotation::Name(name) if name == "unknown" => Ok((
            Source::External(ExternalSource {
                functor: "unknown".to_owned(),
                rendered: "unknown".to_owned(),
            }),
            Vec::new(),
            Vec::new(),
        )),
        // `<dag_source> ::= <name>` — the formula comes from the named formula.
        Annotation::Name(name) => Ok((Source::Parent, vec![name], Vec::new())),
        Annotation::List(items) => Err(unliftable(format!(
            "line {line}, column {col}: the step's source is a <sources> LIST of {} entries; it \
             declares several independent provenances for one formula, and this bridge will \
             neither pick one (dropping the rest) nor mint a step identity for each",
            items.len()
        ))),
        Annotation::Func(functor, args) => {
            if functor == "inference" {
                return recognize_inference(&functor, args, line, col);
            }
            if EXTERNAL_SOURCE_FUNCTORS.contains(&functor.as_str()) {
                let rendered = Annotation::Func(functor.clone(), args).render();
                return Ok((
                    Source::External(ExternalSource { functor, rendered }),
                    Vec::new(),
                    Vec::new(),
                ));
            }
            Err(unliftable(format!(
                "line {line}, column {col}: `{functor}(…)` is not a TPTP <source> form; this \
                 reader structures `inference`, `file`, `theory`, `introduced`, `creator`, \
                 `unknown`, and a bare parent name, and it will not guess at the shape of a \
                 provenance record the grammar does not define"
            )))
        }
    }
}

/// Recognize an `inference(rule, status-list, parent-list)` record.
fn recognize_inference(
    functor: &str,
    args: Vec<Annotation>,
    line: u32,
    col: u32,
) -> gmeow_errors::Result<(Source, Vec<String>, Vec<ExternalSource>)> {
    let [rule, status, parents] = <[Annotation; 3]>::try_from(args).map_err(|a| {
        syntax(
            line,
            col,
            &format!(
                "{functor}(…) takes exactly (rule, status-list, parent-list); found {} \
                 argument(s)",
                a.len()
            ),
        )
    })?;
    let Annotation::Name(rule) = rule else {
        return Err(syntax(
            line,
            col,
            "an inference's rule must be a bare name or a quoted atom",
        ));
    };
    let Annotation::List(status) = status else {
        return Err(syntax(
            line,
            col,
            "an inference's 2nd argument must be a bracketed status list",
        ));
    };
    let Annotation::List(parent_terms) = parents else {
        return Err(syntax(
            line,
            col,
            "an inference's 3rd argument must be a bracketed parent list",
        ));
    };

    let status: Vec<String> = status.iter().map(Annotation::render).collect();
    let mut parents = Vec::with_capacity(parent_terms.len());
    let mut external_parents = Vec::new();
    for parent in parent_terms {
        match parent {
            // A bare name cites a step of THIS derivation. It must resolve, so it goes to
            // the parent list the well-foundedness walk reads.
            Annotation::Name(name) => parents.push(name),
            // An `<external_source>` in the parent position cites a warrant the derivation
            // did not derive — `theory(equality)`, `file('SET001-1.p', ax7)`. E emits the
            // first on every equality-using inference, so refusing it refused E's canonical
            // output. It carries no sub-proof, so nothing is flattened by taking it as the
            // reference it is; it is NOT a step and never becomes a citation to resolve.
            Annotation::Func(ref functor, _)
                if EXTERNAL_SOURCE_FUNCTORS.contains(&functor.as_str()) =>
            {
                external_parents.push(ExternalSource {
                    functor: functor.clone(),
                    rendered: parent.render(),
                });
            }
            // A genuinely NESTED `inference(...)` is a second, anonymous step identity with
            // its own sub-derivation. Minting a name for it would invent a step the document
            // never named, and dropping it would lose that sub-proof — so this one is a
            // real hard failure, and the message is now true of only this case.
            other => {
                return Err(unliftable(format!(
                    "line {line}, column {col}: the inference cites the nested parent \
                     derivation `{}`; an inline sub-derivation is a second, anonymous step \
                     identity this bridge does not mint, and flattening it would drop the \
                     sub-proof",
                    other.render()
                )));
            }
        }
    }
    Ok((
        Source::Inference { rule, status },
        parents,
        external_parents,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../fixtures/theorem-subclass.tstp");
    const EPROVER_FOF: &str = include_str!("../../fixtures/eprover-fof.tstp");
    const VAMPIRE_CNF: &str = include_str!("../../fixtures/vampire-cnf-refutation.tstp");
    const EPROVER_CLAUSIFY: &str = include_str!("../../fixtures/eprover-clausify-status.tstp");

    fn one(src: &str) -> Derivation {
        parse(src.as_bytes()).unwrap_or_else(|e| panic!("must parse: {e}"))
    }

    fn err(src: &str) -> String {
        format!(
            "{}",
            parse(src.as_bytes()).expect_err("this derivation must not parse")
        )
    }

    /// A minimal well-founded derivation: one asserted leaf, one inference.
    const MINIMAL: &str = "cnf(a0, axiom, p(x)).\n\
                           cnf(d1, plain, q(x), inference(r, [status(thm)], [a0])).\n";

    // -- the committed fixture -------------------------------------------------

    #[test]
    fn the_committed_reasoner_fixture_parses_into_its_three_steps() {
        let derivation = one(FIXTURE);
        assert_eq!(derivation.steps().len(), 3);
        let names: Vec<&str> = derivation.steps().iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "d_7acf4e9d9037faca7a00b6151eb4528f6f41840d",
                "d_29e8ab9f3c2b3beff56160d5073b6e6d7bee576c",
                "d_1ad92f008ebfa11c6dcc62cb8c78d2980e55afe4",
            ]
        );
        assert_eq!(derivation.steps()[0].role, Role::Axiom);
        assert!(!derivation.steps()[0].is_derived());
        assert_eq!(derivation.steps()[1].role, Role::Plain);
        assert_eq!(
            derivation.steps()[1].parents,
            vec!["d_7acf4e9d9037faca7a00b6151eb4528f6f41840d".to_owned()]
        );
        assert_eq!(
            derivation.steps()[1].status(),
            &["status(thm)".to_owned()][..]
        );
    }

    #[test]
    fn a_quoted_atom_carries_its_full_iri_unshortened() {
        let derivation = one(FIXTURE);
        let step = &derivation.steps()[0];
        let Conclusion::Clause(clause) = &step.conclusion else {
            panic!("the reasoner fixture is CNF");
        };
        let Term::Apply { functor, args } = &clause.literals[0].atom else {
            panic!("the leaf concludes an application");
        };
        assert_eq!(functor, "https://blackcatinformatics.ca/gmeow/tptp#a");
        assert_eq!(
            args,
            &[Term::Apply {
                functor: "https://blackcatinformatics.ca/logic/entail/reserved#witness-\
                          d4a1e02579180296"
                    .to_owned(),
                args: Vec::new(),
            }]
        );
        assert!(!clause.literals[0].negated);
    }

    #[test]
    fn the_inference_rule_is_the_content_addressed_firing_iri() {
        let derivation = one(FIXTURE);
        let rule = derivation.steps()[2].rule().expect("a rule");
        assert_eq!(
            rule,
            "https://blackcatinformatics.ca/logic/dag/firing/\
             e333748014025c765c88458a6275b4b2e1ac78826b7f91e1defbff323ab982e3"
        );
    }

    #[test]
    fn the_terminal_step_is_the_derivations_conclusion() {
        let derivation = one(FIXTURE);
        assert_eq!(
            derivation.conclusion().name,
            "d_1ad92f008ebfa11c6dcc62cb8c78d2980e55afe4"
        );
        assert!(derivation.conclusion().is_derived());
    }

    #[test]
    fn a_step_is_reachable_by_name() {
        let derivation = one(MINIMAL);
        assert_eq!(derivation.step("a0").expect("a0").role, Role::Axiom);
        assert!(derivation.step("nope").is_none());
    }

    // -- the term / clause grammar --------------------------------------------

    fn clause_of<'d>(derivation: &'d Derivation, name: &str) -> &'d Clause {
        let Conclusion::Clause(clause) = &derivation.step(name).expect("the step").conclusion
        else {
            panic!("`{name}` concludes a clause");
        };
        clause
    }

    fn formula_of<'d>(derivation: &'d Derivation, name: &str) -> &'d Formula {
        let Conclusion::Formula(formula) = &derivation.step(name).expect("the step").conclusion
        else {
            panic!("`{name}` concludes a formula");
        };
        formula
    }

    #[test]
    fn a_nested_term_structure_survives_to_the_ast() {
        let derivation = one("cnf(a0, axiom, p(f(g(a), X), b)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
        let clause = clause_of(&derivation, "a0");
        assert_eq!(clause.literals.len(), 1);
        assert_eq!(clause.render(), "p(f(g(a), X), b)");
        let Term::Apply { args, .. } = &clause.literals[0].atom else {
            panic!("an application");
        };
        let Term::Apply { functor, args: f } = &args[0] else {
            panic!("a nested application");
        };
        assert_eq!(functor, "f");
        assert_eq!(f[1], Term::Variable("X".to_owned()));
    }

    #[test]
    fn a_disjunctive_clause_keeps_every_literal_and_its_polarity() {
        let derivation = one("cnf(a0, axiom, ( ~p(X) | q(X) | ~r )).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
        let clause = clause_of(&derivation, "a0");
        assert_eq!(clause.literals.len(), 3);
        assert!(clause.literals[0].negated);
        assert!(!clause.literals[1].negated);
        assert!(clause.literals[2].negated);
        assert_eq!(clause.render(), "~p(X) | q(X) | ~r");
    }

    #[test]
    fn a_cnf_equality_literal_is_an_equation_not_a_predicate_named_equals() {
        let derivation = one("cnf(a0, axiom, ( f(X) = X | a != b )).\n\
             cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n");
        let clause = clause_of(&derivation, "a0");
        assert_eq!(clause.literals.len(), 2);
        assert!(!clause.literals[0].negated);
        assert_eq!(
            clause.literals[0].equated.as_ref().map(Term::render),
            Some("X".to_owned())
        );
        assert!(clause.literals[1].negated, "`!=` is a negative equation");
        assert_eq!(clause.render(), "f(X) = X | a != b");
        assert_eq!(one(&derivation.render()), derivation);
    }

    #[test]
    fn a_tilde_negated_equation_canonicalizes_to_the_infix_disequality() {
        let derivation = one("cnf(a0, axiom, ~ f(a) = b).\n\
             cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n");
        assert_eq!(clause_of(&derivation, "a0").render(), "f(a) != b");
        assert_eq!(one(&derivation.render()), derivation);
    }

    #[test]
    fn the_empty_clause_rides_as_the_defined_atom_false() {
        let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n");
        assert_eq!(derivation.conclusion().conclusion.render(), "$false");
    }

    #[test]
    fn comments_and_the_shipped_header_are_skipped() {
        let derivation = one("% a line comment\n\
             /* a block\n comment */\n\
             cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])). % trailing\n");
        assert_eq!(derivation.steps().len(), 2);
    }

    #[test]
    fn a_multi_parent_inference_keeps_every_parent_in_source_order() {
        let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(a1, axiom, q(a)).\n\
             cnf(d2, plain, r(a), inference(res, [status(thm), foo], [a0, a1])).\n");
        assert_eq!(
            derivation.conclusion().parents,
            vec!["a0".to_owned(), "a1".to_owned()]
        );
        assert_eq!(
            derivation.conclusion().status(),
            &["status(thm)".to_owned(), "foo".to_owned()][..]
        );
    }

    #[test]
    fn an_inference_with_no_parent_is_a_derived_step_all_the_same() {
        // A prover may derive a tautology from nothing; it is still not an asserted leaf.
        let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(taut, [status(thm)], [])).\n\
             cnf(d2, plain, r(a), inference(res, [status(thm)], [a0, d1])).\n");
        assert!(derivation.step("d1").expect("d1").is_derived());
        assert!(derivation.step("d1").expect("d1").parents.is_empty());
    }

    #[test]
    fn dependency_order_places_every_parent_before_the_step_that_cites_it() {
        // Source order is deliberately BACKWARDS here: the conclusion is written first.
        let derivation = one(
            "cnf(d2, plain, r(a), inference(res, [status(thm)], [d1, a1])).\n\
             cnf(d1, plain, q(a), inference(res, [status(thm)], [a0])).\n\
             cnf(a0, axiom, p(a)).\n\
             cnf(a1, axiom, s(a)).\n",
        );
        let order = derivation.dependency_order();
        assert_eq!(order.len(), 4, "every step is placed exactly once");
        let position: BTreeMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(slot, &i)| (derivation.steps()[i].name.as_str(), slot))
            .collect();
        for step in derivation.steps() {
            for parent in &step.parents {
                assert!(
                    position[parent.as_str()] < position[step.name.as_str()],
                    "`{parent}` must be placed before `{}`",
                    step.name
                );
            }
        }
    }

    // -- the FOF grammar -------------------------------------------------------

    /// Wrap a `fof` body in a minimal well-founded derivation.
    fn fof(body: &str) -> Derivation {
        one(&format!(
            "fof(a0, axiom, {body}).\n\
             cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n"
        ))
    }

    #[test]
    fn a_quantified_implication_parses_into_a_binder_over_a_connective() {
        let derivation = fof("! [X] : (p(X) => q(X))");
        let Formula::Quantified {
            quantifier,
            variables,
            body,
        } = formula_of(&derivation, "a0")
        else {
            panic!("a quantified formula");
        };
        assert_eq!(*quantifier, Quantifier::ForAll);
        assert_eq!(variables, &["X".to_owned()]);
        let Formula::Binary { connective, .. } = body.as_ref() else {
            panic!("an implication body");
        };
        assert_eq!(*connective, Connective::Imply);
    }

    #[test]
    fn every_tptp_binary_connective_parses_and_renders_back() {
        for (surface, connective) in [
            ("(p & q)", Connective::And),
            ("(p | q)", Connective::Or),
            ("(p => q)", Connective::Imply),
            ("(p <= q)", Connective::RevImply),
            ("(p <=> q)", Connective::Iff),
            ("(p <~> q)", Connective::Xor),
            ("(p ~| q)", Connective::Nor),
            ("(p ~& q)", Connective::Nand),
        ] {
            let derivation = fof(surface);
            let formula = formula_of(&derivation, "a0");
            let Formula::Binary { connective: c, .. } = formula else {
                panic!("{surface} is a binary formula");
            };
            assert_eq!(*c, connective, "{surface}");
            assert_eq!(formula.render(), surface, "the surface round-trips");
        }
    }

    #[test]
    fn a_quantifier_list_binds_every_variable_in_source_order() {
        let derivation = fof("? [X, Y, Z] : p(X, Y, Z)");
        let Formula::Quantified {
            quantifier,
            variables,
            ..
        } = formula_of(&derivation, "a0")
        else {
            panic!("a quantified formula");
        };
        assert_eq!(*quantifier, Quantifier::Exists);
        assert_eq!(variables, &["X".to_owned(), "Y".to_owned(), "Z".to_owned()]);
    }

    #[test]
    fn equality_and_disequality_are_structured_not_read_as_predicates() {
        let derivation = fof("! [X] : (f(X) = g(X))");
        let Formula::Quantified { body, .. } = formula_of(&derivation, "a0") else {
            panic!("quantified");
        };
        let Formula::Equation {
            negated,
            left,
            right,
        } = body.as_ref()
        else {
            panic!("an equation");
        };
        assert!(!negated);
        assert_eq!(left.render(), "f(X)");
        assert_eq!(right.render(), "g(X)");

        let derivation = fof("a != b");
        let Formula::Equation { negated, .. } = formula_of(&derivation, "a0") else {
            panic!("a disequation");
        };
        assert!(negated, "`!=` is a disequation, not a predicate named `!=`");
        assert_eq!(formula_of(&derivation, "a0").render(), "a != b");
    }

    #[test]
    fn a_negated_quantified_formula_nests_rather_than_flattening() {
        let derivation = fof("~! [X] : p(X)");
        let Formula::Not(inner) = formula_of(&derivation, "a0") else {
            panic!("a negation");
        };
        assert!(matches!(inner.as_ref(), Formula::Quantified { .. }));
    }

    #[test]
    fn an_associative_chain_is_left_nested_and_re_renders_identically() {
        let derivation = fof("((p & q) & r)");
        let formula = formula_of(&derivation, "a0");
        let Formula::Binary { left, .. } = formula else {
            panic!("a conjunction");
        };
        assert!(matches!(left.as_ref(), Formula::Binary { .. }));
        assert_eq!(formula.render(), "((p & q) & r)");
    }

    #[test]
    fn mixing_the_associative_connectives_without_parentheses_is_a_syntax_error() {
        let text = err("fof(a0, axiom, p & q | r).\n");
        assert!(text.contains("without parentheses"), "{text}");
        assert!(text.contains("line "), "{text}");
    }

    #[test]
    fn a_chained_non_associative_connective_is_a_syntax_error() {
        let text = err("fof(a0, axiom, p => q => r).\n");
        assert!(text.contains("exactly two unitary operands"), "{text}");
    }

    #[test]
    fn a_fof_step_renders_under_the_fof_keyword_and_a_cnf_step_under_cnf() {
        let derivation = fof("! [X] : p(X)");
        assert!(
            derivation
                .step("a0")
                .expect("a0")
                .render()
                .starts_with("fof(")
        );
        assert!(
            derivation
                .step("d1")
                .expect("d1")
                .render()
                .starts_with("cnf(")
        );
    }

    // -- the full role set -----------------------------------------------------

    #[test]
    fn every_tptp_formula_role_parses_as_itself() {
        for word in [
            "axiom",
            "hypothesis",
            "definition",
            "assumption",
            "lemma",
            "theorem",
            "corollary",
            "conjecture",
            "negated_conjecture",
            "plain",
            "type",
            "fi_domain",
            "fi_functors",
            "fi_predicates",
            "unknown",
        ] {
            let derivation = one(&format!(
                "cnf(a0, {word}, p(a)).\n\
                 cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n"
            ));
            let role = derivation.step("a0").expect("a0").role;
            assert_eq!(role.as_str(), word, "the raw role word survives the parse");
            assert_eq!(Role::from_word(word), Some(role));
        }
    }

    #[test]
    fn only_the_three_foundation_roles_are_foundational() {
        for word in ["axiom", "hypothesis", "assumption"] {
            assert!(
                Role::from_word(word).expect("a role").is_foundational(),
                "{word}"
            );
        }
        for word in [
            "negated_conjecture",
            "conjecture",
            "plain",
            "lemma",
            "theorem",
            "definition",
            "unknown",
        ] {
            assert!(
                !Role::from_word(word).expect("a role").is_foundational(),
                "`{word}` must never be lifted as a law"
            );
        }
    }

    #[test]
    fn a_role_no_longer_dictates_whether_a_step_is_derived() {
        // A real prover writes `cnf(c, negated_conjecture, …, inference(…))` and
        // `cnf(c, plain, …, file(…))`; coupling the role to the source would refuse both.
        let derivation = one(
            "cnf(a0, negated_conjecture, ~p(a), file('problem.p', goal)).\n\
             cnf(d1, negated_conjecture, q(a), inference(r, [status(thm)], [a0])).\n",
        );
        assert!(!derivation.step("a0").expect("a0").is_derived());
        assert!(derivation.step("d1").expect("d1").is_derived());
    }

    // -- the source forms ------------------------------------------------------

    #[test]
    fn a_file_source_is_an_external_reference_carried_verbatim() {
        let derivation = one("cnf(a0, axiom, p(a), file('SET001-1.p', ax7)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
        let Source::External(external) = &derivation.step("a0").expect("a0").source else {
            panic!("a file(…) source is external");
        };
        assert_eq!(external.functor, "file");
        assert_eq!(external.rendered, "file('SET001-1.p', ax7)");
        assert!(
            derivation.step("a0").expect("a0").parents.is_empty(),
            "an external reference cites no parent inside this document"
        );
    }

    #[test]
    fn theory_introduced_creator_and_unknown_are_all_external_references() {
        for (surface, functor) in [
            ("theory(equality)", "theory"),
            (
                "introduced(definition, [new_symbols(definition, [esk1_0])])",
                "introduced",
            ),
            ("creator(eprover, [version('3.0')])", "creator"),
            ("unknown", "unknown"),
        ] {
            let derivation = one(&format!(
                "cnf(a0, axiom, p(a), {surface}).\n\
                 cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n"
            ));
            let Source::External(external) = &derivation.step("a0").expect("a0").source else {
                panic!("`{surface}` must be an external reference");
            };
            assert_eq!(external.functor, functor, "{surface}");
            assert_eq!(external.rendered, surface, "{surface} rides verbatim");
        }
    }

    #[test]
    fn a_bare_name_source_is_a_derived_step_citing_that_parent_with_no_rule() {
        let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, p(a), a0).\n");
        let step = derivation.step("d1").expect("d1");
        assert_eq!(step.source, Source::Parent);
        assert!(step.is_derived(), "a DAG source is a derivation edge");
        assert_eq!(step.parents, vec!["a0".to_owned()]);
        assert_eq!(
            step.rule(),
            None,
            "a bare DAG source names a parent, not a calculus rule"
        );
    }

    #[test]
    fn a_useful_info_field_is_read_rather_than_refused() {
        let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0]), [iquote('0:Res:1,2')]).\n");
        assert_eq!(
            derivation.step("d1").expect("d1").useful_info,
            vec!["iquote('0:Res:1,2')".to_owned()]
        );
    }

    #[test]
    fn a_non_thm_status_is_carried_rather_than_refused() {
        for token in ["cth", "esa", "sab", "ceq"] {
            let derivation = one(&format!(
                "cnf(a0, axiom, p(a)).\n\
                 cnf(d1, plain, q(a), inference(r, [status({token})], [a0])).\n"
            ));
            let step = derivation.step("d1").expect("d1");
            assert_eq!(step.status(), &[format!("status({token})")][..]);
            assert!(!step.declares_thm_status());
        }
    }

    #[test]
    fn an_empty_status_list_is_a_stated_absence_not_a_refusal() {
        let derivation = one("fof(f1, axiom, p(a)).\n\
             cnf(f2, plain, p(a), inference(cnf_transformation, [], [f1])).\n");
        assert!(derivation.step("f2").expect("f2").status().is_empty());
        assert!(!derivation.step("f2").expect("f2").declares_thm_status());
    }

    // -- rendering round-trips -------------------------------------------------

    #[test]
    fn a_rendered_derivation_re_parses_to_the_same_ast() {
        for source in [FIXTURE, MINIMAL, EPROVER_FOF, VAMPIRE_CNF, EPROVER_CLAUSIFY] {
            let first = one(source);
            let second = one(&first.render());
            assert_eq!(first, second, "rendering must be a faithful TSTP surface");
        }
    }

    #[test]
    fn every_source_form_round_trips_through_the_rendered_surface() {
        let source = "fof(a0, axiom, ! [X] : (p(X) => q(X)), file('problem.p', ax1)).\n\
                      cnf(a1, negated_conjecture, ~q(sk1), theory(equality)).\n\
                      cnf(a2, axiom, p(sk1), introduced(definition)).\n\
                      cnf(a3, plain, p(sk1), a2).\n\
                      cnf(d1, plain, $false, \
                          inference(sr, [status(thm)], [a0, a1, a3]), [iquote('x')]).\n";
        let first = one(source);
        assert_eq!(one(&first.render()), first);
    }

    #[test]
    fn an_atom_is_quoted_exactly_when_it_is_not_a_bare_word() {
        assert_eq!(render_atom("plain_word9"), "plain_word9");
        assert_eq!(render_atom("$false"), "$false");
        assert_eq!(render_atom("42"), "42");
        assert_eq!(render_atom("https://e.org/a#b"), "'https://e.org/a#b'");
        assert_eq!(render_atom("Upper"), "'Upper'");
        assert_eq!(render_atom("it's"), r"'it\'s'");
        assert_eq!(render_atom(r"back\slash"), r"'back\\slash'");
    }

    #[test]
    fn a_quoted_atom_with_escapes_round_trips_through_the_lexer() {
        let derivation = one("cnf(a0, axiom, 'it\\'s'(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
        let Term::Apply { functor, .. } = &clause_of(&derivation, "a0").literals[0].atom else {
            panic!("an application");
        };
        assert_eq!(functor, "it's");
        assert_eq!(one(&derivation.render()), derivation);
    }

    // -- the prover fixtures ---------------------------------------------------

    #[test]
    fn the_eprover_fof_fixture_lifts_its_quantifiers_roles_and_file_sources() {
        let derivation = one(EPROVER_FOF);
        let roles: BTreeSet<&str> = derivation.steps().iter().map(|s| s.role.as_str()).collect();
        assert!(roles.contains("axiom"), "{roles:?}");
        assert!(roles.contains("negated_conjecture"), "{roles:?}");
        assert!(roles.contains("plain"), "{roles:?}");
        assert!(
            derivation
                .steps()
                .iter()
                .any(|s| matches!(&s.source, Source::External(e) if e.functor == "file")),
            "the fixture carries file(…) sources"
        );
        assert!(
            derivation.steps().iter().any(|s| matches!(
                &s.conclusion,
                Conclusion::Formula(Formula::Quantified { .. })
            )),
            "the fixture carries a quantified fof conclusion"
        );
        assert_eq!(derivation.conclusion().conclusion.render(), "$false");
    }

    #[test]
    fn the_vampire_fixture_declares_empty_status_lists_and_still_parses() {
        let derivation = one(VAMPIRE_CNF);
        assert!(
            derivation
                .steps()
                .iter()
                .filter(|s| s.is_derived())
                .any(|s| s.status().is_empty()),
            "Vampire writes `inference(rule,[],[parent])`"
        );
        assert_eq!(derivation.conclusion().conclusion.render(), "$false");
    }

    #[test]
    fn the_eprover_clausification_fixture_declares_cth_and_esa_statuses() {
        let derivation = one(EPROVER_CLAUSIFY);
        let statuses: BTreeSet<&str> = derivation
            .steps()
            .iter()
            .flat_map(|s| s.status().iter().map(String::as_str))
            .collect();
        assert!(statuses.contains("status(cth)"), "{statuses:?}");
        assert!(statuses.contains("status(esa)"), "{statuses:?}");
        assert!(statuses.contains("status(thm)"), "{statuses:?}");
    }

    // -- syntax hard failures --------------------------------------------------

    #[test]
    fn every_syntax_failure_carries_a_line_and_a_column() {
        for (source, needle) in [
            ("cnf(a0, axiom, p(a))\n", "unexpected end"),
            ("cnf(a0, axiom, p(a) .\n", "expected `)`"),
            ("cnf(a0 axiom, p(a)).\n", "expected `,`"),
            ("cnf(a0, axiom, 'unterminated).\n", "unterminated"),
            ("/* never closed\ncnf(a0, axiom, p(a)).\n", "block comment"),
            ("cnf(a0, axiom, p(a)).\n@\n", "annotated formula, found `@`"),
            ("cnf(a0, axiom, p(_x)).\n", "starting with `_`"),
            ("cnf(a0, bogus_role, p(a)).\n", "not a TPTP formula role"),
            ("fmt(a0, axiom, p(a)).\n", "expected `cnf` or `fof`"),
            ("cnf(a0, axiom, ''(a)).\n", "empty single-quoted atom"),
            ("fof(a0, axiom, ! [x] : p(x)).\n", "quantified variable"),
            ("fof(a0, axiom, ! [X] p(X)).\n", "expected `:`"),
        ] {
            let text = err(source);
            assert!(text.contains("line "), "{source:?} → {text}");
            assert!(text.contains("column "), "{source:?} → {text}");
            assert!(text.contains(needle), "{source:?} → {text}");
        }
    }

    #[test]
    fn the_reported_position_is_the_offending_token_not_the_document_start() {
        let text = err("cnf(a0, axiom, p(a)).\ncnf(a1, axiom, p(&)).\n");
        assert!(text.contains("line 2, column 18"), "{text}");
    }

    #[test]
    fn a_duplicate_formula_name_is_a_parse_failure() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(a0, axiom, q(a)).\n\
             cnf(d1, plain, r(a), inference(x, [status(thm)], [a0])).\n");
        assert!(text.contains("twice"), "{text}");
        assert!(text.contains("`a0`"), "{text}");
    }

    #[test]
    fn a_malformed_inference_arity_is_a_parse_failure() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)])).\n");
        assert!(
            text.contains("exactly (rule, status-list, parent-list)"),
            "{text}"
        );
    }

    #[test]
    fn a_useful_info_field_that_is_not_a_list_is_a_parse_failure() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0]), iquote('x')).\n");
        assert!(text.contains("bracketed general list"), "{text}");
    }

    // -- unliftable hard failures ---------------------------------------------

    #[test]
    fn a_dangling_parent_is_unliftable_because_there_is_no_well_founded_proof() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [ghost])).\n");
        assert!(text.contains("`ghost`"), "{text}");
        assert!(text.contains("never introduces"), "{text}");
        assert!(text.contains("well-founded"), "{text}");
    }

    #[test]
    fn a_dangling_bare_name_source_is_unliftable_too() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), ghost).\n");
        assert!(text.contains("`ghost`"), "{text}");
        assert!(text.contains("never introduces"), "{text}");
    }

    #[test]
    fn a_cycle_is_unliftable_and_the_diagnostic_names_it() {
        let text = err("cnf(d1, plain, p(a), inference(r, [status(thm)], [d2])).\n\
             cnf(d2, plain, q(a), inference(r, [status(thm)], [d1])).\n");
        assert!(text.contains("cycle"), "{text}");
        assert!(text.contains("d1"), "{text}");
        assert!(text.contains("d2"), "{text}");
    }

    #[test]
    fn a_cycle_through_bare_name_sources_is_caught_the_same_way() {
        let text = err("cnf(d1, plain, p(a), d2).\ncnf(d2, plain, p(a), d1).\n");
        assert!(text.contains("cycle"), "{text}");
    }

    #[test]
    fn a_step_that_is_its_own_parent_is_a_cycle() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [d1])).\n");
        assert!(text.contains("cycle"), "{text}");
    }

    #[test]
    fn a_document_with_no_derived_step_is_unliftable() {
        let text = err("cnf(a0, axiom, p(a)).\n");
        assert!(text.contains("no derived step"), "{text}");
    }

    #[test]
    fn a_document_of_external_leaves_only_is_still_unliftable() {
        let text = err("cnf(a0, axiom, p(a), file('problem.p', ax)).\n");
        assert!(text.contains("no derived step"), "{text}");
    }

    #[test]
    fn a_document_with_several_terminal_steps_is_several_proofs() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n\
             cnf(d2, plain, s(a), inference(r, [status(thm)], [a0])).\n");
        assert!(text.contains("2 terminal steps"), "{text}");
        assert!(text.contains("d1"), "{text}");
    }

    #[test]
    fn an_uncited_asserted_leaf_is_a_second_terminal() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(spare, axiom, z(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
        assert!(text.contains("terminal steps"), "{text}");
        assert!(text.contains("spare"), "{text}");
    }

    #[test]
    fn every_out_of_fragment_construct_is_refused_by_name() {
        for (source, needle) in [
            ("tff(a0, type, a: $i).\n", "TYPED TPTP dialect `tff`"),
            ("thf(a0, axiom, p).\n", "TYPED TPTP dialect `thf`"),
            ("tcf(a0, axiom, p).\n", "TYPED TPTP dialect `tcf`"),
            ("include('Axioms/SET001-0.ax').\n", "`include` directive"),
            (
                "cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), mystery(x)).\n",
                "is not a TPTP <source> form",
            ),
            (
                "cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), [file('p', a), theory(equality)]).\n",
                "<sources> LIST",
            ),
            (
                "cnf(a0, axiom, p(a)).\n\
                 cnf(d1, plain, q(a), inference(r, [status(thm)], [inference(s, [], [a0])])).\n",
                "nested parent",
            ),
        ] {
            let text = err(source);
            assert!(text.contains(needle), "{source:?} → {text}");
        }
    }

    #[test]
    fn eprovers_hash_framed_szs_envelope_parses() {
        // `eprover --proof-object` frames its derivation in `#` lines. They are not TPTP
        // grammar, so the scanner refused them — and an unedited E proof could not be read
        // at all. The committed eprover fixtures had been written WITHOUT the framing, so
        // they passed a parser that could not read the tool they are named for.
        let derivation = parse(
            b"# SZS status Theorem\n\
              # SZS output start CNFRefutation\n\
              cnf(a1, axiom, (p(a))).\n\
              cnf(d1, plain, (q(a)), inference(spm,[status(thm)],[a1,theory(equality)])).\n\
              # SZS output end CNFRefutation\n\
              # Proof object total steps    : 2\n",
        )
        .expect("an unedited E proof object must parse");
        assert_eq!(derivation.steps().len(), 2);
        assert_eq!(
            derivation.step("d1").expect("the derived step").parents,
            vec!["a1".to_owned()]
        );
    }

    #[test]
    fn an_external_source_in_a_parent_list_lifts_as_a_warrant() {
        // E cites theory(equality) in the parent list of EVERY equality-using inference
        // (rw, spm, sr, cn), so refusing it refused E's canonical output. It is a
        // grammatical <parent_info> and carries no sub-proof, so there is nothing to
        // flatten — it is a warrant, not a step.
        let derivation = parse(
            b"cnf(a1, axiom, (f(X) = g(X))).\n\
             cnf(a2, axiom, (p(f(a)))).\n\
             cnf(d1, plain, (p(g(a))), inference(rw,[status(thm)],[a2,a1,theory(equality)])).\n",
        )
        .expect("an E-shaped equality inference must lift");
        let step = derivation.step("d1").expect("the derived step");
        assert_eq!(
            step.parents,
            vec!["a2".to_owned(), "a1".to_owned()],
            "an external citation is NOT a step and must never enter the parent list the \
             well-foundedness walk resolves"
        );
        assert_eq!(step.external_parents.len(), 1);
        assert_eq!(step.external_parents[0].functor, "theory");
        assert_eq!(step.external_parents[0].rendered, "theory(equality)");
    }

    #[test]
    fn a_file_reference_in_a_parent_list_lifts_too() {
        // The other <external_source> form a prover writes in the parent position.
        let derivation = parse(
            b"cnf(a1, axiom, (p(a))).\n\
             cnf(d1, plain, (q(a)), \
             inference(res,[status(thm)],[a1,file('SET001-1.p',ax7)])).\n",
        )
        .expect("a file-referenced premise must lift");
        let step = derivation.step("d1").expect("the derived step");
        assert_eq!(step.parents, vec!["a1".to_owned()]);
        assert_eq!(step.external_parents[0].rendered, "file('SET001-1.p', ax7)");
    }

    #[test]
    fn a_non_utf8_source_is_refused_before_lexing() {
        let text = format!(
            "{}",
            parse(&[b'c', b'n', b'f', 0xff, 0xfe]).expect_err("invalid UTF-8 must not parse")
        );
        assert!(text.contains("not valid UTF-8"), "{text}");
    }

    #[test]
    fn a_deep_chain_does_not_overflow_the_cycle_check() {
        // The dependency walk is explicit-stack, not recursive: a long derivation is
        // untrusted input and must not be able to blow the parser's own stack.
        let mut source = String::from("cnf(a0, axiom, p(a)).\n");
        for i in 1..5_000 {
            source.push_str(&format!(
                "cnf(d{i}, plain, q{i}(a), inference(r, [status(thm)], [{}])).\n",
                if i == 1 {
                    "a0".to_owned()
                } else {
                    format!("d{}", i - 1)
                }
            ));
        }
        let derivation = one(&source);
        assert_eq!(derivation.steps().len(), 5_000);
        assert_eq!(derivation.conclusion().name, "d4999");
    }
}
