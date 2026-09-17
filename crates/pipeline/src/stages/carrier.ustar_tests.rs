// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The GNU long-name sentinel used in wire-format assertions.
const LONGLINK_NAME: &str = "././@LongLink";

/// Decode `(name, bytes)` members from a USTAR archive via the shared codec.
fn parse(raw: &[u8]) -> Vec<(String, Vec<u8>)> {
    purrdf::ustar::read_archive(raw).unwrap()
}

#[test]
fn long_member_name_round_trips_via_longlink() {
    let long = format!(
        "x-gmeow-english/terms/classes/gmeow-{}.html",
        "A".repeat(90)
    );
    assert!(long.len() > 100, "fixture must exceed the 100-byte field");
    let members = vec![
        (long.clone(), b"<html>long</html>".to_vec()),
        ("x-gmeow-english/index.html".to_string(), b"idx".to_vec()),
    ];
    let raw = purrdf::ustar::write_archive(&members).expect("archive");
    let got = parse(&raw);
    assert_eq!(got, members, "GNU LongLink path must round-trip exactly");

    // The first record on the wire is the 'L' LongLink, then the real header
    // whose name field is the 100-byte truncation of the long path.
    assert_eq!(raw[156], b'L', "first record is a LongLink");
    assert_eq!(&raw[0..LONGLINK_NAME.len()], LONGLINK_NAME.as_bytes());
}

#[test]
fn short_names_emit_no_longlink_and_stay_plain_ustar() {
    let members = vec![
        ("mappings/a.sssom.tsv".to_string(), b"x".to_vec()),
        ("slices/core/x/tests/t.ttl".to_string(), vec![0u8; 600]),
    ];
    let raw = purrdf::ustar::write_archive(&members).expect("archive");
    // No member name overflows 100 bytes, so NO 'L' record may appear: the
    // four existing consumer archives must stay byte-identical (fold-stable).
    assert!(
        !raw.chunks(512).any(|c| c.len() == 512 && c[156] == b'L'),
        "short-name archive must not emit a LongLink record"
    );
    // The first header carries the full name inline (typeflag '0', ustar magic).
    assert_eq!(raw[156], b'0');
    assert_eq!(&raw[257..263], b"ustar\0");
    assert_eq!(&raw[263..265], b"00");
    assert_eq!(parse(&raw), members);
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn build_reasoning_blob_folds_the_report_artifacts() {
    // Construct a fake stage-reason product with the two report artifacts (avoids
    // running the reasoner); proves the wiring (rep, keys, fail-closed).
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(
        crate::stages::reason::EXPLANATIONS_PATH.to_string(),
        b"# explanations".to_vec(),
    );
    artifacts.insert(
        crate::stages::reason::LEDGER_PATH.to_string(),
        b"# ledger".to_vec(),
    );
    artifacts.insert(
        crate::stages::reason::PERF_LEDGER_PATH.to_string(),
        b"# perf ledger".to_vec(),
    );
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-reason".to_string(),
        StageProduct::from_artifacts("stage-reason", artifacts),
    );
    upstream.insert(
        "stage-verify-attestation".to_owned(),
        StageProduct::from_artifacts(
            "stage-verify-attestation",
            BTreeMap::from([(
                gmeow_logic::verify::PREPARED_GATES_CHANNEL.to_owned(),
                b"synthetic prepared laws".to_vec(),
            )]),
        ),
    );
    upstream.insert(
        "stage-conformance".to_owned(),
        StageProduct::from_artifacts(
            "stage-conformance",
            BTreeMap::from([
                (
                    gmeow_logic_compile::action_policy::SOURCE_ARTIFACT.to_owned(),
                    b"synthetic policy".to_vec(),
                ),
                (
                    gmeow_logic::operator_rules::PREPARED_OPERATOR_CHANNEL.to_owned(),
                    b"synthetic operator rules".to_vec(),
                ),
            ]),
        ),
    );
    let blob = build_reasoning_blob(&upstream).expect("reasoning blob");
    assert_eq!(blob.rep, REP_REASONING);
    assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);
    let members = parse(&blob.data);
    let names: std::collections::BTreeSet<&str> = members.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "reason/dl-el-crosscheck-report.ttl",
            "reason/perf-ledger.ttl",
            gmeow_bundle_view::bundle_blobs::REASONED_GATES_MEMBER,
            gmeow_logic_compile::action_policy::BUNDLE_MEMBER,
            gmeow_logic::operator_rules::PREPARED_OPERATOR_MEMBER,
            "reason/reasoning-explanations.rdf12.ttl"
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<&str>>(),
        "REP_REASONING carries the report artifacts under bundle-relative keys"
    );
    // Missing artifact HARD-fails (no-optionality, fail-closed).
    upstream.remove("stage-verify-attestation");
    assert!(
        build_reasoning_blob(&upstream).is_err(),
        "prepared native laws are required"
    );
    let empty: BTreeMap<String, StageProduct> = BTreeMap::new();
    assert!(
        build_reasoning_blob(&empty).is_err(),
        "a missing stage-reason product must fail closed"
    );
}

