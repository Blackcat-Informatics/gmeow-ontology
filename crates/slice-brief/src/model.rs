// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The serde-serializable value model of a `gmeow:AuthoringPacket` and its parts.
//!
//! Every collection is stored in a deterministic order (ascending by IRI /
//! `BTreeSet`-ordered), so a packet built twice from identical inputs is identical
//! field-for-field — the precondition for byte-stable turtle and a stable digest.

use serde::{Deserialize, Serialize};

use crate::ns;

/// The grounding axis a [`GroundingCell`] measures — the columns of the packet's
/// coverage cross-table (`gmeow:GroundingAttribute`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GroundingAttribute {
    /// The native English annotation coat (`gmeow:groundingEn`).
    En,
    /// A French translation JOINed from `fr.po` (`gmeow:groundingFr`).
    Fr,
    /// A Chinese translation JOINed from `zh.po` (`gmeow:groundingZh`).
    Zh,
    /// An alignment to an external ontology entity (`gmeow:groundingExternalMapped`).
    ExternalMapped,
    /// A same-slice exemplar coat (`gmeow:groundingExemplar`).
    Exemplar,
}

impl GroundingAttribute {
    /// The full attribute-individual IRI this column serializes to.
    #[must_use]
    pub fn iri(self) -> String {
        let local = match self {
            GroundingAttribute::En => "groundingEn",
            GroundingAttribute::Fr => "groundingFr",
            GroundingAttribute::Zh => "groundingZh",
            GroundingAttribute::ExternalMapped => "groundingExternalMapped",
            GroundingAttribute::Exemplar => "groundingExemplar",
        };
        format!("{}{local}", ns::GMEOW)
    }

    /// The stable path segment used when minting a cell IRI for this column.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            GroundingAttribute::En => "en",
            GroundingAttribute::Fr => "fr",
            GroundingAttribute::Zh => "zh",
            GroundingAttribute::ExternalMapped => "external",
            GroundingAttribute::Exemplar => "exemplar",
        }
    }

    /// The BCP-47 language code a language column joins on (`None` for the
    /// non-language columns).
    #[must_use]
    pub fn lang(self) -> Option<&'static str> {
        match self {
            GroundingAttribute::Fr => Some("fr"),
            GroundingAttribute::Zh => Some("zh"),
            _ => None,
        }
    }
}

/// A resolved RDF object term carried in an axiom / neighbour triple.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ObjTerm {
    /// An IRI object, by its full string.
    Iri(String),
    /// A blank node, by its (parse-stable) label.
    Blank(String),
    /// A literal: lexical form, datatype IRI, and optional language tag.
    Literal {
        /// The lexical form, byte-for-byte as authored.
        lexical: String,
        /// The datatype IRI.
        datatype: String,
        /// The language tag, for language-tagged strings.
        language: Option<String>,
    },
}

/// One `(predicate, object)` edge of a term's description (the subject is implied).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Triple {
    /// The predicate IRI.
    pub predicate: String,
    /// The object term.
    pub object: ObjTerm,
}

/// One literal annotation of a term's coat (`rdfs:label`, `skos:definition`, …).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Annotation {
    /// The annotation predicate IRI.
    pub predicate: String,
    /// The language tag of the value, if any.
    pub language: Option<String>,
    /// The literal value.
    pub value: String,
}

/// One definitional-dependency-closure entry: a class/property referenced in a
/// covered term's axioms, with its own label + definition (when authored).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ClosureEntry {
    /// The referenced term IRI.
    pub iri: String,
    /// The referent's `rdfs:label`, if authored.
    pub label: Option<String>,
    /// The referent's `skos:definition`, if authored.
    pub definition: Option<String>,
}

/// The full authoring content assembled for one covered term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoveredTerm {
    /// The term IRI.
    pub iri: String,
    /// The term's English `rdfs:label`, if authored.
    pub label: Option<String>,
    /// The term's `skos:definition`, if authored.
    pub definition: Option<String>,
    /// The full literal annotation coat, `BTreeSet`-ordered.
    pub coat: Vec<Annotation>,
    /// The authored axioms (term-as-subject, non-literal objects), `BTreeSet`-ordered.
    pub axioms: Vec<Triple>,
    /// The depth-1 CBD neighbourhood (the blank-node closure), `BTreeSet`-ordered.
    pub neighbors: Vec<Triple>,
    /// The definitional-dependency closure, sorted by referenced IRI.
    pub closure: Vec<ClosureEntry>,
    /// The per-term content digest (hex sha256 over this term's canonical body).
    pub content_digest: String,
}

/// One incidence cell of the packet's grounding cross-table
/// (`gmeow:GroundingCoverage`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroundingCell {
    /// The stable, minted cell IRI (never a blank node — byte-stable turtle).
    pub cell_iri: String,
    /// The covered term this cell is about.
    pub term: String,
    /// The grounding axis (column) this cell measures.
    pub attribute: GroundingAttribute,
    /// Whether the grounding is present. `false` is the explicit absent record.
    pub present: bool,
    /// For a language cell: the annotation predicate, in CURIE form.
    pub predicate: Option<String>,
    /// For a present language cell: the JOINed translation value.
    pub value: Option<String>,
    /// For an external-mapped cell: the aligned external entity IRI.
    pub external_entity: Option<String>,
    /// For an external-mapped cell: the external entity's English label.
    pub external_label: Option<String>,
    /// For an external-mapped cell: the alignment predicate's local name.
    pub align_predicate: Option<String>,
    /// For an external-mapped cell: the mapping confidence in `[0.0, 1.0]`.
    pub confidence: Option<f64>,
    /// Whether this language cell disagrees with an equivalent term's translation.
    pub conflict: bool,
    /// The equivalent term whose translation disagrees, when `conflict` is true.
    pub conflict_with: Option<String>,
}

/// A self-contained authoring brief for one batch of a slice's terms — the
/// serde-serializable projection that renders to turtle / JSON / human text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthoringPacket {
    /// The minted, stable packet-individual IRI.
    pub packet_iri: String,
    /// The slice whose terms this packet briefs.
    pub source_slice: String,
    /// The subdomain axis the packet is partitioned along (absent = whole slice).
    pub axis: Option<String>,
    /// The zero-based batch index within the axis's sorted term set.
    pub batch: u32,
    /// The content address of the packet's semantic body (hex sha256).
    pub digest: String,
    /// The number of terms this packet covers.
    pub term_count: usize,
    /// The number of exemplar coats the packet fell short of its target by.
    pub exemplar_shortfall: usize,
    /// The covered terms, ascending by IRI.
    pub terms: Vec<CoveredTerm>,
    /// The same-slice exemplar term IRIs, ordered (tier desc, IRI asc).
    pub exemplars: Vec<String>,
    /// The grounding-coverage cells, in deterministic cell order.
    pub grounding: Vec<GroundingCell>,
}
