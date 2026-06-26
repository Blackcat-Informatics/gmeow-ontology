// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! SPARQL expression evaluation (FILTER / BIND / EXISTS), plus the `Filter` and
//! `Extend` graph-pattern nodes that drive it.
//!
//! [`eval_expr`] maps an [`Expression`] over one solution to
//! `Ok(Some(term))` (a value), `Ok(None)` (a SPARQL **error / unbound** — the
//! third truth value), or `Err` (a hard [`EvalError::Unsupported`] for a construct
//! outside the current S6 scope). The `Ok(None)` vs `Err` split is load-bearing: a
//! type error is normal three-valued logic (it makes a FILTER drop the row), while
//! an unimplemented builtin is a hard failure (never a wrong answer).
//!
//! ## Scope (S6)
//!
//! Implemented: logical `&&`/`||`/`!` (Kleene three-valued), comparisons and
//! `sameTerm`, `BOUND`, `IN`, `IF`, `COALESCE`, `EXISTS`, the string/type/RDF
//! built-ins the corpus uses, **numeric arithmetic** (`+ - * /`, unary sign) and
//! **`ABS`/`CEIL`/`FLOOR`/`ROUND`**. The date/hash/uuid/rand built-ins hard-error
//! (`Unsupported`) pending Gap 4 — comparisons over numbers do NOT need arithmetic
//! (they go through `gmeow_xsd::value_cmp`).

use std::cmp::Ordering;

use gmeow_rdf_core::{BlankScope, TermValue};
use gmeow_sparql_algebra::{Expression, Function, GraphPattern, Variable};
use gmeow_xsd::{
    effective_boolean_value, numeric_abs, numeric_add, numeric_ceil, numeric_div, numeric_floor,
    numeric_mul, numeric_round, numeric_sub, numeric_unary_minus, numeric_unary_plus, parse_by_iri,
    value_cmp, XsdValue,
};

use crate::error::EvalError;
use crate::eval::{eval, EvalCtx};
use crate::scratch::SolutionTerm;
use crate::solution::{SolutionSeq, VarSchema};

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

/// Evaluate an expression over a solution. See the [module docs](self) for the
/// `Ok(Some)` / `Ok(None)` / `Err` contract.
pub(crate) fn eval_expr(
    expr: &Expression,
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
) -> Result<Option<SolutionTerm>, EvalError> {
    match expr {
        // ---- atoms ---------------------------------------------------------
        Expression::NamedNode(n) => Ok(Some(intern(ctx, TermValue::Iri(n.as_str().to_owned())))),
        Expression::Literal(l) => Ok(Some(intern(ctx, crate::convert::literal_to_value(l)))),
        Expression::Variable(v) => Ok(lookup(v, row, schema)),
        Expression::Bound(v) => Ok(Some(bool_term(ctx, lookup(v, row, schema).is_some()))),

        // ---- logical (Kleene three-valued) --------------------------------
        Expression::Or(a, b) => {
            let va = ebv_of(a, row, schema, ctx)?;
            let vb = ebv_of(b, row, schema, ctx)?;
            let r = match (va, vb) {
                (Some(true), _) | (_, Some(true)) => Some(true),
                (Some(false), Some(false)) => Some(false),
                _ => None,
            };
            Ok(r.map(|b| bool_term(ctx, b)))
        }
        Expression::And(a, b) => {
            let va = ebv_of(a, row, schema, ctx)?;
            let vb = ebv_of(b, row, schema, ctx)?;
            let r = match (va, vb) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            };
            Ok(r.map(|b| bool_term(ctx, b)))
        }
        Expression::Not(a) => {
            let v = ebv_of(a, row, schema, ctx)?;
            Ok(v.map(|b| bool_term(ctx, !b)))
        }

        // ---- comparisons ---------------------------------------------------
        Expression::Equal(a, b) => compare(a, b, row, schema, ctx, |c| c == Ordering::Equal),
        Expression::Greater(a, b) => compare(a, b, row, schema, ctx, |c| c == Ordering::Greater),
        Expression::GreaterOrEqual(a, b) => {
            compare(a, b, row, schema, ctx, |c| c != Ordering::Less)
        }
        Expression::Less(a, b) => compare(a, b, row, schema, ctx, |c| c == Ordering::Less),
        Expression::LessOrEqual(a, b) => {
            compare(a, b, row, schema, ctx, |c| c != Ordering::Greater)
        }
        Expression::SameTerm(a, b) => {
            let ta = eval_expr(a, row, schema, ctx)?;
            let tb = eval_expr(b, row, schema, ctx)?;
            Ok(match (ta, tb) {
                (Some(x), Some(y)) => Some(bool_term(ctx, x == y)),
                _ => None,
            })
        }

        // ---- conditionals --------------------------------------------------
        Expression::If(c, t, e) => match ebv_of(c, row, schema, ctx)? {
            Some(true) => eval_expr(t, row, schema, ctx),
            Some(false) => eval_expr(e, row, schema, ctx),
            None => Ok(None),
        },
        Expression::Coalesce(items) => {
            for item in items {
                if let Some(term) = eval_expr(item, row, schema, ctx)? {
                    return Ok(Some(term));
                }
            }
            Ok(None)
        }
        Expression::In(needle, haystack) => eval_in(needle, haystack, row, schema, ctx),

        // ---- EXISTS --------------------------------------------------------
        Expression::Exists(pattern) => {
            let found = exists(pattern, row, schema, ctx)?;
            Ok(Some(bool_term(ctx, found)))
        }

        // ---- arithmetic ---------------------------------------------------
        // SPARQL three-valued contract: type errors (non-numeric operands,
        // overflow, divide-by-zero) → Ok(None), NOT Err. A hard EvalError would
        // propagate out of FILTER and break the query; Ok(None) just drops the row.
        Expression::Add(a, b) => binary_numeric(a, b, row, schema, ctx, numeric_add),
        Expression::Subtract(a, b) => binary_numeric(a, b, row, schema, ctx, numeric_sub),
        Expression::Multiply(a, b) => binary_numeric(a, b, row, schema, ctx, numeric_mul),
        Expression::Divide(a, b) => binary_numeric(a, b, row, schema, ctx, numeric_div),
        Expression::UnaryPlus(a) => unary_numeric(a, row, schema, ctx, numeric_unary_plus),
        Expression::UnaryMinus(a) => unary_numeric(a, row, schema, ctx, numeric_unary_minus),

        // ---- functions -----------------------------------------------------
        Expression::FunctionCall(function, args) => eval_function(function, args, row, schema, ctx),
    }
}

