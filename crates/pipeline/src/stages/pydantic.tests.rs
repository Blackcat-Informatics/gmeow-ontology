// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn pydantic_stage_emits_all_authenticated_artifacts() {
    let stage = PydanticStage::new();
    assert_eq!(stage.id(), "stage-export-pydantic");
    let root = repo_root();
    let first = crate::fixture::stage_artifacts(&root, 1, "stage-export-pydantic")
        .expect("authenticated Pydantic package projection");

    for member in [
        "gmeow_models/README.md",
        "gmeow_models/tags.py",
        "gmeow_models/schema_helpers.py",
        "gmeow_models/_base.py",
        "gmeow_models/__init__.py",
        "gmeow_models/py.typed",
        "gmeow_models/__about__.py",
    ] {
        let path = format!("{PACKAGE_DISK_PREFIX}{member}");
        assert!(first.contains_key(&path), "missing {path}");
    }

    assert!(!first.contains_key(&format!("{PACKAGE_DISK_PREFIX}gmeow_models/models.py")));
    let models_py =
        String::from_utf8(first[&format!("{PACKAGE_DISK_PREFIX}gmeow_models/tags.py")].clone())
            .expect("tags.py utf8");
    assert!(
        models_py.contains("class Tagging"),
        "tags module has no Tagging class"
    );
    assert!(models_py.contains("definitionDigest"));
    assert!(models_py.contains("https://blackcatinformatics.ca/gmeow/Tagging"));
    assert!(
        models_py.contains("PurrdfBaseModel"),
        "models.py does not use the purrdf base class"
    );

    let about_py = String::from_utf8(
        first[&format!("{PACKAGE_DISK_PREFIX}gmeow_models/__about__.py")].clone(),
    )
    .expect("about.py utf8");
    assert!(
        about_py.contains("__version__"),
        "__about__.py does not carry a version binding:\n{about_py}"
    );
}

fn synthetic_models() -> (
    purrdf::shapes::json_schema::CompiledSchema,
    purrdf::shapes::shapes::Shapes,
    gmeow_docs::DocsModel,
) {
    let root = tempfile::tempdir().expect("synthetic source directory");
    for (slice, name) in [("tags", "Cat"), ("people", "Person")] {
        let dir = root.path().join(slice);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.ttl"), format!(
            "@prefix g: <https://blackcatinformatics.ca/gmeow/> .\n<https://blackcatinformatics.ca/gmeow/slices/{slice}> a g:Slice ; g:sliceTier g:tierCore ."
        )).unwrap();
        std::fs::write(dir.join("module.ttl"), format!(
            "@prefix g: <https://blackcatinformatics.ca/gmeow/> .\n@prefix l: <https://blackcatinformatics.ca/logic/> .\n@prefix s: <http://www.w3.org/2004/02/skos/core#> .\n@prefix r: <http://www.w3.org/2000/01/rdf-schema#> .\ng:{name} a l:Class ; r:label \"{name}\" ; s:definition \"A documented {name}.\" ; g:useWhen \"Use this {name}.\" ; g:avoidWhen \"Avoid guessing.\" ; g:howToUse \"Name its identity.\" ; s:example \"An example {name}.\" ."
        )).unwrap();
        let mut source = std::fs::read_to_string(dir.join("module.ttl")).unwrap();
        source.push_str(&format!(
            "\ng:{name} r:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/{slice}> .\n"
        ));
        if slice == "people" {
            // A foreign class reference is not a second owning definition.
            source.push_str("g:Cat a l:Class .\n");
        }
        std::fs::write(dir.join("module.ttl"), source).unwrap();
    }
    let mut model = gmeow_docs::DocsModel::from_slice_dir(root.path()).unwrap();
    for term in &mut model.terms {
        term.content_digest = format!("blake3:{}", "a".repeat(64));
    }
    let dataset = purrdf::parse_dataset(
        br#"
        @prefix g: <https://blackcatinformatics.ca/gmeow/> .
        @prefix sh: <http://www.w3.org/ns/shacl#> .
        g:CatShape a sh:NodeShape ; sh:targetClass g:Cat ;
            sh:property [ sh:path g:owner ; sh:class g:Person ; sh:maxCount 1 ] .
        g:PersonShape a sh:NodeShape ; sh:targetClass g:Person ;
            sh:property [ sh:path g:pet ; sh:class g:Cat ; sh:maxCount 1 ] .
    "#,
        "text/turtle",
        None,
    )
    .unwrap();
    let shapes = purrdf::shapes::shapes::from_dataset(&dataset).unwrap();
    let mut compiled =
        purrdf::shapes::json_schema::compile(&shapes, &gmeow_ns::gmeow_json_schema_namespaces())
            .unwrap();
    let mut schema: serde_json::Value = serde_json::from_str(&compiled.schema_json).unwrap();
    schema["$defs"]["CatKindEnum"] = serde_json::json!({"type": "string", "enum": ["gmeow:Cat"]});
    compiled.schema_json = serde_json::to_string(&schema).unwrap();
    (compiled, shapes, model)
}

