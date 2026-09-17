// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The structural near-duplicate (distinctiveness) detector.
//!
//! A slice's per-term annotation coats and its translation `msgstr`s are supposed to
//! *distinguish* one term from another. The prior anti-gaming defenses were blocklists
//! of already-seen template strings, so a NEW template family with different wording
//! sailed through. This module is the general replacement: normalize each value to a
//! **skeleton** and reject a near-duplicate — a value substantively identical to another
//! term's.
//!
//! ## The invariant is structural, never calibrated
//!
//! The threshold is **N = 2**: any two distinct subjects sharing one skeleton is a
//! collision. This is definitional — a collision either is or is not present — never a
//! knob tuned so a score lands on a target. There is no scored axis and no floor here;
//! a collision is a hard boolean reject. (Where this doc-comment or the callers cite
//! false-positive counts over the corpus, those are *verification that the boolean rule
//! does not mis-fire*, not calibration of a threshold to a target.)
//!
//! ## One skeleton: an exact-match over normalized text
//!
//! [`skeleton`] lowercases and collapses whitespace — and deliberately does **not** strip
//! CURIE tokens. In this corpus CURIEs are load-bearing content: a constraint definition
//! names the classes it constrains, and a usage coat names the specific domain/range it
//! applies to (e.g. `math:observationUnit` and `math:statisticalVariable` share the frame
//! "Set it on a math:Sample … with range …" but each names its own distinct range — they
//! are genuinely distinct documentation, not a near-duplicate). Stripping CURIEs would
//! collapse such distinct content into a false collision. So a collision means two
//! subjects carry the *same* normalized text, CURIEs included.
//!
//! ## Two collision shapes
//!
//! - [`collisions`] — a skeleton shared by ≥2 distinct subject keys (coats: distinct TBox
//!   term IRIs sharing a usage-coat or definition skeleton).
//! - [`distinctiveness_violations`] — the translation variant: a `msgstr` skeleton shared
//!   by entries whose **source (`msgid`) skeletons are distinct**. A translation collapsing
//!   a distinction its source made is the violation; two entries whose source is itself the
//!   same (a class + its property twin sharing one English label) legitimately share one
//!   translation and are NOT flagged.

use std::collections::{BTreeMap, BTreeSet};

/// The normalized skeleton of a coat value or translation: lowercase, with runs of
/// whitespace collapsed to a single space and the ends trimmed. CURIEs are kept — they
/// are load-bearing content, so two values that differ only by a CURIE are distinct.
#[must_use]
pub fn skeleton(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// One near-duplicate group: the shared `skeleton` and the identifying `members`
/// (term IRIs, or PO `msgctxt`s) whose values collapse to it. `members` is sorted and
/// deduplicated, so the report is deterministic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collision {
    /// The normalized skeleton shared by every member.
    pub skeleton: String,
    /// The distinct subject keys that collided, sorted.
    pub members: Vec<String>,
}

/// Near-duplicate groups over `(key, skeleton)` pairs: every skeleton shared by ≥2
/// **distinct** keys. An empty/whitespace skeleton is skipped (nothing to distinguish).
/// The same key repeated under one skeleton is one member, not a collision with itself.
/// Deterministic: groups follow `BTreeMap` skeleton order, members `BTreeSet` order.
#[must_use]
pub fn collisions(items: impl IntoIterator<Item = (String, String)>) -> Vec<Collision> {
    let mut by_skeleton: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (key, skeleton) in items {
        if skeleton.trim().is_empty() {
            continue;
        }
        by_skeleton.entry(skeleton).or_default().insert(key);
    }
    by_skeleton
        .into_iter()
        .filter(|(_, keys)| keys.len() >= 2)
        .map(|(skeleton, keys)| Collision {
            skeleton,
            members: keys.into_iter().collect(),
        })
        .collect()
}

/// The translation distinctiveness invariant over `(msgid_skeleton, msgstr_skeleton,
/// key)` triples: a `msgstr` skeleton is a violation only when the entries sharing it
/// carry **≥2 distinct `msgid` skeletons** — i.e. the translation collapsed a
/// distinction its source made. Twin sources (identical `msgid` skeleton → one shared
/// translation) are legitimate and pass. An empty `msgstr` skeleton is skipped.
/// Deterministic (`BTreeMap`/`BTreeSet` order).
#[must_use]
pub fn distinctiveness_violations(
    triples: impl IntoIterator<Item = (String, String, String)>,
) -> Vec<Collision> {
    // msgstr skeleton -> (distinct msgid skeletons, member keys).
    let mut by_target: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for (msgid_skel, msgstr_skel, key) in triples {
        if msgstr_skel.trim().is_empty() {
            continue;
        }
        let entry = by_target.entry(msgstr_skel).or_default();
        entry.0.insert(msgid_skel);
        entry.1.insert(key);
    }
    by_target
        .into_iter()
        .filter(|(_, (sources, _))| sources.len() >= 2)
        .map(|(skeleton, (_, members))| Collision {
            skeleton,
            members: members.into_iter().collect(),
        })
        .collect()
}

#[path = "distinctiveness.tests.rs"]
#[cfg(test)]
mod tests;
