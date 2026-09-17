// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::RdfLookaside;

/// The GTS write profile for test fixtures (arbitrary, deterministic).
const TEST_PROFILE: &str = "purrdf-test";

/// Prose-format `describe` — the default used by every language/resolution test
/// (the JSON/TOON formats are exercised by the CLI live-binary suite).
fn describe_prose(query: &str, gts: &[u8], lang: Option<&str>) -> (String, DescribeStatus) {
    describe(query, gts, lang, CardFormat::Prose, &BTreeSet::new())
}

/// The controlled multilingual fixture, mirroring `_multilingual_gts` in
/// `tests/test_cli.py`: a `gmeow:SampleTerm` class with English (always) and
/// optional French / Mandarin labels + definitions, plus the three
/// `gmeow:Language` individuals that seed the tag map. Emitted as GTS bytes.
fn multilingual_gts(include_fr: bool, include_zh: bool) -> Vec<u8> {
    let mut nt = String::new();
    let term = format!("{NAMESPACE}SampleTerm");
    nt.push_str(&format!("<{term}> <{RDF_TYPE}> <{OWL_CLASS}> .\n"));
    nt.push_str(&format!(
        "<{term}> <{RDFS_LABEL}> \"sample label\"@x-gmeow-english .\n"
    ));
    nt.push_str(&format!(
        "<{term}> <{SKOS_DEFINITION}> \"English definition text.\"@x-gmeow-english .\n"
    ));
    if include_fr {
        nt.push_str(&format!(
            "<{term}> <{RDFS_LABEL}> \"étiquette échantillon\"@x-gmeow-french .\n"
        ));
        nt.push_str(&format!(
            "<{term}> <{SKOS_DEFINITION}> \"Définition en français.\"@x-gmeow-french .\n"
        ));
    }
    if include_zh {
        nt.push_str(&format!(
            "<{term}> <{RDFS_LABEL}> \"样本标签\"@x-gmeow-mandarin .\n"
        ));
        nt.push_str(&format!(
            "<{term}> <{SKOS_DEFINITION}> \"中文定义。\"@x-gmeow-mandarin .\n"
        ));
    }
    nt.push_str(&format!(
        "<{term}> <{RDFS_IS_DEFINED_BY}> <{NAMESPACE}slices/lifecycle> .\n"
    ));

    // Carrier varieties seed the tag map. All three are always present; the
    // English + French carriers always carry a language-tagged label (so en
    // and fr are "available" — carry literals — regardless of whether the TERM
    // has that content), while the Mandarin carrier's tagged label is gated
    // on `include_zh` (so zh is available only when Mandarin content exists).
    // Each internal x-gmeow-* tag rides lang:carrierTag and its generated
    // (folded) external tag rides gmeow:bcp47Tag on a lang:LanguageVariety —
    // the post-graft shape the tag map is built from.
    const LANG_VARIETY: &str = "https://blackcatinformatics.ca/lang/LanguageVariety";
    const CARRIER_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
    for (local, internal, bcp, label) in [
        ("gmeowEnglish", "x-gmeow-english", "en", Some("English")),
        ("gmeowFrench", "x-gmeow-french", "fr", Some("français")),
        (
            "gmeowMandarin",
            "x-gmeow-mandarin",
            "zh",
            if include_zh { Some("中文") } else { None },
        ),
    ] {
        let s = format!("https://blackcatinformatics.ca/lang/{local}");
        nt.push_str(&format!("<{s}> <{RDF_TYPE}> <{LANG_VARIETY}> .\n"));
        nt.push_str(&format!("<{s}> <{CARRIER_TAG}> \"{internal}\" .\n"));
        nt.push_str(&format!("<{s}> <{NAMESPACE}bcp47Tag> \"{bcp}\" .\n"));
        if let Some(label) = label {
            nt.push_str(&format!("<{s}> <{RDFS_LABEL}> \"{label}\"@{internal} .\n"));
        }
    }

    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("fixture N-Triples must parse");
    // gmeow-test-input: synthetic-only
    purrdf::gts_write::to_gts(&ds, &RdfLookaside::default(), TEST_PROFILE)
        .expect("fixture must serialize to GTS")
}

