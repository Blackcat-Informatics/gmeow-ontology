// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::{
    record::{from_slice, to_vec},
    write_pretty,
};

const PROJECTION: &str = r#"{"plan":"raw plan","source_graph":null,"standpoint":"raw standpoint","preconditions":[{"root":"pre","digest":"raw pre digest","facts":[{"subject":"<urn:s>","predicate":"urn:p","object":"\"quoted text\"@MiXeD"}],"tracked_states":["z","a"]}],"loop_count":2,"loop_variable":{"lexical_form":"raw variable","datatype":"raw datatype","language":"MiXeD","direction":"raw direction"},"loop_period":null,"loop_body":["z","a","z"],"steps":[{"id":"step-a","cycle":0,"position":1,"source_node":"action schema","kind":"action"},{"id":"step-g","cycle":1,"position":0,"source_node":"guard schema","kind":"guard"}],"edges":[{"from":"a","to":"b","kind":"serial"},{"from":"b","to":"c","kind":"guard-then"},{"from":"b","to":"d","kind":"guard-else"},{"from":"d","to":"a","kind":"next-iteration"}],"actions":[{"schema":"raw schema","preconditions":[{"root":"action pre","digest":"raw digest","facts":[],"tracked_states":[]}],"effects":[{"root":"effect","digest":"raw effect","facts":[],"tracked_states":["state"]}],"resources":["z","a"],"capabilities":["capability"],"observations":["observation"],"outcomes":[{"source_node":"outcome","case":"case","continuation":null,"compensation":"raw compensation"}],"conditional_steps":[{"source_node":"conditional","guard":{"root":"conditional guard","digest":"raw condition","facts":[],"tracked_states":[]},"then_steps":["z","a"]}]}],"guards":[{"guard":"raw guard","condition":{"root":"guard condition","digest":"guard digest","facts":[],"tracked_states":[]},"then_steps":["then"],"else_steps":["else"]}],"freshness":[{"tracked_state":"state","currency":{"lexical_form":"PT1H","datatype":"raw duration","language":null,"direction":null}}],"loss_evidence":["loss z","loss a"],"complement":{"source_digest":"raw source digest","source_graph":null,"plan":"raw complement plan","standpoint":null,"loop_body":["body"],"expression_digests":["z","a"],"source_facts":[{"graph":null,"subject":"<<opaque subject>>","predicate":"urn:source","object":"\"rendered\ntext\"@MiXeD"}]},"certificate":{"version":"raw version","output_contract":"raw contract","plan":"raw certificate plan","source_digest":"raw source","projection_digest":"raw projection","certificate_digest":"raw certificate"}}"#;
const RECOVERY: &str = r#"{"version":"raw recovery","output_contract":"raw recovery contract","plan":"raw plan","projection_certificate_digest":"raw cert","observed_input_digest":"raw observed","verdict":"planned-skeleton-recovered","occurrences":[{"occurrence":"raw occurrence","graph":null,"schema":"raw schema"}],"recovered_schemas":["z","a"],"unobserved_planned_schemas":["b"],"off_plan_occurrences":["off"],"loss_evidence":["raw loss"],"recovery_digest":"raw recovery digest"}"#;
const PRETTY_RECOVERY: &str = r#"{
  "version": "raw recovery",
  "output_contract": "raw recovery contract",
  "plan": "raw plan",
  "projection_certificate_digest": "raw cert",
  "observed_input_digest": "raw observed",
  "verdict": "planned-skeleton-recovered",
  "occurrences": [
    {
      "occurrence": "raw occurrence",
      "graph": null,
      "schema": "raw schema"
    }
  ],
  "recovered_schemas": [
    "z",
    "a"
  ],
  "unobserved_planned_schemas": [
    "b"
  ],
  "off_plan_occurrences": [
    "off"
  ],
  "loss_evidence": [
    "raw loss"
  ],
  "recovery_digest": "raw recovery digest"
}
"#;

