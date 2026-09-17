// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! World-local family observations over the shared native fixed point. Positive
//! conflict support is independent of writer completion and whole-model admission.

use super::{BoundKind, RefutationPremise};
use crate::physical::{WitnessDerivation, WitnessStatement, metadata_identity};
use purrdf::TermValue;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A content-addressed proof node, including its exact native world and contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NativeProofId(
    /// Full scope-bound native proof commitment.
    pub [u8; 32],
);

/// Original input, committed native inference, and intrinsic law are distinct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeProofOrigin {
    /// Original admitted statements, before deterministic native blank lowering.
    Asserted {
        /// Exact original occurrences owning this execution statement.
        sources: Vec<RefutationPremise>,
        /// Exact selected source-term normalization, never inferred from term shape.
        terms: NativeSourceTerms,
    },
    /// An actual committed inference, with references to its earlier native support.
    Derived {
        /// Exact admitted rule identity.
        rule: String,
        /// Actual committed derivation identifier.
        derivation_id: String,
        /// Ordered positive native premises.
        premises: Vec<NativeProofId>,
    },
    /// An actual context-owned modal firing supported by exact native worlds.
    Modal {
        /// Complete selected evaluation and world-qualified proof references.
        evidence: Box<crate::modal::native::NativeModalEvidence>,
    },
    /// One metadata head from an assessment retained once by this world ledger.
    Contextual {
        /// Content address of the complete typed assessment publication.
        receipt: [u8; 32],
    },
    /// A selected intrinsic domain law with its committed scoped witness.
    Intrinsic {
        /// Actual selected-domain witness receipt and minting head.
        witness: WitnessDerivation,
    },
}

/// Closed source-term admission of an actual asserted input surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeSourceTerms {
    /// RDF ingress retains original occurrences and applies native blank lowering.
    RdfSkolem,
    /// The relational operation receives already native typed facts verbatim.
    NativeFacts,
}
impl NativeSourceTerms {
    fn lower(self, term: &TermValue) -> std::borrow::Cow<'_, TermValue> {
        match self {
            Self::RdfSkolem => crate::facts::skolemize(term),
            Self::NativeFacts => std::borrow::Cow::Borrowed(term),
        }
    }
}

/// One exact committed or asserted statement in the shared proof DAG.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeProofNode {
    /// Full content-addressed identity of this scoped proof node.
    pub id: NativeProofId,
    /// Exact committed native statement proved by this node.
    pub statement: WitnessStatement,
    /// Original assertion, committed inference or intrinsic selected law.
    pub origin: NativeProofOrigin,
}

/// Registered native obligation families sharing one execution world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NativeRefutationFamily {
    /// Cardinality feasibility of populated classes and their restrictions.
    Cardinality,
    /// Distinctness against the completed shared native equality consequences.
    Identity,
    /// Disjoint self-restriction membership.
    HasSelf,
    /// Datatype value membership and capacity obligations.
    Datatype,
}

/// Exact native owner of a family obligation, before any presentation projection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeObligationScope {
    /// Whole selected-world model admission for this family.
    World,
    /// A source-owned class, datatype or restriction definition.
    Definition {
        /// Exact native definition owner.
        #[serde(with = "crate::term_serde")]
        owner: TermValue,
    },
    /// One individual's property obligations, retaining every participating restriction.
    Property {
        /// Exact native individual whose property is constrained.
        #[serde(with = "crate::term_serde")]
        individual: TermValue,
        /// Exact native property identity.
        #[serde(with = "crate::term_serde")]
        property: TermValue,
        /// All selected restriction owners, without last-write selection.
        #[serde(with = "crate::term_serde::vec")]
        restrictions: Vec<TermValue>,
    },
}

/// Every source bound survives, including a bound whose value is not admitted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeBoundEvidence {
    /// Exact native owner of the source bound.
    #[serde(with = "crate::term_serde")]
    pub owner: TermValue,
    /// Exact source predicate spelling, before role interpretation.
    pub predicate: String,
    /// Original source bound term, including its datatype and lexical identity.
    #[serde(with = "crate::term_serde")]
    pub value: TermValue,
    /// Typed interpretation of this record.
    pub kind: BoundKind,
    /// Admitted nonnegative count; None retains an uninterpreted source bound.
    pub interpreted: Option<u128>,
    /// Exact selected class or datatype qualifier, when one was admitted.
    #[serde(with = "crate::term_serde::optional")]
    pub qualifier: Option<TermValue>,
    /// Capacity is meaningful only when the native datatype plan proves it.
    pub capacity: Option<u128>,
    /// References to exact shared proof nodes supporting this record.
    pub support: Vec<NativeProofId>,
}

/// Positive support authorizes exactly this local contradiction, not ex falso.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeSupportedClash {
    /// Only this supported native resource receives the local contradiction.
    #[serde(with = "crate::term_serde")]
    pub subject: TermValue,
    /// Native semantic rule justifying this local conclusion.
    pub rule: String,
    /// Actual committed local head proof; absent for a budget-excluded candidate.
    pub committed: Option<NativeProofId>,
    /// References to exact shared proof nodes supporting this record.
    pub support: Vec<NativeProofId>,
}

