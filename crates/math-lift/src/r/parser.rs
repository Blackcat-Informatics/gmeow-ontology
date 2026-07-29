// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The R parser: a positioned token stream → a typed AST.
//!
//! Recursive descent over R's statistical subset, one function per precedence level, in
//! R's own order (lowest binding first):
//!
//! ```text
//! =                       assignment (right)
//! <-  <<-                 assignment (right)
//! ->  ->>                 right assignment (left)
//! ~                       the model-formula BINDER, one- and two-sided
//! |   ||                  disjunction
//! &   &&                  conjunction
//! !                       negation (prefix)
//! == != < > <= >=         comparison
//! +   -                   additive
//! *   /                   multiplicative
//! %…% |>                  infix / pipe
//! :                       sequence — and, inside a formula, interaction
//! +   -                   sign (prefix)
//! ^                       exponentiation (right)
//! $   @                   component / slot
//! (…) […] [[…]]           call / subscript
//! ::  :::                 namespace
//! ```
//!
//! # The `~` is a binder, not a string
//!
//! `MATHEMATICS-BRIDGES.md` is explicit that a model formula lifts as a binder over
//! indexed terms. That obligation is discharged HERE, in the parse tier: `~` produces a
//! [`Formula`] whose right-hand side has already been run through R's own term algebra —
//! `*` expands to crossing, `:` to interaction, `/` to nesting, `^n` to crossing up to
//! order `n`, `-` to removal, `1`/`0` to the intercept flag. What reaches the lift tier is
//! therefore an ordered, deduplicated **term list**, never an operator tree that a later
//! pass could be tempted to stringify.
//!
//! # No recovery
//!
//! Every failure is an [`RParse`](crate::error::RParse) with a line and a column. There is
//! no resynchronization and no partial AST: `MATHEMATICS-RUNTIME.md`'s ingestion rules make
//! a malformed script a hard failure.

use crate::r::lexer::{Op, Tok, Token, lex, parse_error};

// ── AST ───────────────────────────────────────────────────────────────────────

/// A binary operator with a mathematical or logical reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Subtract,
    /// `*`
    Multiply,
    /// `/`
    Divide,
    /// `^`
    Power,
    /// `:`, the integer-sequence operator.
    Sequence,
    /// `<`
    Less,
    /// `>`
    Greater,
    /// `<=`
    LessEqual,
    /// `>=`
    GreaterEqual,
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `&`
    And,
    /// `&&`
    AndAnd,
    /// `|`
    Or,
    /// `||`
    OrOr,
}

impl BinaryOp {
    /// Whether the operator is arithmetic (as opposed to relational or logical).
    ///
    /// The split decides the routing: arithmetic lifts into the `math:` expression AST,
    /// while a relational or logical operator is a proposition and lowers into `logic:`.
    #[must_use]
    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            Self::Add | Self::Subtract | Self::Multiply | Self::Divide | Self::Power
        )
    }

    /// The operator's source spelling.
    #[must_use]
    pub fn spelling(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Power => "^",
            Self::Sequence => ":",
            Self::Less => "<",
            Self::Greater => ">",
            Self::LessEqual => "<=",
            Self::GreaterEqual => ">=",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::And => "&",
            Self::AndAnd => "&&",
            Self::Or => "|",
            Self::OrOr => "||",
        }
    }
}

/// A prefix operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Unary `+`.
    Plus,
    /// Unary `-`.
    Negate,
    /// `!`.
    Not,
}

impl UnaryOp {
    /// The operator's source spelling.
    #[must_use]
    pub fn spelling(self) -> &'static str {
        match self {
            Self::Plus => "+",
            Self::Negate => "-",
            Self::Not => "!",
        }
    }
}

/// Which assignment form the source used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignKind {
    /// `<-`
    Left,
    /// `<<-`
    SuperLeft,
    /// `=`
    Equals,
    /// `->`
    Right,
    /// `->>`
    SuperRight,
}

/// One argument of a call or a subscript.
#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    /// The argument name, for `f(data = mtcars)`.
    pub name: Option<String>,
    /// The argument value. `None` is R's genuinely EMPTY argument (`x[, 1]`), which is a
    /// distinct thing from a missing one and is kept rather than dropped.
    pub value: Option<RExpr>,
}

/// One formal parameter of a `function(…)` literal.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// The parameter name (`...` included, as an ordinary identifier).
    pub name: String,
    /// Its default expression, if the source supplies one.
    pub default: Option<RExpr>,
}

/// What role a formula term plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermKind {
    /// A single predictor variable — `x1`.
    Main,
    /// A product of two or more factors — `x1:x2`.
    Interaction,
    /// R's `.`, standing for every remaining column of the data.
    Dot,
    /// A transformed term — `I(x^2)`, `log(x)`, `poly(x, 2)`, `offset(w)`.
    Transform,
}

/// One term of a model formula's right-hand side.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaTerm {
    /// The term's factors, in source order. One factor for a main effect, several for an
    /// interaction.
    pub factors: Vec<RExpr>,
    /// The term's role.
    pub kind: TermKind,
}

impl FormulaTerm {
    /// A canonical structural key, used for term deduplication and removal matching.
    #[must_use]
    pub fn structure_key(&self) -> String {
        let mut s = String::from("term(");
        for (i, f) in self.factors.iter().enumerate() {
            if i > 0 {
                s.push(':');
            }
            s.push_str(&f.structure_key());
        }
        s.push(')');
        s
    }

    /// A canonical key that ignores factor ORDER.
    ///
    /// R's term algebra treats an interaction as a SET of factors: `a:b` and `b:a` are one
    /// term, and `(a + b + c)^2` must yield three interactions, not six. Deduplication and
    /// `-` removal therefore compare on this key, while [`Self::structure_key`] stays
    /// order-sensitive because the emitted `math:ArgumentSlot` indexes are not.
    #[must_use]
    pub fn dedup_key(&self) -> String {
        let mut keys: Vec<String> = self.factors.iter().map(RExpr::structure_key).collect();
        keys.sort();
        format!("set({})", keys.join(":"))
    }
}

/// A model formula — the lifted form of R's `~`.
///
/// The `~` is a binder over indexed terms: the response occupies the binder's first
/// operand slot and every surviving predictor term follows in source order.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    /// The response, absent for a one-sided formula (`~ x`).
    pub response: Option<RExpr>,
    /// The surviving predictor terms, deduplicated, in source order.
    pub terms: Vec<FormulaTerm>,
    /// The terms the source explicitly removed with `-`. Retained (rather than silently
    /// dropped) because `y ~ . - x3` removes from the `.` expansion, which is structure a
    /// downstream consumer needs.
    pub removed: Vec<FormulaTerm>,
    /// Whether the formula keeps its intercept (`- 1` and `+ 0` clear it).
    pub intercept: bool,
}

