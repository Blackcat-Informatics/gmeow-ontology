// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The one provenance-DAG walk engine.
//!
//! This is the cycle-guarded, backtracking depth-first reconstruction extracted
//! verbatim (in behaviour) from the logic explanation engine, generalised over
//! the node key and payload. There is exactly ONE such engine in the workspace:
//! `logic::explain` reconstructs derivation trees by calling [`walk`], and the
//! diagnostic ledger walks its witness DAG the same way. Neither re-implements
//! the traversal.
//!
//! The traversal:
//! 1. **resolve** the node (a missing node is [`DagError::Unresolved`]);
//! 2. **cycle-guard** against a visited set threaded through the whole recursion
//!    (a revisited key is [`DagError::Cycle`] — the trace must be a DAG);
//! 3. **descend** into the children the caller supplies (already ordered/deduped
//!    however the domain requires), pushing the current key before and popping it
//!    after, so the visited allocation is `O(depth)`, not `O(depth²)`.
//!
//! The result is a [`DagNode`] tree in which each node precedes its children —
//! flattening it pre-order reproduces the original DFS step order exactly.

use std::fmt;

/// A hard failure during a DAG walk. Both conditions are no-optionality hard
/// fails: there is no silent skip of an unresolved node and no tolerance of a
/// cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagError<K> {
    /// A revisited key: the walked structure is not a DAG.
    Cycle(K),
    /// A referenced key could not be resolved to a node.
    Unresolved(K),
}

impl<K: fmt::Display> fmt::Display for DagError<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DagError::Cycle(k) => write!(f, "cycle detected in DAG at `{k}`"),
            DagError::Unresolved(k) => write!(f, "unresolved DAG node `{k}`"),
        }
    }
}

impl<K: fmt::Display + fmt::Debug> std::error::Error for DagError<K> {}

/// One node of a reconstructed DAG tree: its key, the resolved payload, its depth
/// (root = 0), and its child subtrees in the order the caller yielded them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DagNode<K, P> {
    pub key: K,
    pub payload: P,
    pub depth: u32,
    pub children: Vec<DagNode<K, P>>,
}

impl<K, P> DagNode<K, P> {
    /// Pre-order (DFS) references: this node first, then each child subtree — the
    /// exact order the original step reconstruction produced.
    pub fn preorder(&self) -> Vec<&DagNode<K, P>> {
        let mut out = Vec::new();
        self.collect_preorder(&mut out);
        out
    }

    fn collect_preorder<'a>(&'a self, out: &mut Vec<&'a DagNode<K, P>>) {
        out.push(self);
        for child in &self.children {
            child.collect_preorder(out);
        }
    }
}

/// Reconstruct the DAG rooted at `root`.
///
/// * `resolve(&key) -> Option<payload>` resolves a key to its payload; `None` is
///   a hard [`DagError::Unresolved`].
/// * `children(&key, &payload) -> Vec<key>` yields the child keys **already in the
///   order and with the deduplication the domain requires** (the engine does not
///   sort or dedup — that is a domain decision, kept at the call site so the
///   golden-pinned ordering stays owned by the caller).
///
/// # Errors
/// [`DagError::Unresolved`] if a key cannot be resolved; [`DagError::Cycle`] if a
/// key is revisited on the current path.
pub fn walk<K, P, R, C>(root: K, resolve: R, children: C) -> Result<DagNode<K, P>, DagError<K>>
where
    K: Clone + Eq,
    R: Fn(&K) -> Option<P>,
    C: Fn(&K, &P) -> Vec<K>,
{
    let mut visited: Vec<K> = Vec::new();
    walk_inner(root, &resolve, &children, 0, &mut visited)
}

fn walk_inner<K, P, R, C>(
    key: K,
    resolve: &R,
    children: &C,
    depth: u32,
    visited: &mut Vec<K>,
) -> Result<DagNode<K, P>, DagError<K>>
where
    K: Clone + Eq,
    R: Fn(&K) -> Option<P>,
    C: Fn(&K, &P) -> Vec<K>,
{
    // Resolve first (an unresolved key is a hard fail), then cycle-check — the
    // same order the derivation reconstruction used.
    let payload = match resolve(&key) {
        Some(p) => p,
        None => return Err(DagError::Unresolved(key)),
    };
    if visited.contains(&key) {
        return Err(DagError::Cycle(key));
    }

    let child_keys = children(&key, &payload);

    // Push before descending, pop after — backtracking visited set, O(depth).
    visited.push(key.clone());
    let mut child_nodes = Vec::with_capacity(child_keys.len());
    for child_key in child_keys {
        child_nodes.push(walk_inner(
            child_key,
            resolve,
            children,
            depth + 1,
            visited,
        )?);
    }
    visited.pop();

    Ok(DagNode {
        key,
        payload,
        depth,
        children: child_nodes,
    })
}

#[cfg(test)]
mod tests {
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
}
