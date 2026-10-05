// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-bound termination refinement over proved immutable relations and the
//! exact closures of witness-free relations (see [`Closure`]). This enumerates
//! native metadata bindings, never concrete witness executions. Every possible
//! firing is covered; an exhausted analysis yields no new certificate.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::{BTreeMap, MetadataDigest, PreparedPropertyRule, SemanticVocabulary};
use crate::physical::chase::{
    ChaseAdmission, ExistentialRule, Ladder, StatementRule, firing_statements, join,
};
use crate::physical::effects::{
    ProducerEffect, StatementPattern, WorldProducerEffect, value_flow::ValueFlow,
};
use crate::physical::store::RelationStore;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact, Solution};

/// The proved-immutable selector rows, specialized statements and per-rule join
/// matches one input-specific certificate may analyse. Weak acyclicity, the rung a
/// position-only cardinality program uses, is linear in the specialized program.
const LIST_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const LIST_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
/// Admission-only relation `(list head, cell)`: the cell is reachable from the head
/// through `rdf:rest`. It never enters a source, effect or value-flow read.
const LIST_CELL: &str = "https://blackcatinformatics.ca/gmeow/termination/list-cell";
const MAX_BINDINGS: usize = 1 << 18;
const MAX_BYTES: usize = 128 << 20;
/// The selector rows feeding source-constant enrichment. Its interning stops at 512
/// value cells, so more rows than this cannot sharpen it further.
const ENRICHMENT_ROWS: usize = 4096;
const ENRICHMENT_BYTES: usize = 1 << 20;
/// The retained template rule metadata a cacheable joint template may carry.
pub(super) const CACHEABLE_BYTES: usize = 1 << 20;

