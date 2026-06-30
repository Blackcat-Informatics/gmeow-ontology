// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The CLIF **reader**: CLIF text → [`LogicProgram`] + diagnostics.
//!
//! The inverse of [`project_clif`](super::writer::project_clif). See [`crate::clif`] for the
//! architecture. The **lossless round-trip carrier** is the RDF/predication channel after the
//! sentinel: it is reconstructed into an N-Triples dataset and lifted through the canonical
//! RDF frontend ([`parse_logic_dataset`](crate::frontend::parse_logic_dataset)), so the
//! reconstructed IR — axioms, rules, formulas, contracts, correspondences — is exactly the
//! Exact `canonical-rdf12` round-trip's. The idiomatic FOL sentences before the sentinel are
//! a human-readable VIEW (an `obj_is_literal` rule-term bit and minted reifier-node identity
//! cannot be expressed in idiomatic CL syntax); the bespoke recursive-descent parser still
//! VALIDATES them (a malformed FOL sentence raises a `CLIF_MALFORMED_SENTENCE` diagnostic),
//! but the IR is never reconstructed from them.

use crate::frontend::{parse_logic_dataset, Diagnostic, LogicParseError, Severity};
use crate::ir::{ContextualScope, Formula, LogicAxiom, LogicProgram, Term};

use super::{parse_sexprs, split_on_sentinel, Atom, SExpr};

/// A parsed rule body: the antecedent atoms (positive + negated) and the `(/=)` distinct
/// variable pairs.
type RuleBody = (Vec<LogicAxiom>, Vec<(String, String)>);

use gmeow_rdf::parse_dataset;

/// Parse CLIF source text into a [`LogicProgram`] + diagnostics.
///
/// Fail-soft: a malformed FOL view sentence is recorded as a `CLIF_MALFORMED_SENTENCE`
/// warning (and does not affect the reconstructed IR); a truly unparsable document
/// (unbalanced parens / empty) raises [`LogicParseError`]. A construct is never silently
/// dropped.
pub fn parse_clif_str(
    clif: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if clif.trim().is_empty() {
        return Err(LogicParseError(
            "CLIF source is empty — nothing to parse.".to_owned(),
        ));
    }

    let (fol_src, meta_src) = split_on_sentinel(clif);

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // ── RDF / predication channel (the round-trip authority) ────────────────────
    // Reconstruct the WHOLE program from the predications after the sentinel, by lifting
    // them through the canonical RDF frontend (reuse, not reinvention).
    let (program, meta_diags) = parse_meta_block(&meta_src, source_iri.clone())?;
    diagnostics.extend(meta_diags);

    // ── FOL channel (human-readable view) — VALIDATE ONLY ───────────────────────
    // The idiomatic sentences are cross-checked for well-formedness (so corruption surfaces
    // as a diagnostic), but the IR above is the authority — the FOL parse never feeds it.
    let forms = parse_sexprs(&fol_src).map_err(LogicParseError)?;
    for form in &forms {
        if let Err(msg) = validate_fol_form(form) {
            diagnostics.push(Diagnostic {
                severity: Severity::Warning,
                code: "CLIF_MALFORMED_SENTENCE".to_owned(),
                message: msg,
                subject: None,
            });
        }
    }

    Ok((program, diagnostics))
}

// --------------------------------------------------------------------------- //
// RDF / predication channel
// --------------------------------------------------------------------------- //

