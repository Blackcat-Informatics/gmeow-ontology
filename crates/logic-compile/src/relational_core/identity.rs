// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native program identity, including blanks shared between facts and rules.
//!
//! A temporary typed structural graph gives PurRDF one complete canonicalization
//! problem. No RDF text is parsed and no failure selects a weaker key. Internal
//! record nodes occupy a separate blank scope from the program's own constants.

use std::collections::BTreeSet;

use gmeow_errors::Diag;
use purrdf::{BlankScope, RdfDatasetBuilder, RdfLiteral, TermId};

use super::{RcAtom, RcRule, RcTerm, RelationalCoreProgram};

const NS: &str = "urn:gmeow:relational-identity:";
const RECORD_SCOPE: BlankScope = BlankScope(1);

/// Framed components cannot be confused with separators inside literal values,
/// variable names, residue messages, or nested term keys.
pub(super) use crate::ir::atomic::frame;

fn error(detail: impl std::fmt::Display) -> Diag {
    Diag::of_kind(crate::error::RelationalCore {
        detail: format!("relational-core identity: {detail}"),
    })
}

pub(super) fn program_key(program: &RelationalCoreProgram) -> gmeow_errors::Result<String> {
    graph_key(program, false)
}

/// Transport identity also preserves recorded body order and repeated occurrences.
pub(super) fn projection_key(program: &RelationalCoreProgram) -> gmeow_errors::Result<String> {
    graph_key(program, true)
}

fn graph_key(program: &RelationalCoreProgram, ordered: bool) -> gmeow_errors::Result<String> {
    let mut graph = IdentityGraph {
        builder: RdfDatasetBuilder::new(),
        next_record: 0,
    };
    let root = graph.builder.intern_iri(&format!("{NS}program"));
    let preservation = graph.text(program.preservation().as_str());
    graph.edge(root, "preservation", preservation);
    if let Some(source) = &program.source_iri {
        let value = graph.text(source);
        graph.edge(root, "source", value);
    }
    for fact in program.facts.iter().collect::<BTreeSet<_>>() {
        let node = graph.atom(fact)?;
        graph.edge(root, "fact", node);
    }
    for rule in program.rules.iter().collect::<BTreeSet<_>>() {
        let node = graph.rule(rule, ordered)?;
        graph.edge(root, "rule", node);
    }
    for residue in &program.residue {
        let value = graph.text(&residue.reason);
        graph.edge(root, "residue", value);
    }
    let dataset = graph.builder.freeze().map_err(error)?;
    let canonical = purrdf::try_canonicalize(&dataset).map_err(error)?;
    Ok(frame(
        if ordered {
            "RELATIONAL-CORE-PROJECTION-v1"
        } else {
            "RELATIONAL-CORE-v3"
        },
        [canonical.nquads],
    ))
}

struct IdentityGraph {
    builder: RdfDatasetBuilder,
    next_record: usize,
}

impl IdentityGraph {
    fn record(&mut self) -> TermId {
        let node = self
            .builder
            .intern_blank(&self.next_record.to_string(), RECORD_SCOPE);
        self.next_record += 1;
        node
    }

    fn text(&mut self, value: &str) -> TermId {
        self.builder.intern_literal(RdfLiteral::simple(value))
    }

    fn edge(&mut self, subject: TermId, property: &str, object: TermId) {
        let predicate = self.builder.intern_iri(&format!("{NS}{property}"));
        self.builder.push_quad(subject, predicate, object, None);
    }

    fn iri(&mut self, value: &str) -> gmeow_errors::Result<TermId> {
        // The native interner expects admitted absolute IRIs. Validate before
        // entering that trusted boundary, returning the refusal to the caller.
        purrdf::iri::BaseIri::parse(value).map_err(error)?;
        Ok(self.builder.intern_iri(value))
    }

    fn term(&mut self, term: &RcTerm) -> gmeow_errors::Result<TermId> {
        super::term::encode(&mut self.builder, term)
    }

    fn atom(&mut self, atom: &RcAtom) -> gmeow_errors::Result<TermId> {
        let node = self.record();
        let subject = self.term(&atom.subject)?;
        let predicate = self.iri(&atom.predicate)?;
        let object = self.term(&atom.object)?;
        let negated = self.text(if atom.negated { "true" } else { "false" });
        self.edge(node, "subject", subject);
        self.edge(node, "predicate", predicate);
        self.edge(node, "object", object);
        self.edge(node, "negated", negated);
        Ok(node)
    }

    fn rule(&mut self, rule: &RcRule, ordered: bool) -> gmeow_errors::Result<TermId> {
        let node = self.record();
        let head = self.atom(&rule.head)?;
        self.edge(node, "head", head);
        for (index, conjunct) in rule.head_conjuncts.iter().enumerate() {
            let atom = self.atom(conjunct)?;
            self.edge(node, &format!("head-conjunct/{index}"), atom);
        }
        if ordered {
            for (index, atom) in rule.body.iter().enumerate() {
                let atom = self.atom(atom)?;
                self.edge(node, &format!("body/{index}"), atom);
            }
        } else {
            // Semantic conjunction is a set; transport identity above also binds
            // the recorded evaluation sequence, independently of blank labels.
            for atom in rule.body.iter().collect::<BTreeSet<_>>() {
                let atom = self.atom(atom)?;
                self.edge(node, "body", atom);
            }
        }
        for (index, call) in rule.numeric.iter().enumerate() {
            call.validate().map_err(error)?;
            let record = self.record();
            let operator = self.iri(call.operator.iri())?;
            let left = self.term(&call.left)?;
            let right = self.term(&call.right)?;
            self.edge(record, "numeric-operator", operator);
            self.edge(record, "numeric-left", left);
            self.edge(record, "numeric-right", right);
            if let Some(result) = &call.result {
                let result = self.term(result)?;
                self.edge(record, "numeric-result", result);
            }
            self.edge(node, &format!("numeric/{index}"), record);
        }
        for (left, right) in rule.distinct_pairs.iter().collect::<BTreeSet<_>>() {
            let pair = self.record();
            let left = self.text(left);
            let right = self.text(right);
            self.edge(pair, "left", left);
            self.edge(pair, "right", right);
            self.edge(node, "distinct", pair);
        }
        Ok(node)
    }
}

#[path = "identity.tests.rs"]
#[cfg(test)]
mod tests;
