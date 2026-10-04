// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-bound termination refinement over proved immutable relations. This
//! enumerates native metadata bindings, never concrete witness executions. Every
//! possible firing is covered; an exhausted analysis yields no new certificate.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::{BTreeMap, MetadataDigest, PreparedPropertyRule, SemanticVocabulary};
use crate::physical::chase::{
    ChaseAdmission, ExistentialRule, Ladder, StatementRule, firing_statements, join,
};
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
    ) -> Result<Evidence<'a>, EvidenceGap> {
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
        let mut enrichment = Some(Vec::new());
        let mut enrichment_bytes = 0usize;
        let mut retained = 0usize;
        let mut per_predicate: BTreeMap<&str, usize> = BTreeMap::new();
        let mut bytes = 0usize;
        let mut digest = MetadataDigest(blake3::Hasher::new(), 0);
        digest.0.update(b"gmeow-native-selector-bindings-v3\0");
        write!(digest, "{predicates:?}:{possible:?}:{external:?}").expect("digest-only formatter");
        for (world, rows) in facts {
            write!(digest, "{world:?}").expect("digest-only formatter");
            for fact in rows {
                // Enriched value cells can distinguish formerly unknown values
                // anywhere in the input. Bind EVERY fact, including dynamic
                // predicate rows, without retaining or serializing the dataset.
                write!(digest, "{fact:?}").expect("digest-only formatter");
                let predicate = semantics.predicate(&fact.predicate);
                if !selectors.contains(predicate) {
                    continue;
                }
                let mut size = MetadataDigest(blake3::Hasher::new(), 0);
                write!(size, "{world:?}:{fact:?}").expect("digest-only formatter");
                // Every selector row sharpens the source-constant enrichment, under
                // its own bound; past it the enrichment reads the immutable rows only,
                // the same coarsening the 512-cell interning cap already applies.
                enrichment_bytes = enrichment_bytes.saturating_add(size.1);
                if let Some(rows) = &mut enrichment {
                    if rows.len() < MAX_BINDINGS && enrichment_bytes <= MAX_BYTES {
                        rows.push(fact);
                    } else {
                        enrichment = None;
                    }
                }
                // The certificate's join reads only proved-immutable relations, so
                // only their rows bound the analysis, all of them retained. A mutable
                // selector row can never match that join.
                if !predicates.contains(predicate) {
                    continue;
                }
                bytes = bytes.saturating_add(size.1);
                retained += 1;
                *per_predicate
                    .entry(semantics.predicate(&fact.predicate))
                    .or_default() += 1;
                if retained <= MAX_BINDINGS && bytes <= MAX_BYTES {
                    selected.push(fact);
                }
            }
        }
        if retained > MAX_BINDINGS || bytes > MAX_BYTES {
            let mut heaviest: Vec<_> = per_predicate
                .into_iter()
                .map(|(predicate, rows)| (predicate.to_owned(), rows))
                .collect();
            heaviest.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
            heaviest.truncate(8);
            return Err(EvidenceGap::Bound {
                facts: retained,
                bytes,
                heaviest,
            });
        }
        Ok(Evidence {
            predicates,
            facts: selected,
            enrichment,
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
        // Only producers that can fire on this admitted input bear on its termination.
        let seeds = firing_seeds(facts, possible, external, semantics);
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
        let firing = firing_statements(&analysis, seeds.as_ref(), semantics);
        let admission = ChaseAdmission::certify_statements(&firing, semantics, Ladder::Complete);
        if admission.admits_native() {
            return Ok(Some(admission));
        }
        // Derived selector edges need not be immutable to have a finite value
        // domain. Reuse the same abstract interpreter, with at most 512 exact
        // value cells. This never executes a concrete closure or lowers a rule.
        let enriched = flow.with_source_constants(
            evidence
                .enrichment
                .as_ref()
                .unwrap_or(&evidence.facts)
                .iter()
                .copied()
                .chain(possible.iter().map(|(_, fact)| fact)),
            512,
        );
        // The source-selected metadata flow remains a sound finite refinement
        // when interning every other source value would exceed the analysis cap.
        let enriched = enriched.as_ref().unwrap_or(flow);
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
        let firing = firing_statements(&analysis, seeds.as_ref(), semantics);
        Ok(Some(ChaseAdmission::certify_statements(
            &firing,
            semantics,
            Ladder::Complete,
        )))
    }
}

/// The canonical predicates this input can present: every world fact, every possible
/// fact and every external producer write. `None` when an external write may carry any
/// predicate, so no producer can be shown not to fire.
fn firing_seeds(
    facts: &BTreeMap<String, Vec<Fact>>,
    possible: &[(String, Fact)],
    external: &[WorldProducerEffect],
    semantics: SemanticVocabulary,
) -> Option<BTreeSet<String>> {
    let mut seeds: BTreeSet<String> = facts
        .values()
        .flatten()
        .chain(possible.iter().map(|(_, fact)| fact))
        .map(|fact| semantics.predicate(&fact.predicate).to_owned())
        .collect();
    for write in external
        .iter()
        .flat_map(|effect| effect.effect.writes.iter())
    {
        seeds.insert(semantics.predicate(write.predicate()?).to_owned());
    }
    Some(seeds)
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

/// Why an input's specific termination certificate was not attempted. The
/// source-independent template certificate then decides alone, and a refusal
/// names this gap rather than presenting the template's ledger as the input's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EvidenceGap {
    /// The retained rule metadata exceeds the cacheable bound.
    Uncacheable { metadata_bytes: usize },
    /// The proved-immutable selector rows exceed the analysis bound.
    Bound {
        facts: usize,
        bytes: usize,
        /// The predicates retaining the most rows, heaviest first.
        heaviest: Vec<(String, usize)>,
    },
}

impl std::fmt::Display for EvidenceGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uncacheable { metadata_bytes } => write!(
                f,
                "the joint template's rule metadata ({metadata_bytes} bytes) exceeds the \
                 {MAX_BYTES}-byte cacheable bound"
            ),
            Self::Bound {
                facts,
                bytes,
                heaviest,
            } => {
                write!(
                    f,
                    "{facts} proved-immutable selector rows ({bytes} bytes) exceed the \
                     {MAX_BINDINGS}-row / {MAX_BYTES}-byte analysis bound; heaviest:"
                )?;
                for (predicate, rows) in heaviest {
                    write!(f, " {predicate}={rows}")?;
                }
                Ok(())
            }
        }
    }
}

impl EvidenceGap {
    pub(super) fn uncacheable(metadata_bytes: usize) -> Self {
        Self::Uncacheable { metadata_bytes }
    }
}

/// The bound a specialized statement analysis exhausted.
pub(super) const ANALYSIS_BOUND: usize = MAX_BINDINGS;

pub(super) struct Evidence<'a> {
    predicates: BTreeSet<String>,
    facts: Vec<&'a Fact>,
    /// Every selector row for source-constant enrichment, when within bound.
    enrichment: Option<Vec<&'a Fact>>,
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
