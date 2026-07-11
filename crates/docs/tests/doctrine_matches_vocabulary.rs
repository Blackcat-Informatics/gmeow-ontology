// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The DOCTRINE == VOCABULARY gate for the documentation-maturity standard.
//!
//! `docs/SLICE_GUIDE.md` § 6.8 is the single prose definition of FULL and
//! MAXIMAL — the tiers an author aims a slice at — stated as the exact list of
//! `gmeow:DocCoverageDimension` local names each tier requires. The minted
//! vocabulary lives in `slices/core/documentation/module.ttl` as the
//! `gmeow:maturityRequiresDimension` intents of `gmeow:docMaturityFull` /
//! `gmeow:docMaturityMaximal`, with a Rust twin in
//! [`gmeow_docs::maturity`] ([`anchor_table`]).
//!
//! This test parses the authored doctrine and asserts it EQUALS the vocabulary,
//! so the prose can never silently diverge from the ontology: editing the
//! guide's dimension list without the matching change to `module.ttl` (and its
//! Rust twin) — or vice versa — reds the build.
//!
//! # The parse contract (mirrored in the markdown)
//!
//! Two HTML-comment markers anchor the two canonical lists:
//! `<!-- doctrine-intent:full -->` and `<!-- doctrine-intent:maximal -->`. Each
//! marker is immediately followed by a single fenced code block. The FIRST
//! whitespace-delimited token on each line of that block is a
//! `gmeow:DocCoverageDimension` local name (`dim<Name>`); the rest of the line
//! is the human surface and is ignored. The `full` block lists FULL's whole
//! intent; the `maximal` block lists only the dimensions MAXIMAL adds ON TOP of
//! FULL (matching the nesting `Full ⊆ Maximal` and the module's "the full intent
//! plus …" phrasing).

use std::collections::BTreeSet;

use gmeow_docs::maturity::{Dimension, MaturityAnchor, anchor_table};

mod common;

/// The `full` and `maximal` doctrine-intent markers in `SLICE_GUIDE.md`.
const FULL_MARKER: &str = "<!-- doctrine-intent:full -->";
const MAXIMAL_MARKER: &str = "<!-- doctrine-intent:maximal -->";

/// A dimension local name is `dim` followed by an upper-case letter and more
/// alphanumerics — the shape of every `gmeow:DocCoverageDimension` local name.
fn is_dim_local(tok: &str) -> bool {
    tok.len() > 3
        && tok.starts_with("dim")
        && tok[3..].starts_with(|c: char| c.is_ascii_uppercase())
        && tok[3..].chars().all(|c| c.is_ascii_alphanumeric())
}

/// The set of dimension local names from the FIRST fenced code block following
/// `marker` in `md` — the first token of each fenced line that is a `dim*` local
/// name. Panics with a specific message if the marker or its fence is missing,
/// so a doctrine edit that breaks the parse contract reds loudly rather than
/// silently returning an empty (trivially-passing) set.
fn intent_block(md: &str, marker: &str) -> BTreeSet<String> {
    let after_marker = md
        .split_once(marker)
        .unwrap_or_else(|| panic!("SLICE_GUIDE.md is missing the doctrine marker `{marker}`"))
        .1;
    // Open the first fenced code block after the marker, then skip its
    // info-string line to reach the body.
    let (_, after_open) = after_marker
        .split_once("```")
        .unwrap_or_else(|| panic!("no fenced code block follows the marker `{marker}`"));
    let (_, body_and_rest) = after_open
        .split_once('\n')
        .unwrap_or_else(|| panic!("the fence opener after `{marker}` has no body"));
    let (body, _) = body_and_rest
        .split_once("```")
        .unwrap_or_else(|| panic!("the fenced code block after `{marker}` never closes"));

    let mut dims = BTreeSet::new();
    for line in body.lines() {
        if let Some(tok) = line.split_whitespace().next()
            && is_dim_local(tok)
        {
            dims.insert(tok.to_string());
        }
    }
    dims
}

