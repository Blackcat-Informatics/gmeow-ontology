// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Configuration declarations are data in a classical proof task. Their presence
//! does not request simultaneous execution of incompatible reasoning disciplines.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Support {
    Configuration,
    FacetType,
}

impl AdmissionBuilder<'_> {
    /// Retain the native declaration and every selected fact used to read it.
    /// The ordinary proof profile remains explicit and unchanged by these facts.
    pub(super) fn contract_declaration(
        &mut self,
        root: SourceNode,
        index: usize,
    ) -> SemanticUnitDisposition {
        let Some(configuration) = self.theory.program().contracts.get(index) else {
            return malformed(
                "MISSING_CONTRACT_IR",
                "contract trace index is out of range",
            );
        };
        let dataset = self.theory.source().dataset();
        let mut pending = BTreeSet::from([(root.term, Support::Configuration)]);
        let mut visited = BTreeSet::new();
        let mut facts = BTreeMap::new();
        let mut facet_support = BTreeSet::new();
        while let Some((term, mode)) = pending.pop_first() {
            if !visited.insert((term, mode)) {
                continue;
            }
            let node = SourceNode { term, graph: None };
            if self
                .theory
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
        self.definition_support.extend(facet_support);
        SemanticUnitDisposition::ConfigurationDeclaration {
            declaration: Box::new(SourceContractDeclaration {
                configuration: configuration.clone(),
                facts: facts.into_values().collect(),
            }),
            reason: "this proof task catalogues configuration declarations as metatheory; it does not request their execution or assert configuration syntax as domain axioms".into(),
        }
    }
}
