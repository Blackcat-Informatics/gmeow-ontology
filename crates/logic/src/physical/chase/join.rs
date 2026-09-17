// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Streaming native chase joins. Only the current path and its indexed cursors
//! are retained; no relation-sized table of partial solutions is materialized.

use crate::physical::store::RelationStore;
use crate::rule_ir::{EvalAtom, Fact, Solution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Complete,
    Stopped,
    Exhausted,
}

pub(crate) struct Policy<'a> {
    /// Bound successful partial matches at each atom across the entire traversal.
    /// This bounds a selected budget's exploration without retaining those matches.
    pub(crate) max_matches: usize,
    /// Native term inequalities; apply each as soon as both operands are bound.
    /// A complete solution must still validate that every guard operand is bound.
    pub(crate) distinct: &'a [(String, String)],
    /// Body matches carry their actual source facts; existence probes need bindings.
    pub(crate) retain_sources: bool,
}

/// Visit complete matches in the same nested cursor order as the native join.
/// Returning false stops at a witnessed result, independently of unseen alternatives.
/// Only exhausting every cursor proves absence. Reaching the selected match ceiling
/// with another matching prefix returns Exhausted, never a false negative.
pub(crate) fn walk(
    atoms: &[EvalAtom],
    rel: &RelationStore,
    seed: &Solution,
    policy: Policy<'_>,
    mut visit: impl FnMut(Solution) -> gmeow_errors::Result<bool>,
) -> gmeow_errors::Result<Outcome> {
    if policy.max_matches == 0 {
        return Ok(Outcome::Exhausted);
    }
    let root = Solution {
        bindings: seed.bindings.clone(),
        source_facts: if policy.retain_sources {
            seed.source_facts.clone()
        } else {
            Vec::new()
        },
    };
    if atoms.is_empty() {
        return visit(root).map(|keep_going| {
            if keep_going {
                Outcome::Complete
            } else {
                Outcome::Stopped
            }
        });
    }

    // The explicit stack also keeps large conjunctive heads off the Rust call stack.
    // Each frame owns only one prefix. A cursor borrows the immutable relation,
    // not the prefix used to select its bound range.
    let mut stack = vec![(rel.select_atom(&atoms[0], &root), root)];
    let mut matches = vec![0usize; atoms.len()];
    while !stack.is_empty() {
        let depth = stack.len() - 1;
        let (cursor, base) = stack.last_mut().expect("nonempty join path");
        let Some((s, o, _row, predicate)) = cursor.next() else {
            stack.pop();
            continue;
        };
        let fact = Fact {
            subject: rel.interner().resolve(s).clone(),
            predicate: predicate.to_owned(),
            object: rel.interner().resolve(o).clone(),
        };
        let Some(mut extended) = rel.match_selected(&atoms[depth], &fact, base) else {
            continue;
        };
        if matches[depth] == policy.max_matches {
            return Ok(Outcome::Exhausted);
        }
        matches[depth] += 1;
        if policy.distinct.iter().any(|(left, right)| {
            matches!((extended.get(left), extended.get(right)), (Some(a), Some(b)) if a == b)
        }) {
            continue;
        }
        if policy.retain_sources {
            extended.source_facts.push(fact);
        }
        if depth + 1 == atoms.len() {
            if !visit(extended)? {
                return Ok(Outcome::Stopped);
            }
        } else {
            stack.push((rel.select_atom(&atoms[depth + 1], &extended), extended));
        }
    }
    Ok(Outcome::Complete)
}

#[path = "join.tests.rs"]
#[cfg(test)]
mod tests;
