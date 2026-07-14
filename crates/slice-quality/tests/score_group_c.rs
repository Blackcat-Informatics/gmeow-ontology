// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Score the real rubric slice with the group-C primitives (linkage, projection,
//! testing, documentation, translation).

use std::path::{Path, PathBuf};

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::{ScoreContext, ScoringEnv};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn slice_dir() -> PathBuf {
    repo_root().join("slices/core/slice-quality-rubric")
}

fn slice_graph() -> std::sync::Arc<purrdf::RdfDataset> {
    // The crate's SINGLE path-collection authority (module + examples/ + tests/), so
    // the test graph matches the graph the sweep actually scores.
    let paths = gmeow_slice_quality::report::slice_ttl_paths(&slice_dir());
    let refs: Vec<&Path> = paths.iter().map(PathBuf::as_path).collect();
    gmeow_slice_quality::dataset_from_paths(&refs).unwrap()
}

fn ctx(ds: &purrdf::RdfDataset) -> ScoreContext<'_> {
    ScoreContext::new(
        "https://blackcatinformatics.ca/gmeow/slices/slice-quality-rubric".to_owned(),
        slice_dir(),
        ds,
        ScoringEnv::Repo,
    )
}

#[test]
fn all_group_c_producers_yield_normalized_scores() {
    let ds = slice_graph();
    let c = ctx(&ds);
    for producer in [
        "linkage_axis",
        "projection_axis",
        "testing_axis",
        "documentation_axis",
        "translation_axis",
    ] {
        let r = axes::resolve(producer).unwrap()(&c);
        assert!(
            (0.0..=1.0).contains(&r.score),
            "{producer} → {} not in 0..=1",
            r.score
        );
    }
}

#[test]
fn documentation_thesis_is_present() {
    let ds = slice_graph();
    let doc = axes::resolve("documentation_axis").unwrap()(&ctx(&ds));
    assert_eq!(
        doc.score, 1.0,
        "the rubric slice ships a narrative docs.md thesis"
    );
}

#[test]
fn translation_reflects_the_missing_mandarin_catalog_honestly() {
    // The slice ships fr.po but not (yet) zh.po, so translation must NOT be a
    // perfect 1.0 — the axis reports the real gap rather than smoothing it over.
    let ds = slice_graph();
    let tr = axes::resolve("translation_axis").unwrap()(&ctx(&ds));
    assert!(
        tr.score < 1.0,
        "translation must reflect the missing Mandarin catalog, got {}",
        tr.score
    );
    assert!(
        tr.score > 0.3,
        "English + French are present, so it is not near zero"
    );
    assert!(
        tr.findings.iter().any(|f| f.message.contains("cmn")),
        "an advisory names the missing Mandarin (cmn) coverage"
    );
}