#[test]
fn pydantic_routed_models_carry_ontology_metadata_and_cross_slice_references() {
    let (compiled, shapes, model) = synthetic_models();
    let cats: Vec<_> = model
        .terms
        .iter()
        .filter(|term| term.iri == "https://blackcatinformatics.ca/gmeow/Cat")
        .collect();
    assert_eq!(cats.len(), 1, "foreign declarations do not create owners");
    assert_eq!(
        cats[0].owner_slice,
        "https://blackcatinformatics.ca/gmeow/slices/tags"
    );
    let config = topology::config(&compiled, &shapes, &model).unwrap();
    let package = emit_pydantic(&compiled, &config).unwrap();
    assert!(!package.artifacts.contains_key("gmeow_models/models.py"));
    for module in ["tags", "people", "schema_helpers"] {
        assert!(
            package
                .artifacts
                .contains_key(&format!("gmeow_models/{module}.py"))
        );
    }
    let cat = std::str::from_utf8(&package.artifacts["gmeow_models/tags.py"]).unwrap();
    for expected in [
        "class Cat",
        "gmeow:Cat",
        "definitionDigest",
        "documentation",
        "When to use",
        "When to avoid",
        "How to use",
        "Examples",
        "https://blackcatinformatics.ca/gmeow/terms/",
        "Person",
        "Slice IRI:",
        "Tier:",
        "DOI:",
    ] {
        assert!(cat.contains(expected), "missing {expected} in {cat}");
    }
    let init = std::str::from_utf8(&package.artifacts["gmeow_models/__init__.py"]).unwrap();
    assert!(init.contains("model_rebuild"));
    let helpers = &config.topology().unwrap().classes()["Annotation"];
    assert_eq!(helpers.module_path(), "schema_helpers");
    assert!(helpers.json_schema_extra().is_empty());
    assert_eq!(
        config.topology().unwrap().classes()["CatKindEnum"].module_path(),
        "schema_helpers"
    );
    let readme = String::from_utf8(topology::readme(&config).unwrap()).unwrap();
    assert!(readme.contains("gmeow_models.tags") && readme.contains("gmeow_models.people"));
    assert_eq!(
        package.artifacts,
        emit_pydantic(&compiled, &config).unwrap().artifacts
    );
}

