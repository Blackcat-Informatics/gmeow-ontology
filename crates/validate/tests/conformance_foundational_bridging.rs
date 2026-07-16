// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance gates for the logic-owned grounding correspondence catalogs.
//!
//! Grounding is authored once under `slices/grounding/logic/mappings/` and compiled
//! into shipped `logic:Correspondence` nodes. SSSOM is a generated review dialect;
//! BFO/OBO/SUMO remain commitment-shifting bridge views, while gUFO/OWL/SHACL are
//! declared lowerings of the richer `logic:` source (Principles 4, 5, 7, and 17).

mod conformance_support;
use conformance_support::*;

use gmeow_validate::store::{parse_file_dataset, shacl_validate_dataset};
use std::collections::BTreeSet;

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const MATH: &str = "https://blackcatinformatics.ca/math/";
const GUFO: &str = "http://purl.org/nemo/gufo#";
const BFO: &str = "http://purl.obolibrary.org/obo/";
const RO: &str = "http://purl.obolibrary.org/obo/RO_";
const SUMO: &str = "https://www.ontologyportal.org/SUMO.owl#";
const YAMATO: &str = "http://www.hozo.jp/owl/YAMATO20210808.miz.owl#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SH: &str = "http://www.w3.org/ns/shacl#";
const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";

const ALIGN_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/alignPredicate";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";

const BRIDGE_VIEW: &str = "https://blackcatinformatics.ca/logic/BridgeView";
const COMMITMENT_SHIFTING: &str = "https://blackcatinformatics.ca/logic/CommitmentShiftingBridge";
const INSTITUTION_MORPHISM: &str = "https://blackcatinformatics.ca/logic/InstitutionMorphism";
const VALIDATION_ONLY: &str = "https://blackcatinformatics.ca/logic/ValidationOnly";
const SOUND_UNDER: &str = "https://blackcatinformatics.ca/logic/SoundUnderApproximation";
const UNSUPPORTED: &str = "https://blackcatinformatics.ca/logic/Unsupported";

const CATALOGS: [&str; 6] = [
    "gmeow-logic-gufo.sssom.tsv",
    "gmeow-logic-bfo.sssom.tsv",
    "gmeow-logic-obo.sssom.tsv",
    "gmeow-logic-sumo.sssom.tsv",
    "gmeow-logic-owl.sssom.tsv",
    "gmeow-logic-shacl.sssom.tsv",
];

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct BridgeRecord {
    iri: String,
    source: String,
    predicate: String,
    target: String,
    file: String,
    class: String,
    kind: String,
    preservation: String,
    confidence: String,
}

fn catalog_path() -> std::path::PathBuf {
    repo_root().join("slices/grounding/logic/mappings/grounding-bridges.ttl")
}

#[test]
fn grounding_bridge_fixture_pair_enforces_explicit_preservation() {
    let shapes_ttl = std::fs::read_to_string(repo_root().join("shapes/mapping-dsl-shapes.ttl"))
        .expect("mapping DSL shapes must be readable");
    let shapes =
        purrdf::shapes::engine::parse_shapes(&shapes_ttl).expect("mapping DSL shapes must parse");

    let positive =
        parse_file_dataset(&repo_root().join(
            "slices/grounding/logic/tests/conformance-fixtures/grounding-bridge-wellformed.ttl",
        ))
        .expect("positive grounding bridge fixture must parse");
    let positive_report = shacl_validate_dataset(&positive, &shapes);
    assert!(
        positive_report.conforms,
        "complete grounding bridge must conform: {:?}",
        positive_report.results
    );

    let negative = parse_file_dataset(&repo_root().join(
        "slices/grounding/logic/tests/counter-examples/grounding-bridge-missing-preservation.ttl",
    ))
    .expect("negative grounding bridge fixture must parse");
    let negative_report = shacl_validate_dataset(&negative, &shapes);
    assert!(
        !negative_report.conforms,
        "a grounding bridge without logic:preservationKind must be rejected"
    );
    assert!(
        format!("{:?}", negative_report.results).contains("preservationKind"),
        "the negative fixture must fail for its missing preservation judgment: {:?}",
        negative_report.results
    );
}

