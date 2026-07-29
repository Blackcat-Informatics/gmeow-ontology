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
//! `math:StructuralKeyOnRejectedExpression`, `math:MalformedStructuralKey`), so a single
//! such finding hard-fails the gate. This is a plain Rust computation over the frozen
//! reasoned graph — exactly the architectural shape [`crate::math_dimension`] sets — so
//! it is dispatched from `crate::verify` directly and is NOT part of
//! `reason::NATIVE_CONTRACT_COMPONENTS` (it compiles no `EvalRule`; there is no compiled
//! contract to fold it into).
//!
//! Every `math:structuralKey` reader here goes through [`classify_structural_key_usage`]
//! rather than reading "the first literal found": a subject whose `math:structuralKey`
//! usage is two or more values (of any kind) or a single non-literal value is
//! `math:MalformedStructuralKey`, reported ONCE and excluded from the drift/rejected
//! population — never silently collapsed to whichever value an iteration order happens
//! to visit first.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::{Finding, Severity};
use gmeow_math::{
    TripleIndex, all_iris, all_literals_typed, first_iri, has_type, index_dataset, subjects,
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
/// The finding code for a `math:structuralKey` usage that is not a well-formed
/// singleton literal — two or more values (of any kind), or a single non-literal value.
const CODE_MALFORMED_STRUCTURAL_KEY: &str = "verify.math.malformed-structural-key";

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
    let usage = classify_structural_key_usage(&index);
    let mut findings = Vec::new();

    check_malformed_structural_key(&usage, &mut findings);
    check_structural_key_drift(&keys, &usage, &mut findings);
    check_structural_key_on_rejected_expression(&keys, &usage, &mut findings);
    check_surface_leak_in_normal_form(&index, &mut findings);
    report_alpha_equivalence_classes(&keys, &mut findings);

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    findings
}

/// The classified `math:structuralKey` usage shape of every subject that carries at
/// least one `math:structuralKey` triple (of ANY object kind) in the graph — the SHARED
/// population every reader of `math:structuralKey` below draws from, so "is this
/// subject's key a well-formed singleton" is decided in exactly ONE place rather than
/// re-derived (and potentially re-diverged) per caller.
struct StructuralKeyUsage {
    /// Subjects whose `math:structuralKey` usage is a clean singleton literal, mapped to
    /// that literal's lexical value.
    clean: BTreeMap<String, String>,
    /// Subjects whose `math:structuralKey` usage is NOT a clean singleton literal — two
    /// or more values of any kind, or a single non-literal value.
    malformed: BTreeSet<String>,
}

/// Classify every subject's `math:structuralKey` usage: EXACTLY one literal value is
/// `clean`; zero values is absent (out of scope for every check below — an expression
/// that authors no structural-identity claim at all makes none to verify); anything else
/// (two or more values of any kind, or a single non-literal value) is `malformed`. Reads
/// every object of the predicate — literal AND non-literal — so a plural or wrongly-typed
/// key can never be silently reduced to "the first literal found".
fn classify_structural_key_usage(index: &TripleIndex) -> StructuralKeyUsage {
    let structural_key = math("structuralKey");
    let mut clean = BTreeMap::new();
    let mut malformed = BTreeSet::new();
    let mut subs: Vec<&String> = subjects(index).collect();
    subs.sort();
    for subj in subs {
        let literals = all_literals_typed(index, subj, &structural_key);
        let non_literal_count = all_iris(index, subj, &structural_key).len();
        let total = literals.len() + non_literal_count;
        if total == 0 {
            continue;
        }
        if literals.len() == 1 && non_literal_count == 0 {
            clean.insert(subj.clone(), literals[0].0.to_owned());
        } else {
            malformed.insert(subj.clone());
        }
    }
    StructuralKeyUsage { clean, malformed }
}

/// `math:MalformedStructuralKey`: a subject's `math:structuralKey` usage is not a
/// well-formed singleton literal. Reported ONCE per subject here and excluded from
/// [`check_structural_key_drift`]/[`check_structural_key_on_rejected_expression`]'s
/// population — a plural or non-literal key has no single value either of those checks
/// could compare, so it is never ALSO silently read as "the first value found".
fn check_malformed_structural_key(usage: &StructuralKeyUsage, findings: &mut Vec<Finding>) {
    for subj in &usage.malformed {
        findings.push(error(
            CODE_MALFORMED_STRUCTURAL_KEY,
            format!(
                "math:MalformedStructuralKey: expression {subj} carries a math:structuralKey \
                 that is not a well-formed singleton literal — two or more asserted values (of \
                 any kind), or a single non-literal value, can never be safely read as \"the \
                 first value found\" without silently masking a contradictory or ill-typed \
                 second value"
            ),
        ));
    }
}

