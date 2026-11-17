// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-bound termination refinement over proved immutable relations and the
//! exact closures of witness-free relations (see [`Closure`]). This enumerates
//! native metadata bindings, never concrete witness executions. Every possible
//! firing is covered; an exhausted analysis yields no new certificate.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::sync::Arc;

use hashbrown::HashTable;

use super::{BTreeMap, MetadataDigest, PreparedPropertyRule, SemanticVocabulary};
use crate::physical::chase::{
    ChaseAdmission, ExistentialRule, Ladder, StatementRule, firing_statements, join,
};
use crate::physical::effects::{
    ProducerEffect, StatementPattern, WorldProducerEffect,
    value_flow::{InterningCut, ValueFlow},
};
use crate::physical::store::RelationStore;
use crate::rule_ir::{EvalAtom, EvalRule, EvalTerm, Fact, Solution, fact_parts_hash};

const LIST_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const LIST_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
/// Admission-only relation `(list head, cell)`: the cell is reachable from the head
/// through `rdf:rest`. It never enters a source, effect or value-flow read.
const LIST_CELL: &str = "https://blackcatinformatics.ca/gmeow/termination/list-cell";
/// Shared row, association, match and specialization ceiling for source-specific
/// termination analysis. Pending rows count logical admitted candidate occurrences,
/// including occurrences whose exact fact payload is shared.
const MAX_BINDINGS: usize = 1 << 18;
const MAX_BYTES: usize = 128 << 20;
/// The selector rows feeding source-constant enrichment. Its interning stops at 512
/// value cells, so more rows than this cannot sharpen it further.
const ENRICHMENT_ROWS: usize = 4096;
const ENRICHMENT_BYTES: usize = 1 << 20;
/// The exact value cells source-constant enrichment may intern.
const ENRICHMENT_VALUE_CELLS: usize = 512;
/// The retained template rule metadata a cacheable joint template may carry. It bounds
/// what is retained between runs only; it never decides whether a certificate is tried.
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
            .map(StatementRule::from_ordinary);
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
        let mut closable_facts = 0usize;
        let mut selected = Vec::new();
        let mut enrichment = Some(Vec::new());
        let mut enrichment_bytes = 0usize;
        let mut selector_rows = 0usize;
        let mut retained = 0usize;
        let mut per_predicate: BTreeMap<&str, usize> = BTreeMap::new();
        let mut bytes = 0usize;
        let mut digest = MetadataDigest(purrdf_hash::blake3::Hasher::new(), 0);
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
                let mut size = MetadataDigest(purrdf_hash::blake3::Hasher::new(), 0);
                write!(size, "{world:?}:{fact:?}").expect("digest-only formatter");
                // Every selector row sharpens the source-constant enrichment, under
                // its own bound; past it the enrichment reads the immutable rows only,
                // the same coarsening the 512-cell interning cap already applies.
                enrichment_bytes = enrichment_bytes.saturating_add(size.1);
                selector_rows += 1;
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
                    closable_facts += 1;
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
            selector_rows,
            selector_bytes: enrichment_bytes,
            closable: closable_rows
                .map(|facts| Closable { candidates, facts })
                .ok_or(EvidenceGap::ClosureSourceBound {
                    facts: closable_facts,
                    bytes: closable_bytes,
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
    ) -> gmeow_errors::Result<Result<ChaseAdmission, EvidenceGap>> {
        let Settled {
            admission,
            mut notes,
        } = match self.certify_settled(evidence, facts, possible, external, semantics)? {
            Ok(settled) => settled,
            Err(reason) => return Ok(Err(reason)),
        };
        if admission.admits_native() {
            return Ok(Ok(admission));
        }
        // Derived selector edges need not be immutable to have a finite value
        // domain. Reuse the same abstract interpreter, with at most 512 exact
        // value cells. This never executes a concrete closure or lowers a rule.
        // The rows supplied to the interner are the enrichment rows when every selector row
        // was kept, else the proved-immutable rows; the note counts the ones supplied.
        let source_rows = evidence.enrichment.as_ref().unwrap_or(&evidence.facts);
        let enriched = flow.with_source_constants(
            source_rows
                .iter()
                .copied()
                .chain(possible.iter().map(|(_, fact)| fact)),
            ENRICHMENT_VALUE_CELLS,
        );
        // The source-selected metadata flow remains a sound finite refinement
        // when interning every other source value would exceed the analysis cap. Either
        // coarsening can leave a program refused that a finer flow would certify, so a
        // refusal names it.
        if evidence.enrichment.is_none() {
            notes.push(enrichment_rows_note(
                evidence.selector_rows,
                evidence.selector_bytes,
            ));
        }
        if let Err(cut) = &enriched {
            notes.push(interning_note(
                cut,
                evidence.enrichment.is_some(),
                source_rows.len(),
                possible.len(),
            ));
        }
        let enriched = enriched.as_ref().unwrap_or(flow);
        // Each bound that kept a finer analysis from completing is named on the refusal.
        let refused = |mut admission: ChaseAdmission, extra: Option<String>| {
            if let ChaseAdmission::Uncertified { bounds, .. } = &mut admission {
                for (index, note) in notes.iter().chain(extra.as_ref()).enumerate() {
                    bounds.insert(index, note.clone());
                }
            }
            admission
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
                return Ok(Ok(refused(
                    admission,
                    Some(format!(
                        "the source-enumerated second pass was not attempted: rule {} has more \
                         than {MAX_BINDINGS} value bindings",
                        rule.name
                    )),
                )));
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
                    return Ok(Ok(refused(
                        admission,
                        Some(format!(
                            "the source-enumerated second pass stopped at rule {} after {} \
                             specialized statements ({bytes} bytes): its bound is {MAX_BINDINGS} \
                             statements and {MAX_BYTES} bytes",
                            rule.name,
                            analysis.len()
                        )),
                    )));
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
        Ok(Ok(refused(
            ChaseAdmission::certify_statements(&firing, semantics, Ladder::Complete),
            None,
        )))
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
    ) -> gmeow_errors::Result<Result<Settled, EvidenceGap>> {
        // Only producers that can fire on this admitted input bear on its termination.
        let seeds = firing_seeds(facts, possible, external, semantics);
        // Close the witness-free relations first, so their final extension binds
        // variables exactly like an immutable relation. When that cannot complete
        // within its bound, join immutable relations only, and say so on refusal.
        let mut notes = Vec::new();
        let started = crate::timing::Instant::now();
        let closed = match &evidence.closable {
            Ok(closable) => self.close(evidence, closable, semantics)?,
            Err(reason) => Err(reason.clone()),
        };
        let (closure, rel, list_cells) = closed.unwrap_or_else(|reason| {
            notes.push(format!("witness-free relations were not closed: {reason}"));
            let (rel, cells) = Self::store(evidence, &[], semantics);
            (Closure::empty(), rel, cells)
        });
        // Whichever store the analysis joins, a list-cell binding that reached its bound
        // left member cells unconstrained, which is coarser and is named.
        if let Some(note) = list_cells.note() {
            notes.push(note);
        }
        let list_cells_bound = list_cells.is_bound();
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
    /// how list member cells were bound to the cells of their own lists.
    fn store(
        evidence: &Evidence<'_>,
        extra: &[&Fact],
        semantics: SemanticVocabulary,
    ) -> (RelationStore, ListCells) {
        let mut rel = RelationStore::with_semantics(semantics);
        for fact in evidence.facts.iter().chain(extra) {
            rel.insert(&fact.predicate, &fact.subject, &fact.object);
        }
        // A list operator reads members of ITS list. When list structure is proved
        // immutable, bind each member cell to the cells reachable from that list
        // head, instead of to every cell of every list.
        let proved = [LIST_FIRST, LIST_REST]
            .iter()
            .all(|predicate| evidence.predicates.contains(semantics.predicate(predicate)));
        let list_cells = if !proved {
            ListCells::Unproved
        } else {
            match bind_list_cells(&mut rel, &evidence.facts, semantics) {
                Ok(()) => ListCells::Bound,
                Err(associations) => ListCells::Cut { associations },
            }
        };
        (rel, list_cells)
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
    ) -> gmeow_errors::Result<Result<(Closure, RelationStore, ListCells), EvidenceGap>> {
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
                .map(|(index, rule)| {
                    (
                        index,
                        join_atoms(rule, &vec![true; rule.body.len()], cells.is_bound()),
                    )
                })
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
    ) -> gmeow_errors::Result<Result<Vec<StatementRule>, EvidenceGap>> {
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
            let mut projections =
                std::collections::HashSet::with_hasher(purrdf_hash::fixed::FixedState::new());
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
                return Ok(Err(EvidenceGap::SpecializationBound {
                    rule: rule.name.clone(),
                    settled_matches: matches,
                    distinct_specializations: projections.len(),
                    total_statements: analysis.len(),
                }));
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
    let mut digest = MetadataDigest(purrdf_hash::blake3::Hasher::new(), 0);
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

/// The note for source-constant enrichment that read the proved-immutable rows only.
fn enrichment_rows_note(selector_rows: usize, selector_bytes: usize) -> String {
    format!(
        "source-constant enrichment read the proved-immutable rows only: {selector_rows} \
         selector rows ({selector_bytes} bytes) exceed its {ENRICHMENT_ROWS}-row / \
         {ENRICHMENT_BYTES}-byte bound"
    )
}

/// The note for source-constant enrichment abandoned because the value universe passed its
/// cell bound, so the second pass used the coarser flow without the supplied rows. The note
/// says the bound coarsened the analysis, so a finer one might have decided; it does not say
/// the bound caused the refusal it is attached to. `cut.cells` is the universe size at the
/// cut and `cut.flow_cells` the part the flow already held before any supplied row;
/// `cut.rows_interned` counts the supplied rows the interner had begun when it stopped. When
/// it is zero the note says no row was begun; otherwise the note says the last begun row may
/// be only in part, since the cut can fall inside a row or on its boundary. `row_count` and
/// `possible` are the rows supplied: `row_count` selector rows, then `possible` possible
/// facts. `all_selector_rows` says whether those selector rows were every selector row or only the
/// proved-immutable ones.
fn interning_note(
    cut: &InterningCut,
    all_selector_rows: bool,
    row_count: usize,
    possible: usize,
) -> String {
    let rows = if all_selector_rows {
        "selector rows"
    } else {
        "proved-immutable rows"
    };
    let supplied = row_count + possible;
    let progress = if cut.rows_interned == 0 {
        format!("the interner stopped before beginning any of the {supplied} rows supplied")
    } else {
        format!(
            "the interner stopped with {} of the {supplied} rows supplied begun, the last \
             possibly only in part",
            cut.rows_interned
        )
    };
    format!(
        "source-constant enrichment was abandoned: the value universe passed its \
         {ENRICHMENT_VALUE_CELLS}-value-cell interning bound at {} cells, of which {} were \
         already held by the flow before any supplied row, and {progress} \
         ({row_count} {rows}, then {possible} possible fact(s))",
        cut.cells, cut.flow_cells
    )
}

/// Why an input's specific termination certificate was not decided. The
/// source-independent template certificate then decides alone, and a refusal
/// names this gap rather than presenting the template's ledger as the input's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EvidenceGap {
    /// No input-specific observation was supplied.
    Unobserved,
    /// The proved-immutable selector rows exceed the analysis bound.
    Bound {
        facts: usize,
        bytes: usize,
        /// The predicates retaining the most rows, heaviest first.
        heaviest: Vec<(String, usize)>,
    },
    /// Candidate closed-relation source rows exceeded the declared envelope.
    ClosureSourceBound { facts: usize, bytes: usize },
    /// A witness-free closure stopped before all joins/derived rows completed.
    ClosureBound {
        rule: Option<String>,
        committed_rows: usize,
        /// Logical admitted candidate occurrences, including shared duplicate payloads.
        pending_rows: usize,
    },
    /// Another candidate would exceed the logical pending-occurrence envelope.
    ClosurePendingBound {
        rule: String,
        committed_rows: usize,
        /// Logical admitted candidate occurrences, including shared duplicate payloads.
        pending_rows: usize,
    },
    /// Specialization stopped with its exact rule and observed inventory.
    SpecializationBound {
        rule: String,
        settled_matches: usize,
        distinct_specializations: usize,
        total_statements: usize,
    },
}

impl std::fmt::Display for EvidenceGap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unobserved => f.write_str("no input-specific evidence was observed"),
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
            Self::ClosureSourceBound { facts, bytes } => write!(
                f,
                "their source rows exceed the {MAX_BINDINGS}-row / {MAX_BYTES}-byte analysis bound ({facts} rows, {bytes} bytes observed)"
            ),
            Self::ClosureBound {
                rule,
                committed_rows,
                pending_rows,
            } => write!(
                f,
                "the closure of witness-free relations exceeded its bound of {MAX_BINDINGS} matches or derived rows (rule {rule:?}, {committed_rows} committed rows, {pending_rows} pending rows)"
            ),
            Self::ClosurePendingBound {
                rule,
                committed_rows,
                pending_rows,
            } => write!(
                f,
                "the closure of witness-free relations reached its bound of {MAX_BINDINGS} pending rows and rejected the next candidate (rule {rule:?}, {committed_rows} committed rows, {pending_rows} retained pending rows)"
            ),
            Self::SpecializationBound {
                rule,
                settled_matches,
                distinct_specializations,
                total_statements,
            } => write!(
                f,
                "the input-specific analysis exhausted its bound of {MAX_BINDINGS} specialized statements at {rule} ({settled_matches} settled matches, {distinct_specializations} distinct specializations; {total_statements} statements in total)"
            ),
        }
    }
}