/// Parse the RDF-meta block (the text after the sentinel) into a [`LogicProgram`]
/// (axioms / contracts / path shapes / correspondences / transaction programs). Each
/// `(P S O)` predication becomes one N-Triples line; the assembled document is lifted by
/// the canonical RDF frontend.
fn parse_meta_block(
    meta_src: &str,
    source_iri: Option<String>,
) -> Result<(LogicProgram, Vec<Diagnostic>), LogicParseError> {
    if meta_src.trim().is_empty() {
        // No meta channel: an empty program (the FOL channel carries everything).
        return Ok((
            LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), source_iri),
            Vec::new(),
        ));
    }

    let forms = parse_sexprs(meta_src).map_err(LogicParseError)?;
    let mut nt_lines: Vec<String> = Vec::new();
    for form in &forms {
        let SExpr::List(items) = form else {
            return Err(LogicParseError(format!(
                "CLIF meta block: expected a (P S O) predication, found a bare atom: {form:?}"
            )));
        };
        if items.len() != 3 {
            return Err(LogicParseError(format!(
                "CLIF meta block: predication must have exactly 3 terms (P S O), found {}",
                items.len()
            )));
        }
        let p = nt_term(&items[0])?;
        let s = nt_term(&items[1])?;
        let o = nt_term(&items[2])?;
        nt_lines.push(format!("{s} {p} {o} ."));
    }

    if nt_lines.is_empty() {
        return Ok((
            LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), source_iri),
            Vec::new(),
        ));
    }

    let nt = nt_lines.join("\n");
    let ds = parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| LogicParseError(format!("CLIF meta block: N-Triples re-parse failed: {e}")))?;
    parse_logic_dataset(ds.as_ref(), source_iri)
}

/// Encode one CL meta-predication term as an N-Triples term (`<iri>`, `_:b`, or a literal).
fn nt_term(expr: &SExpr) -> Result<String, LogicParseError> {
    match expr {
        SExpr::Atom(Atom::Name(name)) => {
            if let Some(blank) = name.strip_prefix("_:") {
                Ok(format!("_:{blank}"))
            } else {
                Ok(format!("<{}>", nt_escape_iri(name)))
            }
        }
        SExpr::List(items) => {
            // A `(lit "lex")` / `(lit "lex" 'dt')` / `(lit "lex" @lang)` reserved form.
            let lit = parse_lit_form(items)?;
            Ok(lit.to_ntriples())
        }
        other => Err(LogicParseError(format!(
            "CLIF meta term must be a quoted name or a (lit …) form, found: {other:?}"
        ))),
    }
}

/// A parsed `(lit …)` form.
struct LitTerm {
    lexical: String,
    datatype: Option<String>,
    language: Option<String>,
}

impl LitTerm {
    fn to_ntriples(&self) -> String {
        let lex = nt_escape_literal(&self.lexical);
        match (&self.datatype, &self.language) {
            (_, Some(lang)) => format!("\"{lex}\"@{lang}"),
            (Some(dt), None) => format!("\"{lex}\"^^<{}>", nt_escape_iri(dt)),
            (None, None) => format!("\"{lex}\""),
        }
    }
}

/// Parse a `(lit "lex")` / `(lit "lex" 'dt')` / `(lit "lex" @lang)` form.
fn parse_lit_form(items: &[SExpr]) -> Result<LitTerm, LogicParseError> {
    let head = items.first().and_then(symbol_of);
    if head.as_deref() != Some("lit") {
        return Err(LogicParseError(format!(
            "expected a (lit …) form, found head {head:?}"
        )));
    }
    let lexical = match items.get(1) {
        Some(SExpr::Atom(Atom::Str(s))) => s.clone(),
        other => {
            return Err(LogicParseError(format!(
                "(lit …) first argument must be a \"string\", found {other:?}"
            )))
        }
    };
    match items.get(2) {
        None => Ok(LitTerm {
            lexical,
            datatype: None,
            language: None,
        }),
        Some(SExpr::Atom(Atom::Name(dt))) => Ok(LitTerm {
            lexical,
            datatype: Some(dt.clone()),
            language: None,
        }),
        Some(SExpr::Atom(Atom::Lang(lang))) => Ok(LitTerm {
            lexical,
            datatype: None,
            language: Some(lang.clone()),
        }),
        other => Err(LogicParseError(format!(
            "(lit …) third argument must be a 'datatype' or @lang, found {other:?}"
        ))),
    }
}

