// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::slice::{ArtifactEvidence, ArtifactRole, EdgeEvidence};

const LOGIC: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";
const LANG: &str = "https://blackcatinformatics.ca/gmeow/slices/lang";
const MATH: &str = "https://blackcatinformatics.ca/gmeow/slices/math";
const CORE: &str = "https://blackcatinformatics.ca/gmeow/slices/core";
const EXT: &str = "https://blackcatinformatics.ca/gmeow/slices/ext";

fn nn(iri: &str) -> NamedNode {
    NamedNode::new(iri).unwrap()
}

fn vocab() -> purrdf::SliceVocab {
    gmeow_ns::gmeow_slice_vocab()
}

fn write_manifest(root: &Path, group: &str, name: &str, ttl: &str) {
    let dir = root.join("slices").join(group).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let prefixed = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
             @prefix math: <https://blackcatinformatics.ca/math/> .\n\
             {ttl}"
    );
    std::fs::write(dir.join("manifest.ttl"), prefixed).unwrap();
}

/// Write a slice's `module.ttl` (an ownership-bearing artifact, unlike
/// `manifest.ttl`) — needed to exercise [`ReferencePredicateIndex`]
/// (Class 2 / Class 3) and the slice-IRI-as-data (Class 1) filter, both of
/// which re-parse REAL artifact content off the catalog rather than
/// trusting a hand-built [`OwnershipReport`] fixture.
fn write_module(root: &Path, group: &str, name: &str, ttl: &str) {
    let dir = root.join("slices").join(group).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let prefixed = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
             @prefix math: <https://blackcatinformatics.ca/math/> .\n\
             {ttl}"
    );
    std::fs::write(dir.join("module.ttl"), prefixed).unwrap();
}

/// Write a slice's `mappings/<file>.ttl` (an [`ArtifactRole::Mapping`]
/// artifact) — needed to exercise [`ReferencePredicateIndex`]'s Class 5
/// `skos:relatedMatch` handling, which re-parses REAL mapping content off
/// the catalog rather than trusting a hand-built [`OwnershipReport`]
/// fixture.
fn write_mapping(root: &Path, group: &str, name: &str, file: &str, ttl: &str) {
    let dir = root.join("slices").join(group).join(name).join("mappings");
    std::fs::create_dir_all(&dir).unwrap();
    let prefixed = format!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             {ttl}"
    );
    std::fs::write(dir.join(file), prefixed).unwrap();
}

/// A catalog with the three grounding slices, mutually peered, `logic:`
/// hosting one seam `lang -> logic` carrying `logic:Foo`, and `math:`
/// hosting one seam `lang -> math` carrying `math:Quantity` (the real
/// `quantity` seam's direction — `math -> lang` is NOT sanctioned).
fn grounding_catalog(root: &Path) -> SliceCatalog {
    write_manifest(
        root,
        "grounding",
        "logic",
        r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/lang> ,
                    <https://blackcatinformatics.ca/gmeow/slices/math> .

            <https://blackcatinformatics.ca/gmeow/seam/test-seam>
                a gmeow:Seam ;
                rdfs:label "Test seam" ;
                gmeow:seamDirection [
                    gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;
                    gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/logic>
                ] ;
                gmeow:seamCarryingTerm logic:Foo ;
                gmeow:seamOwningDoc "TEST.md" .
            "#,
    );
    write_manifest(
        root,
        "grounding",
        "lang",
        r#"<https://blackcatinformatics.ca/gmeow/slices/lang>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "lang" ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/logic> ,
                    <https://blackcatinformatics.ca/gmeow/slices/math> .
            "#,
    );
    write_manifest(
        root,
        "grounding",
        "math",
        r#"<https://blackcatinformatics.ca/gmeow/slices/math>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "math" ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/logic> ,
                    <https://blackcatinformatics.ca/gmeow/slices/lang> .

            <https://blackcatinformatics.ca/gmeow/seam/quantity-seam>
                a gmeow:Seam ;
                rdfs:label "Quantity seam" ;
                gmeow:seamDirection [
                    gmeow:seamFromSlice <https://blackcatinformatics.ca/gmeow/slices/lang> ;
                    gmeow:seamToSlice <https://blackcatinformatics.ca/gmeow/slices/math>
                ] ;
                gmeow:seamCarryingTerm math:Quantity ;
                gmeow:seamOwningDoc "QUANTITY.md" .
            "#,
    );
    write_manifest(
        root,
        "core",
        "core",
        r#"<https://blackcatinformatics.ca/gmeow/slices/core>
                a gmeow:Slice ;
                rdfs:label "core" .
            "#,
    );
    write_manifest(
        root,
        "core",
        "ext",
        r#"<https://blackcatinformatics.ca/gmeow/slices/ext>
                a gmeow:Slice ;
                rdfs:label "ext" .
            "#,
    );
    SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
}

fn artifact_evidence(slice: &str) -> ArtifactEvidence {
    ArtifactEvidence {
        slice: slice.to_string(),
        role: ArtifactRole::Module,
        logical_path: "module.ttl".to_string(),
        raw_digest: "deadbeef".to_string(),
    }
}

/// An [`ArtifactEvidence`] for a specific role + logical path — needed to
/// exercise Class 4 (`ArtifactRole::CompetencyQuery`) and Class 5
/// (`ArtifactRole::Mapping`), which [`artifact_evidence`]'s hard-coded
/// `Module`/`module.ttl` cannot represent.
fn artifact_evidence_with(slice: &str, role: ArtifactRole, logical_path: &str) -> ArtifactEvidence {
    ArtifactEvidence {
        slice: slice.to_string(),
        role,
        logical_path: logical_path.to_string(),
        raw_digest: "deadbeef".to_string(),
    }
}

fn edge(
    from: &str,
    to: &str,
    kind: EdgeKind,
    terms: &[&str],
    reconciliation: ReconciliationStatus,
) -> DependencyEdge {
    DependencyEdge {
        from_slice: from.to_string(),
        to_slice: to.to_string(),
        edge_kind: kind,
        evidence: terms
            .iter()
            .map(|t| EdgeEvidence {
                from_artifact: artifact_evidence(from),
                referenced_term: nn(t),
            })
            .collect(),
        reconciliation,
    }
}

/// A [`DependencyEdge`] built from an explicit evidence list — needed when
/// a test must control per-evidence-entry `from_artifact` (role/logical
/// path), unlike [`edge`], which always attaches the uniform
/// `Module`/`module.ttl` [`artifact_evidence`].
fn edge_with_evidence(
    from: &str,
    to: &str,
    kind: EdgeKind,
    evidence: Vec<EdgeEvidence>,
    reconciliation: ReconciliationStatus,
) -> DependencyEdge {
    DependencyEdge {
        from_slice: from.to_string(),
        to_slice: to.to_string(),
        edge_kind: kind,
        evidence,
        reconciliation,
    }
}