#[test]
fn okf_link_targets_missing_from_flags_only_the_absent_target() {
    // Pure-logic test of the hard-fail comparison itself: prove it does not
    // silently accept a link whose target the OKF bundle never emits, and does
    // not false-positive on a link whose target IS emitted.
    let emitted: std::collections::BTreeSet<String> =
        ["classes/Present.md".to_string()].into_iter().collect();
    let links = vec![
        Some("gmeow-okf/classes/Present.md".to_string()),
        Some("gmeow-okf/classes/Absent.md".to_string()),
        None, // e.g. a Datatype/Other term the OKF bundle deliberately skips
    ];
    let missing = okf_link_targets_missing_from(&emitted, &links);
    assert_eq!(
        missing,
        vec![1],
        "only the link whose target is absent from the emitted set must be flagged"
    );
}

#[test]
fn header_checksum_is_valid() {
    // Build a minimal archive and inspect the first 512-byte header.
    let members = vec![("x-gmeow-english/index.html".to_string(), vec![0u8; 42])];
    let raw = purrdf::ustar::write_archive(&members).expect("archive");
    let h: &[u8] = &raw[..512];
    // The stored checksum equals the sum of all bytes with the checksum field
    // taken as spaces — the canonical USTAR self-check.
    let stored = usize::from_str_radix(
        std::str::from_utf8(&h[148..154])
            .unwrap()
            .trim_matches('\0')
            .trim(),
        8,
    )
    .unwrap();
    let mut probe = [0u8; 512];
    probe.copy_from_slice(h);
    for b in &mut probe[148..156] {
        *b = b' ';
    }
    let computed: usize = probe.iter().map(|&b| b as usize).sum();
    assert_eq!(stored, computed);
}

// ── docs-book / docs-print blob wiring (fresh-build, no committed-bundle dep) ──

/// A small, deterministic docs model (one slice, three terms, one competency, one
/// linkage) — the SAME shape the `docs-print` integration suite uses. It stays
/// small so unit tests isolate the renderer; full-catalog render/compile belongs
/// to the regenerate gate.
fn small_docs_model() -> gmeow_docs::model::DocsModel {
    use gmeow_docs::model::{
        DocCompetency, DocLinkage, DocSlice, DocTerm, DocTermCategory, DocsModel, ReasoningVerdict,
    };
    let slice_iri = "https://blackcatinformatics.ca/gmeow/slice/demo".to_string();
    let mk = |iri: &str, curie: &str, label: &str, def: &str, cat: DocTermCategory| DocTerm {
        iri: iri.to_string(),
        curie: curie.to_string(),
        label: Some(label.to_string()),
        definition: Some(def.to_string()),
        category: cat,
        owner_slice: slice_iri.clone(),
        ..Default::default()
    };
    let demo_slice = DocSlice {
        iri: slice_iri.clone(),
        label: Some("Demo".to_string()),
        title: Some("Demo slice".to_string()),
        tier: None,
        identifier: None,
        creators: Vec::new(),
        consumers: Vec::new(),
        profiles: Vec::new(),
        depends_on: Vec::new(),
        artifacts: Vec::new(),
        documents: Vec::new(),
        has_thesis_sentence: false,
        realized_state_complete: false,
    };
    let competency = DocCompetency {
        iri: "https://blackcatinformatics.ca/gmeow/cq/demo".to_string(),
        rationale: Some("Can a demo Foo be found?".to_string()),
        query_file: Some("demo.rq".to_string()),
        exercises: vec!["https://blackcatinformatics.ca/gmeow/Foo".to_string()],
        owner_slice: slice_iri.clone(),
        ..Default::default()
    };
    let linkage = DocLinkage {
        mapping_set: None,
        subject: "https://blackcatinformatics.ca/gmeow/Foo".to_string(),
        subject_curie: "gmeow:Foo".to_string(),
        predicate: "http://www.w3.org/2004/02/skos/core#closeMatch".to_string(),
        object: "http://purl.org/nemo/gufo#Object".to_string(),
        justification: None,
        confidence: Some(0.9),
        owner_slice: slice_iri.clone(),
    };
    DocsModel {
        title: "GMEOW Demo Documentation".to_string(),
        version: "test-1".to_string(),
        slices: vec![demo_slice],
        terms: vec![
            mk(
                "https://blackcatinformatics.ca/gmeow/Foo",
                "gmeow:Foo",
                "Foo",
                "A foundational demonstration class.",
                DocTermCategory::Class,
            ),
            mk(
                "https://blackcatinformatics.ca/gmeow/hasValue",
                "gmeow:hasValue",
                "hasValue",
                "Relates a Foo to a value.",
                DocTermCategory::Property,
            ),
            mk(
                "https://blackcatinformatics.ca/gmeow/Baz",
                "gmeow:Baz",
                "Baz",
                "An individual of the demo.",
                DocTermCategory::Individual,
            ),
        ],
        competencies: vec![competency],
        linkages: vec![linkage],
        reasoning: Some(ReasoningVerdict {
            is_consistent: true,
            unsatisfiable: Default::default(),
        }),
        ..Default::default()
    }
}

