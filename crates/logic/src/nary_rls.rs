// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The n-ary `.rls` PROGRAM parser and the n-ary delimited-data EDB loader.
//!
//! # One `.rls` program, two engines
//!
//! [`parse_nary_rls_program`] parses an n-ary multi-head existential `.rls` program into
//! [`Vec<NaryRule>`] using the SAME Nemo front-end
//! ([`crate::nemo_engine::NemoParsedRules::parse_unvalidated`]) the binary
//! [`crate::rule_ir::parse_eval_rules`] and the arity-generic
//! [`crate::physical::generic::parse_generic_rules`] use — so the predicate / variable
//! surface is byte-identical to the engine, and the SAME program text drives BOTH the
//! native path (via the n-ary → reified-binary lowering,
//! [`crate::nary::run_native_nary_forward`]) and the Nemo oracle (verbatim,
//! [`crate::nary::run_nemo_nary_forward`]). Every n-ary atom is preserved at FULL arity
//! (no subject/object/world projection — an n-ary tuple has no world slot); an existential
//! head variable is one occurring in the head but bound by no positive body atom, detected
//! structurally downstream by the reified lowering exactly as
//! [`crate::physical::ExistentialRule::existentials`] does.
//!
//! # What is REFUSED, not mis-parsed (the engine's refusal discipline)
//!
//! The n-ary fragment is the fixed-arity, range-restricted, positive, conjunctive-head
//! TGD fragment. A construct outside it is HARD-FAILED (named), never silently dropped:
//!
//! * a **negated body literal** (`~atom`) — the restricted chase joins only positive body
//!   atoms; dropping the guard would invent witnesses the negation forbids (a wrong
//!   answer), so a negated literal is refused;
//! * an **arithmetic / aggregation / inequality body operation** — the n-ary atom carries
//!   no builtin/guard slot, so an operation term cannot be represented and is refused
//!   rather than dropped;
//! * a **Skolem-function existential** (a non-range-restricted head argument shared with no
//!   other head atom) — inherited from the reified lowering
//!   ([`crate::nary::lower_nary_rules`]), which this parser runs at parse time so the
//!   refusal fires HERE, not at chase time.
//!
//! A **disjunctive head** is not expressible on the Nemo `.rls` surface at all (a rule head
//! is a CONJUNCTION of atoms), so it cannot reach this parser; the conjunctive multi-head
//! shape it does carry is exactly the multi-tuple-inventing TGD the reified lowering wants.
//!
//! # The delimited EDB loader
//!
//! [`load_nary_data_file`] parses one delimited data file (`<rel>.csv` / `<rel>.tsv`,
//! optionally gzip-compressed as `<rel>.csv.gz`) into [`Vec<NaryTuple>`]: the relation is
//! the file stem, the arity is the (uniform) column count, and each row is one n-ary fact.
//! The SAME loaded tuples are handed to BOTH engines, so the EDB representation is a
//! consistent, engine-neutral choice — the reason a cross-engine parity comparison is
//! sound. gzip is handled through the workspace's existing `flate2`; a uniform-arity
//! violation or a malformed record is a HARD FAIL, never a silently truncated row.

use std::path::Path;

use purrdf::TermValue;

use crate::nary::{NaryArg, NaryAtom, NaryRule, NaryTuple};
use crate::provenance::LOGIC_NAMESPACE;
use crate::rule_ir::{EvalTerm, lower_nemo_term};

/// Wrap an n-ary `.rls` / EDB-loader condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn nary_rls_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Engine { detail })
}

// ── `.rls` program parser ──────────────────────────────────────────────────────

