// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::parse_dataset;
use std::sync::Arc;

const NS: &str = "https://blackcatinformatics.ca/gmeow/";
const ONT: &str = "https://blackcatinformatics.ca/gmeow";

fn cfg() -> LintConfig {
    LintConfig {
        namespace: NS.to_owned(),
        ontology_iri: ONT.to_owned(),
        selector_tokens: ["primary", "preferred", "default", "main"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: [
            "http://www.w3.org/2000/01/rdf-schema#label",
            "http://www.w3.org/2004/02/skos/core#definition",
            "http://www.w3.org/2000/01/rdf-schema#comment",
            "http://purl.org/dc/terms/title",
            "http://purl.org/dc/terms/description",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}

fn store_from(ttl: &str) -> Arc<RdfDataset> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
}

/// The Check-2 policy is a VIEW over the single localizable authority, never a
/// peer constant: every predicate it polices is a member of the authority, and
/// the authority is the full 14-predicate surface.
#[test]
fn check2_policy_is_a_view_over_the_localizable_authority() {
    let authority: HashSet<&str> = crate::localizable::LOCALIZABLE_PREDICATES
        .iter()
        .copied()
        .collect();
    assert!(
        default_annotation_predicates()
            .iter()
            .all(|p| authority.contains(p.as_str())),
        "default_annotation_predicates must be a subset of the localizable authority"
    );
    assert_eq!(
        crate::localizable::LOCALIZABLE_PREDICATES.len(),
        14,
        "the localizable authority must carry all 14 predicates"
    );
}

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix ex: <https://example.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n";

const ROLE: &str = "ex:boxTBox a gmeow:GraphBoxRole .\n";

#[test]
fn structural_flags_missing_definition() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Undocumented a owl:Class ;\n\
               rdfs:label \"x\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> ;\n\
               rdfs:subClassOf owl:Thing .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("skos:definition"))
    );
}

/// G9 canonical-subsumption sweep: a typo'd target reached ONLY over the
/// canonical `logic:subClassOf` edge must still be reported dangling — scanning
/// `rdfs:subClassOf` alone would silently miss it (crates/ns/src/lib.rs:106-166).
#[test]
fn dangling_target_seen_via_canonical_logic_subclass_of() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> ;\n\
               logic:subClassOf gmeow:MissingParent .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("dangling") && e.contains("/gmeow/MissingParent")),
        "errors: {:?}",
        report.errors()
    );
}

/// G9: the comprehensiveness heuristic's `parent_to_children` map must also see
/// subclasses re-authored under the canonical `logic:subClassOf` edge, not only
/// the `rdfs:subClassOf` projection.
#[test]
fn comprehensiveness_heuristic_sees_canonical_logic_subclass_of() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Parent a owl:Class ; rdfs:label \"Parent\" ; skos:definition \"p\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n\
             gmeow:ChildA a owl:Class ; logic:subClassOf gmeow:Parent .\n\
             gmeow:ChildB a owl:Class ; logic:subClassOf gmeow:Parent .\n\
             gmeow:ChildC a owl:Class ; logic:subClassOf gmeow:Parent .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .warnings()
            .iter()
            .any(|w| w.contains("/gmeow/Parent") && w.contains("systematic documentation gap")),
        "warnings: {:?}",
        report.warnings()
    );
}

#[test]
fn structural_clean_for_well_formed_term() {
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(report.errors().is_empty(), "errors: {:?}", report.errors());
}

#[test]
fn structural_flags_missing_graph_box_role() {
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("missing gmeow:graphBoxRole"))
    );
}

#[test]
fn structural_exempts_self_description_abox() {
    // A-Box individuals defined by the `self` self-description ontology are
    // project metadata, not vocabulary surface, so the per-term annotation /
    // graphBoxRole contract must not fire on them. The same individual
    // shape would be flagged if it were ordinary vocabulary.
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:self#contribution-bii a gmeow:Contribution ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/self> .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("contribution-bii")),
        "self-description A-Box must be exempt: {:?}",
        report.errors()
    );
}

#[test]
fn structural_still_flags_vocabulary_individual() {
    // A gmeow individual NOT defined by `self` (ordinary controlled
    // vocabulary) is still held to the contract — the exemption is narrow.
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:roleAuthor a owl:NamedIndividual ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/creative-works> .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("roleAuthor") && e.contains("graphBoxRole")),
        "ordinary vocabulary individual must still be linted: {:?}",
        report.errors()
    );
}

// A fully-annotated, slice-defined `gmeow:boxABox` role, mirroring its real
// kernel definition. Generated A-Box subjects reference it; it stays on the
// vocabulary tier (slice-defined) so it never pollutes assertional fixtures.
const ABOX_ROLE: &str = "gmeow:boxABox a gmeow:GraphBoxRole ;\n\
           rdfs:label \"ABox role\" ;\n\
           skos:definition \"Assertional graph role.\" ;\n\
           rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/kernel> ;\n\
           gmeow:graphBoxRole ex:boxTBox .\n";

#[test]
fn structural_accepts_assertional_instance_without_definition() {
    // Generated A-Box payload (here a diagnostics Finding) anchored to its
    // named graph and self-declaring gmeow:boxABox is exempt from the
    // skos:definition requirement only — type, label, provenance, and a
    // valid box role are still present, so the subject is clean.
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}{ABOX_ROLE}\
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-0> a gmeow:Finding ;\n\
               rdfs:label \"SH001: example finding\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        !report.errors().iter().any(|e| e.contains("finding/abc-0")),
        "well-formed assertional instance must be clean: {:?}",
        report.errors()
    );
}

#[test]
fn structural_flags_assertional_instance_missing_label() {
    // The assertional tier relaxes skos:definition, NOT the label — a
    // generated subject without rdfs:label is still under-specified.
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}{ABOX_ROLE}\
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-1> a gmeow:Finding ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("finding/abc-1") && e.contains("rdfs:label")),
        "assertional instance missing label must still error: {:?}",
        report.errors()
    );
}

#[test]
fn structural_flags_assertional_instance_missing_box_role() {
    // Anchored to a graph but NOT self-declaring gmeow:boxABox: the
    // relaxation is not earned, so the full quartet applies and the missing
    // role (and definition) still fire.
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}{ABOX_ROLE}\
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-2> a gmeow:Finding ;\n\
               rdfs:label \"SH002: example finding\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("finding/abc-2") && e.contains("missing gmeow:graphBoxRole")),
        "assertional instance without boxABox must still error: {:?}",
        report.errors()
    );
}

#[test]
fn structural_denies_relaxation_without_graph_provenance() {
    // Self-declares gmeow:boxABox and carries a label, but isDefinedBy points
    // at an arbitrary non-graph, non-slice IRI. "Not a slice" alone must NOT
    // earn the assertional relaxation — the skos:definition requirement still
    // applies, proving the relaxation is a positive, earned obligation.
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}{ABOX_ROLE}\
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-3> a gmeow:Finding ;\n\
               rdfs:label \"SH003: example finding\" ;\n\
               rdfs:isDefinedBy <https://example.org/somewhere> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("finding/abc-3") && e.contains("skos:definition")),
        "bogus provenance must not earn the relaxation: {:?}",
        report.errors()
    );
}

#[test]
fn structural_keeps_slice_individual_on_vocabulary_tier() {
    // Branch-order invariant: a slice-defined individual that ALSO carries
    // gmeow:boxABox must stay on the vocabulary tier (slice check wins
    // first), so a missing skos:definition still fires.
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}{ABOX_ROLE}\
             gmeow:sensitivityPublic a gmeow:SensitivityLevel ;\n\
               rdfs:label \"public\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/kernel> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("sensitivityPublic") && e.contains("skos:definition")),
        "slice individual must stay on the vocabulary tier: {:?}",
        report.errors()
    );
}

