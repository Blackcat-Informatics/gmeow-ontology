// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The TSTP parse tier: derivation bytes → a typed [`Derivation`], no RDF.
//!
//! The grammar is the TSTP *solution* (derivation) fragment of TPTP:
//!
//! ```text
//! derivation      ::= annotated*
//! annotated       ::= 'cnf' '(' name ',' role ',' clause [ ',' source ] ')' '.'
//! clause          ::= [ '(' ] literal { '|' literal } [ ')' ]
//! literal         ::= [ '~' ] term
//! term            ::= UPPER_WORD | functor [ '(' term { ',' term } ')' ]
//! source          ::= 'inference' '(' rule ',' status-list ',' parent-list ')'
//! ```
//!
//! A functor is a lower word, a `$`-word, an integer, or a **single-quoted atom** — which
//! is how a full IRI rides through TPTP without being lossily shortened, and is exactly
//! what our own reasoner emits (`'https://…/tptp#a'('https://…/reserved#witness-…')`).
//!
//! # Everything is read, or the parse fails
//!
//! The proof bridge is the only one claiming
//! [`Rung::section_retraction`](crate::frame::Rung::section_retraction), whose preservation
//! polarity is `logic:ExactPreservation`. A lift may only claim that if the source
//! genuinely recovers from the lift, so a construct this reader does not STRUCTURE cannot
//! be skipped, annotated, or "carried as prose" — it must hard-fail. The refusals below are
//! that rule applied, not a narrow parser:
//!
//! | construct | outcome | why not carried |
//! |---|---|---|
//! | `fof`/`tff`/`thf`/`tcf`/`include` | [`ProofUnliftable`] | a derivation step's conclusion is a clause; a general FOF formula is not one, and reading it as a clause would misstate it |
//! | a role other than `axiom` / `plain` | [`ProofUnliftable`] | `negated_conjecture`, `lemma`, `conjecture`, … carry an epistemic status the `math:Axiom` range of `math:dependsOnAxiom` does not, and flattening them to "axiom" would assert the negated conjecture as a law |
//! | `file(…)` / `theory(…)` / a bare-name source | [`ProofUnliftable`] | it names provenance OUTSIDE this document; the lift cannot ground it, and minting a node for an unresolvable reference is fabrication |
//! | a `<useful_info>` 5th field | [`ProofUnliftable`] | dropping it silently is exactly the loss an `ExactPreservation` rung denies |
//! | a nested `inference(…)` in a parent list | [`ProofUnliftable`] | an inline parent derivation is a second, anonymous step identity this AST does not mint |
//! | a derived step whose status list omits `status(thm)` | [`ProofUnliftable`] | the QED claim the lift emits rests on the declared thm-status of every step; without it there is no verdict to hold |
//!
//! Malformed *syntax* — an unterminated quoted atom or block comment, a missing `.`, an
//! unexpected token, a stray character, a duplicate formula name — is [`TstpParse`], always
//! with a line and column.
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

/// One literal of a clause: an atom, optionally negated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    /// Whether the atom is under a `~`.
    pub negated: bool,
    /// The literal's atom.
    pub atom: Term,
}

impl Literal {
    /// The canonical TSTP surface of this literal.
    #[must_use]
    pub fn render(&self) -> String {
        if self.negated {
            format!("~{}", self.atom.render())
        } else {
            self.atom.render()
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

/// The formula role of a derivation step — the two this bridge reads.
///
/// Every other TPTP role is refused by name (see the module doc): a role carries epistemic
/// status, and there is no non-lossy image for `negated_conjecture`, `lemma`, `conjecture`,
/// or the finite-interpretation roles in the `math:` proof layer this bridge targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// `axiom` — an asserted leaf of the derivation.
    Axiom,
    /// `plain` — a derived step, justified by an `inference(…)` record.
    Plain,
}

impl Role {
    /// The role's TPTP surface word.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Axiom => "axiom",
            Self::Plain => "plain",
        }
    }
}

/// One annotated formula of a derivation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The step's name — its identity within the derivation, and what a parent list cites.
    pub name: String,
    /// The step's role.
    pub role: Role,
    /// The clause the step concludes.
    pub conclusion: Clause,
    /// The inference rule that justifies the step, or `None` for an asserted leaf.
    ///
    /// `Some` exactly when [`Step::role`] is [`Role::Plain`]: an unjustified `plain` step
    /// claims derivedness with no warrant, and an `axiom` carrying an `inference(…)` claims
    /// to be both asserted and derived. Both are refused by [`parse`].
    pub rule: Option<String>,
    /// The parent step names cited by the step's `inference(…)`, in source order. Empty for
    /// an asserted leaf.
    pub parents: Vec<String>,
    /// The rendered status terms of the step's `inference(…)`, in source order — e.g.
    /// `["status(thm)"]`. Empty for an asserted leaf.
    pub status: Vec<String>,
}

