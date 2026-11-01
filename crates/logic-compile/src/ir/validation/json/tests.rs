// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const WIRE: &str = r#"{"iri":"","target":{"ValueKeyed":{"predicate":"","value":"raw target"}},"properties":[{"path":"","min_count":0,"max_count":4294967295,"cardinality_provenance":"OwlRestriction","components":[{"NumericRange":{"min":-0.0,"max":1e+20,"min_inclusive":false,"max_inclusive":true}},{"PrecisionRange":{"min":1.0,"max":null,"min_inclusive":true,"max_inclusive":false}},{"Datatype":"raw datatype"},{"Class":"raw class"},{"NodeKindShacl":"Iri"},{"In":[{"Iri":"z"},{"Literal":["text","http://www.w3.org/1999/02/22-rdf-syntax-ns#langString","fr",null]},{"Iri":"a"}]},{"Pattern":{"regex":"[ab]","flags":"i"}},{"MinLength":0},{"MaxLength":4294967295},{"LanguageIn":["z","a","z"]},{"DateTimeRange":{"min":"raw min","max":null,"min_inclusive":false,"max_inclusive":true}},{"TerminologyBinding":{"terminology_id":"raw terminology","codes":["z","a","z"]}},{"OrdinalSet":{"pairs":[[-9223372036854775808,"z"],[9223372036854775807,"a"],[-9223372036854775808,"z"]]}},{"DateTimePattern":"raw pattern"},{"HasValue":{"Iri":"raw value"}},{"QualifiedValueShape":{"shape":[{"Not":{"Class":"z"}},{"Class":"a"}],"min":null,"max":0}},{"Not":{"Class":"raw"}},{"Or":[{"Class":"z"},{"Class":"a"}]},{"Xone":[{"Class":"z"},{"Class":"a"}]},{"OrProperties":["z","a","z"]},"UniqueLang"],"inverse":true,"reifier_shape":"raw reifier","reification_required":true,"severity":"Warning","message":"retained 中"}],"node_kind":"Annotation","standpoint":"raw standpoint","node_components":[{"OrProperties":["z","a","z"]}],"label":null,"failure_class":"raw failure"}"#;