#[test]
fn pydantic_imported_opt_targets_keep_their_source_in_shared_models() {
    use crate::stages::compile_logic::{
        OPT_BLOODPRESSURE_BASE, OPT_DATATYPES_BASE, OPT_SOURCE_PATH, OPT_TEST_DATATYPES_PATH,
    };
    use gmeow_logic_compile::opt_lift::{
        OptConstraintIr, OptConstraintKind, OptInterval, lift_opt_to_validation_shape,
    };

    let (_, mut shapes, model) = synthetic_models();
    let mut lifted = Vec::new();
    let mut expected = Vec::new();
    for (base, source, label) in [
        (OPT_BLOODPRESSURE_BASE, OPT_SOURCE_PATH, "Blutdruck"),
        (OPT_DATATYPES_BASE, OPT_TEST_DATATYPES_PATH, "Datatype"),
    ] {
        for (name, kind) in [
            (
                format!("{label}-Diastolic-cardinality"),
                OptConstraintKind::Cardinality {
                    path: "https://blackcatinformatics.ca/gmeow/items".into(),
                    min: Some(1),
                    max: Some(2),
                },
            ),
            (
                format!("{label}-quantity"),
                OptConstraintKind::Quantity {
                    magnitude_path: "https://blackcatinformatics.ca/gmeow/magnitude".into(),
                    interval: OptInterval {
                        lower: Some(0.0),
                        upper: Some(100.0),
                        lower_included: true,
                        upper_included: true,
                    },
                    units_path: "https://blackcatinformatics.ca/gmeow/units".into(),
                    units: "mm[Hg]".into(),
                    precision: None,
                },
            ),
        ] {
            let iri = format!("{base}{name}");
            lifted.push(
                lift_opt_to_validation_shape(&OptConstraintIr {
                    shape_iri: format!("{iri}Shape"),
                    target_class: iri.clone(),
                    kind,
                })
                .unwrap(),
            );
            expected.push((iri, source));
        }
    }
    let program = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None)
        .with_validation_shapes(lifted);
    let ttl = gmeow_logic_compile::projections::shapes::project_validation_shapes_shacl(&program);
    let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap();
    let imported = purrdf::shapes::shapes::from_dataset(&dataset).unwrap();
    shapes.node_shapes.extend(imported.node_shapes);
    let ns = gmeow_ns::gmeow_json_schema_namespaces();
    let compiled = purrdf::shapes::json_schema::compile(&shapes, &ns).unwrap();
    let config = topology::config(&compiled, &shapes, &model).unwrap();
    for (iri, source) in expected {
        let class = &config.topology().unwrap().classes()[&ns.def_key(&iri)];
        assert_eq!(class.module_path(), "schema_helpers");
        assert_eq!(
            class.json_schema_extra()["sourceClass"].as_str(),
            Some(iri.as_str())
        );
        assert_eq!(
            class.json_schema_extra()["sourceArtifact"].as_str(),
            Some(source)
        );
        assert!(!class.json_schema_extra().contains_key("definitionDigest"));
    }
    let package = emit_pydantic(&compiled, &config).unwrap();
    let helpers =
        std::str::from_utf8(&package.artifacts["gmeow_models/schema_helpers.py"]).unwrap();
    assert!(helpers.contains("Blutdruck-Diastolic-cardinality"));
    assert!(helpers.contains(OPT_SOURCE_PATH) && helpers.contains(OPT_TEST_DATATYPES_PATH));
    let cats = std::str::from_utf8(&package.artifacts["gmeow_models/tags.py"]).unwrap();
    assert!(cats.contains("definitionDigest") && cats.contains("gmeow:Cat"));
    assert_eq!(
        package.artifacts,
        emit_pydantic(&compiled, &config).unwrap().artifacts
    );
}

#[test]
fn pydantic_import_scopes_do_not_hide_missing_ontology_documentation() {
    use purrdf::shapes::shapes::Target;
    let (compiled, shapes, mut model) = synthetic_models();
    model
        .terms
        .retain(|term| term.iri != "https://blackcatinformatics.ca/gmeow/Cat");
    for iri in [
        "https://blackcatinformatics.ca/gmeow/openehr/bloodpressure-shadow/Cat",
        "https://blackcatinformatics.ca/gmeow/other/Cat",
        "https://blackcatinformatics.ca/math/imported/Cat",
    ] {
        let mut selected = shapes.clone();
        let shape = selected.node_shapes.iter_mut().find(|shape| {
            shape.targets.iter().any(|target| matches!(target, Target::Class(class) if class.as_str() == "https://blackcatinformatics.ca/gmeow/Cat"))
        }).unwrap();
        let target = &mut shape.targets[0];
        *target = Target::Class(iri.into());
        let compiled = purrdf::shapes::json_schema::compile(
            &selected,
            &gmeow_ns::gmeow_json_schema_namespaces(),
        )
        .unwrap();
        let error = topology::config(&compiled, &selected, &model)
            .unwrap_err()
            .to_string();
        assert!(error.contains("no term documentation"), "{iri}: {error}");
        assert!(
            error.contains(&gmeow_ns::gmeow_json_schema_namespaces().def_key(iri)),
            "{iri}: {error}"
        );
    }
    assert!(topology::config(&compiled, &shapes, &model).is_err());
}

