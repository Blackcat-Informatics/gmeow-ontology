// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The anchor round-trip gate: proves every validator finding code resolves to
//! an anchor that actually exists on the rendered "What GMEOW enforces" page.
//!
//! [`gmeow_validate::rule_catalog::catalog_anchor_uri`] is the validator-side
//! half of the contract — it computes the `helpUri` fragment a finding's rule
//! deep-links to. The docs-side half is the `<a id="{slug}">` the constraint
//! catalog page (`Page::ConstraintCatalog`) actually renders. The two halves
//! are implemented independently (one crate mints codes and resolves them to a
//! fragment, the other renders the catalog HTML from the generated N-Quads
//! fanout) and nothing at the type level forces them to agree — a dynamic
//! family's representative row could drift out of the rendered set, or a
//! catalog regen could rename a slug, and a finding's help link would 404.
//! This test closes that gap: it renders the real page, extracts every anchor
//! id present, and asserts that `catalog_anchor_uri(code)`'s fragment is among
//! them for every code the validator can emit — static rows, family
//! representatives, AND concrete dynamic-family members (whose own code has no
//! catalog row and must resolve to the family stub's anchor).

use std::collections::BTreeSet;

use gmeow_docs::render::{Page, to_html};

mod common;

/// Every `id="..."` attribute value present in `html`, in first-seen order of
/// discovery (collected into a set — only membership matters here).
fn anchor_ids(html: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut rest = html;
    while let Some(pos) = rest.find("id=\"") {
        let after = &rest[pos + 4..];
        let Some(end) = after.find('"') else { break };
        ids.insert(after[..end].to_string());
        rest = &after[end + 1..];
    }
    ids
}

/// The fragment of a `#`-anchored URL (the part after the last `#`). Panics if
/// `uri` carries no fragment — every `catalog_anchor_uri` result does, since
/// [`gmeow_validate::rule_catalog::help_uri_for`] always appends one.
fn fragment_of(uri: &str) -> &str {
    uri.rsplit('#')
        .next()
        .unwrap_or_else(|| panic!("catalog anchor URI carries no '#' fragment: {uri}"))
}

#[test]
fn every_finding_code_resolves_to_a_live_catalog_anchor() {
    let model = common::cached_model();
    let html = to_html(&model, &Page::ConstraintCatalog);
    let ids = anchor_ids(&html);
    assert!(
        !ids.is_empty(),
        "the constraint catalog page rendered no anchors at all — \
         is generated/catalog/constraint-catalog.nq populated?"
    );

    // Every code the validator's registry can enumerate: the static rows and
    // one representative per dynamic family (`all_rules()` — the same
    // enumeration the catalog generator projects from).
    let mut codes: Vec<String> = gmeow_validate::rule_catalog::all_rules()
        .into_iter()
        .map(|seed| seed.code.to_string())
        .collect();

    // PLUS a representative CONCRETE member of each dynamic family, so the
    // family-stub resolution itself is exercised (a concrete member has no
    // catalog row of its own — it must resolve to the family representative's
    // anchor, not a slug of its own full code).
    codes.extend([
        "shacl.MinCountConstraintComponent".to_string(),
        "gts.untrusted-source".to_string(),
        "advice.candAdviceAvoidBareEntity".to_string(),
        "mylabel-dsl.nonconforming".to_string(),
    ]);

    let mut misses: Vec<String> = Vec::new();
    for code in &codes {
        let anchor_uri = gmeow_validate::rule_catalog::catalog_anchor_uri(code);
        let fragment = fragment_of(&anchor_uri);
        if !ids.contains(fragment) {
            misses.push(format!("{code} -> #{fragment}"));
        }
    }

    assert!(
        misses.is_empty(),
        "the following finding codes deep-link to an anchor the rendered \
         constraint-catalog page does NOT have (a broken helpUri — the \
         validator's catalog_anchor_uri and the docs page anchors have \
         drifted apart):\n{}",
        misses.join("\n")
    );
}