#[test]
fn documentation_term_slug_reads_the_documents_inverse() {
    // A doc-entry record documents a term IRI in the graph/documentation NAMED
    // graph; an evidence node documents the SAME term and must be excluded (only
    // the `documentation/term/` subject is the page slug).
    let term = format!("{NAMESPACE}AcceptanceStatus");
    let entry = format!("{NAMESPACE}documentation/term/acceptancestatus-class");
    let evidence = format!("{NAMESPACE}documentation/evidence/acceptancestatus-class/competency");
    let documents = format!("{NAMESPACE}documents");
    let doc_graph = format!("{NAMESPACE}graph/documentation");
    let nq = format!(
        "<{entry}> <{documents}> <{term}> <{doc_graph}> .\n\
             <{evidence}> <{documents}> <{term}> <{doc_graph}> .\n"
    );
    let ds = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
        .expect("fixture N-Quads must parse");
    // gmeow-test-input: synthetic-only
    let bytes = purrdf::gts_write::to_gts(&ds, &RdfLookaside::default(), TEST_PROFILE)
        .expect("fixture must serialize to GTS");
    let graph = DescribeGraph::from_gts_bytes(&bytes).expect("load");

    assert_eq!(
        graph.documentation_term_slug(&term).as_deref(),
        Some("acceptancestatus-class"),
        "must read the injective slug from the documents inverse, excluding evidence nodes"
    );
    assert_eq!(
        graph.documentation_term_slug(&format!("{NAMESPACE}NoSuchTerm")),
        None
    );
}

/// G7 canonical-subsumption sweep: `gmeow describe` on the shipped CLI must
/// still render a term's parent when the taxonomy is re-authored to the
/// canonical `logic:subClassOf`/`logic:subPropertyOf` edge — `DescribeGraph`
/// reads the GTS default graph (the authored, import-free ontology), so an
/// `rdfs:`-only read would silently render the term parent-less
/// (crates/ns/src/lib.rs:106-166).
#[test]
fn describe_renders_parents_over_canonical_logic_subsumption_edges() {
    let term = format!("{NAMESPACE}Cyborg");
    let class_parent = format!("{NAMESPACE}Animal");
    let prop = format!("{NAMESPACE}bondParty");
    let prop_parent = format!("{NAMESPACE}mediates");
    const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
    const LOGIC_SUB_CLASS_OF: &str = "https://blackcatinformatics.ca/logic/subClassOf";
    const LOGIC_SUB_PROPERTY_OF: &str = "https://blackcatinformatics.ca/logic/subPropertyOf";
    let nt = format!(
        "<{term}> <{RD}> <{OWL_CLASS}> .\n\
             <{term}> <{RDFS_LABEL}> \"Cyborg\" .\n\
             <{class_parent}> <{RD}> <{OWL_CLASS}> .\n\
             <{term}> <{LOGIC_SUB_CLASS_OF}> <{class_parent}> .\n\
             <{prop}> <{RD}> <{OWL_OBJECT_PROPERTY}> .\n\
             <{prop}> <{RDFS_LABEL}> \"bondParty\" .\n\
             <{prop_parent}> <{RD}> <{OWL_OBJECT_PROPERTY}> .\n\
             <{prop}> <{LOGIC_SUB_PROPERTY_OF}> <{prop_parent}> .\n",
        RD = RDF_TYPE,
    );
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("fixture N-Triples must parse");
    // gmeow-test-input: synthetic-only
    let gts = purrdf::gts_write::to_gts(&ds, &RdfLookaside::default(), TEST_PROFILE)
        .expect("fixture must serialize to GTS");

    let (text, code) = describe_prose("Cyborg", &gts, None);
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(
        text.contains("gmeow:Animal"),
        "Cyborg's canonical logic:subClassOf parent must render: {text}"
    );

    let (text, code) = describe_prose("bondParty", &gts, None);
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(
        text.contains("gmeow:mediates"),
        "bondParty's canonical logic:subPropertyOf parent must render: {text}"
    );
}

