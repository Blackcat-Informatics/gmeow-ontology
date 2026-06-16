// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal IR, parser, and shared answer types for `.logic` query programs.
//!
//! # Grammar overview
//!
//! A `.logic` file is a small Prolog-ish program supporting:
//! - Comments: lines starting with `%` (and blank lines) are ignored.
//! - Prefix declarations: `:- prefix(ex, 'https://example.org/').`
//! - Rules: `head :- body1, body2, ... .` and facts `head.`
//! - Goal: exactly one `?- goalatom1, goalatom2, ... .`
//! - Atoms are binary predicates over RDF: `pred(Subject, Object)`.
//! - Cut: the body literal `!` is parsed as a [`QBodyLit::Cut`] marker.
//!
//! # Canonicalization
//!
//! - Subject/object constants: IRI → `<iri>` (angle-bracket form).
//! - Predicate IRIs: bare IRI string, no angle brackets.
//! - Variables: any token starting with uppercase or `_`.

use std::collections::BTreeMap;

use crate::seam::BudgetStatus;

// ── IR types ──────────────────────────────────────────────────────────────────

/// A term in a query atom: either a canonical constant string or a variable name.
///
/// For IRI constants the canonical form is `<iri>` (angle brackets).
/// For variables the string is the variable name as written in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QTerm {
    /// A canonical constant string.
    /// - IRI: `<https://example.org/alice>`
    Const(String),
    /// A logic variable name, e.g. `X`, `_Y`.
    Var(String),
}

/// A binary predicate atom over RDF.
///
/// `pred(Subject, Object)` maps to the triple `(Subject, predIRI, Object)`.
/// `pred` is the bare IRI string (no angle brackets); `args` always has length 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QAtom {
    /// Bare IRI string for the predicate (no angle brackets).
    pub pred: String,
    /// Exactly two terms: `[subject, object]`.
    pub args: Vec<QTerm>,
}

/// A body literal in a rule: either a predicate atom or the cut marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QBodyLit {
    /// A normal predicate atom.
    Atom(QAtom),
    /// The Prolog cut `!`. Procedural — not supported by the declarative oracle.
    Cut,
}

/// A rule: `head :- body1, body2, ... .`  or a fact `head.` (empty body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QRule {
    /// The rule head atom.
    pub head: QAtom,
    /// Body literals (empty for facts).
    pub body: Vec<QBodyLit>,
}

/// The conjunctive goal: `?- atom1, atom2, ... .`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QGoal {
    /// Goal atoms (conjuncts), left to right.
    pub atoms: Vec<QAtom>,
}

/// A complete parsed program: a set of rules and exactly one goal.
///
/// Prefix declarations are consumed during parsing; the resulting IRIs are
/// fully expanded in all atoms before this struct is constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QProgram {
    /// Rules and facts, in source order.
    pub rules: Vec<QRule>,
    /// The single conjunctive goal.
    pub goal: QGoal,
}

// ── Answer types ──────────────────────────────────────────────────────────────

/// A single variable binding: variable name → canonical constant string.
pub type Binding = BTreeMap<String, String>;

/// The result of resolving a [`QProgram`] against a world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerSet {
    /// All goal-variable bindings, in deterministic (sorted) order.
    pub bindings: Vec<Binding>,
    /// Whether resolution completed within budget.
    pub status: BudgetStatus,
}

impl AnswerSet {
    /// Sort `bindings` deterministically so output is stable across runs.
    ///
    /// BTreeMaps are already ordered by key; we sort the `Vec` by the
    /// serialized key/value pairs of each binding map.
    pub fn canonicalize(&mut self) {
        self.bindings.sort_by(|a, b| {
            // Compare lexicographically by (key, value) pairs in order.
            let a_pairs: Vec<(&String, &String)> = a.iter().collect();
            let b_pairs: Vec<(&String, &String)> = b.iter().collect();
            a_pairs.cmp(&b_pairs)
        });
    }
}

/// Execution budget for resolution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Budget {
    /// Maximum number of answer bindings to collect before stopping with `Partial`.
    pub max_answers: Option<usize>,
    /// Maximum resolution steps before stopping with `Exhausted`.
    pub max_steps: Option<u64>,
}

// ── Parser ────────────────────────────────────────────────────────────────────

