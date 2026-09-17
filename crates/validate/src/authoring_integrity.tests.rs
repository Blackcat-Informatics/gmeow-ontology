// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::collections::BTreeSet;

fn ds(ttl: &str) -> Dataset {
    let prefixes = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix dcterms: <http://purl.org/dc/terms/> .\n\
             @prefix ex: <https://example.org/> .\n";
    Dataset::parse_turtle(format!("{prefixes}{ttl}").as_bytes(), None, "test").unwrap()
}

#[test]
fn shape_iri_collision_fires_when_one_iri_owns_two_files() {
    let a = ds("ex:PersonShape a sh:NodeShape .");
    let b = ds("ex:PersonShape a sh:NodeShape .\nex:OtherShape a sh:NodeShape .");
    let files = vec![
        (PathBuf::from("shapes/a.ttl"), a),
        (PathBuf::from("shapes/b.ttl"), b),
    ];
    let findings = detect_shape_collisions(&files, Path::new("")).unwrap();
    assert_eq!(findings.len(), 1, "exactly the colliding IRI is reported");
    assert_eq!(findings[0].code, codes::AUTHORING_SHAPE_IRI_COLLISION);
    assert!(findings[0].message.contains("PersonShape"));
    // The non-colliding OtherShape is not flagged.
    assert!(!findings[0].message.contains("OtherShape"));
}

#[test]
fn shape_iri_collision_clean_when_every_iri_is_unique() {
    let a = ds("ex:AShape a sh:NodeShape .");
    let b = ds("ex:BShape a sh:NodeShape .");
    let files = vec![(PathBuf::from("a.ttl"), a), (PathBuf::from("b.ttl"), b)];
    assert!(
        detect_shape_collisions(&files, Path::new(""))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn graft_leak_fires_on_a_norms_iri_in_any_position() {
    // gmeow:Norm as object, gmeow:normIssuer as predicate.
    let d = ds("ex:x a gmeow:Norm ; gmeow:normIssuer ex:issuer .");
    let findings = detect_graft_leaks(&d, "slices/core/rights/module.ttl");
    let codes_seen: Vec<&str> = findings.iter().map(|f| f.code.as_str()).collect();
    assert!(
        findings.len() >= 2,
        "both Norm and normIssuer are flagged: {findings:?}"
    );
    assert!(codes_seen.iter().all(|c| *c == codes::AUTHORING_GRAFT_LEAK));
    assert!(findings.iter().any(|f| f.message.contains("/Norm")));
    assert!(findings.iter().any(|f| f.message.contains("/normIssuer")));
}

#[test]
fn graft_leak_exact_identity_not_substring() {
    // A distinct term whose IRI has a norms term as a prefix must NOT match.
    let d = ds("ex:x <https://blackcatinformatics.ca/gmeow/normIssuerRole> ex:r .");
    assert!(
        detect_graft_leaks(&d, "m.ttl").is_empty(),
        "normIssuerRole must not match normIssuer by substring"
    );
}

#[test]
fn slice_discipline_flags_missing_tier() {
    let d = ds("ex:s a gmeow:Slice ; rdfs:label \"S\"@x-gmeow-english .");
    let findings = detect_slice_discipline(
        &[(PathBuf::from("slices/g/s/manifest.ttl"), d)],
        Path::new(""),
    )
    .unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, codes::SLICE_DISCIPLINE_MISSING_TIER);
}

#[test]
fn slice_discipline_flags_duplicate_iri() {
    let a = ds("ex:dup a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
    let b = ds("ex:dup a gmeow:Slice ; gmeow:sliceTier gmeow:tierExtension .");
    let findings = detect_slice_discipline(
        &[
            (PathBuf::from("slices/core/one/manifest.ttl"), a),
            (PathBuf::from("slices/extensions/two/manifest.ttl"), b),
        ],
        Path::new(""),
    )
    .unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, codes::SLICE_DISCIPLINE_DUPLICATE_IRI);
    assert!(findings[0].message.contains("dup"));
}

#[test]
fn slice_discipline_clean_on_well_formed_unique_tiered_manifests() {
    let a = ds("ex:one a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
    let b = ds("ex:two a gmeow:Slice ; gmeow:sliceTier gmeow:tierExtension .");
    assert!(
        detect_slice_discipline(
            &[
                (PathBuf::from("slices/core/one/manifest.ttl"), a),
                (PathBuf::from("slices/extensions/two/manifest.ttl"), b),
            ],
            Path::new(""),
        )
        .unwrap()
        .is_empty()
    );
}

// ── R10: retired owl: authoring prefix (source-text lint) ────────────────

#[test]
fn retired_authoring_prefix_fires_on_reintroduced_owl_prefix() {
    // A slice module.ttl source with BOTH an `@prefix owl:` declaration and a
    // prefixed-name `owl:Class` use — each must fire the source lint.
    let text = "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
                    ex:Thing a owl:Class .\n";
    let findings = detect_retired_authoring_prefixes(text, "slices/core/x/module.ttl");
    assert!(
        !findings.is_empty(),
        "a reintroduced owl: prefix must fire: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .all(|f| f.code == codes::AUTHORING_RETIRED_OWL_PREFIX),
        "every finding uses the retired-owl-prefix code: {findings:?}"
    );
    assert!(
        findings.iter().all(|f| f.severity == Severity::Error),
        "the source lint is an Error: {findings:?}"
    );
    assert!(
        findings.iter().any(|f| f.message.contains("logic:")),
        "the message names logic: as the canonical authoring vocabulary"
    );
}

#[test]
fn retired_authoring_prefix_clean_on_logic_authoring_and_full_iri_target() {
    // Canonical logic: authoring plus a full-IRI owl# correspondence target —
    // no owl: prefix token, so NO finding. `powl:` (longer prefix) and `OWL`
    // (reworded prose, no colon) must not be false positives either.
    let text = "@prefix logic: <https://blackcatinformatics.ca/gmeow/logic/> .\n\
                    ex:Thing a logic:Class .\n\
                    ex:law logic:correspondsTo <http://www.w3.org/2002/07/owl#Class> .\n\
                    ex:x a powl:Widget .  # OWL is a generated projection, not authored\n";
    assert!(
        detect_retired_authoring_prefixes(text, "slices/core/x/module.ttl").is_empty(),
        "clean logic: authoring with a full-IRI owl# target must not fire"
    );
}

#[test]
fn retired_authoring_prefix_findings_label_repo_relative() {
    // Thread r3818278198: the production scan must label findings repo-root-relative
    // (`slices/core/x/module.ttl`), not slices_dir-relative (`core/x/…`) or absolute.
    let tmp = tempfile::tempdir().expect("temp project root");
    let root = tmp.path();
    let slice_dir = root.join("slices").join("core").join("x");
    std::fs::create_dir_all(&slice_dir).unwrap();
    // manifest.ttl is the discovery surface all_manifests walks; module.ttl carries the owl:.
    std::fs::write(slice_dir.join("manifest.ttl"), "").unwrap();
    std::fs::write(slice_dir.join("module.ttl"), "ex:Thing a owl:Class .\n").unwrap();
    let findings =
        retired_authoring_prefix_findings(root, &root.join("slices")).expect("scan succeeds");
    assert!(
        findings
            .iter()
            .any(|f| f.message.starts_with("slices/core/x/module.ttl:")),
        "finding must be labelled repo-relative, got: {findings:?}"
    );
}

// ── R8: grounding-peerage discipline ─────────────────────────────────────
//
// `detect_peerage_discipline` computes manifest paths relative to
// `slices_dir` itself (the live call passes `slices_dir` as `root`), so
// these tests pass manifest paths WITHOUT a leading `slices/` (unlike the
// R6 tests above, which pass `root = ""` and full `slices/...` paths) —
// `grounding/x/manifest.ttl`, matching the real `rel(path, slices_dir)`
// shape the grounding-marker-drift check keys on.

#[test]
fn peerage_discipline_flags_non_grounding_slice_declaring_peerage() {
    let a = ds("ex:one a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:two .");
    let b = ds("ex:two a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:one .");
    let findings = detect_peerage_discipline(
        &[
            (PathBuf::from("core/one/manifest.ttl"), a),
            (PathBuf::from("core/two/manifest.ttl"), b),
        ],
        Path::new(""),
    )
    .unwrap();
    let non_grounding: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == codes::SLICE_DISCIPLINE_NON_GROUNDING_PEERAGE)
        .collect();
    // Neither `one` nor `two` is typed gmeow:GroundingSlice, and the
    // relation IS mutually symmetric — only the non-grounding-peerage gate
    // fires (twice, once per non-grounding declarer), never the asymmetry
    // gate.
    assert_eq!(
        non_grounding.len(),
        2,
        "both non-grounding peers flagged: {findings:?}"
    );
    assert!(
        findings
            .iter()
            .all(|f| f.code != codes::SLICE_DISCIPLINE_ASYMMETRIC_PEERAGE),
        "a mutually-declared pair must not ALSO fire asymmetric-peerage: {findings:?}"
    );
}

#[test]
fn peerage_discipline_flags_asymmetric_peerage() {
    let a = ds(
        "ex:one a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:two .",
    );
    // `two` never declares the relation back to `one`.
    let b = ds("ex:two a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore .");
    let findings = detect_peerage_discipline(
        &[
            (PathBuf::from("grounding/one/manifest.ttl"), a),
            (PathBuf::from("grounding/two/manifest.ttl"), b),
        ],
        Path::new(""),
    )
    .unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, codes::SLICE_DISCIPLINE_ASYMMETRIC_PEERAGE);
    assert!(findings[0].message.contains("one"));
    assert!(findings[0].message.contains("two"));
}