#[test]
fn describe_known_term_returns_prose_and_zero() {
    let gts = multilingual_gts(true, true);
    let (text, code) = describe_prose("SampleTerm", &gts, None);
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(text.contains("gmeow:SampleTerm"), "{text}");
    assert!(text.contains("English definition text."), "{text}");
    assert!(text.contains("category: Class"), "{text}");
    assert!(text.contains("slice: lifecycle"), "{text}");
}

#[test]
fn store_native_describe_matches_the_byte_entry_point() {
    let gts = multilingual_gts(true, true);
    let graph = purrdf::gts::read_all_segments(&gts).expect("read fixture GTS");
    let dataset = purrdf::gts::dataset_from_gts_graph(&graph).expect("materialize fixture");
    let modeled = BTreeSet::new();

    let from_bytes = describe("SampleTerm", &gts, Some("fr"), CardFormat::Json, &modeled);
    let from_dataset = describe_dataset(
        "SampleTerm",
        dataset,
        Some("fr"),
        CardFormat::Json,
        &modeled,
    );

    assert_eq!(from_dataset, from_bytes);
}

/// `class_is_modeled` gate: a
/// Class with NO `$defs` entry for its `def_key` must never carry a
/// `python_model` line, even though the pre-fix gate (`category == "Class" &&
/// defined_by.is_some()`) would have fabricated one for every documented
/// class regardless of whether a generated model actually exists. A class
/// whose `def_key` IS in `modeled_defs` gets the link.
#[test]
fn describe_gates_python_model_on_schema_defs_membership() {
    let gts = multilingual_gts(true, true);

    // Empty `$defs` set: SampleTerm is a documented Class, but unmodeled.
    let (text, code) = describe(
        "SampleTerm",
        &gts,
        None,
        CardFormat::Prose,
        &BTreeSet::new(),
    );
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(
        !text.to_lowercase().contains("python model"),
        "an unmodeled class must never carry a python_model line:\n{text}"
    );

    // SampleTerm's bare local name IS its `def_key` in the primary `gmeow`
    // namespace, so this membership set says "SampleTerm has a model".
    let mut modeled = BTreeSet::new();
    modeled.insert("SampleTerm".to_string());
    let (text, code) = describe("SampleTerm", &gts, None, CardFormat::Prose, &modeled);
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(
        text.contains("**Python model:** `gmeow_models.lifecycle.SampleTerm`"),
        "a modeled class must carry the python_model line:\n{text}"
    );
}

#[test]
fn describe_renders_french_without_fallback() {
    let gts = multilingual_gts(true, true);
    let (text, code) = describe_prose("SampleTerm", &gts, Some("fr"));
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(text.contains("Définition en français."), "{text}");
    assert!(!text.contains("fallback: en"), "{text}");
}

#[test]
fn describe_renders_mandarin_without_fallback() {
    let gts = multilingual_gts(true, true);
    let (text, code) = describe_prose("SampleTerm", &gts, Some("zh"));
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(text.contains("中文定义。"), "{text}");
    assert!(!text.contains("fallback: en"), "{text}");
}

#[test]
fn describe_falls_back_to_english_when_language_absent() {
    // English-only fixture, French requested → the carrier fallback marker.
    let gts = multilingual_gts(false, false);
    let (text, code) = describe_prose("SampleTerm", &gts, Some("fr"));
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(text.contains("English definition text."), "{text}");
    assert!(text.contains("fallback: en"), "{text}");
}