/// Build a throwaway slice dir with one term carrying `rdfs:label`,
/// `skos:definition`, and `skos:example`, plus fr/zh catalogs that translate
/// `label`+`definition` and — only when `translate_example` — the `example` too.
/// Returns `(dir, slice_iri)`.
fn literal_fixture(name: &str, translate_example: bool) -> (PathBuf, String) {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("gmeow-xlit-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(dir.join("i18n")).unwrap();

    let slice_iri = "https://blackcatinformatics.ca/gmeow/slices/xlit".to_string();
    let term = "https://blackcatinformatics.ca/gmeow/xlit/Thing";
    std::fs::write(
        dir.join("module.ttl"),
        format!(
            "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             <{term}> a owl:Class ;\n\
                 rdfs:isDefinedBy <{slice_iri}> ;\n\
                 rdfs:label \"L\"@en ;\n\
                 skos:definition \"D\"@en ;\n\
                 skos:example \"E\"@en .\n"
        ),
    )
    .unwrap();

    for lang in ["fr", "zh"] {
        let (label, definition, example_value) = match lang {
            "fr" => ("Étiquette", "Définition", "ex:chose a ex:Chose ."),
            "zh" => ("标签", "定义", "示例：ex:thing a ex:Thing ."),
            _ => unreachable!(),
        };
        let example = if translate_example {
            format!("\nmsgctxt \"{term}|skos:example\"\nmsgid \"E\"\nmsgstr \"{example_value}\"\n")
        } else {
            String::new()
        };
        std::fs::write(
            dir.join(format!("i18n/{lang}.po")),
            format!(
                "msgid \"\"\nmsgstr \"Language: {lang}\\n\"\n\
                 \nmsgctxt \"{term}|rdfs:label\"\nmsgid \"L\"\nmsgstr \"{label}\"\n\
                 \nmsgctxt \"{term}|skos:definition\"\nmsgid \"D\"\nmsgstr \"{definition}\"\n{example}"
            ),
        )
        .unwrap();
    }
    (dir, slice_iri)
}

#[test]
fn translation_denominator_is_every_localizable_literal_not_just_label_and_definition() {
    // A term whose label+definition are FULLY translated in fr+cmn but whose
    // skos:example is not. Under the widened measure this scores strictly < 1.0;
    // under the OLD label+definition-only scope it would be a perfect 1.0. This is the
    // decisive discriminator that the denominator widened past label+definition.
    let (dir, iri) = literal_fixture("gap", false);
    let ds = gmeow_slice_quality::dataset_from_paths(&[&dir.join("module.ttl")]).unwrap();
    let tr = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri,
        dir.clone(),
        &ds,
        ScoringEnv::Repo,
    ));
    assert!(
        tr.score < 1.0,
        "an untranslated skos:example must drop the score below 1.0 (widened denominator); \
         a label+definition-only measure would wrongly score 1.0, got {}",
        tr.score
    );

    // Control: translate the example too → every localizable literal covered → 1.0.
    let (dir2, iri2) = literal_fixture("full", true);
    let ds2 = gmeow_slice_quality::dataset_from_paths(&[&dir2.join("module.ttl")]).unwrap();
    let tr2 = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri2,
        dir2.clone(),
        &ds2,
        ScoringEnv::Repo,
    ));
    assert_eq!(
        tr2.score, 1.0,
        "with every localizable literal translated in fr+cmn the score is a perfect 1.0, got {}",
        tr2.score
    );

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&dir2).ok();
}

#[test]
fn translation_axis_does_not_credit_copied_english() {
    let (dir, iri) = literal_fixture("integrity", false);
    let term = "https://blackcatinformatics.ca/gmeow/xlit/Thing";
    std::fs::write(
        dir.join("module.ttl"),
        format!(
            "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             <{term}> a owl:Class ;\n\
                 rdfs:isDefinedBy <{iri}> ;\n\
                 rdfs:label \"Lifecycle state\"@en .\n"
        ),
    )
    .unwrap();
    for lang in ["fr", "zh"] {
        std::fs::write(
            dir.join(format!("i18n/{lang}.po")),
            format!(
                "msgid \"\"\nmsgstr \"Language: {lang}\\n\"\n\n\
                 msgctxt \"{term}|rdfs:label\"\n\
                 msgid \"Lifecycle state\"\n\
                 msgstr \"Lifecycle state\"\n"
            ),
        )
        .unwrap();
    }

    let ds = gmeow_slice_quality::dataset_from_paths(&[&dir.join("module.ttl")]).unwrap();
    let tr = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri,
        dir.clone(),
        &ds,
        ScoringEnv::Repo,
    ));
    assert_eq!(
        tr.score,
        1.0 / 3.0,
        "copied English must earn no fr/cmn credit"
    );
    assert_eq!(
        tr.findings
            .iter()
            .filter(|finding| finding.code == "slice-quality.translation.integrity-rejected")
            .count(),
        2
    );
    std::fs::remove_dir_all(dir).ok();
}
