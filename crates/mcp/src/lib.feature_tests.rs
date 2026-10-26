// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Real builtin surface assembly under the selected segment features, without corpus access.

use super::*;
use std::collections::BTreeSet;

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
