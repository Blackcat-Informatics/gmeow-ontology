// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Scoped native null addresses and committed minting evidence. Candidate allocation
//! is private; only heads in the committed budget prefix can explain a witness.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use purrdf::TermValue;

use crate::facts::skolem_iri;
use crate::rule_ir::{DerivedRow, Fact, FactKey};
use gmeow_term_arena::engine::native_term_key;

use super::RelationStore;

/// A streamed, typed debug commitment to immutable native execution metadata.
/// Source terms retain their full values; no RDF serialization or term IDs enter it.
pub(crate) fn metadata_identity(domain: &str, value: &impl std::fmt::Debug) -> [u8; 32] {
    struct Digest(blake3::Hasher);
    impl std::fmt::Write for Digest {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            self.0.update(value.as_bytes());
            Ok(())
        }
    }
    let mut digest = Digest(blake3::Hasher::new());
    digest.0.update(domain.as_bytes());
    digest.0.update(&[0]);
    write!(digest, "{value:?}").expect("digest-only formatter");
    *digest.0.finalize().as_bytes()
}

/// Mandatory native operator/profile authority, independent of input iteration order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WitnessContract(pub(crate) [u8; 32]);

impl WitnessContract {
    pub(crate) fn native(semantics: crate::native_semantics::SemanticVocabulary) -> Self {
        Self(metadata_identity(
            "gmeow-witness-contract-v1",
            &(crate::reason::native_contract_hash(), semantics),
        ))
    }

    pub(crate) fn with_source(self, source: &crate::native_semantics::ProgramAdmission) -> Self {
        Self(metadata_identity(
            "gmeow-witness-source-contract-v1",
            &(self, source),
        ))
    }

    pub(crate) fn scope(self, world: &str, source_rule: [u8; 32]) -> WitnessScope {
        WitnessScope {
            world: world.to_owned(),
            contract: self.0,
            source_rule,
            origin: WitnessOrigin::Rule,
        }
    }
}

/// Proof ownership is explicit. An intrinsic law has no asserted RDF premises;
/// its complete selected-world contract is part of the witness address instead.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum WitnessOrigin {
    Rule,
    NonemptyDomain(crate::physical::SelectedLogicalWorld),
}

/// The admitted world, governing native/source contract and complete source-rule
/// commitment. A rule name alone is never a content commitment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WitnessScope {
    pub world: String,
    pub contract: [u8; 32],
    pub source_rule: [u8; 32],
    pub origin: WitnessOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkolemTerm {
    pub(crate) scope: WitnessScope,
    pub(crate) rule_iri: String,
    pub(crate) ordinal: usize,
    pub(crate) frontier: Vec<TermValue>,
}

impl SkolemTerm {
    pub(super) fn content_key(&self) -> String {
        fn frame(out: &mut String, field: &str) {
            write!(out, "{}\u{1f}{field}", field.len()).expect("String formatter");
        }
        let mut key = String::from("wa-skolem-v4");
        frame(&mut key, &self.scope.world);
        frame(
            &mut key,
            blake3::Hash::from(self.scope.contract).to_hex().as_str(),
        );
        frame(
            &mut key,
            blake3::Hash::from(self.scope.source_rule).to_hex().as_str(),
        );
        frame(
            &mut key,
            blake3::Hash::from(metadata_identity(
                "gmeow-witness-origin-v1",
                &self.scope.origin,
            ))
            .to_hex()
            .as_str(),
        );
        frame(&mut key, &self.rule_iri);
        frame(&mut key, &self.ordinal.to_string());
        frame(&mut key, &self.frontier.len().to_string());
        for term in &self.frontier {
            frame(&mut key, &native_term_key(term));
        }
        key
    }

    pub(super) fn witness_iri(&self) -> String {
        skolem_iri(&self.content_key())
    }
}

/// Native statement value used in a witness's committed head and ordered premises.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct WitnessStatement {
    #[serde(with = "crate::term_serde")]
    pub subject: TermValue,
    pub predicate: String,
    #[serde(with = "crate::term_serde")]
    pub object: TermValue,
}