/// A minimal valid BibTeX database, the stand-in for the `stage-export-references`
/// product's `references.bib` in the print-blob tests.
fn fixture_bib() -> Vec<u8> {
    b"@article{gmeow2026,\n  title = {The GMEOW Ontology},\n  author = {Audley, Patrick},\n  year = {2026},\n  journal = {Journal of Ontology},\n}\n".to_vec()
}

/// A synthetic upstream product map carrying the two products `build_docs_print_blob`
/// reads: `stage-export-references` (the bibliography) and `stage-compile-logic` (the
/// axiom listings). Each axiom file carries small synthetic bytes — the PDF lists them
/// verbatim, so their content need not be the real projection for a wiring test.
fn print_upstream() -> BTreeMap<String, StageProduct> {
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    let mut refs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    refs.insert(
        crate::stages::references::BIB_PATH.to_string(),
        fixture_bib(),
    );
    upstream.insert(
        "stage-export-references".to_string(),
        StageProduct::from_artifacts("stage-export-references", refs),
    );
    let mut logic: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for rel in AXIOM_FILES {
        logic.insert(
            rel.to_string(),
            format!("% axiom listing for {rel}\n").into_bytes(),
        );
    }
    upstream.insert(
        "stage-compile-logic".to_string(),
        StageProduct::from_artifacts("stage-compile-logic", logic),
    );
    upstream
}

#[test]
fn build_docs_book_archive_packs_the_mdbook_tree() {
    let root = repo_root();
    let model = small_docs_model();
    let exec = gmeow_docs::ExecutableDocsData::default();

    let blob = build_docs_book_archive(&root, &model, &exec).expect("docs-book archive");
    assert_eq!(blob.rep, REP_DOCS_BOOK);
    assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);

    let members = parse(&blob.data);
    // Every member rides under the English internal tag, and the two mdbook anchor
    // files are present.
    assert!(
        members
            .iter()
            .all(|(n, _)| n.starts_with("x-gmeow-english/")),
        "every book member must carry the English internal-tag prefix, got e.g. {:?}",
        members.iter().map(|(n, _)| n).take(3).collect::<Vec<_>>()
    );
    assert!(
        members
            .iter()
            .any(|(n, _)| n == "x-gmeow-english/book.toml"),
        "the mdbook book.toml must be present"
    );
    assert!(
        members
            .iter()
            .any(|(n, _)| n == "x-gmeow-english/src/SUMMARY.md"),
        "the mdbook SUMMARY.md must be present"
    );

    // Byte-stability: a second build folds byte-identical archive bytes.
    let again = build_docs_book_archive(&root, &model, &exec).expect("docs-book archive again");
    assert_eq!(
        blob.data, again.data,
        "the docs-book archive must be byte-deterministic"
    );
}

