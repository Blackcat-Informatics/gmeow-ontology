// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared native source lookups and source-semantic admission.

use super::*;

impl Reader<'_> {
    /// The finite category has explicit indices, not implicit scope inheritance.
    /// Raw-source retention alone does not admit semantics absent from that index.
    pub(super) fn admit_scope(&mut self, node: TermId) -> gmeow_errors::Result<()> {
        let graph = self.graph;
        let mut pending = vec![node];
        while let Some(node) = pending.pop() {
            if self.checked_scope.contains(&node) {
                continue;
            }
            self.admit_direct_scope(node)?;
            self.checked_scope.insert(node);
            for annotation in graph.default_statement_annotations(node) {
                add_size(
                    &mut self.work,
                    1,
                    MAX_SOURCE_WORK,
                    "source statement metadata admission work",
                )?;
                pending.push(annotation);
            }
        }
        Ok(())
    }

    pub(super) fn admit_direct_scope(&mut self, node: TermId) -> gmeow_errors::Result<()> {
        if self.checked_scope.contains(&node) {
            return Ok(());
        }
        let edges = self.graph.outgoing(SourceNode {
            term: node,
            graph: None,
        });
        add_size(
            &mut self.work,
            edges.len() + 1,
            MAX_SOURCE_WORK,
            "source semantic admission work",
        )?;
        for edge in edges {
            let predicate = edge.predicate.iri(self.dataset);
            if matches!(
                edge.role,
                SourceEdgeRole::Module
                    | SourceEdgeRole::Standpoint
                    | SourceEdgeRole::Context
                    | SourceEdgeRole::Import
            ) || predicate.strip_prefix(LOGIC_NAMESPACE) == Some("variableSort")
            {
                return Err(self.error(node, &format!(
                    "{predicate} requires an explicit contextual or sorted presentation lowering; raw source retention does not discharge it"
                )));
            }
        }
        Ok(())
    }

    /// Reuse source ownership edges to cover nested formulas and term carriers.
    /// Shared syntax is checked once, without rebuilding an RDF adjacency cache.
    pub(super) fn admit_syntax(&mut self, root: TermId) -> gmeow_errors::Result<()> {
        let graph = self.graph;
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if self.checked_syntax.contains(&node) {
                continue;
            }
            self.admit_scope(node)?;
            let edges = graph.outgoing(SourceNode {
                term: node,
                graph: None,
            });
            add_size(
                &mut self.work,
                edges.len() + 1,
                MAX_SOURCE_WORK,
                "source syntax admission work",
            )?;
            for edge in edges {
                let predicate = edge.predicate.iri(self.dataset);
                if matches!(
                    predicate.strip_prefix(LOGIC_NAMESPACE),
                    Some("necessarily" | "possibly")
                ) {
                    return Err(self.error(node,
                        "modal source syntax requires a presentation-world-bound lowering; the compiler's default-world translation cannot certify it"));
                }
                if matches!(
                    edge.role,
                    SourceEdgeRole::FormulaComponent | SourceEdgeRole::TermComponent
                ) {
                    pending.push(edge.target);
                }
            }
            self.checked_syntax.insert(node);
        }
        Ok(())
    }

    pub(super) fn named(&self, node: TermId) -> gmeow_errors::Result<String> {
        match self.dataset.resolve(node) {
            TermRef::Iri(iri) => Ok(iri.to_owned()),
            _ => Err(self.error(node, "presentation record requires an IRI identity")),
        }
    }
    pub(super) fn error(&self, node: TermId, detail: &str) -> gmeow_errors::Diag {
        error(detail).with_focus(super::super::source_graph::focus(self.dataset, node))
    }
    pub(super) fn values(
        &mut self,
        node: TermId,
        property: &'static str,
    ) -> gmeow_errors::Result<Vec<TermId>> {
        let predicate = *self.predicates.entry(property).or_insert_with(|| {
            self.dataset
                .term_id_by_iri(&format!("{LOGIC_NAMESPACE}{property}"))
        });
        add_size(&mut self.work, 1, MAX_SOURCE_WORK, "source admission work")?;
        let mut values = Vec::new();
        if let Some(predicate) = predicate {
            for quad in crate::graphutil::default_graph_pattern(
                self.dataset,
                Some(node),
                Some(predicate),
                None,
            ) {
                add_size(&mut self.work, 1, MAX_SOURCE_WORK, "source admission work")?;
                values.push(quad.o);
            }
        }
        values.sort_by_cached_key(|id| self.dataset.term_value(*id));
        values.dedup();
        Ok(values)
    }
    pub(super) fn one(
        &mut self,
        node: TermId,
        property: &'static str,
    ) -> gmeow_errors::Result<TermId> {
        let values = self.values(node, property)?;
        match values.as_slice() {
            [value] => Ok(*value),
            _ => Err(self.error(node, &format!("requires exactly one logic:{property}"))),
        }
    }
    pub(super) fn optional_name(
        &mut self,
        node: TermId,
        property: &'static str,
    ) -> gmeow_errors::Result<Option<String>> {
        match self.values(node, property)?.as_slice() {
            [] => Ok(None),
            [value] => self.named(*value).map(Some),
            _ => Err(self.error(node, &format!("requires at most one logic:{property}"))),
        }
    }
    pub(super) fn name_value(
        &mut self,
        node: TermId,
        property: &'static str,
    ) -> gmeow_errors::Result<String> {
        let value = self.one(node, property)?;
        self.named(value)
    }
    pub(super) fn typed(&mut self, node: TermId, class: &str) -> gmeow_errors::Result<()> {
        self.named(node)?;
        if class != "PresentationEvidence" {
            self.admit_scope(node)?;
        }
        let class = format!("{LOGIC_NAMESPACE}{class}");
        let mut typed = false;
        for predicate in [
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type".to_owned(),
            format!("{LOGIC_NAMESPACE}instanceOf"),
        ] {
            add_size(&mut self.work, 1, MAX_SOURCE_WORK, "source typing work")?;
            if let Some(predicate) = self.dataset.term_id_by_iri(&predicate) {
                for quad in crate::graphutil::default_graph_pattern(
                    self.dataset,
                    Some(node),
                    Some(predicate),
                    None,
                ) {
                    add_size(&mut self.work, 1, MAX_SOURCE_WORK, "source typing work")?;
                    typed |=
                        matches!(self.dataset.resolve(quad.o), TermRef::Iri(iri) if iri == class);
                }
            }
        }
        if !typed {
            return Err(self.error(
                node,
                &format!("requires explicit {class} typing in the default source graph"),
            ));
        }
        Ok(())
    }
}