/// Parse a `.logic` query program from source text.
///
/// The parser:
/// 1. Strips comment lines (`%`) and blank lines.
/// 2. Joins physical lines into logical clauses split by `.`.
/// 3. Dispatches each clause to the appropriate handler.
///
/// # Errors
///
/// Returns `Err(String)` on any malformed input.  Exactly one `?-` goal is
/// required; zero or more than one is an error.
///
/// # Panics
///
/// Never panics — all errors are returned as `Err(String)`.
pub fn parse_query_program(src: &str) -> Result<QProgram, String> {
    let mut prefixes: BTreeMap<String, String> = BTreeMap::new();
    let mut rules: Vec<QRule> = Vec::new();
    let mut goal: Option<QGoal> = None;

    // ── Phase 1: collect raw logical clauses ─────────────────────────────────
    // We join continuation lines into complete clauses terminated by `.`.
    let mut pending = String::new();
    let mut clauses: Vec<String> = Vec::new();

    for line in src.lines() {
        let trimmed = line.trim();
        // Skip comments and blank lines (only at top-level, not inside a pending clause).
        if pending.is_empty() && (trimmed.is_empty() || trimmed.starts_with('%')) {
            continue;
        }
        if !pending.is_empty() {
            pending.push(' ');
        }
        pending.push_str(trimmed);

        // A clause ends at a `.` that is not inside a quoted string.
        // We collect whole clauses (terminated by `.`) from `pending`.
        while let Some(dot_pos) = find_clause_end(&pending) {
            let clause = pending[..dot_pos].trim().to_owned();
            pending = pending[dot_pos + 1..].trim().to_owned();
            // Skip empty clauses (e.g. trailing dots).
            if !clause.is_empty() {
                clauses.push(clause);
            }
        }
    }

    // If there's a non-empty pending without a terminating dot, it's a parse error.
    if !pending.trim().is_empty() {
        return Err(format!(
            "unterminated clause (missing '.'): {:?}",
            pending.trim()
        ));
    }

    // ── Phase 2: dispatch each clause ────────────────────────────────────────
    for clause in clauses {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }

        if let Some(body) = clause.strip_prefix(":-") {
            // Directive: :- prefix(alias, 'iri').
            let body = body.trim();
            if let Some(pfx) = parse_prefix_directive(body)? {
                prefixes.insert(pfx.0, pfx.1);
            }
            // Unknown directives are silently ignored.
        } else if let Some(goal_body) = clause.strip_prefix("?-") {
            // Goal clause.
            if goal.is_some() {
                return Err("program has more than one ?- goal".to_owned());
            }
            let goal_body = goal_body.trim();
            let atoms = parse_atom_list(goal_body, &prefixes)?;
            if atoms.is_empty() {
                return Err("?- goal must have at least one atom".to_owned());
            }
            goal = Some(QGoal { atoms });
        } else {
            // Rule or fact.
            let rule = parse_rule(clause, &prefixes)?;
            rules.push(rule);
        }
    }

    let goal = goal.ok_or_else(|| "program has no ?- goal".to_owned())?;

    Ok(QProgram { rules, goal })
}

// ── Clause-end detector ───────────────────────────────────────────────────────

