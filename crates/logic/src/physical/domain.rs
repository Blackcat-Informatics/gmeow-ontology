// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit native logical-world selection. Physical graph presence never selects
//! a domain law. The caller owns the source/role admission that supplies this contract.

use purrdf::TermValue;

use super::chase::{ExistentialRule, WitnessPolicy};
use super::store::metadata_identity;
use crate::rule_ir::{EvalAtom, EvalTerm};

/// Original native graph identity, before the executor's resource-term projection.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum LogicalGraph {
    Default,
    Named(#[serde(with = "crate::term_serde")] TermValue),
}

impl LogicalGraph {
    pub fn from_graph(graph: Option<TermValue>) -> Self {
        graph.map_or(Self::Default, Self::Named)
    }

    pub fn graph(&self) -> Option<&TermValue> {
        match self {
            Self::Default => None,
            Self::Named(graph) => Some(graph),
        }
    }

    /// Native execution key. Admission checks this key against the retained graph
    /// identity, refusing aliases between default, named and scoped blank graphs.
    pub fn world(&self) -> gmeow_errors::Result<String> {
        match self {
            Self::Default => Ok(crate::reason::rl::DEFAULT_WORLD.to_owned()),
            Self::Named(TermValue::Iri(iri)) => Ok(iri.clone()),
            Self::Named(graph @ TermValue::Blank { .. }) => {
                match crate::facts::skolemize(graph).as_ref() {
                    TermValue::Iri(iri) => Ok(iri.clone()),
                    other => Err(failure(format!(
                        "native graph projection is not a resource: {other:?}"
                    ))),
                }
            }
            Self::Named(other) => Err(failure(format!(
                "logical world graph must be a resource, got {other:?}"
            ))),
        }
    }
}

/// Selected open, nonempty object domain. This is an intrinsic semantic law,
/// not a closed-domain assumption or an asserted source statement.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum DomainProfile {
    NonemptyObjectDomainV1,
}

/// A caller-admitted logical world and the exact role/source selection receipt.
///
/// `authority` names the selected operation's admission contract; `selection`
/// binds its complete source/role inputs. They do not authenticate themselves:
/// the typed caller must validate its inputs before constructing this value.
/// No constructor selects worlds by inspecting dataset graph names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct SelectedLogicalWorld {
    graph: LogicalGraph,
    profile: DomainProfile,
    authority: String,
    selection: [u8; 32],
}

impl SelectedLogicalWorld {
    pub fn new(
        graph: LogicalGraph,
        profile: DomainProfile,
        authority: String,
        selection: [u8; 32],
    ) -> gmeow_errors::Result<Self> {
        let result = Self {
            graph,
            profile,
            authority,
            selection,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn graph(&self) -> &LogicalGraph {
        &self.graph
    }
    pub fn profile(&self) -> DomainProfile {
        self.profile
    }
    pub fn authority(&self) -> &str {
        &self.authority
    }
    pub fn selection(&self) -> &[u8; 32] {
        &self.selection
    }
    pub fn world(&self) -> gmeow_errors::Result<String> {
        self.graph.world()
    }

    pub(crate) fn validate(&self) -> gmeow_errors::Result<()> {
        self.world()?;
        if self.authority.is_empty() {
            return Err(failure(
                "logical domain selection requires its admission authority",
            ));
        }
        Ok(())
    }

    pub(crate) fn identity(&self) -> [u8; 32] {
        metadata_identity("gmeow-intrinsic-domain-selection-v1", self)
    }

    pub(crate) fn rule(&self) -> ExistentialRule {
        ExistentialRule {
            numeric: Vec::new(),
            // A complete scoped rule identifier also separates intrinsic proof
            // applications with no RDF premises in different world/contracts.
            rule_iri: format!(
                "urn:gmeow:rule:nonempty-object-domain:v1:{}",
                blake3::Hash::from(self.identity()).to_hex()
            ),
            body: Vec::new(),
            head: vec![EvalAtom::positive(
                EvalTerm::Var("domain".to_owned()),
                "https://blackcatinformatics.ca/logic/instanceOf",
                EvalTerm::named("https://blackcatinformatics.ca/logic/Thing"),
            )],
            distinct: Vec::new(),
            witness_frontier: Some(Vec::new()),
            witness_policy: WitnessPolicy::FrontierSkolem,
        }
    }
}

/// Complete explicit domain selection for a run. An empty selection means that
/// the selected operation makes no intrinsic nonempty-domain claim; it is never
/// a substitute supplied when a required logical-world receipt is absent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SelectedDomains {
    worlds: Vec<SelectedLogicalWorld>,
}

impl SelectedDomains {
    pub fn new(
        worlds: impl IntoIterator<Item = SelectedLogicalWorld>,
    ) -> gmeow_errors::Result<Self> {
        let mut worlds: Vec<_> = worlds.into_iter().collect();
        worlds.sort();
        let result = Self { worlds };
        result.validate()?;
        Ok(result)
    }

    pub fn worlds(&self) -> &[SelectedLogicalWorld] {
        &self.worlds
    }
    pub fn is_empty(&self) -> bool {
        self.worlds.is_empty()
    }

    pub(crate) fn validate(&self) -> gmeow_errors::Result<()> {
        let mut keys = std::collections::BTreeSet::new();
        for world in &self.worlds {
            world.validate()?;
            if !keys.insert(world.world()?) {
                return Err(failure(
                    "a logical world must have exactly one selected domain contract",
                ));
            }
        }
        if self.worlds.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(failure(
                "logical domain selections must be in canonical order",
            ));
        }
        Ok(())
    }
}

fn failure(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical {
        detail: detail.into(),
    })
}
