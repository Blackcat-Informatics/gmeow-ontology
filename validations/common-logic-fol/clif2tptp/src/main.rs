// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//
//! `clif2tptp` — a semantics-preserving translator from a gmeow-dialect CLIF
//! (Common Logic Interchange Format) export to TPTP-FOF, for the
//! `validations/common-logic-fol/` external cross-check lane.
//!
//! It NEVER hand-parses CLIF: the CLIF text is lifted back to the real
//! `gmeow_logic_compile::ir::LogicProgram` via
//! [`gmeow_logic_compile::clif::parse_clif_str`] — the same reader the in-gate
//! `crates/logic` CL-ingest test exercises — and this binary only renders that
//! reconstructed IR to TPTP-FOF. The IR is a generic (predicate, subject,
//! object) relational core; no DL-specific knowledge is needed to translate it.
//!
//! ## Encoding
//!
//! - An IRI constant becomes a single-quoted TPTP distinct atom `'<iri>'`.
//! - A literal becomes a single-quoted DISTINCT constant atom `'lit|<lexical>|<datatype>'`
//!   (kept apart from the IRI value space so a literal can never unify with an IRI).
//! - A variable (`?Name` in a [`LogicAxiom`] string, or a [`Term::Var`] /
//!   quantifier binder name) becomes the TPTP variable `V_<UPPERCASED-NAME>`.
//! - A [`Term::SequenceMarker`] is outside first-order FOF: this is a hard
//!   BOUNDARY (see [`main`]).
//!
//! Output is fully deterministic: axioms are numbered in encounter order.
use std::env;
use std::fs;
use std::process::ExitCode;

use gmeow_logic_compile::clif::parse_clif_str;
use gmeow_logic_compile::ir::{Formula, LogicAxiom, LogicProgram, Term};

/// A hard, named boundary: an input construct that cannot be expressed in
/// first-order TPTP-FOF. Printed to stderr as `BOUNDARY <what>: <reason>` and
/// causes a non-zero exit — never silently dropped or approximated.
struct Boundary(String);

impl Boundary {
    fn unsupported(what: &str, reason: impl AsRef<str>) -> Self {
        Boundary(format!(
            "BOUNDARY unsupported-construct: {what}: {}",
            reason.as_ref()
        ))
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    match run(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(Boundary(msg)) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

/// Parsed CLI arguments (see the module doc / README for the contract).
struct Args {
    clif_file: String,
    edb_file: Option<String>,
    conjecture: Option<(String, String, String)>,
}

fn parse_args(argv: &[String]) -> Result<Args, Boundary> {
    let mut clif_file: Option<String> = None;
    let mut edb_file: Option<String> = None;
    let mut conjecture: Option<(String, String, String)> = None;

    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--edb" => {
                let path = argv.get(i + 1).ok_or_else(|| {
                    Boundary("BOUNDARY cli: --edb requires a file argument".to_owned())
                })?;
                edb_file = Some(path.clone());
                i += 2;
            }
            "--conjecture" => {
                let pred = argv.get(i + 1);
                let subj = argv.get(i + 2);
                let obj = argv.get(i + 3);
                match (pred, subj, obj) {
                    (Some(p), Some(s), Some(o)) => {
                        conjecture = Some((p.clone(), s.clone(), o.clone()));
                        i += 4;
                    }
                    _ => {
                        return Err(Boundary(
                            "BOUNDARY cli: --conjecture requires <pred-iri> <subj-iri> <obj-iri>"
                                .to_owned(),
                        ));
                    }
                }
            }
            other => {
                if clif_file.is_some() {
                    return Err(Boundary(format!(
                        "BOUNDARY cli: unexpected extra positional argument '{other}'"
                    )));
                }
                clif_file = Some(other.to_owned());
                i += 1;
            }
        }
    }

    let clif_file = clif_file.ok_or_else(|| {
        Boundary("BOUNDARY cli: missing required <clif-file> positional argument".to_owned())
    })?;