/// Evaluate `expr` and reduce it to an effective boolean value (`Ok(None)` =
/// error/unbound).
pub(crate) fn eval_ebv(
    expr: &Expression,
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
) -> Result<Option<bool>, EvalError> {
    ebv_of(expr, row, schema, ctx)
}

/// `Filter(expr, inner)`: keep solutions whose `expr` has effective boolean value
/// `true`; an error/unbound (or `false`) drops the row.
pub(crate) fn eval_filter(
    expr: &Expression,
    inner: &GraphPattern,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let seq = eval(inner, ctx)?;
    let schema = seq.schema.clone();
    let mut rows = Vec::new();
    for row in seq.rows {
        if eval_ebv(expr, &row, &schema, ctx)? == Some(true) {
            rows.push(row);
        }
    }
    Ok(SolutionSeq { schema, rows })
}

/// `Extend(inner, var, expr)` (BIND): add `var` bound to `expr`'s value for each
/// solution. An error/unbound value leaves `var` unbound (the row is NOT dropped).
pub(crate) fn eval_extend(
    inner: &GraphPattern,
    var: &Variable,
    expr: &Expression,
    ctx: &mut EvalCtx<'_>,
) -> Result<SolutionSeq, EvalError> {
    let seq = eval(inner, ctx)?;
    let mut schema = (*seq.schema).clone();
    let col = schema.push(var.clone());
    let width = schema.len();
    let schema = std::rc::Rc::new(schema);

    let mut rows = Vec::with_capacity(seq.rows.len());
    for mut row in seq.rows {
        row.resize(width, None);
        let value = eval_expr(expr, &row, &schema, ctx)?;
        row[col] = value;
        rows.push(row);
    }
    Ok(SolutionSeq { schema, rows })
}

// ---------------------------------------------------------------------------
// internals
// ---------------------------------------------------------------------------

/// Look up a variable's binding in a solution.
fn lookup(
    var: &Variable,
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
) -> Option<SolutionTerm> {
    schema.index_of(var).and_then(|c| row[c])
}

/// Intern a value to a solution term (promoting to an existing dataset id).
fn intern(ctx: &mut EvalCtx<'_>, value: TermValue) -> SolutionTerm {
    ctx.scratch.intern(ctx.dataset, value)
}

/// Materialize a solution term to an owned value.
fn value_of(ctx: &EvalCtx<'_>, term: SolutionTerm) -> TermValue {
    ctx.scratch.value_of(ctx.dataset, term)
}

/// Intern an `xsd:boolean` literal.
fn bool_term(ctx: &mut EvalCtx<'_>, b: bool) -> SolutionTerm {
    intern(ctx, typed(if b { "true" } else { "false" }, XSD_BOOLEAN))
}

/// Intern an `xsd:string` literal.
fn string_term(ctx: &mut EvalCtx<'_>, lexical: String) -> SolutionTerm {
    intern(ctx, typed(&lexical, XSD_STRING))
}

/// Intern an `xsd:integer` literal.
fn integer_term(ctx: &mut EvalCtx<'_>, value: i64) -> SolutionTerm {
    intern(ctx, typed(&value.to_string(), XSD_INTEGER))
}

/// Build a typed (no-language) literal value.
fn typed(lexical: &str, datatype: &str) -> TermValue {
    TermValue::Literal {
        lexical_form: lexical.to_owned(),
        datatype: datatype.to_owned(),
        language: None,
        direction: None,
    }
}

/// The XSD value of a term, if it is an XSD-typed literal; `None` otherwise
/// (non-literal, unknown datatype, or malformed lexical form).
fn xsd_of(value: &TermValue) -> Option<XsdValue> {
    if let TermValue::Literal {
        lexical_form,
        datatype,
        ..
    } = value
    {
        parse_by_iri(lexical_form, datatype).ok().flatten()
    } else {
        None
    }
}

/// The effective boolean value of an evaluated expression (`Ok(None)` = error).
fn ebv_of(
    expr: &Expression,
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
) -> Result<Option<bool>, EvalError> {
    match eval_expr(expr, row, schema, ctx)? {
        Some(term) => Ok(ebv_term(ctx, term)),
        None => Ok(None),
    }
}

/// The effective boolean value of a concrete term (`None` = type error).
fn ebv_term(ctx: &EvalCtx<'_>, term: SolutionTerm) -> Option<bool> {
    let value = value_of(ctx, term);
    match xsd_of(&value) {
        Some(xv) => effective_boolean_value(&xv),
        None => None,
    }
}

/// Evaluate a comparison: both operands to values, compare in the XSD value space,
/// and test the resulting [`Ordering`] with `keep`. `None` (error/unbound operand
/// or incomparable values) propagates.
fn compare(
    a: &Expression,
    b: &Expression,
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
    keep: impl Fn(Ordering) -> bool,
) -> Result<Option<SolutionTerm>, EvalError> {
    let ta = eval_expr(a, row, schema, ctx)?;
    let tb = eval_expr(b, row, schema, ctx)?;
    let (Some(ta), Some(tb)) = (ta, tb) else {
        return Ok(None);
    };
    // sameTerm short-circuit: identical terms are equal regardless of value space.
    if ta == tb {
        return Ok(Some(bool_term(ctx, keep(Ordering::Equal))));
    }
    let va = value_of(ctx, ta);
    let vb = value_of(ctx, tb);
    Ok(rdf_cmp(&va, &vb).map(|ord| bool_term(ctx, keep(ord))))
}

/// Compare two RDF terms in the SPARQL value space. `None` = a type error (the
/// values are not comparable — e.g. distinct IRIs under `<`, or two literals in
/// incomparable value spaces).
fn rdf_cmp(a: &TermValue, b: &TermValue) -> Option<Ordering> {
    match (xsd_of(a), xsd_of(b)) {
        (Some(ax), Some(bx)) => value_cmp(&ax, &bx),
        // Distinct non-value terms (IRIs/blanks): equality is decidable (handled by
        // the sameTerm short-circuit above for `=`), but ordering is a type error.
        _ => None,
    }
}

