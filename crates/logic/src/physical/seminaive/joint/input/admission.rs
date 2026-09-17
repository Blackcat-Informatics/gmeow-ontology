// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-bound termination refinement over proved immutable relations. This
//! enumerates native metadata bindings, never concrete witness executions. Every
//! possible firing is covered; an exhausted analysis yields no new certificate.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::{BTreeMap, MetadataDigest, PreparedPropertyRule, SemanticVocabulary};
use crate::physical::chase::{ChaseAdmission, ExistentialRule, StatementRule, join};
use crate::physical::effects::{ProducerEffect, WorldProducerEffect, value_flow::ValueFlow};
use crate::physical::store::RelationStore;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact, Solution};

const MAX_BINDINGS: usize = 4096;
const MAX_BYTES: usize = 1024 * 1024;

/// Retained rule metadata only; no source facts or per-world closure is cached.
pub(super) struct Template {
    rules: Vec<StatementRule>,
    predicates: BTreeSet<String>,
}

fn predicate(term: &EvalTerm) -> Option<&str> {
    match term {
        EvalTerm::ConstNamed(iri) | EvalTerm::ConstLit(purrdf::TermValue::Iri(iri)) => Some(iri),
        _ => None,
    }
}

impl Template {
    pub(super) fn new(
        rules: &[EvalRule],
        producers: &[ExistentialRule],
        properties: &[PreparedPropertyRule],
        families: &[super::super::families::Arm],
    ) -> Option<Self> {
        // An arithmetic range gap cannot be discharged by schema immutability.
        if rules
            .iter()
            .any(|rule| !rule.builtins.is_empty() && super::super::head_generates_value(rule))
        {
            return None;
        }
        let ordinary = rules
            .iter()
            .filter(|rule| rule.reduction.is_none())
            .map(|rule| StatementRule {
                name: rule.rule_iri.clone(),
                body: rule
                    .body
                    .iter()
                    .filter(|atom| !atom.negated)
                    .map(super::statement)
                    .collect(),
                heads: vec![super::statement(&rule.head)],
                frontier: (!rule.numeric.is_empty()).then(|| {
                    crate::physical::numeric::input_variables(&rule.body)
                        .into_iter()
                        .collect()
                }),
                position_only: !rule.numeric.is_empty(),
            });
        let rules: Vec<_> = ordinary
            .chain(producers.iter().map(StatementRule::from_binary))
            .chain(properties.iter().map(StatementRule::from_property))
            .chain(families.iter().map(|arm| arm.statement.clone()))
            .collect();
        let predicates = rules
            .iter()
            .flat_map(|rule| &rule.body)
            .filter_map(|atom| predicate(&atom[1]).map(str::to_owned))
            .collect();
        Some(Self { rules, predicates })
    }

