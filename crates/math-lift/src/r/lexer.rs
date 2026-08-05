// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The R lexer: source characters → a positioned token stream.
//!
//! Hand-written, single pass, no dependency. R's tokenizer has three properties that a
//! generic scanner gets wrong, and all three are load-bearing for the statistical subset:
//!
//! - **`<-` is whitespace-sensitive.** `x <-1` assigns; `x < -1` compares. R decides by
//!   adjacency, so [`Lexer`] only fuses `<` with a `-` that is the very next character.
//! - **Newlines are statement terminators, but only outside `(` and `[`.** R continues a
//!   line that is still inside a call or a subscript, so the scanner carries a delimiter
//!   stack and suppresses a newline whose innermost open delimiter is a paren or a bracket.
//!   A `{` block does NOT suppress: its newlines separate the statements inside it.
//! - **`%…%` is an open operator class.** `%>%`, `%in%`, `%%`, `%/%`, and any user infix
//!   share one lexical shape, so they are scanned as one token carrying the operator text
//!   rather than enumerated.
//!
//! Every failure is an [`RParse`] carrying the exact line and column. There is no error
//! recovery and no "skip the bad token" mode — `MATHEMATICS-RUNTIME.md`'s ingestion rules
//! forbid a degraded parse, so an unterminated string is a hard failure, not a warning.

use gmeow_errors::Diag;

use crate::error::RParse;

/// Build the crate's typed parse diagnostic with a precise source position.
#[must_use]
pub fn parse_error(line: usize, column: usize, message: impl std::fmt::Display) -> Diag {
    Diag::of_kind(RParse {
        detail: format!("R parse failure at line {line}, column {column}: {message}"),
    })
}

/// A lexical operator: every non-delimiter punctuation form R's statistical subset uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `<-`, the ordinary left assignment.
    Assign,
    /// `<<-`, the enclosing-scope left assignment.
    SuperAssign,
    /// `->`, the right assignment.
    RightAssign,
    /// `->>`, the enclosing-scope right assignment.
    SuperRightAssign,
    /// `=`, assignment at statement level and the named-argument marker inside a call.
    Equals,
    /// `~`, the model-formula binder.
    Tilde,
    /// `+`, unary or binary.
    Plus,
    /// `-`, unary or binary.
    Minus,
    /// `*`.
    Star,
    /// `/`.
    Slash,
    /// `^`, right-associative exponentiation.
    Caret,
    /// `:`, the sequence operator and (inside a formula) the interaction operator.
    Colon,
    /// `::`, the exported-namespace accessor.
    DoubleColon,
    /// `:::`, the internal-namespace accessor.
    TripleColon,
    /// `$`, list/data-frame component extraction.
    Dollar,
    /// `@`, S4 slot extraction.
    At,
    /// `!`, logical negation.
    Bang,
    /// `&`, vectorized conjunction.
    And,
    /// `&&`, scalar conjunction.
    AndAnd,
    /// `|`, vectorized disjunction.
    Or,
    /// `||`, scalar disjunction.
    OrOr,
    /// `<`.
    Less,
    /// `>`.
    Greater,
    /// `<=`.
    LessEqual,
    /// `>=`.
    GreaterEqual,
    /// `==`.
    EqualEqual,
    /// `!=`.
    NotEqual,
    /// `|>`, the native pipe.
    NativePipe,
}

