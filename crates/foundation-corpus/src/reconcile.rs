// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `reconcile_nq` + `NQ_PREDICATE_STATUS` — the no-silent-drop coverage table.
//!
//! Every predicate in the corpus `.nq` form maps to a status string, proving
//! nothing was silently dropped (mapped / deliberately-dropped-with-reason /
//! improved). A faithful port; the `NQ_PREDICATE_STATUS` entries are verbatim.

use std::collections::HashMap;
use std::path::Path;

/// The `.nq` form's predicates, each accounted for (the no-silent-drop table).
///
/// Order matches the Python dict literal so any future ordered consumer is
/// faithful; lookups are unordered.
pub const NQ_PREDICATE_STATUS: [(&str, &str); 16] = [
    (
        "http://lillith.internal/principia/active_character",
        "MAPPED → flat gmeow:narrates (#360)",
    ),
    (
        "http://lillith.internal/principia/key_event",
        "MAPPED → gmeow:Event + flat gmeow:narrates (#360)",
    ),
    (
        "http://lillith.internal/principia/goal_score",
        "IMPROVED → gmeow:Assessment with vantage/rubric/criterion (#353)",
    ),
    (
        "http://lillith.internal/principia/thematic_tag",
        "DROPPED-WITH-REASON → unpromoted; #363 heuristic needs curator confirmation (budget-reported)",
    ),
    (
        "http://lillith.internal/principia/emotional_state",
        "IMPROVED → gmeow:ArcSample with vantage + frame-carried position (#361)",
    ),
    (
        "http://lillith.internal/principia/arc_position",
        "IMPROVED → gmeow:NarrativePosition in a discourse frame (#359)",
    ),
    (
        "http://lillith.internal/principia/content_mode",
        "DEFERRED → statement-layer emission mode (compiler-arc window)",
    ),
    (
        "http://lillith.internal/principia/chapter_index",
        "IMPROVED → gmeow:positionOrdinal on a frame-carried position (#359)",
    ),
    (
        "http://lillith.internal/principia/predicate/exemplifies",
        "IMPROVED → gmeow:Exemplar with exemplarSubject + polarity + anchor (#353/#362)",
    ),
    (
        "http://lillith.internal/principia/predicate/rationale",
        "MAPPED → gmeow:exemplarRationale",
    ),
    (
        "http://lillith.internal/principia/predicate/relationship",
        "DEFERRED → relator extraction from prose blobs (source-data deficiency; statement-layer mode)",
    ),
    (
        "http://lillith.internal/principia/motivation",
        "DEFERRED → #350 Goal extraction from prose (statement-layer mode)",
    ),
    (
        "http://lillith.internal/principia/emotional_arc",
        "IMPROVED → sampled trajectory (whole-arc prose retained at CharacterArc level in the statement-layer mode)",
    ),
    (
        "http://lillith.internal/principia/predicate/character_role",
        "IMPROVED → gmeow:RoleInNarrative (scoped, interpretive) (#362)",
    ),
    (
        "http://lillith.internal/principia/penalty_boundary",
        "DEFERRED → anti-score-anchor import with the principia importer (EPIC #348 consumer)",
    ),
    (
        "http://lillith.internal/principia/predicate/paradigm_assignment",
        "DEFERRED → #355 persona import with the principia importer",
    ),
];

/// Coverage table: every predicate in the `.nq` form mapped to its status.
///
/// Mirrors the Python `reconcile_nq`: count predicates (the second whitespace
/// token, stripped of `<>`), order by `Counter.most_common()` (descending count,
/// then first-seen insertion order for ties), and render `  pred (count): status`.
pub fn reconcile_nq(nq_path: &Path, mapped: &[(&str, &str)]) -> std::io::Result<String> {
    use std::io::BufRead;
    // Stream the `.nq` line-by-line rather than loading the whole file.
    let reader = std::io::BufReader::new(std::fs::File::open(nq_path)?);
    // Insertion-ordered counter: track first-seen order to break count ties the
    // way Python's Counter.most_common does.
    let mut counts: Vec<(String, u64)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for line in reader.lines() {
        let line = line?;
        // Python: line.split(maxsplit=3); parts[1] must start with '<'.
        let toks = split_max(&line, 3);
        if toks.len() > 2 && toks[1].starts_with('<') {
            // Borrow for the lookup; only allocate on first-seen insertion.
            let pred = toks[1].trim_matches(|c| c == '<' || c == '>');
            match index.get(pred) {
                Some(&i) => counts[i].1 += 1,
                None => {
                    let owned = pred.to_string();
                    index.insert(owned.clone(), counts.len());
                    counts.push((owned, 1));
                }
            }
        }
    }
    // most_common: stable sort by descending count (ties keep insertion order).
    let mut ordered: Vec<(usize, &(String, u64))> = counts.iter().enumerate().collect();
    ordered.sort_by(|a, b| b.1 .1.cmp(&a.1 .1).then(a.0.cmp(&b.0)));

    let lookup: HashMap<&str, &str> = mapped.iter().copied().collect();
    let mut lines = vec!["NQ RECONCILIATION (predicate → status)".to_string()];
    for (_, (predicate, count)) in ordered {
        let status = lookup
            .get(predicate.as_str())
            .copied()
            .unwrap_or("UNREVIEWED");
        lines.push(format!("  {predicate} ({count}): {status}"));
    }
    Ok(lines.join("\n"))
}

/// Python `str.split(maxsplit=n)`: split on runs of whitespace, dropping leading
/// whitespace, into at most `n + 1` fields (the remainder kept intact).
fn split_max(line: &str, maxsplit: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = line.trim_start();
    while out.len() < maxsplit {
        match rest.find(char::is_whitespace) {
            Some(pos) => {
                out.push(rest[..pos].to_string());
                rest = rest[pos..].trim_start();
                if rest.is_empty() {
                    return out;
                }
            }
            None => break,
        }
    }
    if !rest.is_empty() {
        out.push(rest.to_string());
    }
    out
}