#[test]
fn build_docs_print_blob_packs_pdf_and_typ() {
    let model = small_docs_model();
    let upstream = print_upstream();

    let (blob, pdf_digest) = build_docs_print_blob(&model, &upstream).expect("docs-print blob");
    assert_eq!(blob.rep, REP_DOCS_PRINT);
    assert_eq!(blob.media_type, ARCHIVE_MEDIA_TYPE);

    let members: BTreeMap<String, Vec<u8>> = parse(&blob.data).into_iter().collect();
    let pdf = members
        .get("x-gmeow-english/gmeow.pdf")
        .expect("the print PDF member must be present");
    assert!(
        pdf.starts_with(b"%PDF"),
        "the print member must be a real PDF (starts with %PDF)"
    );
    assert_eq!(
        pdf_digest,
        purrdf::gts::writer::digest_string(pdf),
        "the returned pdf digest must be the raw PDF's blake3, not the archive's"
    );
    let typ = members
        .get("x-gmeow-english/gmeow.typ")
        .expect("the Typst source member must be present");
    assert!(
        !typ.is_empty(),
        "the Typst source member must carry the rendered source"
    );

    // Byte-stability: a second build folds byte-identical archive bytes (the Typst
    // source is pure and the PDF compile is byte-reproducible).
    let (again, again_digest) =
        build_docs_print_blob(&model, &upstream).expect("docs-print blob again");
    assert_eq!(
        blob.data, again.data,
        "the docs-print archive must be byte-deterministic"
    );
    assert_eq!(
        pdf_digest, again_digest,
        "the raw pdf digest must be byte-deterministic too"
    );
}

/// The threaded PDF digest must bind the raw `gmeow.pdf` bytes, not the tar that
/// packs them. The producer-stage gate owns the corpus-level attestation assertion.
#[test]
fn shipped_pdf_attestation_binds_the_raw_pdf_bytes() {
    let model = small_docs_model();
    let upstream = print_upstream();

    // The producer path: the same blob + raw-PDF digest the carrier threads.
    let (print_blob, print_pdf_digest) =
        build_docs_print_blob(&model, &upstream).expect("docs-print blob");

    // The consumer path: untar the shipped blob, find gmeow.pdf, digest the RAW bytes.
    let members: BTreeMap<String, Vec<u8>> = parse(&print_blob.data).into_iter().collect();
    let pdf = members
        .get("x-gmeow-english/gmeow.pdf")
        .expect("the docs-print blob must carry gmeow.pdf");
    let recomputed = purrdf::gts::writer::digest_string(pdf);
    assert_eq!(
        recomputed, print_pdf_digest,
        "the threaded raw-PDF digest must equal the blake3 of the shipped gmeow.pdf"
    );
}

#[test]
fn docs_book_and_print_resolve_via_bundle_round_trip() {
    let root = repo_root();
    let model = small_docs_model();
    let exec = gmeow_docs::ExecutableDocsData::default();
    let upstream = print_upstream();

    let book_blob = build_docs_book_archive(&root, &model, &exec).expect("docs-book archive");
    let (print_blob, _print_pdf_digest) =
        build_docs_print_blob(&model, &upstream).expect("docs-print blob");

    // Fold a minimal snapshot carrying exactly the two new blobs (plus a well-formed
    // base graph) through the SAME emit path the carrier uses, then read them back
    // through the repo-free `Bundle` reader — the producer↔reader wiring end-to-end.
    let mut builder = SnapshotBuilder::new();
    add_base_nq(
        &mut builder,
        b"<https://blackcatinformatics.ca/gmeow/> \
              <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
              <http://www.w3.org/2002/07/owl#Ontology> .\n",
        "base",
    )
    .expect("fold base graph");
    // gmeow-test-input: synthetic-only
    let gts = emit_gts(
        &builder,
        "dist",
        Some(vec!["zstd-rsyncable".to_string()]),
        vec![book_blob, print_blob],
        Vec::new(),
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(Some(&["zstd-rsyncable".to_string()])),
    )
    .expect("emit snapshot");

    let bundle =
        crate::bundle_blobs::Bundle::from_snapshot(&gts).expect("fold the minimal snapshot");
    let book = bundle.docs_book().expect("docs_book resolves");
    assert!(
        book.contains_key("x-gmeow-english/book.toml")
            && book.contains_key("x-gmeow-english/src/SUMMARY.md"),
        "docs_book() must resolve the mdbook anchor members; got {:?}",
        book.keys().take(4).collect::<Vec<_>>()
    );
    let print = bundle.docs_print().expect("docs_print resolves");
    assert!(
        print
            .get("x-gmeow-english/gmeow.pdf")
            .is_some_and(|b| b.starts_with(b"%PDF")),
        "docs_print() must resolve the PDF member as a real PDF"
    );
    assert!(
        print.contains_key("x-gmeow-english/gmeow.typ"),
        "docs_print() must resolve the Typst source member"
    );
}