/// `math:structuralKey` drift: an authored digest must equal the recomputed structural
/// digest of the SAME root expression. A subject carrying `math:structuralKey` that is
/// not itself a recognized expression root (not a key of `keys`) is out of scope here —
/// it is not part of the `math:` expression grammar's root population — and a rejected
/// root is surfaced instead by [`check_structural_key_on_rejected_expression`], never
/// double-reported as a drift here. Reads only [`StructuralKeyUsage::clean`] subjects — a
/// malformed (plural/non-literal) key is [`check_malformed_structural_key`]'s population,
/// never silently compared here against whichever value happened to be first.
fn check_structural_key_drift(
    keys: &std::collections::BTreeMap<
        String,
        Result<String, crate::physical::lower::MathLoweringError>,
    >,
    usage: &StructuralKeyUsage,
    findings: &mut Vec<Finding>,
) {
    for (subj, authored) in &usage.clean {
        let Some(Ok(computed)) = keys.get(subj) else {
            continue;
        };
        if computed != authored {
            let alpha_iri = crate::physical::lower::alpha_class_iri_for_digest(computed);
            let mut finding = error(
                CODE_STRUCTURAL_KEY_DRIFT,
                format!(
                    "math:StructuralKeyDrift: expression {subj} declares math:structuralKey \
                     \"{authored}\" but its recomputed structural digest is \"{computed}\" — \
                     the key is a computed projection of the expression's own structure, never \
                     an independent authored value; its α-equivalence class is {alpha_iri}"
                ),
            );
            // The α-equivalence-class IRI is a genuinely IRI-typed term this finding's
            // evidence binds (`Finding::cited_iris`), not merely rendered prose: two
            // α-equivalent expressions' drift findings cite the SAME individual, so a
            // consumer can JOIN on it rather than string-compare digests out of `message`.
            finding.cited_iris = vec![alpha_iri];
            findings.push(finding);
        }
    }
}

/// `math:structuralKey` claimed on a REJECTED expression: an expression whose
/// `math:` expression grammar the lowering refutes (a malformed argument-slot family, an
/// unscoped occurrence, a cyclic or too-deep slot graph, ...) has no structural identity
/// to claim — an authored `math:structuralKey` on it asserts an identity for a thing the
/// lowering says is ill-formed. Reads only [`StructuralKeyUsage::clean`] subjects, for the
/// same reason [`check_structural_key_drift`] does.
fn check_structural_key_on_rejected_expression(
    keys: &std::collections::BTreeMap<
        String,
        Result<String, crate::physical::lower::MathLoweringError>,
    >,
    usage: &StructuralKeyUsage,
    findings: &mut Vec<Finding>,
) {
    for subj in usage.clean.keys() {
        if let Some(Err(err)) = keys.get(subj) {
            // The rejection's OWN typed `math:` failure class (`err.failure_class()`) is
            // routed into the message as a second `math:<LocalName>: ` token, alongside this
            // gate's own `math:StructuralKeyOnRejectedExpression` token — never collapsed to
            // the error's untyped `Display` prose alone. This is the ONE production emitter
            // of [`crate::physical::lower::MathLoweringError::failure_class`]: without it the
            // typed rejection algebra decides a class in Rust and then discards it, exactly
            // the defect this gate exists to abolish.
            let class_local = failure_class_local_name(err.failure_class());
            findings.push(error(
                CODE_STRUCTURAL_KEY_ON_REJECTED_EXPRESSION,
                format!(
                    "math:StructuralKeyOnRejectedExpression: expression {subj} declares \
                     math:structuralKey but its math: expression lowering rejects it as \
                     math:{class_local}: {err} — a structural identity cannot be claimed for \
                     an expression the grammar itself refutes"
                ),
            ));
        }
    }
}

/// The local name of a full `math:` failure-class IRI (the substring after the last `/`),
/// used ONLY to render the `math:<LocalName>: ` message token
/// [`crate::physical::lower::MathLoweringError::failure_class`] decides — the same
/// `<prefix>:<Class>:` convention `crates/validate/src/lint.rs`'s native structural lint
/// messages use, which the `math:` conformance-completeness harness
/// (`crates/pipeline/tests/math_conformance_discharge.rs`) scans for verbatim.
fn failure_class_local_name(iri: &str) -> &str {
    iri.rsplit('/').next().unwrap_or(iri)
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

/// Surface the α-equivalence class of every expression the lowering ACCEPTS, as a note.
///
/// Without this the identity edge reaches a consumer only on the DRIFT branch, through a
/// failure's `cited_iris` — so two WRONG expressions could be joined and two RIGHT ones could
/// not, which is backwards for an identity. The materialized edge lives in the reasoned graph
/// this gate reads, and the reasoned graph is internal; a note is how a CLI consumer sees it.
///
/// The message deliberately does NOT open with a `math:<Class>: ` token. That prefix is the
/// native channel's convention for "this finding reports FAILURE class X", and the conformance
/// harness scans source for it to build the reachable-class set — an identity class carrying it
/// registers as a phantom failure class, which is exactly what the harness reported when this
/// note was first written that way.
///
/// It is also the gate's only POSITIVE verdict. "No findings" and "nothing to decide" are the
/// same observation from outside, so a silent population is indistinguishable from a healthy
/// one; one note per decided root makes the population countable.
fn report_alpha_equivalence_classes(
    keys: &std::collections::BTreeMap<
        String,
        Result<String, crate::physical::lower::MathLoweringError>,
    >,
    findings: &mut Vec<Finding>,
) {
    for (root, keyed) in keys {
        let Ok(digest) = keyed else { continue };
        let alpha_iri = crate::physical::lower::alpha_class_iri_for_digest(digest);
        let mut finding = Finding::new(
            Severity::Note,
            "verify.math.alpha-equivalence-class",
            format!(
                "expression {root} resolves to alpha-equivalence class {alpha_iri} — two \
                 expressions identical up to bound-variable renaming and symbol occurrence share \
                 this node, so a consumer joins on it rather than string-comparing digests"
            ),
        )
        .with_tool("verify");
        finding.cited_iris = vec![alpha_iri];
        findings.push(finding);
    }
}
