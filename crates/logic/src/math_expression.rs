// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `math:` expression-identity reasoned-graph gate.
//!
//! Runs at reasoning speed (`make reason-verify`) over the frozen reasoned graph,
//! alongside [`crate::math_dimension`]'s measure-and-dimension gate and the
//! typed-formalization-governance obligation checks. It recomputes every authored
//! `math:structuralKey` through the ONE `math:` expression lowering
//! ([`crate::physical::lower::math_expression_structural_keys`], itself built on the
//! content-addressed, hash-consed term DAG's [`crate::physical::lower::structural_digest`]),
//! never trusting the authored string as an independent second source, and checks that a
//! `math:NormalizationDeclaration`'s structural-identity computation never leaks a
//! surface-stratum (rendered) predicate.
//!
//! Each violation is a `Severity::Error` [`Finding`] naming the typed `math:` failure
//! class it decides (`math:StructuralKeyDrift`, `math:SurfaceLeakInNormalForm`,
//! `math:StructuralKeyOnRejectedExpression`), so a single such finding hard-fails the
//! gate. This is a plain Rust computation over the frozen reasoned graph — exactly the
//! architectural shape [`crate::math_dimension`] sets — so it is dispatched from
//! `crate::verify` directly and is NOT part of `reason::NATIVE_CONTRACT_COMPONENTS`
//! (it compiles no `EvalRule`; there is no compiled contract to fold it into).

use gmeow_errors::{Finding, Severity};
use gmeow_math::{
    TripleIndex, all_iris, first_iri, first_literal, has_type, index_dataset, subjects,
};
use purrdf::RdfDataset;

/// Namespace root for the `math:` measure-and-dimension vocabulary.
const MATH: &str = "https://blackcatinformatics.ca/math/";

/// The finding code for a drifted `math:structuralKey` (authored value disagrees with
/// the recomputed digest).
const CODE_STRUCTURAL_KEY_DRIFT: &str = "verify.math.structural-key-drift";
/// The finding code for a `math:NormalizationDeclaration` (or a `math:normalizes` /
/// `math:normalizesTo` expression it names) directly carrying a surface-stratum
/// predicate as identity input.
const CODE_SURFACE_LEAK_IN_NORMAL_FORM: &str = "verify.math.surface-leak-in-normal-form";
/// The finding code for an authored `math:structuralKey` on an expression whose
/// lowering the `math:` expression grammar rejects.
const CODE_STRUCTURAL_KEY_ON_REJECTED_EXPRESSION: &str =
    "verify.math.structural-key-on-rejected-expression";

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

fn error(code: &str, message: String) -> Finding {
    let mut finding = Finding::new(Severity::Error, code, message).with_tool("verify");
    finding.tags = vec!["reasoned-graph".to_owned(), "math-expression".to_owned()];
    finding
}

/// Run the `math:` expression-identity reasoned gate over the frozen reasoned graph.
/// Returns one `Severity::Error` [`Finding`] per violation, in deterministic (code,
/// message) order. Never panics: every fallible read is either surfaced as a typed
/// finding or a deliberate skip.
#[must_use]
pub fn check_math_expression_findings(reasoned: &RdfDataset) -> Vec<Finding> {
    let index = index_dataset(reasoned);
    // The ONE `math:` expression lowering, run once per root over this same frozen
    // graph — [`check_structural_key_drift`] and [`check_structural_key_on_rejected_expression`]
    // both read off this shared map rather than each re-lowering the graph.
    let keys = crate::physical::lower::math_expression_structural_keys(reasoned);
    let mut findings = Vec::new();

    check_structural_key_drift(&index, &keys, &mut findings);
    check_structural_key_on_rejected_expression(&index, &keys, &mut findings);
    check_surface_leak_in_normal_form(&index, &mut findings);

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    findings
}

