// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

/// Read a term's authored guidance (`howToUse`/`useWhen`/`avoidWhen`) from the
/// given `graphs`, scanning every dataset so a term authored in either the
/// bundle's documentation graph or the caller's subject graph is found.
///
/// Test-only ground-truth reference implementation: production
/// ([`crate::enrich`]) builds a [`GuidanceIndex`] once per report and calls
/// [`GuidanceIndex::term_guidance`] instead — a fresh per-finding scan like
/// this one is the O(findings × bundle) regression this module's `build` was
/// added to eliminate.
#[cfg(test)]
pub(crate) fn term_guidance(
    graphs: &[&RdfDataset],
    term_iri: &str,
    source: GuidanceSource,
    help_uri: Option<String>,
) -> Vec<Guidance> {
    let mut claims: Vec<Guidance> = Vec::new();
    for ds in graphs {
        for q in ds.quads() {
            let (TermRef::Iri(s), TermRef::Iri(p)) = (ds.resolve(q.s), ds.resolve(q.p)) else {
                continue;
            };
            if s != term_iri {
                continue;
            }
            let Some((_, modality)) = MODALITY_PREDICATES.iter().find(|(iri, _)| *iri == p) else {
                continue;
            };
            let TermRef::Literal { lexical, .. } = ds.resolve(q.o) else {
                continue;
            };
            claims.push(Guidance {
                modality: *modality,
                source,
                term_iri: term_iri.to_owned(),
                text: lexical.to_owned(),
                standpoint: Standpoint::Advisory,
                help_uri: help_uri.clone(),
            });
        }
    }
    claims.sort_by(|a, b| {
        modality_rank(a.modality)
            .cmp(&modality_rank(b.modality))
            .then_with(|| a.text.cmp(&b.text))
    });
    claims
        .dedup_by(|a, b| a.modality == b.modality && a.term_iri == b.term_iri && a.text == b.text);
    claims
}

/// Resolve a validation rule's governing term(s): find the bundle's
/// `gmeow:ValidationRule` node whose `gmeow:ruleCode` literal equals `code`
/// (the constraint-catalog projection, `crates/pipeline/src/stages/constraint_catalog.rs`),
/// then collect its `logic:formalizes` / `gmeow:appliesToTerm` IRI objects.
///
/// Sorted and deduped. Empty when the bundle carries no rule for `code`, or the
/// rule resolves no governing term (an honest absence, e.g. a rule the catalog
/// has not enriched from the graph) — never fabricated.
///
/// Test-only ground-truth reference implementation: see [`term_guidance`]'s
/// doc comment above — production uses [`GuidanceIndex::governing_terms`].
#[cfg(test)]
pub(crate) fn governing_terms(bundle: &RdfDataset, code: &str) -> Vec<String> {
    let rule_code = format!("{GMEOW}ruleCode");
    let formalizes = format!("{LOGIC}formalizes");
    let applies_to_term = format!("{GMEOW}appliesToTerm");
    let validation_rule_type = format!("{GMEOW}ValidationRule");

    // First pass: the `gmeow:ValidationRule` subjects whose `gmeow:ruleCode`
    // literal equals `code`.
    let mut rule_subjects: Vec<String> = Vec::new();
    for q in bundle.quads() {
        let (TermRef::Iri(s), TermRef::Iri(p)) = (bundle.resolve(q.s), bundle.resolve(q.p)) else {
            continue;
        };
        if p != rule_code {
            continue;
        }
        let TermRef::Literal { lexical, .. } = bundle.resolve(q.o) else {
            continue;
        };
        if lexical == code {
            rule_subjects.push(s.to_owned());
        }
    }
    if rule_subjects.is_empty() {
        return Vec::new();
    }
    rule_subjects.sort();
    rule_subjects.dedup();

    // Guard against a non-`gmeow:ValidationRule` subject that happens to carry a
    // same-valued `gmeow:ruleCode` literal in an unrelated graph: require the type.
    rule_subjects.retain(|rule_iri| {
        bundle.quads().any(|q| {
            let (TermRef::Iri(s), TermRef::Iri(p), TermRef::Iri(o)) = (
                bundle.resolve(q.s),
                bundle.resolve(q.p),
                bundle.resolve(q.o),
            ) else {
                return false;
            };
            s == rule_iri && p == RDF_TYPE && o == validation_rule_type
        })
    });

    // Second pass: each governing rule subject's `logic:formalizes` /
    // `gmeow:appliesToTerm` IRI objects.
    let mut terms: Vec<String> = Vec::new();
    for q in bundle.quads() {
        let (TermRef::Iri(s), TermRef::Iri(p), TermRef::Iri(o)) = (
            bundle.resolve(q.s),
            bundle.resolve(q.p),
            bundle.resolve(q.o),
        ) else {
            continue;
        };
        if !rule_subjects.iter().any(|r| r == s) {
            continue;
        }
        if p == formalizes || p == applies_to_term {
            terms.push(o.to_owned());
        }
    }
    terms.sort();
    terms.dedup();
    terms
}
