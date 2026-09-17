// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::RdfDatasetBuilder;
use std::sync::Arc;

/// Parse `ttl` (with the `gmeow:` prefix predeclared) into a frozen [`RdfDataset`] —
/// the minimal build helper this file's `term_how_to_use` unit test needs.
fn dataset(ttl: &str) -> Arc<RdfDataset> {
    let parsed = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("ttl parses");
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(parsed.as_ref());
    builder.freeze().expect("freeze")
}

/// A term carrying TWO `@x-gmeow-english gmeow:howToUse` literals resolves to the
/// byte-first one deterministically — same result regardless of which literal was
/// authored first (G7: `term_how_to_use` must not be quad-iteration-order dependent,
/// mirroring `first_object`'s collect-sort-take-first pattern).
#[test]
fn term_how_to_use_picks_the_byte_first_literal_regardless_of_insertion_order() {
    let term = format!("{GMEOW}TestAbductiveTerm");
    let ttl_zebra_first = format!(
        "@prefix gmeow: <{GMEOW}> .\n<{term}> gmeow:howToUse \"Zebra message\"@x-gmeow-english , \"Alpha message\"@x-gmeow-english .\n"
    );
    let ttl_alpha_first = format!(
        "@prefix gmeow: <{GMEOW}> .\n<{term}> gmeow:howToUse \"Alpha message\"@x-gmeow-english , \"Zebra message\"@x-gmeow-english .\n"
    );

    let from_zebra_first = term_how_to_use(dataset(&ttl_zebra_first).as_ref(), &term);
    let from_alpha_first = term_how_to_use(dataset(&ttl_alpha_first).as_ref(), &term);

    assert_eq!(
        from_zebra_first, from_alpha_first,
        "the surfaced message must not depend on authoring/insertion order"
    );
    assert_eq!(
        from_zebra_first.as_deref(),
        Some("Alpha message"),
        "the byte-first literal (\"Alpha message\" < \"Zebra message\") wins"
    );
}

/// A term with no `@x-gmeow-english gmeow:howToUse` literal at all is honest absence,
/// not a panic or a fallback to another language.
#[test]
fn term_how_to_use_is_none_when_the_term_authors_no_source_language_literal() {
    let term = format!("{GMEOW}TestAbductiveTermNoProse");
    let ttl =
        format!("@prefix gmeow: <{GMEOW}> .\n<{term}> gmeow:howToUse \"English prose\"@en .\n");
    assert_eq!(term_how_to_use(dataset(&ttl).as_ref(), &term), None);
}