/// Positive support and completed predicate extension have different scheduling contracts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NativeReadKind {
    /// Present committed support can enable a positive candidate.
    Positive,
    /// Every possible writer must close before a completeness claim.
    Completed,
}

/// Predicate roles are matched under the admitted native vocabulary; None means
/// every predicate, including a dynamically produced property relation.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NativeRead {
    /// Exact predicate role, or every dynamically available predicate.
    pub predicate: Option<String>,
    /// Exact declaration marker object, interpreted only in its predicate role.
    pub marker: Option<String>,
    /// Whether present support or final extension is required.
    pub kind: NativeReadKind,
}

impl NativeRead {
    /// Completion of one exact declaration role, independent of unrelated type memberships.
    #[must_use]
    pub fn declaration(predicate: &str, marker: &str) -> Self {
        Self {
            predicate: Some(predicate.to_owned()),
            marker: Some(marker.to_owned()),
            kind: NativeReadKind::Completed,
        }
    }
}

/// Typed source and capability failures, never semantic contradictions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NativeObstructionKind {
    /// A concrete native analysis resource bound prevents completion.
    ResourceLimit,
    /// Malformed or ambiguous selected grammar.
    SourceShape,
    /// Native value interpretation is not admitted.
    UnsupportedValue,
    /// Individually admitted constructs require an unsupported joint model.
    UnsupportedCombination,
    /// Another selected semantic feature lies outside this family model.
    UnmodeledFeature,
    /// A selected definition has not supplied its complete native structure.
    IncompleteDefinition,
}

/// A retained source/capability boundary with exact available evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeFamilyObstruction {
    /// Typed interpretation of this record.
    pub kind: NativeObstructionKind,
    /// Precise source or capability reason.
    pub detail: String,
    /// References to exact shared proof nodes supporting this record.
    pub support: Vec<NativeProofId>,
}

/// Pending writer completion is never a final coherent result. Exhaustion is a
/// terminal incomplete result; a supported positive clash can coexist with either.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeFamilyCompletion {
    /// No selected obligation exists after its possible selector writers finish.
    NotEngaged,
    /// Every declared dependency and obligation is complete.
    Complete,
    /// The current snapshot cannot yet establish model completion.
    Awaiting {
        /// Actual remaining completed-read dependencies.
        reads: Vec<NativeRead>,
    },
    /// A retained source or capability boundary prevents model completion.
    Obstructed,
    /// A selected producer obstruction prevents required completed reads.
    Blocked {
        /// Exact dependencies withheld by the obstructed positive writer.
        reads: Vec<NativeRead>,
    },
    /// The shared analysis allowance ended before this obligation completed.
    Exhausted,
}

/// Complete observation of one obligation, including simultaneous positive and unsupported evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeFamilyOutcome {
    /// Owning registered native family.
    pub family: NativeRefutationFamily,
    /// Exact selected native obligation owner.
    pub obligation: NativeObligationScope,
    /// Every source bound, including invalid or unsupported values.
    pub bounds: Vec<NativeBoundEvidence>,
    /// All independently supported local contradictions.
    pub conclusions: Vec<NativeSupportedClash>,
    /// All retained source and capability boundaries.
    pub obstructions: Vec<NativeFamilyObstruction>,
    /// Current completion state, independent of positive conclusions.
    pub completion: NativeFamilyCompletion,
}

/// Terminal state of the single native producer graph, independent of a semantic verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NativeClosureStatus {
    /// Every selected producer and required completed read has finished.
    Completed,
    /// The actual selected work or committed-head budget ended execution.
    Exhausted,
    /// A retained selected producer obstruction prevents completed extension reads.
    Blocked {
        /// Exact dependency roles withheld from downstream consumers.
        reads: Vec<NativeRead>,
    },
}

/// One shared analysis allowance, separate from the governor that commits heads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeAnalysisUsage {
    /// Selected analysis allowance; None selects unbounded analysis work.
    pub allowance: Option<u64>,
    /// Actual semantic analysis units consumed across family evaluations.
    pub consumed: u64,
    /// Whether an attempted analysis step exceeded the shared allowance.
    pub exhausted: bool,
}

/// One current native world snapshot with every family result and its shared evidence DAG.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeFamilyLedger {
    /// Exact original-source term admission shared by every asserted proof.
    pub source_terms: NativeSourceTerms,
    /// Selected execution world owning this ledger.
    pub world: String,
    /// Original native graph identity; None denotes the selected default context.
    #[serde(with = "crate::term_serde::optional")]
    pub graph: Option<TermValue>,
    /// Exact source, selection and native semantic admission commitment.
    pub input_contract: [u8; 32],
    /// Every current family and obligation result.
    pub outcomes: Vec<NativeFamilyOutcome>,
    /// Shared proof DAG in dependency order.
    pub proofs: Vec<NativeProofNode>,
    /// Complete contextual assessments, shared by their metadata proof nodes.
    pub contextual_receipts: Vec<crate::contextual::native::NativeContextualReceipt>,
    /// Shared analysis usage, separate from committed-head budgeting.
    pub work: NativeAnalysisUsage,
}

use crate::physical::RelationStore;
#[cfg(test)]
use crate::physical::{LogicalListCache, SchemaValues};
use crate::rule_ir::{DerivedRow, Fact, FactStore};
use std::sync::Arc;

