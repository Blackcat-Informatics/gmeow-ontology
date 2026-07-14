// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The per-term usage-guidance reader (Part 3) — joins a finding onto the
//! `gmeow:howToUse` / `gmeow:useWhen` / `gmeow:avoidWhen` prose authored on
//! ontology terms, from BOTH honest keys:
//!
//! * the finding's RULE's governing term(s) ([`GuidanceIndex::governing_terms`]),
//!   resolved from the bundle's generated constraint-catalog
//!   `gmeow:ValidationRule` nodes (`gmeow:ruleCode` → `logic:formalizes` /
//!   `gmeow:appliesToTerm`); and
//! * the finding's own [`documented_terms`](gmeow_errors::model::Finding::documented_terms)
//!   (the structurally-concerned term(s), e.g. a SHACL `sh:path`).
//!
//! [`GuidanceIndex`] is the one-pass lookup index a caller builds ONCE per
//! report (not per finding): [`GuidanceIndex::build`] scans every dataset in
//! `graphs` a single time, keying both a term → authored-guidance map and a
//! rule-code → governing-term map, so [`GuidanceIndex::governing_terms`] and
//! [`GuidanceIndex::term_guidance`] are then O(1) lookups per finding instead
//! of a fresh full-bundle scan each. Honest absence: a term that authors no
//! modality yields no [`Guidance`] for it, and a code with no governing rule
//! yields no term — never fabricated.

use std::collections::{HashMap, HashSet};

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

/// A stable sort rank for [`GuidanceModality`] (the enum carries no `Ord`).
fn modality_rank(modality: GuidanceModality) -> u8 {
    match modality {
        GuidanceModality::HowToUse => 0,
        GuidanceModality::UseWhen => 1,
        GuidanceModality::AvoidWhen => 2,
    }
}

/// A one-pass lookup index over a report's graphs, built ONCE per report and
/// queried O(1) per finding — replacing the old per-finding full-bundle scans.
///
/// * `term_guidance` is scanned from EVERY dataset in `graphs` (a term authored
///   in either the bundle's documentation graph or the caller's subject graph
///   is found), matching the sort/dedup order the standalone `term_guidance`
///   reader used to produce.
/// * `code_terms` is scanned from `graphs[0]` (the bundle) ONLY — governing
///   terms come from the catalog, which lives in the bundle alone — matching
///   the standalone `governing_terms` reader's bundle-only contract.
pub struct GuidanceIndex {
    /// term IRI -> its authored `(modality, text)` guidance, sorted by
    /// `(modality, text)` and deduped, exactly as the old per-call scan did.
    term_guidance: HashMap<String, Vec<(GuidanceModality, String)>>,
    /// rule code -> its governing term IRIs (from typed `gmeow:ValidationRule`
    /// nodes' `logic:formalizes` / `gmeow:appliesToTerm`), sorted and deduped.
    code_terms: HashMap<String, Vec<String>>,
}

impl GuidanceIndex {
    /// Build both maps from ONE pass over each dataset in `graphs` (bundle
    /// first, then subject/caller graphs). `graphs[0]` MUST be the bundle: it
    /// is the only dataset scanned for the rule-code -> governing-term key.
    pub fn build(graphs: &[&RdfDataset]) -> Self {
        debug_assert!(
            !graphs.is_empty(),
            "GuidanceIndex::build requires at least the bundle dataset"
        );

        let rule_code = format!("{GMEOW}ruleCode");
        let formalizes = format!("{LOGIC}formalizes");
        let applies_to_term = format!("{GMEOW}appliesToTerm");
        let validation_rule_type = format!("{GMEOW}ValidationRule");

        let mut term_guidance: HashMap<String, Vec<(GuidanceModality, String)>> = HashMap::new();
        // Bundle-only temp maps for the rule-code -> governing-term key (see
        // the module doc: only the bundle carries the constraint catalog).
        let mut rule_type: HashSet<String> = HashSet::new();
        let mut rule_code_of: HashMap<String, Vec<String>> = HashMap::new();
        let mut rule_terms_of: HashMap<String, Vec<String>> = HashMap::new();

        for (graph_index, ds) in graphs.iter().enumerate() {
            for q in ds.quads() {
                let (TermRef::Iri(s), TermRef::Iri(p)) = (ds.resolve(q.s), ds.resolve(q.p)) else {
                    continue;
                };

                // Key 1 (all graphs): term -> authored guidance modality/text.
                if let Some((_, modality)) = MODALITY_PREDICATES.iter().find(|(iri, _)| *iri == p) {
                    if let TermRef::Literal { lexical, .. } = ds.resolve(q.o) {
                        term_guidance
                            .entry(s.to_owned())
                            .or_default()
                            .push((*modality, lexical.to_owned()));
                    }
                    continue;
                }

                // Key 2 (bundle only): rule code -> governing term IRIs.
                if graph_index != 0 {
                    continue;
                }
                if p == RDF_TYPE {
                    if let TermRef::Iri(o) = ds.resolve(q.o)
                        && o == validation_rule_type
                    {
                        rule_type.insert(s.to_owned());
                    }
                } else if p == rule_code {
                    if let TermRef::Literal { lexical, .. } = ds.resolve(q.o) {
                        rule_code_of
                            .entry(s.to_owned())
                            .or_default()
                            .push(lexical.to_owned());
                    }
                } else if (p == formalizes || p == applies_to_term)
                    && let TermRef::Iri(o) = ds.resolve(q.o)
                {
                    rule_terms_of
                        .entry(s.to_owned())
                        .or_default()
                        .push(o.to_owned());
                }
            }
        }

        for claims in term_guidance.values_mut() {
            claims.sort_by(|a, b| {
                modality_rank(a.0)
                    .cmp(&modality_rank(b.0))
                    .then_with(|| a.1.cmp(&b.1))
            });
            claims.dedup();
        }

        // Guard against a non-`gmeow:ValidationRule` subject that happens to
        // carry a same-valued `gmeow:ruleCode` literal in an unrelated graph:
        // only subjects typed `gmeow:ValidationRule` contribute their terms,
        // and every one of a code's governing rules' terms is unioned in.
        let mut code_terms: HashMap<String, Vec<String>> = HashMap::new();
        let empty_terms: Vec<String> = Vec::new();
        for rule_iri in &rule_type {
            let Some(codes) = rule_code_of.get(rule_iri) else {
                continue;
            };
            let terms = rule_terms_of.get(rule_iri).unwrap_or(&empty_terms);
            for code in codes {
                code_terms
                    .entry(code.clone())
                    .or_default()
                    .extend(terms.iter().cloned());
            }
        }
        for terms in code_terms.values_mut() {
            terms.sort();
            terms.dedup();
        }

        Self {
            term_guidance,
            code_terms,
        }
    }

