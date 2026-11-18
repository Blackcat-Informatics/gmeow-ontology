// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Neutral syntax ownership. Semantic references never import their definitions.

use super::*;
use crate::source_inventory::{
    SyntaxEvidence, SyntaxFamily, SyntaxIssue, SyntaxOwner, SyntaxOwnership,
};
use purrdf::{DatasetView, GraphMatch, QuadIds};

#[derive(Clone, Copy)]
enum ListMembers {
    Classes,
    Values,
    Properties,
    Facets,
}

impl SyntaxOwnership {
    fn child(&mut self, owner: SyntaxOwner, child: SyntaxOwner, evidence: SyntaxEvidence) {
        if owner != child {
            self.children
                .entry(owner)
                .or_default()
                .entry(child)
                .or_default()
                .insert(evidence);
        }
    }

    fn row(
        &mut self,
        owner: SyntaxOwner,
        row: super::SourceStatementBinding,
        evidence: SyntaxEvidence,
    ) {
        self.rows
            .entry(row)
            .or_default()
            .entry(owner)
            .or_default()
            .insert(evidence);
    }

    /// Activate an actual constructor head in each of its native member roles.
    fn list_head(
        &mut self,
        dataset: &RdfDataset,
        owner: SyntaxOwner,
        source: super::SourceStatementBinding,
        members: ListMembers,
    ) {
        let nil = dataset.term_id_by_iri(crate::lists::RDF_NIL);
        let head = source.canonical.o;
        if Some(head) != nil && is_resource(dataset, head) {
            self.child(
                owner,
                SyntaxOwner {
                    node: SourceNode {
                        term: head,
                        graph: owner.node.graph,
                    },
                    family: members.family(),
                },
                SyntaxEvidence::Statement(source),
            );
        }
    }
}

impl ListMembers {
    fn family(self) -> SyntaxFamily {
        match self {
            Self::Classes => SyntaxFamily::ClassList,
            Self::Values => SyntaxFamily::ValueList,
            Self::Properties => SyntaxFamily::PropertyList,
            Self::Facets => SyntaxFamily::FacetList,
        }
    }
}

/// Match the compiler's asserted source union without erasing physical carriers.
/// Reifier rows have the fixed rdf:reifies predicate and cannot supply list fields.
fn fields(
    dataset: &RdfDataset,
    node: SourceNode,
    predicate: Option<TermId>,
) -> impl Iterator<Item = super::SourceStatementBinding> + '_ {
    let placement = node.graph.map_or(GraphMatch::Default, GraphMatch::Named);
    dataset
        .quads_for_pattern(Some(node.term), predicate, None, placement)
        .map(|canonical| super::SourceStatementBinding {
            canonical,
            carrier: SourceCarrier::Statement,
        })
        .chain(
            dataset
                .annotations_of_with_graph(node.term)
                .filter(move |(p, _, g)| {
                    predicate.is_none_or(|predicate| *p == predicate) && *g == node.graph
                })
                .map(move |(p, o, g)| super::SourceStatementBinding {
                    canonical: QuadIds {
                        s: node.term,
                        p,
                        o,
                        g,
                    },
                    carrier: SourceCarrier::Annotation,
                }),
        )
}

fn class_expression(dataset: &RdfDataset, node: SourceNode) -> bool {
    if !matches!(dataset.resolve(node.term), TermRef::Blank { .. }) {
        return false;
    }
    fields(dataset, node, None).any(|binding| {
        let quad = binding.canonical;
        let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
            return false;
        };
        if matches!(
            predicate,
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                | "https://blackcatinformatics.ca/logic/instanceOf"
        ) && let TermRef::Iri(marker) = dataset.resolve(quad.o)
        {
            return gmeow_ns::schema_type_role(marker)
                == Some(gmeow_ns::SchemaTypeRole::ClassExpression)
                || matches!(
                    marker,
                    "https://blackcatinformatics.ca/logic/Enumeration"
                        | "https://blackcatinformatics.ca/logic/DatatypeRestriction"
                );
        }
        match gmeow_ns::schema_predicate_role(predicate) {
            Some(gmeow_ns::SchemaPredicateRole::ClassField) => !matches!(
                gmeow_ns::canonical_calculus_symbol(predicate),
                Some(
                    "https://blackcatinformatics.ca/logic/onClass"
                        | "https://blackcatinformatics.ca/logic/onDataRange"
                )
            ),
            Some(
                gmeow_ns::SchemaPredicateRole::ClassList
                | gmeow_ns::SchemaPredicateRole::ValueList
                | gmeow_ns::SchemaPredicateRole::FacetList,
            ) => true,
            _ => false,
        }
    })
}

