// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use serde_json::{Value, json};

use purrdf::gts::examples::agent_memory::{RevisionOptions, StoreOptions};

use std::collections::BTreeSet;

use purrdf::RdfTerm;
use purrdf::gts::examples::agent_memory::ToolCallOptions;

use crate::storage::{
    ClaimStore, InMemoryClaimStore, InMemorySegmentLibrary, InMemoryStorage, Storage,
};
use crate::{
    append_library_segments, build_nt_segment, recall_json, run_list_candidates_in,
    store_segment_json, with_library_lock,
};

/// Store three claims and recall them: the browser store returns REAL, non-error
/// results, ranked by the same token-overlap relevance the native package uses, and
/// a suppressed claim drops out of the default recall.
#[test]
fn recall_returns_real_results_against_the_browser_claim_store() {
    let store = InMemoryClaimStore::default();

    let widgets = store
        .store_claim(
            "widgets are blue",
            StoreOptions {
                source: Some("mcp:test"),
                confidence: Some(0.9),
                according_to: None,
            },
        )
        .expect("the browser store accepts a well-formed claim");
    store
        .store_claim(
            "gadgets are red",
            StoreOptions {
                source: None,
                confidence: None,
                according_to: None,
            },
        )
        .expect("the browser store accepts a second claim");
    let retired = store
        .store_claim(
            "widgets are green",
            StoreOptions {
                source: None,
                confidence: None,
                according_to: None,
            },
        )
        .expect("the browser store accepts a third claim");

    // The tool body itself, driven against the browser store.
    let hit: Value = serde_json::from_str(
        &recall_json(&store, &json!({"query": "widgets"})).expect("recall runs"),
    )
    .expect("recall returns JSON");
    assert_eq!(
        hit["ok"], true,
        "recall against the browser store must return a REAL non-error result: {hit}"
    );
    let texts: Vec<&str> = hit["claims"]
        .as_array()
        .expect("claims array")
        .iter()
        .map(|c| c["text"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        texts,
        vec!["widgets are green", "widgets are blue"],
        "both widget claims must be recalled, most recent first among equal scores, \
             and the non-matching gadget claim must not: {hit}"
    );

    // A revision retires a claim: the default recall stops returning it, and asking
    // for suppressed claims brings it back — the store REMEMBERS the suppression
    // rather than deleting the record.
    store
        .revise_claim(
            &retired.id,
            RevisionOptions {
                reason: Some("superseded by the blue measurement"),
                superseded_by: Some(&widgets.id),
            },
        )
        .expect("the browser store accepts a revision");

    let after: Value = serde_json::from_str(
        &recall_json(&store, &json!({"query": "widgets"})).expect("recall runs"),
    )
    .expect("recall returns JSON");
    let texts: Vec<&str> = after["claims"]
        .as_array()
        .expect("claims array")
        .iter()
        .map(|c| c["text"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        texts,
        vec!["widgets are blue"],
        "a suppressed claim must drop out of the default recall: {after}"
    );

    let with_suppressed: Value = serde_json::from_str(
        &recall_json(
            &store,
            &json!({"query": "widgets", "include_suppressed": true}),
        )
        .expect("recall runs"),
    )
    .expect("recall returns JSON");
    assert_eq!(
        with_suppressed["claims"]
            .as_array()
            .expect("claims array")
            .len(),
        2,
        "the suppressed claim is retained and returned on request: {with_suppressed}"
    );
    assert_eq!(
        store.revisions().len(),
        1,
        "the revision itself is recorded, reason and successor included"
    );

    // The store's two input rules are enforced, not merely documented.
    assert!(
        store
            .store_claim(
                "   ",
                StoreOptions {
                    source: None,
                    confidence: None,
                    according_to: None
                }
            )
            .is_err(),
        "an empty claim must be refused"
    );
    assert!(
        store
            .store_claim(
                "out of range",
                StoreOptions {
                    source: None,
                    confidence: Some(1.5),
                    according_to: None
                }
            )
            .is_err(),
        "a confidence outside 0.0..=1.0 must be refused"
    );
}

/// `store_segment` returns the browser store's REAL serialization — the field a
/// session export reads to carry the store it ran against.
///
/// This is the tool the console's export exists to call. Before it there was none:
/// the export read `store_nquads ?? nquads` off a `recall` result, and no engine tool
/// returns either field, so a console session could RECORD a store it could never
/// EXPORT. The assertions below are over the parsed answer, so an engine that answered
/// with an empty or absent serialization would fail here rather than downstream.
#[test]
fn store_segment_serializes_the_browser_claim_store() {
    let store = InMemoryClaimStore::default();

    // An untouched store serializes to nothing, and says so in its counts: "the store
    // holds nothing" and "the store holds something I cannot carry" are opposite
    // situations, and only the second is a failure.
    let empty: Value =
        serde_json::from_str(&store_segment_json(&store).expect("store_segment runs"))
            .expect("store_segment returns JSON");
    assert_eq!(empty["ok"], true, "{empty}");
    assert_eq!(empty["claim_count"], 0, "{empty}");
    assert_eq!(empty["tool_call_count"], 0, "{empty}");
    assert_eq!(empty["nquads"], "", "{empty}");

    let claim = store
        .store_claim(
            "the console can export what it stored",
            StoreOptions {
                source: Some("mcp:test"),
                confidence: Some(0.75),
                according_to: None,
            },
        )
        .expect("stores");
    store
        .record_tool_call(
            "urn:gmeow:tool:store_claim",
            ToolCallOptions {
                arguments: Some(r#"{"text":"the console can export what it stored"}"#),
                result: Some(r#"{"ok":true}"#),
                invocation: None,
                generated: &[claim.id.as_str()],
            },
        )
        .expect("records");

    let read: Value =
        serde_json::from_str(&store_segment_json(&store).expect("store_segment runs"))
            .expect("store_segment returns JSON");
    assert_eq!(read["claim_count"], 1, "{read}");
    assert_eq!(read["tool_call_count"], 1, "{read}");
    let nquads = read["nquads"].as_str().expect("a serialization: {read}");
    assert!(
        !nquads.trim().is_empty(),
        "a store holding state must serialize to a non-empty segment: {read}"
    );

    // The answer is RDF, not a string that looks like it: it parses, and the claim's
    // text survives the round trip through the parser.
    let dataset = purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
        .expect("the served segment parses as N-Quads");
    let carried: BTreeSet<String> = purrdf::flat_rdf_quads_from_dataset(&dataset)
        .into_iter()
        .filter_map(|quad| match quad.object {
            RdfTerm::Literal(literal) => Some(literal.lexical_form),
            _ => None,
        })
        .collect();
    assert!(
        carried.contains("the console can export what it stored"),
        "the stored claim's text must ride in the segment: {carried:?}"
    );

    // And it is genuinely re-seedable: a second store built from the segment holds the
    // same claim, which is what makes an exported session replayable.
    let seeded = InMemoryClaimStore::default();
    assert_eq!(
        crate::storage::seed_claim_store(&seeded, nquads).expect("seeds"),
        (1, 1)
    );
    assert_eq!(
        seeded.claims().expect("reads")[0].text,
        "the console can export what it stored"
    );
}

/// `list_candidates` returns REAL, non-error results against the browser library:
/// an untouched library lists nothing, and a committed candidate segment lists the
/// candidate with its disposition and target provenance.
#[test]
fn list_candidates_returns_real_results_against_the_browser_library() {
    let library = InMemorySegmentLibrary::default();

    // An untouched library is EMPTY, not an error — the same answer the native
    // backend gives for a file that does not exist yet.
    let empty: Value =
        serde_json::from_str(&run_list_candidates_in(&library, None, None).expect("lists"))
            .expect("list_candidates returns JSON");
    assert_eq!(
        empty["ok"], true,
        "an untouched browser library must list cleanly: {empty}"
    );
    assert_eq!(empty["candidate_count"], 0, "…and list nothing: {empty}");

    // Commit one admitted candidate through the very same locked, all-or-nothing
    // path the `submit_candidate` tool uses.
    let node = "urn:gmeow:candidate:browser-test";
    // Every IRI is built from the ONE declaration site — the shared `logic:`
    // namespace and the crate's own candidate-vocabulary constants — so a namespace
    // change cannot leave this fixture asserting a term nothing else recognizes.
    let logic_ns = gmeow_logic_compile::ir::LOGIC_NAMESPACE;
    let rdf_type = crate::RDF_TYPE;
    let candidate_class = crate::GMEOW_AUTHORING_CANDIDATE;
    let for_slice = crate::GMEOW_CANDIDATE_FOR_SLICE;
    let body = format!(
        "<{node}> <{rdf_type}> <{candidate_class}> .\n\
             <{node}> <{rdf_type}> <{logic_ns}Conjecture> .\n\
             <{node}> <{logic_ns}conjectureLifecycleState> <{logic_ns}ConjectureOpen> .\n\
             <{node}> <{for_slice}> <urn:gmeow:slice:demo> .\n"
    );
    let segment = build_nt_segment(&[], &crate::tests::probe_medium(), &body)
        .expect("the candidate body parses");
    with_library_lock(&library, || append_library_segments(&library, &[segment]))
        .expect("the browser library commits under its own lock");

    let listed: Value =
        serde_json::from_str(&run_list_candidates_in(&library, None, None).expect("lists"))
            .expect("list_candidates returns JSON");
    assert_eq!(
        listed["ok"], true,
        "list_candidates against the browser library must return a REAL non-error \
             result: {listed}"
    );
    assert_eq!(
        listed["candidate_count"], 1,
        "the committed candidate must be listed: {listed}"
    );
    assert_eq!(listed["candidates"][0]["candidate"], node);
    assert_eq!(listed["candidates"][0]["disposition"], "in-library");
    assert_eq!(
        listed["candidates"][0]["for_slice"], "urn:gmeow:slice:demo",
        "the candidate's target provenance survives the round trip: {listed}"
    );

    // The filters are real filters, not decoration.
    let filtered: Value = serde_json::from_str(
        &run_list_candidates_in(&library, Some("urn:gmeow:slice:other"), None).expect("lists"),
    )
    .expect("list_candidates returns JSON");
    assert_eq!(
        filtered["candidate_count"], 0,
        "a slice filter that matches nothing lists nothing: {filtered}"
    );
}

/// The browser backend hands out ONE store per kind, so a claim written by one tool
/// call is visible to the next — a per-call store would be a store that forgets.
#[test]
fn the_browser_backend_shares_one_store_across_calls() {
    let backend = InMemoryStorage::new();
    // The browser backend keeps claims as live values, not as GTS segments, so no codec
    // catalog applies to it — it is the one store a snapshot says nothing about.
    backend
        .claim_store(&[])
        .expect("the browser backend always has a claim store")
        .store_claim(
            "persisted across calls",
            StoreOptions {
                source: None,
                confidence: None,
                according_to: None,
            },
        )
        .expect("stores");

    let seen = backend
        .claim_store(&[])
        .expect("the browser backend always has a claim store")
        .claims()
        .expect("reads");
    assert_eq!(
        seen.len(),
        1,
        "a second handle must see the first handle's claim"
    );
    assert_eq!(seen[0].text, "persisted across calls");

    // Configuration behaves like an environment: unset is unset, set is readable.
    assert!(backend.env_var("GMEOW_LANG").is_none());
    backend.set_env("GMEOW_LANG", "fr");
    assert_eq!(backend.env_var("GMEOW_LANG").as_deref(), Some("fr"));
}