// This is an inconclusive proof-analysis outcome. Genuine operational errors
// continue through the outer Diag result; callers can preserve this typed
// outcome on that same substrate when reporting an unsuccessful operation.
impl std::error::Error for EvidenceGap {}

impl gmeow_errors::DiagKind for EvidenceGap {
    fn code(&self) -> gmeow_errors::Code {
        crate::error::Physical::register()
    }
}

impl EvidenceGap {
    /// Whether an analysis bound, rather than an absent observation, left the
    /// input-specific certificate undecided.
    pub(super) fn is_bound(&self) -> bool {
        !matches!(self, Self::Unobserved)
    }
}

pub(super) struct Evidence<'a> {
    predicates: BTreeSet<String>,
    facts: Vec<&'a Fact>,
    /// Every selector row for source-constant enrichment, when within bound.
    enrichment: Option<Vec<&'a Fact>>,
    /// The selector rows and bytes the enrichment bound was measured against.
    selector_rows: usize,
    selector_bytes: usize,
    /// The candidate closed relations and their source rows, or why they exceed
    /// the analysis bound.
    closable: Result<Closable<'a>, EvidenceGap>,
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
        name: format!(
            "{}{}{ordinal}",
            rule.name,
            crate::physical::chase::SPECIALIZATION_MARK
        ),
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
        rja_exact: rule.rja_exact,
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
/// reads exactly its own cells. Returns `Err` with the association count reached, adding
/// nothing, when the rows would exceed the analysis bound: the walk stops at the first
/// component that crosses it, so the count is a lower bound on the full one. Member cells
/// then stay unconstrained, which is coarser but still covers every firing.
fn bind_list_cells(
    rel: &mut RelationStore,
    facts: &[&Fact],
    semantics: SemanticVocabulary,
) -> Result<(), usize> {
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
    if cells.is_empty() {
        return Ok(());
    }
    // Discover the union of all heads' reachable nodes once. Incoming or
    // disconnected rest-only components cannot contribute a head/cell row.
    let mut nodes = cells.clone();
    let mut pending: Vec<_> = cells.iter().copied().collect();
    while let Some(node) = pending.pop() {
        for &tail in next.get(node).into_iter().flatten() {
            if nodes.insert(tail) {
                pending.push(tail);
            }
        }
    }
    let nodes: Vec<_> = nodes.into_iter().collect();
    let index: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, &node)| (node, index))
        .collect();
    let edges: Vec<Vec<usize>> = nodes
        .iter()
        .map(|node| {
            next.get(*node)
                .into_iter()
                .flatten()
                .map(|tail| index[*tail])
                .collect()
        })
        .collect();
    let component = purrdf_core::graph::scc_component_index(&edges);
    let count = component.iter().copied().max().map_or(0, |last| last + 1);
    let mut members = vec![BTreeSet::new(); count];
    for &cell in &cells {
        let node = index[cell];
        members[component[node]].insert(node);
    }
    let mut successors = vec![BTreeSet::new(); count];
    for (node, tails) in edges.iter().enumerate() {
        for &tail in tails {
            if component[node] != component[tail] {
                successors[component[node]].insert(component[tail]);
            }
        }
    }
    // The upstream component IDs are reverse topological: every successor is
    // already settled. Shared rest-only paths reuse their immutable cell sets;
    // only a branch or a new first-cell member requires a union.
    let empty_cells = Arc::new(BTreeSet::new());
    let mut reachable = vec![empty_cells; count];
    let mut rows = 0usize;
    for current in 0..count {
        let mut known = Arc::new(members[current].clone());
        for &tail in &successors[current] {
            if known.is_subset(&reachable[tail]) {
                known = Arc::clone(&reachable[tail]);
            } else if !reachable[tail].is_subset(&known) {
                Arc::make_mut(&mut known).extend(reachable[tail].iter().copied());
            }
        }
        let total = members[current]
            .len()
            .saturating_mul(known.len())
            .saturating_add(rows);
        if total > MAX_BINDINGS {
            return Err(total);
        }
        rows = total;
        reachable[current] = known;
    }
    // Publish only after the complete association count has been admitted.
    for &head in &cells {
        for &cell in reachable[component[index[head]]].iter() {
            rel.insert(LIST_CELL, head, nodes[cell]);
        }
    }
    Ok(())
}