fn property_expression(dataset: &RdfDataset, node: SourceNode) -> bool {
    matches!(dataset.resolve(node.term), TermRef::Blank { .. })
        && fields(dataset, node, None).any(|binding| {
            matches!(dataset.resolve(binding.canonical.p), TermRef::Iri(predicate)
                if gmeow_ns::canonical_calculus_symbol(predicate)
                    == Some("https://blackcatinformatics.ca/logic/inverseOf"))
        })
}

/// The compact atom reader treats these exact native fields, or an explicit
/// TermCarrier declaration, as mandatory syntax instead of an ordinary resource.
fn atomic_term_carrier(dataset: &RdfDataset, node: SourceNode) -> bool {
    node.graph.is_none()
        && is_resource(dataset, node.term)
        && super::super::formula_reader::has_atomic_carrier_syntax(
            dataset,
            &crate::graphutil::subject_of(dataset, node.term),
        )
}

fn member_kinds(dataset: &RdfDataset, node: SourceNode) -> [Option<ListMembers>; 3] {
    let mut classes = false;
    let mut properties = false;
    let mut values = false;
    for binding in fields(dataset, node, None) {
        let quad = binding.canonical;
        if !matches!(
            dataset.resolve(quad.p),
            TermRef::Iri(
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                    | "https://blackcatinformatics.ca/logic/instanceOf"
            )
        ) {
            continue;
        }
        if let TermRef::Iri(marker) = dataset.resolve(quad.o) {
            match gmeow_ns::schema_type_role(marker) {
                Some(gmeow_ns::SchemaTypeRole::DisjointClasses) => classes = true,
                Some(gmeow_ns::SchemaTypeRole::DisjointProperties) => properties = true,
                Some(gmeow_ns::SchemaTypeRole::DistinctIndividuals) => values = true,
                _ => {}
            }
        }
    }
    [
        classes.then_some(ListMembers::Classes),
        properties.then_some(ListMembers::Properties),
        (values || (!classes && !properties)).then_some(ListMembers::Values),
    ]
}

