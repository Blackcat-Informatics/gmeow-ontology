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
fn schemas_stage_emits_all_authenticated_artifacts() {
    let stage = SchemasStage::new();
    assert_eq!(stage.id(), "stage-export-schemas");
    let root = repo_root();
    let first = crate::fixture::stage_artifacts(&root, 1, "stage-export-schemas")
        .expect("authenticated developer-schema projection");
    for path in SCHEMA_PATHS {
        assert!(first.contains_key(path), "missing {path}");
        assert!(!first[path].is_empty(), "{path} is empty");
    }
}

/// A value-vocabulary enum (e.g. `gmeow:TermStability`'s members) must reach
/// the emitted LinkML YAML — proving the enrichment
/// ([`crate::stages::schema_compile::enriched_compiled_schema`]) reached the
/// developer-surface emitters, not just the JSON-Schema leaf.
#[test]
fn value_vocab_enum_reaches_linkml_output() {
    let root = repo_root();
    let artifacts = crate::fixture::stage_artifacts(&root, 1, "stage-export-schemas")
        .expect("authenticated developer-schema projection");
    let linkml_yaml =
        String::from_utf8(artifacts[LINKML_PATH].clone()).expect("linkml yaml is utf8");
    // The `gmeow:TermStability` value vocabulary's seed members are
    // `gmeow:stabilityStable` / `gmeow:stabilityExperimental` /
    // `gmeow:stabilityDeprecated` (slices/core/versions/module.ttl) — assert the
    // enum class name and a real member CURIE both reached the LinkML output.
    assert!(
        linkml_yaml.contains("TermStability") && linkml_yaml.contains("stabilityStable"),
        "expected the TermStability value vocabulary to reach the LinkML output:\n{linkml_yaml}"
    );
}