fn undeclared_diag(from: &str, to: &str, kind: EdgeKind) -> OwnershipDiagnostic {
    OwnershipDiagnostic::UndeclaredDependency {
        from_slice: from.to_string(),
        to_slice: to.to_string(),
        edge_kind: kind,
    }
}

#[test]
fn covered_peer_seam_crossing_is_suppressed() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            LANG,
            LOGIC,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/logic/Foo"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(LANG, LOGIC, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).expect("classify must not hard-fail");
    assert_eq!(classification.verdicts.len(), 1);
    assert_eq!(classification.verdicts[0].coverage, Coverage::Covered);
    assert_eq!(classification.crossings.len(), 1);
    assert_eq!(
        classification.crossings[0].seam_iri,
        "https://blackcatinformatics.ca/gmeow/seam/test-seam"
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        findings.is_empty(),
        "a covered peer+seam crossing must not fire any finding: {findings:?}"
    );
}

#[test]
fn peered_crossing_with_an_off_seam_term_fires_error() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            LANG,
            LOGIC,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/logic/Bar"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(LANG, LOGIC, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).unwrap();
    match &classification.verdicts[0].coverage {
        Coverage::PeeredUnregisteredSeam { offending_terms } => {
            assert_eq!(
                offending_terms,
                &vec![nn("https://blackcatinformatics.ca/logic/Bar")]
            );
        }
        other => panic!("expected PeeredUnregisteredSeam, got {other:?}"),
    }

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "slice-ownership.peered-unregistered-seam");
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(findings[0].message.contains("Bar"));
    assert!(findings[0].message.contains(LANG));
    assert!(findings[0].message.contains(LOGIC));
}

#[test]
fn reverse_direction_of_a_registered_seam_is_not_covered() {
    // The quantity seam sanctions lang -> math carrying math:Quantity; the
    // REVERSE crossing (math -> lang) referencing the same term must not
    // ride free on it.
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            MATH,
            LANG,
            EdgeKind::Mapping,
            &["https://blackcatinformatics.ca/math/Quantity"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(MATH, LANG, EdgeKind::Mapping)],
    };

    let classification = classify(&report, &catalog).unwrap();
    assert_ne!(classification.verdicts[0].coverage, Coverage::Covered);
    assert!(matches!(
        classification.verdicts[0].coverage,
        Coverage::PeeredUnregisteredSeam { .. }
    ));

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "slice-ownership.peered-unregistered-seam");
}

#[test]
fn uncovered_non_peer_undeclared_dependency_stays_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            CORE,
            EXT,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/SomeTerm"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(CORE, EXT, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).unwrap();
    assert_eq!(classification.verdicts[0].coverage, Coverage::Uncovered);

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
    assert_eq!(findings[0].severity, Severity::Error);
}

#[test]
fn a_semantic_undeclared_diagnostic_with_no_matching_edge_hard_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: Vec::new(),
        diagnostics: vec![undeclared_diag(LANG, LOGIC, EdgeKind::Ontology)],
    };

    let result = classify(&report, &catalog);
    assert!(
        result.is_err(),
        "a semantic UndeclaredDependency diagnostic with no matching edge must hard-fail"
    );
}

#[test]
fn a_non_semantic_edge_kind_diagnostic_is_ignored_even_with_no_matching_edge() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: Vec::new(),
        diagnostics: vec![undeclared_diag(LANG, LOGIC, EdgeKind::Test)],
    };

    let classification =
            classify(&report, &catalog).expect("non-semantic edge kinds are filtered before the join, never hard-failing on a missing edge");
    assert!(classification.verdicts.is_empty());

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(findings.is_empty());
}

#[test]
fn seam_records_of_reads_directions_and_raw_term_iris() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_catalog(tmp.path());
    let seams = seam_registry(&catalog).unwrap();
    let test_seam = seams
        .iter()
        .find(|s| s.name == "Test seam")
        .expect("test-seam present");
    assert_eq!(
        test_seam.directions,
        vec![(LANG.to_string(), LOGIC.to_string())]
    );
    assert!(
        test_seam
            .carrying_term_iris
            .contains("https://blackcatinformatics.ca/logic/Foo")
    );
    assert!(test_seam.carrying_terms.contains("logic:Foo"));
}

// ── Class A / Class B genuine-crossing-term exclusion ────────────────────

const QUALITY: &str = "https://blackcatinformatics.ca/gmeow/slices/quality";
const WIDGETS: &str = "https://blackcatinformatics.ca/gmeow/slices/widgets";
const GUIDES: &str = "https://blackcatinformatics.ca/gmeow/slices/guides";