/// One token of R source.
#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// An identifier, including a backtick-quoted one and the bare `.` used in formulas.
    Ident(String),
    /// A numeric literal, with the `L` integer suffix recorded rather than discarded.
    Number {
        /// The literal's value.
        value: f64,
        /// Whether the literal carried R's `L` integer suffix.
        integer: bool,
        /// The literal's source text, kept so the emitted decimal is the authored one.
        text: String,
    },
    /// A string literal, with escapes already resolved.
    Str(String),
    /// `TRUE` / `T`.
    True,
    /// `FALSE` / `F`.
    False,
    /// `NULL`.
    Null,
    /// `NA` and its typed variants (`NA_integer_`, `NA_real_`, `NA_character_`).
    Na,
    /// `NaN`.
    NotANumber,
    /// `Inf`.
    Infinity,
    /// `if`.
    If,
    /// `else`.
    Else,
    /// `for`.
    For,
    /// `while`.
    While,
    /// `repeat`.
    Repeat,
    /// `function`.
    Function,
    /// `break`.
    Break,
    /// `next`.
    Next,
    /// `in`.
    In,
    /// A punctuation operator.
    Op(Op),
    /// A `%…%` infix operator, carrying its full text (`%>%`, `%in%`, `%%`, `%/%`, …).
    Special(String),
    /// `(`.
    LParen,
    /// `)`.
    RParen,
    /// `{`.
    LBrace,
    /// `}`.
    RBrace,
    /// `[`.
    LBracket,
    /// `[[`.
    DoubleLBracket,
    /// `]`. A `]]` is two of these, so `x[y[1]]` and `x[[1]]` both close correctly.
    RBracket,
    /// `,`.
    Comma,
    /// `;`.
    Semicolon,
    /// A statement-terminating newline (suppressed inside `(` and `[`).
    Newline,
}

/// A token together with the 1-based source position it starts at.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The token itself.
    pub tok: Tok,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub column: usize,
}

/// The scanner state.
///
/// Public so a caller can lex without parsing (the parse tier is testable on its own), but
/// the ordinary entry point is [`lex`].
#[derive(Debug)]
pub struct Lexer {
    src: Vec<char>,
    i: usize,
    line: usize,
    column: usize,
    /// The open-delimiter stack. `(`/`[` suppress newlines; `{` does not.
    delims: Vec<char>,
}

/// Scan `source` into a token stream.
///
/// # Errors
///
/// [`RParse`] on an unterminated string, an unterminated `%…%` operator, an unterminated
/// backtick name, a malformed numeric literal, or a character R's grammar does not admit.
pub fn lex(source: &str) -> gmeow_errors::Result<Vec<Token>> {
    Lexer::new(source).run()
}

impl Lexer {
    /// A scanner over `source`.
    #[must_use]
    pub fn new(source: &str) -> Self {
        Self {
            src: source.chars().collect(),
            i: 0,
            line: 1,
            column: 1,
            delims: Vec::new(),
        }
    }