/// Find the position of the first clause-terminating `.` in `s`.
///
/// Skips `.` inside single-quoted strings (`'...'`) to avoid splitting on
/// IRIs with dots.  Returns the byte index of the `.` or `None`.
fn find_clause_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                // Skip single-quoted string.
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1; // skip escaped char
                    }
                    i += 1;
                }
                i += 1; // skip closing quote
            }
            b'.' => {
                // A `.` is a clause terminator if followed by whitespace, another
                // delimiter, or end-of-string — not if it's part of an IRI fragment.
                // Heuristic: if preceded by `)` or alphanumeric/closing-delim it's
                // a terminator.  We accept any `.` at this stage and rely on the
                // grammar to catch malformed input.
                return Some(i);
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

// ── Prefix directive parser ───────────────────────────────────────────────────

/// Parse a `prefix(alias, 'iri')` body (the part after `:-`).
///
/// Returns `Some((alias, iri))` on match, `None` if it's a different directive.
fn parse_prefix_directive(body: &str) -> Result<Option<(String, String)>, String> {
    // Expected form: `prefix(alias, 'https://...')`
    let body = body.trim();
    if !body.starts_with("prefix(") {
        return Ok(None);
    }
    let inner = body
        .strip_prefix("prefix(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| format!("malformed prefix directive: {body:?}"))?;

    let comma = inner
        .find(',')
        .ok_or_else(|| format!("prefix directive missing comma: {body:?}"))?;
    let alias = inner[..comma].trim().to_owned();
    let iri_part = inner[comma + 1..].trim();

    // IRI must be single-quoted.
    if !iri_part.starts_with('\'') || !iri_part.ends_with('\'') {
        return Err(format!("prefix IRI must be single-quoted in: {body:?}"));
    }
    let iri = iri_part[1..iri_part.len() - 1].to_owned();

    if alias.is_empty() {
        return Err(format!("prefix alias is empty in: {body:?}"));
    }
    if iri.is_empty() {
        return Err(format!("prefix IRI is empty in: {body:?}"));
    }

    Ok(Some((alias, iri)))
}

// ── Rule parser ───────────────────────────────────────────────────────────────

/// Parse a rule clause `head :- body1, body2, ... ` or a fact `head`.
fn parse_rule(clause: &str, prefixes: &BTreeMap<String, String>) -> Result<QRule, String> {
    if let Some(idx) = find_neck(clause) {
        let head_str = clause[..idx].trim();
        let body_str = clause[idx + 2..].trim();
        let head = parse_atom(head_str, prefixes)?;
        let body_lits = parse_body_lit_list(body_str, prefixes)?;
        Ok(QRule {
            head,
            body: body_lits,
        })
    } else {
        // Fact: no `:-` neck.
        let head = parse_atom(clause.trim(), prefixes)?;
        Ok(QRule { head, body: vec![] })
    }
}

/// Find the position of the `:-` neck that is not inside parentheses or quotes.
fn find_neck(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b':' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                return Some(i);
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

// ── Body literal list ─────────────────────────────────────────────────────────

/// Parse a comma-separated list of body literals (atoms or `!`).
fn parse_body_lit_list(
    s: &str,
    prefixes: &BTreeMap<String, String>,
) -> Result<Vec<QBodyLit>, String> {
    split_comma_top(s)
        .into_iter()
        .map(|tok| {
            let tok = tok.trim();
            if tok == "!" {
                Ok(QBodyLit::Cut)
            } else {
                parse_atom(tok, prefixes).map(QBodyLit::Atom)
            }
        })
        .collect()
}

/// Parse a comma-separated list of atoms (for `?-` goal).
fn parse_atom_list(s: &str, prefixes: &BTreeMap<String, String>) -> Result<Vec<QAtom>, String> {
    split_comma_top(s)
        .into_iter()
        .map(|tok| parse_atom(tok.trim(), prefixes))
        .collect()
}

/// Split `s` on commas that are not inside parentheses or quotes.
fn split_comma_top(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b',' if depth == 0 => {
                parts.push(&s[start..i]);
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }
    if start <= s.len() {
        parts.push(&s[start..]);
    }
    parts
}

// ── Atom parser ───────────────────────────────────────────────────────────────

/// Parse a single atom `pred(SubjTerm, ObjTerm)`.
///
/// The predicate may be a prefixed name (`ex:foo`) or a single-quoted IRI
/// (`'https://...'`).  Args must be exactly two terms.
fn parse_atom(s: &str, prefixes: &BTreeMap<String, String>) -> Result<QAtom, String> {
    let s = s.trim();
    // Find the opening paren.
    let open = s
        .find('(')
        .ok_or_else(|| format!("atom missing '(': {s:?}"))?;
    if !s.ends_with(')') {
        return Err(format!("atom missing closing ')': {s:?}"));
    }
    let pred_str = s[..open].trim();
    let args_str = s[open + 1..s.len() - 1].trim();

    let pred = resolve_iri(pred_str, prefixes)
        .ok_or_else(|| format!("cannot resolve predicate IRI {pred_str:?}"))?;
    // Predicate: bare IRI string (strip angle brackets if present).
    let pred = strip_angle_brackets(&pred);

    let arg_tokens = split_comma_top(args_str);
    if arg_tokens.len() != 2 {
        return Err(format!(
            "atom {s:?} has {} args; expected exactly 2",
            arg_tokens.len()
        ));
    }

    let args: Vec<QTerm> = arg_tokens
        .into_iter()
        .map(|tok| parse_term(tok.trim(), prefixes))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(QAtom { pred, args })
}

// ── Term parser ───────────────────────────────────────────────────────────────

/// Parse a single term (variable or constant).
fn parse_term(s: &str, prefixes: &BTreeMap<String, String>) -> Result<QTerm, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty term".to_owned());
    }

    // Variable: starts with uppercase ASCII letter or `_`.
    let first = s.chars().next().unwrap();
    if first.is_uppercase() || first == '_' {
        return Ok(QTerm::Var(s.to_owned()));
    }

    // Single-quoted full IRI: `'https://...'`
    if s.starts_with('\'') {
        if !s.ends_with('\'') || s.len() < 2 {
            return Err(format!("unterminated single-quoted IRI: {s:?}"));
        }
        let iri = &s[1..s.len() - 1];
        return Ok(QTerm::Const(format!("<{}>", iri)));
    }

    // Double-quoted literal: `"foo"` — canonicalize as n3 string literal.
    if s.starts_with('"') {
        if !s.ends_with('"') || s.len() < 2 {
            return Err(format!("unterminated double-quoted literal: {s:?}"));
        }
        // Keep it verbatim in canonical n3 form.
        return Ok(QTerm::Const(s.to_owned()));
    }

    // Prefixed name: `ex:alice`.
    if let Some(iri) = resolve_iri(s, prefixes) {
        return Ok(QTerm::Const(iri));
    }

    Err(format!(
        "cannot parse term {s:?} (not a variable, single-quoted IRI, or prefixed name)"
    ))
}

// ── IRI resolution helpers ────────────────────────────────────────────────────

/// Resolve a predicate/constant string to a canonical `<iri>` form.
///
/// Returns `Some("<iri>")` on success, `None` if the input cannot be resolved
/// (e.g. an unknown prefix).
fn resolve_iri(s: &str, prefixes: &BTreeMap<String, String>) -> Option<String> {
    let s = s.trim();

    // Already angle-bracketed: `<https://...>` — pass through.
    if s.starts_with('<') && s.ends_with('>') {
        return Some(s.to_owned());
    }

    // Single-quoted IRI.
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        let iri = &s[1..s.len() - 1];
        return Some(format!("<{}>", iri));
    }

    // Prefixed name: `alias:local`.
    if let Some(colon) = s.find(':') {
        let alias = &s[..colon];
        let local = &s[colon + 1..];
        if let Some(base) = prefixes.get(alias) {
            return Some(format!("<{}{}>", base, local));
        }
    }

    None
}

