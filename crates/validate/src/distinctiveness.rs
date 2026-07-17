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
pub fn collisions(items: &[(String, String)]) -> Vec<Collision> {
    let mut by_skeleton: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (key, skeleton) in items {
        if skeleton.trim().is_empty() {
            continue;
        }
        by_skeleton
            .entry(skeleton.clone())
            .or_default()
            .insert(key.clone());
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
pub fn distinctiveness_violations(triples: &[(String, String, String)]) -> Vec<Collision> {
    // msgstr skeleton -> (distinct msgid skeletons, member keys).
    let mut by_target: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for (msgid_skel, msgstr_skel, key) in triples {
        if msgstr_skel.trim().is_empty() {
            continue;
        }
        let entry = by_target.entry(msgstr_skel.clone()).or_default();
        entry.0.insert(msgid_skel.clone());
        entry.1.insert(key.clone());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skeleton_lowercases_and_collapses_whitespace() {
        assert_eq!(
            skeleton("Assert  in the\tnatural   direction. "),
            "assert in the natural direction."
        );
    }

    #[test]
    fn skeleton_keeps_curies_so_distinct_ranges_stay_distinct() {
        // Two usage coats sharing a frame but naming their own distinct range are NOT
        // near-duplicates — the load-bearing CURIE is kept, so they do not collide.
        let a = skeleton("Set it on a math:Sample with range math:ObservationUnit.");
        let b = skeleton("Set it on a math:Sample with range math:StatisticalVariable.");
        assert_ne!(a, b, "distinct ranges must stay distinct: {a:?} vs {b:?}");
        // A byte-identical coat (modulo case/space) DOES collide.
        let c = skeleton("Assert in the natural direction and read as its inverse.");
        let d = skeleton("assert in the natural direction and read as its inverse. ");
        assert_eq!(c, d);
    }

    #[test]
    fn collisions_flags_n2_and_ignores_singletons_and_empty() {
        let items = vec![
            ("ex:A".to_owned(), skeleton("Avoid a partial quaternion.")),
            ("ex:B".to_owned(), skeleton("Avoid a partial quaternion.")),
            ("ex:C".to_owned(), skeleton("Set exactly one geocode.")),
            ("ex:D".to_owned(), "   ".to_owned()), // empty skeleton — skipped
            ("ex:E".to_owned(), String::new()),    // empty — skipped
        ];
        let got = collisions(&items);
        assert_eq!(got.len(), 1, "one N=2 group: {got:#?}");
        assert_eq!(got[0].skeleton, "avoid a partial quaternion.");
        assert_eq!(got[0].members, vec!["ex:A".to_owned(), "ex:B".to_owned()]);
    }

    #[test]
    fn collisions_same_key_twice_is_not_a_collision() {
        // One term carrying the same value twice is not a cross-term near-duplicate.
        let items = vec![
            ("ex:A".to_owned(), skeleton("same text.")),
            ("ex:A".to_owned(), skeleton("same text.")),
        ];
        assert!(collisions(&items).is_empty());
    }

    #[test]
    fn distinctiveness_passes_twins_flags_collapsed_distinction() {
        // Two twin sources (identical msgid skeleton) sharing one translation → PASS.
        let twins = vec![
            (
                skeleton("p-value"),
                skeleton("p值"),
                "math:PValue|rdfs:label".to_owned(),
            ),
            (
                skeleton("p-value"),
                skeleton("p值"),
                "math:pValue|rdfs:label".to_owned(),
            ),
        ];
        assert!(
            distinctiveness_violations(&twins).is_empty(),
            "identical source → shared translation is legitimate"
        );
        // Two DISTINCT sources collapsed to one translation → FLAG.
        let collapsed = vec![
            (
                skeleton("read"),
                skeleton("lire"),
                "rights:read|rdfs:label".to_owned(),
            ),
            (
                skeleton("play"),
                skeleton("lire"),
                "rights:play|rdfs:label".to_owned(),
            ),
        ];
        let got = distinctiveness_violations(&collapsed);
        assert_eq!(got.len(), 1, "the collapsed distinction reds: {got:#?}");
        assert_eq!(got[0].skeleton, "lire");
        assert_eq!(
            got[0].members,
            vec![
                "rights:play|rdfs:label".to_owned(),
                "rights:read|rdfs:label".to_owned()
            ]
        );
    }

    #[test]
    fn distinctiveness_skips_empty_target() {
        let empties = vec![
            (skeleton("read"), String::new(), "a".to_owned()),
            (skeleton("play"), "   ".to_owned(), "b".to_owned()),
        ];
        assert!(distinctiveness_violations(&empties).is_empty());
    }
}