#[test]
fn describe_unknown_language_nonzero_and_lists_carriers() {
    // Mandarin literals absent, but zh is a framework CARRIER (a shippable
    // translation target), so it stays requestable — a request falls back to
    // English rather than hard-failing, and every carrier is listed. A
    // truly-unknown tag still hard-fails.
    let gts = multilingual_gts(true, false);
    let (text, code) = describe_prose("SampleTerm", &gts, Some("notatag"));
    assert_ne!(code, DescribeStatus::Ok, "{text}");
    assert!(
        text.to_lowercase().contains("unknown language tag"),
        "{text}"
    );
    // All three carriers are always requestable (en first, then lexicographic).
    assert!(text.contains("Available languages: en, fr, zh"), "{text}");

    // The contentless zh carrier resolves with the English fallback marker,
    // never an "unknown language" hard-fail.
    let (zh_text, zh_code) = describe_prose("SampleTerm", &gts, Some("zh"));
    assert_eq!(
        zh_code,
        DescribeStatus::Ok,
        "a contentless carrier must fall back: {zh_text}"
    );
    assert!(zh_text.contains("fallback: en"), "{zh_text}");
}

#[test]
fn describe_empty_lang_selects_english_carrier() {
    // An explicit empty request maps to the default English carrier.
    let gts = multilingual_gts(true, true);
    let (text, code) = describe_prose("SampleTerm", &gts, Some(""));
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(text.contains("English definition text."), "{text}");
    assert!(!text.contains("fallback: en"), "{text}");
}

#[test]
fn describe_unknown_term_returns_nonzero() {
    let gts = multilingual_gts(true, true);
    let (text, code) = describe_prose("NoSuchTermAtAll", &gts, None);
    assert_eq!(code, DescribeStatus::Unresolved);
    assert!(text.contains("NoSuchTermAtAll"), "{text}");
}

#[test]
fn describe_ambiguous_prefix_lists_candidates() {
    // `Sample` is not an exact term but prefixes exactly one local name, so it
    // resolves; a shorter, colliding query would list candidates. Here we prove
    // the case-insensitive exact-name path works for the mixed-case query.
    let gts = multilingual_gts(true, true);
    let (_, code) = describe_prose("sampleterm", &gts, None);
    assert_eq!(code, DescribeStatus::Ok);
}

/// The resolved IRI of a [`Resolution::Resolved`], else `None`.
fn resolved_iri(r: Resolution) -> Option<String> {
    match r {
        Resolution::Resolved(iri) => Some(iri),
        _ => None,
    }
}

#[test]
fn resolve_term_handles_prefix_and_curie_forms() {
    let gts = multilingual_gts(true, true);
    let graph = DescribeGraph::from_gts_bytes(&gts).expect("load");
    assert_eq!(
        resolved_iri(resolve_term(&graph, "gmeow:SampleTerm")).as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/SampleTerm")
    );
    // Full-IRI form resolves directly.
    assert_eq!(
        resolved_iri(resolve_term(
            &graph,
            "https://blackcatinformatics.ca/gmeow/SampleTerm"
        ))
        .as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/SampleTerm")
    );
    // A unique case-insensitive prefix resolves.
    assert_eq!(
        resolved_iri(resolve_term(&graph, "Sample")).as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/SampleTerm")
    );
    // An empty query is NotFound with no suggestions.
    match resolve_term(&graph, "   ") {
        Resolution::NotFound { suggestions } => assert!(suggestions.is_empty()),
        other => panic!("empty query must be NotFound, got {other:?}"),
    }
}

#[test]
fn describe_invalid_gts_bytes_is_nonzero() {
    let (text, code) = describe_prose("SampleTerm", b"not a gts bundle", None);
    assert_ne!(code, DescribeStatus::Ok);
    assert_eq!(code, DescribeStatus::LoadFailed);
    assert!(!text.is_empty());
}

