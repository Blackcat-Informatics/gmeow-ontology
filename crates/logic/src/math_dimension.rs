// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `math:` measure-and-dimension reasoned-graph gate.
//!
//! Runs at reasoning speed (`make reason-verify`) over the frozen reasoned graph,
//! alongside the typed-formalization-governance obligation checks. It computes
//! `math:dimensionVector` string drift, zero-denominator exponent malformation, and
//! the positive-definiteness of every authored `math:GramMatrix` used as a metric
//! form — all THROUGH the one exact-rational (ℚ⁷) source [`gmeow_math::dimension`]
//! and the exact-rational inner-product engine [`gmeow_math::InnerProductSpace`],
//! never asserted data.
//!
//! Each violation is a `Severity::Error` [`Finding`] naming the typed `math:`
//! failure class it decides (`math:MalformedDimension`, `math:AsymmetricGramMatrix`,
//! `math:NonPositiveDefiniteNorm`), so a single such finding hard-fails the gate.
//! Dimensional homogeneity and integral dimensional composition are no longer
//! decided here — the reasoner materializes `math:DimensionalInhomogeneity`
//! directly from the authored `math:dimensionalHomogeneityLaw` /
//! `math:integralDimensionCompositionLaw` `logic:Formula` ASTs, surfaced as a
//! `verify.dimensional-inhomogeneity` finding by the production `verify()` entrypoint
//! (see `crates/logic/tests/dimension_gate.rs`); this Rust sweep would be a
//! redundant second source of truth for that law.

use gmeow_errors::{Finding, Severity};
use gmeow_math::dimension::{DimVector, load_dimension_vector};
use gmeow_math::{
    IndexOutOfRange, InnerProductSpace, Rational, TripleIndex, first_iri, first_literal, has_type,
    index_dataset, load_gram, subjects,
};
use purrdf::RdfDataset;
use std::collections::BTreeMap;

/// Namespace root for the `math:` measure-and-dimension vocabulary.
const MATH: &str = "https://blackcatinformatics.ca/math/";

/// The finding code for a malformed dimension (drift / zero-denominator power).
const CODE_MALFORMED: &str = "verify.math.malformed-dimension";
/// The finding code for a non-positive-definite authored Gram / metric form.
const CODE_NON_PD: &str = "verify.math.non-positive-definite-norm";
/// The finding code for an authored Gram matrix that is not symmetric.
const CODE_ASYMMETRIC: &str = "verify.math.asymmetric-gram-matrix";

fn math(local: &str) -> String {
    format!("{MATH}{local}")
}

/// The IRIs of every subject typed `class`, sorted for deterministic iteration.
fn subjects_of_type(index: &TripleIndex, class: &str) -> Vec<String> {
    let mut out: Vec<String> = subjects(index)
        .filter(|s| has_type(index, s, class))
        .cloned()
        .collect();
    out.sort();
    out
}

/// The exact-rational ℚ⁷ exponent vector of a dimension IRI, or `None` when its
/// structure is ill-formed (a non-base target, a zero-denominator power, arithmetic
/// overflow) or its kind cannot be computed. A `None` is a deliberate skip — the
/// zero-denominator scan and the SHACL `DimensionExponentShape` surface the malformed
/// structure explicitly, so it is never a silent fail-open here.
fn dim_vector(index: &TripleIndex, dim_iri: &str) -> Option<DimVector> {
    load_dimension_vector(index, dim_iri).ok()
}

fn error(code: &str, message: String) -> Finding {
    let mut finding = Finding::new(Severity::Error, code, message).with_tool("verify");
    finding.tags = vec!["reasoned-graph".to_owned(), "math-dimension".to_owned()];
    finding
}

/// Run the `math:` measure-and-dimension reasoned gate over the frozen reasoned
/// graph. Returns one `Severity::Error` [`Finding`] per violation, in deterministic
/// (code, message) order. Never panics: every fallible read is either surfaced as a
/// typed finding or a deliberate skip.
#[must_use]
pub fn check_math_dimension_findings(reasoned: &RdfDataset) -> Vec<Finding> {
    let index = index_dataset(reasoned);
    let mut findings = Vec::new();

    check_dimension_vector_drift(&index, &mut findings);
    check_zero_denominator_exponents(&index, &mut findings);
    check_gram_positive_definiteness(&index, &mut findings);

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    findings
}

