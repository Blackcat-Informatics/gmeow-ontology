// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Score the real rubric slice with the group-C primitives (linkage, projection,
//! testing, documentation, translation).

use std::collections::BTreeMap;
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

/// The slice's own files as the scorer now consumes them: an in-memory map keyed by
/// slice-relative path, read once off the real slice directory.
fn slice_files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    gmeow_slice_quality::report::slice_files_from_dir(dir).expect("slice files read")
}

fn ctx<'a>(ds: &'a purrdf::RdfDataset, files: &'a BTreeMap<String, Vec<u8>>) -> ScoreContext<'a> {
    ScoreContext::new(
        "https://blackcatinformatics.ca/gmeow/slices/slice-quality-rubric".to_owned(),
        files,
        ds,
        ScoringEnv::Repo {
            slice_dir: slice_dir(),
        },
    )
}

#[test]
fn all_group_c_producers_yield_normalized_scores() {
    let ds = slice_graph();
    let files = slice_files(&slice_dir());
    let c = ctx(&ds, &files);
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
    let files = slice_files(&slice_dir());
    let doc = axes::resolve("documentation_axis").unwrap()(&ctx(&ds, &files));
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
    let files = slice_files(&slice_dir());
    let tr = axes::resolve("translation_axis").unwrap()(&ctx(&ds, &files));
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
/// Returns `(guard, dir, slice_iri)`, where the [`tempfile::TempDir`] guard owns
/// `dir` and removes the whole tree when it drops — on success, on panic, and on
/// early return. Callers must bind it (`let (_tmp, dir, iri) = literal_fixture(…);`);
/// a bare `_` binding would delete the fixture before the axis reads it.
fn literal_fixture(name: &str, translate_example: bool) -> (tempfile::TempDir, PathBuf, String) {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let dir = tmp.path().join(format!("gmeow-xlit-{name}"));
    std::fs::create_dir_all(dir.join("i18n")).unwrap();

    let slice_iri = "https://blackcatinformatics.ca/gmeow/slices/xlit".to_string();
    // A real slice directory declares its identity in a manifest; the fixture is read
    // through the same `slice_files_from_dir` entry point production uses.
    std::fs::write(
        dir.join("manifest.ttl"),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <{slice_iri}> a gmeow:Slice .\n"
        ),
    )
    .unwrap();
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
    (tmp, dir, slice_iri)
}

#[test]
fn translation_denominator_is_every_localizable_literal_not_just_label_and_definition() {
    // A term whose label+definition are FULLY translated in fr+cmn but whose
    // skos:example is not. Under the widened measure this scores strictly < 1.0;
    // under the OLD label+definition-only scope it would be a perfect 1.0. This is the
    // decisive discriminator that the denominator widened past label+definition.
    let (_tmp, dir, iri) = literal_fixture("gap", false);
    let ds = gmeow_slice_quality::dataset_from_paths(&[&dir.join("module.ttl")]).unwrap();
    let tr = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri,
        &slice_files(&dir),
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    ));
    assert!(
        tr.score < 1.0,
        "an untranslated skos:example must drop the score below 1.0 (widened denominator); \
         a label+definition-only measure would wrongly score 1.0, got {}",
        tr.score
    );

    // Control: translate the example too → every localizable literal covered → 1.0.
    let (_tmp2, dir2, iri2) = literal_fixture("full", true);
    let ds2 = gmeow_slice_quality::dataset_from_paths(&[&dir2.join("module.ttl")]).unwrap();
    let tr2 = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri2,
        &slice_files(&dir2),
        &ds2,
        ScoringEnv::Repo {
            slice_dir: dir2.clone(),
        },
    ));
    assert_eq!(
        tr2.score, 1.0,
        "with every localizable literal translated in fr+cmn the score is a perfect 1.0, got {}",
        tr2.score
    );
}

#[test]
fn translation_axis_does_not_credit_copied_english() {
    let (_tmp, dir, iri) = literal_fixture("integrity", false);
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
        &slice_files(&dir),
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
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
}

#[test]
fn mislabeled_catalog_header_cannot_credit_copied_english() {
    // A fr.po / zh.po that LIES in its `Language:` header (claims `en`) while carrying
    // copied English must NOT be integrity-checked as English: coverage is evaluated against
    // the configured target (fr/cmn), so the copy earns no credit, and the mislabeled header
    // is surfaced as an advisory rather than silently trusted. Before this fix, trusting the
    // header would let the copy pass the (English) integrity guard and falsely score 1.0.
    let (_tmp, dir, iri) = literal_fixture("mislabel", false);
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
        // The header LIES: `Language: en` in a {lang}.po carrying copied English.
        std::fs::write(
            dir.join(format!("i18n/{lang}.po")),
            format!(
                "msgid \"\"\nmsgstr \"Language: en\\n\"\n\n\
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
        &slice_files(&dir),
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    ));
    assert_eq!(
        tr.score,
        1.0 / 3.0,
        "a lying `Language: en` header must not let copied English earn fr/cmn credit, got {}",
        tr.score
    );
    assert_eq!(
        tr.findings
            .iter()
            .filter(|f| f.code == "slice-quality.translation.mislabeled-catalog")
            .count(),
        2,
        "each mislabeled catalog (fr.po and zh.po both claiming `en`) is surfaced as an advisory"
    );
}