    Ok(Args {
        clif_file,
        edb_file,
        conjecture,
    })
}

fn run(argv: &[String]) -> Result<String, Boundary> {
    let args = parse_args(argv)?;

    let clif_text = fs::read_to_string(&args.clif_file).map_err(|e| {
        Boundary(format!(
            "BOUNDARY io: could not read CLIF file '{}': {e}",
            args.clif_file
        ))
    })?;

    let (program, _diags) = parse_clif_str(&clif_text, Some(args.clif_file.clone()))
        .map_err(|e| Boundary(format!("BOUNDARY clif-parse: {e}")))?;

    let mut out = String::new();
    let mut n: usize = 0;

    render_program(&program, &mut out, &mut n)?;

    if let Some(edb_path) = &args.edb_file {
        let edb_text = fs::read_to_string(edb_path).map_err(|e| {
            Boundary(format!(
                "BOUNDARY io: could not read --edb file '{edb_path}': {e}"
            ))
        })?;
        render_edb(&edb_text, &mut out, &mut n)?;
    }

    if let Some((pred, subj, obj)) = &args.conjecture {
        out.push_str(&format!(
            "fof(goal, conjecture, {}({},{})).\n",
            quote_iri(pred),
            quote_iri(subj),
            quote_iri(obj)
        ));
    }

    Ok(out)
}

// --------------------------------------------------------------------------- //
// TPTP atom / term quoting
// --------------------------------------------------------------------------- //

/// Escape a lexical form for embedding inside a TPTP single-quoted atom:
/// backslash first, then the single quote (order matters — escaping the quote
/// first would double-escape the backslashes it introduces).
fn tptp_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// A CL/RDF IRI constant, quoted as a single TPTP distinct atom `'<iri>'`.
fn quote_iri(iri: &str) -> String {
    format!("'{}'", tptp_escape(iri))
}

/// A CL/RDF literal constant, quoted as a DISTINCT single-quoted atom that
/// encodes both lexical and datatype so it can never unify with an IRI or a
/// differently-typed literal of the same lexical form.
fn quote_literal(lexical: &str, datatype: Option<&str>) -> String {
    let dt = datatype.unwrap_or("plain");
    format!("'lit|{}|{}'", tptp_escape(lexical), tptp_escape(dt))
}

/// A TPTP variable name: `V_` + the uppercased, sanitized (non-alphanumeric →
/// `_`) authored variable name. `name` may or may not carry the `?` sigil.
fn tptp_var(name: &str) -> String {
    let stripped = name.strip_prefix('?').unwrap_or(name);
    let mut v = String::from("V_");
    for c in stripped.chars() {
        if c.is_ascii_alphanumeric() {
            v.push(c.to_ascii_uppercase());
        } else {
            v.push('_');
        }
    }
    v
}

/// Render one bare `LogicAxiom` term string (subject or object: a bare IRI, a
/// `?var`, or — when `is_literal` — a literal lexical) as a TPTP term.
///
/// A leading `?` wins over `obj_is_literal`: the CLIF meta-channel's Horn
/// operand encoding represents a `?var` in OBJECT position as `(lit "?Y")` —
/// i.e. `obj_is_literal=true` with a `?`-prefixed lexical — so a var must be
/// detected by its `?` sigil first, never by the literal bit alone (a
/// previously-caught trap; see `horn_operand`/`parse_lit_simple` in
/// `gmeow_logic_compile::clif::reader`).
fn term_str(value: &str, is_literal: bool) -> String {
    if value.starts_with('?') {
        tptp_var(value)
    } else if is_literal {
        quote_literal(value, None)
    } else {
        quote_iri(value)
    }
}

/// Render one `LogicAxiom` predication (subject/predicate/object atom), honoring
/// `negated` (negation-as-failure body literal — becomes TPTP strong negation,
/// the closest FOF has; the boundary between NAF and strong negation is a
/// pre-existing modeling choice of the projection, not introduced here).
fn axiom_atom(ax: &LogicAxiom) -> String {
    let atom = format!(
        "{}({},{})",
        quote_iri(&ax.predicate),
        term_str(&ax.subject, false),
        term_str(&ax.obj, ax.obj_is_literal)
    );
    if ax.negated {
        format!("~({atom})")
    } else {
        atom
    }
}