/// An R expression.
#[derive(Debug, Clone, PartialEq)]
pub enum RExpr {
    /// A numeric literal.
    Number {
        /// Its value.
        value: f64,
        /// Whether the source wrote the `L` integer suffix.
        integer: bool,
        /// An exponent-free canonical decimal rendering of `value`.
        text: String,
    },
    /// A string literal, escapes resolved.
    Str(String),
    /// `TRUE` / `FALSE`.
    Logical(bool),
    /// `NULL`.
    Null,
    /// `NA`.
    Na,
    /// `NaN`.
    NotANumber,
    /// `Inf`.
    Infinity,
    /// An identifier. `.` inside a formula is one of these.
    Ident(String),
    /// `pkg::name` / `pkg:::name`.
    Namespace {
        /// The package.
        package: String,
        /// The name within it.
        name: String,
        /// `true` for the internal `:::` accessor.
        internal: bool,
    },
    /// `f(args…)`.
    Call {
        /// The callee expression (usually an identifier or a namespaced name).
        callee: Box<RExpr>,
        /// The arguments, positional and named, in source order.
        args: Vec<Arg>,
    },
    /// `x[args…]` or `x[[args…]]`.
    Index {
        /// The indexed object.
        object: Box<RExpr>,
        /// The subscripts.
        args: Vec<Arg>,
        /// `true` for `[[`.
        double: bool,
    },
    /// `x$name` or `x@name`.
    Component {
        /// The object.
        object: Box<RExpr>,
        /// The component name.
        name: String,
        /// `true` for the S4 `@` slot accessor.
        slot: bool,
    },
    /// A prefix application.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// Its operand.
        operand: Box<RExpr>,
    },
    /// An infix application.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// Left operand.
        lhs: Box<RExpr>,
        /// Right operand.
        rhs: Box<RExpr>,
    },
    /// A `%…%` infix application (`%in%`, `%%`, `%/%`, a user infix).
    Special {
        /// The operator's full text, `%` delimiters included.
        operator: String,
        /// Left operand.
        lhs: Box<RExpr>,
        /// Right operand.
        rhs: Box<RExpr>,
    },
    /// A pipe: `lhs %>% rhs` or `lhs |> rhs`.
    Pipe {
        /// The piped value.
        lhs: Box<RExpr>,
        /// The receiving call.
        rhs: Box<RExpr>,
        /// `true` for the native `|>`.
        native: bool,
    },
    /// A model formula.
    Formula(Box<Formula>),
    /// An assignment used as an expression.
    Assign {
        /// The assigned-to expression.
        target: Box<RExpr>,
        /// The assigned value.
        value: Box<RExpr>,
        /// Which arrow the source wrote.
        kind: AssignKind,
    },
    /// `function(params) body`.
    Function {
        /// The formals.
        params: Vec<Param>,
        /// The body.
        body: Box<RExpr>,
    },
    /// `{ … }`.
    Block(Vec<RStmt>),
    /// `if (c) a else b`.
    If {
        /// The condition.
        condition: Box<RExpr>,
        /// The consequent.
        then_branch: Box<RExpr>,
        /// The alternative, if written.
        else_branch: Option<Box<RExpr>>,
    },
    /// `for (v in seq) body`.
    For {
        /// The loop variable.
        variable: String,
        /// The sequence expression.
        sequence: Box<RExpr>,
        /// The body.
        body: Box<RExpr>,
    },
    /// `while (c) body`.
    While {
        /// The condition.
        condition: Box<RExpr>,
        /// The body.
        body: Box<RExpr>,
    },
    /// `repeat body`.
    Repeat {
        /// The body.
        body: Box<RExpr>,
    },
    /// `break`.
    Break,
    /// `next`.
    Next,
    /// `( … )`, retained so a source grouping is not silently re-associated.
    Paren(Box<RExpr>),
}

impl RExpr {
    /// Strip redundant `(…)` wrappers.
    #[must_use]
    pub fn unparenthesized(&self) -> &Self {
        let mut e = self;
        while let Self::Paren(inner) = e {
            e = inner;
        }
        e
    }

    /// Whether this expression is R's control flow or general computation.
    ///
    /// These are exactly the forms `MATHEMATICS-BRIDGES.md` routes to `logic:` rather than
    /// into the `math:` codomain.
    #[must_use]
    pub fn is_control_flow(&self) -> bool {
        matches!(
            self.unparenthesized(),
            Self::If { .. }
                | Self::For { .. }
                | Self::While { .. }
                | Self::Repeat { .. }
                | Self::Function { .. }
                | Self::Block(_)
                | Self::Break
                | Self::Next
        )
    }

    /// A canonical, injective structural rendering.
    ///
    /// Used for formula-term deduplication and removal matching, where two occurrences of
    /// the same term must compare equal without a float or an `Eq` impl on `f64`.
    #[must_use]
    pub fn structure_key(&self) -> String {
        match self {
            Self::Number { text, integer, .. } => {
                format!("num({text},{integer})")
            }
            Self::Str(s) => format!("str({})", escape_key(s)),
            Self::Logical(b) => format!("lgl({b})"),
            Self::Null => "null".to_owned(),
            Self::Na => "na".to_owned(),
            Self::NotANumber => "nan".to_owned(),
            Self::Infinity => "inf".to_owned(),
            Self::Ident(name) => format!("id({})", escape_key(name)),
            Self::Namespace {
                package,
                name,
                internal,
            } => format!(
                "ns({},{},{internal})",
                escape_key(package),
                escape_key(name)
            ),
            Self::Call { callee, args } => {
                format!("call({},{})", callee.structure_key(), args_key(args))
            }
            Self::Index {
                object,
                args,
                double,
            } => format!(
                "idx({},{},{double})",
                object.structure_key(),
                args_key(args)
            ),
            Self::Component { object, name, slot } => format!(
                "cmp({},{},{slot})",
                object.structure_key(),
                escape_key(name)
            ),
            Self::Unary { op, operand } => {
                format!("un({},{})", op.spelling(), operand.structure_key())
            }
            Self::Binary { op, lhs, rhs } => format!(
                "bin({},{},{})",
                op.spelling(),
                lhs.structure_key(),
                rhs.structure_key()
            ),
            Self::Special { operator, lhs, rhs } => format!(
                "spec({},{},{})",
                escape_key(operator),
                lhs.structure_key(),
                rhs.structure_key()
            ),
            Self::Pipe { lhs, rhs, native } => format!(
                "pipe({},{},{native})",
                lhs.structure_key(),
                rhs.structure_key()
            ),
            Self::Formula(f) => {
                let mut s = String::from("formula(");
                match &f.response {
                    Some(r) => s.push_str(&r.structure_key()),
                    None => s.push_str("none"),
                }
                for t in &f.terms {
                    s.push('|');
                    s.push_str(&t.structure_key());
                }
                s.push_str(&format!("|intercept={})", f.intercept));
                s
            }
            Self::Assign {
                target,
                value,
                kind,
            } => format!(
                "assign({},{},{kind:?})",
                target.structure_key(),
                value.structure_key()
            ),
            Self::Function { params, body } => {
                let mut s = String::from("fn(");
                for p in params {
                    s.push_str(&escape_key(&p.name));
                    if let Some(d) = &p.default {
                        s.push('=');
                        s.push_str(&d.structure_key());
                    }
                    s.push(',');
                }
                s.push_str(&body.structure_key());
                s.push(')');
                s
            }
            Self::Block(stmts) => {
                let mut s = String::from("block(");
                for st in stmts {
                    s.push_str(&st.expression().structure_key());
                    s.push(';');
                }
                s.push(')');
                s
            }
            Self::If {
                condition,
                then_branch,
                else_branch,
            } => format!(
                "if({},{},{})",
                condition.structure_key(),
                then_branch.structure_key(),
                else_branch
                    .as_ref()
                    .map_or_else(|| "none".to_owned(), |e| e.structure_key())
            ),
            Self::For {
                variable,
                sequence,
                body,
            } => format!(
                "for({},{},{})",
                escape_key(variable),
                sequence.structure_key(),
                body.structure_key()
            ),
            Self::While { condition, body } => format!(
                "while({},{})",
                condition.structure_key(),
                body.structure_key()
            ),
            Self::Repeat { body } => format!("repeat({})", body.structure_key()),
            Self::Break => "break".to_owned(),
            Self::Next => "next".to_owned(),
            Self::Paren(inner) => inner.structure_key(),
        }
    }
}