/// `math:structuralKey` drift: an authored digest must equal the recomputed structural
/// digest of the SAME root expression. A subject carrying `math:structuralKey` that is
/// not itself a recognized expression root (not a key of `keys`) is out of scope here —
/// it is not part of the `math:` expression grammar's root population — and a rejected
/// root is surfaced instead by [`check_structural_key_on_rejected_expression`], never
/// double-reported as a drift here.
fn check_structural_key_drift(
    index: &TripleIndex,
    keys: &std::collections::BTreeMap<
        String,
        Result<String, crate::physical::lower::MathLoweringError>,
    >,
    findings: &mut Vec<Finding>,
) {
    let structural_key = math("structuralKey");
    let mut subs: Vec<String> = subjects(index)
        .filter(|s| first_literal(index, s, &structural_key).is_some())
        .cloned()
        .collect();
    subs.sort();
    for subj in subs {
        let Some(authored) = first_literal(index, &subj, &structural_key) else {
            continue;
        };
        let Some(Ok(computed)) = keys.get(&subj) else {
            continue;
        };
        if *computed != authored {
            findings.push(error(
                CODE_STRUCTURAL_KEY_DRIFT,
                format!(
                    "math:StructuralKeyDrift: expression {subj} declares math:structuralKey \
                     \"{authored}\" but its recomputed structural digest is \"{computed}\" — \
                     the key is a computed projection of the expression's own structure, never \
                     an independent authored value"
                ),
            ));
        }
    }
}

/// `math:structuralKey` claimed on a REJECTED expression: an expression whose
/// `math:` expression grammar the lowering refutes (a malformed argument-slot family, an
/// unscoped occurrence, a cyclic or too-deep slot graph, ...) has no structural identity
/// to claim — an authored `math:structuralKey` on it asserts an identity for a thing the
/// lowering says is ill-formed.
fn check_structural_key_on_rejected_expression(
    index: &TripleIndex,
    keys: &std::collections::BTreeMap<
        String,
        Result<String, crate::physical::lower::MathLoweringError>,
    >,
    findings: &mut Vec<Finding>,
) {
    let structural_key = math("structuralKey");
    let mut subs: Vec<String> = subjects(index)
        .filter(|s| first_literal(index, s, &structural_key).is_some())
        .cloned()
        .collect();
    subs.sort();
    for subj in subs {
        if let Some(Err(err)) = keys.get(&subj) {
            findings.push(error(
                CODE_STRUCTURAL_KEY_ON_REJECTED_EXPRESSION,
                format!(
                    "math:StructuralKeyOnRejectedExpression: expression {subj} declares \
                     math:structuralKey but its math: expression lowering rejects it ({err}) — \
                     a structural identity cannot be claimed for an expression the grammar \
                     itself refutes"
                ),
            ));
        }
    }
}

/// `math:SurfaceLeakInNormalForm`: structural-normal-form identity is computed over
/// structural content alone, independent of rendering or notation — mirrors
/// `lang:SurfaceLeakInContentKey`'s syntactic shape one stratum over. Flag a
/// `math:NormalizationDeclaration` that itself directly carries `math:rendersAs`, or
/// whose `math:normalizes` source or `math:normalizesTo` target directly carries
/// `math:rendersAs`.
fn check_surface_leak_in_normal_form(index: &TripleIndex, findings: &mut Vec<Finding>) {
    let renders_as = math("rendersAs");
    let normalizes = math("normalizes");
    let normalizes_to = math("normalizesTo");
    for decl in subjects_of_type(index, &math("NormalizationDeclaration")) {
        let mut culprits: Vec<String> = Vec::new();
        if first_iri(index, &decl, &renders_as).is_some() {
            culprits.push(decl.clone());
        }
        for src in all_iris(index, &decl, &normalizes) {
            if first_iri(index, &src, &renders_as).is_some() {
                culprits.push(src);
            }
        }
        for tgt in all_iris(index, &decl, &normalizes_to) {
            if first_iri(index, &tgt, &renders_as).is_some() {
                culprits.push(tgt);
            }
        }
        culprits.sort();
        culprits.dedup();
        for culprit in culprits {
            findings.push(error(
                CODE_SURFACE_LEAK_IN_NORMAL_FORM,
                format!(
                    "math:SurfaceLeakInNormalForm: {culprit} directly carries \
                     math:rendersAs while participating in normalization declaration \
                     {decl}'s structural-identity computation — normal-form identity is \
                     computed over structural content alone, independent of rendering or \
                     notation, never the rendered surface"
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests;
