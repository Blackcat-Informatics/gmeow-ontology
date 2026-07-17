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

use std::path::{Path, PathBuf};

use gmeow_validate::distinctiveness::{distinctiveness_violations, skeleton};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LANG: &str = "https://blackcatinformatics.ca/lang/";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Parse a fixture Turtle file to N-Triples (Rust-native, no rdflib) and return each
/// `(subject, predicate, object-token)` triple. The object token is a raw `<iri>` or a
/// `"literal"` form; grouping only needs its identity, so escaping is irrelevant.
fn triples(fixture: &str) -> Vec<(String, String, String)> {
    let ttl = std::fs::read(repo_root().join(fixture)).expect("read fixture");
    let ds = purrdf::parse_dataset(&ttl, "text/turtle", None).expect("parse fixture");
    let nt = purrdf::serialize_dataset(
        &ds,
        "application/n-triples",
        purrdf::SerializeGraph::Dataset,
    )
    .expect("serialize NT");
    let nt = String::from_utf8(nt).expect("utf8");
    let mut out = Vec::new();
    for line in nt.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_suffix(" .") else {
            continue;
        };
        // Subject and predicate are always `<iri>`; the object is the remainder.
        let Some(rest) = rest.strip_prefix('<') else {
            continue;
        };
        let Some((subject, rest)) = rest.split_once("> ") else {
            continue;
        };
        let Some(rest) = rest.strip_prefix('<') else {
            continue;
        };
        let Some((predicate, object)) = rest.split_once("> ") else {
            continue;
        };
        out.push((
            subject.to_string(),
            predicate.to_string(),
            object.to_string(),
        ));
    }
    out
}

/// The literal lexical value of an object token `"lex"` / `"lex"@tag` / `"lex"^^<dt>`.
fn literal(object: &str) -> Option<String> {
    let inner = object.strip_prefix('"')?;
    // The lexical form ends at the last unescaped closing quote before an optional
    // `@lang` / `^^<datatype>`. The fixture literals contain no embedded quotes.
    let end = inner.rfind('"')?;
    Some(inner[..end].to_string())
}

/// The IRI of an object token `<iri>` (the trailing `>` was consumed by the splitter).
fn iri(object: &str) -> Option<String> {
    object.strip_suffix('>').map(str::to_string)
}

/// The executable detector's verdict, keyed on `gmeow:glossaryConcept` — the SAME grouping
/// the `logic:Constraint` uses. A concept bearing ≥2 distinct translation skeletons is a
/// violation.
fn concept_keyed_violation(triples: &[(String, String, String)]) -> bool {
    let concept_p = format!("{GMEOW}glossaryConcept");
    let xlat_p = format!("{GMEOW}glossaryTranslation");
    let concept: std::collections::BTreeMap<String, String> = triples
        .iter()
        .filter(|(_, p, _)| *p == concept_p)
        .filter_map(|(s, _, o)| iri(o).map(|c| (s.clone(), c)))
        .collect();
    let feed = triples
        .iter()
        .filter(|(_, p, _)| *p == xlat_p)
        .filter_map(|(s, _, o)| {
            let c = concept.get(s)?.clone();
            let t = skeleton(&literal(o)?);
            // (msgid=translation, msgstr=concept, key=entry): groups by concept, flags ≥2 xlats.
            Some((t, c, s.clone()))
        });
    !distinctiveness_violations(feed).is_empty()
}

/// The executable detector's verdict, keyed on the English source skeleton with the
/// declared-homograph escape — the SAME logic the `make i18n-lint` gate runs over `.po`.
fn source_keyed_violation(triples: &[(String, String, String)]) -> bool {
    let source_p = format!("{GMEOW}glossarySource");
    let xlat_p = format!("{GMEOW}glossaryTranslation");
    let hg_p = format!("{LANG}homographSource");
    let homographs: std::collections::BTreeSet<String> = triples
        .iter()
        .filter(|(_, p, _)| *p == hg_p)
        .filter_map(|(_, _, o)| literal(o).map(|l| skeleton(&l)))
        .collect();
    let source: std::collections::BTreeMap<String, String> = triples
        .iter()
        .filter(|(_, p, _)| *p == source_p)
        .filter_map(|(s, _, o)| literal(o).map(|l| (s.clone(), skeleton(&l))))
        .collect();
    let feed = triples
        .iter()
        .filter(|(_, p, _)| *p == xlat_p)
        .filter_map(|(s, _, o)| {
            let src = source.get(s)?.clone();
            if homographs.contains(&src) {
                return None; // the ontology-resident escape
            }
            let t = skeleton(&literal(o)?);
            Some((t, src, s.clone()))
        });
    !distinctiveness_violations(feed).is_empty()
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
            concept_keyed_violation(&t),
            expected,
            "concept-keyed detector must match the constraint verdict for {fixture}"
        );
        assert_eq!(
            source_keyed_violation(&t),
            expected,
            "source-keyed (.po-lint) detector must match the constraint verdict for {fixture}"
        );
    }
}