    /// The governing term IRIs for a rule code (empty when none). O(1).
    pub fn governing_terms(&self, code: &str) -> &[String] {
        self.code_terms.get(code).map_or(&[], Vec::as_slice)
    }

    /// The authored guidance claims for a term, stamped with `source`/`help_uri`.
    /// Honest absence: no authored modality -> empty. O(1) lookup + small clone.
    pub fn term_guidance(
        &self,
        term_iri: &str,
        source: GuidanceSource,
        help_uri: Option<String>,
    ) -> Vec<Guidance> {
        let Some(claims) = self.term_guidance.get(term_iri) else {
            return Vec::new();
        };
        claims
            .iter()
            .map(|(modality, text)| Guidance {
                modality: *modality,
                source,
                term_iri: term_iri.to_owned(),
                text: text.clone(),
                standpoint: Standpoint::Advisory,
                help_uri: help_uri.clone(),
            })
            .collect()
    }
}

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

    /// Equivalence: the one-pass [`GuidanceIndex`] must produce the exact same
    /// `term_guidance` claims and `governing_terms` set as the ground-truth
    /// per-call scans above, over a small multi-rule fixture (two rules
    /// sharing a term, one rule with no governing term, one undocumented
    /// term) — this is the byte-identical-output guarantee the perf refactor
    /// depends on.
    #[test]
    fn guidance_index_matches_the_ground_truth_scans() {
        let bundle = dataset(&format!(
            r#"{PREFIXES}
            gmeow:rule/discipline-frame-completeness
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:requiresFrame ;
                gmeow:appliesToTerm gmeow:MeasuredQuantity .

            gmeow:rule/discipline-frame-completeness-2
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:requiresUnit .

            gmeow:rule/no-governing-term
                a gmeow:ValidationRule ;
                gmeow:ruleCode "discipline/bare-rule" .

            gmeow:not-a-rule
                gmeow:ruleCode "discipline/frame-completeness" ;
                logic:formalizes gmeow:decoyTerm .

            gmeow:requiresFrame
                gmeow:howToUse "Annotate the frame-carrying class." ;
                gmeow:useWhen "Use when a value is frame-relative." ;
                gmeow:avoidWhen "Avoid on frame-independent values." .

            gmeow:requiresUnit
                gmeow:howToUse "Annotate the unit-carrying class." .
            "#
        ));
        let subject = dataset(&format!(
            r#"{PREFIXES}
            gmeow:requiresFrame
                gmeow:useWhen "Use when a value is frame-relative." ;
                gmeow:avoidWhen "Documented again in the subject graph." .
            "#
        ));

        let graphs: [&RdfDataset; 2] = [bundle.as_ref(), subject.as_ref()];
        let index = GuidanceIndex::build(&graphs);

        for code in [
            "discipline/frame-completeness",
            "discipline/bare-rule",
            "discipline/no-such-rule",
        ] {
            let mut expected = governing_terms(bundle.as_ref(), code);
            expected.sort();
            let mut actual = index.governing_terms(code).to_vec();
            actual.sort();
            assert_eq!(actual, expected, "governing_terms mismatch for {code}");
        }

        for term in [
            "https://blackcatinformatics.ca/gmeow/requiresFrame",
            "https://blackcatinformatics.ca/gmeow/requiresUnit",
            "https://blackcatinformatics.ca/gmeow/decoyTerm",
            "https://blackcatinformatics.ca/gmeow/undocumentedTerm",
        ] {
            let expected = term_guidance(
                &graphs,
                term,
                GuidanceSource::RuleGoverningTerm,
                Some("https://example.test/catalog#anchor".to_owned()),
            );
            let actual = index.term_guidance(
                term,
                GuidanceSource::RuleGoverningTerm,
                Some("https://example.test/catalog#anchor".to_owned()),
            );
            assert_eq!(actual, expected, "term_guidance mismatch for {term}");
        }
    }
}
