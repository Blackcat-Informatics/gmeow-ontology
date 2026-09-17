// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn parse_po_reads_fuzzy_and_language() {
    let text = "msgid \"\"\nmsgstr \"\"\n\"Language: fr\\n\"\n\n#, fuzzy\nmsgctxt \"x|rdfs:label\"\nmsgid \"A\"\nmsgstr \"B\"\n";
    assert_eq!(language_from_po(text).unwrap(), Some("fr".to_owned()));
    let entries = parse_po(text, true).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].fuzzy);
    assert_eq!(entries[0].msgstr, "B");
}

#[test]
fn live_translation_target_gates_fuzzy_seeds() {
    // The shared fuzzy-gating policy consumed by both pipeline corpus builders: a
    // reviewed entry contributes its msgstr; a machine-seeded `#, fuzzy` entry
    // contributes NO live target (English fallback), byte-identical to untranslated.
    let reviewed = PoEntry {
        msgctxt: "x|rdfs:label".to_owned(),
        msgid: "A".to_owned(),
        msgstr: "B".to_owned(),
        fuzzy: false,
    };
    assert_eq!(live_translation_target(&reviewed), "B");
    let seeded = PoEntry {
        fuzzy: true,
        ..reviewed
    };
    assert_eq!(
        live_translation_target(&seeded),
        "",
        "a #, fuzzy seed contributes no live target to the shipped bundle"
    );
}

/// Non-ASCII authored Turtle does not crash the byte-walking scanners.
///
/// Both scanners advance a byte cursor and then slice `text[i..]`. A byte step through
/// a multi-byte codepoint leaves the cursor INSIDE it, and the next slice panics with
/// `byte index N is not a char boundary`. Every source below carries a character that
/// reproduced exactly that — a `é`, an em dash, and CJK — in the positions the
/// scanners walk: inside a literal, between statements, and inside a comment.
#[test]
fn the_turtle_scanners_survive_non_ascii_sources() {
    const SOURCES: [&str; 6] = [
        // Non-ASCII OUTSIDE any literal or comment — here inside an IRI, which the
        // cursor walks one step at a time between its quote probes. This is the source
        // that reaches the scanners' own fall-through step.
        "@prefix ex: <http://\u{4f8b}/> .\nex:a rdfs:label \"plain\"@x-gmeow-english .\n",
        // Non-ASCII in a BARE token: the tokenizer's name predicates are ASCII-only, so
        // the prefixed-name reader stops at the accent and the cursor falls through on
        // the character itself — the tokenizer's own fall-through step.
        "@prefix ex: <http://ex/> .\nex:na\u{ef}ve a ex:Thing .\n\
             ex:a rdfs:label \"plain\"@x-gmeow-english .\n",
        // Non-ASCII inside a single-quoted English literal.
        "@prefix ex: <http://ex/> .\nex:a rdfs:label \"café — 日本語\"@x-gmeow-english .\n",
        // Non-ASCII inside a triple-quoted literal.
        "@prefix ex: <http://ex/> .\nex:a skos:definition \"\"\"Ünicode — ok\"\"\"@x-gmeow-english .\n",
        // Non-ASCII OUTSIDE any literal: in a comment, which the cursor walks byte by
        // byte before it ever reaches a quote.
        "# a comment with é and — in it\n@prefix ex: <http://ex/> .\nex:a rdfs:label \"plain\"@x-gmeow-english .\n",
        // A backslash-escaped non-ASCII character: the escape skip must step over one
        // whole codepoint, not one byte.
        "@prefix ex: <http://ex/> .\nex:a rdfs:label \"esc \\é done\"@x-gmeow-english .\n",
    ];
    for source in SOURCES {
        // The candidate scanner…
        let candidates = english_literal_candidates(source);
        for candidate in &candidates {
            // Every recorded span must be a valid slice of the source, or the caller
            // (`replace_literal_in_text`) panics on the rewrite instead of the scan.
            assert!(
                source.is_char_boundary(candidate.start) && source.is_char_boundary(candidate.end),
                "candidate span {}..{} is not on codepoint boundaries of {source:?}",
                candidate.start,
                candidate.end
            );
            let _ = &source[candidate.start..candidate.end];
        }
        // …and the tokenizer, which walks the same cursor.
        let _ = tokenize_turtle(source, source.len());
    }

    // Non-vacuity: the scanner really does FIND the non-ASCII English literals, so a
    // scanner that silently returned nothing could not pass this test.
    let found = english_literal_candidates(SOURCES[2]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].decoded, "café — 日本語");
}

