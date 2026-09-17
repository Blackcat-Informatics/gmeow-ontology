// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::physical::store::{Bound, RelationStore};
use purrdf::TermValue;

fn term(iri: &str) -> TermValue {
    TermValue::iri(iri)
}

/// Drain a cursor into a `Vec` — a `#[cfg(test)]`-only convenience for asserting a
/// cursor's full row set (the production hot path never collects).
fn drain(mut c: RowCursor<'_>) -> Vec<(TermId, TermId, RowId)> {
    let mut out = Vec::new();
    while let Some(row) = c.next() {
        out.push(row);
    }
    out
}

fn drain_values<const COLUMN: u8>(mut c: ValueCursor<'_, COLUMN>) -> Vec<(TermId, RowId)> {
    let mut out = Vec::new();
    while let Some(row) = c.next() {
        out.push(row);
    }
    out
}

/// Resolve selected id rows to `(subject, object)` display-surface pairs, as an
/// ORDER-INDEPENDENT set (the cursor enumerates batch-then-tail, not sorted, so tests
/// compare sets — winner selection, not cursor order, fixes output).
fn resolved_set(
    s: &RelationStore,
    rows: &[(TermId, TermId, RowId)],
) -> std::collections::BTreeSet<(String, String)> {
    rows.iter()
        .map(|&(si, oi, _)| {
            (
                format!("{:?}", s.interner().resolve(si)),
                format!("{:?}", s.interner().resolve(oi)),
            )
        })
        .collect()
}

fn pair(sub: &str, obj: &str) -> (String, String) {
    (format!("{:?}", term(sub)), format!("{:?}", term(obj)))
}

/// A store large enough to force several batch seals (past `TAIL_SEAL_THRESHOLD`),
/// so the galloping batch runs — not just the tail leg — are exercised.  `p` holds
/// `(a, o_i)` for many objects plus a second subject `z` with one edge.
fn big_store() -> RelationStore {
    let mut s = RelationStore::new();
    for i in 0..200 {
        assert!(
            s.insert(
                "http://ex/p",
                &term("http://ex/a"),
                &term(&format!("http://ex/o{i:03}")),
            )
            .is_some()
        );
    }
    assert!(
        s.insert("http://ex/p", &term("http://ex/z"), &term("http://ex/o000"))
            .is_some()
    );
    s
}

#[test]
fn cursor_any_yields_every_row_as_a_set() {
    let s = big_store();
    let got = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Any)));
    assert_eq!(got.len(), 201, "200 a-edges + 1 z-edge, deduped");
    assert!(got.contains(&pair("http://ex/a", "http://ex/o000")));
    assert!(got.contains(&pair("http://ex/z", "http://ex/o000")));
    assert!(got.contains(&pair("http://ex/a", "http://ex/o199")));
}

#[test]
fn cursor_subject_bound_gallops_batches() {
    let s = big_store();
    let a = s.iri_id("http://ex/a").expect("a interned");
    let got = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Subject(a))));
    assert_eq!(got.len(), 200, "exactly a's 200 edges");
    assert!(
        got.iter()
            .all(|(sub, _)| *sub == pair("http://ex/a", "x").0)
    );

    let z = s.iri_id("http://ex/z").expect("z interned");
    let zrows = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Subject(z))));
    assert_eq!(zrows, [pair("http://ex/z", "http://ex/o000")].into());
}

#[test]
fn cursor_object_bound_uses_lazy_permutation() {
    let s = big_store();
    let o0 = s.iri_id("http://ex/o000").expect("o000 interned");
    // o000 is the object of BOTH a and z.
    let got = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Object(o0))));
    assert_eq!(
        got,
        [
            pair("http://ex/a", "http://ex/o000"),
            pair("http://ex/z", "http://ex/o000"),
        ]
        .into()
    );
    // A distinct object appears once.
    let o5 = s.iri_id("http://ex/o005").expect("o005 interned");
    let g5 = resolved_set(&s, &drain(s.select("http://ex/p", Bound::Object(o5))));
    assert_eq!(g5, [pair("http://ex/a", "http://ex/o005")].into());
}

#[test]
fn cursor_both_bound_is_unique() {
    let s = big_store();
    let a = s.iri_id("http://ex/a").expect("a interned");
    let o7 = s.iri_id("http://ex/o007").expect("o007 interned");
    assert_eq!(drain(s.select("http://ex/p", Bound::Both(a, o7))).len(), 1);
    // A subject/object that never co-occur ⇒ empty.
    let z = s.iri_id("http://ex/z").expect("z interned");
    let o7b = s.iri_id("http://ex/o007").expect("o007 interned");
    assert!(
        drain(s.select("http://ex/p", Bound::Both(z, o7b))).is_empty(),
        "z only links o000, never o007"
    );
}