/// `math:dimensionVector` string drift: an authored string must equal the canonical
/// render of the structured exponents (the string is a computed projection, never a
/// divergent second source). A subject whose structure is ill-formed is skipped here
/// (surfaced by the zero-denominator scan / SHACL), never silently mismatched.
fn check_dimension_vector_drift(index: &TripleIndex, findings: &mut Vec<Finding>) {
    let dimension_vector = math("dimensionVector");
    let mut dims: Vec<String> = subjects(index)
        .filter(|s| first_literal(index, s, &dimension_vector).is_some())
        .cloned()
        .collect();
    dims.sort();
    for subj in dims {
        let Some(lexical) = first_literal(index, &subj, &dimension_vector) else {
            continue;
        };
        let Some(vec) = dim_vector(index, &subj) else {
            continue;
        };
        let canonical = vec.render();
        if canonical != lexical {
            findings.push(error(
                CODE_MALFORMED,
                format!(
                    "math:MalformedDimension: dimension {subj} declares math:dimensionVector \
                     \"{lexical}\" but its structured exponents render to \"{canonical}\" — the \
                     string is a computed projection, not an independent source"
                ),
            ));
        }
    }
}

/// Zero-denominator exponent: an exact-rational power needs a non-zero denominator.
/// [`dim_vector`] returns `None` on such a cell (which the homogeneity / composition
/// loops would then skip), so surface it here as `math:MalformedDimension` — a
/// malformed power hard-fails rather than failing open.
fn check_zero_denominator_exponents(index: &TripleIndex, findings: &mut Vec<Finding>) {
    let denom = math("exponentDenominator");
    for cell in subjects_of_type(index, &math("DimensionExponent")) {
        if first_literal(index, &cell, &denom)
            .map(|l| l.trim().parse::<i128>() == Ok(0))
            .unwrap_or(false)
        {
            findings.push(error(
                CODE_MALFORMED,
                format!(
                    "math:MalformedDimension: dimension-exponent cell {cell} declares \
                     math:exponentDenominator 0 — an exact-rational power needs a non-zero \
                     denominator; the cell is ill-formed"
                ),
            ));
        }
    }
}

/// The first off-diagonal cell `(i, j)` (with `i < j`) whose explicitly-authored
/// transpose mate `(j, i)` carries a DIFFERENT exact-rational value — the witness
/// that a declared Gram matrix is not symmetric — returned as `(i, j, v_ij, v_ji)`.
///
/// An un-authored transpose mate is NOT an asymmetry: authoring only the upper (or
/// lower) triangle is the ordinary idiom, and the caller's symmetric fill mirrors a
/// lone cell across the diagonal. Only two conflicting explicit entries witness a
/// genuine non-symmetric authoring. Returns `None` when the matrix is symmetric.
fn first_asymmetric_cell(
    cells: &[(usize, usize, Rational)],
) -> Option<(usize, usize, Rational, Rational)> {
    let mut authored: BTreeMap<(usize, usize), Rational> = BTreeMap::new();
    for &(row, col, value) in cells {
        authored.insert((row, col), value);
    }
    for (&(row, col), value) in &authored {
        if row >= col {
            continue;
        }
        if let Some(mate) = authored.get(&(col, row))
            && mate != value
        {
            return Some((row, col, *value, *mate));
        }
    }
    None
}

