// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Spec ↔ executable agreement gate for the glossary terminology-consistency invariant
//! (Principle 7: the two enforcement paths cannot silently drift).
//!
//! `lang:GlossaryTermConsistencyConstraint` (the `logic:Constraint` SPEC, conformance-tested
//! over the three glossary twins by the slicetest harness) and
//! `gmeow_validate::distinctiveness::distinctiveness_violations` (the EXECUTABLE form the
//! `make i18n-lint` gate runs over the `.po` substrate) must render the SAME verdict on the
//! SAME twins. This test runs the executable detector over the very fixtures the constraint
//! governs — BOTH keyed on `gmeow:glossaryConcept` (as the constraint groups) and keyed on
//! the English source skeleton with the declared-homograph escape (as the `.po` lint groups)
//! — and asserts both agree with each fixture's declared conformance outcome.

use super::super::glossary::Triple;
use purrdf::TermValue;

use gmeow_validate::distinctiveness::{distinctiveness_violations, skeleton};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LANG: &str = "https://blackcatinformatics.ca/lang/";

/// Read exact native source coordinates; the independent detectors below own their judgments.
fn triples(fixture: &str) -> &'static [Triple] {
    type Observations =
        std::collections::BTreeMap<String, Result<Vec<Triple>, gmeow_errors::RecordedDiag>>;
    static OBSERVATIONS: std::sync::OnceLock<
        Result<super::source_artifact::Selected<Observations>, gmeow_errors::Diag>,
    > = std::sync::OnceLock::new();
    super::source_artifact::get(&OBSERVATIONS, super::super::glossary::CHANNEL)
        .get(fixture)
        .expect("selected glossary fixture")
        .as_ref()
        .expect("well-formed source")
}

fn literal(object: &TermValue) -> Option<String> {
    match object {
        TermValue::Literal { lexical_form, .. } => Some(lexical_form.clone()),
        _ => None,
    }
}

fn iri(object: &TermValue) -> Option<String> {
    match object {
        TermValue::Iri(iri) => Some(iri.clone()),
        _ => None,
    }
}

/// The executable detector's verdict, keyed on `gmeow:glossaryConcept` — the SAME grouping
/// the `logic:Constraint` uses. A concept bearing ≥2 distinct translation skeletons is a
/// violation.
fn concept_keyed_violation(triples: &[Triple]) -> bool {
    let concept_p = format!("{GMEOW}glossaryConcept");
    let xlat_p = format!("{GMEOW}glossaryTranslation");
    let concept: std::collections::BTreeMap<String, String> = triples
        .iter()
        .filter(|Triple(_, p, _)| *p == concept_p)
        .filter_map(|Triple(s, _, o)| iri(o).map(|c| (s.clone(), c)))
        .collect();
    let feed = triples
        .iter()
        .filter(|Triple(_, p, _)| *p == xlat_p)
        .filter_map(|Triple(s, _, o)| {
            let c = concept.get(s)?.clone();
            let t = skeleton(&literal(o)?);
            // (msgid=translation, msgstr=concept, key=entry): groups by concept, flags ≥2 xlats.
            Some((t, c, s.clone()))
        });
    !distinctiveness_violations(feed).is_empty()
}

/// The executable detector's verdict, keyed on the English source skeleton with the
/// declared-homograph escape — the SAME logic the `make i18n-lint` gate runs over `.po`.
fn source_keyed_violation(triples: &[Triple]) -> bool {
    let source_p = format!("{GMEOW}glossarySource");
    let xlat_p = format!("{GMEOW}glossaryTranslation");
    let hg_p = format!("{LANG}homographSource");
    let homographs: std::collections::BTreeSet<String> = triples
        .iter()
        .filter(|Triple(_, p, _)| *p == hg_p)
        .filter_map(|Triple(_, _, o)| literal(o).map(|l| skeleton(&l)))
        .collect();
    let source: std::collections::BTreeMap<String, String> = triples
        .iter()
        .filter(|Triple(_, p, _)| *p == source_p)
        .filter_map(|Triple(s, _, o)| literal(o).map(|l| (s.clone(), skeleton(&l))))
        .collect();
    let feed = triples
        .iter()
        .filter(|Triple(_, p, _)| *p == xlat_p)
        .filter_map(|Triple(s, _, o)| {
            let src = source.get(s)?.clone();
            if homographs.contains(&src) {
                return None; // the ontology-resident escape
            }
            let t = skeleton(&literal(o)?);
            Some((t, src, s.clone()))
        });
    !distinctiveness_violations(feed).is_empty()
}