impl From<&Fact> for WitnessStatement {
    fn from(fact: &Fact) -> Self {
        Self {
            subject: fact.subject.clone(),
            predicate: fact.predicate.clone(),
            object: fact.object.clone(),
        }
    }
}

impl WitnessStatement {
    fn fact(&self) -> Fact {
        Fact {
            subject: self.subject.clone(),
            predicate: self.predicate.clone(),
            object: self.object.clone(),
        }
    }
}

/// Actual value-null occurrences in a grounded native head. Tuple reifier identity
/// remains a separate proposition identity and is not a value invention.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum WitnessPosition {
    Subject,
    Object,
}

/// An actual minting-rule head committed in the owning world. The complete ordered
/// premises retain this proof even when another proof wins the closure's row tiebreak.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct WitnessHead {
    pub statement: WitnessStatement,
    pub positions: Vec<WitnessPosition>,
    pub premises: Vec<WitnessStatement>,
    pub derivation_id: String,
}

/// Decomposable address and exact committed evidence for one native invented value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WitnessDerivation {
    pub witness: String,
    pub rule_iri: String,
    pub scope: WitnessScope,
    pub ordinal: usize,
    #[serde(with = "frontier_serde")]
    pub frontier: Vec<TermValue>,
    pub heads: Vec<WitnessHead>,
}

impl WitnessDerivation {
    /// Versioned output receipt. There is no legacy unscoped admission path.
    pub fn to_wire(&self) -> gmeow_errors::Result<String> {
        self.validate()?;
        serde_json::to_string(self)
            .map(|json| format!("gmeow-witness-v2:{json}"))
            .map_err(|error| {
                gmeow_errors::Diag::of_kind(crate::error::Physical {
                    detail: format!("native witness receipt encoding: {error}"),
                })
            })
    }

    /// Decode an explicit receipt and check its address and proof framing. This is
    /// intrinsic validation, not independent certification of a supplied source.
    pub fn from_wire(wire: &str) -> gmeow_errors::Result<Self> {
        let fail = |detail: String| gmeow_errors::Diag::of_kind(crate::error::Physical { detail });
        let json = wire
            .strip_prefix("gmeow-witness-v2:")
            .ok_or_else(|| fail("unsupported native witness receipt version".to_owned()))?;
        let witness: Self = serde_json::from_str(json)
            .map_err(|error| fail(format!("native witness receipt decoding: {error}")))?;
        witness.validate()?;
        Ok(witness)
    }

    /// Validate intrinsic address/evidence integrity. Closure membership and source
    /// authority remain the consuming native result's responsibility.
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        let fail = || {
            gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!("invalid scoped witness evidence for <{}>", self.witness),
            })
        };
        let recipe = SkolemTerm {
            scope: self.scope.clone(),
            rule_iri: self.rule_iri.clone(),
            ordinal: self.ordinal,
            frontier: self.frontier.clone(),
        };
        if recipe.witness_iri() != self.witness || self.heads.is_empty() {
            return Err(fail());
        }
        if let WitnessOrigin::NonemptyDomain(domain) = &self.scope.origin {
            domain.validate()?;
            let rule = domain.rule();
            if domain.world()? != self.scope.world
                || domain.identity() != self.scope.source_rule
                || rule.rule_iri != self.rule_iri
                || self.ordinal != 0
                || !self.frontier.is_empty()
                || self.heads.len() != 1
                || self.heads[0].statement.subject != TermValue::iri(&self.witness)
                || self.heads[0].statement.predicate != rule.head[0].predicate
                || self.heads[0].statement.object
                    != TermValue::iri("https://blackcatinformatics.ca/logic/Thing")
                || !self.heads[0].premises.is_empty()
            {
                return Err(fail());
            }
        }
        let value = TermValue::iri(&self.witness);
        for head in &self.heads {
            let expected = positions(&head.statement.fact(), &value);
            let source_ids = head
                .premises
                .iter()
                .map(|p| p.fact().reifier())
                .collect::<gmeow_errors::Result<Vec<_>>>()?;
            let ids: Vec<_> = source_ids.iter().map(String::as_str).collect();
            if expected.is_empty()
                || head.positions != expected
                || head.derivation_id != crate::provenance::mint_derivation_id(&self.rule_iri, &ids)
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}