type Result<T> = gmeow_errors::Result<T>;
fn invalid(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.into(),
    })
}

impl NativeFamilyLedger {
    /// Empty retained observations for one authenticated selected native world.
    #[must_use]
    pub fn new(
        world: String,
        graph: Option<TermValue>,
        input_contract: [u8; 32],
        allowance: Option<u64>,
    ) -> Self {
        Self {
            source_terms: NativeSourceTerms::RdfSkolem,
            world,
            graph,
            input_contract,
            outcomes: Vec::new(),
            proofs: Vec::new(),
            contextual_receipts: Vec::new(),
            work: NativeAnalysisUsage {
                allowance,
                consumed: 0,
                exhausted: false,
            },
        }
    }

    /// A supported local contradiction does not require every other obligation to finish.
    #[must_use]
    pub fn has_conflict(&self) -> bool {
        self.outcomes
            .iter()
            .any(|outcome| !outcome.conclusions.is_empty())
    }

    /// All engaged obligations must finish before absence of a clash is conclusive.
    #[must_use]
    pub fn complete(&self) -> bool {
        !self.work.exhausted
            && self.outcomes.iter().all(|outcome| {
                matches!(
                    outcome.completion,
                    NativeFamilyCompletion::Complete | NativeFamilyCompletion::NotEngaged
                )
            })
    }

    fn proof_id(&self, statement: &WitnessStatement, origin: &NativeProofOrigin) -> NativeProofId {
        NativeProofId(metadata_identity(
            "gmeow-native-family-proof-v1",
            &(
                &self.world,
                &self.graph,
                self.input_contract,
                statement,
                origin,
            ),
        ))
    }

