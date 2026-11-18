// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared compiler reading of authored configuration declarations. Their presence
//! catalogues metatheory and never selects incompatible reasoning disciplines.

use super::*;
use crate::frontend::{SemanticUnitDisposition, SourceContractDeclaration};
use crate::source_inventory::{SourceDeclaration, SourceInventory};
use purrdf::{GraphMatch, TermRef};
use std::collections::{BTreeMap, BTreeSet};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const LOGIC_INSTANCE_OF: &str = "https://blackcatinformatics.ca/logic/instanceOf";

fn blocked(code: impl Into<String>, reason: impl Into<String>) -> SemanticUnitDisposition {
    SemanticUnitDisposition::Blocked {
        code: code.into(),
        reason: reason.into(),
    }
}
fn malformed(code: impl Into<String>, reason: impl Into<String>) -> SemanticUnitDisposition {
    SemanticUnitDisposition::Malformed {
        code: code.into(),
        reason: reason.into(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Support {
    Configuration,
    FacetType,
}

impl SourceInventory<'_> {
    pub(crate) fn read_contract_declaration(
        &self,
        root: SourceNode,
        index: usize,
    ) -> SourceDeclaration {
        let mut facet_support = BTreeSet::new();
        let disposition = (|| {
            let Some(configuration) = self.theory().program().contracts.get(index) else {
                return malformed(
                    "MISSING_CONTRACT_IR",
                    "contract trace index is out of range",
                );
            };
            let dataset = self.theory().source().dataset();
            let mut pending = BTreeSet::from([(root.term, Support::Configuration)]);
            let mut visited = BTreeSet::new();
            let mut facts = BTreeMap::new();
            while let Some((term, mode)) = pending.pop_first() {
                if !visited.insert((term, mode)) {
                    continue;
                }
                let node = SourceNode { term, graph: None };
                if self
                    .theory()
                    .source()
                    .source_graph()
                    .outgoing(node)
                    .iter()
                    .any(|edge| {
                        matches!(
                            edge.role,
                            SourceEdgeRole::Standpoint
                                | SourceEdgeRole::Context
                                | SourceEdgeRole::Provenance
                                | SourceEdgeRole::Module
                        )
                    })
                {
                    return blocked(
                        "CONTEXT_ERASURE_FORBIDDEN",
                        "a scoped contract declaration requires typed context translation",
                    );
                }
                for quad in dataset.quads_for_pattern(Some(term), None, None, GraphMatch::Default) {
                    let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
                        continue;
                    };
                    let local = predicate.strip_prefix(LOGIC);
                    let type_fact = matches!(predicate, RDF_TYPE | LOGIC_INSTANCE_OF);
                    if !type_fact
                        && (mode == Support::FacetType
                            || !local.is_some_and(crate::frontend::is_facet_config_predicate))
                    {
                        continue;
                    }
                    if !self.statement_selected(quad, SourceCarrier::Statement) {
                        return blocked(
                            "UNSELECTED_CONTRACT_SUPPORT",
                            "contract configuration depends on facts outside the selected documents",
                        );
                    }
                    if mode == Support::FacetType
                        && type_fact
                        && matches!(dataset.resolve(quad.o), TermRef::Iri(iri)
                        if iri.strip_prefix(LOGIC).is_some_and(crate::frontend::is_facet_class))
                    {
                        facet_support.insert(SourceStatementBinding {
                            canonical: quad,
                            carrier: SourceCarrier::Statement,
                        });
                    }
                    let assertion = match assertion_from_quad(dataset, quad) {
                        Ok(assertion) => assertion,
                        Err(error) => {
                            return blocked("UNSUPPORTED_CONTRACT_DECLARATION", error.to_string());
                        }
                    };
                    facts.insert(assertion.key(), assertion);
                    if mode == Support::Configuration {
                        let support = match local {
                            Some("closureEntry") => Some(Support::Configuration),
                            Some("expandsToFacet") => Some(Support::FacetType),
                            _ => None,
                        };
                        if let Some(support) = support {
                            if !matches!(
                                dataset.resolve(quad.o),
                                TermRef::Iri(_) | TermRef::Blank { .. }
                            ) {
                                return malformed(
                                    "MALFORMED_CONTRACT_DECLARATION",
                                    "a referenced configuration node must be a resource",
                                );
                            }
                            pending.insert((quad.o, support));
                        }
                    }
                }
            }
            if facts.is_empty() {
                return malformed(
                    "MALFORMED_CONTRACT_DECLARATION",
                    "a contract owner has no selected declaration facts",
                );
            }
            SemanticUnitDisposition::ConfigurationDeclaration {
            declaration: Box::new(SourceContractDeclaration {
                configuration: configuration.clone(),
                facts: facts.into_values().collect(),
            }),
            reason: "this proof task catalogues configuration declarations as metatheory; it does not request their execution or assert configuration syntax as domain axioms".into(),
        }
        })();
        if !matches!(
            disposition,
            SemanticUnitDisposition::ConfigurationDeclaration { .. }
        ) {
            facet_support.clear();
        }
        SourceDeclaration {
            disposition,
            support: facet_support,
        }
    }
}
