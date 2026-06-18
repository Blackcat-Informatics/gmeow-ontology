// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use crate::RdfLocation;

/// Structured non-triple material that travels with an RDF store.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RdfLookaside {
    pub resources: Vec<RdfLookasideResource>,
    pub metadata: Vec<RdfMetadataEntry>,
    pub segments: Vec<RdfSegmentRecord>,
    pub blobs: Vec<RdfBlobRecord>,
    pub suppressions: Vec<RdfSuppressionRecord>,
    pub opaque_nodes: Vec<RdfOpaqueNodeRecord>,
    pub signatures: Vec<RdfSignatureRecord>,
}

impl RdfLookaside {
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
            && self.metadata.is_empty()
            && self.segments.is_empty()
            && self.blobs.is_empty()
            && self.suppressions.is_empty()
            && self.opaque_nodes.is_empty()
            && self.signatures.is_empty()
    }

    pub fn resources_of_kind(
        &self,
        kind: RdfLookasideKind,
    ) -> impl Iterator<Item = &RdfLookasideResource> {
        self.resources
            .iter()
            .filter(move |resource| resource.kind == kind)
    }
}

/// Known companion/index kinds. Unknown domains remain representable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum RdfLookasideKind {
    Shacl,
    Shex,
    Docs,
    Logic,
    Schema,
    Query,
    Mapping,
    Projection,
    Ontology,
    #[default]
    Metadata,
    Blob,
    Other(String),
}

impl RdfLookasideKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Shacl => "shacl",
            Self::Shex => "shex",
            Self::Docs => "docs",
            Self::Logic => "logic",
            Self::Schema => "schema",
            Self::Query => "query",
            Self::Mapping => "mapping",
            Self::Projection => "projection",
            Self::Ontology => "ontology",
            Self::Metadata => "metadata",
            Self::Blob => "blob",
            Self::Other(value) => value.as_str(),
        }
    }

    pub fn from_hint(value: &str) -> Self {
        let lower = value.to_ascii_lowercase();
        match lower.as_str() {
            "shacl" | "shape" | "shapes" => Self::Shacl,
            "shex" => Self::Shex,
            "doc" | "docs" | "documentation" | "ontology-docs" => Self::Docs,
            "logic" | "rule" | "rules" => Self::Logic,
            "schema" | "schemas" | "json-schema" => Self::Schema,
            "query" | "queries" | "sparql" => Self::Query,
            "mapping" | "mappings" => Self::Mapping,
            "projection" | "projections" => Self::Projection,
            "ontology" | "owl" => Self::Ontology,
            "metadata" | "meta" => Self::Metadata,
            "blob" | "blobs" => Self::Blob,
            _ => Self::Other(value.to_owned()),
        }
    }
}

/// A typed sidecar resource such as SHACL, ShEx, docs, logic, schemas, or queries.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RdfLookasideResource {
    pub kind: RdfLookasideKind,
    pub iri: Option<String>,
    pub name: Option<String>,
    pub graph_name: Option<String>,
    pub media_type: Option<String>,
    pub content_digest: Option<String>,
    pub path: Option<String>,
    pub location: Option<RdfLocation>,
    pub metadata: BTreeMap<String, RdfMetadataValue>,
}

impl RdfLookasideResource {
    pub fn new(kind: RdfLookasideKind) -> Self {
        Self {
            kind,
            ..Self::default()
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    pub fn with_digest(mut self, digest: impl Into<String>) -> Self {
        self.content_digest = Some(digest.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RdfMetadataEntry {
    pub scope: String,
    pub key: String,
    pub value: RdfMetadataValue,
    pub location: Option<RdfLocation>,
}

impl RdfMetadataEntry {
    pub fn new(scope: impl Into<String>, key: impl Into<String>, value: RdfMetadataValue) -> Self {
        Self {
            scope: scope.into(),
            key: key.into(),
            value,
            location: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum RdfMetadataValue {
    Null,
    Bool(bool),
    Integer(i128),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Array(Vec<RdfMetadataValue>),
    Map(BTreeMap<String, RdfMetadataValue>),
    Tagged {
        tag: u64,
        value: Box<RdfMetadataValue>,
    },
    Opaque(String),
}

impl RdfMetadataValue {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdfSegmentRecord {
    pub index: usize,
    pub head: Option<String>,
    pub profile: Option<String>,
    pub claimed_streamable: bool,
    pub covered: usize,
    pub tail: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RdfBlobRecord {
    pub digest: String,
    pub media_type: Option<String>,
    pub representation: Option<String>,
    pub decoded_len: Option<usize>,
    pub metadata: BTreeMap<String, RdfMetadataValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RdfSuppressionRecord {
    pub reason: Option<String>,
    pub by: Option<String>,
    pub targets: Vec<RdfMetadataValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RdfOpaqueNodeRecord {
    pub id: String,
    pub frame_type: String,
    pub reason: String,
    pub signature_status: String,
    pub public_metadata: Option<RdfMetadataValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdfSignatureRecord {
    pub frame_id: String,
    pub key_id: Option<String>,
    pub status: String,
    pub has_cose: bool,
}