    /// Validate intrinsic framing of an authenticated ledger. This validates proof
    /// links and source roles, not the authenticity of an untrusted caller's bytes.
    pub fn validate(&self) -> Result<()> {
        if self.world.is_empty()
            || self
                .work
                .allowance
                .is_some_and(|limit| self.work.consumed > limit)
        {
            return Err(invalid("invalid native family scope or analysis usage"));
        }
        let mut seen = BTreeSet::new();
        let mut statements = BTreeMap::<NativeProofId, &WitnessStatement>::new();
        if self
            .contextual_receipts
            .windows(2)
            .any(|pair| pair[0].id >= pair[1].id)
        {
            return Err(invalid(
                "contextual receipts are not uniquely content-addressed",
            ));
        }
        for receipt in &self.contextual_receipts {
            receipt.validate()?;
            if receipt.input_contract != self.input_contract
                || self.world != crate::result_rdf::GRAPH_REASONING
            {
                return Err(invalid(
                    "contextual receipt belongs to another native world or contract",
                ));
            }
        }
        let published: BTreeSet<_> = self.proofs.iter().map(|proof| &proof.statement).collect();
        for proof in &self.proofs {
            if self.proof_id(&proof.statement, &proof.origin) != proof.id
                || seen.contains(&proof.id)
            {
                return Err(invalid("duplicate or altered native family proof identity"));
            }
            match &proof.origin {
                NativeProofOrigin::Asserted { sources, terms } => {
                    if *terms != self.source_terms
                        || sources.is_empty()
                        || sources.windows(2).any(|pair| pair[0] >= pair[1])
                        || sources.iter().any(|source| {
                            source.graph != self.graph
                                || terms.lower(&source.subject).as_ref() != &proof.statement.subject
                                || source.predicate != proof.statement.predicate
                                || terms.lower(&source.object).as_ref() != &proof.statement.object
                        })
                    {
                        return Err(invalid(
                            "native family assertion does not name its exact original source",
                        ));
                    }
                }
                NativeProofOrigin::Derived {
                    rule,
                    derivation_id,
                    premises,
                } => {
                    if rule.is_empty()
                        || rule == crate::provenance::ASSERT_RULE_IRI
                        || derivation_id.is_empty()
                        || premises.iter().any(|id| !seen.contains(id))
                    {
                        return Err(invalid(
                            "native family derivation has missing or forward proof support",
                        ));
                    }
                    let reifiers = premises
                        .iter()
                        .map(|id| {
                            let source = statements[id];
                            crate::provenance::mint_reifier(
                                &source.subject,
                                &source.predicate,
                                &source.object,
                            )
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let refs: Vec<_> = reifiers.iter().map(String::as_str).collect();
                    if crate::provenance::mint_derivation_id(rule, &refs) != *derivation_id {
                        return Err(invalid(
                            "native family derivation identifier does not bind its actual premises",
                        ));
                    }
                }
                NativeProofOrigin::Modal { evidence } => {
                    if evidence.input_contract != self.input_contract {
                        return Err(invalid(
                            "modal proof belongs to another native input contract",
                        ));
                    }
                    evidence.validate_structure(&self.world, &proof.statement)?;
                }
                NativeProofOrigin::Contextual { receipt } => {
                    let index = self
                        .contextual_receipts
                        .binary_search_by_key(receipt, |receipt| receipt.id)
                        .map_err(|_| {
                            invalid("contextual proof names an absent assessment receipt")
                        })?;
                    self.contextual_receipts[index].validate_head(&self.world, &proof.statement)?;
                }
                NativeProofOrigin::Intrinsic { witness } => {
                    witness.validate()?;
                    if !matches!(
                        &witness.scope.origin,
                        crate::physical::WitnessOrigin::NonemptyDomain(_)
                    ) || witness.scope.world != self.world
                        || !witness
                            .heads
                            .iter()
                            .any(|head| head.statement == proof.statement)
                    {
                        return Err(invalid(
                            "native intrinsic proof does not own its committed head and world",
                        ));
                    }
                }
            }
            seen.insert(proof.id);
            statements.insert(proof.id, &proof.statement);
        }
        for receipt in &self.contextual_receipts {
            if receipt
                .statements
                .iter()
                .any(|statement| !published.contains(statement))
            {
                return Err(invalid(
                    "native ledger omits part of a contextual metadata publication",
                ));
            }
        }
        for outcome in &self.outcomes {
            if matches!(
                outcome.completion,
                NativeFamilyCompletion::Complete | NativeFamilyCompletion::NotEngaged
            ) && !outcome.obstructions.is_empty()
            {
                return Err(invalid("obstructed native family cannot claim completion"));
            }
            if matches!(&outcome.completion, NativeFamilyCompletion::Awaiting { reads } | NativeFamilyCompletion::Blocked { reads } if reads.is_empty())
            {
                return Err(invalid(
                    "pending native family must identify unfinished writer reads",
                ));
            }
            let supports = outcome
                .bounds
                .iter()
                .map(|bound| &bound.support)
                .chain(outcome.conclusions.iter().map(|clash| &clash.support))
                .chain(
                    outcome
                        .obstructions
                        .iter()
                        .map(|obstruction| &obstruction.support),
                );
            for support in supports {
                if support.iter().any(|id| !seen.contains(id)) {
                    return Err(invalid(
                        "native family outcome references an absent proof node",
                    ));
                }
            }
            for clash in &outcome.conclusions {
                if let Some(id) = clash.committed {
                    let head = statements
                        .get(&id)
                        .ok_or_else(|| invalid("native clash commitment has no proof"))?;
                    if head.subject != clash.subject
                        || head.predicate != "https://blackcatinformatics.ca/logic/instanceOf"
                        || head.object
                            != TermValue::iri("https://blackcatinformatics.ca/logic/Nothing")
                    {
                        return Err(invalid(
                            "native clash commitment does not prove its exact local head",
                        ));
                    }
                }
            }
            if outcome
                .conclusions
                .iter()
                .any(|clash| clash.support.is_empty() || clash.rule.is_empty())
            {
                return Err(invalid(
                    "a local native conflict requires actual support and a rule",
                ));
            }
        }
        Ok(())
    }

    /// Resolve only original assertion leaves of already-accounted proof nodes.
    /// Intrinsic law evidence has no RDF assertion leaf.
    pub fn source_leaves(&self, roots: &[NativeProofId]) -> Result<Vec<RefutationPremise>> {
        let nodes: BTreeMap<_, _> = self.proofs.iter().map(|node| (node.id, node)).collect();
        let mut pending = roots.to_vec();
        let mut visited = BTreeSet::new();
        let mut sources = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let node = nodes
                .get(&id)
                .ok_or_else(|| invalid("source support references an absent native proof"))?;
            match &node.origin {
                NativeProofOrigin::Asserted {
                    sources: leaves, ..
                } => sources.extend(leaves.iter().cloned()),
                NativeProofOrigin::Derived { premises, .. } => pending.extend(premises),
                // Foreign modal leaves remain world-qualified native proof IDs.
                // The global evidence owner validates their closed source DAG.
                NativeProofOrigin::Modal { .. }
                | NativeProofOrigin::Contextual { .. }
                | NativeProofOrigin::Intrinsic { .. } => {}
            }
        }
        Ok(sources.into_iter().collect())
    }

    pub(crate) fn charge(&mut self, amount: u64) -> bool {
        if self.work.exhausted {
            return false;
        }
        let Some(next) = self.work.consumed.checked_add(amount) else {
            self.work.exhausted = true;
            return false;
        };
        if self.work.allowance.is_some_and(|limit| next > limit) {
            self.work.exhausted = true;
            return false;
        }
        self.work.consumed = next;
        true
    }
}

impl NativeFamilyOutcome {
    pub(crate) fn new(family: NativeRefutationFamily, obligation: NativeObligationScope) -> Self {
        Self {
            family,
            obligation,
            bounds: Vec::new(),
            conclusions: Vec::new(),
            obstructions: Vec::new(),
            completion: NativeFamilyCompletion::Complete,
        }
    }
    pub(crate) fn obstruct(
        &mut self,
        kind: NativeObstructionKind,
        detail: impl Into<String>,
        support: Vec<NativeProofId>,
    ) {
        self.obstructions.push(NativeFamilyObstruction {
            kind,
            detail: detail.into(),
            support,
        });
        self.completion = NativeFamilyCompletion::Obstructed;
    }
    pub(crate) fn finish(
        &mut self,
        input: &NativeFamilyInput<'_>,
        reads: Vec<NativeRead>,
        ledger: &NativeFamilyLedger,
    ) {
        if ledger.work.exhausted {
            self.completion = NativeFamilyCompletion::Exhausted;
        } else if !self.obstructions.is_empty() {
            self.completion = NativeFamilyCompletion::Obstructed;
        } else {
            let mut required = reads;
            if let NativeFamilyCompletion::Awaiting { reads } = &self.completion {
                required.extend(reads.iter().cloned());
            }
            let pending: Vec<_> = required
                .into_iter()
                .filter(|read| {
                    !input.defer_world_completion
                        || read.kind != NativeReadKind::Completed
                        || read.predicate.is_some()
                        || read.marker.is_some()
                })
                .filter(|read| !input.completed(read))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            if !pending.is_empty() {
                self.completion = NativeFamilyCompletion::Awaiting { reads: pending };
            }
        }
    }
}

#[derive(Clone)]
enum Origin {
    Asserted(Vec<usize>),
    Derived(usize),
    Intrinsic(WitnessDerivation),
}

/// One run-local origin column alongside the native FactStore. Only original input
/// admission and committed native rows can populate it; there is no proof callback.
pub(crate) struct NativeEvidenceIndex {
    world: String,
    graph: Option<TermValue>,
    input_contract: [u8; 32],
    intrinsic_rules: BTreeSet<String>,
    sources: Arc<[RefutationPremise]>,
    origins: Vec<Option<Origin>>,
    observed_rows: usize,
    original_rows: usize,
    terms: NativeSourceTerms,
    contextual_receipts:
        BTreeMap<[u8; 32], Arc<crate::contextual::native::NativeContextualReceipt>>,
    recorded: std::cell::RefCell<BTreeMap<usize, (NativeProofId, usize)>>,
}

impl NativeEvidenceIndex {
    pub(crate) fn new(
        world: String,
        graph: Option<TermValue>,
        input_contract: [u8; 32],
        sources: Arc<[RefutationPremise]>,
        store: &FactStore,
        domains: &crate::physical::SelectedDomains,
    ) -> Result<Self> {
        Self::with_terms(
            world,
            graph,
            input_contract,
            sources,
            store,
            domains,
            NativeSourceTerms::RdfSkolem,
        )
    }