/// A catalog with `logic:` (a lone, seam-free grounding slice — the
/// peerage/seam machinery is irrelevant to these tests) plus three
/// ordinary domain slices, `quality`, `widgets`, and `guides`. Dedicated
/// to the Class A (slice-IRI-as-data) and Class B (corpus-declared
/// non-coupling predicates) exclusion tests, which need REAL artifact
/// content parsed off disk (`slice_iris` and [`ReferencePredicateIndex`]
/// re-parse the catalog directly) — a hand-built [`OwnershipReport`]
/// fixture alone can never exercise them.
///
/// The predicate DECLARATIONS here mirror the real corpus exactly, because
/// [`NonCouplingPredicates`] derives its whole set from them and from
/// nothing else:
///
/// * `logic:formalizes` — `owl:AnnotationProperty` (real corpus: same);
/// * `logic:characterizes` — `owl:ObjectProperty` carrying
///   `gmeow:graphBoxRole gmeow:boxRBox` (real corpus: same), i.e. a reasoned
///   RBox axiom, which is therefore COUPLING;
/// * `logic:relation` — `owl:ObjectProperty` with a narrow range (real
///   corpus: a `logic:Formula` AST slot), also coupling;
/// * `gmeow:usesTerm` — `owl:ObjectProperty` with `rdfs:range rdfs:Resource`
///   (real corpus: same), i.e. an explicitly open range, non-coupling.
///
/// `guides` is deliberately a PLAIN (non-`GroundingSlice`) domain slice,
/// proving the exclusion is a property of the PREDICATE and applies from any
/// slice, not only the three grounding ones.
fn class_filter_catalog(root: &Path) -> SliceCatalog {
    write_manifest(
        root,
        "grounding",
        "logic",
        r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" .
            "#,
    );
    write_module(
        root,
        "grounding",
        "logic",
        r#"logic:formalizes
                a owl:AnnotationProperty ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> .

            logic:characterizes
                a owl:ObjectProperty ;
                rdfs:range gmeow:Facet ;
                gmeow:graphBoxRole gmeow:boxRBox ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> .

            logic:relation
                a owl:ObjectProperty ;
                rdfs:range gmeow:Relation ;
                gmeow:graphBoxRole gmeow:boxRBox ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> .

            logic:widgetFacetFormalization
                a owl:NamedIndividual ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:formalizes gmeow:widgetFacet .

            logic:widgetFacetCharacteristic
                a owl:NamedIndividual , logic:PropertyCharacteristicAssertion ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:characterizes gmeow:widgetCharacterizedTerm ;
                logic:formalizes gmeow:widgetCharacterizedTerm .

            logic:widgetOtherTermUsage
                a logic:Formula ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:relation gmeow:widgetOtherTerm .

            logic:widgetBothTermUsage
                a logic:Formula ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/logic> ;
                logic:relation gmeow:widgetBothTerm ;
                logic:formalizes gmeow:widgetBothTerm .
            "#,
    );
    write_manifest(
        root,
        "core",
        "quality",
        r#"<https://blackcatinformatics.ca/gmeow/slices/quality>
                a gmeow:Slice ;
                rdfs:label "quality" .
            "#,
    );
    write_manifest(
        root,
        "core",
        "widgets",
        r#"<https://blackcatinformatics.ca/gmeow/slices/widgets>
                a gmeow:Slice ;
                rdfs:label "widgets" .
            "#,
    );
    write_mapping(
        root,
        "core",
        "quality",
        "widgets-correspondences.ttl",
        r#"gmeow:qualityRelatedMatchOnlyTerm
                a rdfs:Resource ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/quality> ;
                skos:relatedMatch gmeow:widgetRelatedMatchOnlyTerm .

            gmeow:qualityRelatedMatchAndStructuralTerm
                a rdfs:Resource ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/quality> ;
                skos:relatedMatch gmeow:widgetRelatedMatchAndStructuralTerm ;
                gmeow:relatedTerm gmeow:widgetRelatedMatchAndStructuralTerm .
            "#,
    );
    write_manifest(
        root,
        "core",
        "guides",
        r#"<https://blackcatinformatics.ca/gmeow/slices/guides>
                a gmeow:Slice ;
                rdfs:label "guides" .
            "#,
    );
    write_module(
        root,
        "core",
        "guides",
        r#"gmeow:usesTerm
                a owl:ObjectProperty ;
                rdfs:range rdfs:Resource ;
                gmeow:graphBoxRole gmeow:boxRBox ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/guides> .

            gmeow:relatedTerm
                a owl:ObjectProperty ;
                rdfs:range gmeow:Term ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/guides> .

            gmeow:guideWidgetDocOnly
                a gmeow:Recipe ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/guides> ;
                gmeow:usesTerm gmeow:widgetDocOnlyTerm .

            gmeow:guideWidgetDocAndReal
                a gmeow:Recipe ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/guides> ;
                gmeow:usesTerm gmeow:widgetDocAndRealTerm ;
                gmeow:relatedTerm gmeow:widgetDocAndRealTerm .
            "#,
    );
    SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
}

/// Class A is REACHABLE, not dead code: prove that `purrdf`'s ownership
/// analyzer really does admit a slice's own IRI into `validated_owner`, so
/// the [`slice_iris`] filter is live. Every slice `module.ttl` declares its
/// own slice IRI as the module's `owl:Ontology` header with
/// `rdfs:isDefinedBy <itself>`, and that IRI is inside the `gmeow:` vocab
/// namespace, so Phase 1's `subject.starts_with(vocab_ns)` harvest admits it
/// and Phase 2 validates it (physical origin == declared owner). Without
/// this, `is_ownership_bearing == Module | Shapes` would suggest a slice IRI
/// can never become an owned term — it can.
#[test]
fn a_slice_iri_really_is_a_validated_owned_term_so_class_a_is_reachable() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_manifest(
        root,
        "core",
        "owner",
        r#"<https://blackcatinformatics.ca/gmeow/slices/owner>
                a gmeow:Slice ;
                rdfs:label "owner" .
            "#,
    );
    // The real shape every slice module.ttl carries: the slice IRI as the
    // module's owl:Ontology header, defined by itself.
    write_module(
        root,
        "core",
        "owner",
        r#"<https://blackcatinformatics.ca/gmeow/slices/owner>
                a owl:Ontology ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/owner> ;
                rdfs:label "Owner module" .
            "#,
    );
    write_manifest(
        root,
        "core",
        "citer",
        r#"<https://blackcatinformatics.ca/gmeow/slices/citer>
                a gmeow:Slice ;
                rdfs:label "citer" .
            "#,
    );
    // The real `slice-quality-rubric` shape: an ABox record naming ANOTHER
    // slice's IRI as the assessment target.
    write_module(
        root,
        "core",
        "citer",
        r#"<https://blackcatinformatics.ca/gmeow/slices/citer>
                a owl:Ontology ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/citer> .

            gmeow:citerRubricRecord
                a owl:NamedIndividual ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/citer> ;
                gmeow:ceilingSlice <https://blackcatinformatics.ca/gmeow/slices/owner> .
            "#,
    );
    let catalog = SliceCatalog::discover(&root.join("slices"), vocab()).unwrap();
    let report = purrdf::slice::OwnershipAnalyzer::new(&catalog)
        .analyze()
        .unwrap();

    let owner_iri = "https://blackcatinformatics.ca/gmeow/slices/owner";
    let record = report
        .ownership
        .get(&nn(owner_iri))
        .expect("the slice IRI must be an owned term at all — Class A's whole premise");
    assert_eq!(
        record.status,
        purrdf::slice::OwnershipStatus::Validated,
        "the slice IRI must be VALIDATED-owned, which is what puts it into \
             validated_owner and lets it produce real dependency edges"
    );
    // And it really did produce an edge whose only evidence is that IRI.
    let edge = report
        .edges
        .iter()
        .find(|e| e.to_slice == owner_iri)
        .expect("the slice-IRI citation produced a real dependency edge");
    assert!(
        edge.evidence
            .iter()
            .all(|e| e.referenced_term.as_str() == owner_iri),
        "the edge's evidence is the slice IRI itself: {:?}",
        edge.evidence
    );
    // Class A is what keeps it from gating.
    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        findings.is_empty(),
        "Class A must suppress a pure slice-IRI-as-data crossing: {findings:?}"
    );
}

#[test]
fn class_a_slice_iri_as_data_crossing_is_suppressed() {
    // `quality`'s module references `widgets`' own SLICE IRI as DATA (the
    // real-world `slice-quality-rubric` `gmeow:ceilingSlice
    // <…/slices/widgets>` shape) — never one of `widgets`' vocabulary
    // terms, so this must never surface as a dependency at all.
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            QUALITY,
            WIDGETS,
            EdgeKind::Ontology,
            &[WIDGETS],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).expect("classify must not hard-fail");
    assert!(
        classification.verdicts.is_empty(),
        "a slice-IRI-as-data crossing has zero genuine terms and must be suppressed \
             entirely: {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        findings.is_empty(),
        "a slice-IRI-as-data crossing must never fire a finding: {findings:?}"
    );
}