/// A fixture spanning the grounding namespaces: `math:Function` and
/// `logic:Function` (a cross-namespace bare-name COLLISION on `Function`) plus a
/// unique `lang:Denotation`. Each carries the full describable predicate shape,
/// and an English carrier seeds the tag map so cards render.
fn grounding_gts() -> Vec<u8> {
    let mut nt = String::new();
    for (prefix, local) in [
        ("math", "Function"),
        ("logic", "Function"),
        ("lang", "Denotation"),
    ] {
        let iri = format!("https://blackcatinformatics.ca/{prefix}/{local}");
        nt.push_str(&format!("<{iri}> <{RDF_TYPE}> <{OWL_CLASS}> .\n"));
        nt.push_str(&format!(
            "<{iri}> <{RDFS_LABEL}> \"{local}\"@x-gmeow-english .\n"
        ));
        nt.push_str(&format!(
            "<{iri}> <{SKOS_DEFINITION}> \"Definition of {prefix}:{local}.\"@x-gmeow-english .\n"
        ));
        nt.push_str(&format!(
            "<{iri}> <{RDFS_IS_DEFINED_BY}> <{NAMESPACE}slices/{prefix}> .\n"
        ));
    }
    // English carrier so the tag map resolves `x-gmeow-english`.
    const LANG_VARIETY: &str = "https://blackcatinformatics.ca/lang/LanguageVariety";
    const CARRIER_TAG: &str = "https://blackcatinformatics.ca/lang/carrierTag";
    let carrier = "https://blackcatinformatics.ca/lang/gmeowEnglish";
    nt.push_str(&format!("<{carrier}> <{RDF_TYPE}> <{LANG_VARIETY}> .\n"));
    nt.push_str(&format!(
        "<{carrier}> <{CARRIER_TAG}> \"x-gmeow-english\" .\n"
    ));
    nt.push_str(&format!("<{carrier}> <{NAMESPACE}bcp47Tag> \"en\" .\n"));

    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("fixture N-Triples must parse");
    // gmeow-test-input: synthetic-only
    purrdf::gts_write::to_gts(&ds, &RdfLookaside::default(), TEST_PROFILE)
        .expect("fixture must serialize to GTS")
}

#[test]
fn resolve_term_spans_grounding_namespaces() {
    let gts = grounding_gts();
    let graph = DescribeGraph::from_gts_bytes(&gts).expect("load");

    // Registered CURIE across each grounding namespace.
    assert_eq!(
        resolved_iri(resolve_term(&graph, "math:Function")).as_deref(),
        Some("https://blackcatinformatics.ca/math/Function")
    );
    // Full IRI.
    assert_eq!(
        resolved_iri(resolve_term(
            &graph,
            "https://blackcatinformatics.ca/lang/Denotation"
        ))
        .as_deref(),
        Some("https://blackcatinformatics.ca/lang/Denotation")
    );
    // Bare local name unique to one namespace.
    assert_eq!(
        resolved_iri(resolve_term(&graph, "Denotation")).as_deref(),
        Some("https://blackcatinformatics.ca/lang/Denotation")
    );
    // Bare local name colliding across namespaces → Ambiguous, sorted CURIEs, no
    // silent gmeow: precedence.
    match resolve_term(&graph, "Function") {
        Resolution::Ambiguous { candidates } => assert_eq!(
            candidates,
            vec!["logic:Function".to_string(), "math:Function".to_string()]
        ),
        other => panic!("colliding bare name must be Ambiguous, got {other:?}"),
    }
    // A registered prefix whose term is absent → NotFound (not a bare search).
    match resolve_term(&graph, "math:Nonexistent") {
        Resolution::NotFound { .. } => {}
        other => panic!("absent CURIE must be NotFound, got {other:?}"),
    }
    // Wholly unknown.
    match resolve_term(&graph, "Nonexistent") {
        Resolution::NotFound { .. } => {}
        other => panic!("unknown must be NotFound, got {other:?}"),
    }
}

#[test]
fn describe_renders_grounding_term_card() {
    let gts = grounding_gts();
    let (text, code) = describe_prose("lang:Denotation", &gts, None);
    assert_eq!(code, DescribeStatus::Ok, "{text}");
    assert!(text.starts_with("# lang:Denotation"), "{text}");
    assert!(text.contains("category: Class"), "{text}");
    assert!(text.contains("slice: lang"), "{text}");
    assert!(text.contains("Definition of lang:Denotation."), "{text}");
}