#[test]
fn peerage_discipline_flags_grounding_marker_drift_both_directions() {
    // Under grounding/ but NOT typed gmeow:GroundingSlice.
    let untyped_under_grounding = ds("ex:one a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
    // Typed gmeow:GroundingSlice but NOT under grounding/.
    let typed_elsewhere =
        ds("ex:two a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore .");
    let findings = detect_peerage_discipline(
        &[
            (
                PathBuf::from("grounding/one/manifest.ttl"),
                untyped_under_grounding,
            ),
            (PathBuf::from("core/two/manifest.ttl"), typed_elsewhere),
        ],
        Path::new(""),
    )
    .unwrap();
    let drift: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == codes::SLICE_DISCIPLINE_GROUNDING_MARKER_DRIFT)
        .collect();
    assert_eq!(drift.len(), 2, "{findings:?}");
    assert!(drift.iter().any(|f| f.message.contains("one")));
    assert!(drift.iter().any(|f| f.message.contains("two")));
}

#[test]
fn peerage_discipline_clean_on_the_real_corpus_shape() {
    // Mirrors the real grounding trio: three GroundingSlice manifests under
    // grounding/, mutually peered.
    let logic = ds(
        "ex:logic a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:lang, ex:math .",
    );
    let lang = ds(
        "ex:lang a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:logic, ex:math .",
    );
    let math = ds(
        "ex:math a gmeow:Slice, gmeow:GroundingSlice ; gmeow:sliceTier gmeow:tierCore ; \
             gmeow:sliceCoFoundationalWith ex:logic, ex:lang .",
    );
    let core = ds("ex:core a gmeow:Slice ; gmeow:sliceTier gmeow:tierCore .");
    assert!(
        detect_peerage_discipline(
            &[
                (PathBuf::from("grounding/logic/manifest.ttl"), logic),
                (PathBuf::from("grounding/lang/manifest.ttl"), lang),
                (PathBuf::from("grounding/math/manifest.ttl"), math),
                (PathBuf::from("core/core/manifest.ttl"), core),
            ],
            Path::new(""),
        )
        .unwrap()
        .is_empty()
    );
}

fn slice(iri: &str, tier: Option<Tier>) -> SliceRec {
    SliceRec {
        iri: iri.to_string(),
        tier,
        raw_tiers: match tier {
            Some(Tier::Core) => vec![TIER_CORE.to_string()],
            Some(Tier::Extension) => vec![TIER_EXTENSION.to_string()],
            Some(Tier::Profile) => vec![TIER_PROFILE.to_string()],
            None => Vec::new(),
        },
    }
}

fn iset(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn profile_closure_clean_on_a_well_formed_partition() {
    let slices = vec![
        slice("g/core-a", Some(Tier::Core)),
        slice("g/ext-a", Some(Tier::Extension)),
        slice("g/prof-a", Some(Tier::Profile)),
    ];
    // full = ontology ∪ {ext}, claims ⊊ core.
    let full = iset(&[ONTOLOGY_IRI, "g/ext-a"]);
    let claims = iset(&[]); // strict subset of {core-a}
    assert!(detect_profile_closure(&slices, &full, &claims).is_empty());
}

#[test]
fn profile_closure_flags_full_missing_an_extension() {
    let slices = vec![
        slice("g/core-a", Some(Tier::Core)),
        slice("g/ext-a", Some(Tier::Extension)),
    ];
    let full = iset(&[ONTOLOGY_IRI]); // missing ext-a
    let claims = iset(&[]);
    let findings = detect_profile_closure(&slices, &full, &claims);
    assert!(findings.iter().any(|f| f.message.contains("full.ttl")));
}

#[test]
fn profile_closure_flags_claims_not_strict_subset() {
    let slices = vec![slice("g/core-a", Some(Tier::Core))];
    let full = iset(&[ONTOLOGY_IRI]);
    // claims == core (not STRICT) → violation.
    let claims = iset(&["g/core-a"]);
    let findings = detect_profile_closure(&slices, &full, &claims);
    assert!(findings.iter().any(|f| f.message.contains("claims.ttl")));
}

#[test]
fn profile_closure_flags_unrecognized_tier_but_not_a_tierless_slice() {
    // A tierless slice is the discipline gate's job — NOT re-reported here.
    // (A real core slice is present so the claims⊊core check is well-formed;
    // claims ⊊ {core-a} holds for the empty claims set.)
    let clean = vec![
        slice("g/core-a", Some(Tier::Core)),
        slice("g/tierless", None),
    ];
    let full = iset(&[ONTOLOGY_IRI]);
    assert!(detect_profile_closure(&clean, &full, &iset(&[])).is_empty());

    // A slice WITH a sliceTier value that is not one of the three IS flagged.
    let bogus = SliceRec {
        iri: "g/bogus".to_string(),
        tier: None,
        raw_tiers: vec!["https://blackcatinformatics.ca/gmeow/tierBogus".to_string()],
    };
    let findings = detect_profile_closure(
        &[slice("g/core-a", Some(Tier::Core)), bogus],
        &full,
        &iset(&[]),
    );
    assert!(findings.iter().any(|f| f.message.contains("unrecognized")));
}

#[test]
fn catalog_names_parse_ignores_comments_and_default_namespace() {
    let xml = "<?xml version=\"1.0\"?>\n\
             <!-- a comment mentioning uri name= that must not be scanned -->\n\
             <catalog xmlns=\"urn:oasis:names:tc:entity:xmlns:xml:catalog\">\n\
               <uri name=\"https://blackcatinformatics.ca/gmeow/slices/temporal\" uri=\"a.ttl\"/>\n\
               <uri name=\"https://blackcatinformatics.ca/gmeow\" uri=\"b.ttl\"/>\n\
             </catalog>";
    let names = parse_catalog_names(xml, Path::new("catalog-v001.xml")).unwrap();
    assert_eq!(names.len(), 2);
    assert!(names.contains("https://blackcatinformatics.ca/gmeow/slices/temporal"));
    // The commented-out text is not harvested.
    assert!(!names.iter().any(|n| n.contains("must not be scanned")));
}

#[test]
fn module_iri_expected_is_the_slice_dir_not_the_group() {
    // The expected IRI derives from the immediate parent dir (slice name),
    // never the grandparent group segment.
    let module = PathBuf::from("slices/core/temporal/module.ttl");
    let slice_dir = module.parent().unwrap().file_name().unwrap();
    assert_eq!(slice_dir, "temporal");
    let expected = format!("{GMEOW_NS}slices/{}", slice_dir.to_string_lossy());
    assert_eq!(
        expected,
        "https://blackcatinformatics.ca/gmeow/slices/temporal"
    );
}

#[test]
fn undeclared_term_fires_on_a_term_absent_from_the_declared_set() {
    let declared: BTreeSet<String> = ["https://blackcatinformatics.ca/gmeow/Person".to_string()]
        .into_iter()
        .collect();
    // Uses gmeow:Person (declared) and gmeow:hasBogusProp (undeclared).
    let d = ds("ex:x a gmeow:Person ; gmeow:hasBogusProp ex:y .");
    let findings =
        detect_undeclared_terms(&declared, &[(PathBuf::from("f.ttl"), d)], Path::new(""));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, codes::AUTHORING_UNDECLARED_TERM);
    assert!(findings[0].message.contains("hasBogusProp"));
}

#[test]
fn vocab_terms_exclude_examples_and_modules_iris() {
    let d = ds("gmeow:RealTerm a gmeow:Class .\n\
             <https://blackcatinformatics.ca/gmeow/examples/foo> a gmeow:RealTerm .\n\
             <https://blackcatinformatics.ca/gmeow/modules/bar> a gmeow:RealTerm .");
    let terms = gmeow_vocab_terms(&d);
    assert!(terms.contains("https://blackcatinformatics.ca/gmeow/RealTerm"));
    assert!(!terms.iter().any(|t| t.contains("/examples/")));
    assert!(!terms.iter().any(|t| t.contains("/modules/")));
}

