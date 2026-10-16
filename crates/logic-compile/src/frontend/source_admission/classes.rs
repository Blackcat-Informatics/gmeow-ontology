// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Checked source-class laws. Definitions use the formula core, not the compiler's
//! content-hash IRIs as substitutes for source class-object identity.

use super::*;

struct ClassLowering {
    definition: SourceClassDefinition,
    support: Vec<QuadIds>,
    dependencies: BTreeSet<SourceNode>,
}

/// A lowering can refuse a law; it cannot return admitted data as its error.
enum ClassRefusal {
    Blocked { code: &'static str, reason: String },
    Malformed { code: &'static str, reason: String },
}

impl From<ClassRefusal> for SemanticUnitDisposition {
    fn from(refusal: ClassRefusal) -> Self {
        match refusal {
            ClassRefusal::Blocked { code, reason } => blocked(code, reason),
            ClassRefusal::Malformed { code, reason } => malformed(code, reason),
        }
    }
}

impl AdmissionBuilder<'_> {
    pub(super) fn admit_classes(&mut self) {
        let dataset = self.theory.source().dataset();
        let mut roots: BTreeSet<_> = self
            .theory
            .axiom_sources()
            .iter()
            .flatten()
            .filter_map(|source| {
                if let AxiomSource::ClassExpression(node) = source {
                    Some(*node)
                } else {
                    None
                }
            })
            .collect();
        roots.extend(
            self.theory
                .source()
                .source_graph()
                .units()
                .filter(|unit| {
                    unit.declared_kinds
                        .contains(&SourceUnitKind::ClassExpression)
                })
                .map(|unit| unit.node),
        );
        for quad in dataset.quads().filter(|quad| quad.g.is_none()) {
            if let TermRef::Iri(predicate) = dataset.resolve(quad.p)
                && predicate.strip_prefix(LOGIC).is_some_and(|local| {
                    class_field(local) && !matches!(local, "onClass" | "onDataRange")
                })
            {
                roots.insert(SourceNode {
                    term: quad.s,
                    graph: None,
                });
            }
        }
        let mut lowerings = BTreeMap::new();
        let mut dispositions = BTreeMap::new();
        for root in &roots {
            let documents = self.node_documents(*root);
            if root.graph.is_some() || !self.documents_selected(&documents) {
                dispositions.insert(
                    *root,
                    outside("class definition is outside the selected default-graph documents"),
                );
                continue;
            }
            match self.lower_class(*root, &roots) {
                Ok(lowering) => {
                    lowerings.insert(*root, lowering);
                }
                Err(disposition) => {
                    dispositions.insert(*root, disposition.into());
                }
            }
        }
        // Mutual definitions are first-order laws, not recursive syntax expansion.
        // Propagate refusals through reverse edges once, without repeatedly
        // scanning all definitions along a long dependency chain.
        let mut dependents: BTreeMap<SourceNode, Vec<SourceNode>> = BTreeMap::new();
        let mut refused = BTreeSet::new();
        for (node, lowering) in &lowerings {
            for dependency in &lowering.dependencies {
                dependents.entry(*dependency).or_default().push(*node);
                if !lowerings.contains_key(dependency) {
                    refused.insert(*node);
                }
            }
        }
        while let Some(node) = refused.pop_first() {
            if lowerings.remove(&node).is_none() {
                continue;
            }
            dispositions.insert(
                node,
                blocked(
                    "UNADMITTED_CLASS_DEPENDENCY",
                    "a class law depends on an excluded or unsupported class definition",
                ),
            );
            if let Some(consumers) = dependents.remove(&node) {
                refused.extend(consumers);
            }
        }
        for (node, lowering) in lowerings {
            self.certified_classes.insert(node);
            self.class_support
                .extend(
                    lowering
                        .support
                        .into_iter()
                        .map(|canonical| SourceStatementBinding {
                            canonical,
                            carrier: SourceCarrier::Statement,
                        }),
                );
            dispositions.insert(
                node,
                SemanticUnitDisposition::Admitted(AdmittedSemanticUnit::ClassDefinition(
                    lowering.definition,
                )),
            );
        }
        for (node, disposition) in dispositions {
            self.push(
                SemanticUnitKind::ClassDefinition,
                node_anchor(dataset, node),
                self.node_documents(node),
                disposition,
            );
        }
    }

    fn lower_class(
        &self,
        node: SourceNode,
        roots: &BTreeSet<SourceNode>,
    ) -> Result<ClassLowering, ClassRefusal> {
        let dataset = self.theory.source().dataset();
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
            return Err(ClassRefusal::Blocked {
                code: "CONTEXT_ERASURE_FORBIDDEN",
                reason: "a scoped class definition requires typed context translation".into(),
            });
        }
        let mut fields: BTreeMap<String, TermId> = BTreeMap::new();
        let mut support = Vec::new();
        for quad in dataset.quads_for_pattern(Some(node.term), None, None, GraphMatch::Default) {
            let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
                continue;
            };
            if predicate
                .strip_prefix("http://www.w3.org/2002/07/owl#")
                .is_some_and(class_field)
            {
                return Err(unsupported_class(
                    "external class constructors require canonical source lowering",
                ));
            }
            let Some(local) = predicate
                .strip_prefix(LOGIC)
                .filter(|local| class_field(local))
            else {
                continue;
            };
            if !self.statement_selected(quad, SourceCarrier::Statement) {
                return Err(unsupported_class(
                    "class-definition support is outside the selected documents",
                ));
            }
            if let Some(previous) = fields.insert(local.to_owned(), quad.o)
                && previous != quad.o
            {
                return Err(ClassRefusal::Malformed {
                    code: "AMBIGUOUS_CLASS_CONSTRAINT",
                    reason: format!("class definition has multiple logic:{local} values"),
                });
            }
            support.push(quad);
        }
        if let Some(unsupported) = fields.keys().find(|local| {
            !matches!(
                local.as_str(),
                "onProperty" | "someValuesFrom" | "allValuesFrom" | "hasValue"
            )
        }) {
            return Err(unsupported_class(format!(
                "logic:{unsupported} has no certified source-class formula lowering"
            )));
        }
        let property = fields
            .get("onProperty")
            .ok_or_else(|| unsupported_class("a restriction requires logic:onProperty"))?;
        let TermRef::Iri(property) = dataset.resolve(*property) else {
            return Err(unsupported_class(
                "logic:onProperty requires an IRI-valued relation",
            ));
        };
        if self.is_procedural_predicate(property) {
            return Err(unsupported_class(
                "procedural relations cannot define classical property restrictions",
            ));
        }
        if StructuralSourceGraph::predicate_role(property).is_some() {
            return Err(unsupported_class(
                "source grammar relations require a certified restriction interpretation",
            ));
        }
        if data_resource(
            dataset,
            *fields.get("onProperty").expect("checked property"),
        ) {
            return Err(unsupported_class(
                "datatype properties require a certified datatype interpretation",
            ));
        }
        let mut constants = BTreeMap::new();
        let mut bind = |id| -> Result<Term, ClassRefusal> {
            if !matches!(dataset.resolve(id), TermRef::Iri(_) | TermRef::Blank { .. }) {
                return Err(unsupported_class(
                    "literal and quoted class terms require a certified value interpretation",
                ));
            }
            let value =
                atomic_term(dataset, id, false).map_err(|error| ClassRefusal::Malformed {
                    code: "MALFORMED_CLASS_TERM",
                    reason: error.to_string(),
                })?;
            let name = format!("__source_constant_{}", constants.len());
            constants.insert(name.clone(), value);
            Ok(Term::Var(name))
        };
        let root = bind(node.term)?;
        let x = Term::Var("class_member".into());
        let y = Term::Var("class_value".into());
        let mut parts = Vec::new();
        let mut dependencies = BTreeSet::new();
        for local in ["someValuesFrom", "allValuesFrom", "hasValue"] {
            let Some(value) = fields.get(local) else {
                continue;
            };
            if local == "hasValue" {
                parts.push(atom(property, vec![x.clone(), bind(*value)?]));
                continue;
            }
            if data_resource(dataset, *value) {
                return Err(unsupported_class(
                    "datatype membership requires a certified datatype interpretation",
                ));
            }
            if let TermRef::Iri(iri) = dataset.resolve(*value)
                && (iri.starts_with("http://www.w3.org/2001/XMLSchema#")
                    || iri == "http://www.w3.org/2000/01/rdf-schema#Literal")
            {
                return Err(unsupported_class(
                    "datatype membership requires a certified datatype interpretation",
                ));
            }
            let target = SourceNode {
                term: *value,
                graph: None,
            };
            if roots.contains(&target) {
                dependencies.insert(target);
            }
            let membership = atom(LOGIC_INSTANCE_OF, vec![y.clone(), bind(*value)?]);
            let relation = atom(property, vec![x.clone(), y.clone()]);
            let body = if local == "someValuesFrom" {
                Formula::Exists {
                    vars: vec!["class_value".into()],
                    body: Box::new(Formula::And(vec![relation, membership])),
                }
            } else {
                Formula::Forall {
                    vars: vec!["class_value".into()],
                    body: Box::new(Formula::Implies(Box::new(relation), Box::new(membership))),
                }
            };
            parts.push(body);
        }
        let condition = match parts.len() {
            0 => {
                return Err(unsupported_class(
                    "a restriction requires at least one admitted constraint",
                ));
            }
            1 => parts.pop().expect("one condition"),
            _ => Formula::And(parts),
        };
        let formula = Formula::Forall {
            vars: vec!["class_member".into()],
            body: Box::new(Formula::Iff(
                Box::new(atom(LOGIC_INSTANCE_OF, vec![x, root])),
                Box::new(condition),
            )),
        };
        Ok(ClassLowering {
            definition: SourceClassDefinition { formula, constants },
            support,
            dependencies,
        })
    }
}