/// How the join store related list member cells to the cells of their own lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListCells {
    /// Each list head is bound to the cells reachable from it.
    Bound,
    /// List structure was not proved immutable, so no binding was attempted.
    Unproved,
    /// The binding reached its analysis bound and was dropped; member cells are
    /// unconstrained. `associations` is the head/cell association count at the stop.
    Cut { associations: usize },
}

impl ListCells {
    fn is_bound(self) -> bool {
        self == Self::Bound
    }

    /// The bound note for a binding that reached its bound. It says the bound coarsened
    /// the analysis, so a finer one might have decided; it does not say the bound caused
    /// the refusal it is attached to. The walk stops at the first list component that
    /// crosses the bound, so `associations` is a lower bound on the full count.
    fn note(self) -> Option<String> {
        let Self::Cut { associations } = self else {
            return None;
        };
        Some(format!(
            "list member cells were left unconstrained: binding each list head to the cells of \
             its own list stopped at {associations} head/cell associations, past its \
             {MAX_BINDINGS}-association bound (the full count is at least that)"
        ))
    }
}

/// The first-pass certificate and the analysis-bound notes a refusal must carry.
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

/// One payload per exact native fact, kept in first-seen order. Occurrence accounting
/// preserves the analysis envelope independently of how many payloads are shared.
#[derive(Default)]
struct PendingFacts {
    facts: Vec<Fact>,
    /// Borrowed-field membership only; emission follows the vector, never this index.
    index: HashTable<usize>,
    /// Admitted candidate occurrences in this batch, including duplicates.
    occurrences: usize,
}