#[test]
fn untagged_localizable_literal_fires_and_tagged_is_clean() {
    // rdfs:label untagged → flagged; skos:definition tagged → clean.
    let bad = ds("ex:x rdfs:label \"plain\" ; skos:definition \"tagged\"@x-gmeow-english .");
    let findings = detect_untagged_localizable(&[(PathBuf::from("m.ttl"), bad)], Path::new(""));
    assert_eq!(findings.len(), 1, "only the untagged label is flagged");
    assert_eq!(
        findings[0].code,
        codes::AUTHORING_UNTAGGED_LOCALIZABLE_LITERAL
    );
    assert!(findings[0].message.contains("label"));
}

#[test]
fn untagged_ignores_non_localizable_predicates() {
    // A plain literal on a NON-localizable predicate is not a translation concern.
    let d = ds("ex:x ex:count \"42\" .");
    assert!(detect_untagged_localizable(&[(PathBuf::from("m.ttl"), d)], Path::new("")).is_empty());
}

#[test]
fn docs_markdown_extraction_finds_fenced_and_inline_terms() {
    let md = "# Doc\n\nUse `gmeow:Person` inline.\n\n```turtle\n\
             ex:a a gmeow:Organization ; `gmeow:memberOf` ex:b .\n```\n";
    let terms = extract_gmeow_terms_from_markdown(md);
    assert!(terms.contains("https://blackcatinformatics.ca/gmeow/Person"));
    assert!(terms.contains("https://blackcatinformatics.ca/gmeow/Organization"));
    assert!(terms.contains("https://blackcatinformatics.ca/gmeow/memberOf"));
}

/// F3 regression: a MODULE-LESS slice (a `manifest.ttl` with NO `module.ttl` —
/// the pure-selection profile-slice shape) whose `examples/*.ttl` uses an
/// undeclared term must still be caught. Discovery keyed on `slice_module_files`
/// alone (the pre-fix behavior) would silently skip this slice entirely,
/// because it mints no module and so is invisible to a module-only walk —
/// exactly the blind spot `slices/profile/agent-runtime` demonstrated live.
/// Keying on `all_manifests` instead (every slice has a manifest) closes it.
#[test]
fn example_undeclared_term_fires_on_a_module_less_slice() {
    let tmp = tempfile::tempdir().expect("temp slices dir");
    let slice_dir = tmp.path().join("profile/no-module-slice");
    std::fs::create_dir_all(slice_dir.join("examples")).unwrap();

    // A manifest declaring gmeow:Slice + a tier — NO module.ttl anywhere in
    // this slice directory (the module-less pure-selection shape).
    std::fs::write(
        slice_dir.join("manifest.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             <https://blackcatinformatics.ca/gmeow/slices/no-module-slice> a gmeow:Slice ;\n\
               gmeow:sliceTier gmeow:tierProfile ;\n\
               rdfs:label \"no-module-slice\"@x-gmeow-english .\n",
    )
    .unwrap();
    assert!(
        !slice_dir.join("module.ttl").exists(),
        "the fixture must genuinely be module-less"
    );

    // The example references an undeclared GMEOW term.
    std::fs::write(
        slice_dir.join("examples/bad.ttl"),
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix ex: <https://blackcatinformatics.ca/gmeow/examples/no-module-slice/> .\n\
             ex:thing gmeow:totallyBogusUndeclaredPredicateXYZ ex:other .\n",
    )
    .unwrap();

    let declared: BTreeSet<String> = BTreeSet::new();
    let files = load_ttl_files(&slice_example_files(tmp.path()).unwrap()).unwrap();
    let findings = detect_undeclared_terms(&declared, &files, tmp.path());
    assert_eq!(
        findings.len(),
        1,
        "the module-less slice's example must be discovered and its undeclared \
             term flagged: {findings:?}"
    );
    assert_eq!(findings[0].code, codes::AUTHORING_UNDECLARED_TERM);
    assert!(
        findings[0]
            .message
            .contains("totallyBogusUndeclaredPredicateXYZ")
    );
}

// ── R9: registered minting namespaces ────────────────────────────────────

/// Build a one-slice fixture tree: `manifest.ttl` plus the given authored
/// `module.ttl` / `shapes.ttl` bodies, prefixed with the standard header.
fn temp_minting_slice(
    name: &str,
    module_body: &str,
    shapes_body: Option<&str>,
) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp slices dir");
    let slice_dir = tmp.path().join("core").join(name);
    std::fs::create_dir_all(&slice_dir).unwrap();
    let header = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
             @prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n";
    std::fs::write(
        slice_dir.join("manifest.ttl"),
        format!(
            "{header}<https://blackcatinformatics.ca/gmeow/slices/{name}> a gmeow:Slice ;\n  \
                 gmeow:sliceTier gmeow:tierCore ;\n  rdfs:label \"{name}\"@x-gmeow-english .\n"
        ),
    )
    .unwrap();
    std::fs::write(
        slice_dir.join("module.ttl"),
        format!("{header}{module_body}"),
    )
    .unwrap();
    if let Some(body) = shapes_body {
        std::fs::write(slice_dir.join("shapes.ttl"), format!("{header}{body}")).unwrap();
    }
    tmp
}

/// R9 FIRES: a slice minting its whole vocabulary into an unregistered
/// GMEOW-authority namespace — the `math`-shaped slice that was invisible to
/// ownership analysis. Both the T-Box typing and the `rdfs:isDefinedBy`
/// ownership claim are reported, and the fixture proves the gate is capable
/// of failing rather than being vacuously green.
#[test]
fn unregistered_minting_fires_on_a_slice_minting_into_its_own_namespace() {
    let tmp = temp_minting_slice(
        "chem",
        "<https://blackcatinformatics.ca/chem/Molecule>\n  a owl:Class ;\n  \
             rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/chem> ;\n  \
             rdfs:label \"molecule\"@x-gmeow-english .\n",
        None,
    );
    let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
    assert_eq!(
        findings.len(),
        1,
        "the unregistered mint must be flagged exactly once: {findings:?}"
    );
    assert_eq!(
        findings[0].code,
        codes::AUTHORING_UNREGISTERED_TERM_NAMESPACE
    );
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(
        findings[0]
            .message
            .contains("https://blackcatinformatics.ca/chem/Molecule"),
        "{}",
        findings[0].message
    );
    // Both claim kinds are named, so the message says WHY it was gated.
    assert!(
        findings[0]
            .message
            .contains("declared as a vocabulary term")
            && findings[0]
                .message
                .contains("claims rdfs:isDefinedBy a GMEOW slice"),
        "{}",
        findings[0].message
    );
}

/// R9 fires on `shapes.ttl` too, not only `module.ttl`.
#[test]
fn unregistered_minting_fires_on_the_shape_surface() {
    let tmp = temp_minting_slice(
        "chem",
        "gmeow:Fine a owl:Class ; rdfs:isDefinedBy \
             <https://blackcatinformatics.ca/gmeow/slices/chem> .\n",
        Some(
            "<https://blackcatinformatics.ca/chem/BondShape> a owl:Class ;\n  \
                 rdfs:label \"bond shape\"@x-gmeow-english .\n",
        ),
    );
    let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0].message.contains("shapes.ttl")
            && findings[0].message.contains("chem/BondShape"),
        "{}",
        findings[0].message
    );
}

/// R9 is CLEAN for every registered namespace — the mutation control for the
/// firing test above. Minting the identical shapes into `gmeow:`, `logic:`,
/// `lang:` and `math:` produces zero findings, so the gate discriminates on
/// the namespace and not on the triple pattern.
#[test]
fn unregistered_minting_is_clean_for_every_registered_namespace() {
    for (prefix, ns) in gmeow_ns::TERM_NAMESPACE_PREFIXES {
        let tmp = temp_minting_slice(
            "registered",
            &format!(
                "{prefix}:Molecule\n  a owl:Class ;\n  rdfs:isDefinedBy \
                     <https://blackcatinformatics.ca/gmeow/slices/registered> ;\n  \
                     rdfs:label \"molecule\"@x-gmeow-english .\n"
            ),
            None,
        );
        let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
        assert!(
            findings.is_empty(),
            "{ns} is registered, so minting into it must be clean: {findings:?}"
        );
    }
}