/// `expr IN (list)`: true if equal (value semantics) to any list entry; an error in
/// the list propagates only if no `true` is found (SPARQL §17.4.1.9).
fn eval_in(
    needle: &Expression,
    haystack: &[Expression],
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
) -> Result<Option<SolutionTerm>, EvalError> {
    let Some(target) = eval_expr(needle, row, schema, ctx)? else {
        return Ok(None);
    };
    let tv = value_of(ctx, target);
    let mut saw_error = false;
    for item in haystack {
        match eval_expr(item, row, schema, ctx)? {
            Some(candidate) => {
                if target == candidate {
                    return Ok(Some(bool_term(ctx, true)));
                }
                let cv = value_of(ctx, candidate);
                match rdf_equal(&tv, &cv) {
                    Some(true) => return Ok(Some(bool_term(ctx, true))),
                    Some(false) => {}
                    None => saw_error = true,
                }
            }
            None => saw_error = true,
        }
    }
    if saw_error {
        Ok(None)
    } else {
        Ok(Some(bool_term(ctx, false)))
    }
}

/// RDF term value-equality (`=`). `None` = type error (two literals not comparable).
fn rdf_equal(a: &TermValue, b: &TermValue) -> Option<bool> {
    match (xsd_of(a), xsd_of(b)) {
        (Some(ax), Some(bx)) => value_cmp(&ax, &bx).map(|o| o == Ordering::Equal),
        _ => {
            if a == b {
                Some(true)
            } else if is_literal(a) && is_literal(b) {
                // Two different literals neither side could value-compare.
                None
            } else {
                // Distinct terms of (at least one) non-literal kind: known unequal.
                Some(false)
            }
        }
    }
}

fn is_literal(v: &TermValue) -> bool {
    matches!(v, TermValue::Literal { .. })
}

/// Evaluate `EXISTS { pattern }` for the current solution: bind the pattern's free
/// variables from `row`, evaluate, and report whether any solution results.
fn exists(
    pattern: &GraphPattern,
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
) -> Result<bool, EvalError> {
    // Substitute the outer row's bindings, then AND the substitution into the
    // pattern by joining with a one-row VALUES-like seed. We realize the seed as a
    // BGP-independent solution and Join it via the standard hash join.
    let seed = seed_from_row(row, schema);
    let inner = eval(pattern, ctx)?;
    let joined = crate::binop::join_seqs(&seed, &inner);
    Ok(!joined.is_empty())
}

/// A one-solution sequence carrying the current row's *bound* variables — the seed
/// that constrains an `EXISTS`/`NOT EXISTS` subpattern to the outer bindings.
fn seed_from_row(row: &[Option<SolutionTerm>], schema: &VarSchema) -> SolutionSeq {
    let mut seed_schema = VarSchema::new();
    let mut values = Vec::new();
    for (i, var) in schema.vars().iter().enumerate() {
        if let Some(term) = row[i] {
            seed_schema.push(var.clone());
            values.push(Some(term));
        }
    }
    SolutionSeq {
        schema: std::rc::Rc::new(seed_schema),
        rows: vec![values],
    }
}

/// Dispatch a built-in (or custom) function call.
fn eval_function(
    function: &Function,
    args: &[Expression],
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
) -> Result<Option<SolutionTerm>, EvalError> {
    // Evaluate all arguments first (a missing/unbound argument is a per-function
    // concern handled below; most functions are strict and error on it).
    let mut vals: Vec<Option<TermValue>> = Vec::with_capacity(args.len());
    for a in args {
        vals.push(eval_expr(a, row, schema, ctx)?.map(|t| value_of(ctx, t)));
    }

    match function {
        // ---- type tests (total: never a type error) -----------------------
        Function::IsIri | Function::IsUri => Ok(Some(bool_term(
            ctx,
            matches!(vals.first(), Some(Some(TermValue::Iri(_)))),
        ))),
        Function::IsBlank => Ok(Some(bool_term(
            ctx,
            matches!(vals.first(), Some(Some(TermValue::Blank { .. }))),
        ))),
        Function::IsLiteral => Ok(Some(bool_term(
            ctx,
            matches!(vals.first(), Some(Some(TermValue::Literal { .. }))),
        ))),
        Function::IsNumeric => {
            let numeric = matches!(arg(&vals, 0), Some(v) if xsd_of(v).is_some_and(is_numeric));
            Ok(Some(bool_term(ctx, numeric)))
        }
        Function::IsTriple => Ok(Some(bool_term(
            ctx,
            matches!(vals.first(), Some(Some(TermValue::Triple { .. }))),
        ))),

        // ---- term accessors ------------------------------------------------
        Function::Str => match arg(&vals, 0) {
            Some(TermValue::Literal { lexical_form, .. }) => {
                Ok(Some(string_term(ctx, lexical_form.clone())))
            }
            Some(TermValue::Iri(iri)) => Ok(Some(string_term(ctx, iri.clone()))),
            _ => Ok(None),
        },
        Function::Lang => match arg(&vals, 0) {
            Some(TermValue::Literal { language, .. }) => {
                Ok(Some(string_term(ctx, language.clone().unwrap_or_default())))
            }
            _ => Ok(None),
        },
        Function::Datatype => match arg(&vals, 0) {
            Some(TermValue::Literal { datatype, .. }) => {
                Ok(Some(intern(ctx, TermValue::Iri(datatype.clone()))))
            }
            _ => Ok(None),
        },

        // ---- string functions ---------------------------------------------
        Function::StrLen => match string_arg(&vals, 0) {
            Some((s, _)) => Ok(Some(integer_term(ctx, s.chars().count() as i64))),
            None => Ok(None),
        },
        Function::UCase => map_string(ctx, &vals, |s| s.to_uppercase()),
        Function::LCase => map_string(ctx, &vals, |s| s.to_lowercase()),
        Function::Contains => string_pred(ctx, &vals, |h, n| h.contains(n)),
        Function::StrStarts => string_pred(ctx, &vals, |h, n| h.starts_with(n)),
        Function::StrEnds => string_pred(ctx, &vals, |h, n| h.ends_with(n)),
        Function::Concat => eval_concat(ctx, &vals),
        Function::SubStr => eval_substr(ctx, &vals),
        Function::StrBefore => eval_str_before_after(ctx, &vals, true),
        Function::StrAfter => eval_str_before_after(ctx, &vals, false),
        Function::Replace => eval_replace(ctx, &vals),
        Function::Regex => eval_regex(ctx, &vals),
        Function::LangMatches => eval_lang_matches(ctx, &vals),

        // ---- term constructors --------------------------------------------
        Function::Iri | Function::Uri => match arg(&vals, 0) {
            Some(TermValue::Iri(iri)) => Ok(Some(intern(ctx, TermValue::Iri(iri.clone())))),
            Some(TermValue::Literal { lexical_form, .. }) => {
                Ok(Some(intern(ctx, TermValue::Iri(lexical_form.clone()))))
            }
            _ => Ok(None),
        },
        Function::StrLang => eval_str_lang(ctx, &vals),
        Function::StrDt => eval_str_dt(ctx, &vals),
        Function::BNode => {
            // BNODE() / BNODE(str): mint a fresh blank node per call.
            ctx.bnode_counter += 1;
            let label = format!("bnode{}", ctx.bnode_counter);
            Ok(Some(intern(
                ctx,
                TermValue::Blank {
                    label,
                    scope: BlankScope::DEFAULT,
                },
            )))
        }

        // ---- RDF 1.2 triple-term functions --------------------------------
        Function::Triple => eval_triple_ctor(ctx, &vals),
        Function::Subject => triple_part(ctx, &vals, |s, _, _| s),
        Function::Predicate => triple_part(ctx, &vals, |_, p, _| p),
        Function::Object => triple_part(ctx, &vals, |_, _, o| o),

        // ---- numeric math functions (ABS/CEIL/FLOOR/ROUND) ----------------
        // All four are strict in one numeric argument; type errors → Ok(None).
        Function::Abs => unary_numeric_fn(ctx, &vals, numeric_abs),
        Function::Ceil => unary_numeric_fn(ctx, &vals, numeric_ceil),
        Function::Floor => unary_numeric_fn(ctx, &vals, numeric_floor),
        Function::Round => unary_numeric_fn(ctx, &vals, numeric_round),

        // ---- out of S6 scope: hard error (never a wrong answer) -----------
        Function::Rand
        | Function::Year
        | Function::Month
        | Function::Day
        | Function::Hours
        | Function::Minutes
        | Function::Seconds
        | Function::Timezone
        | Function::Tz
        | Function::Now
        | Function::Uuid
        | Function::StrUuid
        | Function::Md5
        | Function::Sha1
        | Function::Sha256
        | Function::Sha384
        | Function::Sha512
        | Function::EncodeForUri => Err(EvalError::unsupported(format!(
            "SPARQL built-in {function:?} (not yet implemented in sparql-eval)"
        ))),
        Function::Custom(iri) => Err(EvalError::unsupported(format!(
            "custom SPARQL function <{}>",
            iri.as_str()
        ))),
    }
}

