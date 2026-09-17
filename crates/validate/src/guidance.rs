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

#[path = "guidance.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "guidance_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::governing_terms;
#[cfg(test)]
pub(crate) use test_support::term_guidance;