/// R9 does NOT fire on a FOREIGN term redeclared locally so it validates
/// (`skos:definition a owl:AnnotationProperty`). GMEOW does not mint it,
/// purrdf never treats it as owned, and reporting it would be a false claim
/// about someone else's vocabulary.
#[test]
fn unregistered_minting_ignores_a_locally_redeclared_foreign_term() {
    let tmp = temp_minting_slice(
        "kernel",
        "skos:definition a owl:AnnotationProperty .\n\
             <http://purl.org/dc/terms/created> a owl:AnnotationProperty .\n\
             <http://www.w3.org/ns/lemon/ontolex#LexicalEntry> a owl:Class .\n",
        None,
    );
    let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
    assert!(
        findings.is_empty(),
        "a redeclared foreign term is described, not minted: {findings:?}"
    );
}

/// A foreign-authority IRI that nonetheless claims `rdfs:isDefinedBy` a GMEOW
/// slice IS gated: the ownership claim is exactly what purrdf drops, whatever
/// the authority.
#[test]
fn unregistered_minting_fires_on_a_foreign_iri_claiming_gmeow_ownership() {
    let tmp = temp_minting_slice(
        "kernel",
        "<http://example.org/borrowed/Term> a owl:Class ;\n  rdfs:isDefinedBy \
             <https://blackcatinformatics.ca/gmeow/slices/kernel> .\n",
        None,
    );
    let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0]
            .message
            .contains("http://example.org/borrowed/Term"),
        "{}",
        findings[0].message
    );
}

/// An A-BOX INDIVIDUAL minted under a separate GMEOW authority path is NOT a
/// vocabulary term and is not gated, even though it claims
/// `rdfs:isDefinedBy` a GMEOW slice. This is the live `slices/core/affect`
/// shape (`gmeow-registry/…` classifier-label identities); purrdf's
/// `declared_terms` trigger is the vocabulary typing, and this gate mirrors
/// it exactly rather than inventing a stricter rule about instance identity.
#[test]
fn unregistered_minting_ignores_an_abox_individual_under_a_registry_path() {
    let tmp = temp_minting_slice(
        "affect",
        "gmeow:AffectLabelSet a owl:Class ; rdfs:isDefinedBy \
             <https://blackcatinformatics.ca/gmeow/slices/affect> .\n\
             <https://blackcatinformatics.ca/gmeow-registry/labelset/GoEmotions>\n  \
             a gmeow:AffectLabelSet ;\n  rdfs:isDefinedBy \
             <https://blackcatinformatics.ca/gmeow/slices/affect> ;\n  \
             rdfs:label \"GoEmotions\"@x-gmeow-english .\n",
        None,
    );
    let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
    assert!(
        findings.is_empty(),
        "an A-Box individual is instance identity, not a minted term: {findings:?}"
    );

    // …but the SAME IRI typed as a vocabulary term IS gated, so the
    // discrimination is on the typing and not on the namespace path.
    let tmp = temp_minting_slice(
        "affect",
        "<https://blackcatinformatics.ca/gmeow-registry/labelset/GoEmotions>\n  \
             a owl:Class ;\n  rdfs:isDefinedBy \
             <https://blackcatinformatics.ca/gmeow/slices/affect> .\n",
        None,
    );
    let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(
        findings[0]
            .message
            .contains("gmeow-registry/labelset/GoEmotions"),
        "{}",
        findings[0].message
    );
}

/// A subject that merely APPEARS in a module — no vocabulary typing, no
/// ownership claim — is not a mint and is not gated.
#[test]
fn unregistered_minting_ignores_a_merely_referenced_iri() {
    let tmp = temp_minting_slice(
        "kernel",
        "gmeow:Thing a owl:Class ; rdfs:seeAlso <http://purl.obolibrary.org/obo/BFO_0000001> .\n\
             <http://purl.obolibrary.org/obo/BFO_0000001> rdfs:label \"entity\"@x-gmeow-english .\n",
        None,
    );
    let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
    assert!(
        findings.is_empty(),
        "a referenced/annotated external IRI is not a mint: {findings:?}"
    );
}

/// The gate's namespace set is [`gmeow_ns::TERM_NAMESPACES`] itself, not a
/// second copy — so registering a namespace there is the ONE edit that makes
/// the gate accept mints into it.
#[test]
fn unregistered_minting_reports_the_registered_set_it_keyed_on() {
    let tmp = temp_minting_slice(
        "chem",
        "<https://blackcatinformatics.ca/chem/Molecule> a owl:Class .\n",
        None,
    );
    let findings = registered_minting_namespace_findings(tmp.path()).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    for ns in gmeow_ns::TERM_NAMESPACES {
        assert!(
            findings[0].message.contains(ns),
            "the message must name every registered namespace; missing {ns}"
        );
    }
}

const DOCS_ENTITIES_OWNER: &str = "https://blackcatinformatics.ca/gmeow/slices/entities";
const DOCS_LOGIC_OWNER: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";

#[test]
fn docs_authority_sources_include_moduleless_slice_shapes() {
    let tmp = tempfile::tempdir().expect("temporary repository");
    let slice = tmp.path().join("slices/profiles/selection");
    std::fs::create_dir_all(&slice).expect("create module-less slice");
    std::fs::write(slice.join("manifest.ttl"), b"# discovery marker\n")
        .expect("write slice manifest");
    let shapes = slice.join("shapes.ttl");
    std::fs::write(&shapes, b"# authored shape authority\n").expect("write slice shapes");

    let sources = docs_authority_source_files(tmp.path()).expect("discover authority sources");
    assert!(
        sources.contains(&shapes),
        "manifest-declared shapes must remain an authority source even when their slice has \
             no module.ttl: {sources:#?}"
    );
}

fn docs_test_ownership(terms: &[(&str, &str)]) -> DocsTermOwnership {
    let mut by_term: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (term, owner) in terms {
        by_term
            .entry(format!("{GMEOW_NS}{term}"))
            .or_default()
            .insert((*owner).to_owned());
    }
    let owners = by_term.values().flatten().cloned().collect();
    DocsTermOwnership { by_term, owners }
}

fn docs_test_authority(
    imports: &[&str],
    declares: &[&str],
) -> BTreeMap<String, DocsDocumentAuthority> {
    BTreeMap::from([(
        "docs/example.md".to_owned(),
        DocsDocumentAuthority {
            imports: imports.iter().map(|value| (*value).to_owned()).collect(),
            declares: declares.iter().map(|value| (*value).to_owned()).collect(),
            resolves: BTreeMap::new(),
        },
    )])
}

fn docs_test_findings(
    markdown: &str,
    authorities: &BTreeMap<String, DocsDocumentAuthority>,
    ownership: &DocsTermOwnership,
) -> Vec<Finding> {
    detect_docs_undeclared_terms(
        &[(PathBuf::from("docs/example.md"), markdown.to_owned())],
        authorities,
        ownership,
        Path::new(""),
    )
}