    /// Consume the whole source.
    ///
    /// # Errors
    ///
    /// See [`lex`].
    pub fn run(mut self) -> gmeow_errors::Result<Vec<Token>> {
        let mut out = Vec::new();
        while let Some(c) = self.peek(0) {
            match c {
                ' ' | '\t' | '\r' | '\u{0c}' => {
                    self.bump();
                }
                '\n' => {
                    let (line, column) = (self.line, self.column);
                    self.bump();
                    if self.newline_is_significant() {
                        push(&mut out, Tok::Newline, line, column);
                    }
                }
                '#' => {
                    while let Some(c) = self.peek(0) {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                '"' | '\'' => {
                    let (line, column) = (self.line, self.column);
                    let s = self.string(c)?;
                    push(&mut out, Tok::Str(s), line, column);
                }
                '`' => {
                    let (line, column) = (self.line, self.column);
                    let name = self.backtick_name()?;
                    push(&mut out, Tok::Ident(name), line, column);
                }
                '%' => {
                    let (line, column) = (self.line, self.column);
                    let text = self.special_operator()?;
                    push(&mut out, Tok::Special(text), line, column);
                }
                '0'..='9' => {
                    let (line, column) = (self.line, self.column);
                    let tok = self.number()?;
                    push(&mut out, tok, line, column);
                }
                '.' if self.peek(1).is_some_and(|d| d.is_ascii_digit()) => {
                    let (line, column) = (self.line, self.column);
                    let tok = self.number()?;
                    push(&mut out, tok, line, column);
                }
                c if is_ident_start(c) => {
                    let (line, column) = (self.line, self.column);
                    let word = self.identifier();
                    push(&mut out, keyword_or_ident(word), line, column);
                }
                _ => {
                    let (line, column) = (self.line, self.column);
                    let tok = self.punctuation()?;
                    push(&mut out, tok, line, column);
                }
            }
        }
        Ok(out)
    }

    /// A newline terminates a statement unless the innermost open delimiter is `(` or `[`.
    fn newline_is_significant(&self) -> bool {
        !matches!(self.delims.last(), Some('(' | '['))
    }

    fn peek(&self, k: usize) -> Option<char> {
        self.src.get(self.i + k).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.src.get(self.i).copied()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn identifier(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek(0) {
            if is_ident_continue(c) {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        s
    }

    fn backtick_name(&mut self) -> gmeow_errors::Result<String> {
        let (line, column) = (self.line, self.column);
        self.bump();
        let mut s = String::new();
        loop {
            match self.bump() {
                Some('`') => return Ok(s),
                Some('\n') | None => {
                    return Err(parse_error(
                        line,
                        column,
                        "unterminated backtick-quoted name: a `…` name must close on its own line",
                    ));
                }
                Some(c) => s.push(c),
            }
        }
    }

    fn special_operator(&mut self) -> gmeow_errors::Result<String> {
        let (line, column) = (self.line, self.column);
        self.bump();
        let mut s = String::from("%");
        loop {
            match self.bump() {
                Some('%') => {
                    s.push('%');
                    return Ok(s);
                }
                Some('\n') | None => {
                    return Err(parse_error(
                        line,
                        column,
                        "unterminated %…% infix operator: it must close with a second `%` on the \
                         same line",
                    ));
                }
                Some(c) => s.push(c),
            }
        }
    }

    fn string(&mut self, quote: char) -> gmeow_errors::Result<String> {
        let (line, column) = (self.line, self.column);
        self.bump();
        let mut s = String::new();
        loop {
            let Some(c) = self.bump() else {
                return Err(parse_error(
                    line,
                    column,
                    format!(
                        "unterminated string literal: no closing `{quote}` before end of input"
                    ),
                ));
            };
            if c == quote {
                return Ok(s);
            }
            if c != '\\' {
                s.push(c);
                continue;
            }
            let Some(esc) = self.bump() else {
                return Err(parse_error(
                    self.line,
                    self.column,
                    "unterminated escape sequence at end of input",
                ));
            };
            match esc {
                'n' => s.push('\n'),
                't' => s.push('\t'),
                'r' => s.push('\r'),
                '0' => s.push('\0'),
                'a' => s.push('\u{7}'),
                'b' => s.push('\u{8}'),
                'f' => s.push('\u{c}'),
                'v' => s.push('\u{b}'),
                '\\' => s.push('\\'),
                '"' => s.push('"'),
                '\'' => s.push('\''),
                '`' => s.push('`'),
                'x' => s.push(self.hex_escape(2)?),
                'u' => s.push(self.brace_or_fixed_hex_escape(4)?),
                'U' => s.push(self.brace_or_fixed_hex_escape(8)?),
                other => {
                    return Err(parse_error(
                        self.line,
                        self.column,
                        format!("unsupported string escape `\\{other}`"),
                    ));
                }
            }
        }
    }

    fn hex_escape(&mut self, max_digits: usize) -> gmeow_errors::Result<char> {
        let (line, column) = (self.line, self.column);
        let mut value: u32 = 0;
        let mut digits = 0;
        while digits < max_digits {
            let Some(c) = self.peek(0) else { break };
            let Some(d) = c.to_digit(16) else { break };
            value = value * 16 + d;
            digits += 1;
            self.bump();
        }
        if digits == 0 {
            return Err(parse_error(
                line,
                column,
                "hex escape with no hexadecimal digits",
            ));
        }
        char::from_u32(value).ok_or_else(|| {
            parse_error(
                line,
                column,
                format!("hex escape \\{value:x} is not a Unicode scalar value"),
            )
        })
    }

    fn brace_or_fixed_hex_escape(&mut self, max_digits: usize) -> gmeow_errors::Result<char> {
        if self.peek(0) == Some('{') {
            let (line, column) = (self.line, self.column);
            self.bump();
            let c = self.hex_escape(max_digits)?;
            if self.bump() != Some('}') {
                return Err(parse_error(line, column, "unclosed `\\u{…}` escape"));
            }
            return Ok(c);
        }
        self.hex_escape(max_digits)
    }

    fn number(&mut self) -> gmeow_errors::Result<Tok> {
        let (line, column) = (self.line, self.column);
        let mut text = String::new();

        if self.peek(0) == Some('0') && matches!(self.peek(1), Some('x' | 'X')) {
            text.push(self.bump().unwrap_or('0'));
            text.push(self.bump().unwrap_or('x'));
            let mut digits = String::new();
            while let Some(c) = self.peek(0) {
                if c.is_ascii_hexdigit() {
                    digits.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
            if digits.is_empty() {
                return Err(parse_error(
                    line,
                    column,
                    "hexadecimal literal `0x` with no digits",
                ));
            }
            let value = u64::from_str_radix(&digits, 16).map_err(|e| {
                parse_error(line, column, format!("malformed hexadecimal literal: {e}"))
            })?;
            let integer = matches!(self.peek(0), Some('L'));
            if integer {
                self.bump();
            }
            #[allow(clippy::cast_precision_loss)]
            let value = value as f64;
            return Ok(Tok::Number {
                value,
                integer,
                text: format_source_decimal(value),
            });
        }

        while let Some(c) = self.peek(0) {
            if c.is_ascii_digit() {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if self.peek(0) == Some('.') {
            text.push('.');
            self.bump();
            while let Some(c) = self.peek(0) {
                if c.is_ascii_digit() {
                    text.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        if matches!(self.peek(0), Some('e' | 'E')) {
            let mut exponent = String::new();
            exponent.push('e');
            self.bump();
            if matches!(self.peek(0), Some('+' | '-')) {
                exponent.push(self.bump().unwrap_or('+'));
            }
            let mut digits = 0;
            while let Some(c) = self.peek(0) {
                if c.is_ascii_digit() {
                    exponent.push(c);
                    digits += 1;
                    self.bump();
                } else {
                    break;
                }
            }
            if digits == 0 {
                return Err(parse_error(
                    line,
                    column,
                    "numeric literal exponent has no digits",
                ));
            }
            text.push_str(&exponent);
        }

        let value: f64 = text
            .parse()
            .map_err(|e| parse_error(line, column, format!("malformed numeric literal: {e}")))?;
        if !value.is_finite() {
            return Err(parse_error(
                line,
                column,
                format!("numeric literal `{text}` is not finite"),
            ));
        }
        let integer = matches!(self.peek(0), Some('L'));
        if integer || matches!(self.peek(0), Some('i')) {
            self.bump();
        }
        Ok(Tok::Number {
            value,
            integer,
            text: format_source_decimal(value),
        })
    }

    fn punctuation(&mut self) -> gmeow_errors::Result<Tok> {
        let (line, column) = (self.line, self.column);
        let c = self.peek(0).unwrap_or(' ');
        let c1 = self.peek(1);
        let c2 = self.peek(2);

        // Three-character forms first: `<<-`, `->>`, `:::`.
        let three = match (c, c1, c2) {
            ('<', Some('<'), Some('-')) => Some(Tok::Op(Op::SuperAssign)),
            ('-', Some('>'), Some('>')) => Some(Tok::Op(Op::SuperRightAssign)),
            (':', Some(':'), Some(':')) => Some(Tok::Op(Op::TripleColon)),
            _ => None,
        };
        if let Some(tok) = three {
            self.bump();
            self.bump();
            self.bump();
            return Ok(tok);
        }

        // Two-character forms. `<-`/`->` fuse ONLY on adjacency, which is what keeps
        // `x < -1` a comparison and `x <-1` an assignment.
        let two = match (c, c1) {
            ('<', Some('-')) => Some(Tok::Op(Op::Assign)),
            ('-', Some('>')) => Some(Tok::Op(Op::RightAssign)),
            ('<', Some('=')) => Some(Tok::Op(Op::LessEqual)),
            ('>', Some('=')) => Some(Tok::Op(Op::GreaterEqual)),
            ('=', Some('=')) => Some(Tok::Op(Op::EqualEqual)),
            ('!', Some('=')) => Some(Tok::Op(Op::NotEqual)),
            ('&', Some('&')) => Some(Tok::Op(Op::AndAnd)),
            ('|', Some('|')) => Some(Tok::Op(Op::OrOr)),
            ('|', Some('>')) => Some(Tok::Op(Op::NativePipe)),
            (':', Some(':')) => Some(Tok::Op(Op::DoubleColon)),
            ('[', Some('[')) => Some(Tok::DoubleLBracket),
            _ => None,
        };
        if let Some(tok) = two {
            self.bump();
            self.bump();
            if tok == Tok::DoubleLBracket {
                // Two opens, because `]]` scans as two `]`.
                self.delims.push('[');
                self.delims.push('[');
            }
            return Ok(tok);
        }

        let single = match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            ',' => Tok::Comma,
            ';' => Tok::Semicolon,
            '+' => Tok::Op(Op::Plus),
            '-' => Tok::Op(Op::Minus),
            '*' => Tok::Op(Op::Star),
            '/' => Tok::Op(Op::Slash),
            '^' => Tok::Op(Op::Caret),
            '~' => Tok::Op(Op::Tilde),
            '=' => Tok::Op(Op::Equals),
            '<' => Tok::Op(Op::Less),
            '>' => Tok::Op(Op::Greater),
            '!' => Tok::Op(Op::Bang),
            '&' => Tok::Op(Op::And),
            '|' => Tok::Op(Op::Or),
            ':' => Tok::Op(Op::Colon),
            '$' => Tok::Op(Op::Dollar),
            '@' => Tok::Op(Op::At),
            other => {
                return Err(parse_error(
                    line,
                    column,
                    format!("character `{other}` is not part of R's grammar"),
                ));
            }
        };
        self.bump();
        match single {
            Tok::LParen => self.delims.push('('),
            Tok::LBracket => self.delims.push('['),
            Tok::LBrace => self.delims.push('{'),
            Tok::RParen | Tok::RBracket | Tok::RBrace if self.delims.pop().is_none() => {
                return Err(parse_error(
                    line,
                    column,
                    format!("unbalanced closing delimiter `{c}`"),
                ));
            }
            _ => {}
        }
        Ok(single)
    }
}

fn push(out: &mut Vec<Token>, tok: Tok, line: usize, column: usize) {
    out.push(Token { tok, line, column });
}

/// R identifiers start with a letter or `.`; `_` is accepted only after the first byte.
fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '.'
}

fn is_ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '.' || c == '_'
}

fn keyword_or_ident(word: String) -> Tok {
    match word.as_str() {
        "if" => Tok::If,
        "else" => Tok::Else,
        "for" => Tok::For,
        "while" => Tok::While,
        "repeat" => Tok::Repeat,
        "function" => Tok::Function,
        "break" => Tok::Break,
        "next" => Tok::Next,
        "in" => Tok::In,
        "TRUE" | "T" => Tok::True,
        "FALSE" | "F" => Tok::False,
        "NULL" => Tok::Null,
        "NA" | "NA_integer_" | "NA_real_" | "NA_character_" => Tok::Na,
        "NaN" => Tok::NotANumber,
        "Inf" => Tok::Infinity,
        _ => Tok::Ident(word),
    }
}

/// A canonical, exponent-free decimal for a scanned literal.
///
/// The lift emits numeric leaves as `xsd:decimal`, whose lexical space has no exponent
/// form, so `1e5` must land as `100000.0` rather than as its source spelling.
fn format_source_decimal(value: f64) -> String {
    let s = format!("{value}");
    if s.contains(['e', 'E']) {
        // `{:.*}` never uses exponent notation. 17 significant digits round-trips an f64.
        let rendered = format!("{value:.17}");
        let trimmed = rendered.trim_end_matches('0');
        let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
        return if trimmed.contains('.') {
            trimmed.to_owned()
        } else {
            format!("{trimmed}.0")
        };
    }
    if s.contains('.') { s } else { format!("{s}.0") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Tok> {
        lex(src)
            .expect("lexes")
            .into_iter()
            .map(|t| t.tok)
            .collect()
    }

    #[test]
    fn comments_and_whitespace_carry_no_tokens() {
        assert_eq!(kinds("# a comment only"), Vec::<Tok>::new());
        assert_eq!(
            kinds("x # trailing\n"),
            vec![Tok::Ident("x".to_owned()), Tok::Newline]
        );
    }

    #[test]
    fn strings_resolve_both_quote_styles_and_escapes() {
        assert_eq!(
            kinds(r#""a\tb" 'c\'d' "\u{263A}""#),
            vec![
                Tok::Str("a\tb".to_owned()),
                Tok::Str("c'd".to_owned()),
                Tok::Str("\u{263a}".to_owned()),
            ]
        );
    }

    #[test]
    fn an_unterminated_string_is_a_positioned_hard_failure() {
        let err = lex("x <- \"oops").expect_err("must not lex");
        let text = format!("{err}");
        assert!(text.contains("line 1"), "{text}");
        assert!(text.contains("unterminated string literal"), "{text}");
    }

    #[test]
    fn numbers_cover_decimals_exponents_and_the_integer_suffix() {
        assert_eq!(
            kinds("1 2.5 1e5 3L .5"),
            vec![
                Tok::Number {
                    value: 1.0,
                    integer: false,
                    text: "1.0".to_owned()
                },
                Tok::Number {
                    value: 2.5,
                    integer: false,
                    text: "2.5".to_owned()
                },
                Tok::Number {
                    value: 100_000.0,
                    integer: false,
                    text: "100000.0".to_owned()
                },
                Tok::Number {
                    value: 3.0,
                    integer: true,
                    text: "3.0".to_owned()
                },
                Tok::Number {
                    value: 0.5,
                    integer: false,
                    text: "0.5".to_owned()
                },
            ]
        );
    }

    #[test]
    fn a_scanned_number_never_renders_in_exponent_form() {
        assert_eq!(format_source_decimal(1e5), "100000.0");
        assert_eq!(format_source_decimal(1e-7), "0.0000001");
        assert_eq!(format_source_decimal(2.0), "2.0");
    }

    #[test]
    fn the_literal_keywords_are_their_own_tokens() {
        assert_eq!(
            kinds("TRUE FALSE NA NULL NaN Inf"),
            vec![
                Tok::True,
                Tok::False,
                Tok::Na,
                Tok::Null,
                Tok::NotANumber,
                Tok::Infinity
            ]
        );
    }

    #[test]
    fn assignment_arrows_fuse_only_on_adjacency() {
        assert_eq!(
            kinds("x <- 1\n"),
            vec![
                Tok::Ident("x".to_owned()),
                Tok::Op(Op::Assign),
                Tok::Number {
                    value: 1.0,
                    integer: false,
                    text: "1.0".to_owned()
                },
                Tok::Newline,
            ]
        );
        assert_eq!(
            kinds("x < -1\n"),
            vec![
                Tok::Ident("x".to_owned()),
                Tok::Op(Op::Less),
                Tok::Op(Op::Minus),
                Tok::Number {
                    value: 1.0,
                    integer: false,
                    text: "1.0".to_owned()
                },
                Tok::Newline,
            ]
        );
        assert_eq!(
            kinds("1 -> x\n"),
            vec![
                Tok::Number {
                    value: 1.0,
                    integer: false,
                    text: "1.0".to_owned()
                },
                Tok::Op(Op::RightAssign),
                Tok::Ident("x".to_owned()),
                Tok::Newline,
            ]
        );
        assert_eq!(kinds("x <<- 1")[1], Tok::Op(Op::SuperAssign));
        assert_eq!(kinds("1 ->> x")[1], Tok::Op(Op::SuperRightAssign));
    }

    #[test]
    fn a_newline_inside_parens_or_brackets_is_suppressed() {
        assert!(!kinds("f(a,\n b)").contains(&Tok::Newline));
        assert!(!kinds("x[1,\n 2]").contains(&Tok::Newline));
        // A brace block's newlines DO separate its statements.
        assert_eq!(
            kinds("{\na\n}")
                .iter()
                .filter(|t| **t == Tok::Newline)
                .count(),
            2
        );
    }

    #[test]
    fn double_brackets_open_twice_so_the_close_pair_balances() {
        assert_eq!(
            kinds("x[[1]]\n"),
            vec![
                Tok::Ident("x".to_owned()),
                Tok::DoubleLBracket,
                Tok::Number {
                    value: 1.0,
                    integer: false,
                    text: "1.0".to_owned()
                },
                Tok::RBracket,
                Tok::RBracket,
                Tok::Newline,
            ]
        );
        assert!(!kinds("x[[1,\n2]]").contains(&Tok::Newline));
    }

    #[test]
    fn percent_operators_scan_as_one_token_with_their_text() {
        assert_eq!(
            kinds("a %>% b %in% c %/% d\n"),
            vec![
                Tok::Ident("a".to_owned()),
                Tok::Special("%>%".to_owned()),
                Tok::Ident("b".to_owned()),
                Tok::Special("%in%".to_owned()),
                Tok::Ident("c".to_owned()),
                Tok::Special("%/%".to_owned()),
                Tok::Ident("d".to_owned()),
                Tok::Newline,
            ]
        );
        let err = lex("a %oops\n").expect_err("must not lex");
        assert!(format!("{err}").contains("unterminated %"), "{err}");
    }

    #[test]
    fn a_bare_dot_is_an_identifier_but_a_dotted_number_is_a_number() {
        assert_eq!(kinds(". x")[0], Tok::Ident(".".to_owned()));
        assert!(matches!(kinds(".5")[0], Tok::Number { .. }));
    }

    #[test]
    fn a_backtick_name_scans_as_one_identifier() {
        assert_eq!(
            kinds("`my var` <- 1")[0],
            Tok::Ident("my var".to_owned()),
            "backtick names carry spaces"
        );
        assert!(lex("`unterminated").is_err());
    }

    #[test]
    fn an_illegal_character_is_a_positioned_hard_failure() {
        let err = lex("x <- 1\ny \u{a7} 2").expect_err("must not lex");
        let text = format!("{err}");
        assert!(text.contains("line 2"), "{text}");
        assert!(text.contains("not part of R's grammar"), "{text}");
    }

    #[test]
    fn namespace_and_pipe_operators_scan_at_full_length() {
        assert_eq!(kinds("stats::lm")[1], Tok::Op(Op::DoubleColon));
        assert_eq!(kinds("stats:::lm")[1], Tok::Op(Op::TripleColon));
        assert_eq!(kinds("a |> b")[1], Tok::Op(Op::NativePipe));
        assert_eq!(kinds("a:b")[1], Tok::Op(Op::Colon));
    }
}