/// Strip angle brackets from `<iri>` → `iri`.
fn strip_angle_brackets(s: &str) -> String {
    if s.starts_with('<') && s.ends_with('>') {
        s[1..s.len() - 1].to_owned()
    } else {
        s.to_owned()
    }
}

// ── Utility impls ─────────────────────────────────────────────────────────────

impl QBodyLit {
    /// Extract the inner `QAtom` if this is a `QBodyLit::Atom`, else `None`.
    pub fn into_atom(self) -> Option<QAtom> {
        match self {
            QBodyLit::Atom(a) => Some(a),
            QBodyLit::Cut => None,
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Variable vs constant classification ───────────────────────────────────

    #[test]
    fn variable_uppercase_first() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, ex:a).\n\
             ?- ex:p(X, Y).\n",
        )
        .unwrap();
        let fact = &prog.rules[0];
        assert_eq!(fact.head.args[0], QTerm::Var("X".to_owned()));
        assert_eq!(
            fact.head.args[1],
            QTerm::Const("<https://example.org/a>".to_owned())
        );
    }

    #[test]
    fn variable_underscore_first() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(_Z, ex:b).\n\
             ?- ex:p(_Z, ex:b).\n",
        )
        .unwrap();
        assert_eq!(prog.rules[0].head.args[0], QTerm::Var("_Z".to_owned()));
    }

    // ── Prefix expansion correctness ──────────────────────────────────────────

    #[test]
    fn prefix_expansion_is_correct() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/profiles/positive-horn/').\n\
             ex:parentOf(ex:alice, ex:bob).\n\
             ?- ex:parentOf(ex:alice, Y).\n",
        )
        .unwrap();
        let fact = &prog.rules[0];
        assert_eq!(
            fact.head.pred,
            "https://example.org/profiles/positive-horn/parentOf"
        );
        assert_eq!(
            fact.head.args[0],
            QTerm::Const("<https://example.org/profiles/positive-horn/alice>".to_owned())
        );
        assert_eq!(
            fact.head.args[1],
            QTerm::Const("<https://example.org/profiles/positive-horn/bob>".to_owned())
        );
    }

    // ── Prefix + 2 rules + goal parse ─────────────────────────────────────────

    #[test]
    fn parse_prefix_two_rules_and_goal() {
        let src = "\
:- prefix(ex, 'https://example.org/').\
\n\
ex:parentOf(ex:alice, ex:bob).\
\n\
ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\
\n\
ex:ancestorOf(X, Y) :- ex:parentOf(X, Z), ex:ancestorOf(Z, Y).\
\n\
?- ex:ancestorOf(ex:alice, Y).\
";
        let prog = parse_query_program(src).unwrap();
        assert_eq!(prog.rules.len(), 3, "1 fact + 2 rules");
        assert_eq!(prog.goal.atoms.len(), 1);
        assert_eq!(prog.goal.atoms[0].pred, "https://example.org/ancestorOf");
    }

    // ── Fact parse ────────────────────────────────────────────────────────────

    #[test]
    fn parse_fact_no_body() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b).\n\
             ?- ex:p(ex:a, ex:b).\n",
        )
        .unwrap();
        assert_eq!(prog.rules.len(), 1);
        let fact = &prog.rules[0];
        assert!(fact.body.is_empty(), "fact must have empty body");
        assert_eq!(fact.head.pred, "https://example.org/p");
    }

    // ── Cut in body ───────────────────────────────────────────────────────────

    #[test]
    fn parse_cut_in_body() {
        let prog = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- ex:q(X, Y), !, ex:r(X, Y).\n\
             ?- ex:p(X, Y).\n",
        )
        .unwrap();
        let rule = &prog.rules[0];
        assert_eq!(rule.body.len(), 3);
        assert_eq!(
            rule.body[0],
            QBodyLit::Atom(rule.body[0].clone().into_atom().unwrap())
        );
        assert_eq!(rule.body[1], QBodyLit::Cut);
        assert_eq!(
            rule.body[2],
            QBodyLit::Atom(rule.body[2].clone().into_atom().unwrap())
        );
    }

    // ── Reject: no goal ───────────────────────────────────────────────────────

    #[test]
    fn reject_program_with_no_goal() {
        let result = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b).\n",
        );
        assert!(result.is_err(), "must reject program with no goal");
        assert!(result.unwrap_err().contains("no ?- goal"));
    }

    // ── Reject: malformed clause ──────────────────────────────────────────────

    #[test]
    fn reject_malformed_atom_missing_parens() {
        let result = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p ex:a ex:b.\n\
             ?- ex:p(ex:a, ex:b).\n",
        );
        assert!(result.is_err(), "must reject atom missing parentheses");
    }

    #[test]
    fn reject_atom_wrong_arity() {
        let result = parse_query_program(
            ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b, ex:c).\n\
             ?- ex:p(ex:a, ex:b).\n",
        );
        assert!(result.is_err(), "must reject atom with arity != 2");
    }

    // ── Answer-set canonicalization ───────────────────────────────────────────

    #[test]
    fn answer_set_canonicalize_sorts_bindings() {
        let mut b1 = BTreeMap::new();
        b1.insert("Y".to_owned(), "<https://example.org/c>".to_owned());
        let mut b2 = BTreeMap::new();
        b2.insert("Y".to_owned(), "<https://example.org/a>".to_owned());
        let mut b3 = BTreeMap::new();
        b3.insert("Y".to_owned(), "<https://example.org/b>".to_owned());

        let mut ans = AnswerSet {
            bindings: vec![b1.clone(), b3.clone(), b2.clone()],
            status: BudgetStatus::Ok,
        };
        ans.canonicalize();
        assert_eq!(ans.bindings[0]["Y"], "<https://example.org/a>");
        assert_eq!(ans.bindings[1]["Y"], "<https://example.org/b>");
        assert_eq!(ans.bindings[2]["Y"], "<https://example.org/c>");
    }
}