#[test]
fn docs_term_authority_accepts_an_explicit_owner_import() {
    let ownership = docs_test_ownership(&[("Person", DOCS_ENTITIES_OWNER)]);
    let findings = docs_test_findings(
        "Use `gmeow:Person`.",
        &docs_test_authority(&[DOCS_ENTITIES_OWNER], &[]),
        &ownership,
    );
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn docs_term_authority_disambiguates_a_multiply_owned_term_with_one_import() {
    let ownership = docs_test_ownership(&[
        ("SharedTerm", DOCS_ENTITIES_OWNER),
        ("SharedTerm", DOCS_LOGIC_OWNER),
    ]);
    let findings = docs_test_findings(
        "Use `gmeow:SharedTerm`.",
        &docs_test_authority(&[DOCS_ENTITIES_OWNER], &[]),
        &ownership,
    );
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn docs_term_authority_rejects_multiple_matching_imports_as_ambiguous() {
    let ownership = docs_test_ownership(&[
        ("SharedTerm", DOCS_ENTITIES_OWNER),
        ("SharedTerm", DOCS_LOGIC_OWNER),
    ]);
    let findings = docs_test_findings(
        "Use `gmeow:SharedTerm`.",
        &docs_test_authority(&[DOCS_ENTITIES_OWNER, DOCS_LOGIC_OWNER], &[]),
        &ownership,
    );
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(findings[0].message.contains("ambiguously"));
    assert!(findings[0].message.contains("select its exact owner"));
}

#[test]
fn docs_term_authority_accepts_an_explicit_resolution_for_ambiguous_imports() {
    let ownership = docs_test_ownership(&[
        ("SharedTerm", DOCS_ENTITIES_OWNER),
        ("SharedTerm", DOCS_LOGIC_OWNER),
    ]);
    let mut authorities = docs_test_authority(&[DOCS_ENTITIES_OWNER, DOCS_LOGIC_OWNER], &[]);
    authorities
        .get_mut("docs/example.md")
        .expect("test document authority")
        .resolves
        .insert(
            format!("{GMEOW_NS}SharedTerm"),
            DOCS_ENTITIES_OWNER.to_owned(),
        );
    let findings = docs_test_findings("Use `gmeow:SharedTerm`.", &authorities, &ownership);
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn docs_term_authority_accepts_an_exact_document_local_declaration() {
    let term = format!("{GMEOW_NS}IllustrativeOnly");
    let findings = docs_test_findings(
        "Use `gmeow:IllustrativeOnly`.",
        &docs_test_authority(&[], &[&term]),
        &DocsTermOwnership::default(),
    );
    assert!(findings.is_empty(), "{findings:#?}");
}

#[test]
fn docs_term_authority_rejects_a_typo() {
    let findings = docs_test_findings(
        "```turtle\nex:a gmeow:TotallyUndeclaredXyz ex:b .\n```",
        &docs_test_authority(&[], &[]),
        &DocsTermOwnership::default(),
    );
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(findings[0].message.contains("no authored rdfs:isDefinedBy"));
}

#[test]
fn docs_term_authority_rejects_a_real_term_from_an_unimported_slice() {
    let ownership = docs_test_ownership(&[
        ("Person", DOCS_ENTITIES_OWNER),
        ("BareEntitySortalAdviceConstraint", DOCS_LOGIC_OWNER),
    ]);
    let findings = docs_test_findings(
        "Use `gmeow:Person`.",
        &docs_test_authority(&[DOCS_LOGIC_OWNER], &[]),
        &ownership,
    );
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(findings[0].message.contains("Person"));
    assert!(findings[0].message.contains(DOCS_ENTITIES_OWNER));
}

#[test]
fn docs_term_authority_discovers_and_rejects_a_wrong_real_corpus_term() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/validate lives two levels under the repository root");
    let ownership = docs_term_ownership(root).expect("discover authored term owners");
    let person = format!("{GMEOW_NS}Person");
    assert_eq!(
        ownership.by_term.get(&person),
        Some(&BTreeSet::from([DOCS_ENTITIES_OWNER.to_owned()])),
        "the negative control must use the real uniquely owned gmeow:Person declaration"
    );
    assert!(
        ownership.owners.contains(DOCS_LOGIC_OWNER),
        "the unrelated import must itself be a real authored authority"
    );

    let findings = docs_test_findings(
        "Use `gmeow:Person`.",
        &docs_test_authority(&[DOCS_LOGIC_OWNER], &[]),
        &ownership,
    );
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(findings[0].message.contains(DOCS_ENTITIES_OWNER));
}

#[test]
fn docs_term_authority_requires_an_entry_for_each_referencing_document() {
    let ownership = docs_test_ownership(&[("Person", DOCS_ENTITIES_OWNER)]);
    let findings = docs_test_findings("Use `gmeow:Person`.", &BTreeMap::new(), &ownership);
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(findings[0].message.contains("no per-document entry"));
}

#[test]
fn docs_term_authority_manifest_rejects_unknown_imports_and_owned_redeclarations() {
    let root = Path::new("/repo");
    let docs = vec![PathBuf::from("/repo/docs/example.md")];
    let ownership = docs_test_ownership(&[("Person", DOCS_ENTITIES_OWNER)]);
    let unknown = ("version = 1\n\n[[document]]\npath = \"docs/example.md\"\n\
             imports = [\"https://example.org/not-an-authored-owner\"]\ndeclares = []\n")
        .to_string();
    let error = parse_docs_authority_text(
        Path::new("/repo/docs/term-authority.toml"),
        &unknown,
        &docs,
        root,
        &ownership,
    )
    .expect_err("an unknown owner import must hard-fail");
    assert!(
        error.message().contains("unknown term authority"),
        "{error}"
    );

    let owned_term = format!("{GMEOW_NS}Person");
    let redeclared = format!(
        "version = 1\n\n[[document]]\npath = \"docs/example.md\"\nimports = []\n\
             declares = [\"{owned_term}\"]\n"
    );
    let error = parse_docs_authority_text(
        Path::new("/repo/docs/term-authority.toml"),
        &redeclared,
        &docs,
        root,
        &ownership,
    )
    .expect_err("an ontology-owned term cannot be laundered through a local declaration");
    assert!(
        error.message().contains("import one of its exact owners"),
        "{error}"
    );
}

#[test]
fn docs_term_authority_manifest_validates_explicit_owner_resolutions() {
    let root = Path::new("/repo");
    let docs = vec![PathBuf::from("/repo/docs/example.md")];
    let provenance_owner = "https://blackcatinformatics.ca/gmeow/slices/provenance";
    let ownership = docs_test_ownership(&[
        ("SharedTerm", DOCS_ENTITIES_OWNER),
        ("SharedTerm", DOCS_LOGIC_OWNER),
        ("SharedTerm", provenance_owner),
        ("Person", DOCS_ENTITIES_OWNER),
    ]);
    let shared = format!("{GMEOW_NS}SharedTerm");
    let valid = format!(
        "version = 1\n\n[[document]]\npath = \"docs/example.md\"\n\
             imports = [\"{DOCS_ENTITIES_OWNER}\", \"{DOCS_LOGIC_OWNER}\"]\n\
             declares = []\nresolves = {{ \"{shared}\" = \"{DOCS_ENTITIES_OWNER}\" }}\n"
    );
    let parsed = parse_docs_authority_text(
        Path::new("/repo/docs/term-authority.toml"),
        &valid,
        &docs,
        root,
        &ownership,
    )
    .expect("an exact resolution may select one of several imported owners");
    assert_eq!(
        parsed["docs/example.md"]
            .resolves
            .get(&shared)
            .map(String::as_str),
        Some(DOCS_ENTITIES_OWNER)
    );

    let person = format!("{GMEOW_NS}Person");
    let redundant = format!(
        "version = 1\n\n[[document]]\npath = \"docs/example.md\"\n\
             imports = [\"{DOCS_ENTITIES_OWNER}\"]\ndeclares = []\n\
             resolves = {{ \"{person}\" = \"{DOCS_ENTITIES_OWNER}\" }}\n"
    );
    let error = parse_docs_authority_text(
        Path::new("/repo/docs/term-authority.toml"),
        &redundant,
        &docs,
        root,
        &ownership,
    )
    .expect_err("a unique import must not carry a stale resolution override");
    assert!(error.message().contains("redundantly resolves"), "{error}");

    let unrelated = format!(
        "version = 1\n\n[[document]]\npath = \"docs/example.md\"\n\
             imports = [\"{DOCS_ENTITIES_OWNER}\", \"{DOCS_LOGIC_OWNER}\"]\n\
             declares = []\nresolves = {{ \"{shared}\" = \"https://example.org/unrelated\" }}\n"
    );
    let error = parse_docs_authority_text(
        Path::new("/repo/docs/term-authority.toml"),
        &unrelated,
        &docs,
        root,
        &ownership,
    )
    .expect_err("a resolution may not select an unrelated authority");
    assert!(error.message().contains("not one of its exact"), "{error}");

    let unimported = format!(
        "version = 1\n\n[[document]]\npath = \"docs/example.md\"\n\
             imports = [\"{DOCS_ENTITIES_OWNER}\", \"{DOCS_LOGIC_OWNER}\"]\n\
             declares = []\nresolves = {{ \"{shared}\" = \"{provenance_owner}\" }}\n"
    );
    let error = parse_docs_authority_text(
        Path::new("/repo/docs/term-authority.toml"),
        &unimported,
        &docs,
        root,
        &ownership,
    )
    .expect_err("a resolution must select an authority imported by that document");
    assert!(error.message().contains("does not import"), "{error}");
}

#[test]
fn docs_term_authority_manifest_rejects_malformed_and_duplicate_declarations() {
    let root = Path::new("/repo");
    let docs = vec![PathBuf::from("/repo/docs/example.md")];
    let ownership = docs_test_ownership(&[("Person", DOCS_ENTITIES_OWNER)]);
    let local_term = format!("{GMEOW_NS}IllustrativeOnly");
    let cases = [
        (
            "missing declares field",
            "version = 1\n\n[[document]]\npath = \"docs/example.md\"\nimports = []\n".to_owned(),
            None,
        ),
        (
            "duplicate document entry",
            "version = 1\n\n[[document]]\npath = \"docs/example.md\"\nimports = []\n\
                 declares = []\n\n[[document]]\npath = \"docs/example.md\"\nimports = []\n\
                 declares = []\n"
                .to_owned(),
            Some("duplicate term-authority entry"),
        ),
        (
            "duplicate local declaration",
            format!(
                "version = 1\n\n[[document]]\npath = \"docs/example.md\"\nimports = []\n\
                     declares = [\"{local_term}\", \"{local_term}\"]\n"
            ),
            Some("declares term"),
        ),
        (
            "duplicate owner import",
            format!(
                "version = 1\n\n[[document]]\npath = \"docs/example.md\"\n\
                     imports = [\"{DOCS_ENTITIES_OWNER}\", \"{DOCS_ENTITIES_OWNER}\"]\n\
                     declares = []\n"
            ),
            Some("imports authority"),
        ),
        (
            "non-GMEOW declaration",
            "version = 1\n\n[[document]]\npath = \"docs/example.md\"\nimports = []\n\
                 declares = [\"https://example.org/not-gmeow\"]\n"
                .to_owned(),
            Some("declares non-GMEOW term"),
        ),
    ];

    for (name, manifest, expected_message) in cases {
        let error = parse_docs_authority_text(
            Path::new("/repo/docs/term-authority.toml"),
            &manifest,
            &docs,
            root,
            &ownership,
        )
        .expect_err(name);
        if let Some(expected_message) = expected_message {
            assert!(
                error.message().contains(expected_message),
                "{name}: {error}"
            );
        }
    }
}

#[test]
fn docs_term_authority_covers_the_committed_document_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/validate lives two levels under the repository root");
    let findings = docs_undeclared_findings(root).expect("evaluate docs term authority");
    assert!(
        findings.is_empty(),
        "committed docs term-authority findings:\n{}",
        findings
            .iter()
            .map(|finding| finding.message.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// ── R7: grounding seam-registry drift ────────────────────────────────────

fn sample_seam_manifest() -> Dataset {
    ds(
        "<https://blackcatinformatics.ca/gmeow/slices/logic> a gmeow:Slice, gmeow:GroundingSlice .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
            <https://blackcatinformatics.ca/gmeow/seam/denotation>\n\
                a gmeow:Seam ;\n\
                rdfs:label \"Denotation seam\"@x-gmeow-english ;\n\
                gmeow:seamDirection [\n\
                    gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;\n\
                    gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>\n\
                ] ;\n\
                gmeow:seamCarryingTerm lang:denotationTarget , lang:denotationKind ;\n\
                gmeow:seamOwningDoc \"LANG-MEANING.md\" .\n",
    )
}

#[test]
fn seam_records_of_reads_label_terms_and_docs() {
    let d = sample_seam_manifest();
    let records = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
    assert_eq!(records.len(), 1);
    let seam = &records[0];
    assert_eq!(seam.name, "Denotation seam");
    assert_eq!(
        seam.carrying_terms,
        BTreeSet::from([
            "lang:denotationKind".to_string(),
            "lang:denotationTarget".to_string(),
        ])
    );
    assert_eq!(
        seam.owning_docs,
        BTreeSet::from(["LANG-MEANING.md".to_string()])
    );
}

#[test]
fn seam_records_of_ignores_a_non_grounding_slice_manifest() {
    // A gmeow:Seam authored on a slice NOT typed gmeow:GroundingSlice must not
    // be picked up (mirrors gmeow_docs::model::is_grounding_slice's gate).
    let d = ds(
        "<https://blackcatinformatics.ca/gmeow/slices/plain> a gmeow:Slice .\n\
                    <https://blackcatinformatics.ca/gmeow/seam/rogue> a gmeow:Seam ; rdfs:label \"Rogue\"@x-gmeow-english .\n",
    );
    let records = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
    assert!(records.is_empty());
}

// ── R7 fixtures ──────────────────────────────────────────────────────────
//
// A TWO-seam registry, because the defect this gate exists to catch is
// per-seam: a page that assigns the right terms/docs/directions to the WRONG
// seam unions to exactly the correct set and is invisible to any comparison
// that pools the seams before checking.

/// A grounding manifest carrying two seams with disjoint terms, docs, and
/// directions — `lang → logic` and `math → logic`.
fn two_seam_manifest() -> Dataset {
    ds(
        "<https://blackcatinformatics.ca/gmeow/slices/logic> a gmeow:Slice, gmeow:GroundingSlice .\n\
            @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
            @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
            @prefix math: <https://blackcatinformatics.ca/math/> .\n\
            <https://blackcatinformatics.ca/gmeow/seam/denotation>\n\
                a gmeow:Seam ;\n\
                rdfs:label \"Denotation seam\"@x-gmeow-english ;\n\
                gmeow:seamDirection [\n\
                    gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;\n\
                    gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>\n\
                ] ;\n\
                gmeow:seamCarryingTerm lang:denotationTarget , lang:denotationKind ;\n\
                gmeow:seamOwningDoc \"LANG-MEANING.md\" .\n\
            <https://blackcatinformatics.ca/gmeow/seam/compilation>\n\
                a gmeow:Seam ;\n\
                rdfs:label \"Compilation seam\"@x-gmeow-english ;\n\
                gmeow:seamDirection [\n\
                    gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/math> ;\n\
                    gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>\n\
                ] ;\n\
                gmeow:seamCarryingTerm math:compilesToLogicTerm ;\n\
                gmeow:seamOwningDoc \"MATHEMATICS-EXPRESSIONS.md\" .\n",
    )
}

/// Wrap table rows in the page frame `gmeow_docs::render::md_seam_registry`
/// emits — intro prose (which names a `gmeow:` CURIE that is NOT a carrying
/// term), the header, the alignment row, then the `## Definitions` section
/// that closes the table region.
fn seam_page(rows: &[&str]) -> String {
    format!(
        "# Grounding seams\n\n\
             The closed set of sanctioned channels; every peered cross-slice reference must \
             land on one rather than riding free on `gmeow:sliceCoFoundationalWith`.\n\n\
             {header}\n\
             | --- | --- | --- | --- |\n\
             {rows}\n\n\
             ## Definitions\n\n\
             ### Denotation seam\n\n\
             Prose that also mentions `lang:denotationTarget` and `LANG-MEANING.md`.\n",
        header = SEAM_TABLE_HEADER,
        rows = rows.join("\n"),
    )
}

/// The Denotation seam's row, direction rendered as bare slice names (the
/// `seam_slice_link` fallback for an unresolvable slice).
const DENOTATION_ROW: &str = "| **Denotation seam** | lang → logic | \
        `lang:denotationKind`, `lang:denotationTarget` | `LANG-MEANING.md` |";
/// The Compilation seam's row, direction and carrying term rendered as the
/// markdown LINKS the real renderer emits for resolvable slices/terms.
const COMPILATION_ROW: &str = "| **Compilation seam** | \
        [math](../slices/math/index.md) → [logic](../slices/logic/index.md) | \
        [`math:compilesToLogicTerm`](../terms/math-compilestologicterm/index.md) | \
        `MATHEMATICS-EXPRESSIONS.md` |";

fn matching_page_text() -> String {
    seam_page(&[DENOTATION_ROW])
}

fn two_seam_page() -> String {
    seam_page(&[COMPILATION_ROW, DENOTATION_ROW])
}

fn two_seams() -> Vec<SeamRecord> {
    seam_records_of(&two_seam_manifest(), Path::new("manifest.ttl")).unwrap()
}

fn drift_messages(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("[{:?}] {}", f.severity, f.message))
        .collect::<Vec<_>>()
        .join("\n")
}

// ── R7 non-vacuity: the clean cases (a gate that always fires is not a gate)

#[test]
fn detect_seam_registry_drift_is_clean_when_page_matches_data() {
    let d = sample_seam_manifest();
    let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
    let findings = detect_seam_registry_drift(&seams, &matching_page_text());
    assert!(
        findings.is_empty(),
        "a page that carries every seam/term/doc/direction must not drift:\n{}",
        drift_messages(&findings)
    );
}

#[test]
fn detect_seam_registry_drift_is_clean_for_two_seams_with_rendered_links() {
    // NON-VACUITY for every negative below: the same fixture, undisturbed, is
    // clean — including the markdown-link forms of both a direction leg and a
    // carrying term, so the parsers are proven to read the REAL render shape.
    let seams = two_seams();
    assert_eq!(seams.len(), 2, "the two-seam fixture must carry two seams");
    let findings = detect_seam_registry_drift(&seams, &two_seam_page());
    assert!(
        findings.is_empty(),
        "the matching two-seam page must not drift:\n{}",
        drift_messages(&findings)
    );
}

// ── R7: per-seam assignment ──────────────────────────────────────────────

#[test]
fn detect_seam_registry_drift_fires_when_the_right_terms_are_on_the_wrong_seam() {
    // THE per-seam defect: both rows together carry exactly the right terms,
    // so any comparison that unions the seams first passes. Each row's terms
    // belong to the OTHER seam.
    let seams = two_seams();
    let swapped_denotation =
        DENOTATION_ROW.replace("`lang:denotationKind`, `lang:denotationTarget`", "MARKER");
    let swapped_compilation = COMPILATION_ROW.replace(
        "[`math:compilesToLogicTerm`](../terms/math-compilestologicterm/index.md)",
        "`lang:denotationKind`, `lang:denotationTarget`",
    );
    let page = seam_page(&[
        &swapped_compilation,
        &swapped_denotation.replace("MARKER", "`math:compilesToLogicTerm`"),
    ]);
    let findings = detect_seam_registry_drift(&seams, &page);
    let text = drift_messages(&findings);
    assert!(
        findings
            .iter()
            .all(|f| f.code == codes::AUTHORING_SEAM_REGISTRY_DRIFT),
        "every finding is a seam-registry drift finding:\n{text}"
    );
    for (seam, term) in [
        ("Denotation seam", "lang:denotationKind"),
        ("Denotation seam", "lang:denotationTarget"),
        ("Compilation seam", "math:compilesToLogicTerm"),
    ] {
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains(seam) && f.message.contains(term)),
            "seam {seam:?} must be reported as missing its own carrying term \
                 {term:?}:\n{text}"
        );
    }
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("Compilation seam")
                && f.message.contains("lang:denotationTarget")
                && f.message.contains("does not declare")),
        "the Compilation seam's row must be reported for listing a term that seam \
             does not declare:\n{text}"
    );
}