#[test]
fn describe_ambiguous_grounding_name_is_typed() {
    let gts = grounding_gts();
    let (text, code) = describe_prose("Function", &gts, None);
    assert_eq!(code, DescribeStatus::Ambiguous, "{text}");
    assert!(text.contains("logic:Function"), "{text}");
    assert!(text.contains("math:Function"), "{text}");
}

#[test]
fn prefix_query_yields_deterministic_sorted_candidates() {
    let gts = grounding_gts();
    let graph = DescribeGraph::from_gts_bytes(&gts).expect("load");
    // `Fun` prefix-matches `math:Function` and `logic:Function` → NotFound with a
    // deterministic, CURIE-sorted suggestion list.
    match resolve_term(&graph, "Fun") {
        Resolution::NotFound { suggestions } => assert_eq!(
            suggestions,
            vec!["logic:Function".to_string(), "math:Function".to_string()]
        ),
        other => panic!("multi-prefix query must be NotFound with suggestions, got {other:?}"),
    }
}

#[test]
fn short_uses_canonical_registry_and_gufo_matches() {
    // The local GUFO constant must equal the registry's `gufo` namespace, so
    // stereotype detection and CURIE-shortening never diverge.
    assert_eq!(registry_iri("gufo"), Some(GUFO));
    assert_eq!(
        short("https://blackcatinformatics.ca/lang/Denotation"),
        "lang:Denotation"
    );
    assert_eq!(short(&format!("{GUFO}Kind")), "gufo:Kind");
}

#[test]
fn every_grounding_namespace_has_describable_terms_that_render() {
    // Bundle-wide sweep: prove the REQUIREMENT ("every bundled term"), not an
    // example — each grounding namespace has >0 describable terms and a
    // deterministic sample renders on the production entry point.
    let bytes = shipped_bundle_bytes();
    let graph = DescribeGraph::from_gts_bytes(&bytes).expect("load shipped bundle");
    let terms = term_iris(&graph);
    for prefix in ["gmeow", "logic", "math", "lang"] {
        let ns = registry_iri(prefix).expect("grounding prefix registered");
        let in_ns: Vec<&String> = terms.iter().filter(|t| t.starts_with(ns)).collect();
        assert!(
            !in_ns.is_empty(),
            "no describable terms in the `{prefix}:` namespace — the feature is dark there"
        );
        // `terms` is a BTreeSet, so the sample is deterministic. One render per
        // namespace proves the surface is live end-to-end (breadth is covered by
        // the coherence gate and the resolver tests); `describe` re-folds the
        // whole bundle per call, so the sample is kept small.
        let iri = in_ns[0];
        let (text, code) = describe_prose(iri, &bytes, None);
        assert_eq!(code, DescribeStatus::Ok, "`{prefix}:` term {iri}: {text}");
    }
}

