// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The structural near-duplicate (distinctiveness) detector.
//!
//! A slice's per-term annotation coats and its translation `msgstr`s are supposed to
//! *distinguish* one term from another. The prior anti-gaming defenses were blocklists
//! of already-seen template strings, so a NEW template family with different wording
//! sailed through. This module is the general replacement: normalize each value to a
//! **skeleton** and reject a near-duplicate — a value cosmetically dressed up but
//! substantively identical to another term's.
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
//! ## Two skeleton normalizers, chosen by what carries the meaning
//!
//! - [`coat_skeleton`] / [`translation_skeleton`] **strip CURIE tokens** before
//!   lowercasing and collapsing whitespace. Usage-coat prose and translations reference
//!   terms via incidental CURIEs; stripping them defeats the "swap one CURIE to disguise
//!   a template" dodge.
//! - [`definition_skeleton`] does **not** strip CURIEs — a definition's CURIEs are
//!   load-bearing content (the class names it constrains), so it is an exact-match over
//!   the un-stripped, lowercased, whitespace-collapsed text.
//!
//! ## Two collision shapes
//!
//! - [`collisions`] — a skeleton shared by ≥2 distinct subject keys (coats: distinct
//!   term IRIs sharing a usage-coat or definition skeleton).
//! - [`distinctiveness_violations`] — the translation variant: a `msgstr` skeleton
//!   shared by entries whose **source (`msgid`) skeletons are distinct**. A translation
//!   collapsing a distinction its source made is the violation; two entries whose source
//!   is itself the same (a class + its property twin sharing one English label) legitimately
//!   share one translation and are NOT flagged.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use std::sync::LazyLock;

/// A turtle CURIE token: a prefix name, a `:`, then a local name (`prefix:local`).
///
/// The local part requires a name char immediately after the `:`, so an IRI scheme
/// (`http://…`, where `:` is followed by `/`) and a bare prose colon (`"note: text"`,
/// where `:` is followed by a space) do NOT match — only a real CURIE is stripped.
static CURIE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z][A-Za-z0-9_-]*:[A-Za-z0-9_-]+").expect("valid CURIE regex")
});

/// Collapse runs of ASCII/Unicode whitespace to a single space and trim the ends.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lowercase + whitespace-collapse, with CURIE tokens removed first.
fn strip_curie_skeleton(s: &str) -> String {
    let stripped = CURIE.replace_all(s, " ");
    collapse_ws(&stripped.to_lowercase())
}

/// The skeleton of a usage coat (`gmeow:useWhen`/`avoidWhen`/`howToUse`): strip CURIE
/// tokens, lowercase, collapse whitespace. CURIEs in usage prose are incidental term
/// references; stripping them means a template disguised only by a swapped CURIE still
/// collides.
#[must_use]
pub fn coat_skeleton(s: &str) -> String {
    strip_curie_skeleton(s)
}

/// The skeleton of a translation (`msgstr`/`msgid`): identical normalization to
/// [`coat_skeleton`] (strip CURIEs, lowercase, collapse) so a source and its target are
/// compared under the same rule.
#[must_use]
pub fn translation_skeleton(s: &str) -> String {
    strip_curie_skeleton(s)
}

/// The skeleton of a `skos:definition`: lowercase + whitespace-collapse only, with
/// **no** CURIE stripping. A definition's CURIEs are load-bearing (the classes it
/// constrains), so this is an exact-match over the un-stripped text — it catches a
/// genuine cross-term duplicate definition with no false positives from shared CURIEs.
#[must_use]
pub fn definition_skeleton(s: &str) -> String {
    collapse_ws(&s.to_lowercase())
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
    fn coat_skeleton_strips_curies_lowercases_collapses() {
        // A CURIE reference is removed; case and whitespace are normalized.
        assert_eq!(
            coat_skeleton("Avoid  euler angles without   gmeow:eulerOrder."),
            "avoid euler angles without ."
        );
        // Two usage coats that differ ONLY by a swapped CURIE collapse to one skeleton.
        let a = coat_skeleton("Set it on a math:PValue with range xsd:double.");
        let b = coat_skeleton("Set it on a math:effectSize with range xsd:double.");
        assert_eq!(a, b, "CURIE-only difference must collide: {a:?} vs {b:?}");
    }

    #[test]
    fn scheme_iri_and_prose_colon_are_not_stripped() {
        // A full-IRI scheme (`:` before `/`) and a prose colon (`:` before space) are
        // not CURIEs and survive normalization.
        assert!(
            coat_skeleton("see http://example.org/x for detail").contains("http://example.org/x")
        );
        assert!(coat_skeleton("note: read this").contains("note: read this"));
    }

    #[test]
    fn definition_skeleton_keeps_curies() {
        // Load-bearing CURIEs distinguish two constraint definitions and must survive.
        let a = definition_skeleton(
            "A closed-world integrity constraint: a gmeow:Foo declares a gmeow:Bar.",
        );
        let b = definition_skeleton(
            "A closed-world integrity constraint: a gmeow:Baz declares a gmeow:Qux.",
        );
        assert_ne!(a, b, "distinct CURIEs must keep definitions distinct");
        // But a byte-identical definition (modulo case/space) collides.
        let c = definition_skeleton(
            "Whether an honorific is rendered before (prefix) or after (suffix) the name.",
        );
        let d = definition_skeleton(
            "Whether an honorific is rendered  before (prefix) or after (suffix) the name. ",
        );
        assert_eq!(c, d);
    }

    #[test]
    fn collisions_flags_n2_and_ignores_singletons_and_empty() {
        let items = vec![
            ("ex:A".to_owned(), "avoid a partial quaternion.".to_owned()),
            ("ex:B".to_owned(), "avoid a partial quaternion.".to_owned()),
            ("ex:C".to_owned(), "set exactly one geocode.".to_owned()),
            ("ex:D".to_owned(), "   ".to_owned()), // empty skeleton — skipped
            ("ex:E".to_owned(), "".to_owned()),    // empty — skipped
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
            ("ex:A".to_owned(), "same text.".to_owned()),
            ("ex:A".to_owned(), "same text.".to_owned()),
        ];
        assert!(collisions(&items).is_empty());
    }

    #[test]
    fn distinctiveness_passes_twins_flags_collapsed_distinction() {
        // Two twin sources (identical msgid skeleton) sharing one translation → PASS.
        let twins = vec![
            (
                "p-value".to_owned(),
                "p值".to_owned(),
                "math:PValue|rdfs:label".to_owned(),
            ),
            (
                "p-value".to_owned(),
                "p值".to_owned(),
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
                "read".to_owned(),
                "lire".to_owned(),
                "rights:read|rdfs:label".to_owned(),
            ),
            (
                "play".to_owned(),
                "lire".to_owned(),
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
            ("read".to_owned(), "".to_owned(), "a".to_owned()),
            ("play".to_owned(), "   ".to_owned(), "b".to_owned()),
        ];
        assert!(distinctiveness_violations(&empties).is_empty());
    }
}
