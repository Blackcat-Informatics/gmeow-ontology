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
        "advice.FooAdviceConstraintShape.abc123".to_string(),
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

/// The `#advice-` section anchor — the single static resolution
/// target of every `advice.*` finding code — must appear EXACTLY ONCE on the page.
/// It heads the distinct Advice section; the `advice.` family rule must therefore be
/// pulled OUT of the compliance category grouping (which would emit a second
/// `id="advice-"`). Two identical anchors is an HTML defect that the membership check
/// above cannot catch (a set dedupes), so this guards the moved-anchor duplicate risk.
#[test]
fn advice_section_anchor_appears_exactly_once() {
    let model = common::cached_model();
    let html = to_html(&model, &Page::ConstraintCatalog);
    // The exact `id="advice-"` substring (with the closing quote) matches ONLY the
    // section head — per-term sub-anchors are `id="advice-<term>"` and do not match.
    let count = html.matches("id=\"advice-\"").count();
    assert_eq!(
        count, 1,
        "the #advice- section anchor must appear exactly once (it heads the distinct \
         Advice section and is every advice.* code's resolution target); found {count}"
    );
    // And a realized-shaped advisory finding code still resolves to that one anchor.
    let anchor_uri = gmeow_validate::rule_catalog::catalog_anchor_uri(
        "advice.BareEntitySortalAdviceConstraint.deadbeef",
    );
    assert_eq!(fragment_of(&anchor_uri), "advice-");
    assert!(anchor_ids(&html).contains("advice-"));
}

/// Non-vacuity guard for the REAL page's Advice section. The test above only proves
/// the single `#advice-` SECTION-HEAD anchor appears exactly once — that assertion
/// still passes if the section is honestly rendered but carries ZERO entries (see the
/// `model.advice_entries.is_empty()` early-return in
/// `crates/docs/src/render.rs::md_advice_section`). The per-term sub-anchors and the
/// avoid/use/how-to prose are otherwise only exercised against the SYNTHETIC
/// `tiny_model()` in `render.rs`'s unit tests, never against the real generated
/// catalog. This test closes that gap: it renders the real page from
/// `generated/catalog/constraint-catalog.nq` and asserts the two realized advice
/// carriers (`gmeow:Entity`, `gmeow:Event` — see `advice_entries.len() == 2` in the
/// N-Quads fanout) are actually present with their per-term anchors AND stable,
/// verbatim slices of their real `adviceAvoidWhen` / `adviceUseWhen` / `adviceHowToUse`
/// prose. A regression that silently emptied the real Advice section, or that dropped
/// an entry's prose while leaving the section head intact, fails this test even though
/// it would NOT fail `advice_section_anchor_appears_exactly_once` above.
#[test]
fn advice_section_on_the_real_page_is_non_vacuous() {
    let model = common::cached_model();
    let html = to_html(&model, &Page::ConstraintCatalog);

    // The per-term sub-anchors for both realized advice carriers.
    assert!(
        html.contains("id=\"advice-Entity\""),
        "the real page is missing the gmeow:Entity advice sub-anchor:\n{html}"
    );
    assert!(
        html.contains("id=\"advice-Event\""),
        "the real page is missing the gmeow:Event advice sub-anchor:\n{html}"
    );

    // The deontic-modality prose-line markers must actually be emitted (not just the
    // section head / heading text). `to_html` runs the Markdown body through
    // `pulldown-cmark`, so the `**label:**` bold Markdown the renderer emits (see
    // `md_advice_section`'s `"- **{label}:** {value}"` format string) becomes a real
    // `<strong>label:</strong>` element on the actual HTML page, not a literal
    // `**...**` substring.
    assert!(
        html.contains("<strong>Avoid when:</strong>"),
        "the real page's Advice section carries no 'Avoid when' prose line:\n{html}"
    );
    assert!(
        html.contains("<strong>Use when:</strong>"),
        "the real page's Advice section carries no 'Use when' prose line:\n{html}"
    );
    assert!(
        html.contains("<strong>How to use:</strong>"),
        "the real page's Advice section carries no 'How to use' prose line:\n{html}"
    );

    // Stable, verbatim substrings of each realized carrier's actual prose (sourced
    // from `generated/catalog/constraint-catalog.nq`'s `gmeow:advice/Entity` and
    // `gmeow:advice/Event` subjects) — specific enough that an empty section, a
    // placeholder, or a swapped/truncated entry would fail.
    assert!(
        html.contains(
            "Avoid typing an instance as a bare gmeow:Entity when a more specific sortal applies"
        ),
        "the real page is missing gmeow:Entity's verbatim adviceAvoidWhen prose:\n{html}"
    );
    assert!(
        html.contains(
            "Use as the universal occurrent whenever something HAPPENS"
        ),
        "the real page is missing gmeow:Event's verbatim adviceUseWhen prose:\n{html}"
    );
    assert!(
        html.contains(
            "Type the occurrence gmeow:Event, give it one or more gmeow:eventType values"
        ),
        "the real page is missing gmeow:Event's verbatim adviceHowToUse prose:\n{html}"
    );
}