fn records() -> Vec<BridgeRecord> {
    let graph = GraphStore::parse_ttl_file(&catalog_path());
    graph
        .subjects_of_type(GROUNDING_CORRESPONDENCE)
        .into_iter()
        .map(|cell| {
            assert!(
                graph.has(Some(&cell), Some(RDF_TYPE), Some(TERM_EQUIVALENCE)),
                "{cell} must also be a gmeow:TermEquivalence frontend cell"
            );
            BridgeRecord {
                iri: cell.clone(),
                source: exactly_one(graph.objects(&cell, ALIGN_SUBJECT), &cell, "alignSubject"),
                predicate: exactly_one(
                    graph.objects(&cell, ALIGN_PREDICATE),
                    &cell,
                    "alignPredicate",
                ),
                target: exactly_one(graph.objects(&cell, ALIGN_OBJECT), &cell, "alignObject"),
                file: exactly_one(graph.objects_lex(&cell, SSSOM_FILE), &cell, "sssomFile"),
                class: exactly_one(graph.objects(&cell, MORPHISM_CLASS), &cell, "morphismClass"),
                kind: exactly_one(graph.objects(&cell, MORPHISM_KIND), &cell, "morphismKind"),
                preservation: exactly_one(
                    graph.objects(&cell, PRESERVATION_KIND),
                    &cell,
                    "preservationKind",
                ),
                confidence: exactly_one(graph.objects_lex(&cell, CONFIDENCE), &cell, "confidence"),
            }
        })
        .collect()
}

fn records_for(file: &str) -> Vec<BridgeRecord> {
    records().into_iter().filter(|r| r.file == file).collect()
}

fn record_for_source(file: &str, source: &str) -> BridgeRecord {
    records_for(file)
        .into_iter()
        .find(|row| row.source == logic(source))
        .unwrap_or_else(|| panic!("{file} must contain a row for logic:{source}"))
}

fn pairs_for(file: &str) -> BTreeSet<(String, String)> {
    records_for(file)
        .into_iter()
        .map(|r| (r.source, r.target))
        .collect()
}

fn logic(local: &str) -> String {
    format!("{LOGIC}{local}")
}

fn math(local: &str) -> String {
    format!("{MATH}{local}")
}

fn expected_pairs(entries: &[(&str, &str)]) -> BTreeSet<(String, String)> {
    entries
        .iter()
        .map(|(source, target)| (logic(source), (*target).to_owned()))
        .collect()
}

#[test]
fn grounding_catalog_is_single_owner_explicit_and_total() {
    let all = records();
    assert!(!all.is_empty(), "the grounding catalog must not be empty");

    let logic_module =
        GraphStore::parse_ttl_file(&repo_root().join("slices/grounding/logic/module.ttl"));
    let math_module =
        GraphStore::parse_ttl_file(&repo_root().join("slices/grounding/math/module.ttl"));
    let expected_files: BTreeSet<&str> = CATALOGS.into_iter().collect();
    let mut actual_files = BTreeSet::new();
    for record in &all {
        actual_files.insert(record.file.as_str());
        let owning_module = if record.source.starts_with(LOGIC) {
            &logic_module
        } else if record.source.starts_with(MATH) {
            assert_eq!(
                record.file, "gmeow-logic-sumo.sssom.tsv",
                "{} may cross the source-slice boundary only through the logic-owned SUMO catalog",
                record.iri
            );
            &math_module
        } else {
            panic!(
                "{} reverses the take1 orientation: source must be canonical logic: or math:, got {}",
                record.iri, record.source
            );
        };
        assert!(
            owning_module.has(Some(&record.source), None, None),
            "{} uses undeclared grounding source {}",
            record.iri,
            record.source
        );
    }
    assert_eq!(actual_files, expected_files);
    for file in CATALOGS {
        assert!(
            !records_for(file).is_empty(),
            "{file} must retain a non-empty canonical correspondence surface"
        );
    }

    let kernel =
        std::fs::read_to_string(repo_root().join("slices/core/kernel/mappings/equivalences.ttl"))
            .expect("kernel mappings");
    let central = std::fs::read_to_string(repo_root().join("dsl/mappings/mapping-sets.ttl"))
        .expect("mapping sets");
    assert!(!kernel.contains("eqFoundational"));
    assert!(!central.contains("mapsetFoundational"));
}