/// The value at argument index `i`, if it was bound (not unbound/error).
fn arg(vals: &[Option<TermValue>], i: usize) -> Option<&TermValue> {
    vals.get(i).and_then(|v| v.as_ref())
}

/// Whether an XSD value is in the numeric tower.
fn is_numeric(v: XsdValue) -> bool {
    matches!(
        v,
        XsdValue::Integer { .. } | XsdValue::Decimal(_) | XsdValue::Float(_) | XsdValue::Double(_)
    )
}

/// Extract `(lexical, language)` from a plain/`xsd:string`/`rdf:langString` literal
/// argument. `None` for any other term (a string-function type error).
fn string_arg(vals: &[Option<TermValue>], i: usize) -> Option<(String, Option<String>)> {
    match arg(vals, i)? {
        TermValue::Literal {
            lexical_form,
            datatype,
            language,
            ..
        } if datatype == XSD_STRING || datatype == RDF_LANG_STRING => {
            Some((lexical_form.clone(), language.clone()))
        }
        _ => None,
    }
}

/// Apply a pure string transform to a single string argument, preserving its
/// language tag.
fn map_string(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
    f: impl Fn(&str) -> String,
) -> Result<Option<SolutionTerm>, EvalError> {
    match string_arg(vals, 0) {
        Some((s, lang)) => Ok(Some(make_string(ctx, f(&s), lang))),
        None => Ok(None),
    }
}

/// A two-string boolean predicate (CONTAINS/STRSTARTS/STRENDS).
fn string_pred(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
    f: impl Fn(&str, &str) -> bool,
) -> Result<Option<SolutionTerm>, EvalError> {
    match (string_arg(vals, 0), string_arg(vals, 1)) {
        (Some((h, _)), Some((n, _))) => Ok(Some(bool_term(ctx, f(&h, &n)))),
        _ => Ok(None),
    }
}

/// Intern a string literal, as `rdf:langString@lang` if a language is present, else
/// `xsd:string`.
fn make_string(ctx: &mut EvalCtx<'_>, lexical: String, lang: Option<String>) -> SolutionTerm {
    match lang {
        Some(l) => intern(
            ctx,
            TermValue::Literal {
                lexical_form: lexical,
                datatype: RDF_LANG_STRING.to_owned(),
                language: Some(l),
                direction: None,
            },
        ),
        None => string_term(ctx, lexical),
    }
}

/// `CONCAT(...)`: concatenate string arguments. The result keeps a common language
/// tag iff every argument shares it; otherwise it is `xsd:string`.
fn eval_concat(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let mut out = String::new();
    let mut common: Option<Option<String>> = None;
    for i in 0..vals.len() {
        let Some((s, lang)) = string_arg(vals, i) else {
            return Ok(None);
        };
        out.push_str(&s);
        common = Some(match common {
            None => lang,
            Some(prev) if prev == lang => prev,
            Some(_) => None,
        });
    }
    let lang = common.flatten();
    Ok(Some(make_string(ctx, out, lang)))
}

/// `SUBSTR(str, start[, length])` with 1-based indexing over Unicode scalars.
fn eval_substr(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let Some((s, lang)) = string_arg(vals, 0) else {
        return Ok(None);
    };
    let Some(start) = arg(vals, 1).and_then(xsd_int_of) else {
        return Ok(None);
    };
    let chars: Vec<char> = s.chars().collect();
    // SPARQL substr is 1-based; clamp to the string bounds.
    let start0 = (start - 1).max(0) as usize;
    let end = match vals.get(2).and_then(|v| v.as_ref()) {
        Some(len_val) => {
            let Some(len) = xsd_int_of(len_val) else {
                return Ok(None);
            };
            ((start - 1).max(0) + len.max(0)) as usize
        }
        None => chars.len(),
    };
    let slice: String = chars
        .get(start0..end.min(chars.len()))
        .unwrap_or(&[])
        .iter()
        .collect();
    Ok(Some(make_string(ctx, slice, lang)))
}