/// Parse an n-ary multi-head existential `.rls` PROGRAM into [`Vec<NaryRule>`].
///
/// Reuses the Nemo front-end (arity-preserving, like
/// [`crate::physical::generic::parse_generic_rules`]) and enforces the fragment refusals
/// (see the module doc). The parsed rules are validated through the reified lowering
/// ([`crate::nary::lower_nary_rules`]) at parse time, so a Skolem-function existential (or
/// any other lowering refusal) hard-fails HERE with its named message.
///
/// # Errors
///
/// Returns the Nemo parse error, a per-rule fragment refusal (negation / operation /
/// zero-arity atom), or a reified-lowering refusal (empty head / Skolem-function
/// existential).
pub fn parse_nary_rls_program(rls: &str) -> gmeow_errors::Result<Vec<NaryRule>> {
    use crate::nemo_engine::NemoParsedRules;
    use nemo::rule_model::programs::ProgramRead;

    let program = NemoParsedRules::parse_unvalidated(rls)?.into_program();
    let mut out: Vec<NaryRule> = Vec::new();
    for (rule_index, rule) in program.rules().enumerate() {
        let name = rule
            .name()
            .unwrap_or_else(|| format!("{LOGIC_NAMESPACE}rule/anonymous/{rule_index}"));

        // Positive, guard-free fragment: a negated body literal or a body operation
        // (arithmetic / aggregation / inequality) cannot be carried by an n-ary atom and
        // is refused rather than dropped.
        if rule.body_negative().count() > 0 {
            return Err(nary_rls_err(format!(
                "n-ary .rls rule {name:?} carries a negated body literal (~atom) — the reified \
                 restricted-chase n-ary fragment joins only positive body atoms; refusing rather \
                 than dropping the guard and inventing witnesses it forbids"
            )));
        }
        if rule.body_operations().count() > 0 {
            return Err(nary_rls_err(format!(
                "n-ary .rls rule {name:?} carries a body operation (arithmetic / aggregation / \
                 inequality guard) — a fixed-arity n-ary atom carries no builtin or guard slot, \
                 so the operation is not representable; refusing rather than dropping it"
            )));
        }

        let head: Vec<NaryAtom> = rule
            .head()
            .iter()
            .map(|atom| lower_nary_atom(atom, &name, "head"))
            .collect::<Result<_, _>>()?;
        let body: Vec<NaryAtom> = rule
            .body_positive()
            .map(|atom| lower_nary_atom(atom, &name, "body"))
            .collect::<Result<_, _>>()?;

        out.push(NaryRule { name, body, head });
    }

    // Validate the whole program through the reified lowering so the doctrinal refusals
    // (empty head, zero-arity head atom, Skolem-function existential) fire at PARSE time,
    // not only when the native chase runs. The lowered rules are discarded — this is a
    // structural admissibility check, not the chase.
    crate::nary::lower_nary_rules(&out)?;

    Ok(out)
}

/// Lower one Nemo atom into a full-arity [`NaryAtom`], KEEPING ALL terms (no world slot).
fn lower_nary_atom(
    atom: &nemo::rule_model::components::atom::Atom,
    rule_name: &str,
    site: &str,
) -> gmeow_errors::Result<NaryAtom> {
    let relation = atom.predicate().to_string();
    let mut args: Vec<NaryArg> = Vec::new();
    for term in atom.terms() {
        args.push(nary_arg_from_term(term, rule_name, site, &relation)?);
    }
    if args.is_empty() {
        return Err(nary_rls_err(format!(
            "n-ary .rls rule {rule_name:?} has a zero-arity {site} atom for relation \
             {relation:?} — a fixed-arity n-ary tuple has at least one argument"
        )));
    }
    Ok(NaryAtom { relation, args })
}

/// Lower one Nemo argument term into a [`NaryArg`] via the SAME term codec the binary IR
/// uses ([`lower_nemo_term`], permissive `slot = "object"` — an n-ary argument may be a
/// variable, a constant IRI, or a constant literal in any position).
fn nary_arg_from_term(
    term: &nemo::rule_model::components::term::Term,
    rule_name: &str,
    site: &str,
    relation: &str,
) -> gmeow_errors::Result<NaryArg> {
    match lower_nemo_term(term, "object").map_err(|e| {
        nary_rls_err(format!(
            "n-ary .rls rule {rule_name:?} {site} atom for relation {relation:?} carries an \
             argument the n-ary fragment cannot represent (an arithmetic / aggregate / function \
             term is refused, not mis-lowered): {e}"
        ))
    })? {
        EvalTerm::Var(v) => Ok(NaryArg::Var(v)),
        EvalTerm::ConstNamed(iri) => Ok(NaryArg::Named(iri)),
        EvalTerm::ConstLit(t) => Ok(NaryArg::Lit(t)),
    }
}

// ── Delimited EDB loader ───────────────────────────────────────────────────────

/// Load one delimited n-ary data file into [`Vec<NaryTuple>`].
///
/// The relation is the file stem (with any `.gz` and `.csv`/`.tsv` suffix stripped); the
/// delimiter is `,` for `.csv` and a tab for `.tsv`; a `.gz` suffix is transparently
/// gunzipped through the workspace's `flate2`. Every row must have the SAME arity (the
/// schema is the first row's column count); a differing arity, an empty file, or a
/// malformed record is a HARD FAIL.
///
/// # Errors
///
/// Returns `Err` on an unreadable / undecodable file, an unknown extension, a gzip decode
/// failure, a non-uniform arity, or a file with zero data rows.
pub fn load_nary_data_file(path: &Path) -> gmeow_errors::Result<Vec<NaryTuple>> {
    let name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
        nary_rls_err(format!(
            "n-ary EDB loader: data file {} has no valid UTF-8 name",
            path.display()
        ))
    })?;

    let (relation, delimiter, gzipped) = classify_data_file(name)?;

    let raw = std::fs::read(path).map_err(|e| {
        nary_rls_err(format!(
            "n-ary EDB loader: cannot read {}: {e}",
            path.display()
        ))
    })?;
    let bytes = if gzipped { gunzip(&raw, path)? } else { raw };

    parse_nary_delimited(&relation, &bytes, delimiter)
}

