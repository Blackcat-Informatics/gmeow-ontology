// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently authored tiny user mappings and data; no repository ontology,
//! generated fixture, GTS author, corpus loader or producer is invoked.

use super::*;
use gmeow_logic_compile::{
    ingest::DslView,
    projections::{
        correspondence::{parse_correspondence, project_correspondence_dataset},
        correspondence_frontend::transpile_correspondences_indexed,
        profile_program::ProjectionProfileProgram,
        sparql::lower_sparql,
    },
};

const FIXTURE: &str = r#"
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
lang:literalValue a logic:DatatypeProperty ; logic:range rdfs:Literal ;
    rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/lang> .
<urn:fixture:value> a logic:DatatypeProperty ; logic:subPropertyOf lang:literalValue .
lang:literalValue skos:relatedMatch <https://schema.org/value> .
lang:fixtureBridge rdf:reifies <<(lang:literalValue skos:relatedMatch <https://schema.org/value>)>> ;
    a logic:GroundingCorrespondence ; gm:sssomFile "fixture.tsv" ;
    gm:justification <urn:fixture:justification> ;
    logic:sourceEndpoint lang:literalValue ; logic:targetEndpoint <https://schema.org/value> ;
    logic:morphismClass logic:BridgeView ; logic:morphismKind logic:CommitmentShiftingBridge ;
    logic:preservationKind logic:ValidationOnly .
<urn:fixture:mapping> a gm:ProjectionMapping ; rdfs:label "literal fixture" ;
    gm:hasMappingPattern [ gm:anchor "s" ; gm:atom (
        [ gm:subjectVar "s" ; gm:predicate <urn:fixture:value> ; gm:objectVar "v" ] ) ] ;
    gm:hasBinding [ gm:profile "schema-org" ; gm:relation "<=" ;
        gm:templateAtoms ( [ gm:tSubj "s" ; gm:tPredFromCorrespondence lang:fixtureBridge ; gm:tObj "v" ] ) ] .
"#;

fn dataset(source: &str) -> Arc<RdfDataset> {
    purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap()
}

fn compile(source: &str) -> gmeow_errors::Result<(GroundingCatalog, ProjectionProfileProgram)> {
    let owned = vec![owned_source(source)?];
    let source = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None)?;
    let view = DslView::new(&source);
    let (program, analysis) = transpile_correspondences_indexed(&view, &view, &owned)?;
    let lowered = lower_sparql(&view, &analysis, &["schema-org"])?;
    let profiles = lowered.profiles.values().cloned().collect::<Vec<_>>();
    let program = program
        .with_projection_bindings(lowered.bindings)?
        .with_projection_profiles(profiles.clone())?;
    let catalog = GroundingCatalog::from_program(program, &view, &owned)?;
    Ok((catalog, profiles[0].clone()))
}

fn owned_source(
    source: &str,
) -> gmeow_errors::Result<gmeow_logic_compile::projections::grounding_catalog::OwnedGroundingSource>
{
    gmeow_logic_compile::projections::grounding_catalog::OwnedGroundingSource::new(
        "https://blackcatinformatics.ca/gmeow/slices/lang",
        "slices/grounding/lang/mappings/independently-authored-fixture.ttl",
        source.as_bytes(),
    )
}

/// Existing direct native-query controls have no selected correspondence. This
/// private test adapter exercises that executor/transform behavior only; selected
/// product admission is exercised through the real public prepare path above.
pub(crate) fn prepare_queries(queries: &[(String, String)]) -> Vec<PreparedProjectionProfile> {
    let engine = NativeSparqlEngine::new();
    queries
        .iter()
        .map(|(name, query)| PreparedProjectionProfile {
            program: ProjectionProfileProgram::new(
                name.clone(),
                query.clone(),
                Vec::new(),
                "urn:fixture:unselected-query-control".to_owned(),
            ),
            forward: engine.prepare_query(query, None).unwrap(),
            checks: Vec::new(),
        })
        .collect()
}

