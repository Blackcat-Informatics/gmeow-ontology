// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Real builtin surface assembly under the selected segment features, without corpus access.

use super::*;
use std::collections::BTreeSet;

/// These handler branches need only the in-memory view: neither reads snapshot bytes.
#[cfg(all(feature = "reasoning", not(feature = "core")))]
#[test]
fn reasoning_only_handlers_validate_served_requests_and_defer_core() {
    let dataset = purrdf::parse_dataset(b"", "text/turtle", None)
        .expect("an empty caller dataset is valid RDF");
    let view = McpView::from_dataset(dataset, Arc::from(Vec::<u8>::new()))
        .expect("the reasoning view needs no documentation header");
    let server = McpServer::from_admitted_view(
        Arc::new(view),
        SegmentSet::reasoning_only(),
        Extension::new(),
        Some("en"),
    )
    .expect("assemble the actual reasoning-only server surface");

    let deferred = server
        .surface
        .dispatch_tool(&server, "lookup_term", &json!({"term": "urn:caller:term"}))
        .expect_err("the unlinked core implementation must defer");
    let deferred = deferred
        .downcast_ref::<crate::error::SegmentNotLoaded>()
        .expect("deferral is a typed routing outcome");
    assert_eq!(deferred.tool, "lookup_term");
    assert_eq!(deferred.segment, CORE_SEGMENT);

    let served = server
        .surface
        .dispatch_tool(
            &server,
            "reason_graph",
            &json!({"data": "", "format": "unsupported-profile"}),
        )
        .expect_err("the served handler must validate its own request");
    let served = served
        .downcast_ref::<crate::error::Mcp>()
        .expect("a served handler refusal must not become a deferral");
    assert!(
        served
            .message
            .starts_with("reason_graph: unrecognized RDF format")
    );
}

#[test]
fn selected_features_preserve_complete_registered_surface_and_segment_routing() {
    let linked = Surface::assemble(
        builtin_extension(SegmentSet::linked()).unwrap(),
        Extension::new(),
    )
    .expect("the selected feature profile registers a total surface");
    let names = linked.tool_names();
    assert_eq!(names.len(), TOOL_COUNT);
    assert_eq!(
        names.iter().copied().collect::<BTreeSet<_>>().len(),
        TOOL_COUNT
    );
    assert!(names.contains(&"lookup_term"));
    assert!(names.contains(&"reason_graph"));
    assert!(names.contains(&"verify_graph"));

    for (segments, core, reasoning, chase) in [
        (
            SegmentSet::linked(),
            cfg!(feature = "core"),
            cfg!(feature = "reasoning"),
            cfg!(feature = "reasoning"),
        ),
        (SegmentSet::core(), cfg!(feature = "core"), false, false),
        (
            SegmentSet::reasoning_only(),
            false,
            cfg!(feature = "reasoning"),
            cfg!(feature = "reasoning"),
        ),
    ] {
        let surface = Surface::assemble(builtin_extension(segments).unwrap(), Extension::new())
            .expect("each deployment has paired descriptors and handlers");
        assert_eq!(surface.tool_descriptors(), linked.tool_descriptors());
        assert_eq!(
            surface.resource_descriptors(),
            linked.resource_descriptors()
        );
        assert_eq!(surface.tool_names(), names);
        assert_eq!(segments.serves("lookup_term"), core);
        assert_eq!(segments.serves("reason_graph"), reasoning);
        assert_eq!(segments.serves("verify_graph"), chase);
    }
}