fn escape_key(s: &str) -> String {
    format!("{}:{s}", s.len())
}

fn args_key(args: &[Arg]) -> String {
    let mut s = String::new();
    for a in args {
        s.push('[');
        if let Some(n) = &a.name {
            s.push_str(&escape_key(n));
        }
        s.push('=');
        match &a.value {
            Some(v) => s.push_str(&v.structure_key()),
            None => s.push_str("empty"),
        }
        s.push(']');
    }
    s
}

/// What a statement is.
#[derive(Debug, Clone, PartialEq)]
pub enum RStmtKind {
    /// A top-level assignment.
    Assign {
        /// The assigned-to expression.
        target: RExpr,
        /// The assigned value.
        value: RExpr,
        /// Which arrow the source wrote.
        kind: AssignKind,
    },
    /// A bare expression, evaluated for its value or its effect.
    Expr(RExpr),
}

/// One statement, with the source position it starts at.
#[derive(Debug, Clone, PartialEq)]
pub struct RStmt {
    /// The statement.
    pub kind: RStmtKind,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub column: usize,
}

impl RStmt {
    /// The statement's value expression: the right-hand side of an assignment, or the
    /// expression itself.
    #[must_use]
    pub fn expression(&self) -> &RExpr {
        match &self.kind {
            RStmtKind::Assign { value, .. } => value,
            RStmtKind::Expr(e) => e,
        }
    }
}

/// A parsed R script.
#[derive(Debug, Clone, PartialEq)]
pub struct RScript {
    /// Its top-level statements, in source order.
    pub statements: Vec<RStmt>,
}

// ── Pipe desugaring ───────────────────────────────────────────────────────────

/// Rewrite `lhs %>% rhs` / `lhs |> rhs` into the ordinary call it denotes.
///
/// Magrittr's `%>%` substitutes `lhs` for an explicit `.` placeholder when one is present
/// and otherwise inserts it as the first argument; the native `|>` always inserts first and
/// has no placeholder. Both are desugared rather than modelled as their own `math:` node,
/// because a pipe is R syntax and carries no mathematical content of its own.
#[must_use]
pub fn desugar_pipe(lhs: &RExpr, rhs: &RExpr, native: bool) -> RExpr {
    match rhs.unparenthesized() {
        RExpr::Call { callee, args } => {
            let placeholder = !native
                && args.iter().any(|a| {
                    matches!(a.value.as_ref().map(RExpr::unparenthesized), Some(RExpr::Ident(n)) if n == ".")
                });
            if placeholder {
                let args = args
                    .iter()
                    .map(|a| {
                        let replaced = match a.value.as_ref().map(RExpr::unparenthesized) {
                            Some(RExpr::Ident(n)) if n == "." => Some(lhs.clone()),
                            _ => a.value.clone(),
                        };
                        Arg {
                            name: a.name.clone(),
                            value: replaced,
                        }
                    })
                    .collect();
                return RExpr::Call {
                    callee: callee.clone(),
                    args,
                };
            }
            let mut all = Vec::with_capacity(args.len() + 1);
            all.push(Arg {
                name: None,
                value: Some(lhs.clone()),
            });
            all.extend(args.iter().cloned());
            RExpr::Call {
                callee: callee.clone(),
                args: all,
            }
        }
        other => RExpr::Call {
            callee: Box::new(other.clone()),
            args: vec![Arg {
                name: None,
                value: Some(lhs.clone()),
            }],
        },
    }
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parse `source` into a script AST.
///
/// # Errors
///
/// [`RParse`](crate::error::RParse) on any lexical or syntactic failure, carrying the
/// offending line and column.
pub fn parse(source: &str) -> gmeow_errors::Result<RScript> {
    let tokens = lex(source)?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        last: (1, 1),
    };
    let mut statements = Vec::new();
    loop {
        parser.skip_separators();
        if parser.at_end() {
            break;
        }
        let stmt = parser.statement()?;
        statements.push(stmt);
        parser.end_of_statement()?;
    }
    Ok(RScript { statements })
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    last: (usize, usize),
}