fn atom(relation: &str, args: Vec<Term>) -> Formula {
    Formula::Atom {
        relation: Term::Iri(relation.to_owned()),
        args,
    }
}

fn unsupported_class(reason: impl Into<String>) -> ClassRefusal {
    ClassRefusal::Blocked {
        code: "UNSUPPORTED_CLASS_EXPRESSION",
        reason: reason.into(),
    }
}

fn class_field(local: &str) -> bool {
    local == "onProperty"
        || crate::restriction::CONSTRAINT_LOCALS.contains(&local)
        || matches!(
            local,
            "hasSelf"
                | "oneOf"
                | "unionOf"
                | "intersectionOf"
                | "complementOf"
                | "disjointUnionOf"
                | "onDatatype"
                | "withRestrictions"
        )
}

fn data_resource(dataset: &RdfDataset, subject: TermId) -> bool {
    dataset
        .quads_for_pattern(Some(subject), None, None, GraphMatch::Default)
        .any(|quad| {
            matches!(
                dataset.resolve(quad.p),
                TermRef::Iri(RDF_TYPE | LOGIC_INSTANCE_OF)
            ) && matches!(
                dataset.resolve(quad.o),
                TermRef::Iri(
                    "http://www.w3.org/2000/01/rdf-schema#Datatype"
                        | "http://www.w3.org/2002/07/owl#DatatypeProperty"
                        | "https://blackcatinformatics.ca/logic/Datatype"
                        | "https://blackcatinformatics.ca/logic/Datarange"
                        | "https://blackcatinformatics.ca/logic/DatatypeProperty"
                )
            )
        })
}