/// The Class B set must come out of the CORPUS, not out of a source-level
/// list: assert the two declaration tests actually pick up exactly the
/// fixture's declarations, and — load-bearing — that a `boxRBox`
/// `owl:ObjectProperty` is NOT in it.
#[test]
fn class_b_non_coupling_predicates_are_derived_from_the_corpus_declarations() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let index = CorpusIndex::build(&catalog).unwrap().reference_predicates;

    assert!(
        index
            .non_coupling
            .annotation
            .contains("https://blackcatinformatics.ca/logic/formalizes"),
        "logic:formalizes is declared owl:AnnotationProperty: {:?}",
        index.non_coupling.annotation
    );
    assert!(
        index
            .non_coupling
            .open_range
            .contains("https://blackcatinformatics.ca/gmeow/usesTerm"),
        "gmeow:usesTerm is declared rdfs:range rdfs:Resource: {:?}",
        index.non_coupling.open_range
    );
    // The whole point of the derivation: a reasoned RBox object property is
    // NOT meta, however much it looks like law bookkeeping.
    for reasoned in [
        "https://blackcatinformatics.ca/logic/characterizes",
        "https://blackcatinformatics.ca/logic/relation",
        "https://blackcatinformatics.ca/gmeow/relatedTerm",
    ] {
        assert!(
            !index.non_coupling.contains(reasoned),
            "{reasoned} is a declared owl:ObjectProperty carrying real axiom weight and must \
                 never be treated as non-coupling"
        );
    }
    // An undeclared, purely external predicate satisfies neither test.
    assert!(
        !index
            .non_coupling
            .contains("http://www.w3.org/2004/02/skos/core#relatedMatch"),
        "an external predicate the corpus never declares can never be non-coupling"
    );
}

#[test]
fn class_b_annotation_property_crossing_is_suppressed() {
    // `logic:`'s module names `gmeow:widgetFacet` ONLY via
    // `logic:formalizes`, declared `a owl:AnnotationProperty` in the same
    // corpus — no logical axiom, so no build dependency.
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            LOGIC,
            WIDGETS,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/widgetFacet"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(LOGIC, WIDGETS, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).expect("classify must not hard-fail");
    assert!(
        classification.verdicts.is_empty(),
        "a pure owl:AnnotationProperty crossing has zero genuine terms and must be \
             suppressed entirely: {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        findings.is_empty(),
        "a pure owl:AnnotationProperty crossing must never fire a finding: {findings:?}"
    );
}

/// The regression the hard-coded 21-IRI `GROUNDING_META_PREDICATES` list
/// hid: `logic:characterizes` is an `owl:ObjectProperty` carrying
/// `gmeow:graphBoxRole gmeow:boxRBox` — a reasoned RBox axiom feeding the
/// live DL consistency gate, NOT an annotation. A term named via it (even
/// alongside a genuine annotation) is real term usage and must surface.
#[test]
fn a_reasoned_rbox_object_property_is_no_longer_exempt() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            LOGIC,
            WIDGETS,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/widgetCharacterizedTerm"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(LOGIC, WIDGETS, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).unwrap();
    assert_eq!(
        classification.verdicts.len(),
        1,
        "logic:characterizes is a reasoned RBox object property, never a meta annotation: \
             {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY),
        "{findings:?}"
    );
}

#[test]
fn genuine_term_use_from_a_grounding_slice_still_fires() {
    // `logic:`'s module names `gmeow:widgetOtherTerm` via `logic:relation`
    // — a REAL `logic:Formula` predication (e.g.
    // `logic:coverEntitySortals`'s class-covering pattern), not a
    // law/formalization back-reference — so this crossing is genuine.
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            LOGIC,
            WIDGETS,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/widgetOtherTerm"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(LOGIC, WIDGETS, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).unwrap();
    assert_eq!(classification.verdicts.len(), 1);
    assert_eq!(classification.verdicts[0].coverage, Coverage::Uncovered);

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    let undeclared: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY)
        .collect();
    assert_eq!(undeclared.len(), 1, "{findings:?}");
    assert_eq!(undeclared[0].severity, Severity::Error);
    // `logic:` is a gmeow:GroundingSlice and `widgets` is not, but
    // `gmeow:widgetOtherTerm` carries no gmeow:groundingConceptDomain
    // marker: it is ordinary domain vocabulary, which docs/GROUNDING.md's
    // tier rule explicitly permits a grounding slice to consume. The
    // crossing is an UNDECLARED dependency (declare it) and nothing more.
    assert!(
        !findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
        "the tier rule is qualified 'for a grounding concept': {findings:?}"
    );
}

#[test]
fn a_term_named_via_both_a_meta_predicate_and_a_real_use_is_never_excluded() {
    // `logic:`'s module names `gmeow:widgetBothTerm` via BOTH
    // `logic:formalizes` (meta) AND `logic:relation` (a real `logic:Formula`
    // predication) — per spec, co-presence of a real use means the term is
    // NEVER excluded, even though a meta predicate also names it.
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            LOGIC,
            WIDGETS,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/widgetBothTerm"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(LOGIC, WIDGETS, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).unwrap();
    assert_eq!(
        classification.verdicts.len(),
        1,
        "a term with a genuine co-use must never be suppressed: {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY),
        "{findings:?}"
    );
}

#[test]
fn class_b_open_range_uses_term_crossing_is_suppressed_from_any_slice() {
    // `guides` (a PLAIN domain slice — not one of the three grounding
    // slices) names `gmeow:widgetDocOnlyTerm` ONLY via `gmeow:usesTerm` —
    // the documentation-index reference a `gmeow:Recipe` makes to "any
    // documented term across any slice" — never a real build dependency
    // on `widgets`, so this crossing must be suppressed exactly like a
    // pure Class 2 grounding-formalization crossing.
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            GUIDES,
            WIDGETS,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/widgetDocOnlyTerm"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(GUIDES, WIDGETS, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).expect("classify must not hard-fail");
    assert!(
        classification.verdicts.is_empty(),
        "a pure gmeow:usesTerm documentation crossing has zero genuine terms and must be \
             suppressed entirely: {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        findings.is_empty(),
        "a pure gmeow:usesTerm documentation crossing must never fire a finding: {findings:?}"
    );
}

#[test]
fn a_term_named_via_both_uses_term_and_a_real_use_is_never_excluded() {
    // `guides`' module names `gmeow:widgetDocAndRealTerm` via BOTH
    // `gmeow:usesTerm` (documentation index) AND `gmeow:relatedTerm` (a
    // stand-in for a real, non-meta object-level predication) — per spec,
    // co-presence of a real use means the term is NEVER excluded, even
    // though `gmeow:usesTerm` also names it.
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            GUIDES,
            WIDGETS,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/widgetDocAndRealTerm"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(GUIDES, WIDGETS, EdgeKind::Ontology)],
    };

    let classification = classify(&report, &catalog).unwrap();
    assert_eq!(
        classification.verdicts.len(),
        1,
        "a term with a genuine co-use alongside gmeow:usesTerm must never be suppressed: {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
}