    pub(crate) fn from_native_facts(
        world: String,
        input_contract: [u8; 32],
        sources: Arc<[RefutationPremise]>,
        store: &FactStore,
        domains: &crate::physical::SelectedDomains,
    ) -> Result<Self> {
        Self::with_terms(
            world,
            None,
            input_contract,
            sources,
            store,
            domains,
            NativeSourceTerms::NativeFacts,
        )
    }

    fn with_terms(
        world: String,
        graph: Option<TermValue>,
        input_contract: [u8; 32],
        sources: Arc<[RefutationPremise]>,
        store: &FactStore,
        domains: &crate::physical::SelectedDomains,
        terms: NativeSourceTerms,
    ) -> Result<Self> {
        let intrinsic_rules = domains.worlds().iter().filter_map(|domain| match domain.world() {
            Ok(selected) if selected == world => Some(if domain.graph().graph() == graph.as_ref() {
                Ok(domain.rule().rule_iri)
            } else { Err(invalid("selected intrinsic domain and source evidence disagree on native graph identity")) }),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        }).collect::<Result<BTreeSet<_>>>()?;
        let mut origins = vec![None; store.row_count()];
        for (index, source) in sources.iter().enumerate() {
            if source.graph != graph {
                return Err(invalid(
                    "source occurrence belongs to another selected world",
                ));
            }
            let fact = Fact {
                subject: terms.lower(&source.subject).into_owned(),
                predicate: source.predicate.clone(),
                object: terms.lower(&source.object).into_owned(),
            };
            let row = store.row_index(&fact.key()).ok_or_else(|| {
                invalid("source occurrence is absent from the admitted native input")
            })?;
            match &mut origins[row] {
                Some(Origin::Asserted(indices)) => indices.push(index),
                None => origins[row] = Some(Origin::Asserted(vec![index])),
                _ => return Err(invalid("source admission followed native inference")),
            }
        }
        if origins.iter().any(Option::is_none) {
            return Err(invalid(
                "native input row lacks its original source occurrence",
            ));
        }
        Ok(Self {
            world,
            graph,
            input_contract,
            intrinsic_rules,
            sources,
            origins,
            terms,
            original_rows: store.row_count(),
            observed_rows: 0,
            contextual_receipts: BTreeMap::new(),
            recorded: Default::default(),
        })
    }

