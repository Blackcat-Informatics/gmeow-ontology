// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::{
    record::{from_slice, to_vec},
    write_pretty,
};

const WIRE: &str = r#"{"version":"raw report","program_digest":"raw program","certificates":[{"version":"raw certificate","declaration":"raw declaration","scope":{"program_digest":"scope program","declaration_digest":"scope declaration","context_digest":"","effect_digest":"scope effect","complement_digest":"scope complement","preservation_digest":"scope preservation","reasoning_contract_digest":"scope contract"},"first":{"correspondence":"first","get_leg":"first get","path":"^<urn:p>/<urn:q>","source_endpoint":"source","target_endpoint":"middle","standpoint":null,"checks":["endpoint-type","standpoint","pure-read-effect","path-admission","complement-retention","preservation","logical-gate"]},"second":{"correspondence":"second","get_leg":"second get","path":"raw second path","source_endpoint":"middle","target_endpoint":"target","standpoint":"raw standpoint","checks":["logical-gate","endpoint-type"]},"composite":{"correspondence":"composite","get_leg":"composite get","path":"raw composite path","source_endpoint":"source","target_endpoint":"target","standpoint":null,"checks":[]},"output_contract":"raw output","selected":{"kind":"original","reason":"search-limit"},"applied_rules":["rule z","rule a"],"rule_catalog_digest":"raw rules","original_cost":{"native_queries":2,"materialized_intermediate_relations":1,"path_nodes":5,"stable_tiebreak":"raw original"},"selected_cost":{"native_queries":1,"materialized_intermediate_relations":0,"path_nodes":4,"stable_tiebreak":"raw selected"},"limits":{"max_path_nodes":0},"engine_descriptor_hash":"raw engine","certificate_digest":"raw digest"}]}"#;

