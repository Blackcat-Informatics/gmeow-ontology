// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Iterative resource admission before a source DAG becomes the shared boxed IR.
//! This checks source size without a second formula representation or parser.

use std::collections::{HashMap, HashSet};

use gmeow_errors::Diag;
use purrdf::{DatasetView, TermRef, TermValue};

use super::{FORMULA_SUBLINKS, FormulaSource, Subject, logic_iri, subject_str};
use crate::graphutil::subject_id;

/// Named operational source envelope checked before constructing the shared IR.
pub const FORMULA_SOURCE_ADMISSION_FRAGMENT: &str =
    "https://blackcatinformatics.ca/logic/BoundedFormulaSourceFragment";

/// Maximum source-carrier depth admitted before recursive IR construction.
/// Formula links, argument carriers, and function applications all count. This
/// is an operational envelope, independent of semantic or evaluation budgets.
pub const MAX_FORMULA_SOURCE_DEPTH: usize = 128;

/// Maximum expanded source-carrier occurrences for one formula. A shared child
/// counts once per incoming path, so a small exponential DAG cannot allocate an
/// unbounded boxed tree. Standard translation adds only bounded local structure.
pub const MAX_FORMULA_SOURCE_EXPANSION: usize = 65_536;

#[derive(Clone, Copy)]
struct Size {
    occurrences: usize,
    depth: usize,
}

struct Pending<Id> {
    node: Id,
    children: Vec<Id>,
    next: usize,
    size: Size,
}

fn exhausted(root: &Subject, dimension: &str, limit: usize) -> Diag {
    Diag::of_kind(crate::error::FormulaAdmission {
        detail: format!(
            "{FORMULA_SOURCE_ADMISSION_FRAGMENT} exceeded {dimension} limit {limit}; no formula IR or semantic verdict was produced"
        ),
    })
    .with_focus(subject_str(root))
}

fn pending<S: FormulaSource>(
    source: &S,
    node: <S::Dataset as DatasetView>::Id,
    predicates: &[<S::Dataset as DatasetView>::Id],
    root: &Subject,
) -> gmeow_errors::Result<Pending<<S::Dataset as DatasetView>::Id>> {
    let dataset = source.dataset();
    let mut children = Vec::new();
    for &predicate in predicates {
        for quad in crate::graphutil::source_graph_pattern(
            dataset,
            Some(node),
            Some(predicate),
            None,
            source.source_graph(),
        ) {
            if matches!(
                dataset.resolve(quad.o),
                TermRef::Iri(_) | TermRef::Blank { .. }
            ) {
                children.push(quad.o);
                if children.len() >= MAX_FORMULA_SOURCE_EXPANSION {
                    return Err(exhausted(
                        root,
                        "expanded occurrences",
                        MAX_FORMULA_SOURCE_EXPANSION,
                    ));
                }
            }
        }
    }
    Ok(Pending {
        node,
        children,
        next: 0,
        size: Size {
            occurrences: 1,
            depth: 1,
        },
    })
}

pub(super) fn admit(source: &impl FormulaSource, root: &Subject) -> gmeow_errors::Result<()> {
    let dataset = source.dataset();
    let Some(root_id) = subject_id(dataset, root) else {
        // The strict parser owns missing-node and malformed-constructor errors.
        return Ok(());
    };
    let predicates = FORMULA_SUBLINKS
        .iter()
        .copied()
        .chain(["argument", "quantifiedVariable", "termApplication"])
        .filter_map(|local| dataset.term_id_by_value(&TermValue::Iri(logic_iri(local))))
        .collect::<Vec<_>>();
    let mut sizes: HashMap<_, Size> = HashMap::new();
    let mut active = HashSet::from([root_id]);
    let mut stack = vec![pending(source, root_id, &predicates, root)?];
    while let Some(current) = stack.last_mut() {
        if let Some(&child) = current.children.get(current.next) {
            if active.contains(&child) {
                // The strict parser stops at this back edge with its existing
                // source-focused cycle diagnostic. Still measure every other
                // branch: returning early could admit an exponential sibling.
                current.next += 1;
                continue;
            }
            if let Some(size) = sizes.get(&child) {
                current.next += 1;
                current.size.occurrences =
                    current.size.occurrences.saturating_add(size.occurrences);
                current.size.depth = current.size.depth.max(size.depth + 1);
                if current.size.occurrences > MAX_FORMULA_SOURCE_EXPANSION {
                    return Err(exhausted(
                        root,
                        "expanded occurrences",
                        MAX_FORMULA_SOURCE_EXPANSION,
                    ));
                }
                if current.size.depth > MAX_FORMULA_SOURCE_DEPTH {
                    return Err(exhausted(root, "depth", MAX_FORMULA_SOURCE_DEPTH));
                }
            } else {
                if stack.len() >= MAX_FORMULA_SOURCE_DEPTH {
                    return Err(exhausted(root, "depth", MAX_FORMULA_SOURCE_DEPTH));
                }
                active.insert(child);
                stack.push(pending(source, child, &predicates, root)?);
            }
        } else {
            let done = stack.pop().expect("nonempty admission stack");
            active.remove(&done.node);
            sizes.insert(done.node, done.size);
        }
    }
    Ok(())
}

#[path = "admission.tests.rs"]
#[cfg(test)]
mod tests;
