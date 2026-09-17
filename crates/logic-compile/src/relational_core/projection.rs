// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Build one native projection; serialization belongs to terminal text consumers.

use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, TermId};

use super::*;

/// Project the complete relational dialect directly into a native default graph.
/// Atom signs, positional conjunctions, typed terms and loss evidence survive intact.
pub fn project_relational_core_dataset(
    program: &RelationalCoreProgram,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut graph = Projection {
        builder: RdfDatasetBuilder::new(),
    };
    let root = graph.builder.intern_iri(&program_iri());
    graph.iri_edge(root, RDF_TYPE, &class_program())?;
    graph.iri_edge(root, &p_has_preservation(), &program.preservation().iri())?;
    if let Some(source) = &program.source_iri {
        graph.text(root, &p_source_iri(), source);
    }
    for residue in &program.residue {
        graph.text(root, &p_lossy_drop(), &residue.reason);
    }
    for fact in &program.facts {
        let node = graph.builder.intern_iri(&fact_iri(fact));
        graph.edge(root, &p_has_fact(), node);
        graph.atom(node, fact, &class_fact())?;
    }
    for rule in &program.rules {
        // One framed key per rule, reused for every positional record.
        let key = rule.key();
        let node = graph.builder.intern_iri(&format!(
            "{LOGIC_NAMESPACE}relational-core/rule/{}",
            sha256_hex(&key)
        ));
        graph.edge(root, &p_has_rule(), node);
        graph.iri_edge(node, RDF_TYPE, &class_rule())?;
        let head = graph
            .builder
            .intern_iri(&atom_node_iri("head", &rule.head.key()));
        graph.atom(head, &rule.head, &class_atom())?;
        graph.edge(node, &p_rc_head(), head);
        for (field, prefix, atoms) in [
            (p_rc_head_conjunct(), "headconjunct", &rule.head_conjuncts),
            (p_rc_body(), "body", &rule.body),
        ] {
            for (index, atom) in atoms.iter().enumerate() {
                let position = index.to_string();
                let scoped = identity::frame("position", [&key, &position]);
                let record = graph.builder.intern_iri(&atom_node_iri(prefix, &scoped));
                graph.atom(record, atom, &class_atom())?;
                graph.edge(node, &field, record);
                let value = graph.builder.intern_literal(RdfLiteral::typed(
                    &position,
                    "http://www.w3.org/2001/XMLSchema#integer",
                ));
                graph.edge(record, &p_rc_index(), value);
            }
        }
        for (index, call) in rule.numeric.iter().enumerate() {
            call.validate().map_err(term::error)?;
            let position = index.to_string();
            let scoped = identity::frame("numeric-position", [&key, &position]);
            let record = graph.builder.intern_iri(&atom_node_iri("numeric", &scoped));
            graph.edge(node, &iri("rcNumeric"), record);
            graph.iri_edge(record, RDF_TYPE, &iri("RelationalCoreNumeric"))?;
            graph.iri_edge(record, &iri("rcNumericOperator"), call.operator.iri())?;
            for (field, value) in [
                ("rcNumericLeft", Some(&call.left)),
                ("rcNumericRight", Some(&call.right)),
                ("rcNumericResult", call.result.as_ref()),
            ] {
                if let Some(value) = value {
                    let value = term::project(&mut graph.builder, value)?;
                    graph.edge(record, &iri(field), value);
                }
            }
            let value = graph.builder.intern_literal(RdfLiteral::typed(
                &position,
                "http://www.w3.org/2001/XMLSchema#integer",
            ));
            graph.edge(record, &p_rc_index(), value);
        }
        for (left, right) in &rule.distinct_pairs {
            let scoped = identity::frame("distinct", [&key, left, right]);
            let record = graph
                .builder
                .intern_iri(&atom_node_iri("distinct", &scoped));
            graph.edge(node, &p_rc_distinct(), record);
            graph.text(record, &p_rc_distinct_left(), left);
            graph.text(record, &p_rc_distinct_right(), right);
        }
    }
    graph.builder.freeze().map_err(term::error)
}

/// Serialize a valid relational-core program for a terminal N-Triples consumer.
/// Production stages use [`project_relational_core_dataset`] and propagate errors.
pub fn project_relational_core(program: &RelationalCoreProgram) -> String {
    let dataset =
        project_relational_core_dataset(program).expect("valid relational-core projection");
    let bytes = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .expect("serialize relational-core projection");
    let text = String::from_utf8(bytes).expect("N-Triples is UTF-8");
    let mut lines: Vec<_> = text.lines().filter(|line| !line.is_empty()).collect();
    lines.sort_unstable();
    lines.dedup();
    format!("{}\n", lines.join("\n"))
}

struct Projection {
    builder: RdfDatasetBuilder,
}

impl Projection {
    fn edge(&mut self, subject: TermId, predicate: &str, object: TermId) {
        let predicate = self.builder.intern_iri(predicate);
        self.builder.push_quad(subject, predicate, object, None);
    }

    fn iri_edge(
        &mut self,
        subject: TermId,
        predicate: &str,
        value: &str,
    ) -> gmeow_errors::Result<()> {
        let object = term::iri(&mut self.builder, value)?;
        self.edge(subject, predicate, object);
        Ok(())
    }

    fn text(&mut self, subject: TermId, predicate: &str, value: &str) {
        let object = self.builder.intern_literal(RdfLiteral::simple(value));
        self.edge(subject, predicate, object);
    }

    fn atom(&mut self, node: TermId, atom: &RcAtom, class: &str) -> gmeow_errors::Result<()> {
        self.iri_edge(node, RDF_TYPE, class)?;
        let subject = term::project(&mut self.builder, &atom.subject)?;
        let object = term::project(&mut self.builder, &atom.object)?;
        self.edge(node, &p_rc_subject(), subject);
        self.iri_edge(node, &p_rc_predicate(), &atom.predicate)?;
        self.edge(node, &p_rc_object(), object);
        let sign = self.builder.intern_literal(RdfLiteral::typed(
            if atom.negated { "true" } else { "false" },
            "http://www.w3.org/2001/XMLSchema#boolean",
        ));
        self.edge(node, &p_rc_negated(), sign);
        Ok(())
    }
}