#[test]
fn native_physical_plan_report_preserves_complete_wire_and_pretty_without_newline() {
    let report: CompositionPlanReport = from_slice(WIRE.as_bytes()).unwrap();
    assert_eq!(to_vec(&report), WIRE.as_bytes());
    assert_eq!(report.certificates[0].scope.context_digest, "");
    assert_eq!(report.certificates[0].first.path, "^<urn:p>/<urn:q>");
    assert_eq!(report.certificates[0].applied_rules, ["rule z", "rule a"]);
    assert_eq!(report.certificates[0].first.checks.len(), 7);
    let empty = CompositionPlanReport {
        version: "v".into(),
        program_digest: "p".into(),
        certificates: Vec::new(),
    };
    assert_eq!(
        write_pretty(&empty.to_json()),
        "{\n  \"version\": \"v\",\n  \"program_digest\": \"p\",\n  \"certificates\": []\n}"
    );
    let pretty = write_pretty(&report.to_json());
    assert!(!pretty.ends_with('\n'));
    assert_eq!(
        from_slice::<CompositionPlanReport>(pretty.as_bytes()).unwrap(),
        report
    );
    let missing_standpoint = WIRE.replace("\"standpoint\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<CompositionPlanReport>(missing_standpoint.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let native_reason = WIRE.replace(
        "\"reason\":\"search-limit\"",
        "\"reason\":\"native-admission\"",
    );
    assert_eq!(
        to_vec(&from_slice::<CompositionPlanReport>(native_reason.as_bytes()).unwrap()),
        native_reason.as_bytes()
    );
    let reordered = WIRE.replace(
        r#"{"kind":"original","reason":"search-limit"}"#,
        r#"{"reason":"search-limit","kind":"original"}"#,
    );
    assert_eq!(
        to_vec(&from_slice::<CompositionPlanReport>(reordered.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let unit_objects = WIRE
        .replace(
            "\"reason\":\"search-limit\"",
            "\"reason\":{\"search-limit\":null}",
        )
        .replace("[\"endpoint-type\",", "[{\"endpoint-type\":null},");
    assert_eq!(
        to_vec(&from_slice::<CompositionPlanReport>(unit_objects.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let fused = WIRE.replace(
        r#"{"kind":"original","reason":"search-limit"}"#,
        r#"{"kind":"fused-witness-join"}"#,
    );
    assert_eq!(
        to_vec(&from_slice::<CompositionPlanReport>(fused.as_bytes()).unwrap()),
        fused.as_bytes()
    );
    let ignored_reason = fused.replace(
        r#"{"kind":"fused-witness-join"}"#,
        r#"{"reason":false,"kind":"fused-witness-join","reason":{},"future":null}"#,
    );
    assert_eq!(
        to_vec(&from_slice::<CompositionPlanReport>(ignored_reason.as_bytes()).unwrap()),
        fused.as_bytes()
    );
    let max_limit = WIRE.replace(
        "\"max_path_nodes\":0",
        &format!("\"max_path_nodes\":{}", usize::MAX),
    );
    let report: CompositionPlanReport = from_slice(max_limit.as_bytes()).unwrap();
    assert_eq!(report.certificates[0].limits.max_path_nodes, usize::MAX);
    assert_eq!(to_vec(&report), max_limit.as_bytes());
}

#[test]
fn native_physical_plan_admission_preserves_variant_local_fields_and_strict_counts() {
    let unknown = WIRE
        .replace("\"version\":", "\"future\":null,\"version\":")
        .replace("\"context_digest\":", "\"future\":null,\"context_digest\":")
        .replace("\"correspondence\":", "\"future\":null,\"correspondence\":")
        .replace(
            "\"stable_tiebreak\":",
            "\"future\":null,\"stable_tiebreak\":",
        )
        .replace("\"max_path_nodes\":", "\"future\":null,\"max_path_nodes\":")
        .replace(
            "\"kind\":\"original\"",
            "\"future\":null,\"kind\":\"original\"",
        );
    assert_eq!(
        to_vec(&from_slice::<CompositionPlanReport>(unknown.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    for invalid in [
        WIRE.replacen(
            "\"program_digest\":",
            "\"program_digest\":\"earlier\",\"program_digest\":",
            1,
        ),
        WIRE.replace(
            "\"context_digest\":",
            "\"context_digest\":\"earlier\",\"context_digest\":",
        ),
        WIRE.replace("\"checks\":", "\"checks\":[],\"checks\":"),
        WIRE.replace("\"selected\":", "\"selected\":null,\"selected\":"),
        WIRE.replace(
            "\"max_path_nodes\":0",
            "\"max_path_nodes\":0,\"max_path_nodes\":0",
        ),
        WIRE.replace("\"limits\":{\"max_path_nodes\":0}", "\"limits\":{}"),
        WIRE.replace("\"limits\":{\"max_path_nodes\":0}", "\"limits\":[0]"),
        WIRE.replace("\"native_queries\":2", "\"native_queries\":-0"),
        WIRE.replace("\"native_queries\":2", "\"native_queries\":2e0"),
        WIRE.replace(
            "\"native_queries\":2",
            "\"native_queries\":18446744073709551616",
        ),
        WIRE.replace(
            "\"materialized_intermediate_relations\":1",
            "\"materialized_intermediate_relations\":false",
        ),
        WIRE.replace("\"path_nodes\":4", "\"path_nodes\":-1"),
        WIRE.replace("\"kind\":\"original\",", ""),
        WIRE.replace(
            "\"kind\":\"original\"",
            "\"kind\":\"original\",\"kind\":\"fused-witness-join\"",
        ),
        WIRE.replace("\"kind\":\"original\"", "\"kind\":null"),
        WIRE.replace("\"kind\":\"original\"", "\"kind\":\"unknown\""),
        WIRE.replace(",\"reason\":\"search-limit\"", ""),
        WIRE.replace(
            "\"reason\":\"search-limit\"",
            "\"reason\":\"search-limit\",\"reason\":\"native-admission\"",
        ),
        WIRE.replace("\"reason\":\"search-limit\"", "\"reason\":false"),
        WIRE.replace(
            "\"reason\":\"search-limit\"",
            "\"reason\":\"search-limit-v1\"",
        ),
        WIRE.replace("[\"endpoint-type\",", "[\"endpoint-type-v1\","),
        r#"["report","program",[]]"#.to_owned(),
    ] {
        assert!(
            from_slice::<CompositionPlanReport>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let missing_reason = WIRE.replace(",\"reason\":\"search-limit\"", "");
    let missing_reason_error =
        from_slice::<CompositionPlanReport>(missing_reason.as_bytes()).unwrap_err();
    assert_eq!(missing_reason_error.pointer(), "/certificates/0/selected");
    assert_eq!(missing_reason_error.message(), "missing field `reason`");
    let duplicate_fused =
        r#"{"kind":"fused-witness-join","kind":"fused-witness-join","reason":false}"#;
    assert!(from_slice::<CompositionPhysicalPlan>(duplicate_fused.as_bytes()).is_err());
    assert!(from_slice::<CompositionPhysicalPlan>(br#"["original","search-limit"]"#).is_err());
}
