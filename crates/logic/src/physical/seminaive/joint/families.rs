// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixed native conflict producers. Guarded statement arms are the common
//! dependency, value-flow and termination abstraction of the actual family calls.
//! Final whole-model observations do not write and do not delay positive heads.

use super::{ProvenanceMode, RoundCandidateBuffer, RoundSnapshot, record_candidate};
use crate::physical::chase::StatementRule;
use crate::physical::dependency::ReadDependency;
use crate::physical::effects::value_flow::FlowRule;
use crate::physical::effects::{ProducerEffect, StatementPattern};
use crate::reason::refute::native::{
    NativeFamilyCompletion, NativeFamilyInput, NativeFamilyLedger, NativeFamilyOutcome,
    NativeObligationScope, NativeRead, NativeReadKind, NativeRefutationFamily,
};
use crate::rule_ir::{EvalTerm, Fact};
use std::collections::{BTreeMap, BTreeSet};

mod analysis;

pub(super) const ALL: [NativeRefutationFamily; 4] = [
    NativeRefutationFamily::Cardinality,
    NativeRefutationFamily::Identity,
    NativeRefutationFamily::HasSelf,
    NativeRefutationFamily::Datatype,
];
const TYPE: &str = "https://blackcatinformatics.ca/logic/instanceOf";
const NOTHING: &str = "https://blackcatinformatics.ca/logic/Nothing";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Producer {
    Obligation(NativeRefutationFamily),
    Class,
    Admission(crate::reason::dl::DlConstructFamily),
}

#[derive(Clone, Debug)]
pub(super) struct Arm {
    pub(super) family: Producer,
    pub(super) statement: StatementRule,
    pub(super) effect: ProducerEffect,
    pub(super) flow: FlowRule,
    pub(super) preparation: Vec<NativeRead>,
}
fn v(name: &str) -> EvalTerm {
    EvalTerm::var(name)
}
fn c(iri: &str) -> EvalTerm {
    EvalTerm::named(iri)
}
fn edge(s: EvalTerm, p: &str, o: EvalTerm) -> [EvalTerm; 3] {
    [s, c(p), o]
}
fn owl(local: &str) -> String {
    format!("{OWL}{local}")
}
fn arm(
    family: NativeRefutationFamily,
    ordinal: usize,
    body: Vec<[EvalTerm; 3]>,
    extra_reads: Vec<([EvalTerm; 3], ReadDependency)>,
    implicit: Vec<NativeRead>,
) -> Arm {
    producer_arm(
        Producer::Obligation(family),
        ordinal,
        body,
        extra_reads,
        implicit,
    )
}
fn producer_arm(
    family: Producer,
    ordinal: usize,
    body: Vec<[EvalTerm; 3]>,
    extra_reads: Vec<([EvalTerm; 3], ReadDependency)>,
    implicit: Vec<NativeRead>,
) -> Arm {
    let mut implicit = implicit;
    implicit.extend(
        admission_families(family)
            .into_iter()
            .flat_map(crate::reason::dl::source_admission_reads_for),
    );
    let implicit: Vec<_> = implicit
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let name = format!("native-family:{family:?}:{ordinal}");
    let heads = vec![edge(v("x"), TYPE, c(NOTHING))];
    let mut reads: Vec<_> = body
        .iter()
        .cloned()
        .map(|atom| (Some(atom), ReadDependency::Positive))
        .collect();
    reads.extend(
        extra_reads
            .into_iter()
            .map(|(atom, kind)| (Some(atom), kind)),
    );
    let mut patterns: Vec<_> = reads
        .iter()
        .map(|(atom, kind)| {
            (
                StatementPattern::statement(atom.as_ref().expect("statement arm")),
                *kind,
            )
        })
        .collect();
    patterns.extend(implicit.iter().map(|read| {
        (
            StatementPattern::relation(read.predicate.as_deref(), read.marker.as_deref()),
            match read.kind {
                NativeReadKind::Positive => ReadDependency::Positive,
                NativeReadKind::Completed => ReadDependency::Completed,
            },
        )
    }));
    let effect = ProducerEffect::new(
        name.clone(),
        heads.iter().map(StatementPattern::statement).collect(),
        patterns,
    )
    .in_atomic_group(&format!("native-family:{family:?}"));
    let mut flow_reads: Vec<_> = reads.into_iter().map(|(atom, _)| atom).collect();
    flow_reads.extend(implicit.iter().map(|_| None));
    let preparation = implicit
        .iter()
        .filter(|read| read.kind == NativeReadKind::Completed)
        .cloned()
        .collect();
    Arm {
        family,
        preparation,
        statement: StatementRule {
            name,
            body: body.clone(),
            heads: heads.clone(),
            frontier: None,
            position_only: false,
        },
        effect,
        flow: FlowRule {
            body,
            heads,
            native_witnesses: Vec::new(),
            reads: flow_reads,
        },
    }
}

