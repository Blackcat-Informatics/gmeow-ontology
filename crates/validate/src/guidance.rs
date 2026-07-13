// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The per-term usage-guidance reader (Part 3) — joins a finding onto the
//! `gmeow:howToUse` / `gmeow:useWhen` / `gmeow:avoidWhen` prose authored on
//! ontology terms, from BOTH honest keys:
//!
//! * the finding's RULE's governing term(s) ([`governing_terms`]), resolved from
//!   the bundle's generated constraint-catalog `gmeow:ValidationRule` nodes
//!   (`gmeow:ruleCode` → `logic:formalizes` / `gmeow:appliesToTerm`); and
//! * the finding's own [`documented_terms`](gmeow_errors::model::Finding::documented_terms)
//!   (the structurally-concerned term(s), e.g. a SHACL `sh:path`).
//!
//! [`term_guidance`] scans the term's authored guidance directly off an
//! [`RdfDataset`] — mirroring the manual quad-scan
//! [`gmeow_logic::certificate::ContradictionPolicy::resolve_from_dataset`] uses to
//! read a declared facet off a dataset, rather than going through an owned
//! query-wrapper (this reader only ever holds a *borrowed* dataset). Honest
//! absence: a term that authors no modality yields no [`Guidance`] for it — never
//! fabricated.

use gmeow_errors::{Guidance, GuidanceModality, GuidanceSource, Standpoint};
use purrdf::{RdfDataset, TermRef};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

const HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/howToUse";
const USE_WHEN: &str = "https://blackcatinformatics.ca/gmeow/useWhen";
const AVOID_WHEN: &str = "https://blackcatinformatics.ca/gmeow/avoidWhen";

/// The predicate → modality table this reader recognises, in the deterministic
/// order the DSL vocabulary declares them.
const MODALITY_PREDICATES: &[(&str, GuidanceModality)] = &[
    (HOW_TO_USE, GuidanceModality::HowToUse),
    (USE_WHEN, GuidanceModality::UseWhen),
    (AVOID_WHEN, GuidanceModality::AvoidWhen),
];

/// Read a term's authored guidance (`howToUse`/`useWhen`/`avoidWhen`) from the
/// given `graphs`, scanning every dataset so a term authored in either the
/// bundle's documentation graph or the caller's subject graph is found.
///
/// Every claim is stamped with `source` (which key resolved the term) and
/// `help_uri` (the term's doc/catalog page, or `None` when no natural mapping
/// exists — never fabricated). Honest absence: a term with no authored modality
/// yields an empty vec. Deterministic: sorted by `(modality, text)` and deduped
/// (the same modality/text pair read off two graphs collapses to one claim).
pub fn term_guidance(
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

/// A stable sort rank for [`GuidanceModality`] (the enum carries no `Ord`).
fn modality_rank(modality: GuidanceModality) -> u8 {
    match modality {
        GuidanceModality::HowToUse => 0,
        GuidanceModality::UseWhen => 1,
        GuidanceModality::AvoidWhen => 2,
    }
}

/// Resolve a validation rule's governing term(s): find the bundle's
/// `gmeow:ValidationRule` node whose `gmeow:ruleCode` literal equals `code`
/// (the constraint-catalog projection, `crates/pipeline/src/stages/constraint_catalog.rs`),
/// then collect its `logic:formalizes` / `gmeow:appliesToTerm` IRI objects.
///
/// Sorted and deduped. Empty when the bundle carries no rule for `code`, or the
/// rule resolves no governing term (an honest absence, e.g. a rule the catalog
/// has not enriched from the graph) — never fabricated.
pub fn governing_terms(bundle: &RdfDataset, code: &str) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn dataset(turtle: &str) -> std::sync::Arc<RdfDataset> {
        purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None)
            .expect("turtle fixture parses")
    }

    const PREFIXES: &str = r#"
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
    "#;

    #[test]
    fn term_guidance_reads_every_authored_modality() {
        let ds = dataset(&format!(
            r#"{PREFIXES}
            gmeow:requiresFrame
                gmeow:howToUse "Annotate the frame-carrying class." ;
                gmeow:useWhen "Use when a value is frame-relative." ;
                gmeow:avoidWhen "Avoid on frame-independent values." .
            "#
        ));
        let claims = term_guidance(
            &[ds.as_ref()],
            "https://blackcatinformatics.ca/gmeow/requiresFrame",
            GuidanceSource::RuleGoverningTerm,
            Some("https://example.test/catalog#frame".to_owned()),
        );
        assert_eq!(
            claims.len(),
            3,
            "all three modalities must be read: {claims:?}"
        );
        assert!(
            claims
                .iter()
                .all(|c| c.source == GuidanceSource::RuleGoverningTerm)
        );
        assert!(claims.iter().all(|c| c.standpoint == Standpoint::Advisory));
        assert!(
            claims
                .iter()
                .all(|c| c.help_uri.as_deref() == Some("https://example.test/catalog#frame"))
        );
        let modalities: Vec<_> = claims.iter().map(|c| c.modality).collect();
        assert!(modalities.contains(&GuidanceModality::HowToUse));
        assert!(modalities.contains(&GuidanceModality::UseWhen));
        assert!(modalities.contains(&GuidanceModality::AvoidWhen));
    }

    #[test]
    fn term_guidance_is_honest_absence_for_an_undocumented_term() {
        let ds = dataset(&format!(
            r#"{PREFIXES}
            gmeow:someOtherTerm
                gmeow:howToUse "Unrelated guidance." .
            "#
        ));
        let claims = term_guidance(
            &[ds.as_ref()],
            "https://blackcatinformatics.ca/gmeow/undocumentedTerm",
            GuidanceSource::DocumentedTerm,
            None,
        );
        assert!(
            claims.is_empty(),
            "a term with no authored guidance must yield no fabricated claim: {claims:?}"
        );
    }

    #[test]
    fn governing_terms_resolves_a_known_rule_code() {
        let ds = dataset(&format!(
            r#"{PREFIXES}
            gmeow:rule/discipline-frame-completeness
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:requiresFrame ;
                gmeow:appliesToTerm gmeow:MeasuredQuantity .
            "#
        ));
        let mut terms = governing_terms(ds.as_ref(), "discipline/frame-completeness");
        terms.sort();
        assert_eq!(
            terms,
            vec![
                "https://blackcatinformatics.ca/gmeow/MeasuredQuantity".to_owned(),
                "https://blackcatinformatics.ca/gmeow/requiresFrame".to_owned(),
            ]
        );
    }

    #[test]
    fn governing_terms_is_empty_for_an_unknown_code() {
        let ds = dataset(&format!(
            r#"{PREFIXES}
            gmeow:rule/discipline-frame-completeness
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:requiresFrame .
            "#
        ));
        assert!(governing_terms(ds.as_ref(), "discipline/no-such-rule").is_empty());
    }
}