/// Classify a data-file name into `(relation, delimiter, gzipped)`.
///
/// `edge.csv` → `("edge", b',', false)`; `edge.tsv.gz` → `("edge", b'\t', true)`. An
/// unrecognized extension is a hard fail (never a silently-guessed delimiter).
fn classify_data_file(name: &str) -> gmeow_errors::Result<(String, u8, bool)> {
    let (stem_ext, gzipped) = match name.strip_suffix(".gz") {
        Some(inner) => (inner, true),
        None => (name, false),
    };
    let (relation, delimiter) = if let Some(rel) = stem_ext.strip_suffix(".csv") {
        (rel, b',')
    } else if let Some(rel) = stem_ext.strip_suffix(".tsv") {
        (rel, b'\t')
    } else {
        return Err(nary_rls_err(format!(
            "n-ary EDB loader: data file {name:?} has an unrecognized extension — expected \
             <rel>.csv, <rel>.tsv, or a .gz of one (no silent delimiter guess)"
        )));
    };
    if relation.is_empty() {
        return Err(nary_rls_err(format!(
            "n-ary EDB loader: data file {name:?} has an empty relation stem"
        )));
    }
    Ok((relation.to_owned(), delimiter, gzipped))
}

/// Gunzip `raw` (a gzip stream), hard-failing on a decode error.
fn gunzip(raw: &[u8], path: &Path) -> gmeow_errors::Result<Vec<u8>> {
    use std::io::Read;

    let mut decoder = flate2::read::GzDecoder::new(raw);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|e| {
        nary_rls_err(format!(
            "n-ary EDB loader: cannot gunzip {}: {e}",
            path.display()
        ))
    })?;
    Ok(out)
}

/// Parse delimited `bytes` (schema/arity-driven) into `relation`'s n-ary tuples.
///
/// Uses the workspace's `csv` reader (proper quoting/escaping) with `has_headers(false)`
/// and STRICT arity (`flexible(false)`), so a row with the wrong column count is a hard
/// error. The relation is fixed; each record's cells become the ordered argument terms
/// ([`parse_nary_cell`]). A file with zero data rows is refused.
///
/// # Errors
///
/// Returns `Err` on a CSV parse / arity error or an empty tuple set.
pub fn parse_nary_delimited(
    relation: &str,
    bytes: &[u8],
    delimiter: u8,
) -> gmeow_errors::Result<Vec<NaryTuple>> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(false)
        .from_reader(bytes);

    let mut tuples: Vec<NaryTuple> = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| {
            nary_rls_err(format!(
                "n-ary EDB loader: malformed record in relation {relation:?} (non-uniform arity \
                 or bad quoting): {e}"
            ))
        })?;
        let args: Vec<TermValue> = record.iter().map(parse_nary_cell).collect();
        tuples.push(NaryTuple {
            relation: relation.to_owned(),
            args,
        });
    }

    if tuples.is_empty() {
        return Err(nary_rls_err(format!(
            "n-ary EDB loader: relation {relation:?} carries zero data rows — an n-ary EDB \
             relation must have at least one fact (no silently-empty relation)"
        )));
    }
    Ok(tuples)
}

/// Interpret one delimited cell as an engine-neutral [`TermValue`] (a consistent choice
/// shared by BOTH engines, which is what makes the cross-engine parity comparison sound):
///
/// * `<iri>` (angle-bracketed) → the IRI;
/// * a token that already looks absolute (`scheme://…`) → that IRI;
/// * anything else → a simple string literal (the value round-trips through the Nemo term
///   codec unambiguously, unlike a relative-IRI guess).
fn parse_nary_cell(cell: &str) -> TermValue {
    let trimmed = cell.trim();
    if let Some(inner) = trimmed.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        return TermValue::iri(inner.to_owned());
    }
    if trimmed.contains("://") {
        return TermValue::iri(trimmed.to_owned());
    }
    TermValue::simple_literal(trimmed)
}

#[cfg(test)]
mod tests;
