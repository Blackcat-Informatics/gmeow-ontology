// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::collections::HashMap;

// A tiny adjacency graph keyed by &str, so the generic engine is exercised in
// isolation (logic::explain's conformance goldens exercise it in anger).
fn adjacency(
    edges: &[(&'static str, &[&'static str])],
) -> HashMap<&'static str, Vec<&'static str>> {
    edges.iter().map(|(k, v)| (*k, v.to_vec())).collect()
}

#[test]
fn preorder_matches_dfs_current_then_children() {
    let g = adjacency(&[("a", &["b", "c"]), ("b", &["d"]), ("c", &[]), ("d", &[])]);
    let node = walk(
        "a",
        |k: &&str| g.get(*k).map(|_| *k),
        |k: &&str, _p: &&str| g.get(*k).cloned().unwrap_or_default(),
    )
    .unwrap();
    let order: Vec<&str> = node.preorder().iter().map(|n| n.key).collect();
    assert_eq!(order, vec!["a", "b", "d", "c"]);
    assert_eq!(node.depth, 0);
    assert_eq!(node.children[0].depth, 1);
    assert_eq!(node.children[0].children[0].depth, 2);
}

#[test]
fn unresolved_key_is_a_hard_fail() {
    let g = adjacency(&[("a", &["missing"])]);
    let err = walk(
        "a",
        |k: &&str| g.get(*k).map(|_| *k),
        |k: &&str, _p: &&str| g.get(*k).cloned().unwrap_or_default(),
    )
    .unwrap_err();
    assert_eq!(err, DagError::Unresolved("missing"));
}

#[test]
fn cycle_is_a_hard_fail() {
    let g = adjacency(&[("a", &["b"]), ("b", &["a"])]);
    let err = walk(
        "a",
        |k: &&str| g.get(*k).map(|_| *k),
        |k: &&str, _p: &&str| g.get(*k).cloned().unwrap_or_default(),
    )
    .unwrap_err();
    assert_eq!(err, DagError::Cycle("a"));
}

#[test]
fn diamond_is_not_a_cycle_backtracking_frees_the_visited_key() {
    // a -> b -> d, a -> c -> d. `d` is visited twice on DIFFERENT paths; the
    // backtracking pop means that is not a cycle.
    let g = adjacency(&[("a", &["b", "c"]), ("b", &["d"]), ("c", &["d"]), ("d", &[])]);
    let node = walk(
        "a",
        |k: &&str| g.get(*k).map(|_| *k),
        |k: &&str, _p: &&str| g.get(*k).cloned().unwrap_or_default(),
    )
    .unwrap();
    let order: Vec<&str> = node.preorder().iter().map(|n| n.key).collect();
    assert_eq!(order, vec!["a", "b", "d", "c", "d"]);
}
