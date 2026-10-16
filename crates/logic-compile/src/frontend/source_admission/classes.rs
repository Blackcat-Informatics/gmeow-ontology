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
        if fields.keys().any(|local| {
            matches!(
                local.as_str(),
                "unionOf" | "intersectionOf" | "complementOf" | "oneOf" | "disjointUnionOf"
            )
        }) {
            return self.lower_boolean_class(node, &fields, support, roots);
        }
        if let Some(unsupported) = fields.keys().find(|local| {
            !matches!(
                local.as_str(),
                "onProperty" | "someValuesFrom" | "allValuesFrom" | "hasValue" | "hasSelf"
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
        if let Some(flag) = fields.get("hasSelf") {
            let TermRef::Literal {
                lexical,
                datatype,
                language: None,
                direction: None,
            } = dataset.resolve(*flag)
            else {
                return Err(unsupported_class(
                    "hasSelf requires an admitted true native boolean",
                ));
            };
            let TermRef::Iri("http://www.w3.org/2001/XMLSchema#boolean") =
                dataset.resolve(datatype)
            else {
                return Err(unsupported_class(
                    "hasSelf requires an admitted true native boolean",
                ));
            };
            if !matches!(
                purrdf::xsd::parse_by_iri(lexical, "http://www.w3.org/2001/XMLSchema#boolean"),
                Ok(Some(purrdf::xsd::XsdValue::Boolean(true)))
            ) {
                return Err(unsupported_class(
                    "hasSelf requires an admitted true native boolean",
                ));
            }
            parts.push(atom(property, vec![x.clone(), x.clone()]));
        }
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

    fn lower_boolean_class(
        &self,
        node: SourceNode,
        fields: &BTreeMap<String, TermId>,
        mut support: Vec<QuadIds>,
        roots: &BTreeSet<SourceNode>,
    ) -> Result<ClassLowering, ClassRefusal> {
        let dataset = self.theory.source().dataset();
        if fields.len() != 1 {
            return Err(unsupported_class(
                "combined class constructors require separate certified definition laws",
            ));
        }
        if data_resource(dataset, node.term) {
            return Err(unsupported_class(
                "datatype constructors require a certified value interpretation",
            ));
        }
        let (constructor, value) = fields.first_key_value().expect("one constructor");
        let mut members = if constructor == "complementOf" {
            vec![*value]
        } else {
            self.selected_class_list(*value, &mut support)?
        };
        if constructor == "disjointUnionOf" {
            // This constructor takes a set. Repeating a source member must not
            // manufacture a self-disjointness law that forces it to be empty.
            let mut distinct = BTreeSet::new();
            members.retain(|member| distinct.insert(*member));
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
        let mut dependencies = BTreeSet::new();
        let mut parts = Vec::new();
        for member in members {
            if constructor != "oneOf" {
                if data_resource(dataset, member)
                    || matches!(dataset.resolve(member), TermRef::Iri(iri)
                        if iri.starts_with("http://www.w3.org/2001/XMLSchema#")
                        || iri == "http://www.w3.org/2000/01/rdf-schema#Literal")
                {
                    return Err(unsupported_class(
                        "datatype membership requires a certified value interpretation",
                    ));
                }
                let dependency = SourceNode {
                    term: member,
                    graph: None,
                };
                if roots.contains(&dependency) {
                    dependencies.insert(dependency);
                }
            }
            parts.push(atom(
                if constructor == "oneOf" {
                    "https://blackcatinformatics.ca/logic/sameAs"
                } else {
                    LOGIC_INSTANCE_OF
                },
                vec![x.clone(), bind(member)?],
            ));
        }
        let mut disjointness = Vec::new();
        if constructor == "disjointUnionOf" {
            for (index, left) in parts.iter().enumerate() {
                for right in &parts[index + 1..] {
                    disjointness.push(Formula::Not(Box::new(Formula::And(vec![
                        left.clone(),
                        right.clone(),
                    ]))));
                }
            }
        }
        let condition = match constructor.as_str() {
            "complementOf" => Formula::Not(Box::new(parts.pop().expect("one complement"))),
            _ if parts.len() == 1 => parts.pop().expect("one member"),
            "intersectionOf" => Formula::And(parts),
            _ => Formula::Or(parts),
        };
        let membership = Formula::Iff(
            Box::new(atom(LOGIC_INSTANCE_OF, vec![x, root])),
            Box::new(condition),
        );
        let body = if disjointness.is_empty() {
            membership
        } else {
            let mut laws = vec![membership];
            laws.extend(disjointness);
            Formula::And(laws)
        };
        Ok(ClassLowering {
            definition: SourceClassDefinition {
                formula: Formula::Forall {
                    vars: vec!["class_member".into()],
                    body: Box::new(body),
                },
                constants,
            },
            support,
            dependencies,
        })
    }

    fn selected_class_list(
        &self,
        head: TermId,
        support: &mut Vec<QuadIds>,
    ) -> Result<Vec<TermId>, ClassRefusal> {
        use purrdf_core::SoleObject;

        let dataset = self.theory.source().dataset();
        let first = dataset.term_id_by_iri(crate::lists::RDF_FIRST);
        let rest = dataset.term_id_by_iri(crate::lists::RDF_REST);
        let nil = dataset.term_id_by_iri(crate::lists::RDF_NIL);
        let mut first_support = Vec::new();
        let mut rest_support = Vec::new();
        let edge = |cell: TermId, predicate: Option<TermId>, evidence: &mut Vec<QuadIds>| {
            predicate.map_or(SoleObject::None, |predicate| {
                SoleObject::of(
                    dataset
                        .quads_for_pattern(Some(cell), Some(predicate), None, GraphMatch::Default)
                        .map(|quad| {
                            evidence.push(quad);
                            quad.o
                        }),
                )
            })
        };
        let members = purrdf_core::walk_rdf_list(
            head,
            nil,
            |cell| edge(cell, first, &mut first_support),
            |cell| edge(cell, rest, &mut rest_support),
        )
        .map_err(|error| ClassRefusal::Malformed {
            code: "MALFORMED_CLASS_LIST",
            reason: format!(
                "class constructor requires a finite nonbranching RDF list: {:?}",
                error.kind
            ),
        })?;
        if members.is_empty() {
            return Err(ClassRefusal::Malformed {
                code: "MALFORMED_CLASS_LIST",
                reason: "class constructor lists must be nonempty".into(),
            });
        }
        for quad in first_support.into_iter().chain(rest_support) {
            if !self.statement_selected(quad, SourceCarrier::Statement) {
                return Err(unsupported_class(
                    "class-list support is outside the selected documents",
                ));
            }
            support.push(quad);
        }
        Ok(members)
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