impl Step {
    /// Whether this step is justified by an inference rather than asserted.
    #[must_use]
    pub fn is_derived(&self) -> bool {
        self.rule.is_some()
    }

    /// The step's canonical TSTP surface, one full annotated formula.
    #[must_use]
    pub fn render(&self) -> String {
        let name = render_atom(&self.name);
        let conclusion = self.conclusion.render();
        match &self.rule {
            None => format!("cnf({name}, {}, {conclusion}).", self.role.as_str()),
            Some(rule) => {
                let parents: Vec<String> = self.parents.iter().map(|p| render_atom(p)).collect();
                format!(
                    "cnf({name}, {}, {conclusion}, inference({}, [{}], [{}])).",
                    self.role.as_str(),
                    render_atom(rule),
                    self.status.join(", "),
                    parents.join(", ")
                )
            }
        }
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
            "the derivation contains no derived step: every formula is an asserted leaf, so \
             there is no inference to lift into the math: proof layer and no proof to hold a \
             math:FormalVerificationResult about"
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
    Tilde,
    Pipe,
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
    /// Lexing is TOTAL so that an out-of-fragment dialect body (a `tff` type's `$i > $o`,
    /// an `fof`'s `=>`) survives to the dialect keyword the parser refuses BY NAME. A
    /// lexer that stopped at the first `:` would report a stray character where the real
    /// answer is "this bridge reads `cnf` derivation steps".
    Other(char),
}

impl Tok {
    /// How the token reads back in a diagnostic.
    fn describe(&self) -> String {
        match self {
            Self::LParen => "`(`".to_owned(),
            Self::RParen => "`)`".to_owned(),
            Self::LBracket => "`[`".to_owned(),
            Self::RBracket => "`]`".to_owned(),
            Self::Comma => "`,`".to_owned(),
            Self::Dot => "`.`".to_owned(),
            Self::Tilde => "`~`".to_owned(),
            Self::Pipe => "`|`".to_owned(),
            Self::Lower(w) | Self::Number(w) => format!("`{w}`"),
            Self::Quoted(w) => format!("the quoted atom `{w}`"),
            Self::Upper(w) => format!("the variable `{w}`"),
            Self::Other(c) => format!("`{c}`"),
        }
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
        if c == '%' {
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
        let punct = match c {
            '(' => Some(Tok::LParen),
            ')' => Some(Tok::RParen),
            '[' => Some(Tok::LBracket),
            ']' => Some(Tok::RBracket),
            ',' => Some(Tok::Comma),
            '.' => Some(Tok::Dot),
            '~' => Some(Tok::Tilde),
            '|' => Some(Tok::Pipe),
            _ => None,
        };
        if let Some(tok) = punct {
            step!();
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

    /// `cnf(name, role, clause[, source]).`
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
                            "expected a `cnf(…)` annotated formula, found {}",
                            other.describe()
                        ),
                    ));
                }
            }
        };
        match keyword.as_str() {
            "cnf" => {}
            "fof" | "tff" | "thf" | "tcf" => {
                return Err(unliftable(format!(
                    "line {line}, column {col}: the derivation uses the TPTP dialect `{keyword}`; \
                     a derivation step's conclusion is a CNF clause, and reading a general \
                     `{keyword}` formula as one would misstate what the step concludes"
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
                    &format!("expected `cnf`, found `{other}`"),
                ));
            }
        }

        self.expect(&Tok::LParen)?;
        let name = self.atomic_word("a formula name")?;
        self.expect(&Tok::Comma)?;
        let role = self.role()?;
        self.expect(&Tok::Comma)?;
        let conclusion = self.clause()?;

        let mut rule = None;
        let mut parents = Vec::new();
        let mut status = Vec::new();
        if matches!(self.peek(), Some(Tok::Comma)) {
            self.bump()?;
            let (source_line, source_col) = self.here();
            let source = self.annotation()?;
            let (r, s, p) = recognize_inference(source, source_line, source_col)?;
            rule = Some(r);
            status = s;
            parents = p;
            if matches!(self.peek(), Some(Tok::Comma)) {
                let (line, col) = self.here();
                return Err(unliftable(format!(
                    "line {line}, column {col}: step `{name}` carries a <useful_info> 5th field; \
                     this bridge structures the derivation itself, and silently dropping a field \
                     the source states is exactly the loss its logic:ExactPreservation rung denies"
                )));
            }
        }
        self.expect(&Tok::RParen)?;
        self.expect(&Tok::Dot)?;

        match (role, &rule) {
            (Role::Plain, Some(_)) | (Role::Axiom, None) => {}
            (Role::Plain, None) => {
                return Err(unliftable(format!(
                    "line {line}, column {col}: step `{name}` has the derived role `plain` but no \
                     `inference(…)` record; a step claiming derivedness with no warrant is not a \
                     proof step, and asserting it as an axiom would turn an inference into a law"
                )));
            }
            (Role::Axiom, Some(_)) => {
                return Err(unliftable(format!(
                    "line {line}, column {col}: step `{name}` has the asserted role `axiom` but \
                     carries an `inference(…)` record; a formula cannot be both an asserted leaf \
                     and a derived step, and this bridge will not pick one of the two readings"
                )));
            }
        }

        Ok(Step {
            name,
            role,
            conclusion,
            rule,
            parents,
            status,
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
        match word.as_str() {
            "axiom" => Ok(Role::Axiom),
            "plain" => Ok(Role::Plain),
            "hypothesis" | "definition" | "assumption" | "lemma" | "theorem" | "corollary"
            | "conjecture" | "negated_conjecture" | "type" | "fi_domain" | "fi_functors"
            | "fi_predicates" | "unknown" => Err(unliftable(format!(
                "line {line}, column {col}: the formula role `{word}` carries an epistemic status \
                 the math: proof layer does not flatten: this bridge reads asserted leaves \
                 (`axiom`) and derived steps (`plain`), and lifting a `{word}` as either would \
                 state something the derivation never claimed"
            ))),
            other => Err(syntax(
                line,
                col,
                &format!("`{other}` is not a TPTP formula role"),
            )),
        }
    }

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
        let negated = matches!(self.peek(), Some(Tok::Tilde));
        if negated {
            self.bump()?;
        }
        Ok(Literal {
            negated,
            atom: self.term()?,
        })
    }

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

/// Recognize a `<source>` annotation as the one form this bridge structures.
fn recognize_inference(
    source: Annotation,
    line: u32,
    col: u32,
) -> gmeow_errors::Result<(String, Vec<String>, Vec<String>)> {
    let Annotation::Func(functor, args) = source else {
        return Err(unliftable(format!(
            "line {line}, column {col}: the step's source is not an `inference(…)` record; a \
                 bare name or a list names provenance outside this document, which the lift \
                 cannot ground and will not invent a node for"
        )));
    };
    if functor != "inference" {
        return Err(unliftable(format!(
            "line {line}, column {col}: the step's source is `{functor}(…)`; this bridge \
                 structures the derivation's own `inference(…)` edges, and a source pointing \
                 outside the document (a file, a background theory, an introduced definition) \
                 names a premise the lift has no node for"
        )));
    }
    let [rule, status, parents] = <[Annotation; 3]>::try_from(args).map_err(|a| {
        syntax(
            line,
            col,
            &format!(
                "inference(…) takes exactly (rule, status-list, parent-list); found {} \
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
    if !status.iter().any(|s| s == "status(thm)") {
        return Err(unliftable(format!(
            "line {line}, column {col}: the inference declares the status list [{}], which \
                 does not contain `status(thm)`; the math:FormalVerificationResult this bridge \
                 emits is grounded in every step's declared thm-status, so a step that declares \
                 none supports no verdict to hold",
            status.join(", ")
        )));
    }

    let mut parents = Vec::with_capacity(parent_terms.len());
    for parent in parent_terms {
        match parent {
            Annotation::Name(name) => parents.push(name),
            other => {
                return Err(unliftable(format!(
                    "line {line}, column {col}: the inference cites the nested parent \
                         `{}`; an inline parent derivation is a second, anonymous step identity \
                         this bridge does not mint, and flattening it would drop the sub-proof",
                    other.render()
                )));
            }
        }
    }
    Ok((rule, status, parents))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../fixtures/theorem-subclass.tstp");

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
        assert_eq!(derivation.steps()[1].status, vec!["status(thm)".to_owned()]);
    }

    #[test]
    fn a_quoted_atom_carries_its_full_iri_unshortened() {
        let derivation = one(FIXTURE);
        let step = &derivation.steps()[0];
        let Term::Apply { functor, args } = &step.conclusion.literals[0].atom else {
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
        assert!(!step.conclusion.literals[0].negated);
    }

    #[test]
    fn the_inference_rule_is_the_content_addressed_firing_iri() {
        let derivation = one(FIXTURE);
        let rule = derivation.steps()[2].rule.as_deref().expect("a rule");
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

    #[test]
    fn a_nested_term_structure_survives_to_the_ast() {
        let derivation = one("cnf(a0, axiom, p(f(g(a), X), b)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
        let clause = &derivation.steps()[0].conclusion;
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
        let clause = &derivation.steps()[0].conclusion;
        assert_eq!(clause.literals.len(), 3);
        assert!(clause.literals[0].negated);
        assert!(!clause.literals[1].negated);
        assert!(clause.literals[2].negated);
        assert_eq!(clause.render(), "~p(X) | q(X) | ~r");
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
            derivation.conclusion().status,
            vec!["status(thm)".to_owned(), "foo".to_owned()]
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

    // -- rendering round-trips -------------------------------------------------

    #[test]
    fn a_rendered_derivation_re_parses_to_the_same_ast() {
        for source in [FIXTURE, MINIMAL] {
            let first = one(source);
            let second = one(&first.render());
            assert_eq!(first, second, "rendering must be a faithful TSTP surface");
        }
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
        let Term::Apply { functor, .. } = &derivation.steps()[0].conclusion.literals[0].atom else {
            panic!("an application");
        };
        assert_eq!(functor, "it's");
        assert_eq!(one(&derivation.render()), derivation);
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
            ("fmt(a0, axiom, p(a)).\n", "expected `cnf`"),
            ("cnf(a0, axiom, ''(a)).\n", "empty single-quoted atom"),
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
    fn a_cycle_is_unliftable_and_the_diagnostic_names_it() {
        let text = err("cnf(d1, plain, p(a), inference(r, [status(thm)], [d2])).\n\
             cnf(d2, plain, q(a), inference(r, [status(thm)], [d1])).\n");
        assert!(text.contains("cycle"), "{text}");
        assert!(text.contains("d1"), "{text}");
        assert!(text.contains("d2"), "{text}");
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
    fn an_unjustified_plain_step_is_unliftable() {
        let text = err("cnf(d1, plain, q(a)).\n");
        assert!(text.contains("no `inference(…)` record"), "{text}");
    }

    #[test]
    fn every_out_of_fragment_construct_is_refused_by_name() {
        for (source, needle) in [
            ("fof(a0, axiom, p(a)).\n", "TPTP dialect `fof`"),
            ("tff(a0, type, a: $i).\n", "TPTP dialect `tff`"),
            ("thf(a0, axiom, p).\n", "TPTP dialect `thf`"),
            ("include('Axioms/SET001-0.ax').\n", "`include` directive"),
            (
                "cnf(a0, negated_conjecture, p(a)).\n",
                "role `negated_conjecture`",
            ),
            ("cnf(a0, lemma, p(a)).\n", "role `lemma`"),
            (
                "cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), file('problem.p', a0)).\n",
                "`file(…)`",
            ),
            (
                "cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), theory(equality)).\n",
                "`theory(…)`",
            ),
            (
                "cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), a0).\n",
                "not an `inference(…)` record",
            ),
            (
                "cnf(a0, axiom, p(a)).\n\
                 cnf(d1, plain, q(a), inference(r, [status(thm)], [a0]), [iquote('x')]).\n",
                "<useful_info>",
            ),
            (
                "cnf(a0, axiom, p(a)).\n\
                 cnf(d1, plain, q(a), inference(r, [status(thm)], [inference(s, [], [a0])])).\n",
                "nested parent",
            ),
            (
                "cnf(a0, axiom, p(a)).\n\
                 cnf(d1, plain, q(a), inference(r, [status(cth)], [a0])).\n",
                "does not contain `status(thm)`",
            ),
            (
                "cnf(a0, axiom, p(a)).\n\
                 cnf(d1, plain, q(a), inference(r, [], [a0])).\n",
                "does not contain `status(thm)`",
            ),
        ] {
            let text = err(source);
            assert!(text.contains(needle), "{source:?} → {text}");
        }
    }

    #[test]
    fn an_axiom_carrying_an_inference_will_not_be_read_as_either() {
        let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(a1, axiom, q(a), inference(r, [status(thm)], [a0])).\n");
        assert!(
            text.contains("both an asserted leaf and a derived step"),
            "{text}"
        );
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