/// `STRBEFORE`/`STRAFTER(haystack, needle)`.
fn eval_str_before_after(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
    before: bool,
) -> Result<Option<SolutionTerm>, EvalError> {
    let (Some((h, lang)), Some((n, _))) = (string_arg(vals, 0), string_arg(vals, 1)) else {
        return Ok(None);
    };
    // An empty needle matches at the start: STRBEFORE → "", STRAFTER → the haystack.
    let result = match h.find(&n) {
        Some(idx) => {
            if before {
                h[..idx].to_owned()
            } else {
                h[idx + n.len()..].to_owned()
            }
        }
        // No match → empty (typed xsd:string, no language).
        None => return Ok(Some(string_term(ctx, String::new()))),
    };
    Ok(Some(make_string(ctx, result, lang)))
}

/// `REPLACE(str, pattern, replacement[, flags])` via the regex engine.
fn eval_replace(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let Some((s, lang)) = string_arg(vals, 0) else {
        return Ok(None);
    };
    let (Some((pattern, _)), Some((replacement, _))) = (string_arg(vals, 1), string_arg(vals, 2))
    else {
        return Ok(None);
    };
    let flags = string_arg(vals, 3).map(|(f, _)| f).unwrap_or_default();
    let Some(re) = build_regex(&pattern, &flags) else {
        return Ok(None);
    };
    // SPARQL uses $N for capture-group references — same as the regex crate.
    let replaced = re.replace_all(&s, replacement.as_str()).into_owned();
    Ok(Some(make_string(ctx, replaced, lang)))
}

/// `REGEX(text, pattern[, flags])`.
fn eval_regex(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let Some((text, _)) = string_arg(vals, 0) else {
        return Ok(None);
    };
    let Some((pattern, _)) = string_arg(vals, 1) else {
        return Ok(None);
    };
    let flags = string_arg(vals, 2).map(|(f, _)| f).unwrap_or_default();
    match build_regex(&pattern, &flags) {
        Some(re) => Ok(Some(bool_term(ctx, re.is_match(&text)))),
        None => Ok(None),
    }
}

/// Build a regex from a SPARQL pattern + flag string (`i`, `s`, `m`, `x`).
fn build_regex(pattern: &str, flags: &str) -> Option<regex::Regex> {
    let mut builder = regex::RegexBuilder::new(pattern);
    for f in flags.chars() {
        match f {
            'i' => builder.case_insensitive(true),
            's' => builder.dot_matches_new_line(true),
            'm' => builder.multi_line(true),
            'x' => builder.ignore_whitespace(true),
            _ => return None,
        };
    }
    builder.build().ok()
}

/// `langMatches(tag, range)` — RFC 4647 basic filtering (`*` matches any tag).
fn eval_lang_matches(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let (Some((tag, _)), Some((range, _))) = (string_arg(vals, 0), string_arg(vals, 1)) else {
        return Ok(None);
    };
    let tag = tag.to_ascii_lowercase();
    let range = range.to_ascii_lowercase();
    let matches = if range == "*" {
        !tag.is_empty()
    } else {
        tag == range || tag.starts_with(&format!("{range}-"))
    };
    Ok(Some(bool_term(ctx, matches)))
}

/// `STRLANG(lexical, lang)`.
fn eval_str_lang(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let (Some((lex, _)), Some((lang, _))) = (string_arg(vals, 0), string_arg(vals, 1)) else {
        return Ok(None);
    };
    Ok(Some(make_string(ctx, lex, Some(lang.to_ascii_lowercase()))))
}

/// `STRDT(lexical, datatypeIri)`.
fn eval_str_dt(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let Some((lex, _)) = string_arg(vals, 0) else {
        return Ok(None);
    };
    let Some(TermValue::Iri(dt)) = arg(vals, 1) else {
        return Ok(None);
    };
    Ok(Some(intern(ctx, typed(&lex, dt))))
}

/// `TRIPLE(s, p, o)` — RDF 1.2 triple-term constructor.
fn eval_triple_ctor(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
) -> Result<Option<SolutionTerm>, EvalError> {
    let (Some(s), Some(p), Some(o)) = (arg(vals, 0), arg(vals, 1), arg(vals, 2)) else {
        return Ok(None);
    };
    let triple = TermValue::Triple {
        s: Box::new(s.clone()),
        p: Box::new(p.clone()),
        o: Box::new(o.clone()),
    };
    Ok(Some(intern(ctx, triple)))
}

/// Extract a component of a triple term (`SUBJECT`/`PREDICATE`/`OBJECT`).
fn triple_part(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
    pick: impl Fn(TermValue, TermValue, TermValue) -> TermValue,
) -> Result<Option<SolutionTerm>, EvalError> {
    match arg(vals, 0) {
        Some(TermValue::Triple { s, p, o }) => {
            let part = pick((**s).clone(), (**p).clone(), (**o).clone());
            Ok(Some(intern(ctx, part)))
        }
        _ => Ok(None),
    }
}

/// An `i64` from an XSD integer argument value.
fn xsd_int_of(v: &TermValue) -> Option<i64> {
    match xsd_of(v)? {
        XsdValue::Integer { value, .. } => i64::try_from(value).ok(),
        _ => None,
    }
}

/// Convert a computed [`XsdValue`] back into an interned [`SolutionTerm`] using the
/// canonical typed-literal form. The datatype IRI comes from `v.datatype().iri()`.
fn xsd_to_term(ctx: &mut EvalCtx<'_>, v: &XsdValue) -> SolutionTerm {
    intern(ctx, typed(&v.canonical_lexical(), v.datatype().iri()))
}

/// Evaluate a binary numeric expression: resolve both operands to [`XsdValue`], call
/// `op`, and return `Ok(Some(term))` on success or `Ok(None)` on any error (type
/// error, overflow, divide-by-zero — all SPARQL expression errors).
fn binary_numeric(
    a: &Expression,
    b: &Expression,
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
    op: impl Fn(&XsdValue, &XsdValue) -> Result<XsdValue, gmeow_xsd::XsdError>,
) -> Result<Option<SolutionTerm>, EvalError> {
    let (Some(ta), Some(tb)) = (
        eval_expr(a, row, schema, ctx)?,
        eval_expr(b, row, schema, ctx)?,
    ) else {
        return Ok(None);
    };
    let (va, vb) = (value_of(ctx, ta), value_of(ctx, tb));
    let (Some(xa), Some(xb)) = (xsd_of(&va), xsd_of(&vb)) else {
        return Ok(None); // non-numeric operand → SPARQL type error
    };
    match op(&xa, &xb) {
        Ok(result) => Ok(Some(xsd_to_term(ctx, &result))),
        Err(_) => Ok(None), // overflow / div-by-zero / type-mismatch → expression error
    }
}