#[test]
fn structural_rejects_untyped_graph_box_role() {
    let store = store_from(&format!(
        "{PREFIXES}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               gmeow:graphBoxRole ex:notARole ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("not a gmeow:GraphBoxRole"))
    );
}

#[test]
fn structural_accepts_mixed_case_private_tag() {
    let store = store_from(&format!(
        "{PREFIXES}\
             <https://example.org/name> gmeow:fullName \"Japanese\"@x-GMEOW-Japanese .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(report.errors().is_empty(), "errors: {:?}", report.errors());
}

#[test]
fn structural_rejects_external_tag_on_gmeow_predicate() {
    let store = store_from(&format!(
        "{PREFIXES}\
             <https://example.org/name> gmeow:fullName \"Japanese\"@ja .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("external or invalid language tag"))
    );
    // Exact rdflib repr framing.
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("literal rdflib.term.Literal('Japanese', lang='ja')"))
    );
}

#[test]
fn structural_rejects_en_on_gmeow_label() {
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}\
             gmeow:TestTerm a owl:Class ;\n\
               rdfs:label \"Name\"@en ;\n\
               skos:definition \"A test term.\" ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("external language tag 'en'") && e.contains("label"))
    );
}

#[test]
fn structural_accepts_x_gmeow_english_on_label() {
    let store = store_from(&format!(
        "{PREFIXES}{ROLE}\
             gmeow:TestTerm a owl:Class ;\n\
               rdfs:label \"Name\"@x-gmeow-english ;\n\
               skos:definition \"A test term.\"@x-gmeow-english ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
    ));
    let report = structural_lint_dataset(&store, &cfg());
    assert!(report.errors().is_empty(), "errors: {:?}", report.errors());
}

#[test]
fn lang_tag_exempts_external_publication_fanout_graph_but_not_internal() {
    use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

    // The identical offending triple — a GMEOW-subject rdfs:label carrying a bare
    // public `@en` tag — in an external-publication RO-Crate fanout graph vs an
    // internal fanout graph. The RO-Crate export deliberately retags to public
    // BCP-47 for its consumers, so the external copy is exempt; the internal copy
    // is not. A single label triple keeps the subject untyped, so only the
    // lang-tag check is in play (no missing-* structural findings).
    let label = "http://www.w3.org/2000/01/rdf-schema#label";
    let subject = format!("{NS}evals/corpus/readme");
    let en = RdfTerm::literal(RdfLiteral::language_tagged("README.md", "en"));
    let external_graph = format!("{NS}graph/fanout/research-objects/lillith/ro-crate/corpus.ttl");
    let internal_graph = format!("{NS}graph/fanout/evals/scores.ttl");

    let build = |graph: &str| -> Arc<RdfDataset> {
        let mut b = RdfDatasetBuilder::new();
        b.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(subject.clone()), label, en.clone())
                .in_graph(RdfTerm::iri(graph.to_owned())),
        );
        b.freeze().expect("valid dataset")
    };

    // External-publication graph: exempt — no lang-tag finding.
    let external = structural_lint_dataset(&build(&external_graph), &cfg());
    assert!(
        !external
            .errors()
            .iter()
            .any(|e| e.contains("external language tag 'en'")),
        "external-publication RO-Crate fanout graph must be exempt from the \
             carrier-tag discipline: {:?}",
        external.errors()
    );

    // FnO lowering graph (Principle 17): also exempt.
    let fno_graph = format!("{NS}graph/fanout/projections/functions.fno.ttl");
    let fno = structural_lint_dataset(&build(&fno_graph), &cfg());
    assert!(
        !fno.errors()
            .iter()
            .any(|e| e.contains("external language tag 'en'")),
        "FnO lowering graph must be exempt from the carrier-tag discipline: {:?}",
        fno.errors()
    );

    // Internal fanout graph: the same triple is still flagged.
    let internal = structural_lint_dataset(&build(&internal_graph), &cfg());
    assert!(
        internal
            .errors()
            .iter()
            .any(|e| e.contains("external language tag 'en'") && e.contains("label")),
        "an internal graph's bare @en must still be flagged: {:?}",
        internal.errors()
    );
}

#[test]
fn naming_lint_flags_primary_without_note() {
    let store = store_from(&format!("{PREFIXES}gmeow:PrimaryThing a owl:Class .\n"));
    let report = term_naming_lint_dataset(&store, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("selector token 'primary'"))
    );
}

#[test]
fn naming_lint_respects_naming_note() {
    let store = store_from(&format!(
        "{PREFIXES}gmeow:sourceTierPrimary a owl:Class ;\n\
               gmeow:namingNote \"value vocabulary\" .\n"
    ));
    let report = term_naming_lint_dataset(&store, &cfg());
    assert!(report.errors().is_empty(), "errors: {:?}", report.errors());
}

#[test]
fn collect_typed_terms_resolves_multityped() {
    // A subject typed as both Class and Individual resolves to class (lower rank).
    let store = store_from(&format!(
        "{PREFIXES}gmeow:Thing a owl:Class , gmeow:SomeIndividualType .\n"
    ));
    let terms = collect_typed_terms_dataset(&store, &cfg());
    assert_eq!(
        terms.get("https://blackcatinformatics.ca/gmeow/Thing"),
        Some(&"class".to_owned())
    );
}

#[test]
fn camel_tokens_matches_python() {
    let cases: &[(&str, &[&str])] = &[
        ("PrimaryThing", &["primary", "thing"]),
        ("scriptRolePrimary", &["script", "role", "primary"]),
        ("sourceTierPrimary", &["source", "tier", "primary"]),
        ("HTTPSConnection", &["https", "connection"]),
        ("IRI", &["iri"]),
        ("primary", &["primary"]),
        ("Primary", &["primary"]),
        ("XMLHttpRequest", &["xml", "http", "request"]),
        ("fooBARBaz", &["foo", "bar", "baz"]),
        ("ABCdef", &["ab", "cdef"]),
        ("a1B2c3", &["a1", "b2c3"]),
        ("mainDefault", &["main", "default"]),
        ("URLPreferred", &["url", "preferred"]),
    ];
    for (input, expected) in cases {
        let got = camel_tokens(input);
        let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(&got, &want, "input {input}");
    }
}

/// Parse a Turtle fixture into a frozen native dataset (no oxigraph round-trip).
fn dataset_from(ttl: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
}

