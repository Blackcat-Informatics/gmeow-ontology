// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A native, dependency-free TPTP FOF/CNF parser producing the full-FOL
//! [`Formula`] IR.
//!
//! The grammar covered is the first-order fragment of TPTP: annotated `fof(...)`
//! and `cnf(...)` formulas over the connectives `~ & | => <=> <= <~> ~| ~&`, the
//! quantifiers `!` (∀) and `?` (∃), predicate applications, variables
//! (upper-initial), and constants (lower-initial / single-quoted functors of
//! arity 0). Predicate and constant symbols are minted into the [`TPTP_NS`]
//! namespace so alpha-equivalent problems lower deterministically.
//!
//! # TSTP derivations, not only problems
//!
//! An annotated formula's optional 4th (`<source>`) and 5th (`<useful_info>`)
//! fields are parsed, so this reads a TSTP **solution** — a derivation — as well
//! as a problem. `inference(<rule>, [status(thm)], [<parents>])`, `file(...)`,
//! `theory(...)`, and a bare parent name are recognized into [`TptpSource`]; the
//! generic annotation-term shape survives as [`TstpTerm`]. A derived step's role
//! `plain` is its OWN [`TptpRole::Derived`] rather than collapsing into
//! [`TptpRole::Premise`] — a derivation step is not an asserted premise, and
//! conflating the two would silently re-assert every inference as an axiom.
//!
//! Everything the first-order [`Formula`] AST cannot faithfully carry is a
//! **typed capability gap**, not a silent drop: a function symbol in argument
//! position (the `Term` AST has no function-application leaf — that is the EL/DL
//! fragment boundary made syntactic), equality/`$`-defined atoms, `include`
//! directives, and the typed/higher-order `tff`/`thf` dialects all yield
//! [`TptpError::Unsupported`]. A genuinely malformed source yields
//! [`TptpError::Syntax`].

use gmeow_logic_compile::ir::{Formula, Term};

/// The namespace TPTP predicate / constant symbols are minted into. Symbol names
/// are appended verbatim (`p` → `…#p`), so the lowerer can recover the symbol and
/// decide its class/role/individual role from atom position.
pub const TPTP_NS: &str = "https://blackcatinformatics.ca/gmeow/tptp#";

/// A parse outcome error, split so the caller can route it correctly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TptpError {
    /// The source is malformed TPTP — a hard parse failure (no recovery).
    Syntax(String),
    /// The source is well-formed TPTP but uses a construct outside the
    /// first-order fragment the native engine can carry. The caller records this
    /// as a capability gap (DlGap), never a silent `incomplete`.
    Unsupported(String),
}

impl std::fmt::Display for TptpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TptpError::Syntax(m) => write!(f, "TPTP syntax error: {m}"),
            TptpError::Unsupported(m) => {
                write!(f, "TPTP construct outside the native fragment: {m}")
            }
        }
    }
}

impl std::error::Error for TptpError {}

/// The TPTP formula role. Only the roles whose model-theoretic meaning the
/// refutation reduction or a TSTP derivation needs are distinguished; the rest
/// collapse to [`TptpRole::Premise`]. An unrecognized role string is a
/// [`TptpError::Syntax`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TptpRole {
    /// An asserted premise (`axiom`, `hypothesis`, `definition`, `assumption`,
    /// `lemma`, `theorem`, `corollary`). Used as-is.
    Premise,
    /// A DERIVED step of a TSTP derivation (`plain`) — a formula the prover
    /// inferred, normally carrying an `inference(...)` [`TptpSource`].
    ///
    /// This is deliberately NOT [`TptpRole::Premise`]: a derivation step is
    /// justified by its parents, and treating it as an asserted premise would
    /// re-assert every inference as an independent axiom — turning a proof into a
    /// (much stronger, possibly inconsistent) axiom set.
    Derived,
    /// A conjecture to prove. The refutation reduction negates it
    /// (`premises ∧ ¬conjecture`).
    Conjecture,
    /// An already-negated conjecture (`negated_conjecture`). Used as-is (it is
    /// already the `¬conjecture` half of the reduction).
    NegatedConjecture,
}

impl TptpRole {
    fn parse(s: &str) -> Result<TptpRole, TptpError> {
        match s {
            "axiom" | "hypothesis" | "definition" | "assumption" | "lemma" | "theorem"
            | "corollary" => Ok(TptpRole::Premise),
            "plain" => Ok(TptpRole::Derived),
            "conjecture" => Ok(TptpRole::Conjecture),
            "negated_conjecture" => Ok(TptpRole::NegatedConjecture),
            "type" | "fi_domain" | "fi_functors" | "fi_predicates" => {
                Err(TptpError::Unsupported(format!(
                    "formula role {s:?} (typed/finite-interpretation roles are out of fragment)"
                )))
            }
            other => Err(TptpError::Syntax(format!("unknown formula role {other:?}"))),
        }
    }
}

/// A general TSTP annotation term: the shape the `<source>` / `<useful_info>`
/// fields of an annotated formula are built from.
///
/// Kept structural (rather than a rendered string) so a consumer reads a
/// derivation's annotations without re-parsing text: `status(thm)` is
/// `Func("status", [Name("thm")])`, `[status(thm)]` is a [`Self::List`], and a
/// bare parent name is a [`Self::Name`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TstpTerm {
    /// A bare name: a lower word, a single-quoted atom, an upper word, or a number.
    /// The quoting is not retained — the name is its unescaped text.
    Name(String),
    /// A functor applied to one or more arguments, e.g. `status(thm)`.
    Func(String, Vec<TstpTerm>),
    /// A bracketed list, e.g. `[status(thm)]`, `[c_1, c_2]`, or the empty `[]`.
    List(Vec<TstpTerm>),
}