/// Escape an IRI for an N-Triples `<…>`. The N-Triples `IRIREF` grammar (W3C RDF 1.1
/// §grammar) forbids `<`, `>`, `"`, `{`, `}`, `|`, `^`, `` ` ``, `\`, and every code point
/// `<= 0x20` (space + C0 controls) appearing raw; each must ride as a `\uXXXX` `UCHAR`.
/// Emitting any of them raw would make the reader's reconstructed N-Triples re-parse
/// hard-fail — turning a fail-soft round-trip into a panic — so escape them all.
fn nt_escape_iri(iri: &str) -> String {
    let mut out = String::with_capacity(iri.len());
    for c in iri.chars() {
        match c {
            '\\' => out.push_str("\\u005C"),
            '"' => out.push_str("\\u0022"),
            '<' => out.push_str("\\u003C"),
            '>' => out.push_str("\\u003E"),
            '{' => out.push_str("\\u007B"),
            '}' => out.push_str("\\u007D"),
            '|' => out.push_str("\\u007C"),
            '^' => out.push_str("\\u005E"),
            '`' => out.push_str("\\u0060"),
            c if c <= ' ' => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Escape a literal lexical form for an N-Triples `"…"` string.
fn nt_escape_literal(lex: &str) -> String {
    let mut out = String::with_capacity(lex.len());
    for c in lex.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

// --------------------------------------------------------------------------- //
// FOL channel
// --------------------------------------------------------------------------- //

/// Validate that a top-level FOL view sentence is well-formed (a recognizable rule shape, or
/// a parseable full-FOL formula). `Ok(())` = well-formed; `Err(msg)` = a
/// `CLIF_MALFORMED_SENTENCE` diagnostic. The bespoke rule / formula recursive-descent below
/// still runs (and so is exercised), but the reconstructed IR is discarded — the RDF channel
/// is the round-trip authority (see [`parse_clif_str`]).
fn validate_fol_form(form: &SExpr) -> Result<(), String> {
    let SExpr::List(items) = form else {
        return Err(format!("top-level FOL form must be a list, found {form:?}"));
    };
    // A recognizable, well-formed rule shape is fine.
    if validate_rule(items)? {
        return Ok(());
    }
    // Otherwise it must parse as a full-FOL formula.
    parse_formula(form).map(|_| ())
}

/// Recognize and validate the rule shape `(forall (vars) (if BODY HEAD))` (or a degenerate
/// `(forall (vars) HEAD)` fact). Returns `Ok(true)` when the form IS the rule shape and is
/// well-formed, `Ok(false)` when it is not the rule shape (so the caller falls through to the
/// formula parser), and `Err(msg)` when it is the rule shape but malformed.
fn validate_rule(items: &[SExpr]) -> Result<bool, String> {
    let Some(head_sym) = items.first().and_then(symbol_of) else {
        return Ok(false);
    };
    if head_sym != "forall" || items.len() != 3 {
        return Ok(false);
    }
    if !matches!(items.get(1), Some(SExpr::List(_))) {
        return Ok(false);
    }
    let inner = &items[2];
    let SExpr::List(inner_items) = inner else {
        return Ok(false);
    };
    match inner_items.first().and_then(symbol_of).as_deref() {
        Some("if") if inner_items.len() == 3 => {
            parse_horn_atom(&inner_items[2])?; // head
            parse_rule_body(&inner_items[1])?; // body + distinct pairs
            Ok(true)
        }
        // A `(forall (vars) HEAD)` fact (degenerate, empty body).
        _ => {
            parse_horn_atom(inner)?;
            Ok(true)
        }
    }
}

/// Parse a rule body expression into (positive/negated atoms, distinct pairs). The body is
/// either a single atom / `(not atom)` / `(/= ?a ?b)`, or `(and …)` of those.
fn parse_rule_body(expr: &SExpr) -> Result<RuleBody, String> {
    let mut body: Vec<LogicAxiom> = Vec::new();
    let mut distinct: Vec<(String, String)> = Vec::new();

    let conjuncts: Vec<&SExpr> = match expr {
        SExpr::List(items)
            if symbol_of(items.first().unwrap_or(&SExpr::List(vec![]))).as_deref()
                == Some("and") =>
        {
            items[1..].iter().collect()
        }
        other => vec![other],
    };

    for c in conjuncts {
        let SExpr::List(items) = c else {
            return Err(format!("rule body conjunct must be a list, found {c:?}"));
        };
        match symbol_of(items.first().ok_or("empty conjunct")?).as_deref() {
            Some("not") => {
                if items.len() != 2 {
                    return Err("(not …) must have exactly one argument".to_owned());
                }
                let mut atom = parse_horn_atom(&items[1])?;
                atom.negated = true;
                body.push(atom);
            }
            Some("/=") => {
                if items.len() != 3 {
                    return Err("(/= …) must have exactly two arguments".to_owned());
                }
                let a = var_string(&items[1])?;
                let b = var_string(&items[2])?;
                distinct.push((a, b));
            }
            _ => body.push(parse_horn_atom(c)?),
        }
    }
    Ok((body, distinct))
}

/// Parse a Horn atom `(<pred> <subj> <obj>)` into a [`LogicAxiom`].
fn parse_horn_atom(expr: &SExpr) -> Result<LogicAxiom, String> {
    let SExpr::List(items) = expr else {
        return Err(format!("Horn atom must be a list, found {expr:?}"));
    };
    if items.len() != 3 {
        return Err(format!(
            "Horn atom must have exactly 3 terms (pred subj obj), found {}",
            items.len()
        ));
    }
    let predicate = name_string(&items[0])?;
    let (subject, _) = horn_operand(&items[1])?;
    let (obj, obj_is_literal) = horn_operand(&items[2])?;
    LogicAxiom::new(
        subject,
        predicate,
        obj,
        obj_is_literal,
        false,
        ContextualScope::default(),
    )
}

/// Decode a Horn subject/object operand: a `?var` → `?name` string (non-literal); a
/// `(lit "x")` → plain literal string (literal); a `'iri'` → IRI string (non-literal).
/// Returns `(value, is_literal)`.
fn horn_operand(expr: &SExpr) -> Result<(String, bool), String> {
    match expr {
        SExpr::Atom(Atom::Var(n)) => Ok((format!("?{n}"), false)),
        SExpr::Atom(Atom::Name(iri)) => Ok((iri.clone(), false)),
        SExpr::List(items) => {
            let lit = parse_lit_simple(items)?;
            Ok((lit, true))
        }
        other => Err(format!(
            "Horn operand must be ?var, 'iri', or (lit …), found {other:?}"
        )),
    }
}

/// Parse a `(lit "x")` form's lexical value (the Horn fragment carries no datatype on the
/// object; the RDF channel carries that detail). Rejects a typed/lang `(lit …)`.
fn parse_lit_simple(items: &[SExpr]) -> Result<String, String> {
    if symbol_of(items.first().ok_or("empty list")?).as_deref() != Some("lit") {
        return Err("expected a (lit …) form".to_owned());
    }
    match items.get(1) {
        Some(SExpr::Atom(Atom::Str(s))) => Ok(s.clone()),
        other => Err(format!(
            "(lit …) argument must be a \"string\", found {other:?}"
        )),
    }
}

// --------------------------------------------------------------------------- //
// Full-FOL formula parsing
// --------------------------------------------------------------------------- //

/// Parse a full-FOL [`Formula`] from a CL sentence.
fn parse_formula(expr: &SExpr) -> Result<Formula, String> {
    let SExpr::List(items) = expr else {
        return Err(format!("formula must be a list, found {expr:?}"));
    };
    let head = symbol_of(items.first().ok_or("empty formula list")?);
    match head.as_deref() {
        Some("not") => {
            if items.len() != 2 {
                return Err("(not …) takes one argument".to_owned());
            }
            Ok(Formula::Not(Box::new(parse_formula(&items[1])?)))
        }
        Some("and") => {
            let subs = parse_formula_list(&items[1..])?;
            Ok(Formula::And(subs))
        }
        Some("or") => {
            let subs = parse_formula_list(&items[1..])?;
            Ok(Formula::Or(subs))
        }
        Some("if") => {
            if items.len() != 3 {
                return Err("(if …) takes two arguments".to_owned());
            }
            Ok(Formula::Implies(
                Box::new(parse_formula(&items[1])?),
                Box::new(parse_formula(&items[2])?),
            ))
        }
        Some("iff") => {
            if items.len() != 3 {
                return Err("(iff …) takes two arguments".to_owned());
            }
            Ok(Formula::Iff(
                Box::new(parse_formula(&items[1])?),
                Box::new(parse_formula(&items[2])?),
            ))
        }
        Some("forall") | Some("exists") => {
            if items.len() != 3 {
                return Err("(forall/exists …) takes a variable block and a body".to_owned());
            }
            let vars = parse_var_block(&items[1])?;
            let body = Box::new(parse_formula(&items[2])?);
            if head.as_deref() == Some("forall") {
                Ok(Formula::Forall { vars, body })
            } else {
                Ok(Formula::Exists { vars, body })
            }
        }
        // Any other head is an atomic predication `(<relation> <arg>…)`.
        _ => parse_atom_formula(items),
    }
}

/// Parse a list of sub-formulas (for `(and …)` / `(or …)`).
fn parse_formula_list(items: &[SExpr]) -> Result<Vec<Formula>, String> {
    items.iter().map(parse_formula).collect()
}

/// Parse an atomic predication `(<relation> <arg>…)` into a [`Formula::Atom`].
fn parse_atom_formula(items: &[SExpr]) -> Result<Formula, String> {
    let relation = Term::iri(name_string(&items[0])?)?;
    let mut args = Vec::new();
    for a in &items[1..] {
        args.push(parse_formula_term(a)?);
    }
    Formula::atom(relation, args)
}

/// Parse a [`Term`] argument of an atomic formula.
fn parse_formula_term(expr: &SExpr) -> Result<Term, String> {
    match expr {
        SExpr::Atom(Atom::Var(n)) => Term::var(n.clone()),
        SExpr::Atom(Atom::Name(iri)) => Term::iri(iri.clone()),
        SExpr::List(items) => {
            let head = symbol_of(items.first().ok_or("empty term list")?);
            match head.as_deref() {
                Some("lit") => {
                    let lit = parse_lit_form_term(items)?;
                    Ok(lit)
                }
                Some("seq") => {
                    // `(seq "name")` — a sequence marker.
                    let name = match items.get(1) {
                        Some(SExpr::Atom(Atom::Str(s))) => s.clone(),
                        other => {
                            return Err(format!(
                                "(seq …) argument must be a \"string\", found {other:?}"
                            ))
                        }
                    };
                    Term::sequence_marker(name)
                }
                other => Err(format!("unrecognized term form head {other:?}")),
            }
        }
        other => Err(format!(
            "formula term must be ?var, 'iri', (lit …), or (seq …), found {other:?}"
        )),
    }
}

/// Parse a `(lit "x")` / `(lit "x" 'dt')` form into a [`Term::Literal`] (the formula channel
/// carries the datatype; a `@lang` is not used in the formula term position).
fn parse_lit_form_term(items: &[SExpr]) -> Result<Term, String> {
    let lit = parse_lit_form(items).map_err(|e| e.0)?;
    if lit.language.is_some() {
        return Err(
            "a (lit … @lang) language-tagged literal is not a valid formula term".to_owned(),
        );
    }
    Term::literal(lit.lexical, lit.datatype)
}

/// Parse a `(?v1 ?v2 …)` variable block into authored (sigil-free) names.
fn parse_var_block(expr: &SExpr) -> Result<Vec<String>, String> {
    let SExpr::List(items) = expr else {
        return Err(format!("variable block must be a list, found {expr:?}"));
    };
    items.iter().map(var_name).collect()
}

/// Extract the authored (sigil-free) name from a `?var` atom.
fn var_name(expr: &SExpr) -> Result<String, String> {
    match expr {
        SExpr::Atom(Atom::Var(n)) => Ok(n.clone()),
        other => Err(format!("expected a ?variable, found {other:?}")),
    }
}

// --------------------------------------------------------------------------- //
// Small atom helpers
// --------------------------------------------------------------------------- //

/// The symbol string of an [`SExpr::Atom`] if it is a bare [`Atom::Symbol`].
fn symbol_of(expr: &SExpr) -> Option<String> {
    match expr {
        SExpr::Atom(Atom::Symbol(s)) => Some(s.clone()),
        _ => None,
    }
}

/// The IRI string of a `'name'` atom.
fn name_string(expr: &SExpr) -> Result<String, String> {
    match expr {
        SExpr::Atom(Atom::Name(s)) => Ok(s.clone()),
        other => Err(format!("expected a 'quoted name', found {other:?}")),
    }
}

/// The `?name`-shaped variable string of a `?var` atom (for a `/=` operand).
fn var_string(expr: &SExpr) -> Result<String, String> {
    match expr {
        SExpr::Atom(Atom::Var(n)) => Ok(format!("?{n}")),
        other => Err(format!("expected a ?variable, found {other:?}")),
    }
}
