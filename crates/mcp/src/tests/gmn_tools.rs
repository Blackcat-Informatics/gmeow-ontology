// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMN tool and teaching-resource consumer contracts.

use super::*;

// ── GMN verifier tools: gmn_validate / gmn_expand / gmn_explain ─────────────────

/// Read a frozen GMN-1 conformance-vector file from the shipped corpus (a test
/// artifact, never in the bundle) by its path relative to the vector root.
fn gmn_vector(rel: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fs::read_to_string(
        root.join("slices/grounding/lang/tests/gmn1-vectors")
            .join(rel),
    )
    .unwrap_or_else(|e| panic!("read GMN vector {rel}: {e}"))
}

/// `gmn_validate` accepts a frozen conformance vector (`{ok, conformant:true}`) and
/// rejects a perturbed document with the TYPED `lang:Gmn*Failure` class — the external
/// LLM's entry to the `@err` repair loop.
#[gmeow_test_batch_macros::batch_mcp_test]
fn gmn_validate_accepts_conformant_and_rejects_perturbed() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _env = EnvRestore::capture(&["GMEOW_LANG"]);
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::remove_var("GMEOW_LANG");
    }
    let server = consumer_server();

    // A frozen POSITIVE vector conforms.
    let good = gmn_vector("claim-basic.gmn");
    let ok = text_payload(server.call_tool_result("gmn_validate", &json!({ "gmn": good })));
    assert_eq!(ok["ok"], true, "{ok}");
    assert_eq!(
        ok["conformant"], true,
        "a frozen conformance vector must validate: {ok}"
    );
    assert!(
        ok.get("failure_class").is_none(),
        "a conformant document carries no failure class: {ok}"
    );

    // A perturbed document (a value glyph flipped to a non-canonical 3-digit fraction,
    // the frozen negative fixture the corpus pins) raises the TYPED failure class.
    let bad = gmn_vector("negative-codec/neg-malformed-number-frac.gmn");
    let defect = text_payload(server.call_tool_result("gmn_validate", &json!({ "gmn": bad })));
    assert_eq!(defect["ok"], true, "{defect}");
    assert_eq!(
        defect["conformant"], false,
        "the perturbed document must be rejected: {defect}"
    );
    assert_eq!(
        defect["failure_class"], "https://blackcatinformatics.ca/lang/GmnMalformedNumber",
        "the typed lang:Gmn*Failure class names the defect: {defect}"
    );
    assert_eq!(
        defect["failure_local_name"], "GmnMalformedNumber",
        "{defect}"
    );
    assert!(
        defect["message"].as_str().is_some_and(|m| !m.is_empty()),
        "the defect carries a message: {defect}"
    );
}

/// `gmn_expand` decodes a GMN-1 document to its GMN-0 normal form (alias/glyph → full
/// IRI) and its expansion round-trips: re-encoding equals the input under
/// `gmn0_canonically_equal`.
#[gmeow_test_batch_macros::batch_mcp_test]
fn gmn_expand_roundtrips() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _env = EnvRestore::capture(&["GMEOW_LANG"]);
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::remove_var("GMEOW_LANG");
    }
    let server = consumer_server();
    let doc = gmn_vector("claim-basic.gmn");
    let out = text_payload(server.call_tool_result("gmn_expand", &json!({ "gmn": doc.clone() })));
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(
        out["round_trip"], true,
        "the expansion carries a holding round-trip witness: {out}"
    );
    let expanded = out["expanded_nquads"].as_str().expect("expanded_nquads");
    assert!(
        !expanded.is_empty(),
        "the GMN-0 normal form is non-empty: {out}"
    );
    // The "expand alias/glyph → full IRI" direction: the compact `gmeow__gate1` token
    // expands to its full IRI under the gmeow namespace.
    assert!(
        expanded.contains("https://blackcatinformatics.ca/gmeow/"),
        "the GMN-0 normal form carries full IRIs, not compact aliases: {out}"
    );

    // Expand then re-encode equals the input under gmn0_canonically_equal.
    let reencoded = out["reencoded_gmn"].as_str().expect("reencoded_gmn");
    let dict = server.gmn_dictionary().expect("dictionary resolves");
    let input_model = gmn1_read(&Gmn1Document::from_text(doc), dict).expect("input reads");
    let re_model =
        gmn1_read(&Gmn1Document::from_text(reencoded.to_owned()), dict).expect("re-encoded reads");
    assert!(
        gmn0_canonically_equal(&input_model, &re_model).unwrap(),
        "expand then re-encode equals the input under gmn0_canonically_equal: {out}"
    );
}