#[test]
fn gufo_catalog_covers_every_imported_class_without_silent_drop() {
    let gufo = GraphStore::parse_ttl_file(&repo_root().join("imports/gufo.ttl"));
    let imported: BTreeSet<String> = gufo
        .subjects_of_type(OWL_CLASS)
        .into_iter()
        .filter(|iri| iri.starts_with(GUFO))
        .collect();
    let rows = records_for("gmeow-logic-gufo.sssom.tsv");
    let targets: BTreeSet<String> = rows.iter().map(|r| r.target.clone()).collect();
    let class_targets: BTreeSet<String> = targets.intersection(&imported).cloned().collect();
    assert_eq!(
        class_targets, imported,
        "every stock gUFO class must be represented in the grounding catalog"
    );
    let relation_targets: BTreeSet<String> = targets.difference(&imported).cloned().collect();
    let expected_relation_targets =
        BTreeSet::from([format!("{GUFO}isProperPartOf"), format!("{GUFO}mediates")]);
    assert_eq!(
        relation_targets, expected_relation_targets,
        "the gUFO relation surface must contain only the two audited high-confidence bridges"
    );

    let unsupported_targets: BTreeSet<String> = rows
        .iter()
        .filter(|r| r.preservation == UNSUPPORTED)
        .map(|r| r.target.clone())
        .collect();
    let expected_unsupported: BTreeSet<String> = [
        "IntrinsicMode",
        "QualityValueAttributionSituation",
        "TemporaryConstitutionSituation",
        "TemporaryInstantiationSituation",
        "TemporaryParthoodSituation",
        "TemporaryRelationshipSituation",
    ]
    .into_iter()
    .map(|local| format!("{GUFO}{local}"))
    .collect();
    assert_eq!(unsupported_targets, expected_unsupported);
    assert!(rows.iter().all(|r| {
        r.class == logic("AffineCorrespondence")
            && r.kind == INSTITUTION_MORPHISM
            && matches!(r.preservation.as_str(), VALIDATION_ONLY | UNSUPPORTED)
    }));
}

#[test]
fn bfo_bridge_targets_real_vendored_classes_and_labels() {
    let expected: [(&str, &str, &str); 12] = [
        ("Individual", "BFO_0000001", "entity"),
        ("Endurant", "BFO_0000002", "continuant"),
        ("Event", "BFO_0000003", "occurrent"),
        ("Process", "BFO_0000015", "process"),
        ("Object", "BFO_0000040", "material entity"),
        ("FunctionalComplex", "BFO_0000030", "object"),
        ("Collection", "BFO_0000027", "object aggregate"),
        ("Mode", "BFO_0000020", "specifically dependent continuant"),
        (
            "Relator",
            "BFO_0000020",
            "specifically dependent continuant",
        ),
        ("Disposition", "BFO_0000016", "disposition"),
        ("Quality", "BFO_0000019", "quality"),
        ("Role", "BFO_0000023", "role"),
    ];
    let snapshot = GraphStore::parse_ttl_file(&repo_root().join("imports/targets/bfo.ttl"));
    let pairs = pairs_for("gmeow-logic-bfo.sssom.tsv");
    for (source, bfo_local, label) in expected {
        let target = format!("{BFO}{bfo_local}");
        assert!(pairs.contains(&(logic(source), target.clone())));
        assert!(snapshot.has(Some(&target), Some(RDF_TYPE), Some(OWL_CLASS)));
        assert!(snapshot.objects_lex(&target, RDFS_LABEL).contains(label));
    }
}

