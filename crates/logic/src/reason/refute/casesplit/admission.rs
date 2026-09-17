// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Source-owner admission observed from the same prepared analysis as case execution.

use super::{FragmentBoundary, RefutationPremise, Scan};
use purrdf::DatasetView;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Exact selected source definitions and their complete per-world refusals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassAdmissionWorld {
    /// Original asserting graph, independently of its deterministic execution key.
    #[serde(with = "crate::term_serde::optional")]
    pub graph: Option<purrdf::TermValue>,
    /// Original selected definition and declaration occurrences.
    pub definitions: Vec<RefutationPremise>,
    /// Every selected source grammar or operand refusal in this context.
    pub refusal: Option<FragmentBoundary>,
}

/// The class-expression/list source profile only. An empty selected inventory says
/// nothing about other native operators, world-domain selection or satisfiability.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassAdmissionObservation {
    /// The exact native source-admission contract local name.
    pub contract: String,
    /// Exact input contexts, including declared empty graphs and explicitly selected empty worlds.
    pub source_worlds: BTreeMap<String, ClassAdmissionSourceWorld>,
    /// Only contexts with a source operator owning the selected grammar.
    pub selected_worlds: BTreeMap<String, ClassAdmissionWorld>,
}

/// Original graph ownership and assertion-table census; no inference is counted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassAdmissionSourceWorld {
    /// Exact original graph; None is the selected default context.
    #[serde(with = "crate::term_serde::optional")]
    pub graph: Option<purrdf::TermValue>,
    /// All native assertion rows, including statement-layer assertions.
    pub assertions: u64,
}

fn fail(detail: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.to_owned(),
    })
}

pub(super) fn graph_world(graph: Option<&purrdf::TermValue>) -> gmeow_errors::Result<String> {
    match graph {
        None => Ok(crate::reason::rl::DEFAULT_WORLD.to_owned()),
        Some(term @ (purrdf::TermValue::Iri(_) | purrdf::TermValue::Blank { .. })) => {
            Ok(crate::facts::skolemize(term)
                .as_iri()
                .expect("resource graph")
                .to_owned())
        }
        Some(_) => Err(fail("class admission graph must be a native resource")),
    }
}

/// Original input grammar failure is distinct from a valid unsupported capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClassSourceRefusal {
    /// The selected source structure violates its required grammar.
    Invalid,
    /// The selected structure is well formed but outside the implemented fragment.
    Unsupported,
}

impl ClassAdmissionObservation {
    /// Classify retained typed causes without parsing diagnostics or discarding
    /// sibling evidence. Any malformed selected structure invalidates admission.
    pub fn refusal_class(&self) -> gmeow_errors::Result<Option<ClassSourceRefusal>> {
        self.validate()?;
        fn classify(boundary: &FragmentBoundary, current: &mut Option<ClassSourceRefusal>) {
            match boundary {
                FragmentBoundary::SourceAdmission { issue, .. } => {
                    let value = issue.refusal_class();
                    if *current != Some(ClassSourceRefusal::Invalid) {
                        *current = Some(value);
                    }
                }
                FragmentBoundary::Combined(boundaries) => {
                    for boundary in boundaries {
                        classify(boundary, current);
                    }
                }
                _ => unreachable!("validated source admission has only source causes"),
            }
        }
        let mut result = None;
        for world in self.selected_worlds.values() {
            if let Some(boundary) = &world.refusal {
                classify(boundary, &mut result);
            }
        }
        Ok(result)
    }
    /// No actual source owner selected this particular grammar contract.
    pub fn outside_selection(&self) -> bool {
        self.selected_worlds.is_empty()
    }

    /// The source-observation status alone, never a semantic verdict. Consumers
    /// validate the authenticated record before projecting this closed token.
    pub fn observation_status(&self) -> &'static str {
        if self.outside_selection() {
            "outside-selection"
        } else if self
            .selected_worlds
            .values()
            .any(|world| world.refusal.is_some())
        {
            "refused"
        } else {
            "admitted"
        }
    }

    /// Check the intrinsic scope and source framing of authenticated native evidence.
    /// This does not certify arbitrary external byte strings as a theorem proof.
    pub fn validate(&self) -> gmeow_errors::Result<()> {
        if self.contract != super::super::CLASS_EXPRESSION_SOURCE_ADMISSION_ID {
            return Err(fail("unrecognized class source-admission contract"));
        }
        for (world, source) in &self.source_worlds {
            if graph_world(source.graph.as_ref())? != *world {
                return Err(fail("class source graph and execution world disagree"));
            }
        }
        for (world, selected) in &self.selected_worlds {
            let source = self
                .source_worlds
                .get(world)
                .ok_or_else(|| fail("selected class source world is absent"))?;
            if source.graph != selected.graph
                || selected.definitions.is_empty()
                || selected
                    .definitions
                    .iter()
                    .any(|row| row.graph != selected.graph)
            {
                return Err(fail(
                    "class source definition lost its exact original graph",
                ));
            }
            if let Some(boundary) = &selected.refusal {
                validate_source_boundary(boundary, world, selected.graph.as_ref())?;
            }
        }
        Ok(())
    }
}