#[test]
fn detect_seam_registry_drift_fires_when_an_owning_doc_lands_on_the_wrong_seam() {
    let seams = two_seams();
    let page = seam_page(&[
        &COMPILATION_ROW.replace("`MATHEMATICS-EXPRESSIONS.md`", "`LANG-MEANING.md`"),
        &DENOTATION_ROW.replace("`LANG-MEANING.md`", "`MATHEMATICS-EXPRESSIONS.md`"),
    ]);
    let findings = detect_seam_registry_drift(&seams, &page);
    let text = drift_messages(&findings);
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("Denotation seam")
                && f.message.contains("LANG-MEANING.md")
                && f.message.contains("missing from that seam's row")),
        "the Denotation seam must be reported for losing its own owning doc:\n{text}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("Compilation seam")
                && f.message.contains("LANG-MEANING.md")
                && f.message.contains("does not declare")),
        "the Compilation seam must be reported for claiming another seam's owning \
             doc:\n{text}"
    );
}

// ── R7: direction legs (never compared at all before) ────────────────────

#[test]
fn detect_seam_registry_drift_fires_on_an_inverted_direction_leg() {
    let seams = two_seams();
    let page = seam_page(&[
        COMPILATION_ROW,
        &DENOTATION_ROW.replace("lang → logic", "logic → lang"),
    ]);
    let findings = detect_seam_registry_drift(&seams, &page);
    let text = drift_messages(&findings);
    assert!(
        findings
            .iter()
            .any(|f| f.code == codes::AUTHORING_SEAM_REGISTRY_DRIFT
                && f.message.contains("Denotation seam")
                && f.message.contains("INVERTED")
                && f.message.contains("lang → logic")),
        "an inverted gmeow:seamFromSlice/seamToSlice leg must be reported as \
             inverted:\n{text}"
    );
}