/// The native `structural_lint_dataset` twin must produce byte-identical
/// errors/warnings to the `Store` version across a battery of fixtures.
#[test]
fn native_structural_lint_parity_with_store() {
    let fixtures = [
        // missing definition
        format!(
            "{PREFIXES}\
                 gmeow:Undocumented a owl:Class ;\n\
                   rdfs:label \"x\" ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> ;\n\
                   rdfs:subClassOf owl:Thing .\n"
        ),
        // well-formed (clean)
        format!(
            "{PREFIXES}{ROLE}\
                 gmeow:Documented a owl:Class ;\n\
                   rdfs:label \"Documented\" ;\n\
                   skos:definition \"A well-formed term.\" ;\n\
                   gmeow:graphBoxRole ex:boxTBox ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ),
        // external tag on gmeow predicate + en on label
        format!(
            "{PREFIXES}{ROLE}\
                 gmeow:TestTerm a owl:Class ;\n\
                   rdfs:label \"Name\"@en ;\n\
                   skos:definition \"A test term.\" ;\n\
                   gmeow:graphBoxRole ex:boxTBox ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n\
                 <https://example.org/name> gmeow:fullName \"Japanese\"@ja .\n"
        ),
        // untyped graphBoxRole
        format!(
            "{PREFIXES}\
                 gmeow:Documented a owl:Class ;\n\
                   rdfs:label \"Documented\" ;\n\
                   skos:definition \"A well-formed term.\" ;\n\
                   gmeow:graphBoxRole ex:notARole ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ),
    ];
    for ttl in &fixtures {
        let store = structural_lint_dataset(&store_from(ttl), &cfg());
        let native = structural_lint_dataset(&dataset_from(ttl), &cfg());
        // Compare as sorted sets: the per-check emission order differs between
        // oxigraph's `store.iter()` and the native freeze-sorted scan (Check 6,
        // the whole-graph language-tag pass), but the SET of diagnostics must be
        // identical. Downstream the report is normalized via `Finding::sort_key`
        // (`Report::normalize`), so committed bytes never depend on this order.
        let (mut se, mut ne) = (store.errors().clone(), native.errors().clone());
        se.sort();
        ne.sort();
        assert_eq!(se, ne, "errors diverged for: {ttl}");
        let (mut sw, mut nw) = (store.warnings().clone(), native.warnings().clone());
        sw.sort();
        nw.sort();
        assert_eq!(sw, nw, "warnings diverged for: {ttl}");
    }
}

#[test]
fn py_str_repr_quote_choice() {
    assert_eq!(py_str_repr("Japanese"), "'Japanese'");
    assert_eq!(py_str_repr("it's"), "\"it's\"");
    assert_eq!(py_str_repr("has \"q\""), "'has \"q\"'");
    assert_eq!(py_str_repr("back\\slash"), "'back\\\\slash'");
    assert_eq!(py_str_repr("new\nline"), "'new\\nline'");
    assert_eq!(py_str_repr("tab\there"), "'tab\\there'");
    assert_eq!(py_str_repr("uniécode"), "'uniécode'");
}

// --- lang: meaning-stratum native gates ---------------------------------- #

const LANG_PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix ex: <https://example.org/> .\n";

#[test]
fn undeclared_lowering_flags_bridge_denotation_without_preservation() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationKind lang:denotesLogicFormula ;\n\
               lang:denotedForm ex:f ;\n\
               lang:denotationTarget ex:formula ;\n\
               lang:denotationContext ex:ctx .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UndeclaredLoweringStage")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn undeclared_lowering_clean_when_preservation_declared() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationKind lang:denotesLogicFormula ;\n\
               lang:denotedForm ex:f ;\n\
               lang:denotationTarget ex:formula ;\n\
               lang:denotationContext ex:ctx ;\n\
               logic:preservationKind logic:ExactPreservation .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UndeclaredLoweringStage")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn silent_disambiguation_flags_ungrounded_resolution() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:producedReading ex:r1 , ex:r2 ;\n\
               lang:resolvedReading ex:r1 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentDisambiguation")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn silent_disambiguation_clean_when_resolution_is_vantage_held() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:producedReading ex:r1 , ex:r2 ;\n\
               lang:resolvedReading ex:r1 .\n\
             ex:obs lang:aboutReading ex:r1 ;\n\
               gmeow:vantage ex:annotator .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentDisambiguation")),
        "errors: {:?}",
        report.errors()
    );
}

fn gmn_compaction_silent_disambiguation_fixture_fires_exactly_that_gate() {
    // The GMN counter-example reuses the EXISTING bundle-wide
    // lang:SilentDisambiguation discipline verbatim: a compaction run whose
    // interpretation act collapses two co-resident readings to one with no
    // vantage-held observation. Like projection-silent-disambiguation.ttl it is
    // a native-gate fixture (no SHACL cell), so this test is its executable pin:
    // it fires lang:SilentDisambiguation and NO other lang: failure class — the
    // compaction record itself is deliberately well-formed.
    let report = source_contracts::report(
        "slices/grounding/lang/tests/counter-examples/gmn-compaction-silent-disambiguation.ttl",
        &cfg(),
    );
    let errors = report.errors();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("lang:SilentDisambiguation")),
        "fixture must fire lang:SilentDisambiguation: {errors:?}",
    );
    assert_eq!(
        errors.iter().filter(|e| e.contains("lang:")).count(),
        1,
        "fixture must isolate exactly the silent collapse: {errors:?}",
    );
}

#[test]
fn one_way_bridge_flags_logic_subject_with_lang_predicate() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             logic:someObject lang:denotedForm ex:f .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("one-way bridge violated")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn one_way_bridge_clean_for_lang_to_logic_target() {
    // The lawful direction: a lang: denotation targeting a logic: object.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationTarget logic:someFormula .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("one-way bridge violated")),
        "errors: {:?}",
        report.errors()
    );
}

// --- lang: ingestion-stratum native gates -------------------------------- #

#[test]
fn unattributed_engine_claim_flags_engine_reading_without_vantage() {
    // An engine run (lang:interpretationEngine present) whose produced reading
    // carries no gmeow:vantage — engine output entered as unattributed structure.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:interpretationEngine ex:udParser ;\n\
               lang:producedReading ex:r1 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnattributedEngineClaim")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn unattributed_engine_claim_clean_when_reading_is_vantage_held() {
    // The lawful engine handoff: each produced reading carries the engine's vantage.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:interpretationEngine ex:udParser ;\n\
               lang:producedReading ex:r1 .\n\
             ex:r1 a lang:Reading ; gmeow:vantage ex:udVantage .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnattributedEngineClaim")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn unattributed_engine_claim_ignores_non_engine_act() {
    // A manual/compositional act (no lang:interpretationEngine) may lawfully leave
    // a co-resident reading unclaimed — the gate must NOT fire on it.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:producedReading ex:r1 , ex:r2 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnattributedEngineClaim")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn silent_promotion_flags_promotion_without_editorial_act() {
    // A bare subject promotes a reading with no provenance-carrying act.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:slice lang:promotedReading ex:r1 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentPromotion")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn silent_promotion_clean_when_promotion_is_a_vantage_held_activity() {
    // The lawful promotion: an explicit editorial gmeow:Activity carrying a vantage.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:promote a gmeow:Activity ;\n\
               gmeow:vantage ex:editor ;\n\
               lang:promotedReading ex:r1 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentPromotion")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn silent_promotion_flags_activity_missing_vantage() {
    // An activity that promotes but carries no vantage is still a silent promotion.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:promote a gmeow:Activity ;\n\
               lang:promotedReading ex:r1 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentPromotion")),
        "errors: {:?}",
        report.errors()
    );
}

fn ingestion_counter_example_fixtures_fire_exactly_their_class() {
    // The slice-resident counter-examples for the two native ingestion gates each
    // fire exactly their named failure class (and nothing from the other gate),
    // so the (fixture, class) pair is load-bearing rather than decorative.
    let unattributed =
        "slices/grounding/lang/tests/counter-examples/ingestion-unattributed-engine-claim.ttl";
    let report = source_contracts::report(unattributed, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnattributedEngineClaim")),
        "errors: {:?}",
        report.errors()
    );
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentPromotion")),
        "the unattributed-engine fixture must not also fire SilentPromotion: {:?}",
        report.errors()
    );

    let promotion = "slices/grounding/lang/tests/counter-examples/ingestion-silent-promotion.ttl";
    let report = source_contracts::report(promotion, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentPromotion")),
        "errors: {:?}",
        report.errors()
    );
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnattributedEngineClaim")),
        "the silent-promotion fixture keeps its reading vantage-held: {:?}",
        report.errors()
    );
}

fn ambiguity_positive_fixture_is_clean_under_the_native_gates() {
    // Gate 5 (positive): the co-resident-readings fixture — an engine act producing
    // TWO vantage-held readings with NO resolved winner — trips none of the lang:
    // native gates.
    let fixture = "slices/grounding/lang/tests/conformance-fixtures/ambiguity-saw-her-duck.ttl";
    let report = source_contracts::report(fixture, &cfg());
    let report_errors = report.errors();
    let lang_errors: Vec<&String> = report_errors
        .iter()
        .filter(|e| e.contains("lang:") || e.contains("one-way bridge"))
        .collect();
    assert!(
        lang_errors.is_empty(),
        "the ambiguity fixture must be clean under the native lang: gates: {lang_errors:?}"
    );
}

