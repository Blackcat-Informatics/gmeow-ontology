// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent registry literals; no canonical registry or corpus is consumed.
use super::*;

const REGISTRY: &str = r#"
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
<urn:authored:procedure> a gm:CatalogFamily ; gm:catalogFamilyName "procedure" ;
    gm:catalogNamespace "https://schema.org/HowTo"^^xsd:anyURI,
        "https://schema.org/Recipe"^^xsd:anyURI, "https://schema.org/supply"^^xsd:anyURI,
        "https://schema.org/tool"^^xsd:anyURI, "https://schema.org/result"^^xsd:anyURI,
        "https://schema.org/step"^^xsd:anyURI ;
    gm:catalogOwner <https://blackcatinformatics.ca/gmeow/slices/logic> ; gm:catalogTargetMinimum 7 .
<urn:authored:value> a gm:CatalogFamily ; gm:catalogFamilyName "value" ;
    gm:catalogNamespace "https://schema.org/value"^^xsd:anyURI ;
    gm:catalogOwner <https://blackcatinformatics.ca/gmeow/slices/lang>,
        <https://blackcatinformatics.ca/gmeow/slices/math> ; gm:catalogTargetMinimum 2 .
<urn:authored:exemption> a gm:ResidueRatchetExemption ;
    gm:exemptCatalogFamily <urn:authored:procedure> ; gm:exemptRationale
    "The authored fixture retains seven procedure publication bridges under their existing explicitly bounded exemption." ;
    gm:exemptRowCeiling 7 .
"#;

fn rows() -> Vec<(&'static str, &'static str)> {
    vec![
        ("urn:row:plan", "https://schema.org/HowTo"),
        ("urn:row:step", "https://schema.org/HowToStep"),
        ("urn:row:recipe", "https://schema.org/Recipe"),
        ("urn:row:supply", "https://schema.org/supply"),
        ("urn:row:tool", "https://schema.org/tool"),
        ("urn:row:result", "https://schema.org/result"),
        ("urn:row:ordered-step", "https://schema.org/step"),
        ("urn:row:math", "https://schema.org/value"),
        ("urn:row:lang", "https://schema.org/value"),
    ]
}