mod frontier_serde {
    use purrdf::TermValue;
    use serde::{Deserialize, Serialize};
    #[derive(Serialize, Deserialize)]
    struct Value(#[serde(with = "crate::term_serde")] TermValue);
    #[derive(Serialize)]
    struct Ref<'a>(#[serde(with = "crate::term_serde")] &'a TermValue);
    pub(super) fn serialize<S: serde::Serializer>(
        values: &[TermValue],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        values
            .iter()
            .map(Ref)
            .collect::<Vec<_>>()
            .serialize(serializer)
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<TermValue>, D::Error> {
        Vec::<Value>::deserialize(deserializer)
            .map(|values| values.into_iter().map(|v| v.0).collect())
    }
}

fn positions(fact: &Fact, value: &TermValue) -> Vec<WitnessPosition> {
    let mut positions = Vec::with_capacity(2);
    if fact.subject == *value {
        positions.push(WitnessPosition::Subject);
    }
    if fact.object == *value {
        positions.push(WitnessPosition::Object);
    }
    positions
}

/// One shared registry retains private recipes and publishes only committed heads.
#[derive(Debug, Clone, Default)]
pub(crate) struct SkolemRegistry {
    recipes: BTreeMap<String, SkolemTerm>,
    heads: BTreeMap<String, BTreeSet<WitnessHead>>,
    pending: BTreeMap<(String, FactKey), Vec<(String, WitnessHead)>>,
}

impl SkolemRegistry {
    /// Keep actual minting proofs independently of the closure's winning proof.
    /// The head must remain derived, and every ordered premise must still exist
    /// in the same committed world. A tuple's winning derivation is not authority
    /// to erase another producer's valid introduction receipt.
    pub(crate) fn retain_committed_facts(&mut self, rows: &[DerivedRow]) {
        self.pending.clear();
        let committed: BTreeSet<_> = rows
            .iter()
            .map(|row| {
                (
                    row.graph.clone(),
                    (
                        row.subject.clone(),
                        row.predicate.clone(),
                        row.object.clone(),
                    ),
                )
            })
            .collect();
        let derived: BTreeSet<_> = rows
            .iter()
            .filter(|row| row.rule_iri != crate::provenance::ASSERT_RULE_IRI)
            .map(|row| {
                (
                    row.graph.clone(),
                    (
                        row.subject.clone(),
                        row.predicate.clone(),
                        row.object.clone(),
                    ),
                )
            })
            .collect();
        self.heads.retain(|witness, heads| {
            let Some(recipe) = self.recipes.get(witness) else {
                return false;
            };
            heads.retain(|head| {
                derived.contains(&(recipe.scope.world.clone(), head.statement.fact().key()))
                    && head.premises.iter().all(|premise| {
                        committed.contains(&(recipe.scope.world.clone(), premise.fact().key()))
                    })
            });
            !heads.is_empty()
        });
    }

    /// Restore allocation recipes without publishing any cached introduction.
    pub(crate) fn recipes_only(&self) -> Self {
        Self {
            recipes: self.recipes.clone(),
            heads: BTreeMap::new(),
            pending: BTreeMap::new(),
        }
    }