#[test]
fn commitment_shifting_catalogs_can_never_emit_equivalence() {
    for file in [
        "gmeow-logic-bfo.sssom.tsv",
        "gmeow-logic-obo.sssom.tsv",
        "gmeow-logic-sumo.sssom.tsv",
    ] {
        for row in records_for(file) {
            assert_eq!(row.class, BRIDGE_VIEW, "{}", row.iri);
            assert_eq!(row.kind, COMMITMENT_SHIFTING, "{}", row.iri);
            assert_eq!(row.preservation, VALIDATION_ONLY, "{}", row.iri);
            assert_ne!(row.predicate, format!("{SKOS}exactMatch"), "{}", row.iri);
        }
    }

    let expected_obo = expected_pairs(&[
        (
            "partOf",
            concat!("http://purl.obolibrary.org/obo/", "BFO_0000050"),
        ),
        (
            "overlaps",
            concat!("http://purl.obolibrary.org/obo/RO_", "0002131"),
        ),
        (
            "causalPartOf",
            concat!("http://purl.obolibrary.org/obo/RO_", "0002418"),
        ),
        (
            "disjoint",
            concat!("http://purl.obolibrary.org/obo/RO_", "0002171"),
        ),
        (
            "memberOf",
            concat!("http://purl.obolibrary.org/obo/RO_", "0002350"),
        ),
    ]);
    assert_eq!(pairs_for("gmeow-logic-obo.sssom.tsv"), expected_obo);
}

#[test]
fn audited_foundation_rows_use_only_warranted_relations() {
    let event = record_for_source("gmeow-logic-bfo.sssom.tsv", "Event");
    assert_eq!(event.predicate, format!("{SKOS}broadMatch"));
    let role = record_for_source("gmeow-logic-bfo.sssom.tsv", "Role");
    assert_eq!(role.predicate, format!("{SKOS}relatedMatch"));

    let gufo_proper_part = record_for_source("gmeow-logic-gufo.sssom.tsv", "properPartOf");
    assert_eq!(gufo_proper_part.target, format!("{GUFO}isProperPartOf"));
    assert_eq!(gufo_proper_part.predicate, format!("{SKOS}closeMatch"));
    assert_eq!(gufo_proper_part.preservation, VALIDATION_ONLY);
    assert_eq!(gufo_proper_part.confidence, "0.95");

    let gufo_mediates = record_for_source("gmeow-logic-gufo.sssom.tsv", "mediates");
    assert_eq!(gufo_mediates.target, format!("{GUFO}mediates"));
    assert_eq!(gufo_mediates.predicate, format!("{SKOS}closeMatch"));
    assert_eq!(gufo_mediates.preservation, VALIDATION_ONLY);
    assert_eq!(gufo_mediates.confidence, "0.9");

    let quantity = records_for("gmeow-logic-sumo.sssom.tsv")
        .into_iter()
        .find(|row| row.source == math("Quantity"))
        .expect("the logic-owned SUMO boundary must carry the math:Quantity bridge");
    assert_eq!(quantity.target, format!("{SUMO}Quantity"));
    assert_eq!(quantity.predicate, format!("{SKOS}broadMatch"));
    assert_eq!(quantity.class, BRIDGE_VIEW);
    assert_eq!(quantity.preservation, VALIDATION_ONLY);

    let all = records();
    for (source, target) in [
        (logic("OccurrentBoundary"), format!("{BFO}BFO_0000035")),
        (logic("precedes"), format!("{BFO}BFO_0000063")),
        (logic("Quantity"), format!("{SUMO}Quantity")),
    ] {
        assert!(
            all.iter()
                .all(|row| row.source != source || row.target != target),
            "the rejected grounding row {source} -> {target} must not survive"
        );
    }
}