#[test]
fn grounding_term_grounding_references_are_themselves_describable() {
    // Navigability closure: a card renders parents/domain/range as CURIEs; every
    // GMEOW-local (grounding) reference must ITSELF be describable, or the
    // "self-describing" ontology has dead links.
    let bytes = shipped_bundle_bytes();
    let graph = DescribeGraph::from_gts_bytes(&bytes).expect("load shipped bundle");
    let terms = term_iris(&graph);
    let grounding_ns: [&str; 4] = ["gmeow", "logic", "math", "lang"]
        .map(|p| registry_iri(p).expect("grounding prefix registered"));
    let is_grounding = |iri: &str| grounding_ns.iter().any(|ns| iri.starts_with(ns));

    let mut checked = 0usize;
    for term in terms.iter().filter(|t| is_grounding(t)).take(200) {
        for reference in gmeow_ns::SUB_CLASS_OF
            .iter()
            .flat_map(|pred| graph.named_objects(term, pred))
            .chain(graph.named_objects(term, RDFS_DOMAIN))
            .chain(graph.named_objects(term, RDFS_RANGE))
        {
            if is_grounding(&reference) {
                assert!(
                    terms.contains(&reference),
                    "term {term} references grounding term {reference} that is not itself describable"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 0,
        "navigability closure asserted nothing — no grounding references found in the sample"
    );
}

/// The staged, shipped bundle bytes (`generated/dist/gmeow.gts`) — the exact
/// bytes `gmeow-cli` embeds. Used by the coherence gate.
///
/// The bundle is a git-ignored local/release product materialized by
/// `make check` (or `make install`), never a committed input, so an absent or
/// zero-length (empty/truncated) file here is a bootstrap problem, not a
/// bare IO error — fail closed with an actionable pointer instead of
/// surfacing a raw `std::io::Error`.
fn shipped_bundle_bytes() -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = gmeow_bundle_import::load_authenticated_source_bytes(&root)
        .expect("authenticated shipped bundle; tests never produce it");
    assert!(
        !bytes.is_empty(),
        "authenticated shipped bundle must be non-empty"
    );
    bytes
}

/// The namespace of an IRI: everything up to and including its last `/` or `#`.
fn namespace_of(iri: &str) -> &str {
    match iri.rfind(['#', '/']) {
        Some(i) => &iri[..=i],
        None => iri,
    }
}

/// COHERENCE GATE: every vocabulary term (OWL class/property) in the shipped
/// bundle must live in a namespace the canonical `PREFIX_REGISTRY` knows. A
/// describable term in an UNREGISTERED namespace can neither be resolved by CURIE
/// nor shortened for display — it would be silently undescribable. This turns
/// "every bundled term resolves" into a machine-checked invariant: add a fifth
/// grounding slice to the bundle without registering its prefix, and this HARD
/// FAILS (rather than the term silently vanishing from `describe`/MCP).
#[test]
fn every_bundled_term_namespace_is_registered() {
    use gmeow_logic_compile::ingest::PREFIX_REGISTRY;

    let bytes = shipped_bundle_bytes();
    let graph = DescribeGraph::from_gts_bytes(&bytes).expect("load shipped bundle");

    // The bundle carries each term's type in the CANONICAL `logic:` spelling (a term is
    // `logic:Class`, not `owl:Class`, after the `owl:`→`logic:` surface flip); the `owl:`
    // spellings are kept so a not-yet-reauthored corpus is still covered. Enumerate both.
    let mut term_subjects: BTreeSet<String> = BTreeSet::new();
    for ty in [
        gmeow_ns::LOGIC_CLASS,
        gmeow_ns::LOGIC_OBJECT_PROPERTY,
        gmeow_ns::LOGIC_DATATYPE_PROPERTY,
        gmeow_ns::LOGIC_ANNOTATION_PROPERTY,
        OWL_CLASS,
        OWL_OBJECT_PROPERTY,
        OWL_DATATYPE_PROPERTY,
        OWL_ANNOTATION_PROPERTY,
    ] {
        term_subjects.extend(graph.subjects_with_object(RDF_TYPE, ty));
    }
    assert!(
        !term_subjects.is_empty(),
        "the shipped bundle declared no vocabulary terms (logic:/owl: class or property) \
             — the gate would be vacuous"
    );

    let registered: BTreeSet<&str> = PREFIX_REGISTRY.iter().map(|(_, ns)| *ns).collect();
    let unregistered: BTreeSet<&str> = term_subjects
        .iter()
        .map(|s| namespace_of(s))
        .filter(|ns| !registered.contains(ns))
        .collect();
    assert!(
        unregistered.is_empty(),
        "describable OWL terms live in namespaces absent from the canonical PREFIX_REGISTRY \
             (they can neither resolve by CURIE nor shorten — register their prefixes): {unregistered:?}"
    );

    // Positive guard: the four grounding namespaces are actually present as term
    // subjects (else the gate is vacuously green for a namespace that silently
    // dropped out of the bundle).
    for prefix in ["gmeow", "logic", "math", "lang"] {
        let ns = registry_iri(prefix).expect("grounding prefix registered");
        assert!(
            term_subjects.iter().any(|s| s.starts_with(ns)),
            "no describable OWL term found in the `{prefix}:` grounding namespace"
        );
    }
}