    pub(super) fn observe<'a>(
        &self,
        facts: &'a BTreeMap<String, Vec<Fact>>,
        possible: &[(String, Fact)],
        flow: &ValueFlow,
        effects: &[ProducerEffect],
        external: &[WorldProducerEffect],
        semantics: SemanticVocabulary,
    ) -> Option<Evidence<'a>> {
        let predicates: BTreeSet<_> = self
            .predicates
            .iter()
            .filter(|predicate| {
                flow.immutable_predicate(
                    predicate,
                    effects
                        .iter()
                        .chain(external.iter().map(|effect| &effect.effect)),
                ) && !possible.iter().any(|(_, fact)| {
                    semantics.predicate(&fact.predicate) == semantics.predicate(predicate)
                })
            })
            .map(|predicate| semantics.predicate(predicate).to_owned())
            .collect();
        let selectors: BTreeSet<_> = self
            .predicates
            .iter()
            .map(|predicate| semantics.predicate(predicate))
            .collect();
        let mut selected = Vec::new();
        let mut bytes = 0usize;
        let mut digest = MetadataDigest(blake3::Hasher::new(), 0);
        digest.0.update(b"gmeow-native-selector-bindings-v2\0");
        write!(digest, "{predicates:?}:{possible:?}:{external:?}").expect("digest-only formatter");
        for (world, rows) in facts {
            write!(digest, "{world:?}").expect("digest-only formatter");
            for fact in rows {
                // Enriched value cells can distinguish formerly unknown values
                // anywhere in the input. Bind EVERY fact, including dynamic
                // predicate rows, without retaining or serializing the dataset.
                write!(digest, "{fact:?}").expect("digest-only formatter");
                if !selectors.contains(semantics.predicate(&fact.predicate)) {
                    continue;
                }
                let mut size = MetadataDigest(blake3::Hasher::new(), 0);
                write!(size, "{world:?}:{fact:?}").expect("digest-only formatter");
                bytes = bytes.saturating_add(size.1);
                if selected.len() == MAX_BINDINGS || bytes > MAX_BYTES {
                    return None;
                }
                selected.push(fact);
            }
        }
        Some(Evidence {
            predicates,
            facts: selected,
            identity: *digest.0.finalize().as_bytes(),
        })
    }

    pub(super) fn certify(
        &self,
        evidence: &Evidence<'_>,
        flow: &ValueFlow,
        facts: &BTreeMap<String, Vec<Fact>>,
        possible: &[(String, Fact)],
        external: &[WorldProducerEffect],
        semantics: SemanticVocabulary,
    ) -> gmeow_errors::Result<Option<ChaseAdmission>> {
        let mut rel = RelationStore::with_semantics(semantics);
        for fact in &evidence.facts {
            rel.insert(&fact.predicate, &fact.subject, &fact.object);
        }
        let mut analysis = Vec::new();
        let mut bytes = 0usize;
        for rule in &self.rules {
            let immutable: Vec<_> = rule
                .body
                .iter()
                .filter_map(|atom| {
                    let predicate = predicate(&atom[1])?;
                    evidence
                        .predicates
                        .contains(semantics.predicate(predicate))
                        .then(|| EvalAtom::positive(atom[0].clone(), predicate, atom[2].clone()))
                })
                .collect();
            let outcome = join::walk(
                &immutable,
                &rel,
                &Solution {
                    bindings: Vec::new(),
                    source_facts: Vec::new(),
                },
                join::Policy {
                    max_matches: MAX_BINDINGS,
                    distinct: &[],
                    retain_sources: false,
                },
                |solution| Ok(push_analysis(&mut analysis, &mut bytes, rule, &solution)),
            )?;
            if outcome != join::Outcome::Complete {
                return Ok(None);
            }
        }
        // A union across worlds only introduces extra bindings. Constant
        // substitution retains all mutable body atoms and every witness frontier
        // dependency. A finite abstract closure therefore bounds every world.
        let admission = ChaseAdmission::certify_statements(&analysis, semantics);
        if admission.admits_native() {
            return Ok(Some(admission));
        }
        // Derived selector edges need not be immutable to have a finite value
        // domain. Reuse the same abstract interpreter, with at most 512 exact
        // value cells. This never executes a concrete closure or lowers a rule.
        let Some(enriched) = flow.with_source_constants(
            evidence
                .facts
                .iter()
                .copied()
                .chain(possible.iter().map(|(_, fact)| fact)),
            512,
        ) else {
            return Ok(Some(admission));
        };
        let bindings = enriched.finite_bindings_with_patterns(
            facts
                .values()
                .flatten()
                .chain(possible.iter().map(|(_, fact)| fact)),
            external
                .iter()
                .flat_map(|effect| effect.effect.writes.iter()),
        );
        assert_eq!(
            bindings.len(),
            self.rules.len(),
            "all native producers share one analysis"
        );
        analysis.clear();
        bytes = 0;
        for (rule, bindings) in self.rules.iter().zip(bindings) {
            let Some(bindings) = bindings else {
                continue;
            };
            let choices: Vec<_> = bindings.into_iter().collect();
            let Some(count) = choices.iter().try_fold(1usize, |count, (_, values)| {
                count
                    .checked_mul(values.len())
                    .filter(|count| *count <= MAX_BINDINGS)
            }) else {
                return Ok(None);
            };
            for ordinal in 0..count {
                let mut cursor = ordinal;
                let solution = Solution {
                    bindings: choices
                        .iter()
                        .map(|(name, values)| {
                            let value = (*values[cursor % values.len()]).clone();
                            cursor /= values.len();
                            (name.clone(), value)
                        })
                        .collect(),
                    source_facts: Vec::new(),
                };
                if !push_analysis(&mut analysis, &mut bytes, rule, &solution) {
                    return Ok(None);
                }
            }
        }
        Ok(Some(ChaseAdmission::certify_statements(
            &analysis, semantics,
        )))
    }
}

fn push_analysis(
    analysis: &mut Vec<StatementRule>,
    bytes: &mut usize,
    rule: &StatementRule,
    solution: &Solution,
) -> bool {
    if analysis.len() == MAX_BINDINGS {
        return false;
    }
    let specialized = specialize(rule, solution, analysis.len());
    let mut digest = MetadataDigest(blake3::Hasher::new(), 0);
    write!(
        digest,
        "{:?}:{:?}:{:?}",
        specialized.body, specialized.heads, specialized.frontier
    )
    .expect("digest-only formatter");
    *bytes = bytes.saturating_add(digest.1);
    if *bytes > MAX_BYTES {
        return false;
    }
    analysis.push(specialized);
    true
}

pub(super) struct Evidence<'a> {
    predicates: BTreeSet<String>,
    facts: Vec<&'a Fact>,
    pub(super) identity: [u8; 32],
}

fn specialize(rule: &StatementRule, solution: &Solution, ordinal: usize) -> StatementRule {
    let substitute = |atom: &[EvalTerm; 3]| {
        atom.each_ref().map(|term| {
            if let EvalTerm::Var(name) = term
                && let Some(value) = solution.get(name)
            {
                return EvalTerm::ConstLit(value.clone());
            }
            term.clone()
        })
    };
    StatementRule {
        name: format!("{}:immutable-binding:{ordinal}", rule.name),
        body: rule.body.iter().map(substitute).collect(),
        heads: rule.heads.iter().map(substitute).collect(),
        frontier: rule.frontier.as_ref().map(|frontier| {
            frontier
                .iter()
                .filter(|name| solution.get(name).is_none())
                .cloned()
                .collect()
        }),
        position_only: rule.position_only,
    }
}
