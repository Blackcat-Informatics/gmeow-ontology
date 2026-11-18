// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact native syntax coordinates shared with the compiler ownership recorder.

use std::collections::{BTreeMap, BTreeSet};

use crate::frontend::{SourceNode, SourceStatementBinding};

/// Exact coordinates only; there is no second dataset or formula reading.
#[derive(Debug, Default)]
pub(crate) struct SyntaxOwnership {
    pub(crate) children: BTreeMap<SyntaxOwner, BTreeMap<SyntaxOwner, BTreeSet<SyntaxEvidence>>>,
    pub(crate) rows:
        BTreeMap<SourceStatementBinding, BTreeMap<SyntaxOwner, BTreeSet<SyntaxEvidence>>>,
    pub(crate) issues: BTreeMap<SyntaxOwner, Vec<SyntaxIssue>>,
}

/// Original graph-edge indices avoid copying their stored carrier coordinates.
/// Collection fields have no structural edge entry and retain their exact row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SyntaxEvidence {
    Edge(usize),
    Statement(SourceStatementBinding),
}

#[derive(Debug)]
pub(crate) enum SyntaxIssue {
    MalformedCollection {
        source: SourceStatementBinding,
        detail: String,
    },
}

/// Owning a resource's formula does not own unrelated class or property laws
/// about that same resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SyntaxOwner {
    pub(crate) node: SourceNode,
    pub(crate) family: SyntaxFamily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SyntaxFamily {
    Formula,
    Rule,
    Atom,
    Term,
    Class,
    Owner,
    Correspondence,
    ClassList,
    ValueList,
    PropertyList,
    FacetList,
    Schema,
    Module,
}