// ── Previously-blanket exclusions that are now GONE ──────────────────────

/// The `ArtifactRole::CompetencyQuery` blanket exclusion is DELETED.
/// `purrdf` maps `CompetencyQuery | VerifyQuery -> EdgeKind::Query`, one of
/// only four semantic edge kinds, so excluding the role removed essentially
/// the whole `Query` edge kind from both the dependency rule and the tier
/// rule — and `VerifyQuery`, the identical shape, was never excluded. A
/// competency query that references another slice's term is a real crossing.
#[test]
fn a_competency_query_crossing_now_surfaces() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge_with_evidence(
            QUALITY,
            WIDGETS,
            EdgeKind::Query,
            vec![EdgeEvidence {
                from_artifact: artifact_evidence_with(
                    QUALITY,
                    ArtifactRole::CompetencyQuery,
                    "queries/competency/can-quality-answer.rq",
                ),
                referenced_term: nn("https://blackcatinformatics.ca/gmeow/widgetCompetencyTerm"),
            }],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Query)],
    };

    let classification = classify(&report, &catalog).expect("classify must not hard-fail");
    assert_eq!(
        classification.verdicts.len(),
        1,
        "a competency-query crossing is a real Query-kind dependency: {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].code,
        crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY
    );
}

/// A `VerifyQuery` crossing behaves IDENTICALLY to a `CompetencyQuery` one
/// — the two roles map to the same `EdgeKind::Query`, and neither is
/// role-exempt any more. This is the asymmetry the deleted exclusion
/// created.
#[test]
fn a_verify_query_crossing_surfaces_identically_to_a_competency_query() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let build = |role: ArtifactRole, path: &str| OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge_with_evidence(
            QUALITY,
            WIDGETS,
            EdgeKind::Query,
            vec![EdgeEvidence {
                from_artifact: artifact_evidence_with(QUALITY, role, path),
                referenced_term: nn("https://blackcatinformatics.ca/gmeow/widgetQueryTerm"),
            }],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Query)],
    };
    let competency = peerage_aware_ownership_findings(
        &build(ArtifactRole::CompetencyQuery, "queries/competency/thing.rq"),
        &catalog,
    )
    .unwrap();
    let verify = peerage_aware_ownership_findings(
        &build(ArtifactRole::VerifyQuery, "queries/verify/thing.rq"),
        &catalog,
    )
    .unwrap();
    assert_eq!(competency.len(), 1, "{competency:?}");
    assert_eq!(
        competency.iter().map(|f| &f.code).collect::<Vec<_>>(),
        verify.iter().map(|f| &f.code).collect::<Vec<_>>(),
        "CompetencyQuery and VerifyQuery are the same EdgeKind::Query shape and must gate \
             identically"
    );
}

/// The `skos:relatedMatch`-in-`Mapping` blanket exclusion is DELETED.
/// `skos:relatedMatch` is a purely EXTERNAL predicate the corpus declares
/// nowhere: it is neither an `owl:AnnotationProperty` nor open-ranged in
/// this ontology, so it passes neither [`NonCouplingPredicates`] test.
/// Keeping it exempt would mean re-hard-coding an IRI — exactly the defect
/// the derived set removes. It is also a real symmetric SKOS object
/// property, and `ArtifactRole::Mapping -> EdgeKind::Mapping` is a
/// `is_semantic()` kind by construction.
#[test]
fn an_internal_related_match_crossing_now_surfaces() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge_with_evidence(
            QUALITY,
            WIDGETS,
            EdgeKind::Mapping,
            vec![EdgeEvidence {
                from_artifact: artifact_evidence_with(
                    QUALITY,
                    ArtifactRole::Mapping,
                    "mappings/widgets-correspondences.ttl",
                ),
                referenced_term: nn(
                    "https://blackcatinformatics.ca/gmeow/widgetRelatedMatchOnlyTerm",
                ),
            }],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Mapping)],
    };

    let classification = classify(&report, &catalog).expect("classify must not hard-fail");
    assert_eq!(
        classification.verdicts.len(),
        1,
        "an internal skos:relatedMatch crossing is a real Mapping-kind dependency: {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].code,
        crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY
    );
}

#[test]
fn a_term_named_via_related_match_and_a_structural_use_is_never_excluded() {
    // `quality`'s mapping names `gmeow:widgetRelatedMatchAndStructuralTerm`
    // via BOTH `skos:relatedMatch` (correspondence) AND `gmeow:relatedTerm`
    // (a stand-in for a real, non-meta object-level predication in the
    // SAME artifact) — per spec, co-presence of a structural use means the
    // term is NEVER excluded, even though `skos:relatedMatch` also names
    // it.
    let tmp = tempfile::tempdir().unwrap();
    let catalog = class_filter_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge_with_evidence(
            QUALITY,
            WIDGETS,
            EdgeKind::Mapping,
            vec![EdgeEvidence {
                from_artifact: artifact_evidence_with(
                    QUALITY,
                    ArtifactRole::Mapping,
                    "mappings/widgets-correspondences.ttl",
                ),
                referenced_term: nn(
                    "https://blackcatinformatics.ca/gmeow/widgetRelatedMatchAndStructuralTerm",
                ),
            }],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(QUALITY, WIDGETS, EdgeKind::Mapping)],
    };

    let classification = classify(&report, &catalog).unwrap();
    assert_eq!(
        classification.verdicts.len(),
        1,
        "a term with a genuine structural co-use must never be suppressed: {:?}",
        classification.verdicts
    );

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, "slice-ownership.undeclared-dependency");
}

// ── R5: tier-forbidden edges ──────────────────────────────────────────────

const TIER_CORE_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/tier-core";
const TIER_EXT_A: &str = "https://blackcatinformatics.ca/gmeow/slices/tier-ext-a";
const TIER_EXT_B: &str = "https://blackcatinformatics.ca/gmeow/slices/tier-ext-b";

/// A catalog with one `tierCore` slice and two `tierExtension` slices —
/// dedicated to the R5 forbidden-tier gate so it never shares (and can
/// never accidentally perturb) `grounding_catalog`'s tierless CORE/EXT
/// fixture the peerage-coverage tests above depend on.
fn tier_catalog(root: &Path) -> SliceCatalog {
    write_manifest(
        root,
        "core",
        "tier-core",
        r#"<https://blackcatinformatics.ca/gmeow/slices/tier-core>
                a gmeow:Slice ;
                rdfs:label "tier-core" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
    );
    write_manifest(
        root,
        "extensions",
        "tier-ext-a",
        r#"<https://blackcatinformatics.ca/gmeow/slices/tier-ext-a>
                a gmeow:Slice ;
                rdfs:label "tier-ext-a" ;
                gmeow:sliceTier gmeow:tierExtension .
            "#,
    );
    write_manifest(
        root,
        "extensions",
        "tier-ext-b",
        r#"<https://blackcatinformatics.ca/gmeow/slices/tier-ext-b>
                a gmeow:Slice ;
                rdfs:label "tier-ext-b" ;
                gmeow:sliceTier gmeow:tierExtension .
            "#,
    );
    SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
}