#[test]
fn markdown_extract_uses_stable_hash() {
    let entries = extract_markdown_text("# Title\n\nBody.", "README.md");
    assert_eq!(entries.len(), 2);
    assert!(entries[0].msgctxt.starts_with("README.md|"));
    assert_eq!(entries[0].msgid, "# Title");
}

#[test]
fn csv_escape_quotes_commas() {
    assert_eq!(csv_escape("a,b"), "\"a,b\"");
}

#[test]
fn turtle_replace_disambiguates_by_subject_and_predicate() {
    let source = r#"@prefix ex: <http://example.org/> .

ex:s ex:p "shared"@x-gmeow-english ;
    ex:q "shared"@x-gmeow-english .
"#;
    let updated = replace_literal_in_text(
        source,
        "http://example.org/s",
        "http://example.org/p",
        "shared",
        "changed",
    )
    .unwrap();
    assert!(updated.contains(r#"ex:p "changed"@x-gmeow-english"#));
    assert!(updated.contains(r#"ex:q "shared"@x-gmeow-english"#));
}

#[test]
fn turtle_replace_preserves_triple_quoted_style() {
    let source = r#"@prefix ex: <http://example.org/> .

ex:s ex:p """old value"""@x-gmeow-english .
"#;
    let updated = replace_literal_in_text(
        source,
        "http://example.org/s",
        "http://example.org/p",
        "old value",
        "new value",
    )
    .unwrap();
    assert!(updated.contains(r#""""new value"""@x-gmeow-english"#));
}

#[test]
fn turtle_sync_reports_source_already_at_po_value_as_unchanged() {
    let source = r#"@prefix ex: <http://example.org/> .

ex:s ex:p "hand-edited value"@x-gmeow-english .
"#;
    let po = r#"msgctxt "http://example.org/s|http://example.org/p"
msgid "old value"
msgstr "hand-edited value"
"#;
    let report = sync_turtle(
        Path::new("test.po"),
        Path::new("module.ttl"),
        po,
        source,
        true,
    )
    .unwrap();
    assert!(report.changed_files.is_empty());
    assert!(report.conflicts.is_empty());
    assert!(report.skipped.is_empty());
    assert_eq!(
        report.unchanged,
        vec!["http://example.org/s|http://example.org/p"]
    );
}

#[test]
fn turtle_sync_conflicts_when_source_and_po_both_changed() {
    let source = r#"@prefix ex: <http://example.org/> .

ex:s ex:p "current value"@x-gmeow-english .
"#;
    let po = r#"msgctxt "http://example.org/s|http://example.org/p"
msgid "old value"
msgstr "proposed value"
"#;
    let report = sync_turtle(
        Path::new("test.po"),
        Path::new("module.ttl"),
        po,
        source,
        true,
    )
    .unwrap();
    assert!(report.changed_files.is_empty());
    assert_eq!(report.conflicts.len(), 1);
    assert!(report.conflicts[0].contains("source and PO both changed"));
}

/// A fresh, empty repo root for one test, owned by the returned
/// [`tempfile::TempDir`] so the tree is removed when that guard drops — on
/// success, on panic, and on early return. Uniqueness comes from the guard;
/// `name` is only a readable label for the root inside it. Callers must bind
/// the guard (`let (_tmp, root) = test_root("…");`); a bare `_` binding drops
/// it at once and deletes the root out from under the test.
fn test_root(name: &str) -> (tempfile::TempDir, PathBuf) {
    let guard = tempfile::tempdir().expect("create temp dir");
    let root = guard.path().join(name);
    fs::create_dir_all(&root).unwrap();
    (guard, root)
}

fn write_minimal_ontology(root: &Path) {
    let ontology = root.join("ontology/gmeow.ttl");
    fs::create_dir_all(ontology.parent().unwrap()).unwrap();
    fs::write(
        ontology,
        r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

gmeow:eventTypeAdoption rdfs:label "adoption"@x-gmeow-english .
gmeow:chainId rdfs:label "chain id"@x-gmeow-english .
gmeow:placeTypeCity rdfs:label "city"@x-gmeow-english .
"#,
    )
    .unwrap();
}

fn write_test_po(root: &Path, name: &str, body: &str) {
    let path = root.join("slices/core/test/i18n").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

fn po_body(entries: &[(&str, &str, &str, bool)]) -> String {
    let mut lines = vec![
        "msgid \"\"".to_owned(),
        "msgstr \"\"".to_owned(),
        "\"Language: fr\\n\"".to_owned(),
        "\"MIME-Version: 1.0\\n\"".to_owned(),
        "\"Content-Type: text/plain; charset=UTF-8\\n\"".to_owned(),
        "\"Content-Transfer-Encoding: 8bit\\n\"".to_owned(),
        String::new(),
    ];
    for (ctx, msgid, msgstr, fuzzy) in entries {
        if *fuzzy {
            lines.push("#, fuzzy".to_owned());
        }
        lines.push(format!("msgctxt \"{ctx}\""));
        lines.push(format!("msgid \"{msgid}\""));
        lines.push(format!("msgstr \"{msgstr}\""));
        lines.push(String::new());
    }
    lines.join("\n")
}

#[test]
fn xliff_export_uses_actual_slice_path() {
    let (_tmp, root) = test_root("xliff-slice-path");
    let po_path = root.join("slices/extensions/example/i18n/fr.po");
    fs::create_dir_all(po_path.parent().unwrap()).unwrap();
    fs::write(
        &po_path,
        po_body(&[(
            "https://blackcatinformatics.ca/gmeow/exampleTerm|rdfs:label",
            "example label",
            "example label translated",
            false,
        )]),
    )
    .unwrap();

    let text = export_xliff(&root, None).unwrap();
    assert!(text.contains("original=\"slices/extensions/example\""));
    assert!(!text.contains("original=\"slices/core/example\""));
    // A non-fuzzy entry is emitted as an XLIFF `translated` target state.
    assert!(text.contains("<target state=\"translated\">example label translated</target>"));
}

#[test]
fn lint_valid_catalog_reports_no_errors() {
    let (_tmp, root) = test_root("lint-valid");
    write_minimal_ontology(&root);
    write_test_po(
        &root,
        "valid_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
                "adoption",
                "adoption",
                false,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                "chain id",
                "identifiant de chaine",
                false,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.total_counts.get("x-gmeow-french"), Some(&2));
    assert_eq!(report.fuzzy_counts.get("x-gmeow-french"), Some(&0));
}

#[test]
fn lint_flags_collapsed_translation_distinction() {
    // Two DISTINCT English sources translated to the SAME target — the translation
    // collapsed a distinction the source made. A hard reject.
    let (_tmp, root) = test_root("lint-collapsed");
    write_minimal_ontology(&root);
    write_test_po(
        &root,
        "collapsed_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
                "adoption",
                "pareil",
                false,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                "chain id",
                "pareil",
                false,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    let collisions: Vec<&String> = report
        .errors
        .iter()
        .filter(|e| e.contains("collides across"))
        .collect();
    assert_eq!(
        collisions.len(),
        1,
        "one distinctiveness error: {:?}",
        report.errors
    );
    assert!(
        collisions[0].contains("pareil") && collisions[0].contains("distinct sources"),
        "names the shared target: {collisions:?}"
    );
}

#[test]
fn lint_passes_twin_source_shared_translation() {
    // A class and its property twin share ONE English label, so sharing ONE target
    // translation is legitimate (identical msgid skeleton) and must NOT be flagged.
    let (_tmp, root) = test_root("lint-twin");
    write_minimal_ontology(&root);
    write_test_po(
        &root,
        "twin_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/PValue|rdfs:label",
                "p-value",
                "valeur p",
                false,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/pValue|rdfs:label",
                "p-value",
                "valeur p",
                false,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    assert!(
        !report.errors.iter().any(|e| e.contains("collides across")),
        "twin sources sharing one translation must not red: {:?}",
        report.errors
    );
}

/// An ontology whose terms carry the English labels the glossary-consistency tests
/// render ("read" twice as a homograph pair, "play" once), so entries do not orphan.
fn write_glossary_ontology(root: &Path) {
    let ontology = root.join("ontology/gmeow.ttl");
    fs::create_dir_all(ontology.parent().unwrap()).unwrap();
    fs::write(
        ontology,
        r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

gmeow:readPresent rdfs:label "read"@x-gmeow-english .
gmeow:readPast rdfs:label "read"@x-gmeow-english .
gmeow:playMedia rdfs:label "play"@x-gmeow-english .
"#,
    )
    .unwrap();
}

/// Author a `lang:DeclaredTerminologyHomograph` per source into a slice `module.ttl`
/// (which `authored_turtle_files` scans), so the real `lint_po_files` loader exempts them.
fn write_declared_homographs(root: &Path, sources: &[&str]) {
    let path = root.join("slices/core/test/module.ttl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut ttl = String::from(
        "@prefix lang: <https://blackcatinformatics.ca/lang/> .\n@prefix ex: <http://example.org/h/> .\n\n",
    );
    for (i, s) in sources.iter().enumerate() {
        ttl.push_str(&format!(
                "ex:hg{i} a lang:DeclaredTerminologyHomograph ; lang:homographSource \"{s}\" ; lang:homographConcept ex:c{i}a , ex:c{i}b .\n"
            ));
    }
    fs::write(path, ttl).unwrap();
}

#[test]
fn lint_flags_glossary_inconsistency() {
    // One English source ("read") translated two different ways ("lire" / "lu") across
    // batches — the cross-batch terminology-consistency violation (the dual of the
    // distinctiveness collapse). A hard reject via lang:GlossaryTermInconsistency.
    let (_tmp, root) = test_root("lint-glossary-inconsistent");
    write_glossary_ontology(&root);
    write_test_po(
        &root,
        "glossary_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/readPresent|rdfs:label",
                "read",
                "lire",
                false,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/readPast|rdfs:label",
                "read",
                "lu",
                false,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    let hits: Vec<&String> = report
        .errors
        .iter()
        .filter(|e| e.contains("different ways across batches"))
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "one glossary-consistency error: {:?}",
        report.errors
    );
    assert!(
        hits[0].contains("\"read\"") && hits[0].contains("lang:GlossaryTermInconsistency"),
        "names the source and the failure class: {hits:?}"
    );
}

#[test]
fn lint_passes_consistent_glossary() {
    // One English source rendered ONE consistent way across batches — no violation.
    let (_tmp, root) = test_root("lint-glossary-consistent");
    write_glossary_ontology(&root);
    write_test_po(
        &root,
        "glossary_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/readPresent|rdfs:label",
                "read",
                "lire",
                false,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/readPast|rdfs:label",
                "read",
                "lire",
                false,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.contains("different ways across batches")),
        "a consistent glossary must not red: {:?}",
        report.errors
    );
}

#[test]
fn lint_passes_declared_homograph() {
    // "read" is DECLARED a homograph, so its two senses may render differently
    // ("lire" / "lu") without a consistency violation. The real lint_po_files loader
    // reads the declaration from authored TTL (write_declared_homographs), not a
    // hand-built set — the production module.ttl -> gate flow.
    let (_tmp, root) = test_root("lint-glossary-homograph");
    write_glossary_ontology(&root);
    write_declared_homographs(&root, &["read"]);
    write_test_po(
        &root,
        "glossary_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/readPresent|rdfs:label",
                "read",
                "lire",
                false,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/readPast|rdfs:label",
                "read",
                "lu",
                false,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    assert!(
        !report
            .errors
            .iter()
            .any(|e| e.contains("different ways across batches")),
        "a declared homograph must be exempt: {:?}",
        report.errors
    );
}

#[test]
fn lint_flags_inconsistency_despite_unrelated_homograph() {
    // Guardrail: an UNRELATED declared homograph ("play") must not widen the exempt set
    // and mask a genuine "read" inconsistency — the exempt-set read is source-keyed and
    // authored-TTL-only, so only the exact declared source is exempted.
    let (_tmp, root) = test_root("lint-glossary-unrelated-homograph");
    write_glossary_ontology(&root);
    write_declared_homographs(&root, &["play"]);
    write_test_po(
        &root,
        "glossary_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/readPresent|rdfs:label",
                "read",
                "lire",
                false,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/readPast|rdfs:label",
                "read",
                "lu",
                false,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("different ways across batches") && e.contains("\"read\"")),
        "an unrelated homograph must not mask the read inconsistency: {:?}",
        report.errors
    );
}

#[test]
fn lint_excludes_fuzzy_from_distinctiveness() {
    // Fuzzy entries are not candidate translations, so a fuzzy collapsed pair is not
    // a distinctiveness violation (consistent with the rest of the lint's exclusions).
    let (_tmp, root) = test_root("lint-fuzzy-excluded");
    write_minimal_ontology(&root);
    write_test_po(
        &root,
        "fuzzy_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
                "adoption",
                "pareil",
                true,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                "chain id",
                "pareil",
                true,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    assert!(
        !report.errors.iter().any(|e| e.contains("collides across")),
        "fuzzy entries are excluded from the distinctiveness check: {:?}",
        report.errors
    );
}

#[test]
fn lint_reports_orphaned_and_stale_entries_as_warnings() {
    let (_tmp, root) = test_root("lint-stale");
    write_minimal_ontology(&root);
    write_test_po(
        &root,
        "stale_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/NonExistentLintTerm|rdfs:label",
                "missing term",
                "terme manquant",
                false,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/placeTypeCity|rdfs:label",
                "old city label",
                "ancienne etiquette",
                false,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.warnings.len(), 2);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("orphaned"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("stale"))
    );
}

#[test]
fn lint_rejects_copied_or_hybrid_english_as_translation() {
    let (_tmp, root) = test_root("lint-english-leak");
    write_minimal_ontology(&root);
    write_test_po(
        &root,
        "leaked_fr.po",
        &po_body(&[(
            "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
            "chain id",
            "chain id",
            false,
        )]),
    );
    let report = lint_po_files(&root, 100.0);
    assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
    assert!(report.errors[0].contains("copied into msgstr"));
}

#[test]
fn lint_all_fuzzy_catalog_is_error() {
    let (_tmp, root) = test_root("lint-fuzzy");
    write_test_po(
        &root,
        "fuzzy_fr.po",
        &po_body(&[
            (
                "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
                "adoption",
                "adoption",
                true,
            ),
            (
                "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                "chain id",
                "identifiant de chaine",
                true,
            ),
        ]),
    );
    let report = lint_po_files(&root, 100.0);
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].contains("x-gmeow-french has only fuzzy entries"));
}

#[test]
fn lint_missing_language_header_is_error() {
    let (_tmp, root) = test_root("lint-no-lang");
    write_test_po(
        &root,
        "no_lang.po",
        "msgid \"\"\nmsgstr \"\"\n\nmsgctxt \"x|rdfs:label\"\nmsgid \"x\"\nmsgstr \"y\"\n",
    );
    let report = lint_po_files(&root, 100.0);
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].contains("missing Language header"));
}