// ---- lang: projection-stratum native gates (the lossy-lowering contract) ----

#[test]
fn projection_missing_preservation_kind_fires() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexeme .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:MissingPreservationKind")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_missing_preservation_kind_clean_when_declared() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexeme ;\n\
               logic:preservationKind logic:ExactPreservation .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:MissingPreservationKind")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_undeclared_unsupported_fires() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:grammarSrc a lang:Grammar .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"EBNF\" ;\n\
               lang:projectsSource ex:grammarSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UndeclaredUnsupportedConstruct")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_undeclared_unsupported_clean_when_enumerated() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:grammarSrc a lang:Grammar .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"EBNF\" ;\n\
               lang:projectsSource ex:grammarSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"left-recursion\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UndeclaredUnsupportedConstruct")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_unrecorded_epistemic_loss_fires() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:lexemeSrc a lang:Lexeme ;\n\
               gmeow:vantage ex:annotatorVantage .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexemeSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"inflection-tables\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnrecordedEpistemicLoss")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_unrecorded_epistemic_loss_clean_when_stratum_named() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:lexemeSrc a lang:Lexeme ;\n\
               gmeow:vantage ex:annotatorVantage .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexemeSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"vantage-held-readings\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnrecordedEpistemicLoss")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_unrecorded_epistemic_loss_fires_when_only_one_of_many_strata_named() {
    // A source flattening TWO strata (vantage + interpretation) whose emission names only
    // ONE (vantage) leaves interpretation silently unrecorded — the gate must fire. Under
    // the earlier `any` semantics this escaped; `all` closes it.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:lexemeSrc a lang:Lexeme, lang:InterpretationAct ;\n\
               gmeow:vantage ex:annotatorVantage .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexemeSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"vantage-flattened\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnrecordedEpistemicLoss")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_unrecorded_epistemic_loss_clean_when_all_strata_named() {
    // The same two-stratum source is clean only when the emission names BOTH strata.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:lexemeSrc a lang:Lexeme, lang:InterpretationAct ;\n\
               gmeow:vantage ex:annotatorVantage .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"OntoLex-Lemon\" ;\n\
               lang:projectsSource ex:lexemeSrc ;\n\
               logic:preservationKind logic:SoundUnderApproximation ;\n\
               lang:unsupportedConstruct \"vantage-flattened\" ;\n\
               lang:unsupportedConstruct \"interpretation-act-dropped\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:UnrecordedEpistemicLoss")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_silent_disambiguation_fires() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:form a lang:ComposedForm .\n\
             ex:r1 a lang:Reading ; lang:readingOf ex:form .\n\
             ex:r2 a lang:Reading ; lang:readingOf ex:form .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"CoNLL-U\" ;\n\
               lang:projectsSource ex:form ;\n\
               logic:preservationKind logic:ExactPreservation ;\n\
               lang:emittedReadingCount 1 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:ProjectionSilentDisambiguation")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_silent_disambiguation_clean_when_all_readings_emitted() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:form a lang:ComposedForm .\n\
             ex:r1 a lang:Reading ; lang:readingOf ex:form .\n\
             ex:r2 a lang:Reading ; lang:readingOf ex:form .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"CoNLL-U\" ;\n\
               lang:projectsSource ex:form ;\n\
               logic:preservationKind logic:ExactPreservation ;\n\
               lang:emittedReadingCount 2 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:ProjectionSilentDisambiguation")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_exact_preservation_violated_fires() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:grammar a lang:Grammar .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"GTS-grammar-surface\" ;\n\
               lang:projectsSource ex:grammar ;\n\
               logic:preservationKind logic:ExactPreservation ;\n\
               lang:roundTripHolds false .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:ExactPreservationViolated")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn projection_exact_preservation_violated_clean_when_round_trip_holds() {
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:grammar a lang:Grammar .\n\
             ex:em a lang:ProjectionEmission ;\n\
               lang:projectionTargetName \"GTS-grammar-surface\" ;\n\
               lang:projectsSource ex:grammar ;\n\
               logic:preservationKind logic:ExactPreservation ;\n\
               lang:roundTripHolds true .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:ExactPreservationViolated")),
        "errors: {:?}",
        report.errors()
    );
}

fn projection_counter_example_fixtures_fire_exactly_their_class() {
    // The four native-gate projection counter-examples each fire exactly their named
    // failure class (and no OTHER projection class), so each (fixture, class) pair is
    // load-bearing. (The MissingPreservationKind fixture rides the SHACL harness and is
    // covered by its inline native test above.)
    let cases: [(&str, &str, [&str; 3]); 4] = [
        (
            "slices/grounding/lang/tests/counter-examples/projection-undeclared-unsupported.ttl",
            "lang:UndeclaredUnsupportedConstruct",
            [
                "lang:UnrecordedEpistemicLoss",
                "lang:ProjectionSilentDisambiguation",
                "lang:ExactPreservationViolated",
            ],
        ),
        (
            "slices/grounding/lang/tests/counter-examples/projection-unrecorded-epistemic-loss.ttl",
            "lang:UnrecordedEpistemicLoss",
            [
                "lang:UndeclaredUnsupportedConstruct",
                "lang:ProjectionSilentDisambiguation",
                "lang:ExactPreservationViolated",
            ],
        ),
        (
            "slices/grounding/lang/tests/counter-examples/projection-silent-disambiguation.ttl",
            "lang:ProjectionSilentDisambiguation",
            [
                "lang:UndeclaredUnsupportedConstruct",
                "lang:UnrecordedEpistemicLoss",
                "lang:ExactPreservationViolated",
            ],
        ),
        (
            "slices/grounding/lang/tests/counter-examples/projection-exact-preservation-violated.ttl",
            "lang:ExactPreservationViolated",
            [
                "lang:UndeclaredUnsupportedConstruct",
                "lang:UnrecordedEpistemicLoss",
                "lang:ProjectionSilentDisambiguation",
            ],
        ),
    ];
    for (ttl, expected, forbidden) in cases {
        let report = source_contracts::report(ttl, &cfg());
        assert!(
            report.errors().iter().any(|e| e.contains(expected)),
            "fixture must fire {expected}: {:?}",
            report.errors()
        );
        // The MissingPreservationKind gate must also stay silent (every fixture declares
        // its preservation kind).
        assert!(
            !report
                .errors()
                .iter()
                .any(|e| e.contains("lang:MissingPreservationKind")),
            "fixture for {expected} declares a preservation kind: {:?}",
            report.errors()
        );
        for other in forbidden {
            assert!(
                !report.errors().iter().any(|e| e.contains(other)),
                "fixture for {expected} must not also fire {other}: {:?}",
                report.errors()
            );
        }
    }
}

/// The five native probability-layer failure classes the isolation test polices.
const MATH_PROBABILITY_CLASSES: [&str; 5] = [
    "math:ProbabilityOutOfBounds",
    "math:DistributionParameterConstraint",
    "math:MissingProbabilityModelLowering",
    "math:IncompleteDependencyModel",
    "math:ExactPreservationViolated",
];