#[test]
fn projection_literal_preserves_complete_native_literals() {
    let (catalog, profile) = compile(FIXTURE).unwrap();
    let prepared = PreparedProjectionProfile::prepare(&profile, &catalog).unwrap();
    for value in [
        r#""text""#,
        r#""0042"^^<http://www.w3.org/2001/XMLSchema#integer>"#,
        r#""العربية"@ar--rtl"#,
        r#""中文 🐈"@zh"#,
    ] {
        let source = dataset(&format!(
            "<urn:fixture:subject> <urn:fixture:value> {value} ."
        ));
        let expected = source.owned_quads().next().unwrap().object;
        let SparqlResult::Graph(output) = prepared.execute(&source).unwrap() else {
            panic!("graph");
        };
        let rows: Vec<_> = output.owned_quads().collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].predicate, "https://schema.org/value");
        assert_eq!(rows[0].object, expected);
    }
}

#[test]
fn projection_literal_same_profile_siblings_own_only_their_selected_occurrences() {
    let source = FIXTURE.replace("gm:objectVar \"v\" ] )", "gm:objectVar \"v\" ] [ gm:subjectVar \"s\" ; gm:predicate <urn:fixture:second> ; gm:objectVar \"w\" ] )");
    let source = format!(
        r#"{source}
<urn:fixture:second> a logic:DatatypeProperty ; logic:subPropertyOf lang:literalValue .
lang:literalValue skos:relatedMatch <https://schema.org/valueReference> .
lang:secondBridge rdf:reifies <<(lang:literalValue skos:relatedMatch <https://schema.org/valueReference>)>> ;
    a logic:GroundingCorrespondence ; gm:sssomFile "fixture.tsv" ;
    gm:justification <urn:fixture:justification> ;
    logic:sourceEndpoint lang:literalValue ; logic:targetEndpoint <https://schema.org/valueReference> ;
    logic:morphismClass logic:BridgeView ; logic:morphismKind logic:CommitmentShiftingBridge ;
    logic:preservationKind logic:ValidationOnly .
<urn:fixture:mapping> gm:hasBinding <urn:fixture:second-binding>, <urn:fixture:raw-binding> .
<urn:fixture:second-binding> gm:profile "schema-org" ; gm:relation "<=" ;
    gm:templateAtoms ( [ gm:tSubj "s" ; gm:tPredFromCorrespondence lang:secondBridge ; gm:tObj "w" ] ) .
<urn:fixture:raw-binding> gm:profile "schema-org" ; gm:relation "<=" ;
    gm:templateAtoms ( [ gm:tSubj "s" ; gm:tPred <urn:fixture:unselected> ; gm:tObj "v" ] ) .
"#
    );
    let (catalog, profile) = compile(&source).unwrap();
    assert_eq!(catalog.program().projection_bindings.len(), 3);
    assert_eq!(
        catalog
            .program()
            .projection_bindings
            .iter()
            .filter(|binding| binding.selected.is_empty())
            .count(),
        1
    );
    assert_eq!(profile.payload.checks.len(), 2);
    let selected: std::collections::BTreeSet<_> = profile
        .payload
        .checks
        .iter()
        .map(|check| {
            (
                check.selected.support.target_property.as_str(),
                check.selected.object_variable.as_str(),
            )
        })
        .collect();
    assert_eq!(
        selected,
        std::collections::BTreeSet::from([
            ("https://schema.org/value", "v"),
            ("https://schema.org/valueReference", "w")
        ])
    );
    let prepared = PreparedProjectionProfile::prepare(&profile, &catalog).unwrap();
    let input =
        dataset("<urn:fixture:s> <urn:fixture:value> \"one\" ; <urn:fixture:second> \"two\" .");
    let SparqlResult::Graph(output) = prepared.execute(&input).unwrap() else {
        panic!("graph");
    };
    let outputs: std::collections::BTreeSet<_> = output
        .owned_quads()
        .map(|quad| (quad.predicate, quad.object.to_string()))
        .collect();
    assert_eq!(
        outputs,
        std::collections::BTreeSet::from([
            ("https://schema.org/value".to_owned(), "\"one\"".to_owned()),
            (
                "https://schema.org/valueReference".to_owned(),
                "\"two\"".to_owned()
            ),
            ("urn:fixture:unselected".to_owned(), "\"one\"".to_owned()),
        ])
    );
    let invalid = dataset(
        "<urn:fixture:s> <urn:fixture:value> \"one\" ; <urn:fixture:second> <urn:fixture:bad> .",
    );
    let failure = prepared.execute(&invalid).unwrap_err().to_string();
    assert!(failure.contains("urn:fixture:second"), "{failure}");
    assert!(failure.contains("urn:fixture:bad"), "{failure}");
}

#[test]
fn projection_literal_required_archive_selection_refuses_duplicate_and_missing_products() {
    let (catalog, profile) = compile(FIXTURE).unwrap();
    let path = "schema-org.projection.json".to_owned();
    let member = (path, profile.to_bytes());
    let valid = purrdf::ustar::write_archive(std::slice::from_ref(&member)).unwrap();
    assert_eq!(
        programs_from_required_archive(&valid, &["schema-org"], &catalog).unwrap(),
        vec![profile]
    );
    let duplicate = purrdf::ustar::write_archive(&[member.clone(), member]).unwrap();
    let failure = programs_from_required_archive(&duplicate, &["schema-org"], &catalog)
        .unwrap_err()
        .to_string();
    assert!(failure.contains("repeats required member"), "{failure}");
    assert!(programs_from_required_archive(&valid, &["dcat"], &catalog).is_err());
    let source_path = "slices/grounding/lang/mappings/authored-fixture.ttl".to_owned();
    let source = (
        source_path.clone(),
        b"@prefix ex: <urn:fixture:> .".to_vec(),
    );
    let duplicate = purrdf::ustar::write_archive(&[source.clone(), source]).unwrap();
    assert!(
        gmeow_gts_profile::archive::archive_member(
            &duplicate,
            &source_path,
            gmeow_gts_profile::archive::MAX_NATIVE_MEMBER_BYTES
        )
        .is_err()
    );
}

#[test]
fn projection_literal_refuses_nonliteral_matches_before_construct() {
    let (catalog, profile) = compile(FIXTURE).unwrap();
    let prepared = PreparedProjectionProfile::prepare(&profile, &catalog).unwrap();
    for value in [
        "<urn:fixture:iri>",
        "_:object",
        "<<( <urn:a> <urn:p> <urn:b> )>>",
    ] {
        let source = dataset(&format!(
            "<urn:fixture:good> <urn:fixture:value> \"good\" . <urn:fixture:bad> <urn:fixture:value> {value} ."
        ));
        let error = prepared.execute(&source).unwrap_err().to_string();
        assert!(error.contains("complete RDF literal"), "{error}");
        assert!(error.contains("urn:fixture:value"), "{error}");
        assert!(error.contains("urn:fixture:mapping"), "{error}");
        assert!(error.contains("urn:fixture:bad"), "{error}");
        if value.starts_with('<') && !value.starts_with("<<") {
            assert!(error.contains("urn:fixture:iri"), "{error}");
        }
    }
}

#[test]
fn projection_literal_batch_admits_every_profile_before_construction_capabilities() {
    let (catalog, profile) = compile(FIXTURE).unwrap();
    let prepared = PreparedProjectionProfile::prepare(&profile, &catalog).unwrap();
    let first = prepare_queries(&[(
        "authored-unselected".to_owned(),
        "CONSTRUCT { ?s <urn:fixture:output> ?v } WHERE { ?s <urn:fixture:value> ?v }".to_owned(),
    )])
    .remove(0);
    let programs = [first, prepared];
    let source = dataset("<urn:fixture:subject> <urn:fixture:value> <urn:fixture:bad-value> .");
    // No construction capability is returned, including for the earlier
    // profile which itself has no selected input requirements.
    let failure = admit_all(&programs, &source).unwrap_err().to_string();
    assert!(failure.contains("urn:fixture:mapping"), "{failure}");
    assert!(failure.contains("urn:fixture:bad-value"), "{failure}");
}

#[test]
fn projection_literal_refuses_off_owner_catalog_even_with_valid_semantic_envelope() {
    use gmeow_logic_compile::projections::grounding_catalog::OwnedGroundingSource;
    let owner = "https://blackcatinformatics.ca/gmeow/slices/lang";
    assert!(
        OwnedGroundingSource::new(
            owner,
            "slices/core/identifiers/mappings/bridge.ttl",
            FIXTURE.as_bytes()
        )
        .is_err()
    );
    assert!(
        OwnedGroundingSource::new(
            "https://blackcatinformatics.ca/gmeow/slices/identifiers",
            "slices/grounding/lang/mappings/bridge.ttl",
            FIXTURE.as_bytes()
        )
        .is_err()
    );
    let source = dataset(FIXTURE);
    let view = DslView::new(&source);
    // All isDefinedBy, native types, endpoints and qualifiers remain present.
    // No explicit actual owned artifact means no selected authority.
    assert!(transpile_correspondences_indexed(&view, &view, &[]).is_err());
}

#[test]
fn projection_literal_retained_get_leg_refuses_joint_catalog_product_check_tampering() {
    use gmeow_logic_compile::projections::profile_program::ProjectionInputCheck;
    let (catalog, profile) = compile(FIXTURE).unwrap();
    let missing = ProjectionProfileProgram::new(
        profile.payload.profile.clone(),
        profile.payload.forward.clone(),
        Vec::new(),
        profile.payload.catalog_id.clone(),
    );
    let original = &profile.payload.checks[0];
    let narrowed = ProjectionInputCheck::new(
        original.binding_iri.clone(),
        original.binding_key.clone(),
        original.binding_program.clone(),
        original.source_mapping.clone(),
        original.query.replace("WHERE {", "WHERE { FILTER(false)"),
        original.selected.clone(),
    );
    let narrowed = ProjectionProfileProgram::new(
        profile.payload.profile.clone(),
        profile.payload.forward.clone(),
        vec![narrowed],
        profile.payload.catalog_id.clone(),
    );
    for product in [missing, narrowed] {
        // Both product and catalog list change together, with their local
        // content identities recomputed. The independent get-leg body remains.
        assert!(
            catalog
                .program()
                .clone()
                .with_projection_profiles(vec![product])
                .is_err()
        );
    }
    let altered = FIXTURE.replace("gm:atom (", "gm:atom ( [ gm:subjectVar \"s\" ; gm:predicate <urn:fixture:context> ; gm:objectVar \"ctx\" ]");
    let (other, _) = compile(&altered).unwrap();
    let before = &catalog.program().projection_bindings[0];
    let after = &other.program().projection_bindings[0];
    assert_ne!(before.source_pattern, after.source_pattern);
    assert_ne!(before.id, after.id);
    assert_ne!(before.correspondence, after.correspondence);
    assert_eq!(
        other
            .program()
            .correspondences
            .iter()
            .find(|cell| cell.iri == after.correspondence)
            .unwrap()
            .get_leg
            .as_deref(),
        Some(after.id.as_str())
    );
}

#[test]
fn projection_literal_requires_strict_mandatory_source_witness() {
    for replacement in [
        "gm:predicate <urn:fixture:value>, <urn:other>",
        "gm:predicate <urn:fixture:value> ; gm:optional \"garbage\"",
        "gm:predicate <urn:fixture:value> ; gm:optional true",
        "gm:predicate <urn:fixture:value> ; gm:tPred <urn:other>",
        "gm:predicate <urn:fixture:value> ; gm:predicateVar \"p\"",
        "gm:predicate <urn:fixture:value> ; gm:tObj \"other\"",
    ] {
        assert!(
            compile(&FIXTURE.replace("gm:predicate <urn:fixture:value>", replacement)).is_err(),
            "{replacement}"
        );
    }
}

#[test]
fn projection_literal_refuses_bad_selector_fields_membership_and_flow() {
    for replacement in [
        "gm:tPredFromCorrespondence <urn:missing>",
        "gm:tPredFromCorrespondence \"literal\"",
        "gm:tPredFromCorrespondence _:anonymous",
        "gm:tPredFromCorrespondence lang:fixtureBridge, <urn:other>",
        "gm:tPredFromCorrespondence lang:fixtureBridge ; gm:tPred <https://schema.org/value>",
        "gm:tPredFromCorrespondence lang:fixtureBridge ; gm:objectValue <urn:other>",
    ] {
        assert!(
            compile(&FIXTURE.replace("gm:tPredFromCorrespondence lang:fixtureBridge", replacement))
                .is_err(),
            "{replacement}"
        );
    }
    assert!(compile(&FIXTURE.replace("gm:tObj \"v\"", "gm:tObj \"missing\"")).is_err());
    assert!(
        compile(&format!(
            "{FIXTURE}\n<urn:dangling> gm:tPredFromCorrespondence lang:fixtureBridge ."
        ))
        .is_err()
    );
}

#[test]
fn projection_literal_refuses_missing_wrong_or_forged_owner_premises() {
    for (from, to) in [
        (
            "logic:subPropertyOf lang:literalValue",
            "logic:subPropertyOf <urn:wrong>",
        ),
        (
            "logic:range rdfs:Literal",
            "logic:range <http://www.w3.org/2001/XMLSchema#string>",
        ),
        (
            "https://blackcatinformatics.ca/gmeow/slices/lang",
            "https://blackcatinformatics.ca/gmeow/slices/finance",
        ),
        (
            "logic:DatatypeProperty ; logic:subPropertyOf",
            "logic:ObjectProperty ; logic:subPropertyOf",
        ),
        (
            "logic:preservationKind logic:ValidationOnly",
            "logic:preservationKind logic:ExactPreservation",
        ),
    ] {
        assert!(
            compile(&FIXTURE.replace(from, to)).is_err(),
            "{from} -> {to}"
        );
    }
}

#[test]
fn projection_literal_catalog_native_rdf_roundtrip_keeps_exact_support() {
    let (catalog, profile) = compile(FIXTURE).unwrap();
    let projected = project_correspondence_dataset(catalog.program()).unwrap();
    let restored = parse_correspondence(&projected).unwrap();
    assert_eq!(restored, *catalog.program());
    let ontology = dataset(FIXTURE);
    let owned = vec![owned_source(FIXTURE).unwrap()];
    let restored =
        GroundingCatalog::from_program(restored, &DslView::new(&ontology), &owned).unwrap();
    assert_eq!(restored.identity(), catalog.identity());
    assert_eq!(
        ProjectionProfileProgram::from_bytes(&profile.to_bytes(), &restored).unwrap(),
        profile
    );
    assert!(
        GroundingCatalog::from_program(catalog.program().clone(), &DslView::new(&ontology), &[])
            .is_err()
    );
    let altered_original = vec![
        owned_source(&format!(
            "{FIXTURE}\n# distinct original artifact identity\n"
        ))
        .unwrap(),
    ];
    assert!(
        GroundingCatalog::from_program(
            catalog.program().clone(),
            &DslView::new(&ontology),
            &altered_original
        )
        .is_err()
    );
    let missing = dataset("");
    assert!(
        GroundingCatalog::from_program(catalog.program().clone(), &DslView::new(&missing), &owned)
            .is_err()
    );
}

#[test]
fn projection_literal_catalog_refuses_missing_extra_duplicate_and_swapped_checks() {
    let (catalog, profile) = compile(FIXTURE).unwrap();
    let mut variants = Vec::new();
    let mut absent = profile.clone();
    absent.payload.checks.clear();
    variants.push(absent);
    let mut duplicate = profile.clone();
    duplicate
        .payload
        .checks
        .push(duplicate.payload.checks[0].clone());
    variants.push(duplicate);
    let mut changed = profile.clone();
    changed.payload.checks[0].query = "SELECT ?v WHERE { ?s <urn:wrong> ?v }".to_owned();
    variants.push(changed);
    let mut forward = profile.clone();
    forward.payload.forward =
        "CONSTRUCT { ?s <urn:wrong> ?v } WHERE { ?s <urn:fixture:value> ?v }".to_owned();
    variants.push(forward);
    for changed in variants {
        // Recompute the product identity: catalog validation, not merely the
        // artifact's own checksum, must reject capability substitution.
        let changed = ProjectionProfileProgram::new(
            changed.payload.profile,
            changed.payload.forward,
            changed.payload.checks,
            changed.payload.catalog_id,
        );
        assert!(PreparedProjectionProfile::prepare(&changed, &catalog).is_err());
    }
}

#[test]
fn projection_literal_native_record_refuses_duplicate_and_unknown_fields() {
    let (catalog, profile) = compile(FIXTURE).unwrap();
    let json = String::from_utf8(profile.to_bytes()).unwrap();
    assert!(
        ProjectionProfileProgram::from_bytes(
            json.replacen('{', "{\"extra\":true,", 1).as_bytes(),
            &catalog
        )
        .is_err()
    );
    assert!(
        ProjectionProfileProgram::from_bytes(
            json.replacen("\"id\":", "\"id\":\"duplicate\",\"id\":", 1)
                .as_bytes(),
            &catalog
        )
        .is_err()
    );
}