    /// Account only the committed suffix. Original assertion echoes are verified,
    /// never promoted to fresh assertions; budget-excluded candidates cannot enter.
    pub(crate) fn observe(
        &mut self,
        store: &FactStore,
        rows: &[DerivedRow],
        witnesses: &[WitnessDerivation],
    ) -> Result<()> {
        if rows.len() < self.observed_rows || store.row_count() < self.origins.len() {
            return Err(invalid(
                "native evidence input moved behind its committed prefix",
            ));
        }
        self.origins.resize(store.row_count(), None);
        for (index, row) in rows.iter().enumerate().skip(self.observed_rows) {
            if !row.graph.is_empty() && row.graph != self.world {
                return Err(invalid("committed inference belongs to another world"));
            }
            let fact = Fact {
                subject: row.subject.clone(),
                predicate: row.predicate.clone(),
                object: row.object.clone(),
            };
            let slot = store
                .row_index(&fact.key())
                .ok_or_else(|| invalid("uncommitted head cannot justify a family proof"))?;
            if self.origins[slot].is_some() {
                continue;
            }
            if row.rule_iri == crate::provenance::ASSERT_RULE_IRI || row.derivation_id.is_empty() {
                return Err(invalid(
                    "derived native fact has no retained inference evidence",
                ));
            }
            if let Some(cross_world) = &row.cross_world {
                use crate::modal::native::NativeCrossWorldEvidence;
                let (contract, rule, sources, derivation) = match cross_world {
                    NativeCrossWorldEvidence::Modal(evidence) => {
                        evidence.validate_structure(&self.world, &WitnessStatement::from(&fact))?;
                        (
                            evidence.input_contract,
                            crate::modal::MODAL_RULE_IRI,
                            evidence
                                .evaluation
                                .positive_premises()
                                .iter()
                                .map(crate::modal::ModalPremise::triple_id)
                                .collect::<Vec<_>>(),
                            evidence.evaluation.derivation_id(),
                        )
                    }
                    NativeCrossWorldEvidence::Contextual(receipt) => {
                        match self.contextual_receipts.entry(receipt.id) {
                            std::collections::btree_map::Entry::Vacant(entry) => {
                                receipt.validate()?;
                                entry.insert(Arc::clone(receipt));
                            }
                            std::collections::btree_map::Entry::Occupied(entry) => {
                                if !Arc::ptr_eq(entry.get(), receipt)
                                    && entry.get().as_ref() != receipt.as_ref()
                                {
                                    return Err(invalid(
                                        "contextual receipt identity aliases different assessment evidence",
                                    ));
                                }
                            }
                        }
                        receipt.validate_head(&self.world, &WitnessStatement::from(&fact))?;
                        (
                            receipt.input_contract,
                            crate::contextual::RULE_IRI,
                            receipt.source_quad_ids(),
                            receipt.derivation_id(),
                        )
                    }
                };
                if contract != self.input_contract
                    || !row.antecedents.is_empty()
                    || row.rule_iri != rule
                    || row.source_quad_ids != sources
                    || row.derivation_id != derivation
                {
                    return Err(invalid(
                        "committed modal firing lost its exact contextual evidence",
                    ));
                }
                self.origins[slot] = Some(Origin::Derived(index));
                continue;
            }
            let reifiers = row
                .antecedents
                .iter()
                .map(Fact::reifier)
                .collect::<Result<Vec<_>>>()?;
            let refs: Vec<_> = reifiers.iter().map(String::as_str).collect();
            if reifiers != row.source_quad_ids
                || crate::provenance::mint_derivation_id(&row.rule_iri, &refs) != row.derivation_id
            {
                return Err(invalid(
                    "native committed row lost its exact positive premise identity",
                ));
            }
            for premise in &row.antecedents {
                let premise = store
                    .row_index(&premise.key())
                    .ok_or_else(|| invalid("committed premise is absent"))?;
                if self.origins[premise].is_none() {
                    return Err(invalid("committed premise has no supported origin"));
                }
            }
            let intrinsic = witnesses.iter().find(|witness| {
                matches!(
                    &witness.scope.origin,
                    crate::physical::WitnessOrigin::NonemptyDomain(_)
                ) && witness.scope.world == self.world
                    && witness.rule_iri == row.rule_iri
                    && witness.heads.iter().any(|head| {
                        head.statement == WitnessStatement::from(&fact)
                            && head.derivation_id == row.derivation_id
                    })
            });
            if self.intrinsic_rules.contains(&row.rule_iri) != intrinsic.is_some() {
                return Err(invalid(
                    "intrinsic domain inference requires its exact selected committed witness receipt",
                ));
            }
            self.origins[slot] = Some(if let Some(witness) = intrinsic {
                witness.validate()?;
                Origin::Intrinsic(witness.clone())
            } else {
                Origin::Derived(index)
            });
        }
        self.observed_rows = rows.len();
        if self.origins.iter().any(Option::is_none) {
            return Err(invalid("native closure contains an unaccounted row"));
        }
        Ok(())
    }

    /// Borrow the one shared registry and retain only receipts needed by the new
    /// intrinsic suffix. No whole-registry snapshot is cloned per round.
    pub(crate) fn observe_registry(
        &mut self,
        store: &FactStore,
        rows: &[DerivedRow],
        registry: &crate::physical::SkolemRegistry,
    ) -> Result<()> {
        let mut witnesses = Vec::new();
        for row in rows
            .iter()
            .skip(self.observed_rows)
            .filter(|row| self.intrinsic_rules.contains(&row.rule_iri))
        {
            for term in [&row.subject, &row.object] {
                if let Some(iri) = term.as_iri()
                    && let Some(receipt) = registry.explain(iri)
                {
                    witnesses.push(receipt);
                }
            }
        }
        self.observe(store, rows, &witnesses)
    }