/// Every entry's `gmeow:glossarySense` must be typed `lang:Sense` and `lang:evokes` the
/// SAME `lang:LexicalConcept` the entry carries on `gmeow:glossaryConcept` — the OntoLex
/// Frege triangle the projection now emits (two senses evoking one concept is synonymy,
/// derived not asserted). The consistency invariant still groups on the concept, so the
/// sense edges never change the verdict; this locks that the twins model that structure.
fn sense_evokes_entry_concept(triples: &[Triple]) -> bool {
    let sense_p = format!("{GMEOW}glossarySense");
    let concept_p = format!("{GMEOW}glossaryConcept");
    let evokes_p = format!("{LANG}evokes");
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_string();
    let sense_ty = format!("{LANG}Sense");
    // Native IRI identity is shared by source subjects and objects.
    let clean = |o: &TermValue| iri(o);
    let entry_concept: std::collections::BTreeMap<String, String> = triples
        .iter()
        .filter(|Triple(_, p, _)| *p == concept_p)
        .filter_map(|Triple(s, _, o)| clean(o).map(|c| (s.clone(), c)))
        .collect();
    let entry_sense: std::collections::BTreeMap<String, String> = triples
        .iter()
        .filter(|Triple(_, p, _)| *p == sense_p)
        .filter_map(|Triple(s, _, o)| clean(o).map(|snse| (s.clone(), snse)))
        .collect();
    let is_typed_sense = |node: &str| {
        triples.iter().any(|Triple(s, p, o)| {
            s == node && *p == rdf_type && clean(o).as_deref() == Some(sense_ty.as_str())
        })
    };
    let sense_evokes = |node: &str, concept: &str| {
        triples.iter().any(|Triple(s, p, o)| {
            s == node && *p == evokes_p && clean(o).as_deref() == Some(concept)
        })
    };
    // Every entry that carries a concept must carry a sense of type lang:Sense that
    // lang:evokes that exact concept.
    entry_concept.iter().all(|(entry, concept)| {
        entry_sense
            .get(entry)
            .is_some_and(|sense| is_typed_sense(sense) && sense_evokes(sense, concept))
    })
}

#[test]
fn detector_agrees_with_constraint_on_glossary_twins() {
    // (fixture, expected-violation) — the SAME outcomes example-conformance.ttl declares to
    // the slicetest harness for the derived logic:Constraint SHACL.
    let cases = [
        (
            "slices/grounding/lang/tests/conformance-fixtures/glossary-term-consistent.ttl",
            false,
        ),
        (
            "slices/grounding/lang/tests/counter-examples/glossary-term-inconsistent.ttl",
            true,
        ),
        (
            "slices/grounding/lang/tests/conformance-fixtures/glossary-declared-homograph.ttl",
            false,
        ),
    ];
    for (fixture, expected) in cases {
        let t = triples(fixture);
        assert!(
            !t.is_empty(),
            "fixture {fixture} parsed to no triples — path/format wrong"
        );
        assert_eq!(
            concept_keyed_violation(t),
            expected,
            "concept-keyed detector must match the constraint verdict for {fixture}"
        );
        assert_eq!(
            source_keyed_violation(t),
            expected,
            "source-keyed (.po-lint) detector must match the constraint verdict for {fixture}"
        );
        assert!(
            sense_evokes_entry_concept(t),
            "every entry in {fixture} must carry a lang:Sense that lang:evokes its \
             gmeow:glossaryConcept (the Frege triangle the projection emits)"
        );
    }
}