impl Parser {
    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos).map(|t| &t.tok)
    }

    fn peek_at(&self, k: usize) -> Option<&Tok> {
        self.tokens.get(self.pos + k).map(|t| &t.tok)
    }

    fn position(&self) -> (usize, usize) {
        self.tokens
            .get(self.pos)
            .map_or(self.last, |t| (t.line, t.column))
    }

    fn advance(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos)?.clone();
        self.last = (t.line, t.column);
        self.pos += 1;
        Some(t.tok)
    }

    fn at(&self, tok: &Tok) -> bool {
        self.peek() == Some(tok)
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.at(tok) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> gmeow_errors::Result<()> {
        if self.eat(tok) {
            return Ok(());
        }
        let (line, column) = self.position();
        Err(parse_error(
            line,
            column,
            format!("expected {what}, found {}", self.describe()),
        ))
    }

    fn describe(&self) -> String {
        match self.peek() {
            None => "end of input".to_owned(),
            Some(t) => format!("`{}`", render(t)),
        }
    }

    fn skip_newlines(&mut self) {
        while self.at(&Tok::Newline) {
            self.pos += 1;
        }
    }

    fn skip_separators(&mut self) {
        while self.at(&Tok::Newline) || self.at(&Tok::Semicolon) {
            self.pos += 1;
        }
    }

    fn end_of_statement(&mut self) -> gmeow_errors::Result<()> {
        if self.at_end() || self.at(&Tok::Newline) || self.at(&Tok::Semicolon) {
            return Ok(());
        }
        let (line, column) = self.position();
        Err(parse_error(
            line,
            column,
            format!(
                "expected a newline or `;` between statements, found {}",
                self.describe()
            ),
        ))
    }

    // -- statements ----------------------------------------------------------

    fn statement(&mut self) -> gmeow_errors::Result<RStmt> {
        let (line, column) = self.position();
        let expr = self.expression()?;
        let kind = match expr {
            RExpr::Assign {
                target,
                value,
                kind,
            } => RStmtKind::Assign {
                target: *target,
                value: *value,
                kind,
            },
            other => RStmtKind::Expr(other),
        };
        Ok(RStmt { kind, line, column })
    }

    // -- expressions ---------------------------------------------------------

    fn expression(&mut self) -> gmeow_errors::Result<RExpr> {
        self.assignment()
    }

    fn assignment(&mut self) -> gmeow_errors::Result<RExpr> {
        let lhs = self.tilde()?;
        let left_kind = match self.peek() {
            Some(Tok::Op(Op::Assign)) => Some(AssignKind::Left),
            Some(Tok::Op(Op::SuperAssign)) => Some(AssignKind::SuperLeft),
            Some(Tok::Op(Op::Equals)) => Some(AssignKind::Equals),
            _ => None,
        };
        if let Some(kind) = left_kind {
            self.advance();
            self.skip_newlines();
            let value = self.assignment()?;
            return Ok(RExpr::Assign {
                target: Box::new(lhs),
                value: Box::new(value),
                kind,
            });
        }
        let mut current = lhs;
        loop {
            let right_kind = match self.peek() {
                Some(Tok::Op(Op::RightAssign)) => AssignKind::Right,
                Some(Tok::Op(Op::SuperRightAssign)) => AssignKind::SuperRight,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let target = self.tilde()?;
            current = RExpr::Assign {
                target: Box::new(target),
                value: Box::new(current),
                kind: right_kind,
            };
        }
        Ok(current)
    }

    fn tilde(&mut self) -> gmeow_errors::Result<RExpr> {
        if self.at(&Tok::Op(Op::Tilde)) {
            let (line, column) = self.position();
            self.advance();
            self.skip_newlines();
            let rhs = self.or_level()?;
            return Ok(RExpr::Formula(Box::new(build_formula(
                None, &rhs, line, column,
            )?)));
        }
        let lhs = self.or_level()?;
        if self.at(&Tok::Op(Op::Tilde)) {
            let (line, column) = self.position();
            self.advance();
            self.skip_newlines();
            let rhs = self.or_level()?;
            return Ok(RExpr::Formula(Box::new(build_formula(
                Some(lhs),
                &rhs,
                line,
                column,
            )?)));
        }
        Ok(lhs)
    }

    fn or_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let mut lhs = self.and_level()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(Op::Or)) => BinaryOp::Or,
                Some(Tok::Op(Op::OrOr)) => BinaryOp::OrOr,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let rhs = self.and_level()?;
            lhs = binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn and_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let mut lhs = self.not_level()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(Op::And)) => BinaryOp::And,
                Some(Tok::Op(Op::AndAnd)) => BinaryOp::AndAnd,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let rhs = self.not_level()?;
            lhs = binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn not_level(&mut self) -> gmeow_errors::Result<RExpr> {
        if self.at(&Tok::Op(Op::Bang)) {
            self.advance();
            self.skip_newlines();
            let operand = self.not_level()?;
            return Ok(RExpr::Unary {
                op: UnaryOp::Not,
                operand: Box::new(operand),
            });
        }
        self.comparison_level()
    }

    fn comparison_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let mut lhs = self.additive_level()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(Op::Less)) => BinaryOp::Less,
                Some(Tok::Op(Op::Greater)) => BinaryOp::Greater,
                Some(Tok::Op(Op::LessEqual)) => BinaryOp::LessEqual,
                Some(Tok::Op(Op::GreaterEqual)) => BinaryOp::GreaterEqual,
                Some(Tok::Op(Op::EqualEqual)) => BinaryOp::Equal,
                Some(Tok::Op(Op::NotEqual)) => BinaryOp::NotEqual,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let rhs = self.additive_level()?;
            lhs = binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn additive_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let mut lhs = self.multiplicative_level()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(Op::Plus)) => BinaryOp::Add,
                Some(Tok::Op(Op::Minus)) => BinaryOp::Subtract,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let rhs = self.multiplicative_level()?;
            lhs = binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn multiplicative_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let mut lhs = self.infix_level()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(Op::Star)) => BinaryOp::Multiply,
                Some(Tok::Op(Op::Slash)) => BinaryOp::Divide,
                _ => break,
            };
            self.advance();
            self.skip_newlines();
            let rhs = self.infix_level()?;
            lhs = binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn infix_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let mut lhs = self.sequence_level()?;
        loop {
            match self.peek().cloned() {
                Some(Tok::Special(text)) => {
                    self.advance();
                    self.skip_newlines();
                    let rhs = self.sequence_level()?;
                    lhs = if text == "%>%" {
                        RExpr::Pipe {
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                            native: false,
                        }
                    } else {
                        RExpr::Special {
                            operator: text,
                            lhs: Box::new(lhs),
                            rhs: Box::new(rhs),
                        }
                    };
                }
                Some(Tok::Op(Op::NativePipe)) => {
                    self.advance();
                    self.skip_newlines();
                    let rhs = self.sequence_level()?;
                    lhs = RExpr::Pipe {
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                        native: true,
                    };
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn sequence_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let mut lhs = self.sign_level()?;
        while self.at(&Tok::Op(Op::Colon)) {
            self.advance();
            self.skip_newlines();
            let rhs = self.sign_level()?;
            lhs = binary(BinaryOp::Sequence, lhs, rhs);
        }
        Ok(lhs)
    }

    fn sign_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let op = match self.peek() {
            Some(Tok::Op(Op::Minus)) => Some(UnaryOp::Negate),
            Some(Tok::Op(Op::Plus)) => Some(UnaryOp::Plus),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            self.skip_newlines();
            let operand = self.sign_level()?;
            return Ok(RExpr::Unary {
                op,
                operand: Box::new(operand),
            });
        }
        self.power_level()
    }

    fn power_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let base = self.postfix_level()?;
        if self.at(&Tok::Op(Op::Caret)) {
            self.advance();
            self.skip_newlines();
            // Right-associative, and the exponent may itself carry a sign (`2^-1`).
            let exponent = self.sign_level()?;
            return Ok(binary(BinaryOp::Power, base, exponent));
        }
        Ok(base)
    }

    fn postfix_level(&mut self) -> gmeow_errors::Result<RExpr> {
        let mut expr = self.primary()?;
        loop {
            match self.peek() {
                Some(Tok::LParen) => {
                    self.advance();
                    let args = self.arguments(&Tok::RParen)?;
                    self.expect(&Tok::RParen, "`)` closing a call")?;
                    expr = RExpr::Call {
                        callee: Box::new(expr),
                        args,
                    };
                }
                Some(Tok::LBracket) => {
                    self.advance();
                    let args = self.arguments(&Tok::RBracket)?;
                    self.expect(&Tok::RBracket, "`]` closing a subscript")?;
                    expr = RExpr::Index {
                        object: Box::new(expr),
                        args,
                        double: false,
                    };
                }
                Some(Tok::DoubleLBracket) => {
                    self.advance();
                    let args = self.arguments(&Tok::RBracket)?;
                    self.expect(&Tok::RBracket, "the first `]` closing a `[[` subscript")?;
                    self.expect(&Tok::RBracket, "the second `]` closing a `[[` subscript")?;
                    expr = RExpr::Index {
                        object: Box::new(expr),
                        args,
                        double: true,
                    };
                }
                Some(Tok::Op(Op::Dollar | Op::At)) => {
                    let slot = self.at(&Tok::Op(Op::At));
                    self.advance();
                    let name = self.component_name(if slot { "@" } else { "$" })?;
                    expr = RExpr::Component {
                        object: Box::new(expr),
                        name,
                        slot,
                    };
                }
                Some(Tok::Op(Op::DoubleColon | Op::TripleColon)) => {
                    let internal = self.at(&Tok::Op(Op::TripleColon));
                    let (line, column) = self.position();
                    self.advance();
                    let RExpr::Ident(package) = expr else {
                        return Err(parse_error(
                            line,
                            column,
                            "the left operand of `::` must be a package name",
                        ));
                    };
                    let name = self.component_name(if internal { ":::" } else { "::" })?;
                    expr = RExpr::Namespace {
                        package,
                        name,
                        internal,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn component_name(&mut self, accessor: &str) -> gmeow_errors::Result<String> {
        let (line, column) = self.position();
        match self.advance() {
            Some(Tok::Ident(name)) => Ok(name),
            Some(Tok::Str(name)) => Ok(name),
            _ => Err(parse_error(
                line,
                column,
                format!("expected a name after `{accessor}`"),
            )),
        }
    }

    fn arguments(&mut self, close: &Tok) -> gmeow_errors::Result<Vec<Arg>> {
        let mut args = Vec::new();
        self.skip_newlines();
        if self.at(close) {
            return Ok(args);
        }
        loop {
            self.skip_newlines();
            if self.at(&Tok::Comma) || self.at(close) {
                args.push(Arg {
                    name: None,
                    value: None,
                });
            } else {
                let name = self.argument_name();
                self.skip_newlines();
                let value = self.expression()?;
                args.push(Arg {
                    name,
                    value: Some(value),
                });
            }
            self.skip_newlines();
            if self.eat(&Tok::Comma) {
                continue;
            }
            break;
        }
        Ok(args)
    }

    /// A leading `name =` (never `==`) marks a named argument.
    fn argument_name(&mut self) -> Option<String> {
        let name = match self.peek() {
            Some(Tok::Ident(n)) => n.clone(),
            Some(Tok::Str(n)) => n.clone(),
            _ => return None,
        };
        if self.peek_at(1) != Some(&Tok::Op(Op::Equals)) {
            return None;
        }
        self.pos += 2;
        Some(name)
    }

    fn primary(&mut self) -> gmeow_errors::Result<RExpr> {
        let (line, column) = self.position();
        let Some(tok) = self.peek().cloned() else {
            return Err(parse_error(
                line,
                column,
                "unexpected end of input where an expression was expected",
            ));
        };
        match tok {
            Tok::Number {
                value,
                integer,
                text,
            } => {
                self.advance();
                Ok(RExpr::Number {
                    value,
                    integer,
                    text,
                })
            }
            Tok::Str(s) => {
                self.advance();
                Ok(RExpr::Str(s))
            }
            Tok::True => {
                self.advance();
                Ok(RExpr::Logical(true))
            }
            Tok::False => {
                self.advance();
                Ok(RExpr::Logical(false))
            }
            Tok::Null => {
                self.advance();
                Ok(RExpr::Null)
            }
            Tok::Na => {
                self.advance();
                Ok(RExpr::Na)
            }
            Tok::NotANumber => {
                self.advance();
                Ok(RExpr::NotANumber)
            }
            Tok::Infinity => {
                self.advance();
                Ok(RExpr::Infinity)
            }
            Tok::Ident(name) => {
                self.advance();
                Ok(RExpr::Ident(name))
            }
            Tok::Break => {
                self.advance();
                Ok(RExpr::Break)
            }
            Tok::Next => {
                self.advance();
                Ok(RExpr::Next)
            }
            Tok::LParen => {
                self.advance();
                self.skip_newlines();
                let inner = self.expression()?;
                self.skip_newlines();
                self.expect(&Tok::RParen, "`)` closing a parenthesized expression")?;
                Ok(RExpr::Paren(Box::new(inner)))
            }
            Tok::LBrace => self.block(),
            Tok::If => self.if_expression(),
            Tok::For => self.for_expression(),
            Tok::While => self.while_expression(),
            Tok::Repeat => {
                self.advance();
                self.skip_newlines();
                let body = self.expression()?;
                Ok(RExpr::Repeat {
                    body: Box::new(body),
                })
            }
            Tok::Function => self.function_literal(),
            other => Err(parse_error(
                line,
                column,
                format!("`{}` cannot start an expression", render(&other)),
            )),
        }
    }

    fn block(&mut self) -> gmeow_errors::Result<RExpr> {
        self.expect(&Tok::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        loop {
            self.skip_separators();
            if self.eat(&Tok::RBrace) {
                break;
            }
            if self.at_end() {
                let (line, column) = self.position();
                return Err(parse_error(line, column, "unclosed `{` block"));
            }
            stmts.push(self.statement()?);
            self.skip_separators();
            if self.eat(&Tok::RBrace) {
                break;
            }
        }
        Ok(RExpr::Block(stmts))
    }

    fn if_expression(&mut self) -> gmeow_errors::Result<RExpr> {
        self.expect(&Tok::If, "`if`")?;
        self.expect(&Tok::LParen, "`(` after `if`")?;
        self.skip_newlines();
        let condition = self.expression()?;
        self.skip_newlines();
        self.expect(&Tok::RParen, "`)` closing the `if` condition")?;
        self.skip_newlines();
        let then_branch = self.expression()?;
        let checkpoint = self.pos;
        self.skip_newlines();
        let else_branch = if self.eat(&Tok::Else) {
            self.skip_newlines();
            Some(Box::new(self.expression()?))
        } else {
            self.pos = checkpoint;
            None
        };
        Ok(RExpr::If {
            condition: Box::new(condition),
            then_branch: Box::new(then_branch),
            else_branch,
        })
    }

    fn for_expression(&mut self) -> gmeow_errors::Result<RExpr> {
        self.expect(&Tok::For, "`for`")?;
        self.expect(&Tok::LParen, "`(` after `for`")?;
        let (line, column) = self.position();
        let Some(Tok::Ident(variable)) = self.advance() else {
            return Err(parse_error(
                line,
                column,
                "expected the loop variable name after `for (`",
            ));
        };
        self.expect(&Tok::In, "`in` in a `for` header")?;
        self.skip_newlines();
        let sequence = self.expression()?;
        self.skip_newlines();
        self.expect(&Tok::RParen, "`)` closing the `for` header")?;
        self.skip_newlines();
        let body = self.expression()?;
        Ok(RExpr::For {
            variable,
            sequence: Box::new(sequence),
            body: Box::new(body),
        })
    }

    fn while_expression(&mut self) -> gmeow_errors::Result<RExpr> {
        self.expect(&Tok::While, "`while`")?;
        self.expect(&Tok::LParen, "`(` after `while`")?;
        self.skip_newlines();
        let condition = self.expression()?;
        self.skip_newlines();
        self.expect(&Tok::RParen, "`)` closing the `while` condition")?;
        self.skip_newlines();
        let body = self.expression()?;
        Ok(RExpr::While {
            condition: Box::new(condition),
            body: Box::new(body),
        })
    }

    fn function_literal(&mut self) -> gmeow_errors::Result<RExpr> {
        self.expect(&Tok::Function, "`function`")?;
        self.expect(&Tok::LParen, "`(` after `function`")?;
        let mut params = Vec::new();
        self.skip_newlines();
        if !self.at(&Tok::RParen) {
            loop {
                self.skip_newlines();
                let (line, column) = self.position();
                let Some(Tok::Ident(name)) = self.advance() else {
                    return Err(parse_error(
                        line,
                        column,
                        "expected a formal parameter name in a `function(…)` header",
                    ));
                };
                let default = if self.eat(&Tok::Op(Op::Equals)) {
                    self.skip_newlines();
                    Some(self.expression()?)
                } else {
                    None
                };
                params.push(Param { name, default });
                self.skip_newlines();
                if self.eat(&Tok::Comma) {
                    continue;
                }
                break;
            }
        }
        self.expect(&Tok::RParen, "`)` closing a `function(…)` header")?;
        self.skip_newlines();
        let body = self.expression()?;
        Ok(RExpr::Function {
            params,
            body: Box::new(body),
        })
    }
}

fn binary(op: BinaryOp, lhs: RExpr, rhs: RExpr) -> RExpr {
    RExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn render(tok: &Tok) -> String {
    match tok {
        Tok::Ident(n) => n.clone(),
        Tok::Number { text, .. } => text.clone(),
        Tok::Str(s) => format!("\"{s}\""),
        Tok::Special(s) => s.clone(),
        Tok::True => "TRUE".to_owned(),
        Tok::False => "FALSE".to_owned(),
        Tok::Null => "NULL".to_owned(),
        Tok::Na => "NA".to_owned(),
        Tok::NotANumber => "NaN".to_owned(),
        Tok::Infinity => "Inf".to_owned(),
        Tok::If => "if".to_owned(),
        Tok::Else => "else".to_owned(),
        Tok::For => "for".to_owned(),
        Tok::While => "while".to_owned(),
        Tok::Repeat => "repeat".to_owned(),
        Tok::Function => "function".to_owned(),
        Tok::Break => "break".to_owned(),
        Tok::Next => "next".to_owned(),
        Tok::In => "in".to_owned(),
        Tok::Op(op) => op_spelling(*op).to_owned(),
        Tok::LParen => "(".to_owned(),
        Tok::RParen => ")".to_owned(),
        Tok::LBrace => "{".to_owned(),
        Tok::RBrace => "}".to_owned(),
        Tok::LBracket => "[".to_owned(),
        Tok::DoubleLBracket => "[[".to_owned(),
        Tok::RBracket => "]".to_owned(),
        Tok::Comma => ",".to_owned(),
        Tok::Semicolon => ";".to_owned(),
        Tok::Newline => "newline".to_owned(),
    }
}

fn op_spelling(op: Op) -> &'static str {
    match op {
        Op::Assign => "<-",
        Op::SuperAssign => "<<-",
        Op::RightAssign => "->",
        Op::SuperRightAssign => "->>",
        Op::Equals => "=",
        Op::Tilde => "~",
        Op::Plus => "+",
        Op::Minus => "-",
        Op::Star => "*",
        Op::Slash => "/",
        Op::Caret => "^",
        Op::Colon => ":",
        Op::DoubleColon => "::",
        Op::TripleColon => ":::",
        Op::Dollar => "$",
        Op::At => "@",
        Op::Bang => "!",
        Op::And => "&",
        Op::AndAnd => "&&",
        Op::Or => "|",
        Op::OrOr => "||",
        Op::Less => "<",
        Op::Greater => ">",
        Op::LessEqual => "<=",
        Op::GreaterEqual => ">=",
        Op::EqualEqual => "==",
        Op::NotEqual => "!=",
        Op::NativePipe => "|>",
    }
}

// ── The formula term algebra ──────────────────────────────────────────────────

/// One term as produced by the expansion, before deduplication.
#[derive(Debug, Clone)]
struct RawTerm {
    factors: Vec<RExpr>,
    removed: bool,
}

/// The largest crossing order a `(a + b)^n` formula may request.
///
/// R itself imposes no bound, but the expansion is exponential in `n`, so an unbounded
/// exponent would let a two-line script mint an unbounded ABox. A formula asking for more
/// is a hard failure rather than a silently truncated expansion.
const MAX_CROSSING_ORDER: u32 = 8;

/// Run R's term algebra over a formula's right-hand side.
fn build_formula(
    response: Option<RExpr>,
    rhs: &RExpr,
    line: usize,
    column: usize,
) -> gmeow_errors::Result<Formula> {
    let mut raw = Vec::new();
    expand_terms(rhs, false, &mut raw, line, column)?;

    let mut intercept = true;
    let mut kept: Vec<FormulaTerm> = Vec::new();
    let mut kept_keys: Vec<String> = Vec::new();
    let mut removed: Vec<FormulaTerm> = Vec::new();
    let mut removed_keys: Vec<String> = Vec::new();

    for term in raw {
        if let Some(literal) = intercept_literal(&term) {
            // `+ 1` keeps it, `- 1` and `+ 0` clear it.
            intercept = if term.removed { false } else { literal };
            continue;
        }
        let kind = classify(&term.factors);
        let built = FormulaTerm {
            factors: term.factors,
            kind,
        };
        let key = built.dedup_key();
        if term.removed {
            if !removed_keys.contains(&key) {
                removed_keys.push(key);
                removed.push(built);
            }
        } else if !kept_keys.contains(&key) {
            kept_keys.push(key);
            kept.push(built);
        }
    }

    // A removal deletes a term the expansion produced; a removal with no match (`. - x3`)
    // survives in `removed` so the lift can build the exclusion structurally.
    let survivors: Vec<FormulaTerm> = kept
        .into_iter()
        .filter(|t| !removed_keys.contains(&t.dedup_key()))
        .collect();

    if survivors.is_empty() && response.is_none() {
        return Err(parse_error(
            line,
            column,
            "a model formula with neither a response nor a surviving term binds nothing",
        ));
    }

    Ok(Formula {
        response,
        terms: survivors,
        removed,
        intercept,
    })
}

/// `1` / `0` as a bare term is the intercept switch, not a predictor.
fn intercept_literal(term: &RawTerm) -> Option<bool> {
    if term.factors.len() != 1 {
        return None;
    }
    match term.factors[0].unparenthesized() {
        RExpr::Number { value, .. } if *value == 1.0 => Some(true),
        RExpr::Number { value, .. } if *value == 0.0 => Some(false),
        _ => None,
    }
}

fn classify(factors: &[RExpr]) -> TermKind {
    if factors.len() > 1 {
        return TermKind::Interaction;
    }
    match factors.first().map(RExpr::unparenthesized) {
        Some(RExpr::Ident(name)) if name == "." => TermKind::Dot,
        Some(RExpr::Ident(_)) => TermKind::Main,
        _ => TermKind::Transform,
    }
}

fn expand_terms(
    expr: &RExpr,
    removed: bool,
    out: &mut Vec<RawTerm>,
    line: usize,
    column: usize,
) -> gmeow_errors::Result<()> {
    match expr {
        RExpr::Paren(inner) => expand_terms(inner, removed, out, line, column),
        RExpr::Unary {
            op: UnaryOp::Plus,
            operand,
        } => expand_terms(operand, removed, out, line, column),
        RExpr::Unary {
            op: UnaryOp::Negate,
            operand,
        } => expand_terms(operand, !removed, out, line, column),
        RExpr::Binary { op, lhs, rhs } => match op {
            BinaryOp::Add => {
                expand_terms(lhs, removed, out, line, column)?;
                expand_terms(rhs, removed, out, line, column)
            }
            BinaryOp::Subtract => {
                expand_terms(lhs, removed, out, line, column)?;
                expand_terms(rhs, !removed, out, line, column)
            }
            BinaryOp::Multiply => {
                // `a * b` ≡ `a + b + a:b` — R's crossing operator.
                let left = collect(lhs, removed, line, column)?;
                let right = collect(rhs, removed, line, column)?;
                out.extend(left.iter().cloned());
                out.extend(right.iter().cloned());
                out.extend(cross(&left, &right));
                Ok(())
            }
            BinaryOp::Divide => {
                // `a / b` ≡ `a + a:b` — R's nesting operator.
                let left = collect(lhs, removed, line, column)?;
                let right = collect(rhs, removed, line, column)?;
                out.extend(left.iter().cloned());
                out.extend(cross(&left, &right));
                Ok(())
            }
            BinaryOp::Sequence => {
                // `a : b` ≡ the single interaction term.
                let left = collect(lhs, removed, line, column)?;
                let right = collect(rhs, removed, line, column)?;
                out.extend(cross(&left, &right));
                Ok(())
            }
            BinaryOp::Power => {
                let order = match rhs.unparenthesized() {
                    RExpr::Number { value, .. }
                        if *value >= 1.0
                            && value.fract() == 0.0
                            && *value <= f64::from(u32::MAX) =>
                    {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let order = *value as u32;
                        order
                    }
                    _ => {
                        return Err(parse_error(
                            line,
                            column,
                            "a formula `^` crossing order must be a positive whole number",
                        ));
                    }
                };
                if order > MAX_CROSSING_ORDER {
                    return Err(parse_error(
                        line,
                        column,
                        format!(
                            "formula crossing order {order} exceeds the supported maximum \
                             {MAX_CROSSING_ORDER}"
                        ),
                    ));
                }
                let base = collect(lhs, removed, line, column)?;
                out.extend(crossing_to_order(&base, order));
                Ok(())
            }
            _ => {
                out.push(RawTerm {
                    factors: vec![expr.clone()],
                    removed,
                });
                Ok(())
            }
        },
        _ => {
            out.push(RawTerm {
                factors: vec![expr.clone()],
                removed,
            });
            Ok(())
        }
    }
}

fn collect(
    expr: &RExpr,
    removed: bool,
    line: usize,
    column: usize,
) -> gmeow_errors::Result<Vec<RawTerm>> {
    let mut out = Vec::new();
    expand_terms(expr, removed, &mut out, line, column)?;
    Ok(out)
}

/// The pairwise interaction of two term sets, concatenating factors without duplication.
fn cross(left: &[RawTerm], right: &[RawTerm]) -> Vec<RawTerm> {
    let mut out = Vec::new();
    for a in left {
        for b in right {
            let mut factors = a.factors.clone();
            let mut keys: Vec<String> = factors.iter().map(RExpr::structure_key).collect();
            for f in &b.factors {
                let key = f.structure_key();
                if !keys.contains(&key) {
                    keys.push(key);
                    factors.push(f.clone());
                }
            }
            out.push(RawTerm {
                factors,
                removed: a.removed || b.removed,
            });
        }
    }
    out
}

/// `(a + b + c)^n`: every interaction of the base terms up to order `n`.
fn crossing_to_order(base: &[RawTerm], order: u32) -> Vec<RawTerm> {
    let mut out: Vec<RawTerm> = base.to_vec();
    let mut seen: Vec<String> = out.iter().map(raw_key).collect();
    let mut frontier: Vec<RawTerm> = base.to_vec();
    for _ in 1..order {
        let next = cross(&frontier, base);
        let mut fresh = Vec::new();
        for term in next {
            let key = raw_key(&term);
            if !seen.contains(&key) {
                seen.push(key);
                fresh.push(term.clone());
                out.push(term);
            }
        }
        if fresh.is_empty() {
            break;
        }
        frontier = fresh;
    }
    out
}

fn raw_key(term: &RawTerm) -> String {
    FormulaTerm {
        factors: term.factors.clone(),
        kind: TermKind::Main,
    }
    .dedup_key()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(src: &str) -> RScript {
        parse(src).expect("parses")
    }

    fn only(src: &str) -> RExpr {
        let s = script(src);
        assert_eq!(s.statements.len(), 1, "expected one statement in `{src}`");
        s.statements[0].expression().clone()
    }

    fn formula(src: &str) -> Formula {
        match only(src) {
            RExpr::Formula(f) => *f,
            other => panic!("expected a formula, got {other:?}"),
        }
    }

    fn term_names(f: &Formula) -> Vec<String> {
        f.terms
            .iter()
            .map(|t| {
                t.factors
                    .iter()
                    .map(|e| match e.unparenthesized() {
                        RExpr::Ident(n) => n.clone(),
                        other => other.structure_key(),
                    })
                    .collect::<Vec<_>>()
                    .join(":")
            })
            .collect()
    }

    #[test]
    fn every_assignment_form_parses_to_one_statement() {
        for (src, kind) in [
            ("x <- 1", AssignKind::Left),
            ("x <<- 1", AssignKind::SuperLeft),
            ("x = 1", AssignKind::Equals),
            ("1 -> x", AssignKind::Right),
            ("1 ->> x", AssignKind::SuperRight),
        ] {
            let s = script(src);
            match &s.statements[0].kind {
                RStmtKind::Assign {
                    target, kind: got, ..
                } => {
                    assert_eq!(*got, kind, "{src}");
                    assert_eq!(*target, RExpr::Ident("x".to_owned()), "{src}");
                }
                other => panic!("{src} did not parse as an assignment: {other:?}"),
            }
        }
    }

    #[test]
    fn a_call_carries_positional_and_named_arguments_in_order() {
        let RExpr::Call { callee, args } = only("lm(mpg ~ wt, data = mtcars)") else {
            panic!("expected a call")
        };
        assert_eq!(*callee, RExpr::Ident("lm".to_owned()));
        assert_eq!(args.len(), 2);
        assert!(args[0].name.is_none());
        assert!(matches!(args[0].value, Some(RExpr::Formula(_))));
        assert_eq!(args[1].name.as_deref(), Some("data"));
        assert_eq!(args[1].value, Some(RExpr::Ident("mtcars".to_owned())));
    }

    #[test]
    fn an_empty_subscript_argument_is_kept_not_dropped() {
        let RExpr::Index { args, double, .. } = only("m[, 1]") else {
            panic!("expected a subscript")
        };
        assert!(!double);
        assert_eq!(args.len(), 2);
        assert!(args[0].value.is_none(), "the empty subscript survives");
        assert!(args[1].value.is_some());
    }

    #[test]
    fn double_bracket_and_component_accessors_parse() {
        assert!(matches!(only("x[[1]]"), RExpr::Index { double: true, .. }));
        let RExpr::Component { name, slot, .. } = only("fit$residuals") else {
            panic!("expected a `$` component")
        };
        assert_eq!(name, "residuals");
        assert!(!slot);
        assert!(matches!(
            only("obj@slot"),
            RExpr::Component { slot: true, .. }
        ));
    }

    #[test]
    fn namespaced_names_parse() {
        assert_eq!(
            only("stats::lm"),
            RExpr::Namespace {
                package: "stats".to_owned(),
                name: "lm".to_owned(),
                internal: false
            }
        );
        assert!(matches!(
            only("broom:::tidy"),
            RExpr::Namespace { internal: true, .. }
        ));
    }

    #[test]
    fn arithmetic_honours_r_precedence_and_associativity() {
        // `-2^2` is `-(2^2)`; `^` is right-associative; `*` binds tighter than `+`.
        assert_eq!(only("-2^2").structure_key(), only("-(2^2)").structure_key());
        assert_eq!(
            only("2^3^2").structure_key(),
            only("2^(3^2)").structure_key()
        );
        assert_eq!(
            only("a + b * c").structure_key(),
            only("a + (b * c)").structure_key()
        );
        assert_eq!(
            only("a:b + c").structure_key(),
            only("(a:b) + c").structure_key()
        );
        assert_eq!(
            only("!a == b").structure_key(),
            only("!(a == b)").structure_key()
        );
    }

    #[test]
    fn a_two_sided_formula_indexes_its_terms_in_source_order() {
        let f = formula("mpg ~ wt + hp");
        assert_eq!(f.response, Some(RExpr::Ident("mpg".to_owned())));
        assert_eq!(term_names(&f), vec!["wt", "hp"]);
        assert!(f.intercept);
        assert!(f.terms.iter().all(|t| t.kind == TermKind::Main));
    }

    #[test]
    fn a_one_sided_formula_has_no_response() {
        let f = formula("~ x");
        assert!(f.response.is_none());
        assert_eq!(term_names(&f), vec!["x"]);
    }

    #[test]
    fn crossing_expands_to_main_effects_plus_the_interaction() {
        let f = formula("y ~ x1 * x2");
        assert_eq!(term_names(&f), vec!["x1", "x2", "x1:x2"]);
        assert_eq!(f.terms[2].kind, TermKind::Interaction);
    }

    #[test]
    fn the_colon_operator_is_interaction_inside_a_formula() {
        let f = formula("y ~ x1:x2");
        assert_eq!(term_names(&f), vec!["x1:x2"]);
        assert_eq!(f.terms[0].kind, TermKind::Interaction);
    }

    #[test]
    fn nesting_expands_to_the_outer_term_plus_the_interaction() {
        let f = formula("y ~ a / b");
        assert_eq!(term_names(&f), vec!["a", "a:b"]);
    }

    #[test]
    fn crossing_to_an_order_produces_every_interaction_up_to_it() {
        let f = formula("y ~ (a + b + c)^2");
        assert_eq!(term_names(&f), vec!["a", "b", "c", "a:b", "a:c", "b:c"]);
        assert!(parse("y ~ (a + b)^99").is_err(), "an unbounded order fails");
    }

    #[test]
    fn the_dot_term_and_a_removal_both_survive_structurally() {
        let f = formula("y ~ . - x3");
        assert_eq!(f.terms.len(), 1);
        assert_eq!(f.terms[0].kind, TermKind::Dot);
        assert_eq!(f.removed.len(), 1, "`- x3` is retained as a removal");
    }

    #[test]
    fn a_removal_deletes_a_matching_expanded_term() {
        let f = formula("y ~ a + b - b");
        assert_eq!(term_names(&f), vec!["a"]);
    }

    #[test]
    fn the_intercept_switch_is_a_flag_not_a_term() {
        assert!(!formula("y ~ x - 1").intercept);
        assert!(!formula("y ~ x + 0").intercept);
        assert!(formula("y ~ x + 1").intercept);
        assert_eq!(term_names(&formula("y ~ x - 1")), vec!["x"]);
    }

    #[test]
    fn a_transformed_term_keeps_its_inner_expression() {
        let f = formula("y ~ I(x^2) + log(z)");
        assert_eq!(f.terms.len(), 2);
        assert!(f.terms.iter().all(|t| t.kind == TermKind::Transform));
        let RExpr::Call { callee, args } = f.terms[0].factors[0].unparenthesized() else {
            panic!("expected I(...)")
        };
        assert_eq!(**callee, RExpr::Ident("I".to_owned()));
        assert!(matches!(
            args[0].value.as_ref().map(RExpr::unparenthesized),
            Some(RExpr::Binary {
                op: BinaryOp::Power,
                ..
            })
        ));
    }

    #[test]
    fn duplicate_terms_collapse() {
        assert_eq!(term_names(&formula("y ~ x + x")), vec!["x"]);
    }

    #[test]
    fn control_flow_forms_parse_and_are_recognized() {
        for src in [
            "if (x > 1) y else z",
            "for (i in 1:10) f(i)",
            "while (x < 3) x <- x + 1",
            "repeat break",
            "function(a, b = 2) a + b",
            "{ a; b }",
        ] {
            let e = only(src);
            assert!(e.is_control_flow(), "`{src}` must route to logic:");
        }
    }

    #[test]
    fn a_block_body_holds_its_own_statements() {
        let RExpr::Function { params, body } = only("function(x, y = 1) {\n  z <- x + y\n  z\n}")
        else {
            panic!("expected a function literal")
        };
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "x");
        assert!(params[1].default.is_some());
        let RExpr::Block(stmts) = *body else {
            panic!("expected a block body")
        };
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn both_pipes_parse_and_desugar_to_ordinary_calls() {
        let RExpr::Pipe { lhs, rhs, native } = only("mtcars %>% lm(mpg ~ wt, data = .)") else {
            panic!("expected a magrittr pipe")
        };
        assert!(!native);
        let desugared = desugar_pipe(&lhs, &rhs, native);
        assert_eq!(
            desugared.structure_key(),
            only("lm(mpg ~ wt, data = mtcars)").structure_key(),
            "the `.` placeholder receives the piped value"
        );

        let RExpr::Pipe { lhs, rhs, native } = only("x |> sum()") else {
            panic!("expected a native pipe")
        };
        assert!(native);
        assert_eq!(
            desugar_pipe(&lhs, &rhs, native).structure_key(),
            only("sum(x)").structure_key()
        );
    }

    #[test]
    fn a_pipe_without_a_placeholder_inserts_first() {
        let RExpr::Pipe { lhs, rhs, native } = only("mtcars %>% summary()") else {
            panic!("expected a pipe")
        };
        assert_eq!(
            desugar_pipe(&lhs, &rhs, native).structure_key(),
            only("summary(mtcars)").structure_key()
        );
    }

    #[test]
    fn a_newline_inside_a_call_continues_the_statement() {
        let s = script("fit <- lm(\n  mpg ~ wt,\n  data = mtcars\n)\n");
        assert_eq!(s.statements.len(), 1);
    }

    #[test]
    fn a_multi_statement_script_splits_on_newlines_and_semicolons() {
        let s = script("a <- 1\nb <- 2; c <- 3\n\n# comment\nd <- 4\n");
        assert_eq!(s.statements.len(), 4);
        assert_eq!(s.statements[3].line, 5);
    }

    #[test]
    fn a_malformed_script_is_a_positioned_hard_failure() {
        for src in [
            "lm(mpg ~ wt",
            "x <- ",
            "f(a b)",
            "{ a",
            "if (x",
            "for (1 in x) y",
        ] {
            let err = parse(src).expect_err("must not parse");
            assert!(
                format!("{err}").contains("R parse failure at line"),
                "`{src}` produced {err}"
            );
        }
    }

    #[test]
    fn the_structure_key_separates_and_identifies_expressions() {
        assert_eq!(
            only("log(x)").structure_key(),
            only("log(x)").structure_key()
        );
        assert_ne!(
            only("log(x)").structure_key(),
            only("log(y)").structure_key()
        );
        assert_eq!(only("(a)").structure_key(), only("a").structure_key());
    }
}