#[test]
fn detect_seam_registry_drift_fires_on_a_missing_direction_leg() {
    let seams = two_seams();
    let page = seam_page(&[
        &COMPILATION_ROW.replace(
            "[math](../slices/math/index.md) → [logic](../slices/logic/index.md)",
            "",
        ),
        DENOTATION_ROW,
    ]);
    let findings = detect_seam_registry_drift(&seams, &page);
    let text = drift_messages(&findings);
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("Compilation seam")
                && f.message.contains("math → logic")
                && f.message.contains("missing from that seam's row")),
        "a dropped direction leg must be reported:\n{text}"
    );
}

#[test]
fn detect_seam_registry_drift_fires_on_an_extra_direction_leg() {
    let seams = two_seams();
    let page = seam_page(&[
        COMPILATION_ROW,
        &DENOTATION_ROW.replace("lang → logic", "lang → logic; math → logic"),
    ]);
    let findings = detect_seam_registry_drift(&seams, &page);
    let text = drift_messages(&findings);
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("Denotation seam")
                && f.message.contains("math → logic")
                && f.message.contains("does not declare")),
        "a direction leg the seam never declares must be reported:\n{text}"
    );
}

// ── R7: exact identity, never substring ──────────────────────────────────

#[test]
fn detect_seam_registry_drift_matches_a_seam_name_exactly_not_by_substring() {
    // The retired gate asked `page_text.contains(seam.name)`, so a row whose
    // name merely CONTAINED the authored name passed. The seam is not on the
    // page under its own name and must be reported both ways.
    let d = sample_seam_manifest();
    let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
    let page = seam_page(&[
        &DENOTATION_ROW.replace("**Denotation seam**", "**Denotation seam (deprecated)**")
    ]);
    let findings = detect_seam_registry_drift(&seams, &page);
    let text = drift_messages(&findings);
    assert!(
        findings.iter().any(|f| f
            .message
            .contains("seam \"Denotation seam\" is declared in the grounding manifests")),
        "the authored seam must be reported as absent from the page:\n{text}"
    );
    assert!(
        findings.iter().any(|f| f
            .message
            .contains("\"Denotation seam (deprecated)\", which no gmeow:Seam individual")),
        "the unbacked page row must be reported:\n{text}"
    );
}

#[test]
fn detect_seam_registry_drift_matches_carrying_terms_exactly_not_by_prefix() {
    // The file's standing discipline (NORMS_EXTENSION_TERMS): `normIssuer`
    // must never match `normIssuerRole`. A page that lengthens a carrying
    // term's local name is drift in BOTH directions.
    let d = sample_seam_manifest();
    let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
    let page =
        seam_page(&[&DENOTATION_ROW.replace("`lang:denotationKind`", "`lang:denotationKindRole`")]);
    let findings = detect_seam_registry_drift(&seams, &page);
    let text = drift_messages(&findings);
    assert!(
        findings.iter().any(|f| f.message.contains(
            "declares carrying term \
                 lang:denotationKind,"
        )),
        "the authored term must be reported missing, not swallowed by its longer \
             page-side namesake:\n{text}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("lang:denotationKindRole")
                && f.message.contains("does not declare")),
        "the longer page-side term must be reported as unbacked:\n{text}"
    );
}

// ── R7: the original single-field negatives, retained ────────────────────

#[test]
fn detect_seam_registry_drift_fires_when_a_carrying_term_is_missing_from_the_page() {
    let d = sample_seam_manifest();
    let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
    let page = matching_page_text().replace(", `lang:denotationTarget`", "");
    let findings = detect_seam_registry_drift(&seams, &page);
    assert!(
        findings
            .iter()
            .any(|f| f.code == codes::AUTHORING_SEAM_REGISTRY_DRIFT
                && f.message.contains("lang:denotationTarget")),
        "a data-side carrying term missing from the page must be flagged:\n{}",
        drift_messages(&findings)
    );
}

#[test]
fn detect_seam_registry_drift_fires_on_an_orphan_page_term() {
    let d = sample_seam_manifest();
    let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
    let page = matching_page_text().replace(
        "`lang:denotationTarget`",
        "`lang:denotationTarget`, `logic:NotARealCarryingTerm`",
    );
    let findings = detect_seam_registry_drift(&seams, &page);
    assert!(
        findings
            .iter()
            .any(|f| f.code == codes::AUTHORING_SEAM_REGISTRY_DRIFT
                && f.message.contains("logic:NotARealCarryingTerm")),
        "a page-side term unbacked by data must be flagged:\n{}",
        drift_messages(&findings)
    );
}

#[test]
fn detect_seam_registry_drift_fires_when_an_owning_doc_is_missing() {
    let d = sample_seam_manifest();
    let seams = seam_records_of(&d, Path::new("manifest.ttl")).unwrap();
    let page = matching_page_text().replace("`LANG-MEANING.md`", "(no doc)");
    let findings = detect_seam_registry_drift(&seams, &page);
    assert!(
        findings
            .iter()
            .any(|f| f.code == codes::AUTHORING_SEAM_REGISTRY_DRIFT
                && f.message.contains("LANG-MEANING.md")),
        "a data-side owning doc missing from the page must be flagged:\n{}",
        drift_messages(&findings)
    );
}

// ── R7: structurally unusable pages are drift, not silence ───────────────

#[test]
fn detect_seam_registry_drift_fires_when_the_page_carries_no_table() {
    let seams = two_seams();
    let findings = detect_seam_registry_drift(
        &seams,
        "# Grounding seams\n\nNo grounding seams are declared in this model.\n",
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("carries no seam table")),
        "a page with no table, against a non-empty registry, must be drift:\n{}",
        drift_messages(&findings)
    );
}