fn admission_families(family: Producer) -> Vec<crate::reason::dl::DlConstructFamily> {
    use crate::reason::dl::DlConstructFamily as F;
    match family {
        Producer::Obligation(NativeRefutationFamily::Cardinality) => vec![
            F::Cardinality,
            F::MinCardinality,
            F::MaxCardinality,
            F::QualifiedCardinality,
            F::MinQualifiedCardinality,
            F::MaxQualifiedCardinality,
            F::IntersectionOf,
        ],
        Producer::Obligation(NativeRefutationFamily::HasSelf) => vec![F::HasSelf],
        Producer::Obligation(NativeRefutationFamily::Identity) => vec![F::AllDifferent],
        Producer::Obligation(NativeRefutationFamily::Datatype) => vec![
            F::SomeValuesFrom,
            F::AllValuesFrom,
            F::Cardinality,
            F::MinCardinality,
            F::MaxCardinality,
            F::QualifiedCardinality,
            F::MinQualifiedCardinality,
            F::MaxQualifiedCardinality,
            F::OneOf,
            F::WithRestrictions,
        ],
        Producer::Class | Producer::Admission(_) => Vec::new(),
    }
}
fn reads(predicates: impl IntoIterator<Item = String>, kind: NativeReadKind) -> Vec<NativeRead> {
    predicates
        .into_iter()
        .map(|predicate| NativeRead {
            marker: None,
            predicate: Some(predicate),
            kind,
        })
        .collect()
}
/// Every positive subfamily is present independently of initial source presence.
/// Dynamic value predicates are guarded by the actual native selector roles.
pub(super) fn arms() -> Vec<Arm> {
    let count_reads = [
        "onProperty",
        "onClass",
        "onDataRange",
        "cardinality",
        "minCardinality",
        "maxCardinality",
        "qualifiedCardinality",
        "minQualifiedCardinality",
        "maxQualifiedCardinality",
        "equivalentClass",
        "intersectionOf",
    ]
    .into_iter()
    .map(owl)
    .chain([SUBCLASS.to_owned()]);
    let mut cardinality = reads(count_reads, NativeReadKind::Positive);
    cardinality.extend(reads(
        [FIRST.to_owned(), REST.to_owned()],
        NativeReadKind::Completed,
    ));
    let mut result = vec![arm(
        NativeRefutationFamily::Cardinality,
        0,
        vec![edge(v("x"), TYPE, v("class"))],
        vec![],
        cardinality,
    )];
    let same = owl("sameAs");
    let different = owl("differentFrom");
    let identity_markers = [
        "FunctionalProperty",
        "InverseFunctionalProperty",
        "AllDifferent",
    ]
    .into_iter()
    .map(|marker| NativeRead {
        predicate: Some(TYPE.to_owned()),
        marker: Some(owl(marker)),
        kind: NativeReadKind::Positive,
    })
    .collect::<Vec<_>>();
    // Both supported endpoints receive local membership; self-distinctness needs
    // no invented reflexive equality assertion.
    for (index, (s, o)) in [(v("x"), v("other")), (v("other"), v("x"))]
        .into_iter()
        .enumerate()
    {
        result.push(arm(
            NativeRefutationFamily::Identity,
            index,
            vec![edge(s.clone(), &different, o.clone()), edge(s, &same, o)],
            vec![],
            identity_markers.clone(),
        ));
    }
    result.push(arm(
        NativeRefutationFamily::Identity,
        2,
        vec![edge(v("x"), &different, v("x"))],
        vec![],
        identity_markers,
    ));
    // HasSelf reads only the property bound by the selected restriction, and
    // the same individual bound in that property assertion and class membership.
    result.push(arm(
        NativeRefutationFamily::HasSelf,
        0,
        vec![
            edge(v("restriction"), &owl("hasSelf"), v("flag")),
            edge(v("restriction"), &owl("onProperty"), v("property")),
            [v("x"), v("property"), v("x")],
            edge(v("x"), TYPE, v("class")),
        ],
        vec![],
        reads([owl("disjointWith")], NativeReadKind::Positive),
    ));
    let mut datatype = reads(
        [
            SUBCLASS.to_owned(),
            "http://www.w3.org/2000/01/rdf-schema#range".to_owned(),
            owl("onProperty"),
            owl("someValuesFrom"),
            owl("allValuesFrom"),
            owl("onDataRange"),
            owl("cardinality"),
            owl("minCardinality"),
            owl("maxCardinality"),
            owl("qualifiedCardinality"),
            owl("minQualifiedCardinality"),
            owl("maxQualifiedCardinality"),
        ],
        NativeReadKind::Positive,
    );
    datatype.extend(crate::reason::refute::datatype::preparation_reads());
    let property = edge(v("property"), TYPE, c(&owl("DatatypeProperty")));
    result.push(arm(
        NativeRefutationFamily::Datatype,
        0,
        vec![property.clone(), [v("x"), v("property"), v("value")]],
        vec![(edge(v("x"), TYPE, v("class")), ReadDependency::Positive)],
        datatype.clone(),
    ));
    // A selected existential/capacity obligation can clash before any value
    // exists. Its value read is therefore not a reachability guard.
    result.push(arm(
        NativeRefutationFamily::Datatype,
        1,
        vec![
            property,
            edge(v("x"), TYPE, v("class")),
            edge(v("restriction"), &owl("onProperty"), v("property")),
        ],
        vec![(
            [v("x"), v("property"), v("value")],
            ReadDependency::Positive,
        )],
        datatype,
    ));
    let class_reads: Vec<_> = crate::reason::refute::class_preparation_reads()
        .into_iter()
        .chain(crate::reason::refute::class_positive_reads())
        .collect();
    let mut class_guards = vec![
        edge(v("x"), TYPE, v("class")),
        [v("list"), c(FIRST), v("x")],
    ];
    for predicate in [owl("sameAs"), owl("differentFrom")] {
        class_guards.push(edge(v("x"), &predicate, v("other")));
        class_guards.push(edge(v("other"), &predicate, v("x")));
    }
    for (ordinal, guard) in class_guards.into_iter().enumerate() {
        result.push(producer_arm(
            Producer::Class,
            ordinal,
            vec![guard],
            vec![],
            class_reads.clone(),
        ));
    }
    result
}