/// The local names of an anchor's intent, read from the Rust twin of
/// `module.ttl`'s `gmeow:maturityRequiresDimension` (which the slice's structural
/// cells hold in lockstep with the TTL).
fn anchor_intent_locals(anchor: MaturityAnchor) -> BTreeSet<String> {
    anchor_table()
        .into_iter()
        .find(|(a, _)| *a == anchor)
        .map(|(_, intent)| intent)
        .unwrap_or_else(|| panic!("anchor_table missing {anchor:?}"))
        .iter()
        .map(|d: &Dimension| d.local_name().to_string())
        .collect()
}

/// Render a set as a stable, readable comma list for assertion messages.
fn show(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// DOCTRINE == VOCABULARY: the FULL / MAXIMAL dimension lists authored in
/// `SLICE_GUIDE.md` § 6.8 equal the minted `gmeow:maturityRequiresDimension`
/// intents for `gmeow:docMaturityFull` / `gmeow:docMaturityMaximal`.
#[test]
fn slice_guide_full_and_maximal_lists_match_the_maturity_vocabulary() {
    let guide = common::repo_root().join("docs/SLICE_GUIDE.md");
    let md = std::fs::read_to_string(&guide)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", guide.display()));

    let doctrine_full = intent_block(&md, FULL_MARKER);
    let doctrine_maximal_extra = intent_block(&md, MAXIMAL_MARKER);

    // Non-vacuity: a parser that silently collapsed to empty would trivially
    // satisfy a subset check; require the blocks to genuinely carry dimensions.
    assert!(
        doctrine_full.len() >= 2 && !doctrine_maximal_extra.is_empty(),
        "the doctrine intent blocks parsed too thin (full = {}, maximal-extra = {}); \
         the parse contract in SLICE_GUIDE.md § 6.8 may have been broken",
        doctrine_full.len(),
        doctrine_maximal_extra.len(),
    );

    let vocab_full = anchor_intent_locals(MaturityAnchor::Full);
    let vocab_maximal = anchor_intent_locals(MaturityAnchor::Maximal);

    // FULL: the authored block must be exactly the FULL intent.
    assert_eq!(
        doctrine_full,
        vocab_full,
        "FULL doctrine ⇄ vocabulary DIVERGENCE.\n  \
         doctrine-only: {}\n  vocabulary-only: {}\n  \
         Reconcile SLICE_GUIDE.md § 6.8 `doctrine-intent:full` with \
         gmeow:docMaturityFull's gmeow:maturityRequiresDimension in \
         slices/core/documentation/module.ttl (and its maturity.rs twin).",
        show(&doctrine_full.difference(&vocab_full).cloned().collect()),
        show(&vocab_full.difference(&doctrine_full).cloned().collect()),
    );

    // MAXIMAL: the doctrine states it as FULL plus the extra block; their union
    // must be exactly the MAXIMAL intent.
    let doctrine_maximal: BTreeSet<String> = doctrine_full
        .union(&doctrine_maximal_extra)
        .cloned()
        .collect();
    assert_eq!(
        doctrine_maximal,
        vocab_maximal,
        "MAXIMAL doctrine ⇄ vocabulary DIVERGENCE.\n  \
         doctrine-only: {}\n  vocabulary-only: {}\n  \
         Reconcile SLICE_GUIDE.md § 6.8 `doctrine-intent:maximal` (FULL plus the extra \
         block) with gmeow:docMaturityMaximal's gmeow:maturityRequiresDimension in \
         slices/core/documentation/module.ttl (and its maturity.rs twin).",
        show(
            &doctrine_maximal
                .difference(&vocab_maximal)
                .cloned()
                .collect()
        ),
        show(
            &vocab_maximal
                .difference(&doctrine_maximal)
                .cloned()
                .collect()
        ),
    );

    // The MAXIMAL-extra block is a PROPER extension of FULL (the doctrine says
    // "everything in FULL, plus …") — no dimension is listed in both, so the
    // author does not silently re-state FULL rows under MAXIMAL.
    assert!(
        doctrine_full.is_disjoint(&doctrine_maximal_extra),
        "the MAXIMAL-extra block re-lists FULL dimensions: {}",
        show(
            &doctrine_full
                .intersection(&doctrine_maximal_extra)
                .cloned()
                .collect()
        ),
    );
}