const XLIT_TERM: &str = "https://blackcatinformatics.ca/gmeow/xlit/Thing";

#[test]
fn fuzzy_entry_does_not_count_toward_coverage() {
    // Control: a fully-reviewed fixture (label+definition+example in fr & zh) is 1.0.
    let (_tmp, dir, iri) = literal_fixture("fuzzygate", true);
    let ds = gmeow_slice_quality::dataset_from_paths(&[&dir.join("module.ttl")]).unwrap();
    let full = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri.clone(),
        &slice_files(&dir),
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    ));
    assert_eq!(
        full.score, 1.0,
        "control: every literal reviewed in fr+cmn → 1.0, got {}",
        full.score
    );

    // Flag the fr rdfs:label entry `#, fuzzy`: machine-seeded/unreviewed, so it must
    // NOT count. The graph is unchanged, so only the on-disk catalog drives the drop.
    let fr = std::fs::read_to_string(dir.join("i18n/fr.po")).unwrap();
    let fuzzed = fr.replace(
        &format!("msgctxt \"{XLIT_TERM}|rdfs:label\""),
        &format!("#, fuzzy\nmsgctxt \"{XLIT_TERM}|rdfs:label\""),
    );
    assert_ne!(
        fr, fuzzed,
        "the fixture must contain the fr label entry to flag"
    );
    std::fs::write(dir.join("i18n/fr.po"), fuzzed).unwrap();

    let gated = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri,
        &slice_files(&dir),
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    ));
    assert!(
        gated.score < full.score,
        "a #, fuzzy entry must not count toward coverage: gated {} vs full {}",
        gated.score,
        full.score
    );
    assert!(
        gated.findings.iter().any(
            |f| f.code == "slice-quality.translation.incomplete" && f.message.contains("fuzzy")
        ),
        "an advisory narrates the machine-seeded (fuzzy) entry awaiting review"
    );
}

#[test]
fn removing_fuzzy_flag_raises_coverage() {
    // Seed the fr label as `#, fuzzy` (machine-seeded, unreviewed) in an otherwise
    // fully-translated fixture; a human then promotes it by deleting the flag.
    let (_tmp, dir, iri) = literal_fixture("fuzzyremove", true);
    let ds = gmeow_slice_quality::dataset_from_paths(&[&dir.join("module.ttl")]).unwrap();

    let fr = std::fs::read_to_string(dir.join("i18n/fr.po")).unwrap();
    let seeded = fr.replace(
        &format!("msgctxt \"{XLIT_TERM}|rdfs:label\""),
        &format!("#, fuzzy\nmsgctxt \"{XLIT_TERM}|rdfs:label\""),
    );
    std::fs::write(dir.join("i18n/fr.po"), &seeded).unwrap();
    let before = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri.clone(),
        &slice_files(&dir),
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    ))
    .score;

    // The single action that moves coverage upward: deleting the `#, fuzzy` flag.
    std::fs::write(dir.join("i18n/fr.po"), seeded.replace("#, fuzzy\n", "")).unwrap();
    let after = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri,
        &slice_files(&dir),
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    ))
    .score;

    assert!(
        after > before,
        "removing the #, fuzzy flag must raise measured coverage: before {before}, after {after}"
    );
    assert_eq!(
        after, 1.0,
        "with every entry reviewed the score is a perfect 1.0, got {after}"
    );
}

#[test]
fn zh_po_without_language_header_still_gets_cmn_integrity() {
    // A zh.po with NO `Language:` header and a copied-English (non-Han) translation:
    // the axis must fall back to the `cmn` tag so the integrity guard still rejects
    // the copied English rather than crediting it as coverage.
    let (_tmp, dir, iri) = literal_fixture("cmnfallback", false);
    std::fs::write(
        dir.join("i18n/zh.po"),
        format!(
            "msgctxt \"{XLIT_TERM}|rdfs:label\"\nmsgid \"L\"\nmsgstr \"Label\"\n\n\
             msgctxt \"{XLIT_TERM}|skos:definition\"\nmsgid \"D\"\nmsgstr \"Definition\"\n"
        ),
    )
    .unwrap();
    let ds = gmeow_slice_quality::dataset_from_paths(&[&dir.join("module.ttl")]).unwrap();
    let tr = axes::resolve("translation_axis").unwrap()(&ScoreContext::new(
        iri,
        &slice_files(&dir),
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    ));
    assert!(
        tr.findings
            .iter()
            .any(|f| f.code == "slice-quality.translation.integrity-rejected"),
        "copied English in a header-less zh.po is rejected via the cmn fallback, not credited"
    );
}