    /// Schedule independently valid introductions after their producer, actual
    /// head and every ordered premise become committed. The caller admits the
    /// unchanged source/domain template and excludes invalidated producers.
    pub(crate) fn retained_introductions(
        &self,
        producers: &BTreeMap<(String, String), usize>,
        facts: &BTreeMap<String, BTreeMap<FactKey, usize>>,
        derived: &BTreeSet<(String, FactKey)>,
    ) -> BTreeMap<usize, Vec<WitnessDerivation>> {
        let mut scheduled: BTreeMap<usize, Vec<WitnessDerivation>> = BTreeMap::new();
        for (witness, heads) in &self.heads {
            let recipe = &self.recipes[witness];
            let Some(producer_rank) =
                producers.get(&(recipe.scope.world.clone(), recipe.rule_iri.clone()))
            else {
                continue;
            };
            let Some(world) = facts.get(&recipe.scope.world) else {
                continue;
            };
            let mut by_rank: BTreeMap<usize, Vec<WitnessHead>> = BTreeMap::new();
            for head in heads {
                if !derived.contains(&(recipe.scope.world.clone(), head.statement.fact().key())) {
                    continue;
                }
                let Some(rank) = std::iter::once(&head.statement)
                    .chain(&head.premises)
                    .try_fold(*producer_rank, |rank, statement| {
                        world
                            .get(&statement.fact().key())
                            .map(|ready| rank.max(*ready))
                    })
                else {
                    continue;
                };
                by_rank.entry(rank).or_default().push(head.clone());
            }
            for (rank, heads) in by_rank {
                scheduled.entry(rank).or_default().push(WitnessDerivation {
                    witness: witness.clone(),
                    rule_iri: recipe.rule_iri.clone(),
                    scope: recipe.scope.clone(),
                    ordinal: recipe.ordinal,
                    frontier: recipe.frontier.clone(),
                    heads,
                });
            }
        }
        scheduled
    }

