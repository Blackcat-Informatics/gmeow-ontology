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

/// A syntactically complete first-order TPTP document before proof obligations are sealed.
///
/// Problems and finite-interpretation artifacts are made from the same `fof`/`cnf`
/// annotated-formula grammar as a TSTP derivation, but they are not proofs: they need not
/// contain a derived step, a unique terminal, or any dependency edge at all.  Keeping this
/// neutral document tier lets consumers reuse the one parser without weakening
/// [`Derivation`]'s stronger invariants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    steps: Vec<Step>,
}

impl Document {
    /// Every annotated formula, in source order.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Consume the document and return its annotated formulas.
    #[must_use]
    pub fn into_steps(self) -> Vec<Step> {
        self.steps
    }
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
    seal(parse_document(source)?.into_steps())
}

/// Parse a neutral first-order TPTP document without claiming that it is a proof.
///
/// The accepted syntax is exactly the same self-contained `fof`/`cnf` annotated-formula
/// syntax as [`parse`]. Formula names must still be unique because they are document
/// identities. Dependency well-foundedness, derivedness, and terminal uniqueness are
/// intentionally left to [`parse`], which seals this syntax as a [`Derivation`].
///
/// # Errors
///
/// - [`SourceNotUtf8`] when `source` is not valid UTF-8.
/// - [`TstpParse`] for malformed syntax, duplicate formula names, or a dialect outside
///   the untyped first-order fragment.
pub fn parse_document(source: &[u8]) -> gmeow_errors::Result<Document> {
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
    let mut names = BTreeSet::new();
    for step in &steps {
        if !names.insert(step.name.clone()) {
            return Err(gmeow_errors::Diag::of_kind(TstpParse {
                detail: format!(
                    "the document introduces the formula name `{}` twice; a TPTP name is the \
                     annotated formula's identity",
                    step.name
                ),
            }));
        }
    }
    Ok(Document { steps })
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

#[path = "tstp.tests.rs"]
#[cfg(test)]
mod tests;