fn math_probability_counter_examples_fire_exactly_their_class() {
    // Each native-gate probability counter-example fires EXACTLY its named failure
    // class (and none of the other four probability classes), so each (fixture, class)
    // pair is load-bearing. All three distribution-parameter counter-examples fire the
    // shared math:DistributionParameterConstraint class (positivity arm vs the
    // absolute-dimension arm vs the relational — same-as/square-of — dimension arm).
    let cases: [(&str, &str); 12] = [
        (
            "slices/grounding/math/tests/counter-examples/probability-out-of-bounds.ttl",
            "math:ProbabilityOutOfBounds",
        ),
        (
            "slices/grounding/math/tests/counter-examples/probability-out-of-bounds-signed.ttl",
            "math:ProbabilityOutOfBounds",
        ),
        (
            "slices/grounding/math/tests/counter-examples/distribution-parameter-negative.ttl",
            "math:DistributionParameterConstraint",
        ),
        (
            "slices/grounding/math/tests/counter-examples/distribution-parameter-wrong-dimension.ttl",
            "math:DistributionParameterConstraint",
        ),
        (
            "slices/grounding/math/tests/counter-examples/distribution-parameter-relational-dimension.ttl",
            "math:DistributionParameterConstraint",
        ),
        (
            "slices/grounding/math/tests/counter-examples/missing-probability-model-lowering.ttl",
            "math:MissingProbabilityModelLowering",
        ),
        (
            "slices/grounding/math/tests/counter-examples/incomplete-dependency-model.ttl",
            "math:IncompleteDependencyModel",
        ),
        (
            "slices/grounding/math/tests/counter-examples/factor-graph-incomplete.ttl",
            "math:IncompleteDependencyModel",
        ),
        (
            "slices/grounding/math/tests/counter-examples/exact-preservation-violated.ttl",
            "math:ExactPreservationViolated",
        ),
        (
            "slices/grounding/math/tests/counter-examples/bayesian-network-exact-preservation-violated.ttl",
            "math:ExactPreservationViolated",
        ),
        (
            "slices/grounding/math/tests/counter-examples/factor-graph-exact-preservation-violated.ttl",
            "math:ExactPreservationViolated",
        ),
        (
            "slices/grounding/math/tests/counter-examples/markov-kernel-exact-preservation-violated.ttl",
            "math:ExactPreservationViolated",
        ),
    ];
    for (ttl, expected) in cases {
        let report = source_contracts::report(ttl, &cfg());
        assert!(
            report.errors().iter().any(|e| e.contains(expected)),
            "fixture must fire {expected}: {:?}",
            report.errors()
        );
        for other in MATH_PROBABILITY_CLASSES {
            if other == expected {
                continue;
            }
            assert!(
                !report.errors().iter().any(|e| e.contains(other)),
                "fixture for {expected} must not also fire {other}: {:?}",
                report.errors()
            );
        }
    }
}

fn math_probability_clean_fixtures_fire_no_probability_class() {
    // Each clean conformance fixture is the positive counterpart of one counter-example
    // and MUST raise none of the five native probability failure classes.
    let clean: [&str; 11] = [
        "slices/grounding/math/tests/conformance-fixtures/probability-in-bounds.ttl",
        "slices/grounding/math/tests/conformance-fixtures/distribution-parameter-positive.ttl",
        "slices/grounding/math/tests/conformance-fixtures/distribution-parameter-right-dimension.ttl",
        // Positive counterpart of counter-examples/distribution-parameter-relational-dimension.ttl
        // for the RELATIONAL (math:sameAsRandomVariableDimension) dimension arm: the random
        // variable and its location parameter's quantity both carry the resolvable
        // math:lengthDimension, so the ℚ⁷ exact comparison agrees and raises nothing.
        "slices/grounding/math/tests/fixtures/random-variable-distribution.ttl",
        "slices/grounding/math/tests/conformance-fixtures/probability-model-lowering-declared.ttl",
        "slices/grounding/math/tests/conformance-fixtures/dependency-model-complete.ttl",
        "slices/grounding/math/tests/conformance-fixtures/factor-graph-complete.ttl",
        "slices/grounding/math/tests/conformance-fixtures/joint-table-mass-one.ttl",
        "slices/grounding/math/tests/conformance-fixtures/bayesian-network-exact-complete.ttl",
        "slices/grounding/math/tests/conformance-fixtures/factor-graph-exact-complete.ttl",
        "slices/grounding/math/tests/conformance-fixtures/markov-kernel-exact-complete.ttl",
    ];
    for ttl in clean {
        let report = source_contracts::report(ttl, &cfg());
        for class in MATH_PROBABILITY_CLASSES {
            assert!(
                !report.errors().iter().any(|e| e.contains(class)),
                "clean fixture must not fire {class}: {:?}",
                report.errors()
            );
        }
    }
}

/// The two native projection-side failure classes the isolation test polices.
const MATH_PROJECTION_CLASSES: [&str; 2] = [
    "math:ProjectionConfidenceAsProbability",
    "math:ProjectionDroppedParameterization",
];

fn math_projection_counter_examples_fire_exactly_their_class() {
    // Each projection-side counter-example fires EXACTLY its named failure class (and
    // not the other projection class), so each (fixture, class) pair is load-bearing.
    let cases: [(&str, &str); 2] = [
        (
            "slices/grounding/math/tests/counter-examples/projection-confidence-as-probability.ttl",
            "math:ProjectionConfidenceAsProbability",
        ),
        (
            "slices/grounding/math/tests/counter-examples/projection-dropped-parameterization.ttl",
            "math:ProjectionDroppedParameterization",
        ),
    ];
    for (ttl, expected) in cases {
        let report = source_contracts::report(ttl, &cfg());
        assert!(
            report.errors().iter().any(|e| e.contains(expected)),
            "fixture must fire {expected}: {:?}",
            report.errors()
        );
        for other in MATH_PROJECTION_CLASSES {
            if other == expected {
                continue;
            }
            assert!(
                !report.errors().iter().any(|e| e.contains(other)),
                "fixture for {expected} must not also fire {other}: {:?}",
                report.errors()
            );
        }
    }
}

fn math_projection_clean_fixtures_fire_no_projection_class() {
    // Each clean conformance fixture is the positive counterpart of one projection
    // counter-example and MUST raise neither projection failure class.
    let clean: [&str; 2] = [
        "slices/grounding/math/tests/conformance-fixtures/projection-confidence-mapping-declared.ttl",
        "slices/grounding/math/tests/conformance-fixtures/projection-parameterization-recorded.ttl",
    ];
    for ttl in clean {
        let report = source_contracts::report(ttl, &cfg());
        for class in MATH_PROJECTION_CLASSES {
            assert!(
                !report.errors().iter().any(|e| e.contains(class)),
                "clean fixture must not fire {class}: {:?}",
                report.errors()
            );
        }
    }
}

/// Prefixes for inline math: projection-loss unit fixtures (tokenization regression: the
/// discharge-by-mention check over `logic:unsupportedConstruct` must tokenize the
/// literal, never `str::contains` it — "ast" is a substring of many ordinary words).
const MATH_PROJECTION_LOSS_PREFIXES: &str = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix ex: <http://example.org/math/> .\n";

/// A lossy `math:ProjectionRecord` whose `math:projectionSource` is a
/// `math:MathematicalExpression`, with `unsupported_literal` as its ONLY
/// `logic:unsupportedConstruct` entry.
fn math_projection_loss_fixture(unsupported_literal: &str) -> String {
    format!(
        "{MATH_PROJECTION_LOSS_PREFIXES}\
             ex:src a math:MathematicalExpression .\n\
             ex:proj a math:ProjectionRecord ;\n\
               math:projectionSource ex:src ;\n\
               logic:preservationKind logic:Unsupported ;\n\
               logic:unsupportedConstruct \"{unsupported_literal}\" .\n"
    )
}

#[test]
fn unrecorded_projection_loss_not_discharged_by_last_writer_wins() {
    let report = structural_lint_dataset(
        &dataset_from(&math_projection_loss_fixture("last-writer-wins")),
        &cfg(),
    );
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:UnrecordedProjectionLoss")),
        "\"last-writer-wins\" must NOT discharge the loss ledger via a bare \"ast\" \
             substring match: {:?}",
        report.errors()
    );
}