#[test]
fn compiled_schema_emission_retains_complete_linkml_audit_and_shared_inputs() {
    use purrdf::shapes::json_schema::CompiledSchema;
    use purrdf::shapes::linkml::{LinkmlSlotDisposition, parse_linkml};

    // More unsafe source names than the progress log's five-example budget.
    let properties: BTreeMap<_, _> = (0..7)
        .map(|index| {
            (
                format!("gmeow:unsafe/name{index}"),
                Value::from(Object::new().with("type", "string")),
            )
        })
        .collect();
    let required: Vec<_> = properties.keys().cloned().collect();
    let definition = Object::new()
        .with("type", "object")
        .with("properties", properties.to_json())
        .with("required", required.to_json())
        .with(
            "dependentRequired",
            Value::from(
                Object::new().with("gmeow:unsafe/name0", vec!["gmeow:unsafe/name1"].to_json()),
            ),
        );
    let compiled = CompiledSchema {
        schema_json: purrdf_lex::json::write_compact(&Value::from(Object::new().with(
            "$defs",
            Value::from(Object::new().with("Probe", Value::from(definition))),
        ))),
        openapi_json: "{\"openapi\":\"3.1.0\"}".to_owned(),
        losses: purrdf::LossLedger::new(),
    };
    let expected_linkml =
        emit_linkml(&compiled, &linkml_config(linkml_prefixes()).unwrap()).unwrap();
    assert_eq!(expected_linkml.slot_renames.len(), 7);
    assert!(
        !expected_linkml.losses.entries().is_empty(),
        "authored dependency has a located LinkML loss"
    );
    let expected_typescript = emit_typescript(&compiled, &typescript_config().unwrap()).unwrap();
    let expected_graphql = emit_graphql(&compiled, &graphql_config().unwrap()).unwrap();

    let artifacts = render_compiled_schemas(&compiled).expect("actual six-artifact emitter");
    assert_eq!(
        artifacts
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        SCHEMA_PATHS
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
    );
    assert_eq!(artifacts[LINKML_PATH], expected_linkml.yaml.as_bytes());
    assert_eq!(
        artifacts[LINKML_LOSSES_PATH],
        expected_linkml.losses.render_json().as_bytes(),
        "native version, every loss and every location ship without rewriting"
    );
    assert_eq!(
        artifacts[TYPESCRIPT_PATH], expected_typescript.artifacts[TYPESCRIPT_DECLARATION_ARTIFACT],
        "TypeScript receives the original schema"
    );
    assert_eq!(
        artifacts[GRAPHQL_PATH], expected_graphql.artifacts[GRAPHQL_SCHEMA_ARTIFACT],
        "GraphQL receives the original schema"
    );
    assert_eq!(
        artifacts[GRAPHQL_NAME_MAP_PATH],
        expected_graphql.artifacts[GRAPHQL_NAME_MAP_ARTIFACT]
    );

    let text = std::str::from_utf8(&artifacts[LINKML_NAME_MAP_PATH]).unwrap();
    assert!(text.starts_with("{\n  \"schema_version\": 1,\n  \"sanitize_policy\": \"rename\",\n"));
    assert!(text.ends_with("}\n") && !text.ends_with("}\n\n"));
    let audit = purrdf_lex::json::read(text).unwrap();
    assert_eq!(
        audit
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "schema_version",
            "sanitize_policy",
            "element_names",
            "slot_renames",
            "slot_diagnostics"
        ]
    );
    assert_eq!(
        audit.get("element_names"),
        Some(&expected_linkml.element_names.to_json())
    );
    assert_eq!(
        audit.get("slot_renames"),
        Some(&Value::Array(
            expected_linkml
                .slot_renames
                .iter()
                .map(|row| row.to_json())
                .collect()
        )),
        "every field of every rename ships"
    );
    assert_eq!(
        audit.get("slot_diagnostics"),
        Some(&Value::Array(
            expected_linkml
                .slot_diagnostics
                .iter()
                .map(|row| row.to_json())
                .collect()
        ))
    );
    let yaml = parse_linkml(std::str::from_utf8(&artifacts[LINKML_PATH]).unwrap()).unwrap();
    for row in &expected_linkml.slot_renames {
        assert_eq!(row.disposition, LinkmlSlotDisposition::IdentityPreserved);
        assert_eq!(row.old_slot_uri.as_deref(), Some(row.source_name.as_str()));
        assert_eq!(row.emitted_slot_uri, row.source_name);
        let attribute = yaml
            .as_value()
            .get("classes")
            .unwrap()
            .get(&row.emitted_class)
            .unwrap()
            .get("attributes")
            .unwrap()
            .get(&row.new_slot_name)
            .unwrap();
        assert_eq!(
            attribute.get("slot_uri").and_then(Value::as_str),
            Some(row.source_name.as_str()),
            "unsafe source RDF identity must survive the emitted name"
        );
        assert_eq!(
            attribute.get("required").and_then(Value::as_bool),
            Some(true)
        );
    }
    let dependency_location = expected_linkml
        .losses
        .entries()
        .iter()
        .filter_map(|loss| loss.location.as_deref())
        .find(|location| location.subject.as_deref() == Some("#/$defs/Probe/dependentRequired"))
        .expect("native dependency loss location")
        .display();
    let losses =
        purrdf_lex::json::read(std::str::from_utf8(&artifacts[LINKML_LOSSES_PATH]).unwrap())
            .unwrap();
    assert!(
        losses
            .get("losses")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .any(|loss| {
                loss.get("location").and_then(Value::as_str) == Some(dependency_location.as_str())
            }),
        "the actual sidecar retains the complete native dependency location"
    );

    let safe = CompiledSchema {
        schema_json:
            r#"{"$defs":{"Safe":{"type":"object","properties":{"gmeow:label":{"type":"string"}}}}}"#
                .to_owned(),
        openapi_json: "{}".to_owned(),
        losses: purrdf::LossLedger::new(),
    };
    let safe_artifacts = render_compiled_schemas(&safe).unwrap();
    let safe_audit =
        purrdf_lex::json::read(std::str::from_utf8(&safe_artifacts[LINKML_NAME_MAP_PATH]).unwrap())
            .unwrap();
    assert_eq!(safe_audit.get("slot_renames"), Some(&Value::Array(vec![])));
    assert_eq!(
        safe_audit.get("slot_diagnostics"),
        Some(&Value::Array(vec![]))
    );
}
