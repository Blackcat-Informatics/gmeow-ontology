// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete, world-local reduce groups over native join solutions. Membership is
//! a set of full substitutions; equal values from distinct members are retained.

use std::collections::{BTreeMap, btree_map::Entry};

use purrdf::{TermValue, sparql::fold_values};

use super::{Solution, seminaive_err};
use crate::rule_ir::{Fact, FactKey, Reduction};

#[derive(Default)]
struct Group {
    values: Vec<TermValue>,
    sources: BTreeMap<FactKey, Fact>,
}

/// Reduce each complete group through PurRDF's existing native accumulator.
pub(super) fn evaluate(
    rule: &str,
    reduction: &Reduction,
    solutions: Vec<Solution>,
) -> gmeow_errors::Result<Vec<Solution>> {
    let mut members = BTreeMap::new();
    for mut solution in solutions {
        solution.bindings.sort_by(|a, b| a.0.cmp(&b.0));
        // Multiple semantic spellings/proofs of one substitution do not multiply
        // its membership. Select the lexically least concrete witness, independently
        // of join or worker order, while preserving that witness's actual RDF facts.
        match members.entry(solution.bindings.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(solution);
            }
            Entry::Occupied(mut entry) => {
                if solution
                    .source_facts
                    .iter()
                    .map(Fact::key)
                    .cmp(entry.get().source_facts.iter().map(Fact::key))
                    .is_lt()
                {
                    entry.insert(solution);
                }
            }
        }
    }
    let mut groups = BTreeMap::<Vec<TermValue>, Group>::new();
    if reduction.groups.is_empty() {
        groups.insert(Vec::new(), Group::default());
    }
    for solution in members.into_values() {
        let required = |name: &str| {
            solution.get(name).cloned().ok_or_else(|| {
                seminaive_err(format!(
                    "aggregation in rule <{rule}> has unbound input {name}",
                ))
            })
        };
        let key = reduction
            .groups
            .iter()
            .map(|name| required(name))
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        let value = required(&reduction.input)?;
        let group = groups.entry(key).or_default();
        group.values.push(value);
        for fact in solution.source_facts {
            group.sources.entry(fact.key()).or_insert(fact);
        }
    }
    groups
        .into_iter()
        .map(|(key, group)| {
            let result = fold_values(reduction.function(), &group.values)
                .map_err(|error| {
                    seminaive_err(format!(
                        "{} aggregation in rule <{rule}>: {error}",
                        reduction.name(),
                    ))
                })?
                .ok_or_else(|| {
                    seminaive_err(format!(
                        "{} aggregation in rule <{rule}> is undefined for its complete group",
                        reduction.name(),
                    ))
                })?;
            let mut bindings: Vec<_> = reduction.groups.iter().cloned().zip(key).collect();
            bindings.push((reduction.result.clone(), result));
            Ok(Solution {
                bindings,
                source_facts: group.sources.into_values().collect(),
            })
        })
        .collect()
}