#[test]
fn cursor_tail_only_small_relation() {
    // A relation below the seal threshold is a pure tail (no batches) — the
    // allocation-light regime — and still selects correctly on every bound.
    let mut s = RelationStore::new();
    for (sub, obj) in [("a", "b"), ("a", "c"), ("b", "c")] {
        assert!(
            s.insert(
                "http://ex/k",
                &term(&format!("http://ex/{sub}")),
                &term(&format!("http://ex/{obj}")),
            )
            .is_some()
        );
    }
    let a = s.iri_id("http://ex/a").expect("a interned");
    let got = resolved_set(&s, &drain(s.select("http://ex/k", Bound::Subject(a))));
    assert_eq!(
        got,
        [
            pair("http://ex/a", "http://ex/b"),
            pair("http://ex/a", "http://ex/c"),
        ]
        .into()
    );
    assert!(s.contains(
        "http://ex/k",
        &TermValue::iri("http://ex/b"),
        &TermValue::iri("http://ex/c")
    ));
    assert!(!s.contains(
        "http://ex/k",
        &TermValue::iri("http://ex/a"),
        &TermValue::iri("http://ex/z")
    ));
}

#[test]
fn cursor_any_remaining_probes_without_collecting() {
    let s = big_store();
    let a = s.iri_id("http://ex/a").expect("a interned");
    assert!(s.select("http://ex/p", Bound::Subject(a)).any_remaining());
    let missing = s.iri_id("http://ex/a").expect("a interned");
    let none_obj = s.iri_id("http://ex/o000").expect("o000 interned");
    // (a, o000) exists.
    assert!(
        s.select("http://ex/p", Bound::Both(missing, none_obj))
            .any_remaining()
    );
}

/// The trie cursor globally merges several immutable batches plus the tail in
/// either orientation, and fixing the opposite column narrows the sorted stream.
#[test]
fn value_cursor_is_globally_sorted_in_both_orientations() {
    let s = big_store();

    let subject_rows = drain_values(s.values_subject("http://ex/p", None));
    assert_eq!(subject_rows.len(), 201);
    assert!(subject_rows.windows(2).all(|rows| rows[0].0 <= rows[1].0));

    let object_rows = drain_values(s.values_object("http://ex/p", None));
    assert_eq!(object_rows.len(), 201);
    assert!(object_rows.windows(2).all(|rows| rows[0].0 <= rows[1].0));

    let o0 = s.iri_id("http://ex/o000").expect("o000 interned");
    let subjects_at_o0 = drain_values(s.values_subject("http://ex/p", Some(o0)));
    assert_eq!(subjects_at_o0.len(), 2, "a and z point at o000");
    assert!(subjects_at_o0.windows(2).all(|rows| rows[0].0 <= rows[1].0));

    let a = s.iri_id("http://ex/a").expect("a interned");
    let objects_at_a = drain_values(s.values_object("http://ex/p", Some(a)));
    assert_eq!(objects_at_a.len(), 200);
    assert!(objects_at_a.windows(2).all(|rows| rows[0].0 <= rows[1].0));
}

/// Seek applies to every sorted run before the k-way merge, so no value below the
/// requested trie frontier can reappear from an older batch or the mutable tail.
#[test]
fn value_cursor_seek_advances_all_runs() {
    let s = big_store();
    let target = s.iri_id("http://ex/o150").expect("o150 interned");
    let mut cursor = s.values_object("http://ex/p", None);
    cursor.seek(target);
    let rows = drain_values(cursor);
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|&(value, _)| value >= target));
    assert_eq!(rows[0].0, target);
}

#[test]
fn value_cursor_frontier_removes_exhausted_batch_runs() {
    let s = big_store();
    let mut cursor = s.values_object("http://ex/p", None);
    let source_count = cursor.sources.len();
    assert!(source_count > 1, "fixture must span multiple sorted runs");

    let mut previous = None;
    let mut count = 0;
    while let Some((value, _row)) = cursor.next() {
        if let Some(previous) = previous {
            assert!(previous <= value);
        }
        previous = Some(value);
        count += 1;
        assert!(cursor.frontier.len() <= source_count);
    }

    assert_eq!(count, 201);
    assert!(cursor.frontier.is_empty());
    assert!(cursor.sources.iter().all(|source| source.peek().is_none()));
}