#[test]
fn validation_ir_native_wire_retains_recursive_components_and_all_tags() {
    let shape: ValidationShapeIr = from_slice(WIRE.as_bytes()).unwrap();
    assert_eq!(to_vec(&shape), WIRE.as_bytes());
    assert!(shape.iri.is_empty() && shape.properties[0].path.is_empty());
    assert_eq!(shape.node_kind, NodeKind::Annotation);
    assert_eq!(shape.properties[0].components.len(), 21);
    let ConstraintComponent::NumericRange { min, .. } = &shape.properties[0].components[0] else {
        panic!("retained numeric range")
    };
    assert!(min.unwrap().is_sign_negative());
    let ConstraintComponent::LanguageIn(languages) = &shape.properties[0].components[9] else {
        panic!("retained language order")
    };
    assert_eq!(languages, &["z", "a", "z"]);
    let upper_language = WIRE.replace("\"fr\",null", "\"FR\",null");
    assert_eq!(
        to_vec(&from_slice::<ValidationShapeIr>(upper_language.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    for (kind, name) in [
        (NodeKind::ObjectLevelFormula, "ObjectLevelFormula"),
        (NodeKind::MetaLevelFormula, "MetaLevelFormula"),
        (NodeKind::Constraint, "Constraint"),
        (NodeKind::DerivationRule, "DerivationRule"),
        (NodeKind::Query, "Query"),
        (NodeKind::TransactionProgram, "TransactionProgram"),
        (NodeKind::ActionSchema, "ActionSchema"),
        (NodeKind::ValidationShape, "ValidationShape"),
        (NodeKind::Correspondence, "Correspondence"),
        (NodeKind::Annotation, "Annotation"),
    ] {
        let wire = WIRE.replace(
            "\"node_kind\":\"Annotation\"",
            &format!("\"node_kind\":\"{name}\""),
        );
        let shape: ValidationShapeIr = from_slice(wire.as_bytes()).unwrap();
        assert_eq!(shape.node_kind, kind);
        assert_eq!(to_vec(&shape), wire.as_bytes());
    }
    for name in [
        "Iri",
        "Literal",
        "BlankNode",
        "IriOrLiteral",
        "BlankNodeOrIri",
        "BlankNodeOrLiteral",
    ] {
        let wire = WIRE.replace(
            "\"NodeKindShacl\":\"Iri\"",
            &format!("\"NodeKindShacl\":\"{name}\""),
        );
        assert_eq!(
            to_vec(&from_slice::<ValidationShapeIr>(wire.as_bytes()).unwrap()),
            wire.as_bytes()
        );
    }
    for name in ["Violation", "Warning", "Info"] {
        let wire = WIRE.replace(
            "\"severity\":\"Warning\"",
            &format!("\"severity\":\"{name}\""),
        );
        assert_eq!(
            to_vec(&from_slice::<ValidationShapeIr>(wire.as_bytes()).unwrap()),
            wire.as_bytes()
        );
    }
    for name in ["OwlRestriction", "OptNative"] {
        let wire = WIRE.replace("\"OwlRestriction\"", &format!("\"{name}\""));
        assert_eq!(
            to_vec(&from_slice::<ValidationShapeIr>(wire.as_bytes()).unwrap()),
            wire.as_bytes()
        );
    }
    for target in [
        r#"{"Class":"raw"}"#,
        r#"{"SubjectsOf":"raw"}"#,
        r#"{"ObjectsOf":"raw"}"#,
        r#"{"DirectClass":"raw"}"#,
        r#"{"Sparql":"SELECT raw"}"#,
    ] {
        let wire = WIRE.replace(
            r#"{"ValueKeyed":{"predicate":"","value":"raw target"}}"#,
            target,
        );
        assert_eq!(
            to_vec(&from_slice::<ValidationShapeIr>(wire.as_bytes()).unwrap()),
            wire.as_bytes()
        );
    }
    let omitted = WIRE
        .replace("\"label\":null,", "")
        .replace("\"min\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<ValidationShapeIr>(omitted.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let unit_objects = WIRE
        .replace(
            "\"NodeKindShacl\":\"Iri\"",
            "\"NodeKindShacl\":{\"Iri\":null}",
        )
        .replace(",\"UniqueLang\"]", ",{\"UniqueLang\":null}]");
    assert_eq!(
        to_vec(&from_slice::<ValidationShapeIr>(unit_objects.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
}

#[test]
fn validation_ir_native_admission_refuses_record_variant_and_numeric_drift() {
    let unknown = WIRE
        .replace("\"iri\":", "\"future\":null,\"iri\":")
        .replace("\"path\":", "\"future\":null,\"path\":")
        .replace("\"predicate\":", "\"future\":null,\"predicate\":")
        .replace("\"min_inclusive\":", "\"future\":null,\"min_inclusive\":");
    assert_eq!(
        to_vec(&from_slice::<ValidationShapeIr>(unknown.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    for invalid in [
        WIRE.replace("\"inverse\":true,", ""),
        WIRE.replace("\"reification_required\":true,", ""),
        WIRE.replace("\"node_kind\":\"Annotation\",", ""),
        WIRE.replace(
            "\"node_components\":[{\"OrProperties\":[\"z\",\"a\",\"z\"]}],",
            "",
        ),
        WIRE.replace("\"components\":", "\"components\":[],\"components\":"),
        WIRE.replace(
            "\"predicate\":\"\"",
            "\"predicate\":\"earlier\",\"predicate\":\"\"",
        ),
        WIRE.replace("\"min_count\":0", "\"min_count\":-0"),
        WIRE.replace("\"min_count\":0", "\"min_count\":1e0"),
        WIRE.replace("\"MaxLength\":4294967295", "\"MaxLength\":4294967296"),
        WIRE.replace("\"max\":1e+20", "\"max\":1e400"),
        WIRE.replace(
            "[-9223372036854775808,\"z\"]",
            "[-9223372036854775809,\"z\"]",
        ),
        WIRE.replace("[-9223372036854775808,\"z\"]", "[-0,\"z\"]"),
        WIRE.replace("[-9223372036854775808,\"z\"]", "[1e0,\"z\"]"),
        WIRE.replace("[-9223372036854775808,\"z\"]", "[1,\"z\",\"extra\"]"),
        WIRE.replace("[-9223372036854775808,\"z\"]", "[1,null]"),
        WIRE.replace("\"fr\",null]", "\"fr\"]"),
        WIRE.replace("\"Annotation\"", "\"Unknown\""),
        WIRE.replace(",\"UniqueLang\"]", ",{\"UniqueLang\":false}]"),
        WIRE.replace(
            r#"{"ValueKeyed":{"predicate":"","value":"raw target"}}"#,
            r#"{"ValueKeyed":["","raw target"]}"#,
        ),
        r#"["shape",{"Class":"raw"},[],"ValidationShape",null,[],null,null]"#.to_owned(),
    ] {
        assert!(
            from_slice::<ValidationShapeIr>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let invalid = WIRE.replace("[-9223372036854775808,\"z\"]", "[1e0,\"z\"]");
    let error = from_slice::<ValidationShapeIr>(invalid.as_bytes()).unwrap_err();
    assert_eq!(
        error.pointer(),
        "/properties/0/components/12/OrdinalSet/pairs/0/0"
    );
}