#[test]
fn core_depending_on_extension_is_forbidden() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = tier_catalog(tmp.path());
    // A MATCHED (authored) edge — declaring it does not license it.
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            TIER_CORE_SLICE,
            TIER_EXT_A,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/SomeTerm"],
            ReconciliationStatus::Matched,
        )],
        diagnostics: Vec::new(),
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(
        findings[0].code,
        crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY
    );
    assert_eq!(findings[0].severity, Severity::Error);
    assert!(findings[0].message.contains(TIER_CORE_SLICE));
    assert!(findings[0].message.contains(TIER_EXT_A));
}

#[test]
fn extension_depending_on_another_extension_is_forbidden() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = tier_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            TIER_EXT_A,
            TIER_EXT_B,
            EdgeKind::Mapping,
            &["https://blackcatinformatics.ca/gmeow/OtherTerm"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(TIER_EXT_A, TIER_EXT_B, EdgeKind::Mapping)],
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    let forbidden: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY)
        .collect();
    assert_eq!(forbidden.len(), 1, "{findings:?}");
    assert_eq!(forbidden[0].severity, Severity::Error);
    // The ordinary undeclared-dependency observation ALSO fires — a
    // forbidden tier crossing is an additional, independent violation, not
    // a replacement for the undeclared-dependency finding.
    assert!(
        findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_UNDECLARED_DEPENDENCY)
    );
}

#[test]
fn extension_depending_on_core_is_not_forbidden() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = tier_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            TIER_EXT_A,
            TIER_CORE_SLICE,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/gmeow/SomeTerm"],
            ReconciliationStatus::Matched,
        )],
        diagnostics: Vec::new(),
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        findings.is_empty(),
        "extension -> core is the ordinary direction, never forbidden: {findings:?}"
    );
}

/// A catalog whose `tier-core` slice AUTHORS `gmeow:sliceDependsOn` on a
/// `tierExtension` slice, with no computed edge and no evidence at all.
fn declared_forbidden_catalog(root: &Path) -> SliceCatalog {
    write_manifest(
        root,
        "core",
        "tier-core",
        r#"<https://blackcatinformatics.ca/gmeow/slices/tier-core>
                a gmeow:Slice ;
                rdfs:label "tier-core" ;
                gmeow:sliceTier gmeow:tierCore ;
                gmeow:sliceDependsOn <https://blackcatinformatics.ca/gmeow/slices/tier-ext-a> .
            "#,
    );
    write_manifest(
        root,
        "extensions",
        "tier-ext-a",
        r#"<https://blackcatinformatics.ca/gmeow/slices/tier-ext-a>
                a gmeow:Slice ;
                rdfs:label "tier-ext-a" ;
                gmeow:sliceTier gmeow:tierExtension .
            "#,
    );
    SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
}

/// R5 must judge the DECLARED `gmeow:sliceDependsOn` set, not only computed
/// edges. Before this, an edge whose evidence was fully exempted was
/// `continue`d BEFORE the tier test, so a declared-forbidden crossing was
/// invisible to the forbidden gate — contradicting the code's own doc
/// ("declaring it does not license it"). A declaration needs no evidence.
#[test]
fn a_declared_forbidden_crossing_fires_with_no_evidence_at_all() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = declared_forbidden_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: Vec::new(),
        diagnostics: Vec::new(),
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    let forbidden: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY)
        .collect();
    assert_eq!(
        forbidden.len(),
        1,
        "a declared core -> extension crossing is forbidden on the declaration alone: \
             {findings:?}"
    );
    assert!(
        forbidden[0]
            .message
            .contains("declared gmeow:sliceDependsOn"),
        "the finding must name the declaration as its witness: {}",
        forbidden[0].message
    );
    assert!(
        forbidden[0].message.contains("tierCore") && forbidden[0].message.contains("tierExtension"),
        "the finding must name both tiers: {}",
        forbidden[0].message
    );
}

/// Even when EVERY piece of an edge's evidence is exempt (a pure
/// slice-IRI-as-data crossing), a matching DECLARATION still fires R5. This
/// is the exact hole the pre-declaration gate had: the evidence filter
/// `continue`d the edge before the tier test could see it.
#[test]
fn a_declared_forbidden_crossing_fires_even_when_all_evidence_is_exempt() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = declared_forbidden_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        // The only evidence is the target slice's own IRI — Class A, fully
        // exempt, so the computed leg contributes nothing.
        edges: vec![edge(
            TIER_CORE_SLICE,
            TIER_EXT_A,
            EdgeKind::Ontology,
            &[TIER_EXT_A],
            ReconciliationStatus::Matched,
        )],
        diagnostics: Vec::new(),
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY)
            .count(),
        1,
        "{findings:?}"
    );
}

// ── R6: grounding doctrine ────────────────────────────────────────────────

const GROUNDING_LOGIC: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";
const GROUNDING_MATH: &str = "https://blackcatinformatics.ca/gmeow/slices/math";
const DOMAIN_COGNITION: &str = "https://blackcatinformatics.ca/gmeow/slices/cognition";
/// The `gmeow:GroundingDomain` individual `logic:` declares in its manifest.
const DOMAIN_LOGICAL: &str = "https://blackcatinformatics.ca/gmeow/groundingDomainLogical";
/// A term in `cognition` that IS a grounding concept: a knowledge-base
/// partition role, i.e. a logical formalism. Marked as such in the fixture.
const MARKED_CONCEPT: &str = "https://blackcatinformatics.ca/gmeow/kbPartitionRole";
/// A term in `cognition` that is ordinary domain vocabulary — unmarked.
const ORDINARY_TERM: &str = "https://blackcatinformatics.ca/gmeow/beliefState";