/// Source shape completion is a typed non-writing dependency. A missing field
/// cannot erase the selector's possible downstream effect from the NAF proof.
pub(super) fn admission_arms(
    properties: &[super::super::property::PreparedPropertyRule],
) -> Vec<Arm> {
    let mut arms = Vec::new();
    for property in properties {
        for (ordinal, atom) in property.source.body.iter().enumerate() {
            let EvalTerm::ConstNamed(predicate) = &atom.0[1] else {
                continue;
            };
            let object = match &atom.0[2] {
                EvalTerm::ConstNamed(iri) => Some(purrdf::TermValue::iri(iri.clone())),
                EvalTerm::ConstLit(term) => Some(term.clone()),
                EvalTerm::Var(_) => None,
            };
            for family in crate::reason::dl::source_selector_families(predicate, object.as_ref()) {
                let name = format!(
                    "native-source-admission:{}:{ordinal}:{family:?}",
                    property.source.rule_iri
                );
                let preparation = crate::reason::dl::source_admission_reads_for(family);
                let mut reads = vec![(
                    StatementPattern::statement(&atom.0),
                    ReadDependency::Positive,
                )];
                reads.extend(preparation.iter().map(|read| {
                    (
                        StatementPattern::relation(
                            read.predicate.as_deref(),
                            read.marker.as_deref(),
                        ),
                        ReadDependency::Completed,
                    )
                }));
                let effect = ProducerEffect::new(name.clone(), Vec::new(), reads)
                    .completes(
                        property
                            .analysis_heads
                            .iter()
                            .map(|head| StatementPattern::statement(&head.0))
                            .collect(),
                    )
                    .in_atomic_group(&format!("native-source-admission:{family:?}"));
                let flow_reads = std::iter::once(Some(atom.0.clone()))
                    .chain(preparation.iter().map(|_| None))
                    .collect();
                arms.push(Arm {
                    family: Producer::Admission(family),
                    preparation,
                    effect,
                    statement: StatementRule {
                        name,
                        body: vec![atom.0.clone()],
                        heads: Vec::new(),
                        frontier: None,
                        position_only: false,
                    },
                    flow: FlowRule {
                        body: vec![atom.0.clone()],
                        heads: Vec::new(),
                        native_witnesses: Vec::new(),
                        reads: flow_reads,
                    },
                });
            }
        }
    }
    arms
}