#[test]
fn detect_seam_registry_drift_fires_on_a_structurally_malformed_row() {
    let seams = two_seams();
    // A row missing its Owning doc column entirely.
    let page = seam_page(&[
        COMPILATION_ROW,
        "| **Denotation seam** | lang → logic | `lang:denotationKind` |",
    ]);
    let findings = detect_seam_registry_drift(&seams, &page);
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("unparsable") && f.message.contains("columns")),
        "a row with the wrong column count must be reported, never skipped:\n{}",
        drift_messages(&findings)
    );
}

#[test]
fn detect_seam_registry_drift_fires_when_the_page_renders_a_seam_twice() {
    let seams = two_seams();
    let page = seam_page(&[COMPILATION_ROW, DENOTATION_ROW, DENOTATION_ROW]);
    let findings = detect_seam_registry_drift(&seams, &page);
    assert!(
        findings.iter().any(|f| f
            .message
            .contains("renders 2 rows for seam \"Denotation seam\"")),
        "a duplicated row makes the projection ambiguous and must be reported:\n{}",
        drift_messages(&findings)
    );
}

// ── R7: the on-disk wrapper never reports a silent "clean" ───────────────

/// A temp project root carrying a grounding manifest with `seams` authored.
fn temp_seam_repo(manifest_body: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("temp project root");
    let slice_dir = tmp.path().join("slices/grounding/logic");
    std::fs::create_dir_all(&slice_dir).unwrap();
    std::fs::write(
        slice_dir.join("manifest.ttl"),
        format!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                 @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
                 @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
                 <https://blackcatinformatics.ca/gmeow/slices/logic> a gmeow:Slice, \
                 gmeow:GroundingSlice ;\n\
                   gmeow:sliceTier gmeow:tierCore ;\n\
                   rdfs:label \"logic\"@x-gmeow-english .\n\
                 {manifest_body}"
        ),
    )
    .unwrap();
    tmp
}

/// The one seam `temp_seam_repo` authors when handed [`DENOTATION_SEAM_TTL`].
const DENOTATION_SEAM_TTL: &str = "<https://blackcatinformatics.ca/gmeow/seam/denotation>\n\
             a gmeow:Seam ;\n\
             rdfs:label \"Denotation seam\"@x-gmeow-english ;\n\
             gmeow:seamDirection [\n\
                 gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;\n\
                 gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>\n\
             ] ;\n\
             gmeow:seamCarryingTerm lang:denotationTarget , lang:denotationKind ;\n\
             gmeow:seamOwningDoc \"LANG-MEANING.md\" .\n";

#[test]
fn seam_registry_drift_findings_refuses_a_vacuous_registry() {
    // Zero seams discovered means the comparison certifies nothing, so the
    // detector reports an Error — never a clean (empty) verdict.
    let tmp = temp_seam_repo("");
    let findings = seam_registry_drift_findings(tmp.path(), &tmp.path().join("slices")).unwrap();
    assert_eq!(findings.len(), 1, "{}", drift_messages(&findings));
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(
        findings[0].message.contains("certifies nothing"),
        "{}",
        findings[0].message
    );
}

#[test]
fn require_non_vacuous_corpus_refuses_a_seam_free_corpus() {
    // The aggregator-level floor: `authoring_integrity_findings` must not run at
    // all against a corpus whose grounding manifests declare no seam. Driven on a
    // temp root so it needs no `generated/` tree; the seam floor is the LAST of
    // the four floors, so reaching it here would require the earlier three to
    // pass — assert instead on the error text of whichever floor fires, and pin
    // the seam floor directly through its own reader.
    let tmp = temp_seam_repo("");
    let seams = seam_registry_of_slices(&tmp.path().join("slices")).unwrap();
    assert!(
        seams.is_empty(),
        "the fixture must genuinely declare no seam"
    );
    // …and the real corpus's grounding tree is genuinely non-empty, so the floor
    // is not a permanent tripwire.
    let repo_slices = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/validate lives two levels under the repo root")
        .join("slices");
    let real = seam_registry_of_slices(&repo_slices).unwrap();
    assert!(
        !real.is_empty(),
        "the committed grounding manifests must declare at least one gmeow:Seam, \
             otherwise the authoring-integrity seam floor can never be met"
    );
}

#[test]
fn seam_registry_drift_findings_reports_not_compared_when_no_docs_tree_exists() {
    // `ontology-docs/` is written only by a docs-selected sync, so on the
    // `make validate` / `make check` path it is genuinely absent. The gate must
    // say NOT COMPARED — an empty (clean) verdict here would certify nothing.
    let tmp = temp_seam_repo(DENOTATION_SEAM_TTL);
    assert!(!tmp.path().join(SEAM_REGISTRY_PAGE_PATH).exists());
    assert!(!tmp.path().join(ONTOLOGY_DOCS_DIR).exists());

    let findings = seam_registry_drift_findings(tmp.path(), &tmp.path().join("slices")).unwrap();
    assert_eq!(
        findings.len(),
        1,
        "exactly one NOT COMPARED record:\n{}",
        drift_messages(&findings)
    );
    assert!(
        findings[0].message.contains("NOT COMPARED")
            && findings[0]
                .message
                .contains("make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs")
            && findings[0].message.contains("doc-lint"),
        "the record must name the state, the remedy, and the unconditional leg: {}",
        findings[0].message
    );
    assert_eq!(
        findings[0].severity,
        Severity::Warning,
        "an unmaterialized on-demand docs tree is not itself an authoring defect, so \
             it must not hard-fail make validate"
    );
}

#[test]
fn seam_registry_drift_findings_errors_when_a_materialized_docs_tree_drops_the_page() {
    // A docs render DID happen here; a missing seam page is a lost projection.
    let tmp = temp_seam_repo(DENOTATION_SEAM_TTL);
    std::fs::create_dir_all(tmp.path().join(ONTOLOGY_DOCS_DIR).join("terms")).unwrap();
    assert!(!tmp.path().join(SEAM_REGISTRY_PAGE_PATH).exists());

    let findings = seam_registry_drift_findings(tmp.path(), &tmp.path().join("slices")).unwrap();
    assert_eq!(findings.len(), 1, "{}", drift_messages(&findings));
    assert_eq!(findings[0].severity, Severity::Error);
    assert_eq!(findings[0].code, codes::AUTHORING_SEAM_REGISTRY_DRIFT);
    assert!(
        findings[0]
            .message
            .contains("carries no seam-registry page"),
        "{}",
        findings[0].message
    );
}

#[test]
fn seam_registry_drift_findings_compares_a_materialized_page() {
    // The on-disk leg really compares: the SAME repo is clean against a
    // matching page and fires against a drifted one (non-vacuity + teeth).
    let tmp = temp_seam_repo(DENOTATION_SEAM_TTL);
    let page_path = tmp.path().join(SEAM_REGISTRY_PAGE_PATH);
    std::fs::create_dir_all(page_path.parent().unwrap()).unwrap();

    std::fs::write(&page_path, matching_page_text()).unwrap();
    let clean = seam_registry_drift_findings(tmp.path(), &tmp.path().join("slices")).unwrap();
    assert!(
        clean.is_empty(),
        "a materialized page that matches the data must be clean:\n{}",
        drift_messages(&clean)
    );

    std::fs::write(
        &page_path,
        matching_page_text().replace("lang → logic", "logic → lang"),
    )
    .unwrap();
    let drifted = seam_registry_drift_findings(tmp.path(), &tmp.path().join("slices")).unwrap();
    assert!(
        drifted
            .iter()
            .any(|f| f.severity == Severity::Error && f.message.contains("INVERTED")),
        "an inverted leg on the materialized page must hard-fail:\n{}",
        drift_messages(&drifted)
    );
}

// ── R7: markdown cell parsing ────────────────────────────────────────────

#[test]
fn split_row_cells_keeps_escaped_pipes_inside_their_cell() {
    // `md_escape`/`code_escape` render a cell's own pipe as `\|`; splitting on
    // a raw `|` would shear the row and shift every later column.
    let cells = split_row_cells(r"| **A \| B** | lang → logic | `lang:x` | `D.md` |");
    assert_eq!(cells.len(), 6, "{cells:?}");
    assert_eq!(cells[1].trim(), r"**A \| B**");
    assert_eq!(md_unescape(cells[1].trim()), "**A | B**");
}

#[test]
fn slice_token_reads_the_slug_from_a_rendered_slice_link() {
    assert_eq!(
        slice_token_of_cell("[Logic grounding](../slices/logic/index.md)"),
        "logic",
        "the identity is the href slug, never the display title"
    );
    assert_eq!(slice_token_of_cell("logic"), "logic");
    assert_eq!(
        slice_token_of_iri("https://blackcatinformatics.ca/gmeow/slices/logic"),
        "logic"
    );
}