#[test]
fn yamato_catalog_pins_material_quantity_and_quality_value() {
    let graph = GraphStore::parse_ttl_file(
        &repo_root().join("slices/grounding/logic/mappings/foundation-bridges.ttl"),
    );
    let yamato_cells: Vec<String> = graph
        .subjects_of_type(GROUNDING_CORRESPONDENCE)
        .into_iter()
        .filter(|cell| {
            graph
                .objects_lex(cell, SSSOM_FILE)
                .contains("gmeow-logic-yamato.sssom.tsv")
        })
        .collect();
    assert!(
        !yamato_cells.is_empty(),
        "the YAMATO grounding catalog must remain non-empty"
    );

    for (cell, source, target, confidence) in [
        (
            "https://blackcatinformatics.ca/gmeow/eqLogicYamatoQuantity",
            logic("Quantity"),
            format!("{YAMATO}amount_of_matter"),
            "0.9",
        ),
        (
            "https://blackcatinformatics.ca/gmeow/eqLogicYamatoQualityValue",
            logic("QualityValue"),
            format!("{YAMATO}quality_value"),
            "0.85",
        ),
    ] {
        assert_eq!(
            exactly_one(graph.objects(cell, ALIGN_SUBJECT), cell, "alignSubject"),
            source
        );
        assert_eq!(
            exactly_one(graph.objects(cell, ALIGN_OBJECT), cell, "alignObject"),
            target
        );
        assert_eq!(
            exactly_one(graph.objects(cell, ALIGN_PREDICATE), cell, "alignPredicate"),
            format!("{SKOS}closeMatch")
        );
        assert_eq!(
            exactly_one(graph.objects(cell, MORPHISM_CLASS), cell, "morphismClass"),
            BRIDGE_VIEW
        );
        assert_eq!(
            exactly_one(graph.objects(cell, MORPHISM_KIND), cell, "morphismKind"),
            COMMITMENT_SHIFTING
        );
        assert_eq!(
            exactly_one(
                graph.objects(cell, PRESERVATION_KIND),
                cell,
                "preservationKind"
            ),
            VALIDATION_ONLY
        );
        assert_eq!(
            exactly_one(graph.objects_lex(cell, CONFIDENCE), cell, "confidence"),
            confidence
        );
    }
}