impl StructuralSourceGraph {
    /// Build ownership for a selected-source operation. Generic bare-program
    /// execution and the source predicate classification remain unchanged.
    pub(crate) fn syntax_ownership(&self, dataset: &RdfDataset) -> SyntaxOwnership {
        let mut ownership = SyntaxOwnership::default();
        for (ordinal, edge) in self.edges().iter().enumerate() {
            let child = SourceNode {
                term: edge.target,
                graph: edge.source.graph,
            };
            let families: &[(SyntaxFamily, SyntaxFamily)] = match edge.role {
                SourceEdgeRole::FormulaComponent => {
                    &[(SyntaxFamily::Formula, SyntaxFamily::Formula)]
                }
                SourceEdgeRole::ExclusiveFormulaUse => {
                    &[(SyntaxFamily::Owner, SyntaxFamily::Formula)]
                }
                SourceEdgeRole::RecoveryOwnership => &[(SyntaxFamily::Owner, SyntaxFamily::Owner)],
                SourceEdgeRole::RuleComponent => &[(SyntaxFamily::Rule, SyntaxFamily::Atom)],
                SourceEdgeRole::Reification => &[(SyntaxFamily::Atom, SyntaxFamily::Atom)],
                SourceEdgeRole::TermComponent => &[
                    (SyntaxFamily::Formula, SyntaxFamily::Term),
                    (SyntaxFamily::Atom, SyntaxFamily::Term),
                    (SyntaxFamily::Term, SyntaxFamily::Term),
                ],
                _ => &[],
            };
            for &(parent_family, child_family) in families {
                ownership.child(
                    SyntaxOwner {
                        node: edge.source,
                        family: parent_family,
                    },
                    SyntaxOwner {
                        node: child,
                        family: child_family,
                    },
                    SyntaxEvidence::Edge(ordinal),
                );
            }
            if edge.role == SourceEdgeRole::Reification
                && let TermRef::Triple { s, p, o } = dataset.resolve(edge.target)
            {
                for term in [s, p, o] {
                    let node = SourceNode {
                        term,
                        graph: edge.source.graph,
                    };
                    if atomic_term_carrier(dataset, node) {
                        ownership.child(
                            SyntaxOwner {
                                node: child,
                                family: SyntaxFamily::Atom,
                            },
                            SyntaxOwner {
                                node,
                                family: SyntaxFamily::Term,
                            },
                            SyntaxEvidence::Edge(ordinal),
                        );
                    }
                }
            }
        }
        let nil = dataset.term_id_by_iri(crate::lists::RDF_NIL);
        let first = dataset.term_id_by_iri(crate::lists::RDF_FIRST);
        let rest = dataset.term_id_by_iri(crate::lists::RDF_REST);
        for (quad, carrier) in source_statements(dataset) {
            let binding = super::SourceStatementBinding {
                canonical: quad,
                carrier,
            };
            let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
                continue;
            };
            let node = SourceNode {
                term: quad.s,
                graph: quad.g,
            };
            if matches!(predicate, crate::lists::RDF_FIRST | crate::lists::RDF_REST) {
                // Record every authored branch, including malformed ones. Only
                // a constructor's actual head activates a list family; an
                // invalid ownership claim cannot promote its children globally.
                for family in [
                    SyntaxFamily::ClassList,
                    SyntaxFamily::ValueList,
                    SyntaxFamily::PropertyList,
                    SyntaxFamily::FacetList,
                ] {
                    let cell = SyntaxOwner { node, family };
                    ownership.row(cell, binding, SyntaxEvidence::Statement(binding));
                    if predicate == crate::lists::RDF_REST
                        && Some(quad.o) != nil
                        && is_resource(dataset, quad.o)
                    {
                        ownership.child(
                            cell,
                            SyntaxOwner {
                                node: SourceNode {
                                    term: quad.o,
                                    graph: quad.g,
                                },
                                family,
                            },
                            SyntaxEvidence::Statement(binding),
                        );
                    }
                }
                if predicate == crate::lists::RDF_FIRST {
                    if class_expression(
                        dataset,
                        SourceNode {
                            term: quad.o,
                            graph: quad.g,
                        },
                    ) {
                        ownership.child(
                            SyntaxOwner {
                                node,
                                family: SyntaxFamily::ClassList,
                            },
                            SyntaxOwner {
                                node: SourceNode {
                                    term: quad.o,
                                    graph: quad.g,
                                },
                                family: SyntaxFamily::Class,
                            },
                            SyntaxEvidence::Statement(binding),
                        );
                    }
                    if property_expression(
                        dataset,
                        SourceNode {
                            term: quad.o,
                            graph: quad.g,
                        },
                    ) {
                        ownership.child(
                            SyntaxOwner {
                                node,
                                family: SyntaxFamily::PropertyList,
                            },
                            SyntaxOwner {
                                node: SourceNode {
                                    term: quad.o,
                                    graph: quad.g,
                                },
                                family: SyntaxFamily::Schema,
                            },
                            SyntaxEvidence::Statement(binding),
                        );
                    }
                    if is_resource(dataset, quad.o) {
                        for facet in fields(
                            dataset,
                            SourceNode {
                                term: quad.o,
                                graph: quad.g,
                            },
                            None,
                        ) {
                            if matches!(dataset.resolve(facet.canonical.p), TermRef::Iri(iri)
                                if iri.starts_with("http://www.w3.org/2001/XMLSchema#"))
                            {
                                ownership.row(
                                    SyntaxOwner {
                                        node,
                                        family: SyntaxFamily::FacetList,
                                    },
                                    facet,
                                    SyntaxEvidence::Statement(binding),
                                );
                            }
                        }
                    }
                }
                continue;
            }
            let owner = SyntaxOwner {
                node: SourceNode {
                    term: quad.s,
                    graph: quad.g,
                },
                family: SyntaxFamily::Class,
            };
            let list_members = match gmeow_ns::schema_predicate_role(predicate) {
                Some(gmeow_ns::SchemaPredicateRole::Members) => member_kinds(dataset, node),
                Some(gmeow_ns::SchemaPredicateRole::DistinctMembers) => {
                    [Some(ListMembers::Values), None, None]
                }
                role => [
                    match role {
                        Some(gmeow_ns::SchemaPredicateRole::ClassList) => {
                            Some(ListMembers::Classes)
                        }
                        Some(gmeow_ns::SchemaPredicateRole::ValueList) => Some(ListMembers::Values),
                        Some(gmeow_ns::SchemaPredicateRole::PropertyList) => {
                            Some(ListMembers::Properties)
                        }
                        Some(gmeow_ns::SchemaPredicateRole::FacetList) => Some(ListMembers::Facets),
                        _ => None,
                    },
                    None,
                    None,
                ],
            };
            if list_members.iter().any(Option::is_some) {
                // Validate one actual head through the native strict walker.
                // Physical duplicate carriers retain evidence but not extra values.
                let edge = |cell, predicate: Option<TermId>| {
                    predicate.map_or(purrdf_core::SoleObject::None, |predicate| {
                        let values: BTreeSet<_> = fields(
                            dataset,
                            SourceNode {
                                term: cell,
                                graph: quad.g,
                            },
                            Some(predicate),
                        )
                        .map(|row| row.canonical.o)
                        .collect();
                        purrdf_core::SoleObject::of(values.into_iter())
                    })
                };
                if let Err(error) = purrdf_core::walk_rdf_list(
                    quad.o,
                    nil,
                    |cell| edge(cell, first),
                    |cell| edge(cell, rest),
                ) {
                    ownership.issues.entry(owner).or_default().push(
                        SyntaxIssue::MalformedCollection {
                            source: binding,
                            detail: format!(
                                "finite nonbranching RDF collection required: {error:?}"
                            ),
                        },
                    );
                }
                for members in list_members.into_iter().flatten() {
                    ownership.list_head(dataset, owner, binding, members);
                }
                continue;
            }
            let canonical = gmeow_ns::canonical_calculus_symbol(predicate).unwrap_or(predicate);
            let Some(local) = canonical.strip_prefix(LOGIC_NAMESPACE) else {
                continue;
            };
            if matches!(
                local,
                "allValuesFrom" | "someValuesFrom" | "onClass" | "onDataRange" | "complementOf"
            ) && class_expression(
                dataset,
                SourceNode {
                    term: quad.o,
                    graph: quad.g,
                },
            ) {
                ownership.child(
                    owner,
                    SyntaxOwner {
                        node: SourceNode {
                            term: quad.o,
                            graph: quad.g,
                        },
                        family: SyntaxFamily::Class,
                    },
                    SyntaxEvidence::Statement(binding),
                );
            } else if local == "onProperty"
                && property_expression(
                    dataset,
                    SourceNode {
                        term: quad.o,
                        graph: quad.g,
                    },
                )
            {
                ownership.child(
                    owner,
                    SyntaxOwner {
                        node: SourceNode {
                            term: quad.o,
                            graph: quad.g,
                        },
                        family: SyntaxFamily::Schema,
                    },
                    SyntaxEvidence::Statement(binding),
                );
            } else if gmeow_ns::is_class_position_predicate(predicate)
                && class_expression(
                    dataset,
                    SourceNode {
                        term: quad.o,
                        graph: quad.g,
                    },
                )
            {
                ownership.child(
                    SyntaxOwner {
                        node,
                        family: SyntaxFamily::Schema,
                    },
                    SyntaxOwner {
                        node: SourceNode {
                            term: quad.o,
                            graph: quad.g,
                        },
                        family: SyntaxFamily::Class,
                    },
                    SyntaxEvidence::Statement(binding),
                );
            }
        }
        ownership
    }
}