/// The recognized `<source>` annotation of an annotated formula — where the
/// formula came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TptpSource {
    /// `inference(<rule>, [<status>…], [<parent>…])` — a derived step.
    Inference {
        /// The inference rule name (a bare functor / quoted atom).
        rule: String,
        /// The status list's items verbatim (conventionally one `status(thm)`).
        status: Vec<TstpTerm>,
        /// The parent step names, in source order.
        parents: Vec<String>,
    },
    /// `file('<path>')` or `file('<path>', <name>)` — an input formula.
    File {
        /// The source file path.
        path: String,
        /// The formula's name within that file, when given.
        name: Option<String>,
    },
    /// A bare name: the source is the identically-named formula.
    Name(String),
    /// `theory(<name>, …)` — the formula comes from a background theory.
    Theory {
        /// The theory name (e.g. `equality`).
        name: String,
        /// Any further theory arguments, verbatim.
        args: Vec<TstpTerm>,
    },
}

impl TptpSource {
    /// Recognize a parsed annotation term as a source.
    ///
    /// # Errors
    /// [`TptpError::Syntax`] for a malformed `inference`/`file`/`theory` shape;
    /// [`TptpError::Unsupported`] for a well-formed but unrecognized source
    /// functor (e.g. `introduced(...)`) or a NESTED source inside a parent list —
    /// an honest capability gap, never a silently dropped provenance edge.
    fn recognize(term: TstpTerm) -> Result<TptpSource, TptpError> {
        match term {
            TstpTerm::Name(n) => Ok(TptpSource::Name(n)),
            TstpTerm::List(_) => Err(TptpError::Syntax(
                "a formula source must be a name or a functor, found a list".into(),
            )),
            TstpTerm::Func(f, args) => match f.as_str() {
                "inference" => {
                    let [rule, status, parents] = <[TstpTerm; 3]>::try_from(args).map_err(|a| {
                        TptpError::Syntax(format!(
                            "inference(...) takes exactly (rule, status-list, parent-list); \
                             found {} argument(s)",
                            a.len()
                        ))
                    })?;
                    let TstpTerm::Name(rule) = rule else {
                        return Err(TptpError::Syntax(
                            "an inference rule must be a bare name / quoted atom".into(),
                        ));
                    };
                    let TstpTerm::List(status) = status else {
                        return Err(TptpError::Syntax(
                            "an inference's 2nd argument must be a bracketed status list".into(),
                        ));
                    };
                    let TstpTerm::List(parent_terms) = parents else {
                        return Err(TptpError::Syntax(
                            "an inference's 3rd argument must be a bracketed parent list".into(),
                        ));
                    };
                    let mut parents = Vec::with_capacity(parent_terms.len());
                    for p in parent_terms {
                        match p {
                            TstpTerm::Name(n) => parents.push(n),
                            other => {
                                return Err(TptpError::Unsupported(format!(
                                    "nested inference parent {other:?} — only NAMED parents are \
                                     carried (an inline parent derivation would need a second, \
                                     anonymous step identity this IR does not mint)"
                                )));
                            }
                        }
                    }
                    Ok(TptpSource::Inference {
                        rule,
                        status,
                        parents,
                    })
                }
                "file" => {
                    let mut it = args.into_iter();
                    let path = match it.next() {
                        Some(TstpTerm::Name(p)) => p,
                        _ => {
                            return Err(TptpError::Syntax(
                                "file(...) requires a leading file-name atom".into(),
                            ));
                        }
                    };
                    let name = match it.next() {
                        None => None,
                        Some(TstpTerm::Name(n)) => Some(n),
                        Some(other) => {
                            return Err(TptpError::Syntax(format!(
                                "file(...)'s 2nd argument must be a formula name, found {other:?}"
                            )));
                        }
                    };
                    if it.next().is_some() {
                        return Err(TptpError::Syntax(
                            "file(...) takes at most (path, name)".into(),
                        ));
                    }
                    Ok(TptpSource::File { path, name })
                }
                "theory" => {
                    let mut it = args.into_iter();
                    let name = match it.next() {
                        Some(TstpTerm::Name(n)) => n,
                        _ => {
                            return Err(TptpError::Syntax(
                                "theory(...) requires a leading theory name".into(),
                            ));
                        }
                    };
                    Ok(TptpSource::Theory {
                        name,
                        args: it.collect(),
                    })
                }
                other => Err(TptpError::Unsupported(format!(
                    "formula source `{other}(...)` (only inference/file/theory and a bare \
                     parent name are carried)"
                ))),
            },
        }
    }
}

/// One parsed annotated TPTP formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotatedFormula {
    /// The formula name (the first field of `fof(name, …)`).
    pub name: String,
    /// The formula role.
    pub role: TptpRole,
    /// The parsed first-order formula.
    pub formula: Formula,
    /// The `<source>` annotation (the optional 4th field), when present. A plain
    /// problem file carries none; a TSTP derivation step carries an
    /// [`TptpSource::Inference`].
    pub source: Option<TptpSource>,
    /// The `<useful_info>` annotation (the optional 5th field) verbatim, when
    /// present — retained rather than dropped, so nothing the source states is
    /// silently lost.
    pub useful_info: Option<TstpTerm>,
}