/// Two mutually-peered `gmeow:GroundingSlice`s and one ordinary domain
/// slice, ALL `gmeow:tierCore` — exactly the real corpus's shape, which is
/// why `is_forbidden_edge` can never see this violation: every crossing
/// among them is core -> core and therefore tier-legal.
///
/// `logic:`'s manifest declares the logical `gmeow:GroundingDomain` (as the
/// real one does), and `cognition`'s `module.ttl` owns TWO terms: one
/// carrying the `gmeow:groundingConceptDomain` marker and one without it.
/// The pair is what makes the gate's discrimination testable — the same
/// slice pair, the same crossing direction, opposite verdicts, decided
/// ONLY by the authored marker.
fn grounding_doctrine_catalog(root: &Path) -> SliceCatalog {
    write_manifest(
        root,
        "grounding",
        "logic",
        r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" ;
                gmeow:sliceTier gmeow:tierCore ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/math> .

            <https://blackcatinformatics.ca/gmeow/groundingDomainLogical>
                a gmeow:GroundingDomain ;
                rdfs:label "logical grounding domain" ;
                gmeow:groundingDomainOwner <https://blackcatinformatics.ca/gmeow/slices/logic> .
            "#,
    );
    write_manifest(
        root,
        "grounding",
        "math",
        r#"<https://blackcatinformatics.ca/gmeow/slices/math>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "math" ;
                gmeow:sliceTier gmeow:tierCore ;
                gmeow:sliceCoFoundationalWith <https://blackcatinformatics.ca/gmeow/slices/logic> .
            "#,
    );
    write_manifest(
        root,
        "core",
        "cognition",
        r#"<https://blackcatinformatics.ca/gmeow/slices/cognition>
                a gmeow:Slice ;
                rdfs:label "cognition" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
    );
    write_module(
        root,
        "core",
        "cognition",
        r#"gmeow:kbPartitionRole
                a owl:ObjectProperty ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/cognition> ;
                rdfs:label "kb partition role" ;
                gmeow:groundingConceptDomain <https://blackcatinformatics.ca/gmeow/groundingDomainLogical> .

            gmeow:beliefState
                a owl:ObjectProperty ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/cognition> ;
                rdfs:label "belief state" .
            "#,
    );
    SliceCatalog::discover(&root.join("slices"), vocab()).unwrap()
}

/// Proof the gate is NEEDED: the identical crossing is tier-legal, so R5
/// alone would let it through silently.
#[test]
fn a_grounding_to_marked_concept_crossing_is_tier_legal_but_breaks_the_grounding_doctrine() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_doctrine_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            GROUNDING_LOGIC,
            DOMAIN_COGNITION,
            EdgeKind::Ontology,
            &[MARKED_CONCEPT],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(
            GROUNDING_LOGIC,
            DOMAIN_COGNITION,
            EdgeKind::Ontology,
        )],
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_FORBIDDEN_DEPENDENCY),
        "both slices are tierCore, so the tier gate is blind to this: {findings:?}"
    );
    let doctrine: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY)
        .collect();
    assert_eq!(doctrine.len(), 1, "{findings:?}");
    assert_eq!(doctrine[0].severity, Severity::Error);
    assert!(doctrine[0].message.contains(GROUNDING_LOGIC));
    assert!(doctrine[0].message.contains(DOMAIN_COGNITION));
    // The message must NAME the offending concept, the grounding domain its
    // subject matter falls in, and the grounding slice that must own it —
    // a from/to pair alone is not an actionable finding.
    assert!(
        doctrine[0].message.contains(MARKED_CONCEPT),
        "{}",
        doctrine[0].message
    );
    assert!(
        doctrine[0].message.contains(DOMAIN_LOGICAL),
        "{}",
        doctrine[0].message
    );
    assert!(
        doctrine[0].message.contains("logical grounding domain"),
        "the finding must name the domain by its authored rdfs:label: {}",
        doctrine[0].message
    );
}

/// The mutation twin of the test above, and the whole point of this
/// correction: the SAME grounding slice, the SAME non-grounding slice, the
/// SAME crossing direction, differing ONLY in that the referenced term
/// carries no `gmeow:groundingConceptDomain` marker — ordinary domain
/// vocabulary a grounding slice may consume by reference. It must NOT fire.
#[test]
fn a_grounding_slice_consuming_ordinary_domain_vocabulary_never_fires_the_doctrine_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_doctrine_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            GROUNDING_LOGIC,
            DOMAIN_COGNITION,
            EdgeKind::Ontology,
            &[ORDINARY_TERM],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(
            GROUNDING_LOGIC,
            DOMAIN_COGNITION,
            EdgeKind::Ontology,
        )],
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
        "docs/GROUNDING.md forbids the downward dependency only FOR A GROUNDING CONCEPT; \
             gmeow:beliefState carries no gmeow:groundingConceptDomain marker: {findings:?}"
    );
}

/// The discrimination is decided by the MARKER, not by the term IRI: one
/// edge naming both terms yields exactly ONE finding, and it names the
/// marked term. Guards against a gate that fires per EDGE (which would
/// report a single generic violation) or per TERM without filtering (which
/// would report two).
#[test]
fn one_edge_naming_both_a_marked_and_an_unmarked_term_fires_once_on_the_marked_one() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_doctrine_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            GROUNDING_LOGIC,
            DOMAIN_COGNITION,
            EdgeKind::Ontology,
            &[ORDINARY_TERM, MARKED_CONCEPT],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(
            GROUNDING_LOGIC,
            DOMAIN_COGNITION,
            EdgeKind::Ontology,
        )],
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    let doctrine: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY)
        .collect();
    assert_eq!(doctrine.len(), 1, "{findings:?}");
    assert!(
        doctrine[0].message.contains(MARKED_CONCEPT),
        "{}",
        doctrine[0].message
    );
    assert!(
        !doctrine[0].message.contains(ORDINARY_TERM),
        "{}",
        doctrine[0].message
    );
}

/// A bare `gmeow:sliceDependsOn` from a grounding manifest onto a domain
/// slice is NOT a breach under the qualified rule — `lang:` legitimately
/// declares `versions`, `citations` and `documents` — so a declaration with
/// no grounding concept crossing it must stay silent. (The unqualified
/// reading fired here, which is exactly the over-fire being corrected.)
#[test]
fn a_declared_grounding_to_domain_dependency_alone_is_not_a_doctrine_breach() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_manifest(
        root,
        "grounding",
        "logic",
        r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" ;
                gmeow:sliceTier gmeow:tierCore ;
                gmeow:sliceDependsOn <https://blackcatinformatics.ca/gmeow/slices/cognition> .

            <https://blackcatinformatics.ca/gmeow/groundingDomainLogical>
                a gmeow:GroundingDomain ;
                rdfs:label "logical grounding domain" ;
                gmeow:groundingDomainOwner <https://blackcatinformatics.ca/gmeow/slices/logic> .
            "#,
    );
    write_manifest(
        root,
        "core",
        "cognition",
        r#"<https://blackcatinformatics.ca/gmeow/slices/cognition>
                a gmeow:Slice ;
                rdfs:label "cognition" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
    );
    let catalog = SliceCatalog::discover(&root.join("slices"), vocab()).unwrap();
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: Vec::new(),
        diagnostics: Vec::new(),
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
        "a declaration is not itself a grounding-concept crossing: {findings:?}"
    );
}