    fn proof(
        &self,
        slot: usize,
        input: &NativeFamilyInput<'_>,
        ledger: &mut NativeFamilyLedger,
        recorded: &mut BTreeMap<usize, (NativeProofId, usize)>,
        active: &mut BTreeSet<usize>,
    ) -> Result<NativeProofId> {
        if let Some((id, index)) = recorded.get(&slot)
            && ledger
                .proofs
                .get(*index)
                .is_some_and(|proof| proof.id == *id)
        {
            return Ok(*id);
        }
        if !active.insert(slot) {
            return Err(invalid("native proof dependency cycle"));
        }
        let fact = &input.store.facts()[slot];
        let origin = match self
            .origins
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or_else(|| invalid("missing native proof origin"))?
        {
            Origin::Asserted(indices) => {
                let mut sources: Vec<_> = indices
                    .iter()
                    .map(|index| self.sources[*index].clone())
                    .collect();
                sources.sort();
                sources.dedup();
                NativeProofOrigin::Asserted {
                    sources,
                    terms: self.terms,
                }
            }
            Origin::Derived(index) => {
                let row = input
                    .rows
                    .get(*index)
                    .ok_or_else(|| invalid("missing committed native derivation"))?;
                if let Some(evidence) = &row.cross_world {
                    match evidence {
                        crate::modal::native::NativeCrossWorldEvidence::Modal(evidence) => {
                            NativeProofOrigin::Modal {
                                evidence: evidence.clone(),
                            }
                        }
                        crate::modal::native::NativeCrossWorldEvidence::Contextual(receipt) => {
                            match ledger
                                .contextual_receipts
                                .binary_search_by_key(&receipt.id, |receipt| receipt.id)
                            {
                                Ok(index) => ledger.contextual_receipts[index]
                                    .validate_head(&self.world, &WitnessStatement::from(fact))?,
                                Err(index) => ledger
                                    .contextual_receipts
                                    .insert(index, receipt.as_ref().clone()),
                            }
                            NativeProofOrigin::Contextual {
                                receipt: receipt.id,
                            }
                        }
                    }
                } else {
                    let mut premises = Vec::new();
                    for fact in &row.antecedents {
                        let slot = input
                            .store
                            .row_index(&fact.key())
                            .ok_or_else(|| invalid("missing native derivation premise"))?;
                        premises.push(self.proof(slot, input, ledger, recorded, active)?);
                    }
                    NativeProofOrigin::Derived {
                        rule: row.rule_iri.clone(),
                        derivation_id: row.derivation_id.clone(),
                        premises,
                    }
                }
            }
            Origin::Intrinsic(witness) => NativeProofOrigin::Intrinsic {
                witness: witness.clone(),
            },
        };
        let statement = WitnessStatement::from(fact);
        let id = ledger.proof_id(&statement, &origin);
        let index = ledger.proofs.len();
        ledger.proofs.push(NativeProofNode {
            id,
            statement,
            origin,
        });
        active.remove(&slot);
        recorded.insert(slot, (id, index));
        Ok(id)
    }
}

/// Frozen current native state and the scheduler's proven completed reads. The
/// completion set is an internal effect-graph result, never a public boolean flag.
pub(crate) struct NativeFamilyInput<'a> {
    pub(crate) store: &'a FactStore,
    pub(crate) rel: &'a RelationStore,
    pub(crate) rows: &'a [DerivedRow],
    evidence: &'a NativeEvidenceIndex,
    completed_reads: &'a BTreeSet<NativeRead>,
    admissions: Option<&'a crate::reason::dl::SourceCoverageWorld>,
    defer_world_completion: bool,
}

impl<'a> NativeFamilyInput<'a> {
    pub(crate) fn new(
        store: &'a FactStore,
        rel: &'a RelationStore,
        rows: &'a [DerivedRow],
        evidence: &'a NativeEvidenceIndex,
        completed_reads: &'a BTreeSet<NativeRead>,
    ) -> Result<Self> {
        if evidence.origins.len() != store.row_count() || evidence.observed_rows != rows.len() {
            return Err(invalid(
                "native family snapshot lacks its complete committed evidence prefix",
            ));
        }
        Ok(Self {
            store,
            rel,
            rows,
            evidence,
            completed_reads,
            admissions: None,
            defer_world_completion: false,
        })
    }
    /// Bind the source-preparation phase on the same frozen state. There is no
    /// executable property interpretation without its actual owner admission.
    pub(crate) fn with_admissions(
        mut self,
        admissions: &'a crate::reason::dl::SourceCoverageWorld,
    ) -> Self {
        self.admissions = Some(admissions);
        self
    }

    /// Compute a reusable family analysis before observing whole-world closure.
    /// Only the outcome's final wildcard observation is deferred; constructor,
    /// list and source-admission readers still use the real completed frontier.
    pub(crate) fn defer_world_completion(&self) -> Self {
        Self {
            store: self.store,
            rel: self.rel,
            rows: self.rows,
            evidence: self.evidence,
            completed_reads: self.completed_reads,
            admissions: self.admissions,
            defer_world_completion: true,
        }
    }