#[test]
fn unrecorded_projection_loss_not_discharged_by_broadcast_fanout() {
    let report = structural_lint_dataset(
        &dataset_from(&math_projection_loss_fixture("broadcast-fanout")),
        &cfg(),
    );
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:UnrecordedProjectionLoss")),
        "\"broadcast-fanout\" must NOT discharge the loss ledger via a bare \"ast\" \
             substring match: {:?}",
        report.errors()
    );
}

#[test]
fn unrecorded_projection_loss_not_discharged_by_forecast_dropped() {
    let report = structural_lint_dataset(
        &dataset_from(&math_projection_loss_fixture("forecast-dropped")),
        &cfg(),
    );
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:UnrecordedProjectionLoss")),
        "\"forecast-dropped\" must NOT discharge the loss ledger via a bare \"ast\" \
             substring match: {:?}",
        report.errors()
    );
}

#[test]
fn unrecorded_projection_loss_not_discharged_by_drastic_simplification() {
    let report = structural_lint_dataset(
        &dataset_from(&math_projection_loss_fixture("drastic-simplification")),
        &cfg(),
    );
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:UnrecordedProjectionLoss")),
        "\"drastic-simplification\" must NOT discharge the loss ledger via a bare \"ast\" \
             substring match: {:?}",
        report.errors()
    );
}

#[test]
fn unrecorded_projection_loss_discharged_by_genuine_ast_mention() {
    let report = structural_lint_dataset(
        &dataset_from(&math_projection_loss_fixture(
            "flattened the AST to a string",
        )),
        &cfg(),
    );
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:UnrecordedProjectionLoss")),
        "a genuine whole-word AST mention must discharge the loss ledger: {:?}",
        report.errors()
    );
}

/// Prefixes for inline math: probability unit fixtures.
const MATH_PROB_PREFIXES: &str = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix ex: <http://example.org/math/> .\n";

#[test]
fn probability_value_at_boundaries_is_clean() {
    // A math:ProbabilityValue at exactly 0 and exactly 1 sits on the closed interval's
    // boundary — the gate is inclusive, so neither fires math:ProbabilityOutOfBounds.
    let ds = dataset_from(&format!(
        "{MATH_PROB_PREFIXES}\
             ex:zero a math:ProbabilityValue ; math:quantityValue \"0\" .\n\
             ex:one a math:ProbabilityValue ; math:quantityValue \"1.0\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:ProbabilityOutOfBounds")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn rational_value_three_halves_fires_out_of_bounds() {
    // A math:ProbabilityValue carried as an exact math:RationalValue 3/2 is read from
    // its numerator/denominator pair (never a decimal) and exceeds 1.
    let ds = dataset_from(&format!(
        "{MATH_PROB_PREFIXES}\
             ex:p a math:ProbabilityValue , math:RationalValue ;\n\
               math:numerator 3 ; math:denominator 2 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report.errors().iter().any(|e| e
            .contains("math:ProbabilityOutOfBounds: probability value")
            && e.contains("3/2")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn exact_rational_half_plus_half_sums_to_one_clean() {
    // 1/2 + 1/2 = 1 exactly, so a joint table with that mass does not overclaim.
    let ds = dataset_from(&format!(
        "{MATH_PROB_PREFIXES}\
             ex:t a math:JointProbabilityTable ; logic:jointOutcome ex:a , ex:b .\n\
             ex:a logic:jointProbability 0.5 .\n\
             ex:b logic:jointProbability 0.5 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:ExactPreservationViolated")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn joint_mass_zero_point_nine_fires_exact_preservation() {
    // 0.5 + 0.4 = 0.9 ≠ 1 (exact-rational), so the table overclaims exact preservation.
    let ds = dataset_from(&format!(
        "{MATH_PROB_PREFIXES}\
             ex:t a math:JointProbabilityTable ; logic:jointOutcome ex:a , ex:b .\n\
             ex:a logic:jointProbability 0.5 .\n\
             ex:b logic:jointProbability 0.4 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:ExactPreservationViolated") && e.contains("9/10")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn decimal_to_rational_parses_plain_decimals_and_rejects_scientific() {
    assert_eq!(decimal_to_rational("1.5"), Rational::new(3, 2).ok());
    assert_eq!(decimal_to_rational("-1"), Rational::new(-1, 1).ok());
    assert_eq!(decimal_to_rational("0.72"), Rational::new(18, 25).ok());
    assert_eq!(decimal_to_rational("0"), Rational::new(0, 1).ok());
    assert_eq!(decimal_to_rational("1e3"), None);
    assert_eq!(decimal_to_rational("1.2E4"), None);
    assert_eq!(decimal_to_rational("abc"), None);
    assert_eq!(decimal_to_rational(""), None);
}

#[test]
fn surface_leak_flags_crossing_carrying_surface_predicate() {
    // A translation unit that inlines surface-stratum material (lang:inScript)
    // as identity input rather than referencing structural forms.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:tu a lang:TranslationUnit ;\n\
               lang:translationSource ex:srcForm ;\n\
               lang:translationTarget ex:tgtForm ;\n\
               lang:inScript ex:latinScript .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SurfaceLeakInContentKey")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn surface_leak_clean_when_crossing_references_structural_forms() {
    // A well-formed crossing over structural forms, with no surface-stratum
    // predicate carried directly on the crossing itself.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:tu a lang:TranslationUnit ;\n\
               lang:translationSource ex:srcForm ;\n\
               lang:translationTarget ex:tgtForm ;\n\
               lang:translationCorrespondence ex:corr .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SurfaceLeakInContentKey")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn rendering_as_identity_flags_sameas_to_rendered_content() {
    // A rendering asserted owl:sameAs its own renderedContent — the rendering
    // becoming identity.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:r a lang:Rendering ;\n\
               lang:renderedContent ex:content ;\n\
               lang:renderingForm ex:form ;\n\
               owl:sameAs ex:content .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:RenderingAsIdentity")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn rendering_as_identity_clean_when_form_and_content_are_distinct() {
    // A rendering that names distinct content and form and asserts no identity
    // between the rendering and its content.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:r a lang:Rendering ;\n\
               lang:renderedContent ex:content ;\n\
               lang:renderingForm ex:form .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:RenderingAsIdentity")),
        "errors: {:?}",
        report.errors()
    );
}

// --- lang: form-stratum native gates (blob-by-reference + slot contiguity) - #

#[test]
fn inline_blob_payload_flags_document_scale_surface_text() {
    // A lang:SurfaceForm whose inline lang:surfaceText exceeds the document-scale
    // threshold — payload folded inline instead of held by reference.
    let big = "x".repeat(DOCUMENT_SCALE_BYTES + 1);
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:s a lang:SurfaceForm , lang:UnanalyzedProse ;\n\
               lang:surfaceText \"{big}\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:InlineBlobPayload")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn inline_blob_payload_clean_for_small_surface_and_for_blob_reference() {
    // A small inline surface stays inline (clean); a document-scale surface holding
    // its bytes by reference (lang:surfaceBlob, no inline lang:surfaceText) is also
    // clean — the gate flags only inline document-scale payload.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:small a lang:SurfaceForm , lang:UnanalyzedProse ;\n\
               lang:surfaceText \"cats chase mice\" .\n\
             ex:doc a lang:SurfaceForm , lang:UnanalyzedProse ;\n\
               lang:surfaceBlob \"blake3:deadbeef\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:InlineBlobPayload")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn noncontiguous_slots_flags_internal_gap() {
    // A composed form with slot indexes 0, 1, 3 — an internal gap at 2.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:cf a lang:ComposedForm ; lang:formSlot ex:s0 , ex:s1 , ex:s3 .\n\
             ex:s0 a lang:FormSlot ; lang:slotIndex 0 .\n\
             ex:s1 a lang:FormSlot ; lang:slotIndex 1 .\n\
             ex:s3 a lang:FormSlot ; lang:slotIndex 3 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:NonContiguousSlots")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn noncontiguous_slots_clean_for_zero_based_contiguous() {
    // A composed form with zero-based contiguous slot indexes 0, 1.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:cf a lang:ComposedForm ; lang:formSlot ex:s0 , ex:s1 .\n\
             ex:s0 a lang:FormSlot ; lang:slotIndex 0 .\n\
             ex:s1 a lang:FormSlot ; lang:slotIndex 1 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:NonContiguousSlots")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn silent_ingest_drop_flags_surface_in_limbo() {
    // A lang:SurfaceForm that neither realizes a form nor is typed UnanalyzedProse.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:s a lang:SurfaceForm ;\n\
               lang:surfaceText \"cats chase mice\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentIngestDrop")),
        "errors: {:?}",
        report.errors()
    );
}

#[test]
fn silent_ingest_drop_clean_when_realizes_or_unanalyzed() {
    // Either honest analysis status clears the gate: a surface that realizes an
    // analyzed form, and a surface explicitly typed unanalyzed prose.
    let ds = dataset_from(&format!(
        "{LANG_PREFIXES}\
             ex:s1 a lang:SurfaceForm ; lang:realizes ex:form .\n\
             ex:s2 a lang:SurfaceForm , lang:UnanalyzedProse ;\n\
               lang:surfaceText \"cats chase mice\" .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentIngestDrop")),
        "errors: {:?}",
        report.errors()
    );
}

fn form_and_ingest_counter_example_fixtures_fire_exactly_their_class() {
    // Each slice-resident counter-example for the native form/ingestion gates fires
    // exactly its named failure class (and none of the sibling classes), so the
    // (fixture, class) pair is load-bearing. The blob-payload gate reuses slot-gap.ttl
    // for contiguity — the shipped non-contiguous (0, 1, 3) counter-example.
    let inline_blob =
        "slices/grounding/lang/tests/counter-examples/surface-inline-blob-payload.ttl";
    let report = source_contracts::report(inline_blob, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:InlineBlobPayload")),
        "errors: {:?}",
        report.errors()
    );
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentIngestDrop") || e.contains("lang:NonContiguousSlots")),
        "the inline-blob fixture must fire only lang:InlineBlobPayload: {:?}",
        report.errors()
    );

    let gap = "slices/grounding/lang/tests/counter-examples/slot-gap.ttl";
    let report = source_contracts::report(gap, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:NonContiguousSlots")),
        "errors: {:?}",
        report.errors()
    );
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:InlineBlobPayload") || e.contains("lang:SilentIngestDrop")),
        "the slot-gap fixture must fire only lang:NonContiguousSlots: {:?}",
        report.errors()
    );

    let drop = "slices/grounding/lang/tests/counter-examples/ingest-silent-drop.ttl";
    let report = source_contracts::report(drop, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("lang:SilentIngestDrop")),
        "errors: {:?}",
        report.errors()
    );
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("lang:InlineBlobPayload") || e.contains("lang:NonContiguousSlots")),
        "the silent-drop fixture must fire only lang:SilentIngestDrop: {:?}",
        report.errors()
    );
}

