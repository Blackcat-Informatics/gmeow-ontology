// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `math:` measure-and-dimension reasoned-graph gate.
//!
//! Runs at reasoning speed (`make reason-verify`) over the frozen reasoned graph,
//! alongside the typed-formalization-governance obligation checks. It computes
//! dimensional homogeneity, integral dimensional composition, `math:dimensionVector`
//! string drift, and the positive-definiteness of every authored `math:GramMatrix`
//! used as a metric form — all THROUGH the one exact-rational (ℚ⁷) source
//! [`gmeow_math::dimension`] and the exact-rational inner-product engine
//! [`gmeow_math::InnerProductSpace`], never asserted data.
//!
//! Each violation is a `Severity::Error` [`Finding`] naming the typed `math:`
//! failure class it decides (`math:DimensionalInhomogeneity`,
//! `math:MalformedDimension`, `math:NonPositiveDefiniteNorm`), so a single such
//! finding hard-fails the gate. It is the executable lowering of
//! `math:dimensionalHomogeneityLaw`, `math:integralDimensionCompositionLaw`, and the
//! Gram positive-definiteness constraint authored in the `math:` slice: the laws are
//! the source, this gate the enforcement.

use gmeow_errors::{Finding, Severity};
use gmeow_math::dimension::{DimVector, load_dimension_vector, node_dimension};
use gmeow_math::{
    InnerProductSpace, Rational, TripleIndex, all_iris, first_iri, first_literal, has_type,
    index_dataset, load_gram, subjects,
};
use purrdf::RdfDataset;

/// Namespace root for the `math:` measure-and-dimension vocabulary.
const MATH: &str = "https://blackcatinformatics.ca/math/";

/// The finding code for a dimensional-homogeneity / integral-composition violation.
const CODE_INHOMOGENEITY: &str = "verify.math.dimensional-inhomogeneity";
/// The finding code for a malformed dimension (drift / zero-denominator power).
const CODE_MALFORMED: &str = "verify.math.malformed-dimension";
/// The finding code for a non-positive-definite authored Gram / metric form.
const CODE_NON_PD: &str = "verify.math.non-positive-definite-norm";

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
    check_homogeneity(&index, &mut findings);
    check_integral_composition(&index, &mut findings);
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

/// Homogeneity: every `math:homogeneousOperand` of a `math:DimensionalExpression`
/// shares one ℚ⁷ dimension. An undimensioned operand, or two or more distinct
/// carried dimensions, raises `math:DimensionalInhomogeneity`.
fn check_homogeneity(index: &TripleIndex, findings: &mut Vec<Finding>) {
    let homogeneous_operand = math("homogeneousOperand");
    for expr in subjects_of_type(index, &math("DimensionalExpression")) {
        let mut operands = all_iris(index, &expr, &homogeneous_operand);
        operands.sort();
        let mut seen: Vec<(DimVector, String)> = Vec::new();
        let mut undimensioned: Vec<String> = Vec::new();
        for operand in operands {
            let Some(dim_iri) = node_dimension(index, &operand) else {
                undimensioned.push(operand);
                continue;
            };
            let Some(vec) = dim_vector(index, &dim_iri) else {
                continue;
            };
            if !seen.iter().any(|(v, _)| *v == vec) {
                seen.push((vec, dim_iri));
            }
        }
        if !undimensioned.is_empty() {
            findings.push(error(
                CODE_INHOMOGENEITY,
                format!(
                    "math:DimensionalInhomogeneity: dimensional expression {expr} combines \
                     undimensioned operand(s) [{}] — every math:homogeneousOperand must carry a \
                     math:hasDimension to be shown homogeneous",
                    undimensioned.join(", ")
                ),
            ));
        }
        if seen.len() >= 2 {
            let mut dims: Vec<String> = seen.into_iter().map(|(_, d)| d).collect();
            dims.sort();
            findings.push(error(
                CODE_INHOMOGENEITY,
                format!(
                    "math:DimensionalInhomogeneity: dimensional expression {expr} combines operands \
                     of differing dimensions [{}]",
                    dims.join(", ")
                ),
            ));
        }
    }
}

/// Integral composition: `dim(result) == dim(integrand) ⊕ dim(measure)` (exponent-
/// vector addition). An undimensioned part or a mismatch raises
/// `math:DimensionalInhomogeneity`.
fn check_integral_composition(index: &TripleIndex, findings: &mut Vec<Finding>) {
    for integral in subjects_of_type(index, &math("Integral")) {
        let Some(result_dim) = node_dimension(index, &integral) else {
            continue;
        };
        let integrand = first_iri(index, &integral, &math("integrand"));
        let measure = first_iri(index, &integral, &math("withRespectTo"));
        let (Some(integrand), Some(measure)) = (integrand, measure) else {
            // Missing integrand/measure is math:IncompleteIntegral (SHACL IntegralShape).
            continue;
        };
        let (Some(idim), Some(mdim)) = (
            node_dimension(index, &integrand),
            node_dimension(index, &measure),
        ) else {
            findings.push(error(
                CODE_INHOMOGENEITY,
                format!(
                    "math:DimensionalInhomogeneity: integral {integral} declares result dimension \
                     {result_dim} but its integrand ({integrand}) or measure ({measure}) carries no \
                     math:hasDimension, so the composition cannot be verified"
                ),
            ));
            continue;
        };
        let (Some(rv), Some(iv), Some(mv)) = (
            dim_vector(index, &result_dim),
            dim_vector(index, &idim),
            dim_vector(index, &mdim),
        ) else {
            continue;
        };
        let Ok(composed) = iv.add(&mv) else {
            continue;
        };
        if rv != composed {
            findings.push(error(
                CODE_INHOMOGENEITY,
                format!(
                    "math:DimensionalInhomogeneity: integral {integral} declares result dimension \
                     {result_dim} but its integrand ({idim}) and measure ({mdim}) compose to a \
                     different dimension"
                ),
            ));
        }
    }
}

/// Positive-definiteness: every authored `math:GramMatrix` used as a metric form —
/// one carrying `math:definiteness math:positiveDefinite`, or `math:representsForm` a
/// form that does — must be positive-definite, certified by the exact-rational LDLᵀ
/// factorization ([`InnerProductSpace::ldlt_pivots`], all pivots `> 0` by Sylvester's
/// criterion). A non-PD such form raises `math:NonPositiveDefiniteNorm`. This is the
/// sole positive-definiteness enforcement point; the runtime distance builtin trusts
/// it. SHACL/Datalog cannot compute an LDLᵀ factorization, so the certificate is
/// necessarily native.
fn check_gram_positive_definiteness(index: &TripleIndex, findings: &mut Vec<Finding>) {
    let definiteness = math("definiteness");
    let positive_definite = math("positiveDefinite");
    let represents_form = math("representsForm");
    for gram in subjects_of_type(index, &math("GramMatrix")) {
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
        // Load the exact-rational cells and fill the declared symmetric dense matrix.
        // A Gram that cannot be loaded (missing cells/indices) is a structural
        // malformation the cardinality shapes catch, not a definiteness verdict; skip.
        let Ok(cells) = load_gram(index, &gram) else {
            continue;
        };
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