    /// Admit the actual source selector before a family interprets its fields.
    /// The shared source phase owns malformed/unfinished shape evidence.
    pub(crate) fn admit_selector(
        &self,
        selector: &Fact,
        output: &mut NativeFamilyOutcome,
        ledger: &mut NativeFamilyLedger,
    ) -> Result<bool> {
        let mut admitted = true;
        for family in
            crate::reason::dl::source_selector_families(&selector.predicate, Some(&selector.object))
        {
            if let Some(issue) = crate::reason::dl::source_operand_issue(
                family,
                &selector.predicate,
                &selector.subject,
                &selector.object,
            ) {
                output.obstruct(
                    NativeObstructionKind::SourceShape,
                    issue,
                    self.support(std::slice::from_ref(selector), ledger)?,
                );
                admitted = false;
            }
            if !crate::reason::dl::source_execution_requires_admission(family, &selector.predicate)
            {
                continue;
            }
            let world = self.admissions.ok_or_else(|| {
                invalid("native property interpretation preceded source admission")
            })?;
            let admission = world
                .admission(family, &selector.subject)
                .ok_or_else(|| invalid("native selector has no retained owner admission"))?;
            if admission.completion != NativeFamilyCompletion::Complete
                || !admission.obstructions.is_empty()
            {
                admitted = false;
                output
                    .obstructions
                    .extend(admission.obstructions.iter().cloned());
                match &admission.completion {
                    NativeFamilyCompletion::Awaiting { reads } => {
                        let mut pending = reads.clone();
                        if let NativeFamilyCompletion::Awaiting { reads } = &output.completion {
                            pending.extend(reads.iter().cloned());
                        }
                        output.completion = NativeFamilyCompletion::Awaiting {
                            reads: pending
                                .into_iter()
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .collect(),
                        };
                    }
                    other => output.completion = other.clone(),
                }
            }
        }
        Ok(admitted)
    }

    pub(crate) fn original_row_count(&self) -> usize {
        self.evidence.original_rows
    }
    pub(crate) fn world(&self) -> &str {
        &self.evidence.world
    }
    pub(crate) fn graph(&self) -> Option<&TermValue> {
        self.evidence.graph.as_ref()
    }
    pub(crate) fn input_contract(&self) -> &[u8; 32] {
        &self.evidence.input_contract
    }

    pub(crate) fn completed(&self, read: &NativeRead) -> bool {
        if read.kind == NativeReadKind::Positive {
            return true;
        }
        self.completed_reads.iter().any(|done| {
            if done.kind != NativeReadKind::Completed {
                return false;
            }
            let Some(predicate) = done.predicate.as_deref() else {
                return done.marker.is_none();
            };
            let Some(needed) = read.predicate.as_deref() else {
                return false;
            };
            if self.rel.semantics.predicate(predicate) != self.rel.semantics.predicate(needed) {
                return false;
            }
            match (done.marker.as_deref(), read.marker.as_deref()) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(left), Some(right)) => {
                    left == right
                        || self.rel.semantics.alternate_marker(predicate, left) == Some(right)
                }
            }
        })
    }
    pub(crate) fn support(
        &self,
        facts: &[Fact],
        ledger: &mut NativeFamilyLedger,
    ) -> Result<Vec<NativeProofId>> {
        if ledger.world != self.evidence.world
            || ledger.graph != self.evidence.graph
            || ledger.input_contract != self.evidence.input_contract
            || ledger.source_terms != self.evidence.terms
        {
            return Err(invalid(
                "native family ledger and source admission have different scope",
            ));
        }
        let mut recorded = self.evidence.recorded.borrow_mut();
        let mut active = BTreeSet::new();
        let mut ids = Vec::new();
        for fact in facts {
            if !self
                .rel
                .contains(&fact.predicate, &fact.subject, &fact.object)
            {
                return Err(invalid(
                    "family premise is not an exact current native statement",
                ));
            }
            let slot = self
                .store
                .row_index(&fact.key())
                .ok_or_else(|| invalid("family premise has no FactStore support"))?;
            ids.push(
                self.evidence
                    .proof(slot, self, ledger, &mut recorded, &mut active)?,
            );
        }
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    /// Authenticate the complete assessment publication against this committed
    /// store, including heads already owned by original assertions.
    pub(crate) fn retain_contextual(
        &self,
        receipt: &Arc<crate::contextual::native::NativeContextualReceipt>,
        ledger: &mut NativeFamilyLedger,
    ) -> Result<()> {
        receipt.validate()?;
        if ledger.world != crate::result_rdf::GRAPH_REASONING
            || ledger.input_contract != receipt.input_contract
        {
            return Err(invalid(
                "contextual publication belongs to another native world or contract",
            ));
        }
        for statement in &receipt.statements {
            let fact = Fact {
                subject: statement.subject.clone(),
                predicate: statement.predicate.clone(),
                object: statement.object.clone(),
            };
            self.support(std::slice::from_ref(&fact), ledger)?;
        }
        match ledger
            .contextual_receipts
            .binary_search_by_key(&receipt.id, |receipt| receipt.id)
        {
            Ok(index) if ledger.contextual_receipts[index] != **receipt => {
                return Err(invalid(
                    "contextual receipt identity aliases different assessment evidence",
                ));
            }
            Ok(_) => {}
            Err(index) => ledger
                .contextual_receipts
                .insert(index, receipt.as_ref().clone()),
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "native/testing.rs"]
pub(crate) mod testing;

#[cfg(test)]
#[path = "native/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(crate) use test_support::analyze;