fn form_and_ingest_positive_controls_are_clean() {
    // The shipped conforming fixtures clear the native form/ingestion gates: a
    // zero-based contiguous composed form, and a raw surface typed unanalyzed prose.
    for fixture in [
        "slices/grounding/lang/tests/conformance-fixtures/slot-contiguous.ttl",
        "slices/grounding/lang/tests/conformance-fixtures/surface-analyzed.ttl",
    ] {
        let report = source_contracts::report(fixture, &cfg());
        let report_errors = report.errors();
        let hits: Vec<&String> = report_errors
            .iter()
            .filter(|e| {
                e.contains("lang:InlineBlobPayload")
                    || e.contains("lang:NonContiguousSlots")
                    || e.contains("lang:SilentIngestDrop")
            })
            .collect();
        assert!(
            hits.is_empty(),
            "positive control must clear the native form/ingestion gates: {hits:?}"
        );
    }
}

// --- math: measure-and-dimension reasoned gate --------------------------- #

const MATH_PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix ex: <https://example.org/> .\n";

/// Grade all original authored-source lint contracts in one process, retaining
/// each contract's name and failure while authenticating one compact product.
#[test]
fn authored_lint_contracts_share_one_authenticated_observation() {
    let contracts: [(&str, fn()); 11] = [
        (
            "gmn_compaction_silent_disambiguation_fixture_fires_exactly_that_gate",
            gmn_compaction_silent_disambiguation_fixture_fires_exactly_that_gate,
        ),
        (
            "ingestion_counter_example_fixtures_fire_exactly_their_class",
            ingestion_counter_example_fixtures_fire_exactly_their_class,
        ),
        (
            "ambiguity_positive_fixture_is_clean_under_the_native_gates",
            ambiguity_positive_fixture_is_clean_under_the_native_gates,
        ),
        (
            "projection_counter_example_fixtures_fire_exactly_their_class",
            projection_counter_example_fixtures_fire_exactly_their_class,
        ),
        (
            "math_probability_counter_examples_fire_exactly_their_class",
            math_probability_counter_examples_fire_exactly_their_class,
        ),
        (
            "math_probability_clean_fixtures_fire_no_probability_class",
            math_probability_clean_fixtures_fire_no_probability_class,
        ),
        (
            "math_projection_counter_examples_fire_exactly_their_class",
            math_projection_counter_examples_fire_exactly_their_class,
        ),
        (
            "math_projection_clean_fixtures_fire_no_projection_class",
            math_projection_clean_fixtures_fire_no_projection_class,
        ),
        (
            "form_and_ingest_counter_example_fixtures_fire_exactly_their_class",
            form_and_ingest_counter_example_fixtures_fire_exactly_their_class,
        ),
        (
            "form_and_ingest_positive_controls_are_clean",
            form_and_ingest_positive_controls_are_clean,
        ),
        (
            "unliftable_ingest_fires_when_run_produces_no_codomain",
            unliftable_ingest_fires_when_run_produces_no_codomain,
        ),
    ];
    assert_eq!(
        contracts
            .iter()
            .map(|(name, _)| *name)
            .collect::<BTreeSet<_>>()
            .len(),
        11
    );
    let mut failures = Vec::new();
    for (name, contract) in contracts {
        if let Err(payload) = std::panic::catch_unwind(contract) {
            let detail = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_else(|| "non-string assertion panic".to_owned());
            failures.push(format!("{name}: {detail}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 11 authored lint contracts failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Removal check: structural lint stays silent on dimensional homogeneity,
/// while the production prepared verifier derives its marker using the exact
/// authenticated native `math:dimensionalHomogeneityLaw` product.
#[test]
fn dimensional_homogeneity_is_no_longer_a_validate_lint() {
    let turtle = format!(
        "{MATH_PREFIXES}\
             ex:t1 a math:Quantity ; math:hasDimension math:timeDimension .\n\
             ex:len a math:Quantity ; math:hasDimension math:lengthDimension .\n\
             ex:bad a math:DimensionalExpression ; math:homogeneousOperand ex:t1 , ex:len .\n"
    );
    let ds = dataset_from(&turtle);
    // The validate sweep is silent on the relocated invariant.
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:DimensionalInhomogeneity")),
        "the dimensional-homogeneity sweep must be retired from validate: {:?}",
        report.errors()
    );
    // The reasoner-materialized reason-verify gate is where it now lives: the
    // production prepared verifier surfaces the marker the native `logic:`
    // reasoner derives from the authored `math:dimensionalHomogeneityLaw`
    // `logic:Formula` AST, never a Rust sweep.
    let domains = gmeow_logic::reason::SelectedDomains::new([
        gmeow_logic::reason::SelectedLogicalWorld::new(
            gmeow_logic::reason::LogicalGraph::Default,
            gmeow_logic::reason::DomainProfile::NonemptyObjectDomainV1,
            "gmeow.validate.synthetic-dimensional-law.v1".to_owned(),
            [41; 32],
        )
        .expect("default dimensional theory"),
    ])
    .expect("selected dimensional theory");
    let report = source_contracts::verification()
        .verify(&ds, &domains)
        .expect("verify() must not error");
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "verify.dimensional-inhomogeneity"),
        "the reasoner-derived reason-verify gate must now be the sole homogeneity enforcement path: {:?}",
        report
            .findings
            .iter()
            .map(|f| f.code.as_str())
            .collect::<Vec<_>>()
    );
}

fn unliftable_ingest_fires_when_run_produces_no_codomain() {
    // The slice-resident counter-example for the native math:UnliftableIngest gate: a bridge
    // run that retains a source witness (math:parseSource) and its full grounding frame — so it
    // is NOT math:UngroundedIngestRun — but lifts no structured math: codomain (nothing is
    // gmeow:wasGeneratedBy it), silently dropping its content. Authored in the slice, not
    // inline here, so the (fixture, native-lint) pair is load-bearing rather than a Rust demo.
    let unliftable = "slices/grounding/math/tests/counter-examples/ingest-run-unliftable.ttl";
    let report = source_contracts::report(unliftable, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:UnliftableIngest")
                && e.contains("http://example.org/math/run")),
        "the slice-resident produced-nothing ingest run (parseSource, no gmeow:wasGeneratedBy) \
             must raise math:UnliftableIngest; errors: {:?}",
        report.errors()
    );
    // It retains its source, so the SHACL grounding twin is out of scope: the native gate must
    // not double-report the run as math:UngroundedIngestRun.
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:UngroundedIngestRun")),
        "the produced-nothing fixture retains math:parseSource, so it must not fire \
             math:UngroundedIngestRun: {:?}",
        report.errors()
    );
}

