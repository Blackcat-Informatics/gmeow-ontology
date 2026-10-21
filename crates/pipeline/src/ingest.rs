// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The source-ingestion adapter seam + its minimal-allocation source-span index.
//!
//! The error/diagnostics layer is FORMAT-AGNOSTIC: a finding's source position is
//! NOT recovered by re-scanning source text or by a hand-rolled parser, it is
//! carried out of ingestion by a swappable [`SourceAdapter`]. An adapter parses one
//! source input and yields BOTH the frozen dataset AND a subject→source-position
//! span index; the pipeline never obtains spans any other way. `purrdf` is merely
//! *today's* adapter — [`PurrdfAdapter`] threads `purrdf::parse_dataset_with` with
//! [`purrdf::ParseOptions::track_source_spans`] set, which is zero-cost when off and
//! pins the (already sequential) Turtle pipeline when on. A different ingestion
//! backend would implement the same trait and the rest of the pipeline is unchanged.
//!
//! ## Minimal-allocation span index
//!
//! [`SpanIndex`] maps each authored subject (keyed by its **bare-IRI** lexical
//! string — the SHACL focus-node join key, blank nodes as `_:label`) to a
//! [`SourceSpan`]. The path is interned ONCE per source file: every subject a file
//! contributes shares a single `Arc<str>` path (a cheap ref-count bump, never a
//! fresh `String` per entry), and the line/column/byte-offset ride by value (`Copy`).
//! The native JSON wire form ([`SpanIndex::to_json_bytes`]) preserves that interning — it
//! serializes a per-file path table plus `(subject, path-id, line, column, offset)`
//! rows and re-interns one `Arc<str>` per path on the way back, so a round-trip
//! through the by-reference blob lane keeps the one-Arc-per-file shape.

use std::collections::BTreeMap;
use std::sync::Arc;

use purrdf::{ParseOptions, ParseOutcome, RdfDataset, SpanTable, parse_dataset_with};

/// One authored subject's source position: the interned source path plus the
/// 1-based line/column and byte offset where the subject was FIRST asserted.
///
/// `path` is an [`Arc<str>`] shared with every other subject of the same source
/// file (interned once per file — see the [module docs](self)); the numeric
/// coordinates are `Copy` and ride by value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    /// The repo-relative source path, interned once per file.
    pub path: Arc<str>,
    /// 1-based source line.
    pub line: u64,
    /// 1-based source column (Unicode scalar values from line start).
    pub column: u32,
    /// Byte offset into the source document.
    pub byte_offset: u64,
}

impl SourceSpan {
    /// Construct a span over an already-interned `path`.
    pub fn new(path: Arc<str>, line: u64, column: u32, byte_offset: u64) -> Self {
        Self {
            path,
            line,
            column,
            byte_offset,
        }
    }

    /// Project this span onto the diagnostics [`Location`](gmeow_errors::model::Location)
    /// a finding carries — path + 1-based line/column. The bare-IRI join key stays on
    /// the finding's `logical` field; this fills the physical coordinates.
    pub fn to_location(&self) -> gmeow_errors::model::Location {
        gmeow_errors::model::Location::new(
            Some(self.path.to_string()),
            Some(self.line),
            Some(self.column),
            None,
        )
    }
}

/// A minimal-allocation subject bare-IRI → [`SourceSpan`] index.
///
/// Keyed by the bare-IRI subject lexical string (a SHACL focus node joins directly);
/// blank-node subjects are keyed `_:label`. The path of every subject from one file
/// is a single shared [`Arc<str>`]. Serializable (it becomes a by-reference blob) with
/// the per-file interning preserved across the round-trip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpanIndex {
    by_subject: BTreeMap<String, SourceSpan>,
}

impl SpanIndex {
    /// An empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// The source span of `subject` (a bare-IRI / `_:label` lexical key), if tracked.
    pub fn lookup(&self, subject: &str) -> Option<&SourceSpan> {
        self.by_subject.get(subject)
    }

    /// The number of tracked subjects.
    pub fn len(&self) -> usize {
        self.by_subject.len()
    }

    /// Whether nothing is tracked.
    pub fn is_empty(&self) -> bool {
        self.by_subject.is_empty()
    }