    /// Install only receipts whose real producer stratum has been reached and
    /// whose exact native head and premises are committed in this transaction.
    pub(crate) fn install_introductions(
        &mut self,
        introductions: &[WitnessDerivation],
        committed: impl Fn(&str, &FactKey) -> bool,
    ) -> gmeow_errors::Result<()> {
        for introduction in introductions {
            introduction.validate()?;
            let recipe = SkolemTerm {
                scope: introduction.scope.clone(),
                rule_iri: introduction.rule_iri.clone(),
                ordinal: introduction.ordinal,
                frontier: introduction.frontier.clone(),
            };
            if self.recipes.get(&introduction.witness) != Some(&recipe)
                || introduction.heads.iter().any(|head| {
                    std::iter::once(&head.statement)
                        .chain(&head.premises)
                        .any(|statement| !committed(&recipe.scope.world, &statement.fact().key()))
                })
            {
                return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                    detail: "retained witness introduction lost its scope, head or ordered premise"
                        .to_owned(),
                }));
            }
        }
        for introduction in introductions {
            self.heads
                .entry(introduction.witness.clone())
                .or_default()
                .extend(introduction.heads.iter().cloned());
        }
        Ok(())
    }

    /// Every previously invented value used by a retained statement, including
    /// values nested inside RDF-star terms, needs a ready introduction.
    pub(crate) fn introduced_values<'a>(&'a self, fact: &'a Fact) -> BTreeSet<&'a str> {
        fn visit<'a>(
            registry: &'a SkolemRegistry,
            value: &'a TermValue,
            out: &mut BTreeSet<&'a str>,
        ) {
            match value {
                TermValue::Iri(iri) if registry.recipes.contains_key(iri) => {
                    out.insert(iri);
                }
                TermValue::Triple { s, p, o } => {
                    visit(registry, s, out);
                    visit(registry, p, out);
                    visit(registry, o, out);
                }
                _ => {}
            }
        }
        let mut values = BTreeSet::new();
        visit(self, &fact.subject, &mut values);
        visit(self, &fact.object, &mut values);
        if self.recipes.contains_key(&fact.predicate) {
            values.insert(fact.predicate.as_str());
        }
        values
    }
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn mint(&mut self, recipe: SkolemTerm) -> TermValue {
        let iri = recipe.witness_iri();
        match self.recipes.get(&iri) {
            Some(existing) => assert_eq!(existing, &recipe, "distinct witness recipes collided"),
            None => {
                self.recipes.insert(iri.clone(), recipe);
            }
        }
        TermValue::iri(iri)
    }

    pub(crate) fn mint_dl_blocked(&mut self, recipe: SkolemTerm) -> TermValue {
        if let Some(blocker) = self.dl_recursive_blocker(&recipe) {
            return blocker;
        }
        self.mint(recipe)
    }

    fn dl_recursive_blocker(&self, recipe: &SkolemTerm) -> Option<TermValue> {
        let [TermValue::Iri(frontier)] = recipe.frontier.as_slice() else {
            return None;
        };
        let mut cursor = frontier.as_str();
        let mut seen = BTreeSet::new();
        while seen.insert(cursor.to_owned()) {
            let ancestor = self.recipes.get(cursor)?;
            if ancestor.scope.world != recipe.scope.world
                || ancestor.scope.contract != recipe.scope.contract
            {
                return None;
            }
            if ancestor.scope.source_rule == recipe.scope.source_rule
                && ancestor.rule_iri == recipe.rule_iri
                && ancestor.ordinal == recipe.ordinal
            {
                return Some(TermValue::iri(cursor));
            }
            let [TermValue::Iri(parent)] = ancestor.frontier.as_slice() else {
                return None;
            };
            cursor = parent;
        }
        None
    }

    /// Record genuine introductions from this firing, never arbitrary later uses of
    /// an invented term. Allocation alone does not publish a completed explanation.
    pub(crate) fn record_head(
        &mut self,
        scope: &WitnessScope,
        introduced: &[TermValue],
        fact: &Fact,
        premises: &[Fact],
    ) -> gmeow_errors::Result<()> {
        let mut records = Vec::new();
        for value in introduced {
            let fail = || {
                gmeow_errors::Diag::of_kind(crate::error::Physical {
                    detail: "a minting head requires its exact registered witness scope".to_owned(),
                })
            };
            let iri = value.as_iri().ok_or_else(fail)?;
            let recipe = self.recipes.get(iri).ok_or_else(fail)?;
            if recipe.scope != *scope {
                return Err(fail());
            }
            let positions = positions(fact, value);
            if positions.is_empty() {
                continue;
            }
            let source_ids = premises
                .iter()
                .map(Fact::reifier)
                .collect::<gmeow_errors::Result<Vec<_>>>()?;
            let ids: Vec<_> = source_ids.iter().map(String::as_str).collect();
            records.push((
                iri.to_owned(),
                WitnessHead {
                    statement: WitnessStatement::from(fact),
                    positions,
                    premises: premises.iter().map(WitnessStatement::from).collect(),
                    derivation_id: crate::provenance::mint_derivation_id(&recipe.rule_iri, &ids),
                },
            ));
        }
        if !records.is_empty() {
            self.pending
                .entry((scope.world.clone(), fact.key()))
                .or_default()
                .extend(records);
        }
        Ok(())
    }

    /// Admit every discovered introduction supported by this world's committed
    /// cut, including a later alternative for a head committed in an older round.
    /// Exact ordered premises remain part of each receipt; tuple deduplication
    /// neither drops an alternative nor charges another derivation step.
    pub(crate) fn commit_heads(&mut self, world: &str, committed: &RelationStore) {
        let heads = &mut self.heads;
        self.pending.retain(|(owner, _), records| {
            if owner != world {
                // Worlds gather against one frozen round before any commit.
                // Another world's pending evidence awaits its own committed cut.
                return true;
            }
            for (witness, head) in records.drain(..) {
                if std::iter::once(&head.statement)
                    .chain(&head.premises)
                    .all(|statement| {
                        committed.contains(
                            &statement.predicate,
                            &statement.subject,
                            &statement.object,
                        )
                    })
                {
                    heads.entry(witness).or_default().insert(head);
                }
            }
            // Discard only records outside this world's actual committed cut.
            false
        });
    }

    pub(crate) fn recipe(&self, iri: &str) -> Option<&SkolemTerm> {
        self.recipes.get(iri)
    }
    pub(crate) fn explain(&self, iri: &str) -> Option<WitnessDerivation> {
        let recipe = self.recipe(iri)?;
        let heads = self.heads.get(iri)?;
        Some(WitnessDerivation {
            witness: iri.to_owned(),
            rule_iri: recipe.rule_iri.clone(),
            scope: recipe.scope.clone(),
            ordinal: recipe.ordinal,
            frontier: recipe.frontier.clone(),
            heads: heads.iter().cloned().collect(),
        })
    }
    pub(crate) fn is_invented(&self, term: &TermValue) -> bool {
        term.as_iri()
            .is_some_and(|iri| self.recipes.contains_key(iri))
    }
    pub(crate) fn len(&self) -> usize {
        self.recipes.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }
    pub(crate) fn witnesses(&self) -> impl Iterator<Item = &str> {
        self.heads.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod retention_tests;