#[test]
fn unliftable_ingest_clean_when_run_produces_a_codomain() {
    // The same run, now lifting a structured math: object that points back through
    // gmeow:wasGeneratedBy: a full lift, no violation.
    let ds = dataset_from(&format!(
        "{MATH_PREFIXES}\
             ex:rRun a math:RIngestRun ;\n\
               math:parseSource ex:srcWitness .\n\
             ex:srcWitness a math:MathematicalObject .\n\
             ex:fittedModel a math:FittedModel ;\n\
               gmeow:wasGeneratedBy ex:rRun .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:UnliftableIngest")),
        "an ingest run that lifts a structured math: codomain must NOT raise \
             math:UnliftableIngest; errors: {:?}",
        report.errors()
    );
}

#[test]
fn string_only_computable_expression_fires_when_trigger_has_no_structured_child() {
    // A math:MathematicalExpression claiming a computable normal form
    // (math:normalForm) but carrying none of the four structured-child edges is
    // represented only by a string: the claim is unwarranted.
    let ds = dataset_from(&format!(
        "{MATH_PREFIXES}\
             ex:expr a math:MathematicalExpression ;\n\
               math:normalForm ex:nf .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:StringOnlyComputableExpression")
                && e.contains("https://example.org/expr")),
        "a math:MathematicalExpression with a trigger edge and no structured-child edge \
             must raise math:StringOnlyComputableExpression; errors: {:?}",
        report.errors()
    );
}

#[test]
fn string_only_computable_expression_clean_when_trigger_has_structured_child() {
    // The same trigger edge (math:normalForm), but this time backed by a
    // structured-child edge (math:argumentSlot): the computable claim is warranted,
    // so the gate must stay clean.
    let ds = dataset_from(&format!(
        "{MATH_PREFIXES}\
             ex:expr a math:MathematicalExpression ;\n\
               math:normalForm ex:nf ;\n\
               math:argumentSlot ex:arg0 .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:StringOnlyComputableExpression")),
        "a math:MathematicalExpression with a trigger edge AND a structured-child edge \
             must NOT raise math:StringOnlyComputableExpression; errors: {:?}",
        report.errors()
    );
}

#[test]
fn string_only_computable_expression_clean_when_no_trigger_edge() {
    // No trigger edge at all: the expression makes no computable claim, so the gate
    // is out of scope regardless of structured-child edges (none here either).
    let ds = dataset_from(&format!(
        "{MATH_PREFIXES}\
             ex:expr a math:MathematicalExpression .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:StringOnlyComputableExpression")),
        "a math:MathematicalExpression with no trigger edge must NOT raise \
             math:StringOnlyComputableExpression; errors: {:?}",
        report.errors()
    );
}

#[test]
fn ungrounded_result_claim_fires_when_observation_names_a_result_with_no_vantage() {
    // A gmeow:Observation naming a result through gmeow:observationResult but
    // carrying no gmeow:vantage is an unconditional property claim, never a held
    // observation: math:UngroundedResultClaim. This is a genuine cross-node
    // obligation over gmeow:Observation/gmeow:observationResult/gmeow:vantage, none
    // of which is math:-specific, so it carries no generated/-dependent SHACL twin
    // (the sole gate is this native Rust check).
    let ds = dataset_from(&format!(
        "{MATH_PREFIXES}\
             ex:obs a gmeow:Observation ;\n\
               gmeow:observationResult ex:result .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:UngroundedResultClaim")
                && e.contains("https://example.org/obs")),
        "an Observation naming a result with no vantage must raise \
             math:UngroundedResultClaim; errors: {:?}",
        report.errors()
    );
}

#[test]
fn ungrounded_result_claim_clean_when_observation_carries_a_vantage() {
    // The same result claim, but this time the Observation carries a gmeow:vantage:
    // the claim is held BY that vantage, so the gate must stay clean.
    let ds = dataset_from(&format!(
        "{MATH_PREFIXES}\
             ex:obs a gmeow:Observation ;\n\
               gmeow:observationResult ex:result ;\n\
               gmeow:vantage ex:analyst .\n"
    ));
    let report = structural_lint_dataset(&ds, &cfg());
    assert!(
        !report
            .errors()
            .iter()
            .any(|e| e.contains("math:UngroundedResultClaim")),
        "an Observation naming a result AND carrying a vantage must NOT raise \
             math:UngroundedResultClaim; errors: {:?}",
        report.errors()
    );
}

#[test]
fn ungrounded_result_claim_fires_under_a_non_default_namespace() {
    // Namespace-derivation regression: `check_ungrounded_result_claim` must derive its
    // gmeow:observationResult / gmeow:vantage / gmeow:Observation terms from
    // `cfg.namespace`, exactly like every sibling lookup in
    // `structural_lint_dataset` — never a hardcoded
    // `https://blackcatinformatics.ca/gmeow/` constant. `cfg()`'s default
    // namespace IS that constant, so a test built only against `cfg()` cannot
    // distinguish "derived from cfg" from "hardcoded" — this test uses a
    // DIFFERENT namespace for both the config and the data to prove the gate
    // still fires.
    let other_ns = "https://example.org/other-gmeow/";
    let other_cfg = LintConfig {
        namespace: other_ns.to_owned(),
        ontology_iri: "https://example.org/other-gmeow".to_owned(),
        ..cfg()
    };
    let ds = dataset_from(&format!(
        "@prefix gmeow: <{other_ns}> .\n\
             @prefix ex: <http://example.org/math/> .\n\
             ex:obs a gmeow:Observation ;\n\
               gmeow:observationResult ex:result .\n"
    ));
    let report = structural_lint_dataset(&ds, &other_cfg);
    assert!(
        report
            .errors()
            .iter()
            .any(|e| e.contains("math:UngroundedResultClaim")),
        "an Observation naming a result with no vantage under a NON-default \
             namespace must still raise math:UngroundedResultClaim; errors: {:?}",
        report.errors()
    );
}