/// Parse a complete TPTP problem document into its annotated formulas.
///
/// Comments (`%` line and `/* */` block) — including the `% SZS status` result
/// line — are skipped; the SZS ground truth is read separately by
/// [`szs`](crate::external::szs). Returns the annotated formulas in source order.
///
/// # Errors
/// [`TptpError::Syntax`] for malformed input; [`TptpError::Unsupported`] for a
/// well-formed but out-of-fragment construct (the caller turns the latter into a
/// capability-gap ledger row).
pub fn parse_tptp(source: &str) -> Result<Vec<AnnotatedFormula>, TptpError> {
    let tokens = lex(source)?;
    let mut p = Parser {
        toks: &tokens,
        pos: 0,
    };
    let mut out = Vec::new();
    while !p.at_end() {
        out.push(p.annotated_formula()?);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Dot,
    Colon,
    Tilde,      // ~
    Amp,        // &
    Pipe,       // |
    Implies,    // =>
    RevImplies, // <=
    Iff,        // <=>
    Xor,        // <~>
    Nor,        // ~|
    Nand,       // ~&
    Bang,       // ! (forall)
    Question,   // ? (exists)
    Eq,         // =
    Neq,        // !=
    /// A lower-initial word, a `$`-defined word, or a single-quoted atom (functor
    /// / predicate / constant / role / dialect keyword).
    Lower(String),
    /// An upper-initial word (a variable).
    Upper(String),
    /// A numeric literal (used only as a formula name).
    Number(String),
    /// Any other character. Lexing is total so an out-of-fragment dialect body
    /// (e.g. a `tff` type `$i > $o`) survives to the dialect keyword the parser
    /// rejects wholesale; a stray `Other` inside a real `fof`/`cnf` body is a
    /// syntax error when the parser meets it.
    Other(char),
}

fn lex(src: &str) -> Result<Vec<Tok>, TptpError> {
    let b = src.as_bytes();
    let mut i = 0;
    let n = b.len();
    let mut toks = Vec::new();
    while i < n {
        let c = b[i] as char;
        // Whitespace.
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Line comment: `% … \n`.
        if c == '%' {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment: `/* … */`.
        if c == '/' && i + 1 < n && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 >= n {
                return Err(TptpError::Syntax("unterminated block comment".into()));
            }
            i += 2;
            continue;
        }
        // Multi-char operators (longest match first).
        let two = if i + 1 < n {
            Some(&src[i..i + 2])
        } else {
            None
        };
        let three = if i + 2 < n {
            Some(&src[i..i + 3])
        } else {
            None
        };
        if three == Some("<=>") {
            toks.push(Tok::Iff);
            i += 3;
            continue;
        }
        if three == Some("<~>") {
            toks.push(Tok::Xor);
            i += 3;
            continue;
        }
        if two == Some("=>") {
            toks.push(Tok::Implies);
            i += 2;
            continue;
        }
        if two == Some("<=") {
            toks.push(Tok::RevImplies);
            i += 2;
            continue;
        }
        if two == Some("~|") {
            toks.push(Tok::Nor);
            i += 2;
            continue;
        }
        if two == Some("~&") {
            toks.push(Tok::Nand);
            i += 2;
            continue;
        }
        if two == Some("!=") {
            toks.push(Tok::Neq);
            i += 2;
            continue;
        }
        // Single-char punctuation / operators.
        match c {
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
                continue;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
                continue;
            }
            '[' => {
                toks.push(Tok::LBracket);
                i += 1;
                continue;
            }
            ']' => {
                toks.push(Tok::RBracket);
                i += 1;
                continue;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
                continue;
            }
            '.' => {
                toks.push(Tok::Dot);
                i += 1;
                continue;
            }
            ':' => {
                toks.push(Tok::Colon);
                i += 1;
                continue;
            }
            '~' => {
                toks.push(Tok::Tilde);
                i += 1;
                continue;
            }
            '&' => {
                toks.push(Tok::Amp);
                i += 1;
                continue;
            }
            '|' => {
                toks.push(Tok::Pipe);
                i += 1;
                continue;
            }
            '!' => {
                toks.push(Tok::Bang);
                i += 1;
                continue;
            }
            '?' => {
                toks.push(Tok::Question);
                i += 1;
                continue;
            }
            '=' => {
                toks.push(Tok::Eq);
                i += 1;
                continue;
            }
            _ => {}
        }
        // Single-quoted atom: '…' (with \\ and \' escapes).
        if c == '\'' {
            let mut s = String::new();
            i += 1;
            loop {
                if i >= n {
                    return Err(TptpError::Syntax("unterminated single-quoted atom".into()));
                }
                let ch = b[i] as char;
                if ch == '\\' && i + 1 < n {
                    let esc = b[i + 1] as char;
                    s.push(esc);
                    i += 2;
                    continue;
                }
                if ch == '\'' {
                    i += 1;
                    break;
                }
                s.push(ch);
                i += 1;
            }
            toks.push(Tok::Lower(s));
            continue;
        }
        // Identifiers / numbers / dollar-words.
        if c == '$' || c == '_' || c.is_ascii_alphabetic() {
            let start = i;
            i += 1;
            while i < n {
                let ch = b[i] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    i += 1;
                } else {
                    break;
                }
            }
            let word = &src[start..i];
            let first = word
                .chars()
                .next()
                .ok_or_else(|| TptpError::Syntax("internal: empty identifier token".to_string()))?;
            if first == '_' {
                // A leading `_` is neither a valid TPTP variable (`<upper_word>` =
                // `[A-Z][A-Za-z0-9_]*`) nor a valid functor (`<lower_word>` =
                // `[a-z][A-Za-z0-9_]*`) — it is malformed FOF/CNF. Reject it rather
                // than silently admit `_x` as a constant (which would change the
                // problem's meaning); underscore/anonymous variables are not part of
                // the standard first-order syntax this parser accepts.
                return Err(TptpError::Syntax(format!(
                    "identifier `{word}` starts with `_`, which is not a valid TPTP \
                     variable or functor"
                )));
            }
            if first.is_ascii_uppercase() {
                toks.push(Tok::Upper(word.to_string()));
            } else {
                toks.push(Tok::Lower(word.to_string()));
            }
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < n && (b[i] as char).is_ascii_digit() {
                i += 1;
            }
            toks.push(Tok::Number(src[start..i].to_string()));
            continue;
        }
        // Total lexing: any other character becomes an `Other` token (see the
        // variant doc) rather than a hard lex error, so a non-`fof`/`cnf` dialect
        // statement reaches the keyword the parser rejects with `Unsupported`.
        toks.push(Tok::Other(c));
        i += 1;
    }
    Ok(toks)
}

// ---------------------------------------------------------------------------
// Parser (recursive descent)
// ---------------------------------------------------------------------------

/// An intermediate term shape, so a function application in argument position can
/// be recognized and rejected (the [`Term`] AST cannot carry it).
enum PTerm {
    Var(String),
    /// A functor with zero or more arguments. Arity 0 = a constant.
    Func(String, Vec<PTerm>),
}