    /// Every `(subject key, span)` in sorted subject order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &SourceSpan)> + '_ {
        self.by_subject.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Record one subject→span (first writer wins — a subject asserted in two files
    /// keeps the first source position, matching the parser's first-assertion rule).
    pub fn insert(&mut self, subject: impl Into<String>, span: SourceSpan) {
        self.by_subject.entry(subject.into()).or_insert(span);
    }

    /// Fold a single source file's [`SpanTable`] into this index, interning `path`
    /// ONCE for the whole file so every subject it contributes shares one `Arc<str>`
    /// (minimal allocation — no per-entry path `String`). First writer wins.
    pub fn extend_from_span_table(&mut self, path: &str, table: &SpanTable) {
        let interned: Arc<str> = Arc::from(path);
        for (subject, position) in table.iter() {
            // First writer wins: only allocate the owned subject key when this is the
            // first span seen for it — a subject repeated within/across files keeps the
            // first position, so the `to_owned()` on a duplicate would be discarded.
            if self.by_subject.contains_key(subject) {
                continue;
            }
            self.by_subject.insert(
                subject.to_owned(),
                SourceSpan {
                    path: Arc::clone(&interned),
                    line: position.line,
                    column: position.column,
                    byte_offset: position.byte_offset as u64,
                },
            );
        }
    }

    /// Merge another index into this one (first writer wins). Each `SourceSpan` keeps
    /// its own file's shared `Arc<str>`, so per-file interning survives the fold —
    /// `source_load` uses this to fold many files into one index.
    pub fn merge(&mut self, other: SpanIndex) {
        for (subject, span) in other.by_subject {
            self.by_subject.entry(subject).or_insert(span);
        }
    }

    /// Merge retained source positions without cloning a temporary index.
    /// Existing positions keep first-writer precedence and paths remain shared.
    pub fn extend_from_index(&mut self, other: &SpanIndex) {
        for (subject, span) in &other.by_subject {
            if !self.by_subject.contains_key(subject) {
                self.by_subject.insert(subject.clone(), span.clone());
            }
        }
    }
}

mod json;

/// The result of one ingestion: the frozen dataset and the source-span contribution
/// the adapter tracked for this input (empty when the adapter/format carries none).
pub struct Ingested {
    /// The parsed, frozen dataset — identical to what the plain parse would produce.
    pub dataset: Arc<RdfDataset>,
    /// This input's subject→source-position spans.
    pub spans: SpanContribution,
    /// Base IRI at the end of the source document, with its parser-recorded
    /// origin. Preserve this alongside the dataset for exact source receipts.
    pub document_base: Option<purrdf::iri::ScopedBase>,
}

/// One source input's span contribution — a [`SpanIndex`] over exactly that file's
/// subjects (path interned once). `source_load` folds many contributions into one
/// index via [`SpanIndex::merge`].
pub struct SpanContribution {
    index: SpanIndex,
}

impl SpanContribution {
    /// Borrow the underlying per-file span index.
    pub fn index(&self) -> &SpanIndex {
        &self.index
    }

    /// Take the underlying per-file span index by value.
    pub fn into_index(self) -> SpanIndex {
        self.index
    }
}

/// An ingestion adapter: the swappable seam through which the pipeline obtains a
/// parsed dataset AND its subject→source-position spans. `purrdf` is today's impl
/// ([`PurrdfAdapter`]); a different backend implements the same trait.
pub trait SourceAdapter {
    /// Parse one source input, returning the frozen dataset and (when the adapter
    /// tracks them) the subject→source-position spans. `logical_path` is the
    /// repo-relative path recorded as each subject's span path.
    fn ingest(
        &self,
        logical_path: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> gmeow_errors::Result<Ingested>;
}

/// The `purrdf` ingestion adapter — today's [`SourceAdapter`]. It threads
/// `purrdf::parse_dataset_with` with source-span tracking ON, so a Turtle-family
/// input yields a populated [`SpanTable`] keyed by bare-IRI subject; RDF/XML / TriX /
/// HexTuples carry no text spans and yield an empty table by design. Span tracking is
/// zero-cost when off and, when on, only pins the (already sequential) Turtle pipeline.
#[derive(Debug, Clone, Copy, Default)]
pub struct PurrdfAdapter;

impl SourceAdapter for PurrdfAdapter {
    fn ingest(
        &self,
        logical_path: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> gmeow_errors::Result<Ingested> {
        // Keep all native parse results together. Source receipts must not
        // recover a document's declared base through a separate text pass.
        let ParseOutcome {
            dataset,
            spans: table,
            document_base,
            // Prefixes are surface spelling, not source identity; receipts never read them.
            document_prefixes: _,
        } = parse_dataset_with(
            bytes,
            media_type,
            None,
            &ParseOptions {
                track_source_spans: true,
            },
        )
        .map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("ingest {logical_path}: {e}"),
            })
        })?;
        let mut index = SpanIndex::new();
        if let Some(table) = table {
            index.extend_from_span_table(logical_path, &table);
        }
        Ok(Ingested {
            dataset,
            spans: SpanContribution { index },
            document_base,
        })
    }
}

/// Lift source spans onto a diagnostics report's findings: for every finding whose
/// logical-only location (a SHACL focus node — a bare IRI) matches a span-index
/// entry, fill that location's physical path + 1-based line/column from the span.
///
/// Only logical-only locations are enriched (a location that already carries a
/// physical `path` is left as-is), and the bare-IRI `logical` join key is preserved,
/// so the enrichment is a pure augmentation — it never overwrites an existing path or
/// drops the subject identity. This is what makes the span table genuinely consumed:
/// the coordinates ride onto the shipped SHACL finding locations (and, via the forward
/// fold, into the run-ledger `DiagNode`s).
pub fn enrich_findings_with_spans(report: &mut gmeow_errors::Report, spans: &SpanIndex) {
    for finding in &mut report.findings {
        for location in &mut finding.locations {
            if location.path.is_some() {
                continue;
            }
            let Some(subject) = location.logical.as_deref() else {
                continue;
            };
            if let Some(span) = spans.lookup(subject) {
                location.path = Some(span.path.to_string());
                location.line = Some(span.line);
                location.column = Some(span.column);
            }
        }
    }
}

#[path = "ingest.tests.rs"]
#[cfg(test)]
mod tests;