/// Collect every distinct `?var` occurring (as subject or object) across a
/// rule's head + body, in first-encounter order (deterministic emission).
///
/// Checks the `?` sigil directly (never gated on `obj_is_literal` — see the
/// `term_str` doc comment for why the literal bit alone is not a reliable var
/// detector for the object position).
fn rule_vars(head: &LogicAxiom, body: &[LogicAxiom]) -> Vec<String> {
    let mut seen = Vec::new();
    let mut note = |s: &str| {
        if s.starts_with('?') && !seen.contains(&s.to_owned()) {
            seen.push(s.to_owned());
        }
    };
    note(&head.subject);
    note(&head.obj);
    for b in body {
        note(&b.subject);
        note(&b.obj);
    }
    seen
}

fn render_program(program: &LogicProgram, out: &mut String, n: &mut usize) -> Result<(), Boundary> {
    // Top-level axioms are asserted positive facts (never a bare NAF literal).
    for ax in &program.axioms {
        *n += 1;
        out.push_str(&format!("fof(ax_{n}, axiom, {}).\n", axiom_atom(ax)));
    }

    for rule in &program.rules {
        *n += 1;
        if rule.body.is_empty() && rule.distinct_pairs.is_empty() {
            out.push_str(&format!(
                "fof(rule_{n}, axiom, {}).\n",
                axiom_atom(&rule.head)
            ));
            continue;
        }

        let vars = rule_vars(&rule.head, &rule.body);
        let mut conjuncts: Vec<String> = rule.body.iter().map(axiom_atom).collect();
        for (a, b) in &rule.distinct_pairs {
            conjuncts.push(format!("{} != {}", tptp_var(a), tptp_var(b)));
        }
        let antecedent = conjuncts.join(" & ");
        let quant = if vars.is_empty() {
            String::new()
        } else {
            format!(
                "![{}] : ",
                vars.iter()
                    .map(|v| tptp_var(v))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        out.push_str(&format!(
            "fof(rule_{n}, axiom, {quant}(({antecedent}) => {})).\n",
            axiom_atom(&rule.head)
        ));
    }

    for formula in &program.formulas {
        *n += 1;
        let body = render_formula(formula)?;
        out.push_str(&format!("fof(formula_{n}, axiom, {body}).\n"));
    }

    Ok(())
}

/// Render a [`Term`] (as it occurs inside a full [`Formula`], distinct from the
/// bare-string [`LogicAxiom`] terms above) to a TPTP term.
fn render_term(term: &Term) -> Result<String, Boundary> {
    match term {
        Term::Var(name) => Ok(tptp_var(name)),
        Term::Iri(iri) => Ok(quote_iri(iri)),
        Term::Literal { lexical, datatype } => Ok(quote_literal(lexical, datatype.as_deref())),
        Term::SequenceMarker(name) => Err(Boundary::unsupported(
            "sequence-marker",
            format!("Common Logic sequence marker '{name}' is not first-order FOF-expressible"),
        )),
    }
}

fn render_formula(formula: &Formula) -> Result<String, Boundary> {
    match formula {
        Formula::Atom { relation, args } => {
            let rel = match relation {
                Term::Iri(iri) => quote_iri(iri),
                other => {
                    return Err(Boundary::unsupported(
                        "non-iri-relation",
                        format!("a Formula::Atom relation position must be an IRI, got {other:?}"),
                    ));
                }
            };
            let mut rendered_args = Vec::with_capacity(args.len());
            for a in args {
                rendered_args.push(render_term(a)?);
            }
            Ok(format!("{rel}({})", rendered_args.join(",")))
        }
        Formula::Not(inner) => Ok(format!("~({})", render_formula(inner)?)),
        Formula::And(items) => join_connective(items, "&"),
        Formula::Or(items) => join_connective(items, "|"),
        Formula::Implies(a, b) => Ok(format!(
            "({} => {})",
            render_formula(a)?,
            render_formula(b)?
        )),
        Formula::Iff(a, b) => Ok(format!(
            "({} <=> {})",
            render_formula(a)?,
            render_formula(b)?
        )),
        Formula::Forall { vars, body } => Ok(format!(
            "![{}] : ({})",
            vars.iter()
                .map(|v| tptp_var(v))
                .collect::<Vec<_>>()
                .join(","),
            render_formula(body)?
        )),
        Formula::Exists { vars, body } => Ok(format!(
            "?[{}] : ({})",
            vars.iter()
                .map(|v| tptp_var(v))
                .collect::<Vec<_>>()
                .join(","),
            render_formula(body)?
        )),
    }
}

fn join_connective(items: &[Formula], op: &str) -> Result<String, Boundary> {
    let mut rendered = Vec::with_capacity(items.len());
    for item in items {
        rendered.push(render_formula(item)?);
    }
    Ok(format!("({})", rendered.join(&format!(" {op} "))))
}

// --------------------------------------------------------------------------- //
// `--edb` N-Quads loader
// --------------------------------------------------------------------------- //

/// Split one N-Quads line into its subject / predicate / object tokens. Kept
/// deliberately simple (this lane is not a general N-Quads parser): a term is
/// either an angle-bracketed IRI `<...>` or a double-quoted literal `"..."`;
/// the graph term (4th field, if present) is ignored.
fn split_nquads_line(line: &str) -> Result<(String, String, String, bool), Boundary> {
    let mut rest = line.trim();
    let mut fields: Vec<(String, bool)> = Vec::new();

    while fields.len() < 3 {
        rest = rest.trim_start();
        if rest.starts_with('<') {
            let end = rest.find('>').ok_or_else(|| {
                Boundary(format!(
                    "BOUNDARY edb-parse: unterminated IRI in line: {line}"
                ))
            })?;
            fields.push((rest[1..end].to_owned(), false));
            rest = &rest[end + 1..];
        } else if rest.starts_with('"') {
            // Find the closing quote, honoring backslash-escapes.
            let bytes = rest.as_bytes();
            let mut i = 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    break;
                }
                i += 1;
            }
            if i >= bytes.len() {
                return Err(Boundary(format!(
                    "BOUNDARY edb-parse: unterminated literal in line: {line}"
                )));
            }
            fields.push((rest[1..i].to_owned(), true));
            rest = &rest[i + 1..];
            // Skip a trailing ^^<datatype> or @lang tag, if present, up to whitespace.
            rest = rest.trim_start();
            if rest.starts_with("^^") || rest.starts_with('@') {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                rest = &rest[end..];
            }
        } else {
            return Err(Boundary(format!(
                "BOUNDARY edb-parse: expected '<' or '\"' term in line: {line}"
            )));
        }
    }

    let (s, s_lit) = &fields[0];
    let (p, _) = &fields[1];
    let (o, o_lit) = &fields[2];
    if *s_lit {
        return Err(Boundary(format!(
            "BOUNDARY edb-parse: subject term must be an IRI, got a literal in line: {line}"
        )));
    }
    Ok((s.clone(), p.clone(), o.clone(), *o_lit))
}

fn render_edb(edb_text: &str, out: &mut String, n: &mut usize) -> Result<(), Boundary> {
    for line in edb_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (s, p, o, o_is_literal) = split_nquads_line(trimmed)?;
        *n += 1;
        let obj_term = if o_is_literal {
            quote_literal(&o, None)
        } else {
            quote_iri(&o)
        };
        out.push_str(&format!(
            "fof(edb_{n}, axiom, {}({},{})).\n",
            quote_iri(&p),
            quote_iri(&s),
            obj_term
        ));
    }
    Ok(())
}