/// One immutable parse/admission preparation, borrowed by observation and execution.
/// No second semantic pass, corpus cache or synthetic source axiom is involved.
pub struct PreparedClassAnalysis {
    pub(super) scan: Scan,
    pub(super) native: Option<super::execution::NativeClassState>,
    admission: ClassAdmissionObservation,
}

impl PreparedClassAnalysis {
    /// Admit one standalone source view. Joint execution supplies its already
    /// retained ingress occurrences instead of rebuilding an RDF dataset.
    pub fn new(source: &impl DatasetView) -> gmeow_errors::Result<Self> {
        let scan = Scan::of(source);
        if scan.source_alias {
            return Err(fail(
                "distinct class source graphs share one native execution world",
            ));
        }
        Self::from_scan(scan)
    }

    /// Reuse the joint ingress's exact original occurrences and selected graph.
    /// No family reparses or reconstructs an RDF dataset from the shared native store.
    pub(crate) fn from_native_sources(
        world: &str,
        graph: Option<&purrdf::TermValue>,
        originals: &[RefutationPremise],
    ) -> gmeow_errors::Result<Self> {
        if graph_world(graph)? != world || originals.iter().any(|row| row.graph.as_ref() != graph) {
            return Err(fail(
                "class preparation and native ingress have different source scope",
            ));
        }
        let rows = originals.iter().map(|source| {
            let quad = super::supported_quad(
                &source.subject,
                &source.predicate,
                &source.object,
                source.graph.as_ref(),
            )?;
            Ok((quad, super::Support::one(source.clone())))
        });
        let mut scan = Scan::from_parts(graph.cloned(), rows)?;
        scan.source_worlds
            .entry(world.to_owned())
            .or_insert_with(|| ClassAdmissionSourceWorld {
                graph: graph.cloned(),
                assertions: 0,
            });
        scan.worlds.entry(world.to_owned()).or_default();
        Self::from_scan(scan)
    }

    fn from_scan(scan: Scan) -> gmeow_errors::Result<Self> {
        let mut selected_worlds = BTreeMap::new();
        for (world, data) in &scan.worlds {
            let mut definitions = std::collections::BTreeSet::new();
            for ((owner, predicate, _), support) in &data.source_premises {
                if data.selects_definition(owner, predicate) {
                    definitions.extend(support.rows());
                    if matches!(
                        predicate.as_str(),
                        super::OWL_MEMBERS | super::OWL_DISTINCT_MEMBERS
                    ) {
                        definitions.extend(
                            data.support(owner, super::RDF_TYPE, super::OWL_ALL_DIFFERENT)
                                .rows(),
                        );
                    }
                }
            }
            if !definitions.is_empty() {
                selected_worlds.insert(
                    world.clone(),
                    ClassAdmissionWorld {
                        graph: scan.source_worlds[world].graph.clone(),
                        definitions: definitions.into_iter().collect(),
                        refusal: data.source_boundary.clone(),
                    },
                );
            }
        }
        let admission = ClassAdmissionObservation {
            source_worlds: scan.source_worlds.clone(),
            contract: super::super::CLASS_EXPRESSION_SOURCE_ADMISSION_ID.to_owned(),
            selected_worlds,
        };
        admission.validate()?;
        Ok(Self {
            scan,
            native: None,
            admission,
        })
    }

    /// Borrow the retained admission inventory without executing the selected theory.
    pub fn admission(&self) -> &ClassAdmissionObservation {
        &self.admission
    }
}

/// Source admission carries only actual source occurrences in its selected world.
fn validate_source_boundary(
    boundary: &FragmentBoundary,
    world: &str,
    graph: Option<&purrdf::TermValue>,
) -> gmeow_errors::Result<()> {
    match boundary {
        FragmentBoundary::SourceAdmission {
            world: actual,
            premises,
            ..
        } if actual == world
            && !premises.is_empty()
            && premises.iter().all(|row| row.graph.as_ref() == graph) =>
        {
            Ok(())
        }
        FragmentBoundary::Combined(boundaries) if !boundaries.is_empty() => {
            for boundary in boundaries {
                validate_source_boundary(boundary, world, graph)?;
            }
            Ok(())
        }
        _ => Err(fail(
            "class source refusal has invalid scope or no original source support",
        )),
    }
}