#[test]
fn pydantic_routing_refuses_target_and_documentation_identity_collisions() {
    use purrdf::shapes::shapes::Target;
    let (compiled, mut shapes, model) = synthetic_models();
    let imported = format!(
        "{}Cat",
        crate::stages::compile_logic::OPT_BLOODPRESSURE_BASE
    );
    let cat = shapes
        .node_shapes
        .iter_mut()
        .find(|shape| {
            shape.targets.iter().any(|target| {
        matches!(target, Target::Class(iri) if iri.as_str() == "https://blackcatinformatics.ca/gmeow/Cat")
    })
        })
        .unwrap();
    cat.targets.push(Target::Class(imported.as_str().into()));
    assert!(
        topology::config(&compiled, &shapes, &model)
            .unwrap_err()
            .to_string()
            .contains("model target identity collision")
    );
    let cat = shapes
        .node_shapes
        .iter_mut()
        .find(|shape| {
            shape
                .targets
                .iter()
                .any(|target| matches!(target, Target::Class(iri) if iri.as_str() == imported))
        })
        .unwrap();
    cat.targets = vec![Target::Class(imported.as_str().into())];
    assert!(
        topology::config(&compiled, &shapes, &model)
            .unwrap_err()
            .to_string()
            .contains("model documentation identity mismatch")
    );
}

#[test]
fn pydantic_routing_refuses_conflicting_term_owners() {
    let (compiled, shapes, mut model) = synthetic_models();
    let mut conflicting = model
        .terms
        .iter()
        .find(|term| term.iri == "https://blackcatinformatics.ca/gmeow/Cat")
        .unwrap()
        .clone();
    conflicting.owner_slice = "https://blackcatinformatics.ca/gmeow/slices/people".into();
    model.terms.push(conflicting);
    assert!(
        topology::config(&compiled, &shapes, &model)
            .unwrap_err()
            .to_string()
            .contains("model definition identity collision")
    );
}

#[test]
fn pydantic_routing_refuses_missing_digest_owner_and_class_documentation() {
    let (compiled, shapes, mut model) = synthetic_models();
    let saved = model.clone();
    model.terms[0].content_digest.clear();
    assert!(
        topology::config(&compiled, &shapes, &model)
            .unwrap_err()
            .to_string()
            .contains("definition digest")
    );
    model = saved.clone();
    model.slices.clear();
    assert!(
        topology::config(&compiled, &shapes, &model)
            .unwrap_err()
            .to_string()
            .contains("owning slice")
    );
    model = saved;
    model.terms.clear();
    assert!(
        topology::config(&compiled, &shapes, &model)
            .unwrap_err()
            .to_string()
            .contains("no term documentation")
    );
}

#[test]
fn pydantic_stage_declares_fresh_metadata_edges_in_the_executable_dag() {
    let stage = PydanticStage::new();
    let spec = crate::run::full_spec();
    spec.validate().expect("acyclic pipeline");
    let entry = spec.stages.iter().find(|s| s.id == stage.id()).unwrap();
    assert_eq!(entry.consumes, stage.consumes());
    let authored =
        std::fs::read_to_string(repo_root().join("slices/core/pipeline/module.ttl")).unwrap();
    let rdf_spec = crate::loader::PipelineSpec::from_turtle(&[&authored]).unwrap();
    assert_eq!(
        rdf_spec.stage(stage.id()).unwrap().consumes,
        stage.consumes()
    );
    assert!(entry.consumes.iter().any(|s| s == "stage-term-manifest"));
    assert!(
        entry
            .consumes
            .iter()
            .any(|s| s == "stage-constraint-catalog")
    );
}

#[test]
fn pydantic_routing_refuses_colliding_slice_modules() {
    let (compiled, shapes, mut model) = synthetic_models();
    for (index, slice) in model.slices.iter_mut().enumerate() {
        let previous = slice.iri.clone();
        slice.iri = format!(
            "https://blackcatinformatics.ca/gmeow/slices/{}",
            ["one-name", "one_name"][index]
        );
        for term in &mut model.terms {
            if term.owner_slice == previous {
                term.owner_slice = slice.iri.clone();
            }
        }
    }
    assert!(
        topology::config(&compiled, &shapes, &model)
            .unwrap_err()
            .to_string()
            .contains("collide at model module")
    );
}