#[test]
fn owl_catalog_pins_the_complete_compiler_construct_surface() {
    let expected = expected_pairs(&[
        (
            "subClassOf",
            concat!("http://www.w3.org/2000/01/rdf-schema#", "subClassOf"),
        ),
        (
            "equivalentClass",
            concat!("http://www.w3.org/2002/07/owl#", "equivalentClass"),
        ),
        (
            "disjointWith",
            concat!("http://www.w3.org/2002/07/owl#", "disjointWith"),
        ),
        (
            "subPropertyOf",
            concat!("http://www.w3.org/2000/01/rdf-schema#", "subPropertyOf"),
        ),
        (
            "equivalentProperty",
            concat!("http://www.w3.org/2002/07/owl#", "equivalentProperty"),
        ),
        (
            "inverseOf",
            concat!("http://www.w3.org/2002/07/owl#", "inverseOf"),
        ),
        (
            "domain",
            concat!("http://www.w3.org/2000/01/rdf-schema#", "domain"),
        ),
        (
            "range",
            concat!("http://www.w3.org/2000/01/rdf-schema#", "range"),
        ),
        (
            "transitiveProperty",
            concat!("http://www.w3.org/2002/07/owl#", "TransitiveProperty"),
        ),
        (
            "symmetricProperty",
            concat!("http://www.w3.org/2002/07/owl#", "SymmetricProperty"),
        ),
        (
            "functionalProperty",
            concat!("http://www.w3.org/2002/07/owl#", "FunctionalProperty"),
        ),
        (
            "inverseFunctionalProperty",
            concat!(
                "http://www.w3.org/2002/07/owl#",
                "InverseFunctionalProperty"
            ),
        ),
        (
            "reflexiveProperty",
            concat!("http://www.w3.org/2002/07/owl#", "ReflexiveProperty"),
        ),
        (
            "asymmetricProperty",
            concat!("http://www.w3.org/2002/07/owl#", "AsymmetricProperty"),
        ),
        (
            "irreflexiveProperty",
            concat!("http://www.w3.org/2002/07/owl#", "IrreflexiveProperty"),
        ),
        (
            "Restriction",
            concat!("http://www.w3.org/2002/07/owl#", "Restriction"),
        ),
        (
            "onProperty",
            concat!("http://www.w3.org/2002/07/owl#", "onProperty"),
        ),
        (
            "someValuesFrom",
            concat!("http://www.w3.org/2002/07/owl#", "someValuesFrom"),
        ),
        (
            "allValuesFrom",
            concat!("http://www.w3.org/2002/07/owl#", "allValuesFrom"),
        ),
        (
            "hasValue",
            concat!("http://www.w3.org/2002/07/owl#", "hasValue"),
        ),
        (
            "minCardinality",
            concat!("http://www.w3.org/2002/07/owl#", "minCardinality"),
        ),
        (
            "maxCardinality",
            concat!("http://www.w3.org/2002/07/owl#", "maxCardinality"),
        ),
        (
            "cardinality",
            concat!("http://www.w3.org/2002/07/owl#", "cardinality"),
        ),
        (
            "qualifiedCardinality",
            concat!("http://www.w3.org/2002/07/owl#", "qualifiedCardinality"),
        ),
        (
            "minQualifiedCardinality",
            concat!("http://www.w3.org/2002/07/owl#", "minQualifiedCardinality"),
        ),
        (
            "maxQualifiedCardinality",
            concat!("http://www.w3.org/2002/07/owl#", "maxQualifiedCardinality"),
        ),
        (
            "onClass",
            concat!("http://www.w3.org/2002/07/owl#", "onClass"),
        ),
        (
            "onDataRange",
            concat!("http://www.w3.org/2002/07/owl#", "onDataRange"),
        ),
        (
            "Enumeration",
            concat!("http://www.w3.org/2002/07/owl#", "oneOf"),
        ),
        ("oneOf", concat!("http://www.w3.org/2002/07/owl#", "oneOf")),
        (
            "Datarange",
            concat!("http://www.w3.org/2000/01/rdf-schema#", "Datatype"),
        ),
        (
            "onDatatype",
            concat!("http://www.w3.org/2002/07/owl#", "onDatatype"),
        ),
        (
            "withRestrictions",
            concat!("http://www.w3.org/2002/07/owl#", "withRestrictions"),
        ),
    ]);
    assert_eq!(pairs_for("gmeow-logic-owl.sssom.tsv"), expected);
    assert!(
        records_for("gmeow-logic-owl.sssom.tsv")
            .iter()
            .all(|r| { r.kind == INSTITUTION_MORPHISM && r.preservation == SOUND_UNDER })
    );
}