pub(super) fn initial(ledger: &mut NativeFamilyLedger, exhausted: bool) {
    for family in ALL {
        let mut outcome = NativeFamilyOutcome::new(family, NativeObligationScope::World);
        outcome.completion = if exhausted {
            NativeFamilyCompletion::Exhausted
        } else {
            NativeFamilyCompletion::Awaiting {
                reads: vec![NativeRead {
                    marker: None,
                    predicate: None,
                    kind: NativeReadKind::Completed,
                }],
            }
        };
        ledger.outcomes.push(outcome);
    }
}

pub(super) fn evaluate(
    families: &[NativeRefutationFamily],
    input: &NativeFamilyInput<'_>,
    values: &mut super::super::property::SchemaValues,
    lists: &mut crate::physical::LogicalListCache,
    ledger: &mut NativeFamilyLedger,
) -> gmeow_errors::Result<()> {
    for family in families {
        ledger.outcomes.retain(|outcome| outcome.family != *family);
        match family {
            NativeRefutationFamily::Datatype => {
                crate::reason::refute::datatype::analyze(input, values, lists, ledger)?
            }
            _ => crate::reason::refute::counting::analyze_family(
                *family, input, values, lists, ledger,
            )?,
        }
    }
    ledger.validate()
}

pub(super) fn candidates(
    families: &[NativeRefutationFamily],
    ledger: &NativeFamilyLedger,
    snapshot: RoundSnapshot<'_>,
    round: &mut RoundCandidateBuffer,
) -> gmeow_errors::Result<()> {
    for outcome in ledger
        .outcomes
        .iter()
        .filter(|outcome| families.contains(&outcome.family))
    {
        candidates_from(&outcome.conclusions, ledger, snapshot, round)?;
    }
    Ok(())
}
fn candidates_from(
    conclusions: &[crate::reason::refute::native::NativeSupportedClash],
    ledger: &NativeFamilyLedger,
    snapshot: RoundSnapshot<'_>,
    round: &mut RoundCandidateBuffer,
) -> gmeow_errors::Result<()> {
    let proofs: BTreeMap<_, _> = ledger
        .proofs
        .iter()
        .map(|proof| (proof.id, &proof.statement))
        .collect();
    for clash in conclusions {
        let head = Fact {
            subject: clash.subject.clone(),
            predicate: TYPE.to_owned(),
            object: purrdf::TermValue::iri(NOTHING),
        };
        if snapshot.store.contains_key(&head.key()) {
            continue;
        }
        let premises = clash
            .support
            .iter()
            .map(|id| {
                let statement = proofs.get(id).ok_or_else(|| {
                    super::seminaive_err("family candidate references absent native support")
                })?;
                Ok(Fact {
                    subject: statement.subject.clone(),
                    predicate: statement.predicate.clone(),
                    object: statement.object.clone(),
                })
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?;
        round.insert(
            head.key(),
            record_candidate(&clash.rule, head, &premises, snapshot)?,
            ProvenanceMode::Record,
        )?;
    }
    Ok(())
}

pub(super) fn completed(
    predicates: &BTreeSet<String>,
    final_snapshot: bool,
) -> BTreeSet<NativeRead> {
    if final_snapshot {
        BTreeSet::from([NativeRead {
            marker: None,
            predicate: None,
            kind: NativeReadKind::Completed,
        }])
    } else {
        predicates
            .iter()
            .map(|predicate| NativeRead {
                marker: None,
                predicate: Some(predicate.clone()),
                kind: NativeReadKind::Completed,
            })
            .collect()
    }
}

/// One source-bound evidence column accompanies the existing native fact store.
pub(super) struct State {
    pub(super) ledger: NativeFamilyLedger,
    evidence: Option<crate::reason::refute::native::NativeEvidenceIndex>,
    sources: std::sync::Arc<[crate::reason::refute::RefutationPremise]>,
    domains: crate::physical::SelectedDomains,
    pub(super) coverage: crate::reason::dl::SourceCoverageWorld,
    coverage_rows: usize,
    proof_index: BTreeMap<crate::reason::refute::native::NativeProofId, usize>,
    indexed_proofs: usize,
    analyses: analysis::Observations,
    class: Option<crate::reason::refute::PreparedClassAnalysis>,
    pub(super) classes: Option<crate::reason::refute::ClassExecutionOutcome>,
    operation: super::JointOperation,
}
impl State {
    pub(super) fn new(
        binding: &super::input::NativeInputBinding<'_>,
        world: &str,
        _edb: &[Fact],
        allowance: Option<u64>,
        exhausted: bool,
        class: crate::reason::refute::PreparedClassAnalysis,
        operation: super::JointOperation,
    ) -> gmeow_errors::Result<Self> {
        let sources = binding
            .sources
            .get(world)
            .ok_or_else(|| super::seminaive_err("native world lacks original source admission"))?;
        let graph = binding
            .graphs
            .get(world)
            .ok_or_else(|| super::seminaive_err("native world lacks graph identity"))?;
        let mut ledger = NativeFamilyLedger::new(
            world.to_owned(),
            graph.clone(),
            binding.input_contract,
            allowance,
        );
        initial(&mut ledger, exhausted || allowance == Some(0));
        if operation == super::JointOperation::ClassDiagnostic {
            ledger.outcomes.clear();
        }
        let classes = crate::reason::refute::ClassExecutionOutcome {
            world: world.to_owned(),
            graph: graph.clone(),
            input_contract: binding.input_contract,
            contextual_conflicts: Vec::new(),
            obstructions: Vec::new(),
            completion: if class
                .admission()
                .selected_worlds
                .values()
                .any(|world| world.refusal.is_some())
            {
                NativeFamilyCompletion::Obstructed
            } else if exhausted || allowance == Some(0) {
                NativeFamilyCompletion::Exhausted
            } else {
                NativeFamilyCompletion::Awaiting {
                    reads: crate::reason::refute::class_completion_reads(),
                }
            },
        };
        Ok(Self {
            ledger,
            class: Some(class),
            classes: Some(classes),
            operation,
            evidence: None,
            sources: std::sync::Arc::clone(sources),
            domains: binding.domains.clone(),
            coverage: crate::reason::dl::SourceCoverageWorld {
                graph: graph.clone(),
                constructs: Vec::new(),
                admissions: Vec::new(),
            },
            coverage_rows: 0,
            proof_index: BTreeMap::new(),
            indexed_proofs: 0,
            analyses: analysis::Observations::default(),
        })
    }
    /// The relational API's input consists of native facts and an explicit world
    /// label. It asserts no RDF graph declaration and selects no logical domain.
    pub(super) fn relational(
        world: &str,
        facts: &[Fact],
        semantics: crate::native_semantics::SemanticVocabulary,
        allowance: Option<u64>,
        _exhausted: bool,
    ) -> gmeow_errors::Result<Self> {
        let sources: std::sync::Arc<[_]> = facts
            .iter()
            .map(|fact| crate::reason::refute::RefutationPremise {
                subject: fact.subject.clone(),
                predicate: fact.predicate.clone(),
                object: fact.object.clone(),
                graph: None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .into();
        let input_contract = crate::physical::metadata_identity(
            "gmeow-native-relational-input-v1",
            &(
                world,
                semantics,
                crate::reason::refute::native::NativeSourceTerms::NativeFacts,
                &sources,
            ),
        );
        let mut ledger = NativeFamilyLedger::new(world.to_owned(), None, input_contract, allowance);
        ledger.source_terms = crate::reason::refute::native::NativeSourceTerms::NativeFacts;
        Ok(Self {
            ledger,
            evidence: None,
            sources,
            domains: crate::physical::SelectedDomains::new([])?,
            class: None,
            classes: None,
            operation: super::JointOperation::Relational,
            coverage: crate::reason::dl::SourceCoverageWorld {
                graph: None,
                constructs: Vec::new(),
                admissions: Vec::new(),
            },
            coverage_rows: 0,
            proof_index: BTreeMap::new(),
            indexed_proofs: 0,
            analyses: analysis::Observations::default(),
        })
    }

    pub(super) fn index_proofs(&mut self) {
        for (index, proof) in self
            .ledger
            .proofs
            .iter()
            .enumerate()
            .skip(self.indexed_proofs)
        {
            self.proof_index.insert(proof.id, index);
        }
        self.indexed_proofs = self.ledger.proofs.len();
    }
    pub(super) fn recorded_proof(
        &self,
        id: &crate::reason::refute::native::NativeProofId,
    ) -> Option<&crate::reason::refute::native::NativeProofNode> {
        self.proof_index
            .get(id)
            .and_then(|index| self.ledger.proofs.get(*index))
    }

    pub(super) fn admit(&mut self, store: &crate::rule_ir::FactStore) -> gmeow_errors::Result<()> {
        if self.evidence.is_some() {
            return Err(super::seminaive_err(
                "native source origins were admitted twice",
            ));
        }
        self.evidence = Some(match self.operation {
            super::JointOperation::Relational => {
                crate::reason::refute::native::NativeEvidenceIndex::from_native_facts(
                    self.ledger.world.clone(),
                    self.ledger.input_contract,
                    std::sync::Arc::clone(&self.sources),
                    store,
                    &self.domains,
                )?
            }
            super::JointOperation::Forward | super::JointOperation::ClassDiagnostic => {
                crate::reason::refute::native::NativeEvidenceIndex::new(
                    self.ledger.world.clone(),
                    self.ledger.graph.clone(),
                    self.ledger.input_contract,
                    std::sync::Arc::clone(&self.sources),
                    store,
                    &self.domains,
                )?
            }
        });
        Ok(())
    }
    pub(super) fn prepare_sources(
        &mut self,
        snapshot: RoundSnapshot<'_>,
        rows: &[crate::rule_ir::DerivedRow],
        registry: &crate::physical::SkolemRegistry,
        values: &mut crate::physical::SchemaValues,
        lists: &mut crate::physical::LogicalListCache,
        complete: &BTreeSet<String>,
        observed: &BTreeSet<NativeRead>,
    ) -> gmeow_errors::Result<()> {
        let evidence = self.evidence.as_mut().ok_or_else(|| {
            super::seminaive_err("native execution has no admitted original source column")
        })?;
        evidence.observe_registry(snapshot.store, rows, registry)?;
        let mut complete = completed(complete, false);
        complete.extend(observed.iter().cloned());
        let input =
            NativeFamilyInput::new(snapshot.store, snapshot.rel, rows, evidence, &complete)?;
        for fact in snapshot.store.facts().iter().skip(self.coverage_rows) {
            crate::reason::dl::observe_construct(
                fact,
                &input,
                &mut self.ledger,
                &mut self.coverage,
            )?;
        }
        self.coverage_rows = snapshot.store.row_count();
        if self.operation != super::JointOperation::ClassDiagnostic {
            crate::reason::dl::admit_source_constructs(
                &input,
                values,
                lists,
                &mut self.ledger,
                &mut self.coverage,
            )?;
        }
        Ok(())
    }

    pub(super) fn round(
        &mut self,
        producers: &[Producer],
        snapshot: RoundSnapshot<'_>,
        rows: &[crate::rule_ir::DerivedRow],
        registry: &crate::physical::SkolemRegistry,
        values: &mut crate::physical::SchemaValues,
        lists: &mut crate::physical::LogicalListCache,
        complete: &BTreeSet<String>,
        observed: &BTreeSet<NativeRead>,
        round: &mut RoundCandidateBuffer,
    ) -> gmeow_errors::Result<()> {
        let evidence = self.evidence.as_mut().ok_or_else(|| {
            super::seminaive_err("native execution has no admitted original source column")
        })?;
        evidence.observe_registry(snapshot.store, rows, registry)?;
        let mut complete = completed(complete, false);
        complete.extend(observed.iter().cloned());
        let input =
            NativeFamilyInput::new(snapshot.store, snapshot.rel, rows, evidence, &complete)?;
        let input = input.with_admissions(&self.coverage);
        let families: Vec<_> = producers
            .iter()
            .filter_map(|producer| match producer {
                Producer::Obligation(family) => Some(*family),
                _ => None,
            })
            .collect();
        self.analyses.evaluate(
            &families,
            &input,
            &self.coverage,
            values,
            lists,
            &mut self.ledger,
        )?;
        candidates(&families, &self.ledger, snapshot, round)?;
        if producers.contains(&Producer::Class) {
            let class = self.class.as_mut().ok_or_else(|| {
                super::seminaive_err("selected class producer lacks source admission")
            })?;
            let outcome = class.evaluate(&input, &mut self.ledger)?;
            outcome.validate(&self.ledger)?;
            candidates_from(&outcome.local_conclusions(), &self.ledger, snapshot, round)?;
            self.classes = Some(outcome);
        }
        Ok(())
    }
    pub(super) fn analysis_exhausted(&self) -> bool {
        self.ledger.work.exhausted
            || self
                .ledger
                .outcomes
                .iter()
                .any(|outcome| outcome.completion == NativeFamilyCompletion::Exhausted)
            || self
                .coverage
                .admissions
                .iter()
                .any(|admission| admission.completion == NativeFamilyCompletion::Exhausted)
            || self
                .classes
                .as_ref()
                .is_some_and(|class| class.completion == NativeFamilyCompletion::Exhausted)
    }

    /// Any retained source/value-space obstruction can hide positive conclusions.
    /// Pure source vocabulary membership is not a family obstruction: each actual
    /// construct is interpreted by its selected native owner.
    pub(super) fn positive_blocked(&self, producers: &[Producer]) -> bool {
        if producers.contains(&Producer::Class)
            && self
                .classes
                .as_ref()
                .is_some_and(|classes| !classes.obstructions.is_empty())
        {
            return true;
        }
        if self.coverage.admissions.iter().any(|admission| {
            producers.contains(&Producer::Admission(admission.family))
                && (admission.completion != NativeFamilyCompletion::Complete
                    || !admission.obstructions.is_empty())
        }) {
            return true;
        }
        self.ledger.outcomes.iter().any(|outcome| {
            producers.contains(&Producer::Obligation(outcome.family))
                && !outcome.obstructions.is_empty()
        })
    }

    pub(super) fn finish(
        &mut self,
        store: &crate::rule_ir::FactStore,
        rel: &crate::physical::RelationStore,
        rows: &[crate::rule_ir::DerivedRow],
        registry: &crate::physical::SkolemRegistry,
        values: &mut crate::physical::SchemaValues,
        lists: &mut crate::physical::LogicalListCache,
        complete: &BTreeSet<String>,
        terminal: &crate::reason::refute::native::NativeClosureStatus,
    ) -> gmeow_errors::Result<()> {
        let evidence = self.evidence.as_mut().ok_or_else(|| {
            super::seminaive_err("native finalization has no original source column")
        })?;
        evidence.observe_registry(store, rows, registry)?;
        let finished = matches!(
            terminal,
            crate::reason::refute::native::NativeClosureStatus::Completed
        );
        let complete = completed(complete, finished);
        let input = NativeFamilyInput::new(store, rel, rows, evidence, &complete)?;
        for fact in store.facts().iter().skip(self.coverage_rows) {
            crate::reason::dl::observe_construct(
                fact,
                &input,
                &mut self.ledger,
                &mut self.coverage,
            )?;
        }
        self.coverage_rows = store.row_count();
        if self.operation != super::JointOperation::ClassDiagnostic {
            crate::reason::dl::admit_source_constructs(
                &input,
                values,
                lists,
                &mut self.ledger,
                &mut self.coverage,
            )?;
        }
        let input = input.with_admissions(&self.coverage);
        for row in rows.iter().filter(|row| row.cross_world.is_some()) {
            input.support(
                &[Fact {
                    subject: row.subject.clone(),
                    predicate: row.predicate.clone(),
                    object: row.object.clone(),
                }],
                &mut self.ledger,
            )?;
        }
        if finished {
            if self.operation == super::JointOperation::Forward {
                self.analyses.evaluate(
                    &ALL,
                    &input,
                    &self.coverage,
                    values,
                    lists,
                    &mut self.ledger,
                )?;
            }
            if let Some(class) = &mut self.class {
                self.classes = Some(class.evaluate(&input, &mut self.ledger)?);
            }
        } else {
            for outcome in &mut self.ledger.outcomes {
                if let NativeFamilyCompletion::Awaiting { reads } = &outcome.completion {
                    outcome.completion = match terminal {
                        crate::reason::refute::native::NativeClosureStatus::Blocked { .. } => {
                            NativeFamilyCompletion::Blocked {
                                reads: reads.clone(),
                            }
                        }
                        _ => NativeFamilyCompletion::Exhausted,
                    };
                }
            }
        }
        if let Some(classes) = &mut self.classes {
            if let NativeFamilyCompletion::Awaiting { reads } = &classes.completion {
                classes.completion = match terminal {
                    crate::reason::refute::native::NativeClosureStatus::Blocked { .. } => {
                        NativeFamilyCompletion::Blocked {
                            reads: reads.clone(),
                        }
                    }
                    crate::reason::refute::native::NativeClosureStatus::Exhausted => {
                        NativeFamilyCompletion::Exhausted
                    }
                    crate::reason::refute::native::NativeClosureStatus::Completed => {
                        return Err(super::seminaive_err(
                            "native class finalization still awaits writers",
                        ));
                    }
                };
            }
            classes.validate(&self.ledger)?;
        }
        let heads: Vec<_> = self
            .ledger
            .outcomes
            .iter()
            .enumerate()
            .flat_map(|(oi, outcome)| {
                outcome
                    .conclusions
                    .iter()
                    .enumerate()
                    .map(move |(ci, clash)| {
                        (
                            oi,
                            ci,
                            Fact {
                                subject: clash.subject.clone(),
                                predicate: TYPE.to_owned(),
                                object: purrdf::TermValue::iri(NOTHING),
                            },
                        )
                    })
            })
            .collect();
        for (oi, ci, head) in heads {
            if store.contains_key(&head.key()) {
                let support = input.support(std::slice::from_ref(&head), &mut self.ledger)?;
                self.ledger.outcomes[oi].conclusions[ci].committed = Some(support[0]);
            } else if finished {
                return Err(super::seminaive_err(
                    "native final observation found an uncommitted positive head after its declared writers completed",
                ));
            }
        }
        self.ledger.validate()
    }
}

/// Closed borrowed world evidence available to the fixed cross-world producers.
/// Recording a proof can mutate only the ledger, never the frozen native store.
pub(crate) struct NativeWorldSnapshot<'a> {
    input: NativeFamilyInput<'a>,
    ledger: &'a mut NativeFamilyLedger,
}
impl NativeWorldSnapshot<'_> {
    pub(crate) fn input(&self) -> &NativeFamilyInput<'_> {
        &self.input
    }
    pub(crate) fn support(
        &mut self,
        facts: &[Fact],
    ) -> gmeow_errors::Result<Vec<crate::reason::refute::native::NativeProofId>> {
        self.input.support(facts, self.ledger)
    }

    /// Retain the selected assessment after its complete publication commits.
    /// Existing assertions and other committed proofs can own duplicate heads;
    /// they do not erase the assessment that actually evaluated this request.
    pub(crate) fn retain_contextual(
        &mut self,
        receipt: &std::sync::Arc<crate::contextual::native::NativeContextualReceipt>,
    ) -> gmeow_errors::Result<()> {
        self.input.retain_contextual(receipt, self.ledger)
    }
}
impl State {
    /// Observe a committed round without repeating source analysis. Cross-world
    /// metadata receipts must see these exact rows before retaining their proofs.
    pub(super) fn observe_committed(
        &mut self,
        store: &crate::rule_ir::FactStore,
        rows: &[crate::rule_ir::DerivedRow],
        registry: &crate::physical::SkolemRegistry,
    ) -> gmeow_errors::Result<()> {
        self.evidence
            .as_mut()
            .ok_or_else(|| {
                super::seminaive_err("committed observation precedes native source admission")
            })?
            .observe_registry(store, rows, registry)
    }

    pub(super) fn snapshot<'a>(
        &'a mut self,
        store: &'a crate::rule_ir::FactStore,
        rel: &'a crate::physical::RelationStore,
        rows: &'a [crate::rule_ir::DerivedRow],
        complete: &'a BTreeSet<NativeRead>,
    ) -> gmeow_errors::Result<NativeWorldSnapshot<'a>> {
        let evidence = self.evidence.as_ref().ok_or_else(|| {
            super::seminaive_err("native cross-world snapshot precedes admitted evidence")
        })?;
        let input = NativeFamilyInput::new(store, rel, rows, evidence, complete)?
            .with_admissions(&self.coverage);
        Ok(NativeWorldSnapshot {
            input,
            ledger: &mut self.ledger,
        })
    }
}

#[cfg(test)]
mod contextual_finalization_tests;
