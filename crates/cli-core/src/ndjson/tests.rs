// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure event records and explicitly failing in-memory sinks only.

use super::*;
use gmeow_errors::Severity;

fn line(event: NdjsonEvent<'_>) -> Vec<u8> {
    let mut bytes = Vec::new();
    event.write_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn json_ndjson_all_four_events_preserve_literal_field_order_and_one_lf() {
    assert_eq!(
        line(NdjsonEvent::StageStart {
            stage: "étape\n\t\"\\中😀"
        }),
        "{\"event\":\"stage_start\",\"stage\":\"étape\\n\\t\\\"\\\\中😀\"}\n".as_bytes()
    );
    assert_eq!(line(NdjsonEvent::StageEnd { stage: "tiny", elapsed_ms: u128::MAX }), b"{\"event\":\"stage_end\",\"stage\":\"tiny\",\"elapsed_ms\":340282366920938463463374607431768211455}\n");
    let finding = Finding::new(Severity::Error, "tiny.code", "missing\nevidence");
    assert_eq!(line(NdjsonEvent::Finding { tool: "tiny", finding: &finding }), b"{\"event\":\"finding\",\"tool\":\"tiny\",\"finding\":{\"severity\":\"error\",\"code\":\"tiny.code\",\"message\":\"missing\\nevidence\"}}\n");
    assert_eq!(
        line(NdjsonEvent::Summary {
            tool: "tiny",
            ok: false,
            errors: 2,
            warnings: 3
        }),
        b"{\"event\":\"summary\",\"tool\":\"tiny\",\"ok\":false,\"errors\":2,\"warnings\":3}\n"
    );
}

#[test]
fn json_ndjson_finding_event_retains_identity_provenance_and_typed_payload() {
    let finding: Finding = purrdf_lex::json::record::from_slice(br#"{"severity":"warning","code":"c","message":"m","source_graph":"urn:graph","source_focus":"urn:focus","locations":[{"line":18446744073709551615}],"related_labels":[{"location":{},"message":"related"}],"finding_iri":"urn:finding","anchor_iri":"urn:anchor","anchor_non_trivial":true,"antecedents":["urn:prior"],"failure_class":"urn:failure","category":"transient-chatter","standpoint":"binding","documented_terms":["urn:term"],"derived_from_quads":["urn:quad"],"cited_iris":["urn:citation"]}"#).unwrap();
    assert_eq!(line(NdjsonEvent::Finding { tool: "tool", finding: &finding }), b"{\"event\":\"finding\",\"tool\":\"tool\",\"finding\":{\"severity\":\"warning\",\"code\":\"c\",\"message\":\"m\",\"source_graph\":\"urn:graph\",\"source_focus\":\"urn:focus\",\"locations\":[{\"line\":18446744073709551615}],\"related_labels\":[{\"location\":{},\"message\":\"related\"}],\"finding_iri\":\"urn:finding\",\"anchor_iri\":\"urn:anchor\",\"anchor_non_trivial\":true,\"antecedents\":[\"urn:prior\"],\"failure_class\":\"urn:failure\",\"category\":\"transient-chatter\",\"standpoint\":\"binding\",\"documented_terms\":[\"urn:term\"],\"derived_from_quads\":[\"urn:quad\"],\"cited_iris\":[\"urn:citation\"]}}\n");
}

#[test]
fn json_ndjson_stream_has_no_extra_separators_and_preserves_large_summary_counts() {
    let mut bytes = Vec::new();
    NdjsonEvent::StageStart { stage: "" }
        .write_to(&mut bytes)
        .unwrap();
    NdjsonEvent::StageEnd {
        stage: "",
        elapsed_ms: 0,
    }
    .write_to(&mut bytes)
    .unwrap();
    NdjsonEvent::Summary {
        tool: "",
        ok: true,
        errors: 0,
        warnings: usize::MAX,
    }
    .write_to(&mut bytes)
    .unwrap();
    assert_eq!(bytes, format!("{{\"event\":\"stage_start\",\"stage\":\"\"}}\n{{\"event\":\"stage_end\",\"stage\":\"\",\"elapsed_ms\":0}}\n{{\"event\":\"summary\",\"tool\":\"\",\"ok\":true,\"errors\":0,\"warnings\":{}}}\n", usize::MAX).as_bytes());
    assert_eq!(bytes.iter().filter(|&&byte| byte == b'\n').count(), 3);
}

struct RefusingSink {
    accepted: usize,
    limit: usize,
}

impl Write for RefusingSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.accepted == self.limit {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "authored sink refusal",
            ));
        }
        let length = bytes.len().min(self.limit - self.accepted);
        self.accepted += length;
        Ok(length)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn json_ndjson_required_sink_failures_propagate_for_payload_and_line_separator() {
    let event = NdjsonEvent::StageStart { stage: "x" };
    let payload_len = br#"{"event":"stage_start","stage":"x"}"#.len();
    for limit in [0, 3, payload_len] {
        let mut sink = RefusingSink { accepted: 0, limit };
        let error = event.write_to(&mut sink).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert!(error.to_string().contains("authored sink refusal"));
        assert_eq!(sink.accepted, limit);
    }
}