impl PendingFacts {
    /// Count one candidate occurrence and retain its payload on first admission. A
    /// duplicate at a full bound refuses before the membership probe.
    fn admit(&mut self, fact: Fact) -> bool {
        if self.occurrences == MAX_BINDINGS {
            return false;
        }
        self.occurrences += 1;
        let hash = fact_parts_hash(&fact.subject, &fact.predicate, &fact.object);
        if self
            .index
            .find(hash, |&row| self.facts[row] == fact)
            .is_none()
        {
            let row = self.facts.len();
            self.facts.push(fact);
            let facts = &self.facts;
            self.index.insert_unique(hash, row, |&row| {
                let fact = &facts[row];
                fact_parts_hash(&fact.subject, &fact.predicate, &fact.object)
            });
        }
        true
    }
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
) -> gmeow_errors::Result<Result<(), EvidenceGap>> {
    let empty = Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    };
    let policy = || join::Policy {
        max_matches: MAX_BINDINGS,
        distinct: &[],
        retain_sources: false,
    };
    let exhausted = |rule: Option<&str>, committed_rows, pending_rows| EvidenceGap::ClosureBound {
        rule: rule.map(str::to_owned),
        committed_rows,
        pending_rows,
    };
    let emit = |store: &RelationStore,
                rule: &StatementRule,
                solution: &Solution,
                out: &mut PendingFacts| {
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
                && !out.admit(Fact {
                    subject,
                    predicate,
                    object,
                })
            {
                return false;
            }
        }
        true
    };
    let mut pending = PendingFacts::default();
    for (index, atoms) in eligible {
        let rule = &rules[*index];
        let outcome = join::walk(atoms, store, &empty, policy(), |solution| {
            Ok(emit(store, rule, &solution, &mut pending))
        })?;
        if outcome == join::Outcome::Stopped {
            return Ok(Err(EvidenceGap::ClosurePendingBound {
                rule: rule.name.clone(),
                committed_rows: 0,
                pending_rows: pending.occurrences,
            }));
        }
        if outcome != join::Outcome::Complete {
            return Ok(Err(exhausted(Some(&rule.name), 0, pending.occurrences)));
        }
    }
    let mut derived = 0usize;
    loop {
        let mut fresh = Vec::new();
        // Invalidate row handles before draining their payloads. The logical length
        // resets at this complete batch boundary.
        pending.index.clear();
        pending.occurrences = 0;
        for fact in pending.facts.drain(..) {
            if store
                .insert(&fact.predicate, &fact.subject, &fact.object)
                .is_some()
            {
                fresh.push(fact);
                derived += 1;
            }
        }
        if derived > MAX_BINDINGS {
            return Ok(Err(exhausted(None, derived, pending.occurrences)));
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
                        Ok(emit(store, rule, &solution, &mut pending))
                    })?;
                    if outcome == join::Outcome::Stopped {
                        return Ok(Err(EvidenceGap::ClosurePendingBound {
                            rule: rule.name.clone(),
                            committed_rows: derived,
                            pending_rows: pending.occurrences,
                        }));
                    }
                    if outcome != join::Outcome::Complete {
                        return Ok(Err(exhausted(
                            Some(&rule.name),
                            derived,
                            pending.occurrences,
                        )));
                    }
                }
            }
        }
    }
}

/// Settled-column values of producer variables, for proving that a producer cannot
/// write a closure member. Cached per producer, body atom and slot for one store.
#[derive(Default)]
struct Columns(
    std::collections::HashMap<
        (usize, usize, bool),
        Option<Vec<purrdf::TermValue>>,
        purrdf_hash::fixed::FixedState,
    >,
);

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