#[test]
fn projection_literal_catalog_partitions_seven_procedure_and_two_value_bridges() {
    let dataset = purrdf::parse_dataset(REGISTRY.as_bytes(), "text/turtle", None).unwrap();
    let families = catalog_families_from_dataset(&dataset).unwrap();
    let measured = check_target_catalogs(&families, rows(), "authored").unwrap();
    assert_eq!(measured["procedure"], 7);
    assert_eq!(measured["value"], 2);
    let exemptions = residue_exemptions_from_dataset(&dataset).unwrap();
    assert_eq!(exemptions[0].row_ceiling, 7);
    let guarded = BTreeSet::from(["https://schema.org/value".to_owned()]);
    check_residue_exemptions(&families, &exemptions, &guarded, &measured, "authored").unwrap();
    assert_eq!(
        families_for_target(&families, "https://schema.org/valueReference")[0].name,
        "value"
    );
    assert_eq!(
        families_for_target(&families, "https://schema.org/HowToSupply")[0].name,
        "procedure"
    );

    // A sixth fully admitted row is legal; source scope and complete native proof replace a cap.
    let native = format!("{REGISTRY}\n<urn:authored:prov> a gm:CatalogFamily, gm:NativeKernelCatalogGuard ;
        gm:catalogFamilyName \"prov\" ; gm:catalogNamespace \"http://www.w3.org/ns/prov#\"^^xsd:anyURI ;
        gm:catalogOwner <https://blackcatinformatics.ca/gmeow/slices/logic> ; gm:catalogTargetMinimum 5 .");
    let dataset = purrdf::parse_dataset(native.as_bytes(), "text/turtle", None).unwrap();
    let registry = CatalogRegistry {
        families: catalog_families_from_dataset(&dataset).unwrap(),
        exemptions: residue_exemptions_from_dataset(&dataset).unwrap(),
        native_kernel_families: native_kernel::families_from_dataset(&dataset).unwrap(),
    };
    let prefixes = "@prefix g: <https://blackcatinformatics.ca/gmeow/> .
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
        @prefix skos: <http://www.w3.org/2004/02/skos/core#> .
        @prefix prov: <http://www.w3.org/ns/prov#> .";
    let owner = "https://blackcatinformatics.ca/gmeow/slices/logic";
    let module_path = "slices/grounding/logic/module.ttl";
    let mapping_path = "slices/grounding/logic/mappings/native.ttl";
    let module = format!(
        "{prefixes}\ng:Pattern rdfs:isDefinedBy <{owner}> .\n{}",
        (1..=6)
            .map(|index| { format!("g:Usage{index} rdfs:isDefinedBy <{owner}> .") })
            .collect::<String>()
    );
    let module_dataset = purrdf::parse_dataset(module.as_bytes(), "text/turtle", None).unwrap();
    let domain_owner = "https://blackcatinformatics.ca/gmeow/slices/publication";
    let domain_module = format!("{prefixes}g:Publication rdfs:isDefinedBy <{domain_owner}> .");
    let domain_dataset =
        purrdf::parse_dataset(domain_module.as_bytes(), "text/turtle", None).unwrap();
    let ontology =
        purrdf::RdfDataset::union(&[module_dataset.as_ref(), domain_dataset.as_ref()]).unwrap();
    let ontology = gmeow_logic_compile::ingest::DslView::new(&ontology);
    let mut ownership = native_kernel::CanonicalOwnership::default();
    ownership.record_module(owner, module_path, module.as_bytes(), &module_dataset);
    ownership.record_module(
        domain_owner,
        "slices/core/publication/module.ttl",
        domain_module.as_bytes(),
        &domain_dataset,
    );
    let source = format!(
        "{prefixes}\n{}",
        (1..=6)
            .map(|index| format!(
                "g:Usage{index} skos:closeMatch prov:used {{| a logic:GroundingCorrespondence ;
         g:sssomFile \"native.tsv\" ; g:justification <urn:curation> ;
         logic:sourceEndpoint g:Usage{index} ; logic:targetEndpoint prov:used ;
         logic:morphismClass logic:BridgeView ; logic:morphismKind logic:CommitmentShiftingBridge ;
         logic:preservationKind logic:ValidationOnly |}} ."
            ))
            .collect::<String>()
    );
    let admit = |text: &str,
                 physical_owner: &str,
                 path: &str,
                 ownership: &native_kernel::CanonicalOwnership| {
        let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
        let original =
            native_kernel::OriginalMapping::new(physical_owner, path, text.as_bytes(), dataset);
        native_kernel::admit_source(&registry, &ontology, ownership, &original)
    };
    let evidence = admit(&source, owner, mapping_path, &ownership).unwrap();
    assert_eq!(evidence.len(), 6);
    let mut observations = RegistryObservations {
        registry: registry.clone(),
        guarded_namespaces: guarded.clone(),
        native_kernel_evidence: evidence,
    };
    observations.validate().unwrap();
    let source_dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap();
    let admitted = gmeow_logic_compile::projections::grounding_catalog::GroundingCatalog::admit(
        &gmeow_logic_compile::ingest::DslView::new(&source_dataset),
        &ontology,
    )
    .unwrap();
    observations
        .verify_native_kernel_program(ontology.dataset(), admitted.program())
        .unwrap();
    assert!(
        observations
            .verify_native_kernel_program(&domain_dataset, admitted.program())
            .is_err()
    );
    let changed_owner = purrdf::parse_dataset(
        module.replace(owner, domain_owner).as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap();
    assert!(
        observations
            .verify_native_kernel_program(&changed_owner, admitted.program())
            .is_err()
    );
    let mut changed = admitted.program().clone();
    changed.correspondences[0].grounding = false;
    assert!(
        observations
            .verify_native_kernel_program(ontology.dataset(), &changed)
            .is_err()
    );
    for source in [
        "urn:unowned:source",
        "https://blackcatinformatics.ca/gmeow/Publication",
    ] {
        let mut changed = admitted.program().clone();
        let mut extra = changed.correspondences[0].clone();
        extra.iri = "urn:extra:grounding".to_owned();
        extra.source_endpoint = Some(source.to_owned());
        changed.correspondences.push(extra);
        assert!(
            observations
                .verify_native_kernel_program(ontology.dataset(), &changed)
                .is_err()
        );
    }
    let other_source = observations.native_kernel_evidence[1].source.clone();
    let original_source = std::mem::replace(
        &mut observations.native_kernel_evidence[0].source,
        other_source,
    );
    assert!(
        observations
            .verify_native_kernel_program(ontology.dataset(), admitted.program())
            .is_err()
    );
    observations.native_kernel_evidence[0].source = original_source;
    let mut changed = admitted.program().clone();
    changed.correspondences[0]
        .loss_evidence
        .push(purrdf::RdfLiteral::typed(
            "changed source loss",
            "http://www.w3.org/2001/XMLSchema#string",
        ));
    assert!(
        observations
            .verify_native_kernel_program(ontology.dataset(), &changed)
            .is_err()
    );
    // Ordinary graph transformations need no single SSSOM endpoint pair.
    let transform = format!(
        "{prefixes}@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        g:Export a g:ProjectionMapping ; g:hasMappingPattern [ g:anchor \"x\" ;
            g:atom ( [ g:subjectVar \"x\" ; g:predicate rdf:type ; g:objectValue g:Publication ]
                     [ g:subjectVar \"x\" ; g:predicate rdf:type ; g:objectValue g:Other ] ) ] ;
        g:hasBinding [ g:profile \"prov\" ; g:relation \"<=\" ; g:emitSssom false ;
            g:templateAtoms ( [ g:tSubj \"x\" ; g:tPred rdf:type ; g:tObjValue prov:Entity ] ) ] ."
    );
    for (text, expected) in [
        (transform.clone(), true),
        (
            transform.replace(
                "g:anchor \"x\" ;",
                "g:anchor \"x\" ; g:edoalSource g:Usage1 ;",
            ),
            false,
        ),
    ] {
        let dataset = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
        let (_, analysis) = gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences_indexed(
            &gmeow_logic_compile::ingest::DslView::new(&dataset), &ontology, &[],
        ).unwrap();
        if expected {
            let unrelated = format!(
                "{text}\ng:UnrelatedSelector g:tPredFromCorrespondence <urn:unrelated:alias> ."
            );
            let unrelated =
                purrdf::parse_dataset(unrelated.as_bytes(), "text/turtle", None).unwrap();
            let view = gmeow_logic_compile::ingest::DslView::new(&unrelated);
            analysis
                .inspect_projection_subjects(
                    &view,
                    &BTreeSet::from(["https://blackcatinformatics.ca/gmeow/Export".to_owned()]),
                )
                .unwrap();
            assert!(
                gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences_indexed(
                    &view, &ontology, &[],
                ).is_err(),
                "production admission still rejects the unrelated orphan selector"
            );
        }
        let original =
            native_kernel::OriginalMapping::new(owner, mapping_path, text.as_bytes(), dataset);
        let mut proof = observations.native_kernel_evidence.clone();
        assert_eq!(
            native_kernel::check_projection_alternatives(
                &registry,
                &ownership,
                &mut proof,
                &analysis,
                &[original],
            )
            .is_ok(),
            expected
        );
    }
    let projection = format!("{prefixes}
        g:Export a g:ProjectionMapping, logic:GroundingCorrespondence ;
        g:justification <urn:curation> ; logic:sourceEndpoint g:Usage1 ; logic:targetEndpoint prov:Entity ;
        logic:morphismClass logic:BridgeView ; logic:morphismKind logic:CommitmentShiftingBridge ;
        logic:preservationKind logic:ValidationOnly ; g:hasMappingPattern g:Pattern ;
        g:hasBinding [ g:profile \"prov\" ; g:toClass prov:Entity ; g:relation \"<=\" ; g:emitSssom true ;
            g:sssomPredicate skos:closeMatch ; g:sssomFile \"projection.tsv\" ] .");
    let pattern = format!(
        "{prefixes}@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        g:Pattern g:anchor \"x\" ; g:edoalSource g:Usage1 ;
        g:atom ( [ g:subjectVar \"x\" ; g:predicate rdf:type ; g:objectValue g:Usage1 ] ) ."
    );
    let inspect_projection = |metadata: &str,
                              body: &str,
                              body_owner: &str,
                              body_path: &str|
     -> gmeow_errors::Result<Vec<NativeKernelEvidence>> {
        let metadata_dataset =
            purrdf::parse_dataset(metadata.as_bytes(), "text/turtle", None).unwrap();
        let body_dataset = purrdf::parse_dataset(body.as_bytes(), "text/turtle", None).unwrap();
        let merged = purrdf::RdfDataset::union(&[
            source_dataset.as_ref(),
            metadata_dataset.as_ref(),
            body_dataset.as_ref(),
        ])
        .unwrap();
        let (_, analysis) = gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences_indexed(
            &gmeow_logic_compile::ingest::DslView::new(&merged), &ontology, &[],
        )?;
        let originals = [
            native_kernel::OriginalMapping::new(
                owner,
                mapping_path,
                source.as_bytes(),
                source_dataset.clone(),
            ),
            native_kernel::OriginalMapping::new(
                owner,
                "slices/grounding/logic/mappings/projection.ttl",
                metadata.as_bytes(),
                metadata_dataset,
            ),
            native_kernel::OriginalMapping::new(
                body_owner,
                body_path,
                body.as_bytes(),
                body_dataset,
            ),
        ];
        let mut proof = observations.native_kernel_evidence.clone();
        native_kernel::check_projection_alternatives(
            &registry, &ownership, &mut proof, &analysis, &originals,
        )?;
        Ok(proof)
    };
    let pattern_path = "slices/grounding/logic/mappings/pattern.ttl";
    let projection_proof = inspect_projection(&projection, &pattern, owner, pattern_path).unwrap();
    assert!(
        projection_proof
            .iter()
            .any(|proof| proof.target.ends_with("#Entity")),
        "shared native projection binding is admitted with cross-file definitions"
    );
    let multi_target = projection.replace(
        "g:emitSssom true ;",
        "g:emitSssom false ; g:templateAtoms ( [ g:tSubj \"x\" ; g:tPred prov:used ; g:tObj \"y\" ] ) ;",
    );
    let mismatch = inspect_projection(&multi_target, &pattern, owner, pattern_path).unwrap_err();
    assert!(
        mismatch
            .to_string()
            .contains("outside its grounding target endpoint"),
        "a scoped template target must match the admitted correspondence endpoint: {mismatch}"
    );
    for malformed in [
        projection.replace(", logic:GroundingCorrespondence", ""),
        projection.replace("a g:ProjectionMapping, logic:GroundingCorrespondence ;", ""),
        projection.replace(
            "logic:sourceEndpoint g:Usage1",
            "logic:sourceEndpoint g:Publication",
        ),
        projection.replace(
            "g:justification <urn:curation>",
            "g:justification <urn:curation>, <urn:other>",
        ),
        projection.replace(
            "g:justification <urn:curation>",
            "g:justification \"literal\"",
        ),
        projection.replace("logic:preservationKind logic:ValidationOnly ;", ""),
    ] {
        assert!(inspect_projection(&malformed, &pattern, owner, pattern_path).is_err());
    }
    let stripped_copy = projection
        .replace("a g:ProjectionMapping, logic:GroundingCorrespondence ;", "")
        .replace(
            "logic:sourceEndpoint g:Usage1 ; logic:targetEndpoint prov:Entity ;",
            "",
        );
    assert!(
        inspect_projection(&stripped_copy, &pattern, owner, pattern_path).is_err(),
        "a sole structural candidate cannot disappear with its types and endpoints"
    );
    assert!(
        inspect_projection(
            &projection,
            &format!("{pattern}{stripped_copy}"),
            owner,
            pattern_path
        )
        .is_err(),
        "dedup cannot supply markers or endpoints to an incomplete occurrence"
    );
    assert!(
        inspect_projection(
            &projection,
            &pattern,
            domain_owner,
            "slices/core/publication/mappings/pattern.ttl"
        )
        .is_err(),
        "defining pattern bytes require the actual catalog home"
    );
    observations.native_kernel_evidence.clear();
    assert!(
        observations.validate().is_err(),
        "empty proof is not admission"
    );
    observations.registry.native_kernel_families.clear();
    assert!(
        observations.validate().is_err(),
        "removing governance is not graduation"
    );

    for malformed in [
        source.replace("a logic:GroundingCorrespondence ;", ""),
        source.replace("g:sssomFile \"native.tsv\" ;", ""),
        source.replace("g:justification <urn:curation> ;", ""),
        source.replace(
            "logic:targetEndpoint prov:used",
            "logic:targetEndpoint prov:Plan",
        ),
        source.replace(
            "logic:sourceEndpoint g:Usage1",
            "logic:sourceEndpoint g:Usage2",
        ),
        source.replace("logic:BridgeView", "logic:AffineCorrespondence"),
        source.replace("skos:closeMatch", "<urn:spoof:closeMatch>"),
        source.replace("g:Usage1", "<urn:unowned:source>"),
        format!(
            "{prefixes} [] skos:closeMatch prov:used {{| a logic:GroundingCorrespondence |}} ."
        ),
        format!("{prefixes}g:Usage1 skos:closeMatch prov:used ."),
    ] {
        assert!(admit(&malformed, owner, mapping_path, &ownership).is_err());
    }
    // Even an identical row, with authored ownership claims, fails at its actual off-owner home.
    let spoof = source.replace("g:justification", "logic:groundingSourceOwner <https://blackcatinformatics.ca/gmeow/slices/logic> ; g:justification");
    for text in [&source, &spoof] {
        assert!(
            admit(
                text,
                domain_owner,
                "slices/core/publication/mappings/copy.ttl",
                &ownership
            )
            .is_err()
        );
    }
    let missing = native_kernel::CanonicalOwnership::default();
    assert!(admit(&source, owner, mapping_path, &missing).is_err());
    let mut spoofed = native_kernel::CanonicalOwnership::default();
    spoofed.record_module(
        owner,
        "slices/core/publication/module.ttl",
        module.as_bytes(),
        &module_dataset,
    );
    assert!(admit(&source, owner, mapping_path, &spoofed).is_err());
    let conflict = format!("{prefixes}g:Usage1 rdfs:isDefinedBy <{domain_owner}> .");
    let conflict_dataset = purrdf::parse_dataset(conflict.as_bytes(), "text/turtle", None).unwrap();
    ownership.record_module(
        domain_owner,
        "slices/core/publication/module.ttl",
        conflict.as_bytes(),
        &conflict_dataset,
    );
    assert!(admit(&source, owner, mapping_path, &ownership).is_err());

    let ordinary = format!(
        "{prefixes}g:Publication skos:closeMatch prov:Entity {{| g:sssomFile \"publication.tsv\" |}} ."
    );
    assert!(
        admit(
            &ordinary,
            domain_owner,
            "slices/core/publication/mappings/native.ttl",
            &ownership
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn projection_literal_catalog_refuses_old_broad_overlap_unknown_and_guarded_exemption() {
    let broad = REGISTRY.replace("https://schema.org/HowTo\"", "https://schema.org/\"");
    let dataset = purrdf::parse_dataset(broad.as_bytes(), "text/turtle", None).unwrap();
    assert!(catalog_families_from_dataset(&dataset).is_err());
    let dataset = purrdf::parse_dataset(REGISTRY.as_bytes(), "text/turtle", None).unwrap();
    let families = catalog_families_from_dataset(&dataset).unwrap();
    let mut unknown = rows();
    unknown.push(("urn:row:unknown", "https://schema.org/unregistered"));
    assert!(check_target_catalogs(&families, unknown, "authored").is_err());
    let measured = check_target_catalogs(&families, rows(), "authored").unwrap();
    let exemptions = residue_exemptions_from_dataset(&dataset).unwrap();
    assert!(
        check_residue_exemptions(
            &families,
            &exemptions,
            &BTreeSet::from(["https://schema.org/HowTo".to_owned()]),
            &measured,
            "authored"
        )
        .is_err()
    );
    let mut grown = measured;
    grown.insert("procedure".to_owned(), 8);
    assert!(
        check_residue_exemptions(&families, &exemptions, &BTreeSet::new(), &grown, "authored")
            .is_err()
    );
}