/// Evaluate a unary numeric expression (`+` / `-`): resolve the operand, call `op`,
/// return `Ok(None)` on any error.
fn unary_numeric(
    a: &Expression,
    row: &[Option<SolutionTerm>],
    schema: &VarSchema,
    ctx: &mut EvalCtx<'_>,
    op: impl Fn(&XsdValue) -> Result<XsdValue, gmeow_xsd::XsdError>,
) -> Result<Option<SolutionTerm>, EvalError> {
    let Some(ta) = eval_expr(a, row, schema, ctx)? else {
        return Ok(None);
    };
    let va = value_of(ctx, ta);
    let Some(xa) = xsd_of(&va) else {
        return Ok(None);
    };
    match op(&xa) {
        Ok(result) => Ok(Some(xsd_to_term(ctx, &result))),
        Err(_) => Ok(None),
    }
}

/// Apply a unary numeric function from the `vals` pre-evaluated argument list.
/// Argument 0 must be a numeric literal; type errors → `Ok(None)`.
fn unary_numeric_fn(
    ctx: &mut EvalCtx<'_>,
    vals: &[Option<TermValue>],
    op: impl Fn(&XsdValue) -> Result<XsdValue, gmeow_xsd::XsdError>,
) -> Result<Option<SolutionTerm>, EvalError> {
    let Some(xa) = arg(vals, 0).and_then(xsd_of) else {
        return Ok(None);
    };
    match op(&xa) {
        Ok(result) => Ok(Some(xsd_to_term(ctx, &result))),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf_core::{RdfDataset, RdfDatasetBuilder};
    use gmeow_sparql_algebra::{Literal, NamedNode};
    use std::sync::Arc;

    fn empty_ds() -> Arc<RdfDataset> {
        RdfDatasetBuilder::new().freeze().expect("freeze")
    }

    fn lit(value: &str) -> Expression {
        Expression::Literal(Literal::new_simple(value))
    }
    fn typed_lit(value: &str, dt: &str) -> Expression {
        Expression::Literal(Literal::new_typed(value, NamedNode::new_unchecked(dt)))
    }
    fn iri(iri: &str) -> Expression {
        Expression::NamedNode(NamedNode::new_unchecked(iri))
    }

    /// Evaluate a constant expression (empty solution) and return the EBV.
    fn ebv(ds: &RdfDataset, expr: &Expression) -> Option<bool> {
        let mut ctx = EvalCtx::new(ds);
        let schema = VarSchema::new();
        eval_ebv(expr, &[], &schema, &mut ctx).expect("eval")
    }

    /// Evaluate a constant expression to a string lexical form, if it is a literal.
    fn lex(ds: &RdfDataset, expr: &Expression) -> Option<String> {
        let mut ctx = EvalCtx::new(ds);
        let schema = VarSchema::new();
        let term = eval_expr(expr, &[], &schema, &mut ctx).expect("eval")?;
        match value_of(&ctx, term) {
            TermValue::Literal { lexical_form, .. } => Some(lexical_form),
            TermValue::Iri(s) => Some(s),
            _ => None,
        }
    }

    const XINT: &str = "http://www.w3.org/2001/XMLSchema#integer";

    #[test]
    fn numeric_comparison_uses_value_space() {
        let ds = empty_ds();
        // "2"^^xsd:integer < "10"^^xsd:integer (value, not lexicographic).
        let lt = Expression::Less(
            Box::new(typed_lit("2", XINT)),
            Box::new(typed_lit("10", XINT)),
        );
        assert_eq!(ebv(&ds, &lt), Some(true));
    }

    #[test]
    fn kleene_or_with_error_and_true_is_true() {
        let ds = empty_ds();
        // (error || true) == true, even though the left operand errors.
        let err = Expression::Less(Box::new(iri("http://ex/a")), Box::new(iri("http://ex/b")));
        let expr = Expression::Or(
            Box::new(err),
            Box::new(typed_lit(
                "true",
                "http://www.w3.org/2001/XMLSchema#boolean",
            )),
        );
        assert_eq!(ebv(&ds, &expr), Some(true));
    }

    #[test]
    fn kleene_and_with_error_and_false_is_false() {
        let ds = empty_ds();
        let err = Expression::Less(Box::new(iri("http://ex/a")), Box::new(iri("http://ex/b")));
        let expr = Expression::And(
            Box::new(err),
            Box::new(typed_lit(
                "false",
                "http://www.w3.org/2001/XMLSchema#boolean",
            )),
        );
        assert_eq!(ebv(&ds, &expr), Some(false));
    }

    #[test]
    fn sameterm_distinguishes_lexical_forms() {
        let ds = empty_ds();
        // "1"^^xsd:integer = "01"^^xsd:integer (value equal) but NOT sameTerm.
        let eq = Expression::Equal(
            Box::new(typed_lit("1", XINT)),
            Box::new(typed_lit("01", XINT)),
        );
        let same = Expression::SameTerm(
            Box::new(typed_lit("1", XINT)),
            Box::new(typed_lit("01", XINT)),
        );
        assert_eq!(ebv(&ds, &eq), Some(true));
        assert_eq!(ebv(&ds, &same), Some(false));
    }

    #[test]
    fn str_and_concat_and_strlen() {
        let ds = empty_ds();
        let concat = Expression::FunctionCall(Function::Concat, vec![lit("foo"), lit("bar")]);
        assert_eq!(lex(&ds, &concat), Some("foobar".to_owned()));
        let strlen = Expression::FunctionCall(Function::StrLen, vec![lit("héllo")]);
        assert_eq!(lex(&ds, &strlen), Some("5".to_owned()));
        let str_of_iri = Expression::FunctionCall(Function::Str, vec![iri("http://ex/x")]);
        assert_eq!(lex(&ds, &str_of_iri), Some("http://ex/x".to_owned()));
    }

    #[test]
    fn contains_and_regex() {
        let ds = empty_ds();
        let contains =
            Expression::FunctionCall(Function::Contains, vec![lit("hello world"), lit("o w")]);
        assert_eq!(ebv(&ds, &contains), Some(true));
        let re = Expression::FunctionCall(
            Function::Regex,
            vec![lit("Hello"), lit("^h"), lit("i")], // case-insensitive
        );
        assert_eq!(ebv(&ds, &re), Some(true));
    }

    #[test]
    fn substr_one_based() {
        let ds = empty_ds();
        // SUBSTR("abcdef", 2, 3) == "bcd".
        let s = Expression::FunctionCall(
            Function::SubStr,
            vec![lit("abcdef"), typed_lit("2", XINT), typed_lit("3", XINT)],
        );
        assert_eq!(lex(&ds, &s), Some("bcd".to_owned()));
    }

    #[test]
    fn type_tests() {
        let ds = empty_ds();
        assert_eq!(
            ebv(
                &ds,
                &Expression::FunctionCall(Function::IsIri, vec![iri("http://ex/x")])
            ),
            Some(true)
        );
        assert_eq!(
            ebv(
                &ds,
                &Expression::FunctionCall(Function::IsLiteral, vec![lit("x")])
            ),
            Some(true)
        );
        assert_eq!(
            ebv(
                &ds,
                &Expression::FunctionCall(Function::IsNumeric, vec![typed_lit("3", XINT)])
            ),
            Some(true)
        );
        assert_eq!(
            ebv(
                &ds,
                &Expression::FunctionCall(Function::IsNumeric, vec![lit("x")])
            ),
            Some(false)
        );
    }

    #[test]
    fn coalesce_skips_errors() {
        let ds = empty_ds();
        // COALESCE(error, "fallback") → "fallback".
        let err = Expression::FunctionCall(Function::Str, vec![]); // STR() with no arg → error
        let expr = Expression::Coalesce(vec![err, lit("fallback")]);
        assert_eq!(lex(&ds, &expr), Some("fallback".to_owned()));
    }

    const XDEC: &str = "http://www.w3.org/2001/XMLSchema#decimal";

    // ---- arithmetic: positive tests ----------------------------------------

    #[test]
    fn arithmetic_add_integers() {
        let ds = empty_ds();
        // 1 + 2 = 3
        let expr = Expression::Add(
            Box::new(typed_lit("1", XINT)),
            Box::new(typed_lit("2", XINT)),
        );
        assert_eq!(lex(&ds, &expr), Some("3".to_owned()));
    }

    #[test]
    fn arithmetic_subtract_integers() {
        let ds = empty_ds();
        // 7 - 3 = 4
        let expr = Expression::Subtract(
            Box::new(typed_lit("7", XINT)),
            Box::new(typed_lit("3", XINT)),
        );
        assert_eq!(lex(&ds, &expr), Some("4".to_owned()));
    }

    #[test]
    fn arithmetic_multiply_integers() {
        let ds = empty_ds();
        // 3 * 4 = 12
        let expr = Expression::Multiply(
            Box::new(typed_lit("3", XINT)),
            Box::new(typed_lit("4", XINT)),
        );
        assert_eq!(lex(&ds, &expr), Some("12".to_owned()));
    }

    #[test]
    fn arithmetic_divide_integer_returns_decimal() {
        let ds = empty_ds();
        // 1 / 2 = 0.5 (decimal, per XPath op:numeric-divide)
        let expr = Expression::Divide(
            Box::new(typed_lit("1", XINT)),
            Box::new(typed_lit("2", XINT)),
        );
        // The result is a decimal; lexical "0.5" at scale 18 → canonical starts "0.5"
        let result = lex(&ds, &expr).expect("should produce a value");
        // Parse it back to verify the value; the canonical form has 18 fractional
        // digits so we just check that it starts with "0.5".
        assert!(
            result.starts_with("0.5"),
            "1/2 should be 0.5…, got {result}"
        );
    }

    #[test]
    fn arithmetic_divide_10_4() {
        let ds = empty_ds();
        // 10 / 4 = 2.5
        let expr = Expression::Divide(
            Box::new(typed_lit("10", XINT)),
            Box::new(typed_lit("4", XINT)),
        );
        let result = lex(&ds, &expr).expect("should produce a value");
        assert!(
            result.starts_with("2.5"),
            "10/4 should be 2.5…, got {result}"
        );
    }

    // ---- arithmetic: type error and divide-by-zero → Ok(None) --------------

    #[test]
    fn arithmetic_type_error_is_ok_none() {
        let ds = empty_ds();
        // "a" + 1 → type error → Ok(None) (a FILTER drops the row; no hard Err).
        let expr = Expression::Add(Box::new(lit("a")), Box::new(typed_lit("1", XINT)));
        let mut ctx = EvalCtx::new(&ds);
        let schema = VarSchema::new();
        let result = eval_expr(&expr, &[], &schema, &mut ctx).expect("no hard error");
        assert!(
            result.is_none(),
            "type error must be Ok(None), not Ok(Some)"
        );
    }

    #[test]
    fn arithmetic_divide_by_zero_is_ok_none() {
        let ds = empty_ds();
        // integer/0 → DivisionByZero → Ok(None)
        let expr = Expression::Divide(
            Box::new(typed_lit("5", XINT)),
            Box::new(typed_lit("0", XINT)),
        );
        let mut ctx = EvalCtx::new(&ds);
        let schema = VarSchema::new();
        let result = eval_expr(&expr, &[], &schema, &mut ctx).expect("no hard error");
        assert!(result.is_none(), "divide-by-zero must be Ok(None)");
    }

    // ---- unary operators ---------------------------------------------------

    #[test]
    fn arithmetic_unary_minus() {
        let ds = empty_ds();
        // -5 = -5
        let expr = Expression::UnaryMinus(Box::new(typed_lit("5", XINT)));
        assert_eq!(lex(&ds, &expr), Some("-5".to_owned()));
    }

    // ---- ABS / CEIL / FLOOR / ROUND ----------------------------------------

    #[test]
    fn function_abs() {
        let ds = empty_ds();
        // ABS(-3) = 3
        let expr = Expression::FunctionCall(Function::Abs, vec![typed_lit("-3", XINT)]);
        assert_eq!(lex(&ds, &expr), Some("3".to_owned()));
    }

    #[test]
    fn function_ceil() {
        let ds = empty_ds();
        // CEIL(2.1) = 3 (as xsd:decimal)
        let expr = Expression::FunctionCall(Function::Ceil, vec![typed_lit("2.1", XDEC)]);
        assert_eq!(lex(&ds, &expr), Some("3.0".to_owned()));
    }

    #[test]
    fn function_floor() {
        let ds = empty_ds();
        // FLOOR(2.9) = 2 (as xsd:decimal)
        let expr = Expression::FunctionCall(Function::Floor, vec![typed_lit("2.9", XDEC)]);
        assert_eq!(lex(&ds, &expr), Some("2.0".to_owned()));
    }

    #[test]
    fn function_round() {
        let ds = empty_ds();
        // ROUND(2.5) = 3 (round-half-toward-+infinity per XPath fn:round)
        let expr = Expression::FunctionCall(Function::Round, vec![typed_lit("2.5", XDEC)]);
        assert_eq!(lex(&ds, &expr), Some("3.0".to_owned()));
    }

    // ---- BIND integration: arithmetic column over a real BGP ---------------

    #[test]
    fn bind_arithmetic_computed_column() {
        let ds = typed_graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?s :age ?n . BIND(?n + 1 AS ?plus1) }
        // :a has age 30, so plus1 should be 31.
        // :b has age 17, so plus1 should be 18.
        let inner = bgp1("s", "http://ex/age", "n");
        let expr = Expression::Add(
            Box::new(Expression::Variable(Variable::new("n"))),
            Box::new(typed_lit("1", XINT)),
        );
        let seq = eval(
            &GraphPattern::Extend {
                inner: Box::new(inner),
                variable: Variable::new("plus1"),
                expression: expr,
            },
            &mut ctx,
        )
        .expect("bind arithmetic");
        let plus1_col = seq.schema.index_of(&Variable::new("plus1")).unwrap();
        let mut results: Vec<String> = seq
            .rows
            .iter()
            .filter_map(|r| r[plus1_col])
            .map(|t| match ctx.scratch.value_of(&ds, t) {
                TermValue::Literal { lexical_form, .. } => lexical_form,
                other => format!("{other:?}"),
            })
            .collect();
        results.sort();
        assert_eq!(results, vec!["18".to_owned(), "31".to_owned()]);
    }

    // --- integration: FILTER / BIND / EXISTS over a real BGP ---------------

    fn typed_graph() -> Arc<RdfDataset> {
        // :a :age 30 ; :name "Ann" .
        // :b :age 17 .
        // :a :member :club .   (a is a member; b is not)
        use gmeow_rdf_core::RdfLiteral;
        let mut b = RdfDatasetBuilder::new();
        let age = b.intern_iri("http://ex/age".to_owned());
        let name = b.intern_iri("http://ex/name".to_owned());
        let member = b.intern_iri("http://ex/member".to_owned());
        let a = b.intern_iri("http://ex/a".to_owned());
        let bb = b.intern_iri("http://ex/b".to_owned());
        let club = b.intern_iri("http://ex/club".to_owned());
        let i30 = b.intern_literal(RdfLiteral {
            lexical_form: "30".to_owned(),
            datatype: Some(XINT.to_owned()),
            language: None,
            direction: None,
        });
        let i17 = b.intern_literal(RdfLiteral {
            lexical_form: "17".to_owned(),
            datatype: Some(XINT.to_owned()),
            language: None,
            direction: None,
        });
        let ann = b.intern_literal(RdfLiteral::simple("Ann"));
        b.push_quad(a, age, i30, None);
        b.push_quad(a, name, ann, None);
        b.push_quad(bb, age, i17, None);
        b.push_quad(a, member, club, None);
        b.freeze().expect("freeze")
    }

    fn bgp1(s: &str, p: &str, o: &str) -> GraphPattern {
        use gmeow_sparql_algebra::{NamedNodePattern, TermPattern, TriplePattern};
        GraphPattern::Bgp {
            patterns: vec![TriplePattern {
                subject: TermPattern::Variable(Variable::new(s)),
                predicate: NamedNodePattern::NamedNode(NamedNode::new_unchecked(p)),
                object: TermPattern::Variable(Variable::new(o)),
            }],
        }
    }

    fn subjects(ds: &RdfDataset, seq: &SolutionSeq, var: &str) -> Vec<String> {
        let scratch = crate::scratch::ScratchInterner::new();
        let col = seq.schema.index_of(&Variable::new(var)).unwrap();
        let mut out: Vec<String> = seq
            .rows
            .iter()
            .filter_map(|r| r[col])
            .map(|t| match scratch.value_of(ds, t) {
                TermValue::Iri(s) => s,
                other => format!("{other:?}"),
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn filter_numeric_over_bgp() {
        let ds = typed_graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?s :age ?n FILTER(?n >= 18) } → only :a.
        let inner = bgp1("s", "http://ex/age", "n");
        let cond = Expression::GreaterOrEqual(
            Box::new(Expression::Variable(Variable::new("n"))),
            Box::new(typed_lit("18", XINT)),
        );
        let seq = eval(
            &GraphPattern::Filter {
                expr: cond,
                inner: Box::new(inner),
            },
            &mut ctx,
        )
        .expect("filter");
        assert_eq!(subjects(&ds, &seq, "s"), vec!["http://ex/a".to_owned()]);
    }

    #[test]
    fn bind_adds_a_computed_column() {
        let ds = typed_graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?s :name ?nm . BIND(UCASE(?nm) AS ?u) }
        let inner = bgp1("s", "http://ex/name", "nm");
        let expr = Expression::FunctionCall(
            Function::UCase,
            vec![Expression::Variable(Variable::new("nm"))],
        );
        let seq = eval(
            &GraphPattern::Extend {
                inner: Box::new(inner),
                variable: Variable::new("u"),
                expression: expr,
            },
            &mut ctx,
        )
        .expect("bind");
        let u = seq.schema.index_of(&Variable::new("u")).unwrap();
        // UCASE("Ann") = "ANN" is a *computed* term, so it must be resolved through
        // the SAME scratch interner that the evaluation used (a fresh one cannot
        // resolve scratch ids — only dataset-resident `Existing` terms).
        let val = ctx.scratch.value_of(&ds, seq.rows[0][u].unwrap());
        assert!(matches!(val, TermValue::Literal { lexical_form, .. } if lexical_form == "ANN"));
    }

    #[test]
    fn filter_not_exists_over_bgp() {
        let ds = typed_graph();
        let mut ctx = EvalCtx::new(&ds);
        // { ?s :age ?n FILTER NOT EXISTS { ?s :member ?c } } → people with an age
        // who are NOT members → only :b (a is a member).
        let inner = bgp1("s", "http://ex/age", "n");
        let exists_pat = bgp1("s", "http://ex/member", "c");
        let not_exists = Expression::Not(Box::new(Expression::Exists(Box::new(exists_pat))));
        let seq = eval(
            &GraphPattern::Filter {
                expr: not_exists,
                inner: Box::new(inner),
            },
            &mut ctx,
        )
        .expect("not exists");
        assert_eq!(subjects(&ds, &seq, "s"), vec!["http://ex/b".to_owned()]);
    }
}