struct Parser<'a> {
    toks: &'a [Tok],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn at_end(&self) -> bool {
        self.pos >= self.toks.len()
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn bump(&mut self) -> Result<&Tok, TptpError> {
        let t = self
            .toks
            .get(self.pos)
            .ok_or_else(|| TptpError::Syntax("unexpected end of input".into()))?;
        self.pos += 1;
        Ok(t)
    }

    fn expect(&mut self, want: &Tok) -> Result<(), TptpError> {
        let got = self.bump()?;
        if got == want {
            Ok(())
        } else {
            Err(TptpError::Syntax(format!(
                "expected {want:?}, found {got:?}"
            )))
        }
    }

    /// `fof(name, role, formula).` | `cnf(name, role, clause).`
    fn annotated_formula(&mut self) -> Result<AnnotatedFormula, TptpError> {
        let kw = match self.bump()? {
            Tok::Lower(w) => w.clone(),
            other => {
                return Err(TptpError::Syntax(format!(
                    "expected a `fof`/`cnf` annotated formula, found {other:?}"
                )));
            }
        };
        match kw.as_str() {
            "fof" => self.fof_or_cnf(false),
            "cnf" => self.fof_or_cnf(true),
            "tff" | "thf" | "tcf" => Err(TptpError::Unsupported(format!(
                "TPTP dialect `{kw}` (only untyped first-order `fof`/`cnf` is in fragment)"
            ))),
            "include" => Err(TptpError::Unsupported(
                "`include` directive (problems must be self-contained)".into(),
            )),
            other => Err(TptpError::Syntax(format!(
                "expected `fof` or `cnf`, found `{other}`"
            ))),
        }
    }

    /// `fof(name, role, formula[, source[, useful_info]]).`
    ///
    /// The trailing annotation fields are the TSTP derivation slot: a problem file
    /// omits them (and parses exactly as before), a derivation step supplies at
    /// least the `<source>`.
    fn fof_or_cnf(&mut self, is_cnf: bool) -> Result<AnnotatedFormula, TptpError> {
        self.expect(&Tok::LParen)?;
        let name = self.name()?;
        self.expect(&Tok::Comma)?;
        let role_word = match self.bump()? {
            Tok::Lower(w) => w.clone(),
            other => {
                return Err(TptpError::Syntax(format!(
                    "expected a formula role, found {other:?}"
                )));
            }
        };
        let role = TptpRole::parse(&role_word)?;
        self.expect(&Tok::Comma)?;
        let formula = if is_cnf {
            self.cnf_clause()?
        } else {
            self.fof_formula()?
        };
        let mut source = None;
        let mut useful_info = None;
        if matches!(self.peek(), Some(Tok::Comma)) {
            self.bump()?;
            source = Some(TptpSource::recognize(self.tstp_term()?)?);
            if matches!(self.peek(), Some(Tok::Comma)) {
                self.bump()?;
                useful_info = Some(self.tstp_term()?);
            }
        }
        self.expect(&Tok::RParen)?;
        self.expect(&Tok::Dot)?;
        Ok(AnnotatedFormula {
            name,
            role,
            formula,
            source,
            useful_info,
        })
    }

    /// A general TSTP annotation term: a bracketed list, or a name optionally
    /// applied to a parenthesized argument list.
    fn tstp_term(&mut self) -> Result<TstpTerm, TptpError> {
        if matches!(self.peek(), Some(Tok::LBracket)) {
            self.bump()?;
            let mut items = Vec::new();
            if matches!(self.peek(), Some(Tok::RBracket)) {
                self.bump()?;
                return Ok(TstpTerm::List(items));
            }
            loop {
                items.push(self.tstp_term()?);
                match self.peek() {
                    Some(Tok::Comma) => {
                        self.bump()?;
                    }
                    Some(Tok::RBracket) => break,
                    other => {
                        return Err(TptpError::Syntax(format!(
                            "expected `,` or `]` in an annotation list, found {other:?}"
                        )));
                    }
                }
            }
            self.expect(&Tok::RBracket)?;
            return Ok(TstpTerm::List(items));
        }
        let word = match self.bump()? {
            Tok::Lower(w) | Tok::Upper(w) | Tok::Number(w) => w.clone(),
            other => {
                return Err(TptpError::Syntax(format!(
                    "expected an annotation term, found {other:?}"
                )));
            }
        };
        if !matches!(self.peek(), Some(Tok::LParen)) {
            return Ok(TstpTerm::Name(word));
        }
        self.bump()?;
        let mut args = Vec::new();
        // `f()` is not TPTP: a functor is either bare or applied to ≥1 argument.
        loop {
            args.push(self.tstp_term()?);
            match self.peek() {
                Some(Tok::Comma) => {
                    self.bump()?;
                }
                Some(Tok::RParen) => break,
                other => {
                    return Err(TptpError::Syntax(format!(
                        "expected `,` or `)` in an annotation argument list, found {other:?}"
                    )));
                }
            }
        }
        self.expect(&Tok::RParen)?;
        Ok(TstpTerm::Func(word, args))
    }

    fn name(&mut self) -> Result<String, TptpError> {
        match self.bump()? {
            Tok::Lower(w) | Tok::Upper(w) | Tok::Number(w) => Ok(w.clone()),
            other => Err(TptpError::Syntax(format!(
                "expected a formula name, found {other:?}"
            ))),
        }
    }

    // --- FOF formula grammar (loosest binding outermost) --------------------

    /// A full FOF logic formula: a unitary formula optionally combined with one
    /// associative (`&`/`|`) chain or one non-associative binary connective.
    fn fof_formula(&mut self) -> Result<Formula, TptpError> {
        let first = self.fof_unitary()?;
        match self.peek() {
            Some(Tok::Amp) => {
                let mut parts = vec![first];
                while matches!(self.peek(), Some(Tok::Amp)) {
                    self.bump()?;
                    parts.push(self.fof_unitary()?);
                }
                Ok(Formula::And(parts))
            }
            Some(Tok::Pipe) => {
                let mut parts = vec![first];
                while matches!(self.peek(), Some(Tok::Pipe)) {
                    self.bump()?;
                    parts.push(self.fof_unitary()?);
                }
                Ok(Formula::Or(parts))
            }
            Some(Tok::Implies) => {
                self.bump()?;
                let rhs = self.fof_unitary()?;
                Ok(Formula::Implies(Box::new(first), Box::new(rhs)))
            }
            Some(Tok::RevImplies) => {
                self.bump()?;
                let rhs = self.fof_unitary()?;
                // `a <= b` is `b => a`.
                Ok(Formula::Implies(Box::new(rhs), Box::new(first)))
            }
            Some(Tok::Iff) => {
                self.bump()?;
                let rhs = self.fof_unitary()?;
                Ok(Formula::Iff(Box::new(first), Box::new(rhs)))
            }
            Some(Tok::Xor) => {
                self.bump()?;
                let rhs = self.fof_unitary()?;
                // `a <~> b` = ¬(a ↔ b).
                Ok(Formula::Not(Box::new(Formula::Iff(
                    Box::new(first),
                    Box::new(rhs),
                ))))
            }
            Some(Tok::Nor) => {
                self.bump()?;
                let rhs = self.fof_unitary()?;
                // `a ~| b` = ¬(a ∨ b).
                Ok(Formula::Not(Box::new(Formula::Or(vec![first, rhs]))))
            }
            Some(Tok::Nand) => {
                self.bump()?;
                let rhs = self.fof_unitary()?;
                // `a ~& b` = ¬(a ∧ b).
                Ok(Formula::Not(Box::new(Formula::And(vec![first, rhs]))))
            }
            _ => Ok(first),
        }
    }

    /// A unitary FOF formula: quantified, negation, parenthesized, or an atom.
    fn fof_unitary(&mut self) -> Result<Formula, TptpError> {
        match self.peek() {
            Some(Tok::Bang) => self.quantified(true),
            Some(Tok::Question) => self.quantified(false),
            Some(Tok::Tilde) => {
                self.bump()?;
                let inner = self.fof_unitary()?;
                Ok(Formula::Not(Box::new(inner)))
            }
            Some(Tok::LParen) => {
                self.bump()?;
                let f = self.fof_formula()?;
                self.expect(&Tok::RParen)?;
                Ok(f)
            }
            _ => self.atom(),
        }
    }

    /// `(! | ?) [Var, …] : unitary`
    fn quantified(&mut self, universal: bool) -> Result<Formula, TptpError> {
        self.bump()?; // ! or ?
        self.expect(&Tok::LBracket)?;
        let mut vars = Vec::new();
        loop {
            match self.bump()? {
                Tok::Upper(v) => vars.push(v.clone()),
                other => {
                    return Err(TptpError::Syntax(format!(
                        "expected a quantified variable (upper-case), found {other:?}"
                    )));
                }
            }
            match self.peek() {
                Some(Tok::Comma) => {
                    self.bump()?;
                }
                Some(Tok::RBracket) => break,
                other => {
                    return Err(TptpError::Syntax(format!(
                        "expected `,` or `]` in quantifier variable list, found {other:?}"
                    )));
                }
            }
        }
        self.expect(&Tok::RBracket)?;
        self.expect(&Tok::Colon)?;
        // The quantifier body is a `<fof_unit_formula>` (unitary or unary) per the
        // TPTP BNF — NOT a full binary formula. So `![X] : a(X) => b(X)` parses as
        // `(![X] : a(X)) => b(X)`; a quantified implication must be parenthesized as
        // `![X] : (a(X) => b(X))`. Do NOT "fix" this to `fof_formula()`: that would
        // bind `=>`/`|`/`&` under the quantifier and make the parser non-conformant.
        let body = Box::new(self.fof_unitary()?);
        if universal {
            Ok(Formula::Forall { vars, body })
        } else {
            Ok(Formula::Exists { vars, body })
        }
    }

    /// An atomic formula: a predicate application, or an equality (unsupported).
    fn atom(&mut self) -> Result<Formula, TptpError> {
        // A `$`-defined atom (e.g. `$true`, `$false`) is out of fragment.
        if let Some(Tok::Lower(w)) = self.peek()
            && w.starts_with('$')
        {
            let w = w.clone();
            return Err(TptpError::Unsupported(format!(
                "defined atom `{w}` ($-prefixed defined predicates are out of fragment)"
            )));
        }
        let head = self.pterm()?;
        // Equality / disequality over terms.
        if matches!(self.peek(), Some(Tok::Eq) | Some(Tok::Neq)) {
            self.bump()?;
            let _rhs = self.pterm()?;
            return Err(TptpError::Unsupported(
                "equality (`=`/`!=`) — the EL/DL fragment carries no equality theory".into(),
            ));
        }
        // Otherwise the head IS the atom's predicate applied to its arguments.
        match head {
            PTerm::Var(v) => Err(TptpError::Syntax(format!(
                "a variable `{v}` cannot be a predicate (first-orderness)"
            ))),
            PTerm::Func(pred, args) => {
                let relation = Term::iri(format!("{TPTP_NS}{pred}"))
                    .map_err(|e| TptpError::Syntax(e.message().to_owned()))?;
                let mut term_args = Vec::with_capacity(args.len());
                for a in args {
                    term_args.push(pterm_to_term(a)?);
                }
                Formula::atom(relation, term_args)
                    .map_err(|e| TptpError::Syntax(e.message().to_owned()))
            }
        }
    }

    // --- CNF clause grammar --------------------------------------------------

    /// A CNF clause: a `|`-disjunction of literals, with variables implicitly
    /// universally quantified. Parenthesized clauses are accepted.
    fn cnf_clause(&mut self) -> Result<Formula, TptpError> {
        let inner = if matches!(self.peek(), Some(Tok::LParen)) {
            self.bump()?;
            let f = self.cnf_disjunction()?;
            self.expect(&Tok::RParen)?;
            f
        } else {
            self.cnf_disjunction()?
        };
        // Universally close the implicit clause variables.
        let mut free = std::collections::BTreeSet::new();
        collect_free_vars(&inner, &mut Vec::new(), &mut free);
        if free.is_empty() {
            Ok(inner)
        } else {
            Ok(Formula::Forall {
                vars: free.into_iter().collect(),
                body: Box::new(inner),
            })
        }
    }

    fn cnf_disjunction(&mut self) -> Result<Formula, TptpError> {
        let mut lits = vec![self.cnf_literal()?];
        while matches!(self.peek(), Some(Tok::Pipe)) {
            self.bump()?;
            lits.push(self.cnf_literal()?);
        }
        if lits.len() == 1 {
            lits.pop()
                .ok_or_else(|| TptpError::Syntax("internal: empty CNF disjunction".to_string()))
        } else {
            Ok(Formula::Or(lits))
        }
    }

    fn cnf_literal(&mut self) -> Result<Formula, TptpError> {
        if matches!(self.peek(), Some(Tok::Tilde)) {
            self.bump()?;
            let a = self.atom()?;
            Ok(Formula::Not(Box::new(a)))
        } else {
            self.atom()
        }
    }

    // --- Terms ---------------------------------------------------------------

    /// A term: a variable, or a functor with an optional argument list.
    fn pterm(&mut self) -> Result<PTerm, TptpError> {
        match self.bump()? {
            Tok::Upper(v) => Ok(PTerm::Var(v.clone())),
            Tok::Lower(f) => {
                let f = f.clone();
                if matches!(self.peek(), Some(Tok::LParen)) {
                    self.bump()?;
                    let mut args = Vec::new();
                    loop {
                        args.push(self.pterm()?);
                        match self.peek() {
                            Some(Tok::Comma) => {
                                self.bump()?;
                            }
                            Some(Tok::RParen) => break,
                            other => {
                                return Err(TptpError::Syntax(format!(
                                    "expected `,` or `)` in argument list, found {other:?}"
                                )));
                            }
                        }
                    }
                    self.expect(&Tok::RParen)?;
                    Ok(PTerm::Func(f, args))
                } else {
                    Ok(PTerm::Func(f, Vec::new()))
                }
            }
            other => Err(TptpError::Syntax(format!(
                "expected a term, found {other:?}"
            ))),
        }
    }
}