/// The Principle 19 peerage grant: a grounding -> grounding crossing is
/// legitimate and must NEVER fire R6 (it is governed by the seam registry
/// instead).
#[test]
fn a_grounding_to_grounding_peer_crossing_never_fires_the_doctrine_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_doctrine_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            GROUNDING_LOGIC,
            GROUNDING_MATH,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/math/Quantity"],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(
            GROUNDING_LOGIC,
            GROUNDING_MATH,
            EdgeKind::Ontology,
        )],
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
        "grounding -> grounding is the peerage grant, never a doctrine breach: {findings:?}"
    );
}

/// The sanctioned direction: a domain slice CONSUMING a grounding term is
/// exactly what the doctrine prescribes and must never fire R6 — even when
/// the consumed term is a marked grounding concept, which is the normal,
/// correct state of the world after a promotion.
#[test]
fn a_domain_to_grounding_crossing_never_fires_the_doctrine_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog = grounding_doctrine_catalog(tmp.path());
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            DOMAIN_COGNITION,
            GROUNDING_MATH,
            EdgeKind::Ontology,
            &["https://blackcatinformatics.ca/math/Quantity"],
            ReconciliationStatus::Matched,
        )],
        diagnostics: Vec::new(),
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
        "non-grounding -> grounding is the sanctioned consumption direction: {findings:?}"
    );
}

/// A marker naming a `gmeow:GroundingDomain` no grounding manifest declares
/// resolves to nothing: an undeclared domain has no
/// `gmeow:groundingDomainOwner`, so there is no reconciliation direction the
/// finding could state, and inventing one would be the gate carrying its own
/// opinion. The dangling domain IRI is caught by `authoring.undeclared-term`
/// instead. Guards against a future `domain_of` that returns a synthetic
/// record rather than `None`.
#[test]
fn a_marker_naming_an_undeclared_grounding_domain_does_not_fire() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write_manifest(
        root,
        "grounding",
        "logic",
        r#"<https://blackcatinformatics.ca/gmeow/slices/logic>
                a gmeow:Slice, gmeow:GroundingSlice ;
                rdfs:label "logic" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
    );
    write_manifest(
        root,
        "core",
        "cognition",
        r#"<https://blackcatinformatics.ca/gmeow/slices/cognition>
                a gmeow:Slice ;
                rdfs:label "cognition" ;
                gmeow:sliceTier gmeow:tierCore .
            "#,
    );
    write_module(
        root,
        "core",
        "cognition",
        r#"gmeow:kbPartitionRole
                a owl:ObjectProperty ;
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/cognition> ;
                rdfs:label "kb partition role" ;
                gmeow:groundingConceptDomain <https://blackcatinformatics.ca/gmeow/groundingDomainNeverDeclared> .
            "#,
    );
    let catalog = SliceCatalog::discover(&root.join("slices"), vocab()).unwrap();
    let report = OwnershipReport {
        ownership: std::collections::HashMap::new(),
        edges: vec![edge(
            GROUNDING_LOGIC,
            DOMAIN_COGNITION,
            EdgeKind::Ontology,
            &[MARKED_CONCEPT],
            ReconciliationStatus::Undeclared,
        )],
        diagnostics: vec![undeclared_diag(
            GROUNDING_LOGIC,
            DOMAIN_COGNITION,
            EdgeKind::Ontology,
        )],
    };

    let findings = peerage_aware_ownership_findings(&report, &catalog).unwrap();
    assert!(
        !findings
            .iter()
            .any(|f| f.code == crate::codes::SLICE_OWNERSHIP_GROUNDING_DOWNWARD_DEPENDENCY),
        "an unowned domain names no reconciliation direction: {findings:?}"
    );
}

/// The real corpus's own promotion, asserted as data rather than as prose:
/// the graph-box-role cluster IS marked as a logical grounding concept, the
/// logical domain IS owned by `logic:`, and every marked term IS owned by
/// the grounding slice its domain names. This is what makes the live gate's
/// zero honest — a zero produced by an EMPTY marker set would be vacuous.
#[test]
fn the_real_corpus_marks_the_box_role_cluster_and_owns_every_marked_concept() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices");
    let catalog = SliceCatalog::discover(&dir, vocab()).unwrap();
    let corpus = CorpusIndex::build(&catalog).unwrap();
    let concepts = &corpus.grounding_concepts;
    let report = purrdf::slice::OwnershipAnalyzer::new(&catalog)
        .analyze()
        .unwrap();

    // All three grounding domains are declared, each owned by its slice.
    let owners: BTreeMap<&str, &str> = concepts
        .domains
        .values()
        .map(|d| (d.iri.as_str(), d.owner.as_str()))
        .collect();
    assert_eq!(
        owners.get("https://blackcatinformatics.ca/gmeow/groundingDomainLogical"),
        Some(&"https://blackcatinformatics.ca/gmeow/slices/logic"),
        "{owners:?}"
    );
    assert_eq!(
        owners.get("https://blackcatinformatics.ca/gmeow/groundingDomainLinguistic"),
        Some(&"https://blackcatinformatics.ca/gmeow/slices/lang"),
        "{owners:?}"
    );
    assert_eq!(
        owners.get("https://blackcatinformatics.ca/gmeow/groundingDomainMathematical"),
        Some(&"https://blackcatinformatics.ca/gmeow/slices/math"),
        "{owners:?}"
    );

    // The marker set is NON-EMPTY and contains the whole box-role cluster:
    // the value type, its five role individuals, and the property.
    for local in [
        "GraphBoxRole",
        "boxABox",
        "boxCBox",
        "boxConfigBox",
        "boxRBox",
        "boxTBox",
        "graphBoxRole",
    ] {
        let iri = format!("https://blackcatinformatics.ca/gmeow/{local}");
        let domain = concepts
            .domain_of(&iri)
            .unwrap_or_else(|| panic!("gmeow:{local} must carry gmeow:groundingConceptDomain"));
        assert_eq!(
            domain.iri,
            "https://blackcatinformatics.ca/gmeow/groundingDomainLogical"
        );
    }

    // And the standing invariant the gate's zero rests on: EVERY marked
    // term is owned by the grounding slice its domain names. A marked term
    // owned elsewhere is precisely the R6 violation.
    let mut misowned: Vec<(String, String, String)> = Vec::new();
    for (term, domain_iri) in &concepts.term_domain {
        let Some(domain) = concepts.domains.get(domain_iri) else {
            continue;
        };
        let Some(owned) = NamedNode::new(term)
            .ok()
            .and_then(|n| report.ownership.get(&n))
        else {
            continue;
        };
        if owned.declared_owner != domain.owner {
            misowned.push((
                term.clone(),
                owned.declared_owner.clone(),
                domain.owner.clone(),
            ));
        }
    }
    assert!(
        misowned.is_empty(),
        "every gmeow:groundingConceptDomain-marked term must be owned by its domain's \
             gmeow:groundingDomainOwner; these are not: {misowned:?}"
    );
}