/// Symmetry + positive-definiteness of every authored `math:GramMatrix`.
///
/// A Gram matrix is the coordinate matrix of a *symmetric* bilinear form, so it must
/// equal its own transpose. Two explicitly-authored transpose mates `(i,j)` and
/// `(j,i)` carrying different values contradict that and raise
/// `math:AsymmetricGramMatrix` (an un-authored mate is the ordinary upper-triangle
/// authoring idiom, mirrored by the symmetric fill, never an asymmetry). This runs
/// FIRST: the LDLᵀ factorization below assumes symmetry, so an asymmetric matrix is
/// never handed to it (the finding is raised and the matrix skipped).
///
/// Then, every Gram used as a metric form — one carrying `math:definiteness
/// math:positiveDefinite`, or `math:representsForm` a form that does — must be
/// positive-definite, certified by the exact-rational LDLᵀ factorization
/// ([`InnerProductSpace::ldlt_pivots`], all pivots `> 0` by Sylvester's criterion). A
/// non-PD such form raises `math:NonPositiveDefiniteNorm`. This is the sole
/// positive-definiteness enforcement point; the runtime distance builtin trusts it.
/// SHACL/Datalog cannot compute an LDLᵀ factorization, so the certificate is
/// necessarily native.
fn check_gram_positive_definiteness(index: &TripleIndex, findings: &mut Vec<Finding>) {
    let definiteness = math("definiteness");
    let positive_definite = math("positiveDefinite");
    let represents_form = math("representsForm");
    for gram in subjects_of_type(index, &math("GramMatrix")) {
        // Load the exact-rational cells. `load_gram` bounds each authored
        // `math:atRow`/`math:atColumn` to `[0, MAX_BASIS_DIM)`, so an out-of-range index
        // (`math:atRow "1000000000"`) hard-fails HERE rather than sizing a `dim`×`dim`
        // dense matrix and aborting. An out-of-range index is NOT a cardinality fault the
        // shapes catch — it is a `math:MalformedDimension` this gate must surface, never a
        // silent skip. A genuinely-structural malformation (missing cells / properties) is
        // caught by the cardinality shapes, so it stays a skip.
        let cells = match load_gram(index, &gram) {
            Ok(cells) => cells,
            Err(diag) if diag.is::<IndexOutOfRange>() => {
                findings.push(error(
                    CODE_MALFORMED,
                    format!(
                        "math:MalformedDimension: Gram matrix {gram} authors a matrix index \
                         beyond the supported metric-form order ({}); a positive-definiteness \
                         certificate is not materialized for an out-of-range form",
                        diag.message()
                    ),
                ));
                continue;
            }
            Err(_) => continue,
        };
        // Symmetry FIRST — the LDLᵀ certificate below assumes a symmetric matrix, so
        // an asymmetric Gram must be caught and skipped before it reaches the factor.
        if let Some((i, j, vij, vji)) = first_asymmetric_cell(&cells) {
            findings.push(error(
                CODE_ASYMMETRIC,
                format!(
                    "math:AsymmetricGramMatrix: Gram matrix {gram} authors entry \
                     ({i},{j}) = {} but its transpose mate ({j},{i}) = {} differs — a \
                     Gram matrix of a symmetric bilinear form must equal its transpose",
                    vij.ratio_string(),
                    vji.ratio_string()
                ),
            ));
            continue;
        }
        let self_pd =
            first_iri(index, &gram, &definiteness).as_deref() == Some(positive_definite.as_str());
        let form_pd = first_iri(index, &gram, &represents_form)
            .map(|form| {
                first_iri(index, &form, &definiteness).as_deref()
                    == Some(positive_definite.as_str())
            })
            .unwrap_or(false);
        if !(self_pd || form_pd) {
            // A Gram not claimed positive-definite may legitimately be indefinite
            // (e.g. a metric of Lorentzian signature); it is out of scope here.
            continue;
        }
        // Fill the declared symmetric dense matrix from the (now-verified symmetric)
        // cells.
        let Some(dim) = cells
            .iter()
            .flat_map(|(r, c, _)| [*r, *c])
            .max()
            .map(|m| m + 1)
        else {
            continue;
        };
        let mut matrix = vec![vec![Rational::zero(); dim]; dim];
        for (row, col, value) in cells {
            matrix[row][col] = value;
            matrix[col][row] = value; // declared symmetric fill
        }
        let Ok(space) = InnerProductSpace::new(matrix) else {
            continue;
        };
        if let Err(err) = space.ldlt_pivots() {
            findings.push(error(
                CODE_NON_PD,
                format!(
                    "math:NonPositiveDefiniteNorm: Gram matrix {gram} is authored as a \
                     positive-definite metric form but its exact-rational LDLᵀ factorization \
                     refutes positive-definiteness ({}); a norm √(xᵀGx) resting on it is \
                     ill-formed",
                    err.message()
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests;