/// Convert a parsed argument term into a [`Term`]. A variable becomes
/// [`Term::Var`], an arity-0 functor becomes a [`Term::Iri`] constant, and a
/// function application (arity ≥ 1 in argument position) is a capability gap —
/// the [`Term`] AST has no function-application leaf.
fn pterm_to_term(t: PTerm) -> Result<Term, TptpError> {
    match t {
        PTerm::Var(v) => Term::var(v).map_err(|e| TptpError::Syntax(e.message().to_owned())),
        PTerm::Func(name, args) if args.is_empty() => Term::iri(format!("{TPTP_NS}{name}"))
            .map_err(|e| TptpError::Syntax(e.message().to_owned())),
        PTerm::Func(name, _) => Err(TptpError::Unsupported(format!(
            "function symbol `{name}` in argument position — the first-order Term AST \
             (and the EL/DL fragment) carries no functional terms"
        ))),
    }
}

/// Collect the free variable names of a formula (used to universally close a CNF
/// clause). `bound` is the stack of currently-bound names.
pub(crate) fn collect_free_vars(
    f: &Formula,
    bound: &mut Vec<String>,
    out: &mut std::collections::BTreeSet<String>,
) {
    match f {
        Formula::Atom { args, .. } => {
            for a in args {
                if let Term::Var(v) = a
                    && !bound.contains(v)
                {
                    out.insert(v.clone());
                }
            }
        }
        Formula::Not(b) => collect_free_vars(b, bound, out),
        Formula::And(fs) | Formula::Or(fs) => {
            for g in fs {
                collect_free_vars(g, bound, out);
            }
        }
        Formula::Implies(a, b) | Formula::Iff(a, b) => {
            collect_free_vars(a, bound, out);
            collect_free_vars(b, bound, out);
        }
        Formula::Forall { vars, body } | Formula::Exists { vars, body } => {
            let depth = bound.len();
            bound.extend(vars.iter().cloned());
            collect_free_vars(body, bound, out);
            bound.truncate(depth);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> AnnotatedFormula {
        let mut v = parse_tptp(src).expect("parse ok");
        assert_eq!(v.len(), 1, "expected exactly one formula");
        v.pop().unwrap()
    }

    #[test]
    fn parses_a_universal_implication() {
        let af = one("fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n");
        assert_eq!(af.name, "a_sub_b");
        assert_eq!(af.role, TptpRole::Premise);
        match &af.formula {
            Formula::Forall { vars, body } => {
                assert_eq!(vars, &["X".to_string()]);
                assert!(matches!(**body, Formula::Implies(_, _)));
            }
            other => panic!("expected Forall, got {other:?}"),
        }
    }

    #[test]
    fn quantifier_binds_only_a_unit_body_not_a_binary_formula() {
        // TPTP BNF: `<fof_quantified_formula> ::= <quantifier> [vars] : <fof_unit_formula>`.
        // The body is unitary/unary, so an un-parenthesized `=>` binds OUTSIDE the
        // quantifier: `![X] : a(X) => b(X)` == `(![X] : a(X)) => b(X)`. This pins the
        // BNF-correct precedence against a "fix" that would swallow the whole
        // implication as the body (which would bind `X` free in `b(X)`).
        let af = one("fof(prec, axiom, ![X] : a(X) => b(X)).\n");
        match &af.formula {
            Formula::Implies(l, r) => {
                assert!(
                    matches!(**l, Formula::Forall { .. }),
                    "lhs should be the quantified `a(X)`, got {l:?}"
                );
                assert!(
                    matches!(**r, Formula::Atom { .. }),
                    "rhs should be the bare `b(X)`, got {r:?}"
                );
            }
            other => panic!(
                "expected a top-level Implies (quantifier binds only its unit body), got {other:?}"
            ),
        }
    }

    #[test]
    fn parses_disjointness_as_negated_conjunction() {
        let af = one("fof(b_disj_c, axiom, ![X] : ~(b(X) & c(X))).\n");
        match &af.formula {
            Formula::Forall { body, .. } => match &**body {
                Formula::Not(inner) => assert!(matches!(**inner, Formula::And(_))),
                other => panic!("expected Not, got {other:?}"),
            },
            other => panic!("expected Forall, got {other:?}"),
        }
    }

    #[test]
    fn parses_ground_atom() {
        let af = one("fof(x_is_a, axiom, a(x)).\n");
        match &af.formula {
            Formula::Atom { relation, args } => {
                assert_eq!(*relation, Term::Iri(format!("{TPTP_NS}a")));
                assert_eq!(args, &[Term::Iri(format!("{TPTP_NS}x"))]);
            }
            other => panic!("expected Atom, got {other:?}"),
        }
    }

    #[test]
    fn parses_conjecture_role() {
        let af = one("fof(goal, conjecture, ![X] : (a(X) => b(X))).\n");
        assert_eq!(af.role, TptpRole::Conjecture);
    }

    #[test]
    fn parses_negated_conjecture_role() {
        let af = one("fof(goal, negated_conjecture, a(x)).\n");
        assert_eq!(af.role, TptpRole::NegatedConjecture);
    }

    #[test]
    fn parses_cnf_clause_with_implicit_universal() {
        // ~b(X) | ~c(X) is disjointness in CNF form.
        let af = one("cnf(b_disj_c, axiom, ( ~b(X) | ~c(X) )).\n");
        match &af.formula {
            Formula::Forall { vars, body } => {
                assert_eq!(vars, &["X".to_string()]);
                assert!(matches!(**body, Formula::Or(_)));
            }
            other => panic!("expected universally-closed Or, got {other:?}"),
        }
    }

    #[test]
    fn parses_iff_and_reverse_implication() {
        let af = one("fof(e, axiom, a(x) <=> b(x)).\n");
        assert!(matches!(af.formula, Formula::Iff(_, _)));
        let af = one("fof(r, axiom, a(x) <= b(x)).\n");
        // `a <= b` normalizes to `b => a`.
        match af.formula {
            Formula::Implies(l, r) => {
                assert_eq!(
                    *l,
                    Formula::Atom {
                        relation: Term::Iri(format!("{TPTP_NS}b")),
                        args: vec![Term::Iri(format!("{TPTP_NS}x"))]
                    }
                );
                assert_eq!(
                    *r,
                    Formula::Atom {
                        relation: Term::Iri(format!("{TPTP_NS}a")),
                        args: vec![Term::Iri(format!("{TPTP_NS}x"))]
                    }
                );
            }
            other => panic!("expected Implies, got {other:?}"),
        }
    }

    #[test]
    fn skips_comments_including_szs_status() {
        let src = "% a comment\n\
                   /* block\n comment */\n\
                   fof(x_is_a, axiom, a(x)).\n\
                   % SZS status Unsatisfiable for foo\n";
        let v = parse_tptp(src).unwrap();
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn function_symbol_in_argument_is_a_capability_gap() {
        let err = parse_tptp("fof(f, axiom, p(f(x))).\n").unwrap_err();
        assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
        assert!(err.to_string().contains("function symbol"), "{err}");
    }

    #[test]
    fn equality_is_a_capability_gap() {
        let err = parse_tptp("fof(e, axiom, x = y).\n").unwrap_err();
        assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
    }

    #[test]
    fn defined_atom_is_a_capability_gap() {
        let err = parse_tptp("fof(t, axiom, $true).\n").unwrap_err();
        assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
    }

    #[test]
    fn tff_dialect_is_a_capability_gap() {
        let err = parse_tptp("tff(t, type, a : $i > $o).\n").unwrap_err();
        assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
    }

    #[test]
    fn include_directive_is_a_capability_gap() {
        let err = parse_tptp("include('Axioms/SET001-0.ax').\n").unwrap_err();
        assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
    }

    #[test]
    fn unknown_role_is_a_syntax_error() {
        let err = parse_tptp("fof(x, bogus_role, a(x)).\n").unwrap_err();
        assert!(matches!(err, TptpError::Syntax(_)), "{err}");
    }

    #[test]
    fn missing_trailing_dot_is_a_syntax_error() {
        let err = parse_tptp("fof(x, axiom, a(x))\n").unwrap_err();
        assert!(matches!(err, TptpError::Syntax(_)), "{err}");
    }

    #[test]
    fn leading_underscore_identifier_is_a_syntax_error() {
        // A word starting with `_` is neither a valid variable (`<upper_word>`) nor a
        // valid functor (`<lower_word>`), so it must be rejected — never silently
        // admitted as a constant (which would change the problem's meaning).
        let err = parse_tptp("fof(u, axiom, p(_x)).\n").unwrap_err();
        assert!(matches!(err, TptpError::Syntax(_)), "{err}");
    }

    #[test]
    fn parses_nested_alternating_quantifiers() {
        // `![X] : ?[Y] : r(X, Y)` — the outer body is itself a quantified unit
        // formula, so the parse is Forall(Exists(atom)) with the inner binder distinct.
        let af = one("fof(nest, axiom, ![X] : ?[Y] : r(X, Y)).\n");
        match &af.formula {
            Formula::Forall { vars, body } => {
                assert_eq!(vars, &["X".to_string()]);
                match &**body {
                    Formula::Exists { vars, body } => {
                        assert_eq!(vars, &["Y".to_string()]);
                        assert!(matches!(**body, Formula::Atom { .. }));
                    }
                    other => panic!("expected inner Exists, got {other:?}"),
                }
            }
            other => panic!("expected outer Forall, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiple_formulas_in_order() {
        let src = "fof(a1, axiom, a(x)).\nfof(a2, axiom, b(y)).\n";
        let v = parse_tptp(src).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "a1");
        assert_eq!(v[1].name, "a2");
    }

    // --- TSTP derivation grammar ---------------------------------------------

    #[test]
    fn a_plain_problem_formula_carries_no_annotation() {
        // Backwards shape guarantee: the 3-field form still parses and reports an
        // ABSENT source, so `lower_problem`'s premise/conjecture routing is unchanged.
        let af = one("fof(x_is_a, axiom, a(x)).\n");
        assert_eq!(af.role, TptpRole::Premise);
        assert_eq!(af.source, None);
        assert_eq!(af.useful_info, None);
    }

    #[test]
    fn plain_role_is_a_derived_step_not_a_premise() {
        let af = one("cnf(d_1, plain, b(x), inference(r, [status(thm)], [d_0])).\n");
        assert_eq!(
            af.role,
            TptpRole::Derived,
            "a `plain` TSTP step must not masquerade as an asserted premise"
        );
    }

    #[test]
    fn parses_an_inference_source_with_status_and_parents() {
        let af = one("cnf(c3, plain, b(x), inference(resolution, [status(thm)], [c1, c2])).\n");
        match af.source.expect("a source is present") {
            TptpSource::Inference {
                rule,
                status,
                parents,
            } => {
                assert_eq!(rule, "resolution");
                assert_eq!(
                    status,
                    vec![TstpTerm::Func(
                        "status".into(),
                        vec![TstpTerm::Name("thm".into())]
                    )]
                );
                assert_eq!(parents, vec!["c1".to_string(), "c2".to_string()]);
            }
            other => panic!("expected an inference source, got {other:?}"),
        }
    }

    #[test]
    fn parses_an_inference_with_no_parents_and_a_quoted_rule_iri() {
        let af = one(
            "cnf(c1, plain, 'https://example.org/p'('https://example.org/a'), \
             inference('https://example.org/rule/1', [status(thm)], [])).\n",
        );
        match af.source.expect("source") {
            TptpSource::Inference { rule, parents, .. } => {
                assert_eq!(rule, "https://example.org/rule/1");
                assert!(parents.is_empty());
            }
            other => panic!("expected an inference source, got {other:?}"),
        }
    }

    #[test]
    fn parses_file_bare_name_and_theory_sources() {
        let af = one("cnf(c1, axiom, a(x), file('problem.p', x_is_a)).\n");
        assert_eq!(
            af.source,
            Some(TptpSource::File {
                path: "problem.p".into(),
                name: Some("x_is_a".into())
            })
        );

        let af = one("cnf(c1, axiom, a(x), file('problem.p')).\n");
        assert_eq!(
            af.source,
            Some(TptpSource::File {
                path: "problem.p".into(),
                name: None
            })
        );

        let af = one("cnf(c1, axiom, a(x), x_is_a).\n");
        assert_eq!(af.source, Some(TptpSource::Name("x_is_a".into())));

        let af = one("cnf(c1, axiom, a(x), theory(equality)).\n");
        assert_eq!(
            af.source,
            Some(TptpSource::Theory {
                name: "equality".into(),
                args: vec![]
            })
        );
    }

    #[test]
    fn parses_the_useful_info_field() {
        let af = one("cnf(c1, plain, a(x), inference(r, [], [c0]), [iquote('foo')]).\n");
        assert_eq!(
            af.useful_info,
            Some(TstpTerm::List(vec![TstpTerm::Func(
                "iquote".into(),
                vec![TstpTerm::Name("foo".into())]
            )]))
        );
    }

    #[test]
    fn a_nested_inference_parent_is_a_capability_gap() {
        // An inline parent derivation would need a second, anonymous step identity —
        // an honest gap, never a silently dropped provenance edge.
        let err = parse_tptp("cnf(c2, plain, a(x), inference(r, [], [inference(s, [], [c0])])).\n")
            .unwrap_err();
        assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
    }

    #[test]
    fn an_unrecognized_source_functor_is_a_capability_gap() {
        let err = parse_tptp("cnf(c1, plain, a(x), introduced(definition)).\n").unwrap_err();
        assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
    }

    #[test]
    fn a_malformed_inference_arity_is_a_syntax_error() {
        let err = parse_tptp("cnf(c1, plain, a(x), inference(r, [])).\n").unwrap_err();
        assert!(matches!(err, TptpError::Syntax(_)), "{err}");
    }

    #[test]
    fn the_committed_tptp_mini_problem_corpus_still_parses() {
        // Regression pin for the annotation-slot grammar: every committed
        // `tptp-mini` problem must keep parsing to exactly its authored formulas,
        // with no source annotation invented.
        const CORPUS: &[(&str, &str, usize)] = &[
            (
                "cnf-disjoint-clash",
                include_str!(
                    "../../../../../conformance/logic/cases/external/tptp-mini/cnf-disjoint-clash/source/problem.p"
                ),
                4,
            ),
            (
                "contradictory-axioms",
                include_str!(
                    "../../../../../conformance/logic/cases/external/tptp-mini/contradictory-axioms/source/problem.p"
                ),
                4,
            ),
            (
                "countersatisfiable",
                include_str!(
                    "../../../../../conformance/logic/cases/external/tptp-mini/countersatisfiable/source/problem.p"
                ),
                2,
            ),
            (
                "satisfiable-open",
                include_str!(
                    "../../../../../conformance/logic/cases/external/tptp-mini/satisfiable-open/source/problem.p"
                ),
                2,
            ),
            (
                "theorem-ground",
                include_str!(
                    "../../../../../conformance/logic/cases/external/tptp-mini/theorem-ground/source/problem.p"
                ),
                3,
            ),
            (
                "theorem-subclass",
                include_str!(
                    "../../../../../conformance/logic/cases/external/tptp-mini/theorem-subclass/source/problem.p"
                ),
                3,
            ),
        ];
        for (case, src, expected) in CORPUS {
            let parsed = parse_tptp(src).unwrap_or_else(|e| panic!("{case} must parse: {e}"));
            assert_eq!(parsed.len(), *expected, "{case} formula count");
            for af in &parsed {
                assert_eq!(af.source, None, "{case}/{} carries no source", af.name);
                assert_eq!(af.useful_info, None, "{case}/{}", af.name);
                assert!(
                    matches!(af.role, TptpRole::Premise | TptpRole::Conjecture),
                    "{case}/{} role {:?}",
                    af.name,
                    af.role
                );
            }
        }
    }
}
