// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Browser-library consumer contract using the authenticated producer-selected medium.

use crate::storage::InMemorySegmentLibrary;
use crate::{append_library_segments, build_nt_segment, run_list_candidates_in, with_library_lock};
use serde_json::Value;

/// `list_candidates` returns REAL, non-error results against the browser library:
/// an untouched library lists nothing, and a committed candidate segment lists the
/// candidate with its disposition and target provenance.
#[gmeow_test_batch_macros::batch_mcp_test]
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