/// `gmn_explain` resolves a known operator glyph (`¬` → `logic:not`) to its authored
/// fixity / precedence / arity and its controlled-NL gloss, and returns an HONEST typed
/// miss for an unknown glyph — never a fabricated answer.
#[gmeow_test_batch_macros::batch_mcp_test]
fn gmn_explain_names_fixity_and_gloss() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _env = EnvRestore::capture(&["GMEOW_LANG"]);
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::remove_var("GMEOW_LANG");
    }
    let server = consumer_server();

    // ¬ is the seeded prefix operator for logic:not (precedence 90, arity 1).
    let hit = text_payload(server.call_tool_result("gmn_explain", &json!({ "glyph": "¬" })));
    assert_eq!(hit["ok"], true, "{hit}");
    assert_eq!(
        hit["found"], true,
        "¬ is a covered GMN operator glyph: {hit}"
    );
    assert_eq!(hit["fixity_local_name"], "gmnFixityPrefix", "{hit}");
    assert_eq!(
        hit["precedence"], 90,
        "the graph-authored binding strength: {hit}"
    );
    assert_eq!(hit["arity"], 1, "{hit}");
    assert_eq!(
        hit["denotation_target"], "https://blackcatinformatics.ca/logic/not",
        "{hit}"
    );
    assert!(
        hit["denotation"]
            .as_str()
            .is_some_and(|d| d.contains("blackcatinformatics.ca")),
        "the lang:Denotation IRI is surfaced, not fabricated: {hit}"
    );
    // The gloss is the verbalizer rendering: the prefix template `<label> arg1`.
    assert!(
        hit["gloss"].as_str().is_some_and(|g| g.contains("arg1")),
        "the controlled-NL gloss is the prefix verbalizer rendering: {hit}"
    );
    // A GMN surface carries the record-initial sigil of the scope it reads in, because the
    // SAME glyph denotes different operators under different scopes (see `gmn_verbalize`);
    // the sigil is part of the surface, not decoration around it.
    assert_eq!(
        hit["gmn_surface"], "@ℒ ¬ arg1",
        "the GMN operator surface scopes the record and arranges the glyph in prefix \
             position: {hit}"
    );

    // An unknown glyph returns the honest typed miss, never a fabricated answer.
    let miss = text_payload(server.call_tool_result("gmn_explain", &json!({ "glyph": "☃" })));
    assert_eq!(miss["ok"], true, "{miss}");
    assert_eq!(
        miss["found"], false,
        "an unknown glyph is not found: {miss}"
    );
    assert_eq!(
        miss["failure_class"], "https://blackcatinformatics.ca/lang/GmnUncoveredTerm",
        "the miss is the typed lang:GmnUncoveredTerm class: {miss}"
    );
    assert_eq!(miss["failure_local_name"], "GmnUncoveredTerm", "{miss}");
}

/// The three GMN verifier tools are advertised in the CONSUMER surface (served off the
/// bundle alone, like `validate_local`, never dev-gated) and each advertises its
/// required arg honestly (the `tool()` allowlist addition).
#[gmeow_test_batch_macros::batch_mcp_test]
fn gmn_tools_are_advertised_in_the_consumer_surface() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _env = EnvRestore::capture(&["GMEOW_LANG"]);
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::remove_var("GMEOW_LANG");
    }
    let bytes = snapshot();
    let consumer = McpServer::from_snapshot(&bytes).unwrap();
    let result = consumer.tools_result();
    let arr = result["tools"].as_array().expect("tools array");
    for (name, req) in [
        ("gmn_validate", "gmn"),
        ("gmn_expand", "gmn"),
        ("gmn_explain", "glyph"),
    ] {
        let tool = arr
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name} is advertised: {result}"));
        let required = tool["inputSchema"]["required"]
            .as_array()
            .expect("required array");
        assert!(
            required.iter().any(|r| r == req),
            "{name} advertises its required arg `{req}`: {tool}"
        );
    }
}

/// The GMN-1 teachability primer is exposed as a CONSUMER MCP resource
/// (`gmeow://ontology/gmn1-primer`, served off the bundle alone), advertised in
/// `resources/list` and readable through `resources/read` — a self-contained, graph-derived,
/// budget-bounded card carrying the record sigils, the operator glyph table, and the repair
/// loop. The shared `llms_full` surface carries the same primer section.
#[gmeow_test_batch_macros::batch_mcp_test]
fn gmn1_primer_resource_is_advertised_and_readable() {
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let _env = EnvRestore::capture(&["GMEOW_LANG"]);
    unsafe {
        // SAFETY: tests mutate process env single-threaded under ENV_LOCK.
        env::remove_var("GMEOW_LANG");
    }
    let bytes = snapshot();
    let consumer = McpServer::from_snapshot(&bytes).unwrap();

    // Advertised in the consumer resource list.
    let list = consumer.resources_result();
    let resources = list["resources"].as_array().expect("resources array");
    assert!(
        resources
            .iter()
            .any(|r| r["uri"] == "gmeow://ontology/gmn1-primer"),
        "the gmn1-primer resource must be advertised: {list}"
    );

    // Readable through resources/read, with the primer heading + a repair card + an operator
    // glyph row present (the graph-derived teaching surface).
    let read = consumer.read_resource_result("gmeow://ontology/gmn1-primer");
    assert!(
        read.get("isError").is_none(),
        "primer read must succeed: {read}"
    );
    let text = read["contents"][0]["text"].as_str().expect("primer text");
    assert!(
        text.contains(&format!("## {}", gmn1_primer_heading())),
        "the primer resource must carry its heading: {text}"
    );
    assert!(
        text.contains("gmeow:GmnErr"),
        "the primer resource must teach the @err repair record"
    );
    assert!(
        text.contains("⊑ (infix"),
        "the primer resource must carry the operator glyph table (⊑ subsumption row)"
    );

    // The same primer section rides the shared `llms_full` surface.
    let full = consumer
        .view
        .llms_full_text(vec!["en".to_string()])
        .expect("llms_full builds with the primer");
    assert!(
        full.contains(&format!("## {}", gmn1_primer_heading())),
        "llms_full must carry the primer section"
    );
}

/// The primer heading constant, re-exposed for the resource test (the shared docs const).
fn gmn1_primer_heading() -> &'static str {
    gmeow_docs_model::gmn1_primer::PRIMER_HEADING
}
