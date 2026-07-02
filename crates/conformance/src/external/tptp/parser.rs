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
/// refutation reduction needs are distinguished; the rest collapse to
/// [`TptpRole::Premise`]. An unrecognized role string is a [`TptpError::Syntax`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TptpRole {
    /// An asserted premise (`axiom`, `hypothesis`, `definition`, `assumption`,
    /// `lemma`, `theorem`, `corollary`, `plain`). Used as-is.
    Premise,
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
            | "corollary" | "plain" => Ok(TptpRole::Premise),
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

/// One parsed annotated TPTP formula.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotatedFormula {
    /// The formula name (the first field of `fof(name, …)`).
    pub name: String,
    /// The formula role.
    pub role: TptpRole,
    /// The parsed first-order formula.
    pub formula: Formula,
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
                )))
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

    fn fof_or_cnf(&mut self, is_cnf: bool) -> Result<AnnotatedFormula, TptpError> {
        self.expect(&Tok::LParen)?;
        let name = self.name()?;
        self.expect(&Tok::Comma)?;
        let role_word = match self.bump()? {
            Tok::Lower(w) => w.clone(),
            other => {
                return Err(TptpError::Syntax(format!(
                    "expected a formula role, found {other:?}"
                )))
            }
        };
        let role = TptpRole::parse(&role_word)?;
        self.expect(&Tok::Comma)?;
        let formula = if is_cnf {
            self.cnf_clause()?
        } else {
            self.fof_formula()?
        };
        self.expect(&Tok::RParen)?;
        self.expect(&Tok::Dot)?;
        Ok(AnnotatedFormula {
            name,
            role,
            formula,
        })
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
                    )))
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
                    )))
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
        if let Some(Tok::Lower(w)) = self.peek() {
            if w.starts_with('$') {
                let w = w.clone();
                return Err(TptpError::Unsupported(format!(
                    "defined atom `{w}` ($-prefixed defined predicates are out of fragment)"
                )));
            }
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
                let relation = Term::iri(format!("{TPTP_NS}{pred}")).map_err(TptpError::Syntax)?;
                let mut term_args = Vec::with_capacity(args.len());
                for a in args {
                    term_args.push(pterm_to_term(a)?);
                }
                Formula::atom(relation, term_args).map_err(TptpError::Syntax)
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
                                )))
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
        PTerm::Var(v) => Term::var(v).map_err(TptpError::Syntax),
        PTerm::Func(name, args) if args.is_empty() => {
            Term::iri(format!("{TPTP_NS}{name}")).map_err(TptpError::Syntax)
        }
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
                if let Term::Var(v) = a {
                    if !bound.contains(v) {
                        out.insert(v.clone());
                    }
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
    fn parses_multiple_formulas_in_order() {
        let src = "fof(a1, axiom, a(x)).\nfof(a2, axiom, b(y)).\n";
        let v = parse_tptp(src).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "a1");
        assert_eq!(v[1].name, "a2");
    }
}