#[test]
fn shacl_catalog_pins_the_validation_and_rule_surface() {
    let expected = expected_pairs(&[
        (
            "ValidationShape",
            concat!("http://www.w3.org/ns/shacl#", "NodeShape"),
        ),
        (
            "PathShape",
            concat!("http://www.w3.org/ns/shacl#", "PropertyShape"),
        ),
        (
            "Constraint",
            concat!("http://www.w3.org/ns/shacl#", "SPARQLConstraint"),
        ),
        ("Rule", concat!("http://www.w3.org/ns/shacl#", "SPARQLRule")),
        (
            "onClass",
            concat!("http://www.w3.org/ns/shacl#", "targetClass"),
        ),
        (
            "valueClass",
            concat!("http://www.w3.org/ns/shacl#", "class"),
        ),
        (
            "severity",
            concat!("http://www.w3.org/ns/shacl#", "severity"),
        ),
        ("message", concat!("http://www.w3.org/ns/shacl#", "message")),
        ("not", concat!("http://www.w3.org/ns/shacl#", "not")),
        ("or", concat!("http://www.w3.org/ns/shacl#", "or")),
        ("termIn", concat!("http://www.w3.org/ns/shacl#", "in")),
        (
            "termRegex",
            concat!("http://www.w3.org/ns/shacl#", "pattern"),
        ),
        (
            "directType",
            concat!("http://www.w3.org/ns/shacl#", "SPARQLTarget"),
        ),
        (
            "sparqlTarget",
            concat!("http://www.w3.org/ns/shacl#", "SPARQLTarget"),
        ),
        (
            "integrity",
            concat!("http://www.w3.org/ns/shacl#", "sparql"),
        ),
    ]);
    assert_eq!(pairs_for("gmeow-logic-shacl.sssom.tsv"), expected);
    assert!(records_for("gmeow-logic-shacl.sssom.tsv").iter().all(|r| {
        r.class == logic("AffineCorrespondence")
            && r.kind == INSTITUTION_MORPHISM
            && r.preservation == VALIDATION_ONLY
    }));
}

#[test]
fn generated_sssom_views_have_one_row_per_canonical_correspondence() {
    for file in CATALOGS {
        let path = repo_root().join("generated/mappings").join(file);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing generated view {}: {e}", path.display()));
        let rows = text
            .lines()
            .filter(|line| {
                !line.is_empty() && !line.starts_with('#') && !line.starts_with("subject_id")
            })
            .count();
        let expected_count = records_for(file).len();
        assert_eq!(rows, expected_count, "{} row count", path.display());
    }
    assert!(
        !repo_root()
            .join("generated/mappings/gmeow-foundational.sssom.tsv")
            .exists(),
        "the retired kernel-owned foundational view must be removed as an orphan"
    );
}

#[test]
fn bridge_tboxes_remain_by_reference_outside_object_level_closure() {
    let ontology = GraphStore::ontology();
    for namespace in [BFO, RO, SUMO] {
        let leaked: Vec<String> = ontology
            .subjects_of_type(OWL_CLASS)
            .into_iter()
            .filter(|iri| iri.starts_with(namespace))
            .collect();
        assert!(
            leaked.is_empty(),
            "external TBox leaked into object-level closure for {namespace}: {leaked:?}"
        );
    }
}

#[test]
fn bfo_remains_registered_as_a_by_reference_upper_target() {
    let bfo = gmeow_validate::self_desc::deposit_config::ALIGNMENT_TARGETS
        .iter()
        .find(|(key, _, _, _)| *key == "bfo")
        .expect("bfo must remain a registered target");
    assert_eq!(bfo.3, "upper");
    assert_eq!(
        gmeow_license::policy_for_license("CC-BY-4.0"),
        gmeow_license::LicensePolicy::ImportOk
    );
}

#[test]
fn namespace_constants_match_catalog_contract() {
    // These assertions make accidental namespace drift obvious at the test that owns
    // the catalogs, rather than surfacing later as opaque SSSOM prefix changes.
    assert_eq!(
        format!("{BFO}BFO_0000050"),
        "http://purl.obolibrary.org/obo/BFO_0000050"
    );
    assert_eq!(
        format!("{RO}0002131"),
        "http://purl.obolibrary.org/obo/RO_0002131"
    );
    assert_eq!(
        format!("{OWL}Restriction"),
        "http://www.w3.org/2002/07/owl#Restriction"
    );
    assert_eq!(
        format!("{RDFS}Datatype"),
        "http://www.w3.org/2000/01/rdf-schema#Datatype"
    );
    assert_eq!(
        format!("{SH}NodeShape"),
        "http://www.w3.org/ns/shacl#NodeShape"
    );
}