/// Retained rule metadata only; no source facts or per-world closure is cached.
pub(super) struct Template {
    rules: Vec<StatementRule>,
    predicates: BTreeSet<String>,
    /// The `(predicate, IRI object)` surfaces of constant body atoms: the marker
    /// pairs, such as `(instanceOf, owl:TransitiveProperty)`, that may be closed.
    pairs: BTreeSet<(String, String)>,
    /// The producers [`Self::rules`] models, by name. A writer under any other name,
    /// or a reduction (which `rules` omits), is foreign to the closure analysis.
    names: BTreeSet<String>,
    reductions: BTreeSet<String>,
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
        let reductions = rules
            .iter()
            .filter(|rule| rule.reduction.is_some())
            .map(|rule| rule.rule_iri.clone())
            .collect();
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
                list_cells: Vec::new(),
                witness_family: None,
            });
        let rules: Vec<_> = ordinary
            .chain(producers.iter().map(StatementRule::from_binary))
            .chain(properties.iter().map(StatementRule::from_property))
            .chain(families.iter().map(|arm| arm.statement.clone()))
            .collect();
        let predicates = rules
            .iter()
            .flat_map(|rule| &rule.body)
            .filter_map(|atom| constant_iri(&atom[1]).map(str::to_owned))
            .collect();
        let pairs = rules
            .iter()
            .flat_map(|rule| &rule.body)
            .filter_map(|atom| {
                Some((
                    constant_iri(&atom[1])?.to_owned(),
                    constant_iri(&atom[2])?.to_owned(),
                ))
            })
            .collect();
        let names = rules.iter().map(|rule| rule.name.clone()).collect();
        Some(Self {
            rules,
            predicates,
            pairs,
            names,
            reductions,
        })
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
        // The candidate closed relations (see [`Closure`]): not immutable, absent
        // from the possible facts, and written by no producer outside this analysis.
        // Whether this analysis's own producers keep them closed is decided by the
        // certificate, against their source rows retained below.
        let foreign: Vec<&StatementPattern> = effects
            .iter()
            .filter(|effect| {
                !self.names.contains(effect.name()) || self.reductions.contains(effect.name())
            })
            .flat_map(|effect| &effect.writes)
            .chain(external.iter().flat_map(|effect| &effect.effect.writes))
            .collect();
        let closable = |relation: &str, object: Option<&str>| {
            let canonical = semantics.predicate(relation);
            !predicates.contains(canonical)
                && !possible.iter().any(|(_, fact)| {
                    semantics.predicate(&fact.predicate) == canonical
                        && object.is_none_or(|object| {
                            constant_value_iri(&fact.object).is_some_and(|iri| {
                                semantics.analysis_symbol(iri) == semantics.analysis_symbol(object)
                            })
                        })
                })
                && {
                    let pattern = StatementPattern::relation(Some(relation), object);
                    !foreign
                        .iter()
                        .any(|write| flow.overlaps_write(&pattern, write))
                }
        };
        let mut candidates = Closure::empty();
        candidates.predicates = self
            .predicates
            .iter()
            .filter(|predicate| closable(predicate, None))
            .map(|predicate| semantics.predicate(predicate).to_owned())
            .collect();
        candidates.pairs = self
            .pairs
            .iter()
            // A pair stays a candidate even when its whole predicate is one: the
            // certificate may open the predicate yet keep the pair closed.
            .filter(|(predicate, object)| closable(predicate, Some(object)))
            .map(|(predicate, object)| {
                (
                    semantics.predicate(predicate).to_owned(),
                    semantics.analysis_symbol(object).to_owned(),
                )
            })
            .collect();
        let mut closable_rows = Some(Vec::new());
        let mut closable_bytes = 0usize;
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
                    if rows.len() < ENRICHMENT_ROWS && enrichment_bytes <= ENRICHMENT_BYTES {
                        rows.push(fact);
                    } else {
                        enrichment = None;
                    }
                }
                // A candidate closed relation's source rows seed its exact closure,
                // under the same bound as the immutable rows.
                if candidates.holds(predicate, constant_value_iri(&fact.object), semantics) {
                    closable_bytes = closable_bytes.saturating_add(size.1);
                    if let Some(rows) = &mut closable_rows {
                        if rows.len() < MAX_BINDINGS && closable_bytes <= MAX_BYTES {
                            rows.push(fact);
                        } else {
                            closable_rows = None;
                        }
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
            closable: closable_rows
                .map(|facts| Closable { candidates, facts })
                .ok_or_else(|| {
                    format!(
                        "their source rows exceed the {MAX_BINDINGS}-row / {MAX_BYTES}-byte \
                         analysis bound"
                    )
                }),
            identity: *digest.0.finalize().as_bytes(),
        })
    }

    /// The input-specific certificate: the settled first pass, and the
    /// source-enumerated second pass when the first refuses.
    pub(super) fn certify(
        &self,
        evidence: &Evidence<'_>,
        flow: &ValueFlow,
        facts: &BTreeMap<String, Vec<Fact>>,
        possible: &[(String, Fact)],
        external: &[WorldProducerEffect],
        semantics: SemanticVocabulary,
    ) -> gmeow_errors::Result<Result<ChaseAdmission, String>> {
        let Settled { admission, notes } =
            match self.certify_settled(evidence, facts, possible, external, semantics)? {
                Ok(settled) => settled,
                Err(reason) => return Ok(Err(reason)),
            };
        if admission.admits_native() {
            return Ok(Ok(admission));
        }
        let refused = |mut admission: ChaseAdmission| {
            if let ChaseAdmission::Uncertified { violations } = &mut admission {
                for (index, note) in notes.iter().enumerate() {
                    violations.insert(index, note.clone());
                }
            }
            admission
        };
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
        let mut analysis = Vec::new();
        let mut bytes = 0usize;
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
                // The decided first-pass certificate stands when the finer
                // enumeration cannot complete.
                return Ok(Ok(refused(admission)));
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
                if !push_analysis(&mut analysis, &mut bytes, rule, &solution, &[]) {
                    return Ok(Ok(refused(admission)));
                }
            }
        }
        tracing::info!(
            target: "termination_certificate",
            specialized = analysis.len(),
            "enumerated source-selected bindings"
        );
        let seeds = firing_seeds(facts, possible, external, semantics);
        let firing = firing_statements(&analysis, seeds.as_ref(), semantics);
        Ok(Ok(refused(ChaseAdmission::certify_statements(
            &firing,
            semantics,
            Ladder::Complete,
        ))))
    }

    /// The first pass: specialize every producer on its settled (immutable or
    /// closed) relations and certify the result. It decides alone whenever the
    /// source-enumerated second pass cannot complete, as on a production corpus.
    /// `Ok(Err(reason))` when the specialization exhausts its bound.
    pub(super) fn certify_settled(
        &self,
        evidence: &Evidence<'_>,
        facts: &BTreeMap<String, Vec<Fact>>,
        possible: &[(String, Fact)],
        external: &[WorldProducerEffect],
        semantics: SemanticVocabulary,
    ) -> gmeow_errors::Result<Result<Settled, String>> {
        // Only producers that can fire on this admitted input bear on its termination.
        let seeds = firing_seeds(facts, possible, external, semantics);
        // Close the witness-free relations first, so their final extension binds
        // variables exactly like an immutable relation. When that cannot complete
        // within its bound, join immutable relations only, and say so on refusal.
        let mut notes = Vec::new();
        let started = std::time::Instant::now();
        let closed = match &evidence.closable {
            Ok(closable) => self.close(evidence, closable, semantics)?,
            Err(reason) => Err(reason.clone()),
        };
        let (closure, rel, list_cells_bound) = closed.unwrap_or_else(|reason| {
            notes.push(format!("witness-free relations were not closed: {reason}"));
            let (rel, cells) = Self::store(evidence, &[], semantics);
            (Closure::empty(), rel, cells)
        });
        let analysis = match self.specialize_all(
            &rel,
            &evidence.predicates,
            &closure,
            list_cells_bound,
            semantics,
        )? {
            Ok(analysis) => analysis,
            Err(reason) if !closure.is_empty() => {
                // Closed relations can multiply specializations past the bound; the
                // immutable-only specialization is coarser but still complete.
                notes.push(format!("closed relations were not joined: {reason}"));
                match self.specialize_all(
                    &rel,
                    &evidence.predicates,
                    &Closure::empty(),
                    list_cells_bound,
                    semantics,
                )? {
                    Ok(analysis) => analysis,
                    Err(reason) => return Ok(Err(reason)),
                }
            }
            Err(reason) => return Ok(Err(reason)),
        };
        tracing::info!(
            target: "termination_certificate",
            closed = !closure.is_empty(),
            specialized = analysis.len(),
            elapsed_ms = started.elapsed().as_millis(),
            "specialized producers on settled relations"
        );
        // A union across worlds only introduces extra bindings. Constant
        // substitution retains all mutable body atoms and every witness frontier
        // dependency. A finite abstract closure therefore bounds every world.
        let firing = firing_statements(&analysis, seeds.as_ref(), semantics);
        Ok(Ok(Settled {
            admission: ChaseAdmission::certify_statements(&firing, semantics, Ladder::Complete),
            notes,
        }))
    }

    /// The proved-immutable rows, with `extra` rows, as one join store. Returns
    /// whether list member cells were bound to the cells of their own lists.
    fn store(
        evidence: &Evidence<'_>,
        extra: &[&Fact],
        semantics: SemanticVocabulary,
    ) -> (RelationStore, bool) {
        let mut rel = RelationStore::with_semantics(semantics);
        for fact in evidence.facts.iter().chain(extra) {
            rel.insert(&fact.predicate, &fact.subject, &fact.object);
        }
        // A list operator reads members of ITS list. When list structure is proved
        // immutable, bind each member cell to the cells reachable from that list
        // head, instead of to every cell of every list.
        let list_cells_bound = [LIST_FIRST, LIST_REST]
            .iter()
            .all(|predicate| evidence.predicates.contains(semantics.predicate(predicate)))
            && bind_list_cells(&mut rel, &evidence.facts, semantics);
        (rel, list_cells_bound)
    }

    /// Whether a body atom reads only an immutable or closed relation.
    fn settled(
        atom: &[EvalTerm; 3],
        immutable: &BTreeSet<String>,
        closure: &Closure,
        semantics: SemanticVocabulary,
    ) -> bool {
        constant_iri(&atom[1]).is_some_and(|predicate| {
            let predicate = semantics.predicate(predicate);
            immutable.contains(predicate)
                || closure.holds(predicate, constant_iri(&atom[2]), semantics)
        })
    }

    /// A witness-free producer whose every read is settled: its closure is exact.
    fn eligible(
        rule: &StatementRule,
        immutable: &BTreeSet<String>,
        closure: &Closure,
        semantics: SemanticVocabulary,
    ) -> bool {
        if rule.frontier.is_some() || rule.position_only || rule.witness_family.is_some() {
            return false;
        }
        let bound: BTreeSet<&str> = rule
            .body
            .iter()
            .flatten()
            .filter_map(|term| match term {
                EvalTerm::Var(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        rule.heads
            .iter()
            .flatten()
            .all(|term| !matches!(term, EvalTerm::Var(name) if !bound.contains(name.as_str())))
            && rule
                .body
                .iter()
                .all(|atom| Self::settled(atom, immutable, closure, semantics))
    }

    /// Compute the closed relations of this input and their complete extension (see
    /// [`Closure`]). `Ok(Err(reason))` when the closure exceeds the analysis bound;
    /// the certificate then joins immutable relations only, coarser but sound.
    fn close(
        &self,
        evidence: &Evidence<'_>,
        closable: &Closable<'_>,
        semantics: SemanticVocabulary,
    ) -> gmeow_errors::Result<Result<(Closure, RelationStore, bool), String>> {
        let mut closure = closable.candidates.clone();
        loop {
            let rows: Vec<&Fact> = closable
                .facts
                .iter()
                .copied()
                .filter(|fact| {
                    closure.holds(
                        semantics.predicate(&fact.predicate),
                        constant_value_iri(&fact.object),
                        semantics,
                    )
                })
                .collect();
            let (mut store, cells) = Self::store(evidence, &rows, semantics);
            if closure.is_empty() {
                return Ok(Ok((closure, store, cells)));
            }
            let eligible: Vec<_> = self
                .rules
                .iter()
                .enumerate()
                .filter(|(_, rule)| Self::eligible(rule, &evidence.predicates, &closure, semantics))
                .map(|(index, rule)| (index, join_atoms(rule, &vec![true; rule.body.len()], cells)))
                .collect();
            if let Err(reason) = derive(&mut store, &self.rules, &eligible, &closure, semantics)? {
                return Ok(Err(reason));
            }
            // Drop every member some producer may write without being eligible.
            let mut columns = Columns::default();
            let eligible: BTreeSet<usize> = eligible.iter().map(|(index, _)| *index).collect();
            let mut open_predicates = BTreeSet::new();
            let mut open_pairs = BTreeSet::new();
            for (index, rule) in self.rules.iter().enumerate() {
                if eligible.contains(&index) {
                    continue;
                }
                for head in &rule.heads {
                    for predicate in &closure.predicates {
                        if !open_predicates.contains(predicate)
                            && columns.may_write(
                                (index, rule),
                                head,
                                (predicate, None),
                                (&store, &evidence.predicates, &closure),
                                semantics,
                            )?
                        {
                            open_predicates.insert(predicate.clone());
                        }
                    }
                    for pair in &closure.pairs {
                        if !open_pairs.contains(pair)
                            && columns.may_write(
                                (index, rule),
                                head,
                                (&pair.0, Some(&pair.1)),
                                (&store, &evidence.predicates, &closure),
                                semantics,
                            )?
                        {
                            open_pairs.insert(pair.clone());
                        }
                    }
                }
            }
            if open_predicates.is_empty() && open_pairs.is_empty() {
                return Ok(Ok((closure, store, cells)));
            }
            closure
                .predicates
                .retain(|predicate| !open_predicates.contains(predicate));
            closure.pairs.retain(|pair| !open_pairs.contains(pair));
        }
    }

    /// Specialize every producer on the join solutions of its settled body atoms.
    /// `Ok(Err(reason))` when the analysis exhausts its bound.
    fn specialize_all(
        &self,
        rel: &RelationStore,
        immutable: &BTreeSet<String>,
        closure: &Closure,
        list_cells_bound: bool,
        semantics: SemanticVocabulary,
    ) -> gmeow_errors::Result<Result<Vec<StatementRule>, String>> {
        let mut analysis = Vec::new();
        let mut bytes = 0usize;
        for rule in &self.rules {
            // A fully bound settled atom is a ground source or closed row, and no
            // null ever occupies a settled relation: no rung reads a position it
            // holds. It is discharged, and solutions agreeing on every variable
            // still read elsewhere specialize to one statement.
            let discharged: Vec<bool> = rule
                .body
                .iter()
                .map(|atom| Self::settled(atom, immutable, closure, semantics))
                .collect();
            let immutable_atoms = join_atoms(rule, &discharged, list_cells_bound);
            let relevant: BTreeSet<&str> = rule
                .body
                .iter()
                .zip(&discharged)
                .filter(|(_, discharged)| !**discharged)
                .map(|(atom, _)| atom)
                .chain(&rule.heads)
                .flatten()
                // A witness family's count selects how many witnesses it analyses
                // (one exactly, or the two-ordinal summary), so solutions that
                // differ only in the count are distinct specializations.
                .chain(rule.witness_family.as_ref().map(|family| &family.count))
                .filter_map(|term| match term {
                    EvalTerm::Var(name) => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            // When every still-read variable is bound by the discharged atoms, each
            // specialization is ground: it holds no position a null can reach and
            // fires on the same predicates. One witnesses them all.
            let ground = relevant.iter().all(|name| {
                immutable_atoms
                    .iter()
                    .flat_map(|atom| [&atom.subject, &atom.object])
                    .any(|term| matches!(term, EvalTerm::Var(var) if var == name))
            });
            let mut projections = std::collections::HashSet::new();
            let mut matches = 0usize;
            let outcome = join::walk(
                &immutable_atoms,
                rel,
                &Solution {
                    bindings: Vec::new(),
                    source_facts: Vec::new(),
                },
                join::Policy {
                    max_matches: MAX_BINDINGS,
                    distinct: &[],
                    retain_sources: false,
                },
                |solution| {
                    matches += 1;
                    let projection: Vec<_> = relevant
                        .iter()
                        .map(|name| solution.get(name).cloned())
                        .collect();
                    if !projections.insert(projection) {
                        return Ok(true);
                    }
                    let pushed =
                        push_analysis(&mut analysis, &mut bytes, rule, &solution, &discharged);
                    Ok(pushed && !ground)
                },
            )?;
            let witnessed = ground && outcome == join::Outcome::Stopped && projections.len() == 1;
            if outcome != join::Outcome::Complete && !witnessed {
                return Ok(Err(format!(
                    "the input-specific analysis exhausted its bound of {MAX_BINDINGS} \
                     specialized statements at {} ({matches} settled matches, {} distinct \
                     specializations; {} statements in total)",
                    rule.name,
                    projections.len(),
                    analysis.len()
                )));
            }
        }
        Ok(Ok(analysis))
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
    discharged: &[bool],
) -> bool {
    if analysis.len() == MAX_BINDINGS {
        return false;
    }
    let specialized = specialize(rule, solution, analysis.len(), discharged);
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
                 {CACHEABLE_BYTES}-byte cacheable bound"
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

pub(super) struct Evidence<'a> {
    predicates: BTreeSet<String>,
    facts: Vec<&'a Fact>,
    /// Every selector row for source-constant enrichment, when within bound.
    enrichment: Option<Vec<&'a Fact>>,
    /// The candidate closed relations and their source rows, or why they exceed
    /// the analysis bound.
    closable: Result<Closable<'a>, String>,
    pub(super) identity: [u8; 32],
}

/// `discharged` marks the fully bound immutable body atoms to omit; an empty mask
/// keeps every atom.
fn specialize(
    rule: &StatementRule,
    solution: &Solution,
    ordinal: usize,
    discharged: &[bool],
) -> StatementRule {
    let substitute_term = |term: &EvalTerm| {
        if let EvalTerm::Var(name) = term
            && let Some(value) = solution.get(name)
        {
            return EvalTerm::ConstLit(value.clone());
        }
        term.clone()
    };
    let substitute = |atom: &[EvalTerm; 3]| atom.each_ref().map(substitute_term);
    StatementRule {
        name: format!("{}:immutable-binding:{ordinal}", rule.name),
        body: rule
            .body
            .iter()
            .enumerate()
            .filter(|(index, _)| !discharged.get(*index).copied().unwrap_or(false))
            .map(|(_, atom)| substitute(atom))
            .collect(),
        heads: rule.heads.iter().map(substitute).collect(),
        frontier: rule.frontier.as_ref().map(|frontier| {
            frontier
                .iter()
                .filter(|name| solution.get(name).is_none())
                .cloned()
                .collect()
        }),
        position_only: rule.position_only,
        list_cells: Vec::new(),
        witness_family: rule.witness_family.as_ref().map(|family| {
            crate::physical::chase::WitnessFamily {
                count: substitute_term(&family.count),
                single: family.single,
            }
        }),
    }
}

/// Add `(head, cell)` admission rows for every cell reachable through `rdf:rest`
/// from every cell, so a list operator bound to any head (shared tails included)
/// reads exactly its own cells. Returns `false`, adding nothing, when the rows would
/// exceed the analysis bound; member cells then stay unconstrained, which is coarser
/// but still covers every firing.
fn bind_list_cells(
    rel: &mut RelationStore,
    facts: &[&Fact],
    semantics: SemanticVocabulary,
) -> bool {
    let first = semantics.predicate(LIST_FIRST);
    let rest = semantics.predicate(LIST_REST);
    let mut cells = BTreeSet::new();
    let mut next: BTreeMap<&purrdf::TermValue, Vec<&purrdf::TermValue>> = BTreeMap::new();
    for fact in facts {
        let predicate = semantics.predicate(&fact.predicate);
        if predicate == first {
            cells.insert(&fact.subject);
        } else if predicate == rest {
            next.entry(&fact.subject).or_default().push(&fact.object);
        }
    }
    let mut rows = Vec::new();
    for &head in &cells {
        let mut seen = BTreeSet::from([head]);
        let mut pending = vec![head];
        while let Some(cell) = pending.pop() {
            if cells.contains(cell) {
                rows.push((head, cell));
                if rows.len() > MAX_BINDINGS {
                    return false;
                }
            }
            for &tail in next.get(cell).into_iter().flatten() {
                if seen.insert(tail) {
                    pending.push(tail);
                }
            }
        }
    }
    for (head, cell) in rows {
        rel.insert(LIST_CELL, head, cell);
    }
    true
}

/// The first-pass certificate and the notes a refusal must carry.
pub(super) struct Settled {
    pub(super) admission: ChaseAdmission,
    notes: Vec<String>,
}

/// The candidate closed relations and their source rows, observed with the evidence.
pub(super) struct Closable<'a> {
    candidates: Closure,
    facts: Vec<&'a Fact>,
}

/// Relations whose COMPLETE final extension is computed before specialization.
///
/// A relation is *closed* when every producer that can write it is witness-free
/// (no existential head variable, no witness family, no numeric frontier) and reads
/// only immutable or closed relations. No null can then ever enter it: source facts
/// carry no nulls, and its writers copy only values read from relations that carry
/// none. Its extension in every run is therefore the least fixpoint of those writers
/// over its source rows — a finite Datalog closure computed here once — and the
/// certificate may join and discharge it exactly like an immutable relation.
///
/// Membership is decided per whole predicate (`rdfs:subClassOf`) or per
/// `(predicate, constant object)` pair (`(?, instanceOf, owl:TransitiveProperty)`),
/// as a GREATEST fixpoint: start from every candidate, compute the closure under the
/// writers eligible for the current set, and drop each member with a writer that is
/// neither eligible nor proved unable to write it, until nothing changes. A writer is
/// proved unable to write a member when its head predicate (or object) variable is
/// bound by a closed or immutable body atom whose column, in the computed closure,
/// holds no matching value.
///
/// Soundness at the fixpoint, by induction over any concrete chase: suppose `f` is the
/// first fact of a member relation outside the computed extension `E`. Its firing read
/// only facts derived before `f`; those of member relations lie in `E`. If the writer
/// is eligible, `E` is closed under it, so `f ∈ E`. Otherwise the writer was proved
/// unable to write the member from a closed column that is a superset of the value it
/// read, so `f` cannot belong to the member. Both contradict the choice of `f`. Every
/// world is covered because `E` closes the union of all worlds' rows, a superset of
/// each world's closure; dropped negations and guards only enlarge `E`.
#[derive(Clone)]
struct Closure {
    predicates: BTreeSet<String>,
    pairs: BTreeSet<(String, String)>,
}

impl Closure {
    fn empty() -> Self {
        Self {
            predicates: BTreeSet::new(),
            pairs: BTreeSet::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.predicates.is_empty() && self.pairs.is_empty()
    }

    /// Whether a statement with this canonical predicate and object lies in a member.
    fn holds(&self, predicate: &str, object: Option<&str>, semantics: SemanticVocabulary) -> bool {
        self.predicates.contains(predicate)
            || object.is_some_and(|object| {
                self.pairs.contains(&(
                    predicate.to_owned(),
                    semantics.analysis_symbol(object).to_owned(),
                ))
            })
    }
}

/// An IRI constant term's surface: a fixed operator or marker.
fn constant_iri(term: &EvalTerm) -> Option<&str> {
    match term {
        EvalTerm::ConstNamed(iri) | EvalTerm::ConstLit(purrdf::TermValue::Iri(iri)) => Some(iri),
        _ => None,
    }
}

/// Bind a head term under `solution`; `None` when it stays unbound.
fn ground(term: &EvalTerm, solution: &Solution) -> Option<purrdf::TermValue> {
    match term {
        EvalTerm::Var(name) => solution.get(name).cloned(),
        EvalTerm::ConstNamed(iri) => Some(purrdf::TermValue::iri(iri.clone())),
        EvalTerm::ConstLit(value) => Some(value.clone()),
    }
}

/// The IRI surface of a stored term, if it is an IRI.
fn constant_value_iri(value: &purrdf::TermValue) -> Option<&str> {
    match value {
        purrdf::TermValue::Iri(iri) => Some(iri),
        _ => None,
    }
}

/// The settled atoms of `rule` (`discharged`) in join order. The join evaluates atoms
/// in order: bind each list head first, then restrict its member cells to that list,
/// and only then read the cells — reading cells first would enumerate every cell of
/// every list.
fn join_atoms(rule: &StatementRule, discharged: &[bool], list_cells_bound: bool) -> Vec<EvalAtom> {
    let discharged_atoms: Vec<_> = rule
        .body
        .iter()
        .zip(discharged)
        .filter(|(_, discharged)| **discharged)
        .filter_map(|(atom, _)| {
            let predicate = constant_iri(&atom[1])?;
            Some(EvalAtom::positive(
                atom[0].clone(),
                predicate,
                atom[2].clone(),
            ))
        })
        .collect();
    let cells: Vec<_> = rule
        .list_cells
        .iter()
        .filter(|_| list_cells_bound)
        .collect();
    let reads_cell = |atom: &EvalAtom| {
        cells
            .iter()
            .any(|(_, cell)| atom.subject == *cell || atom.object == *cell)
    };
    discharged_atoms
        .iter()
        .filter(|atom| !reads_cell(atom))
        .cloned()
        .chain(
            cells
                .iter()
                .map(|(list, cell)| EvalAtom::positive(list.clone(), LIST_CELL, cell.clone())),
        )
        .chain(
            discharged_atoms
                .iter()
                .filter(|atom| reads_cell(atom))
                .cloned(),
        )
        .collect()
}

/// Close `store` under the `eligible` producers (indices into `rules`, with their
/// join atoms), retaining only the derived facts of closed relations. Semi-naive:
/// after one complete pass, every further derivation joins at least one fact new
/// in the previous round. `Ok(Err(reason))` past the analysis bound.
fn derive(
    store: &mut RelationStore,
    rules: &[StatementRule],
    eligible: &[(usize, Vec<EvalAtom>)],
    closure: &Closure,
    semantics: SemanticVocabulary,
) -> gmeow_errors::Result<Result<(), String>> {
    let empty = Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    };
    let policy = || join::Policy {
        max_matches: MAX_BINDINGS,
        distinct: &[],
        retain_sources: false,
    };
    let exhausted = || {
        format!(
            "the closure of witness-free relations exceeded its bound of {MAX_BINDINGS} \
             matches or derived rows"
        )
    };
    let emit =
        |store: &RelationStore, rule: &StatementRule, solution: &Solution, out: &mut Vec<Fact>| {
            for head in &rule.heads {
                let (Some(subject), Some(purrdf::TermValue::Iri(predicate)), Some(object)) = (
                    ground(&head[0], solution),
                    ground(&head[1], solution),
                    ground(&head[2], solution),
                ) else {
                    continue;
                };
                if closure.holds(
                    semantics.predicate(&predicate),
                    constant_value_iri(&object),
                    semantics,
                ) && !store.contains(&predicate, &subject, &object)
                {
                    out.push(Fact {
                        subject,
                        predicate,
                        object,
                    });
                }
            }
        };
    let mut pending = Vec::new();
    for (index, atoms) in eligible {
        let rule = &rules[*index];
        let outcome = join::walk(atoms, store, &empty, policy(), |solution| {
            emit(store, rule, &solution, &mut pending);
            Ok(true)
        })?;
        if outcome != join::Outcome::Complete {
            return Ok(Err(exhausted()));
        }
    }
    let mut derived = 0usize;
    loop {
        let mut fresh = Vec::new();
        for fact in pending.drain(..) {
            if store
                .insert(&fact.predicate, &fact.subject, &fact.object)
                .is_some()
            {
                fresh.push(fact);
                derived += 1;
            }
        }
        if derived > MAX_BINDINGS {
            return Ok(Err(exhausted()));
        }
        if fresh.is_empty() {
            return Ok(Ok(()));
        }
        for fact in &fresh {
            for (index, atoms) in eligible {
                let rule = &rules[*index];
                for (position, atom) in atoms.iter().enumerate() {
                    let Some(seed) = store.match_selected(atom, fact, &empty) else {
                        continue;
                    };
                    let rest: Vec<_> = atoms
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other != position)
                        .map(|(_, atom)| atom.clone())
                        .collect();
                    let outcome = join::walk(&rest, store, &seed, policy(), |solution| {
                        emit(store, rule, &solution, &mut pending);
                        Ok(true)
                    })?;
                    if outcome != join::Outcome::Complete {
                        return Ok(Err(exhausted()));
                    }
                }
            }
        }
    }
}

/// Settled-column values of producer variables, for proving that a producer cannot
/// write a closure member. Cached per producer, body atom and slot for one store.
#[derive(Default)]
struct Columns(std::collections::HashMap<(usize, usize, bool), Option<Vec<purrdf::TermValue>>>);

impl Columns {
    /// Whether `head` of producer `rule` may write member `(predicate, object)`:
    /// `false` only when its predicate (or, for a pair, its object) is a different
    /// constant, or a variable whose settled column holds no matching value.
    fn may_write(
        &mut self,
        (index, rule): (usize, &StatementRule),
        head: &[EvalTerm; 3],
        (member, object): (&str, Option<&str>),
        settled: (&RelationStore, &BTreeSet<String>, &Closure),
        semantics: SemanticVocabulary,
    ) -> gmeow_errors::Result<bool> {
        let predicate_matches = |value: &purrdf::TermValue| {
            constant_value_iri(value).is_some_and(|iri| semantics.predicate(iri) == member)
        };
        let writes_predicate = match &head[1] {
            EvalTerm::Var(name) => {
                !self.excludes((index, rule), name, settled, semantics, &predicate_matches)?
            }
            term => constant_iri(term).is_some_and(|iri| semantics.predicate(iri) == member),
        };
        let Some(object) = object.filter(|_| writes_predicate) else {
            return Ok(writes_predicate);
        };
        let object_matches = |value: &purrdf::TermValue| {
            constant_value_iri(value).is_some_and(|iri| semantics.analysis_symbol(iri) == object)
        };
        Ok(match &head[2] {
            EvalTerm::Var(name) => {
                !self.excludes((index, rule), name, settled, semantics, &object_matches)?
            }
            term => constant_iri(term).is_some_and(|iri| semantics.analysis_symbol(iri) == object),
        })
    }

    /// Whether some settled body atom binding `name` has no value satisfying `target`
    /// in the settled store, so no firing can bind `name` to such a value.
    fn excludes(
        &mut self,
        (index, rule): (usize, &StatementRule),
        name: &str,
        (store, immutable, closure): (&RelationStore, &BTreeSet<String>, &Closure),
        semantics: SemanticVocabulary,
        target: &dyn Fn(&purrdf::TermValue) -> bool,
    ) -> gmeow_errors::Result<bool> {
        for (position, atom) in rule.body.iter().enumerate() {
            if !Template::settled(atom, immutable, closure, semantics) {
                continue;
            }
            for (subject, term) in [(true, &atom[0]), (false, &atom[2])] {
                if !matches!(term, EvalTerm::Var(var) if var == name) {
                    continue;
                }
                let column = match self.0.entry((index, position, subject)) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        let read = EvalAtom::positive(
                            atom[0].clone(),
                            constant_iri(&atom[1])
                                .expect("settled atoms have a constant predicate"),
                            atom[2].clone(),
                        );
                        let mut values = Vec::new();
                        let outcome = join::walk(
                            std::slice::from_ref(&read),
                            store,
                            &Solution {
                                bindings: Vec::new(),
                                source_facts: Vec::new(),
                            },
                            join::Policy {
                                max_matches: MAX_BINDINGS,
                                distinct: &[],
                                retain_sources: false,
                            },
                            |solution| {
                                values.extend(solution.get(name).cloned());
                                Ok(true)
                            },
                        )?;
                        // An unfinished column proves nothing.
                        entry.insert((outcome == join::Outcome::Complete).then_some(values))
                    }
                };
                if column
                    .as_ref()
                    .is_some_and(|values| !values.iter().any(target))
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
