// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Invocation-local source membership. Index each contributed occurrence once,
//! retaining graph and storage-carrier identity before aggregate deduplication.

use std::collections::{BTreeMap, BTreeSet};

use crate::frontend::{SourceDocumentOccurrences, SourceNode, SourceStatementBinding};

pub(super) struct OriginIndex<'a> {
    paths: Vec<&'a str>,
    nodes: BTreeMap<SourceNode, Vec<usize>>,
    statements: BTreeMap<SourceStatementBinding, Vec<usize>>,
}

impl<'a> OriginIndex<'a> {
    pub(super) fn new(origins: &'a [SourceDocumentOccurrences]) -> Self {
        let mut index = Self {
            paths: Vec::with_capacity(origins.len()),
            nodes: BTreeMap::new(),
            statements: BTreeMap::new(),
        };
        for (document, origin) in origins.iter().enumerate() {
            index.paths.push(&origin.document.path);
            for binding in &origin.bindings {
                let documents = index.nodes.entry(binding.canonical).or_default();
                // Several source positions can name one canonical node. Document
                // indices arrive monotonically, so dedup needs no second set.
                if documents.last() != Some(&document) {
                    documents.push(document);
                }
            }
            for statement in &origin.statements {
                let documents = index.statements.entry(*statement).or_default();
                if documents.last() != Some(&document) {
                    documents.push(document);
                }
            }
        }
        index
    }

    pub(super) fn node_documents(&self, node: SourceNode) -> BTreeSet<String> {
        self.documents(self.nodes.get(&node))
    }

    pub(super) fn statement_documents(
        &self,
        statement: SourceStatementBinding,
    ) -> BTreeSet<String> {
        self.documents(self.statements.get(&statement))
    }

    fn documents(&self, indices: Option<&Vec<usize>>) -> BTreeSet<String> {
        indices
            .into_iter()
            .flatten()
            .map(|&index| self.paths[index].to_owned())
            .collect()
    }
}

#[cfg(test)]
#[path = "origins.tests.rs"]
mod tests;