#[test]
fn native_plan_payloads_preserve_complete_wire_raw_literals_and_pretty_artifacts() {
    let projection: PlanProjection = from_slice(PROJECTION.as_bytes()).unwrap();
    let recovery: PlanRecovery = from_slice(RECOVERY.as_bytes()).unwrap();
    assert_eq!(to_vec(&projection), PROJECTION.as_bytes());
    assert_eq!(to_vec(&recovery), RECOVERY.as_bytes());
    assert_eq!(
        projection
            .loop_variable
            .as_ref()
            .unwrap()
            .language
            .as_deref(),
        Some("MiXeD")
    );
    assert_eq!(
        projection
            .loop_variable
            .as_ref()
            .unwrap()
            .direction
            .as_deref(),
        Some("raw direction")
    );
    assert_eq!(projection.loop_body, ["z", "a", "z"]);
    assert_eq!(
        projection.complement.source_facts[0].object,
        "\"rendered\ntext\"@MiXeD"
    );
    assert_eq!(recovery.recovered_schemas, ["z", "a"]);
    // Certificate validation remains an explicit operation after hydration.
    assert!(super::super::verify_plan_projection_certificate(&projection).is_err());
    let mut pretty = write_pretty(&projection.to_json()).into_bytes();
    pretty.push(b'\n');
    assert!(pretty.starts_with(b"{\n  \"plan\": \"raw plan\",\n"));
    assert!(pretty.ends_with(b"}\n") && !pretty.ends_with(b"}\n\n"));
    assert_eq!(from_slice::<PlanProjection>(&pretty).unwrap(), projection);
    let mut pretty = write_pretty(&recovery.to_json()).into_bytes();
    pretty.push(b'\n');
    assert_eq!(pretty, PRETTY_RECOVERY.as_bytes());
    assert_eq!(from_slice::<PlanRecovery>(&pretty).unwrap(), recovery);
    let unit_objects = PROJECTION
        .replace("\"kind\":\"action\"", "\"kind\":{\"action\":null}")
        .replace("\"kind\":\"guard-then\"", "\"kind\":{\"guard-then\":null}");
    assert_eq!(
        to_vec(&from_slice::<PlanProjection>(unit_objects.as_bytes()).unwrap()),
        PROJECTION.as_bytes()
    );
    let unit_verdict = RECOVERY.replace(
        "\"verdict\":\"planned-skeleton-recovered\"",
        "\"verdict\":{\"planned-skeleton-recovered\":null}",
    );
    assert_eq!(
        to_vec(&from_slice::<PlanRecovery>(unit_verdict.as_bytes()).unwrap()),
        RECOVERY.as_bytes()
    );
    let omitted = PROJECTION
        .replace("\"source_graph\":null,", "")
        .replace("\"loop_period\":null,", "")
        .replace("\"continuation\":null,", "")
        .replace("\"language\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<PlanProjection>(omitted.as_bytes()).unwrap()),
        PROJECTION.as_bytes()
    );
    let omitted_graph = RECOVERY.replace("\"graph\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<PlanRecovery>(omitted_graph.as_bytes()).unwrap()),
        RECOVERY.as_bytes()
    );
    let max_count = PROJECTION.replace(
        "\"loop_count\":2",
        &format!("\"loop_count\":{}", usize::MAX),
    );
    let projection: PlanProjection = from_slice(max_count.as_bytes()).unwrap();
    assert_eq!(projection.loop_count, usize::MAX);
    assert_eq!(to_vec(&projection), max_count.as_bytes());
}

#[test]
fn native_plan_admission_requires_named_records_counts_and_exact_kebab_tags() {
    let unknown = PROJECTION
        .replace("\"plan\":", "\"future\":null,\"plan\":")
        .replace("\"source_node\":", "\"future\":null,\"source_node\":")
        .replace("\"root\":", "\"future\":null,\"root\":")
        .replace("\"subject\":", "\"future\":null,\"subject\":")
        .replace("\"schema\":", "\"future\":null,\"schema\":")
        .replace("\"datatype\":", "\"future\":null,\"datatype\":");
    assert_eq!(
        to_vec(&from_slice::<PlanProjection>(unknown.as_bytes()).unwrap()),
        PROJECTION.as_bytes()
    );
    for invalid in [
        PROJECTION.replacen("\"plan\":", "\"plan\":\"earlier\",\"plan\":", 1),
        PROJECTION.replace("\"steps\":", "\"steps\":[],\"steps\":"),
        PROJECTION.replace("\"loop_count\":2,", ""),
        PROJECTION.replace("\"loop_count\":2", "\"loop_count\":-0"),
        PROJECTION.replace("\"loop_count\":2", "\"loop_count\":2e0"),
        PROJECTION.replace("\"loop_count\":2", "\"loop_count\":18446744073709551616"),
        PROJECTION.replace("\"cycle\":0", "\"cycle\":-1"),
        PROJECTION.replace("\"position\":1", "\"position\":1.5"),
        PROJECTION.replace("\"source_node\":", "\"source_node\":\"earlier\",\"source_node\":"),
        PROJECTION.replace("\"resources\":[\"z\",\"a\"],", ""),
        PROJECTION.replace("\"datatype\":\"raw datatype\",", ""),
        PROJECTION.replace("\"direction\":\"raw direction\"", "\"direction\":false"),
        PROJECTION.replace("\"root\":\"pre\",", ""),
        PROJECTION.replace("\"kind\":\"action\"", "\"kind\":\"Action\""),
        PROJECTION.replace("\"kind\":\"guard-then\"", "\"kind\":\"guard_then\""),
        PROJECTION.replace("\"kind\":\"guard-then\"", "\"kind\":{\"guard-then\":false}"),
        PROJECTION.replace(r#"{"id":"step-a","cycle":0,"position":1,"source_node":"action schema","kind":"action"}"#, r#"["step-a",0,1,"action schema","action"]"#),
        r#"["plan",null,null,[],0,null,null,[],[],[],[],[],[],[],{},{}]"#.to_owned(),
    ] {
        assert!(from_slice::<PlanProjection>(invalid.as_bytes()).is_err(), "{invalid}");
    }
    for invalid in [
        RECOVERY.replace("\"occurrences\":", "\"occurrences\":[],\"occurrences\":"),
        RECOVERY.replace("\"recovery_digest\":\"raw recovery digest\"", "\"recovery_digest\":null"),
        RECOVERY.replace("\"schema\":\"raw schema\"", "\"schema\":false"),
        RECOVERY.replace("\"planned-skeleton-recovered\"", "\"PlannedSkeletonRecovered\""),
        RECOVERY.replace("\"verdict\":\"planned-skeleton-recovered\"", "\"verdict\":{\"planned-skeleton-recovered\":null,\"future\":null}"),
        r#"["version","contract","plan","certificate","input","planned-skeleton-recovered",[],[],[],[],[],"digest"]"#.to_owned(),
    ] {
        assert!(from_slice::<PlanRecovery>(invalid.as_bytes()).is_err(), "{invalid}");
    }
}
