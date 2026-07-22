// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic, run-once migration of authored `gmeow:TermEquivalence` alignment
//! cells (the `gmeow:alignSubject`/`alignPredicate`/`alignObject` form) into the native
//! RDF-1.2 statement-annotated `skos:*Match` form the correspondence reader consumes
//! (issue #1200 R4 / Task 8).
//!
//! ## What it does
//!
//! For one Turtle mapping file it:
//! 1. authoritatively extracts each `gmeow:TermEquivalence` cell's fields with `purrdf`
//!    (the *old* field logic bundled here — [`read_legacy`] — so no value is ever
//!    regex-guessed), capturing each cell's reifier-node IRI;
//! 2. locates that node's top-level statement span via `purrdf`'s source-span side table
//!    ([`purrdf::parse_dataset_with`] / [`purrdf::SpanTable`]) for the start and a
//!    Turtle-aware statement scanner ([`statement_end`]) for the terminating `.`;
//! 3. **targeted-splices** the native block in place with a FIXED annotation order and
//!    indentation ([`build_block`]) — every other byte (`@prefix`, `gmeow:MappingSet`,
//!    `gmeow:ProjectionMapping`, prose comments, section headers) is left untouched.
//!
//! ## Refuse-to-write self-check (the corpus safeguard)
//!
//! Before returning a rewritten file, [`self_check`] asserts BOTH:
//! * **Field round-trip** — the bundled *old* extractor over the source and the bundled
//!   *native* extractor ([`read_native`], a faithful copy of the Task-7 reader) over the
//!   rewrite agree on the natural key AND every field; the count is cross-checked against
//!   the REAL Task-7 reader ([`equivalence_cells`]).
//! * **SSSOM byte-identity** — [`lower_sssom`] (which itself runs the REAL
//!   `extract_native_equivalences`) over the old and new views produces byte-identical
//!   TSV for every set.
//!
//! Any mismatch is an `Err`; the caller must NOT write that file. A cell whose predicate
//! is not one of the five `skos:*Match` names (an `owl:equivalentClass` / `rdfs:subClassOf`
//! legacy cell) is not consumable by the native reader, so its file self-check fails and
//! the migration aborts — the safeguard, not a silent drop.

use std::collections::{BTreeMap, BTreeSet};

use purrdf::ParseOptions;

use crate::ingest::DslView;
use crate::projections::correspondence_frontend::transpile_correspondences_indexed;
use crate::projections::sssom::{equivalence_cells, lower_sssom};

// ── Bundled predicate constants (a copy of the reader's, per Task 8) ──────────────

const GM_TERM_EQUIVALENCE: &str = "https://blackcatinformatics.ca/gmeow/TermEquivalence";
const GM_ALIGN_SUBJECT: &str = "https://blackcatinformatics.ca/gmeow/alignSubject";
const GM_ALIGN_PREDICATE: &str = "https://blackcatinformatics.ca/gmeow/alignPredicate";
const GM_ALIGN_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/alignObject";
const GM_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const GM_JUSTIFICATION: &str = "https://blackcatinformatics.ca/gmeow/justification";
const GM_COMMENT: &str = "https://blackcatinformatics.ca/gmeow/comment";
const GM_LOSSY_DROP: &str = "https://blackcatinformatics.ca/gmeow/lossyDrop";
const GM_SSSOM_FILE: &str = "https://blackcatinformatics.ca/gmeow/sssomFile";
const GM_SUBJECT_LABEL: &str = "https://blackcatinformatics.ca/gmeow/subjectLabel";
const GM_OBJECT_LABEL: &str = "https://blackcatinformatics.ca/gmeow/objectLabel";
const LOGIC_GROUNDING_CORRESPONDENCE: &str =
    "https://blackcatinformatics.ca/logic/GroundingCorrespondence";
const LOGIC_MORPHISM_CLASS: &str = "https://blackcatinformatics.ca/logic/morphismClass";
const LOGIC_MORPHISM_KIND: &str = "https://blackcatinformatics.ca/logic/morphismKind";
const LOGIC_PRESERVATION_KIND: &str = "https://blackcatinformatics.ca/logic/preservationKind";
const LOGIC_SOURCE_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/sourceEndpoint";
const LOGIC_TARGET_ENDPOINT: &str = "https://blackcatinformatics.ca/logic/targetEndpoint";

// ── Public surface ────────────────────────────────────────────────────────────────

/// The result of migrating one Turtle mapping file.
#[derive(Debug, Clone)]
pub struct FileMigration {
    /// The rewritten source (identical to the input outside the spliced cell spans).
    pub rewritten: String,
    /// Number of `gmeow:TermEquivalence` cells rewritten (0 → no change).
    pub cells_migrated: usize,
}

/// Migrate every `gmeow:TermEquivalence` cell in one Turtle `source` to the native
/// RDF-1.2 `skos:*Match` form, enforcing the per-file refuse-to-write self-check.
///
/// A file with no legacy cells is returned unchanged (`cells_migrated == 0`).
///
/// # Errors
///
/// Returns an error (and NEVER a partially-written result) if the source cannot be
/// parsed, a cell's source span cannot be located, or EITHER self-check gate fails.
pub fn migrate_turtle_source(source: &str) -> gmeow_errors::Result<FileMigration> {
    let options = ParseOptions {
        track_source_spans: true,
    };
    let (old_ds, span) =
        purrdf::parse_dataset_with(source.as_bytes(), "text/turtle", None, &options)
            .map_err(|e| err(format!("parse source: {e}")))?;
    let span = span.ok_or_else(|| err("source-span table was not produced".to_owned()))?;
    let old_view = DslView::new(&old_ds);

    let legacy = read_legacy(&old_view);
    if legacy.is_empty() {
        return Ok(FileMigration {
            rewritten: source.to_owned(),
            cells_migrated: 0,
        });
    }

    let prefixes = parse_prefixes(source);

    // Locate each cell's span and build its replacement block.
    let mut edits: Vec<(usize, usize, String)> = Vec::with_capacity(legacy.len());
    for cell in &legacy {
        let position = span
            .position_for_subject(&cell.node_iri)
            .ok_or_else(|| err(format!("no source span for cell <{}>", cell.node_iri)))?;
        let start = position.byte_offset;
        let end = statement_end(source, start).ok_or_else(|| {
            err(format!(
                "cannot find the terminating '.' for cell <{}> at byte {start}",
                cell.node_iri
            ))
        })?;
        edits.push((start, end, build_block(&cell.facts, &prefixes)));
    }

    // Splice back-to-front so earlier offsets stay valid; reject any overlap.
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    for pair in edits.windows(2) {
        // Descending order: pair[0].start >= pair[1].start; pair[1] ends before pair[0].
        if pair[1].1 > pair[0].0 {
            return Err(err(
                "overlapping cell statement spans detected; refusing to splice".to_owned(),
            ));
        }
    }
    let mut rewritten = source.to_owned();
    for (start, end, block) in &edits {
        rewritten.replace_range(*start..*end, block);
    }

    self_check(&old_view, &rewritten, &legacy)?;

    Ok(FileMigration {
        rewritten,
        cells_migrated: legacy.len(),
    })
}

// ── Cell model ─────────────────────────────────────────────────────────────────────

/// Every field an alignment cell carries, in fully-resolved value space — the unit both
/// the old and native extractors produce and the round-trip gate compares. `confidence`
/// is kept as its ORIGINAL lexical form so the rewrite re-emits it verbatim.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CellFacts {
    subject: String,
    predicate: String,
    obj: String,
    sssom_file: String,
    confidence: Option<String>,
    justification: Option<String>,
    subject_label: String,
    object_label: String,
    comment: String,
    /// Sorted, so old (dataset order) and native (annotation-sorted) compare equal.
    lossy_drops: Vec<String>,
    grounding: bool,
    morphism_class: Option<String>,
    morphism_kind: Option<String>,
    preservation: Option<String>,
    source_endpoint: Option<String>,
    target_endpoint: Option<String>,
}

/// A legacy cell plus its reifier-node IRI (the `gmeow:eqXxx` subject) — the node IRI is
/// the key into the source-span table.
struct LegacyCell {
    node_iri: String,
    facts: CellFacts,
}

// ── Extraction: bundled OLD (align*) reader ────────────────────────────────────────

/// Read every `gmeow:TermEquivalence` cell in the *old* align\* form — a bundled copy of
/// the historical reader that ALSO captures the cell's node IRI for span location. A
/// malformed cell (missing subject/predicate/object/file) is skipped exactly as the
/// historical reader skipped it.
fn read_legacy(view: &DslView) -> Vec<LegacyCell> {
    let grounding: BTreeSet<String> = view
        .subjects_of_type(LOGIC_GROUNDING_CORRESPONDENCE)
        .into_iter()
        .collect();
    let mut out = Vec::new();
    for node in view.subjects_of_type(GM_TERM_EQUIVALENCE) {
        let (Some(subject), Some(predicate), Some(obj), Some(sssom_file)) = (
            view.object_iri(&node, GM_ALIGN_SUBJECT),
            view.object_iri(&node, GM_ALIGN_PREDICATE),
            view.object_iri(&node, GM_ALIGN_OBJECT),
            view.object_literal(&node, GM_SSSOM_FILE),
        ) else {
            continue;
        };
        let mut lossy_drops = view.object_literals(&node, GM_LOSSY_DROP);
        lossy_drops.sort();
        let facts = CellFacts {
            subject,
            predicate,
            obj,
            sssom_file,
            confidence: view.object_literal(&node, GM_CONFIDENCE),
            justification: view.object_iri(&node, GM_JUSTIFICATION),
            subject_label: view
                .object_literal(&node, GM_SUBJECT_LABEL)
                .unwrap_or_default(),
            object_label: view
                .object_literal(&node, GM_OBJECT_LABEL)
                .unwrap_or_default(),
            comment: view.object_literal(&node, GM_COMMENT).unwrap_or_default(),
            lossy_drops,
            grounding: grounding.contains(&node),
            morphism_class: view.object_iri(&node, LOGIC_MORPHISM_CLASS),
            morphism_kind: view.object_iri(&node, LOGIC_MORPHISM_KIND),
            preservation: view.object_iri(&node, LOGIC_PRESERVATION_KIND),
            source_endpoint: view.object_iri(&node, LOGIC_SOURCE_ENDPOINT),
            target_endpoint: view.object_iri(&node, LOGIC_TARGET_ENDPOINT),
        };
        out.push(LegacyCell {
            node_iri: node,
            facts,
        });
    }
    out
}

// ── Extraction: bundled NATIVE reader (faithful copy of the Task-7 reader) ──────────

/// The five `skos:*Match` predicate local-names a native alignment cell may carry.
fn is_skos_match_predicate(predicate: &str) -> bool {
    let local = predicate
        .rsplit(['#', '/', ':'])
        .next()
        .unwrap_or(predicate);
    matches!(
        local,
        "exactMatch" | "closeMatch" | "broadMatch" | "narrowMatch" | "relatedMatch"
    )
}

/// Read every native-form alignment cell — a faithful copy of the Task-7 reader
/// (`extract_native_equivalences`), producing [`CellFacts`] so the round-trip gate can
/// compare EVERY field (including the non-SSSOM `logic:` fields the SSSOM gate does not
/// carry). The reader's SSSOM-visible subset is independently cross-checked against the
/// REAL reader via the [`lower_sssom`] byte-identity gate in [`self_check`].
fn read_native(view: &DslView) -> Vec<CellFacts> {
    let mut out = Vec::new();
    for stmt in view.reified_statements() {
        if !is_skos_match_predicate(&stmt.predicate) {
            continue;
        }
        let Some(sssom_file) = view.annotation_literal(&stmt.reifier, GM_SSSOM_FILE) else {
            continue;
        };
        let Some(obj) = stmt.object.as_iri().map(str::to_owned) else {
            continue;
        };
        out.push(CellFacts {
            subject: stmt.subject.clone(),
            predicate: stmt.predicate.clone(),
            obj,
            sssom_file,
            confidence: view.annotation_literal(&stmt.reifier, GM_CONFIDENCE),
            justification: view.annotation_iri(&stmt.reifier, GM_JUSTIFICATION),
            subject_label: view
                .annotation_literal(&stmt.reifier, GM_SUBJECT_LABEL)
                .unwrap_or_default(),
            object_label: view
                .annotation_literal(&stmt.reifier, GM_OBJECT_LABEL)
                .unwrap_or_default(),
            comment: view
                .annotation_literal(&stmt.reifier, GM_COMMENT)
                .unwrap_or_default(),
            // `annotation_literals` already returns a sorted vec.
            lossy_drops: view.annotation_literals(&stmt.reifier, GM_LOSSY_DROP),
            grounding: view.annotation_has_type(&stmt.reifier, LOGIC_GROUNDING_CORRESPONDENCE),
            morphism_class: view.annotation_iri(&stmt.reifier, LOGIC_MORPHISM_CLASS),
            morphism_kind: view.annotation_iri(&stmt.reifier, LOGIC_MORPHISM_KIND),
            preservation: view.annotation_iri(&stmt.reifier, LOGIC_PRESERVATION_KIND),
            source_endpoint: view.annotation_iri(&stmt.reifier, LOGIC_SOURCE_ENDPOINT),
            target_endpoint: view.annotation_iri(&stmt.reifier, LOGIC_TARGET_ENDPOINT),
        });
    }
    out
}

// ── Native block emission ───────────────────────────────────────────────────────────

/// Emit the native RDF-1.2 block for one cell with the FIXED annotation order (grounding
/// type first, then `gmeow:sssomFile`, `justification`, `confidence`, `subjectLabel`,
/// `objectLabel`, `comment`, sorted `lossyDrop`, then the `logic:` endpoint/class/kind/
/// preservation fields) and FIXED four-space indentation.
fn build_block(cell: &CellFacts, prefixes: &[(String, String)]) -> String {
    let mut lines: Vec<String> = Vec::new();
    if cell.grounding {
        lines.push("a logic:GroundingCorrespondence".to_owned());
    }
    lines.push(format!("gmeow:sssomFile {}", ttl_string(&cell.sssom_file)));
    if let Some(j) = &cell.justification {
        lines.push(format!("gmeow:justification {}", curie(j, prefixes)));
    }
    if let Some(c) = &cell.confidence {
        lines.push(format!("gmeow:confidence {c}"));
    }
    if !cell.subject_label.is_empty() {
        lines.push(format!(
            "gmeow:subjectLabel {}",
            ttl_string(&cell.subject_label)
        ));
    }
    if !cell.object_label.is_empty() {
        lines.push(format!(
            "gmeow:objectLabel {}",
            ttl_string(&cell.object_label)
        ));
    }
    if !cell.comment.is_empty() {
        lines.push(format!("gmeow:comment {}", ttl_string(&cell.comment)));
    }
    for drop in &cell.lossy_drops {
        lines.push(format!("gmeow:lossyDrop {}", ttl_string(drop)));
    }
    if let Some(x) = &cell.source_endpoint {
        lines.push(format!("logic:sourceEndpoint {}", curie(x, prefixes)));
    }
    if let Some(x) = &cell.target_endpoint {
        lines.push(format!("logic:targetEndpoint {}", curie(x, prefixes)));
    }
    if let Some(x) = &cell.morphism_class {
        lines.push(format!("logic:morphismClass {}", curie(x, prefixes)));
    }
    if let Some(x) = &cell.morphism_kind {
        lines.push(format!("logic:morphismKind {}", curie(x, prefixes)));
    }
    if let Some(x) = &cell.preservation {
        lines.push(format!("logic:preservationKind {}", curie(x, prefixes)));
    }

    let subject = curie(&cell.subject, prefixes);
    let predicate = curie(&cell.predicate, prefixes);
    let object = curie(&cell.obj, prefixes);
    let body = lines.join(" ;\n    ");
    format!("{subject} {predicate} {object} {{|\n    {body}\n|}} .")
}

/// Shorten a full IRI to a `prefix:local` CURIE using the FILE's own prefix map (longest
/// namespace match), or fall back to `<full-iri>` when no declared prefix applies.
fn curie(iri: &str, prefixes: &[(String, String)]) -> String {
    for (name, ns) in prefixes {
        if let Some(local) = iri.strip_prefix(ns.as_str())
            && is_valid_pn_local(local)
        {
            return format!("{name}:{local}");
        }
    }
    format!("<{iri}>")
}

/// A conservative check that `local` is a Turtle prefixed-name local part needing no
/// escaping: non-empty, `[A-Za-z0-9_-]` only, not leading with `-`. Anything else falls
/// back to the always-safe `<full-iri>` form (the self-check would catch a bad guess).
fn is_valid_pn_local(local: &str) -> bool {
    let mut chars = local.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return false;
    }
    local
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Serialize a string as a Turtle double-quoted literal with the minimal escapes.
fn ttl_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── Prefix map ──────────────────────────────────────────────────────────────────────

/// Parse `@prefix NAME: <NS> .` declarations from the source, returned sorted by
/// namespace length descending so [`curie`] does a longest-namespace match.
fn parse_prefixes(source: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("@prefix") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(colon) = rest.find(':') else {
            continue;
        };
        let name = rest[..colon].trim().to_owned();
        let after = &rest[colon + 1..];
        let (Some(lt), Some(gt)) = (after.find('<'), after.find('>')) else {
            continue;
        };
        if lt < gt {
            out.push((name, after[lt + 1..gt].to_owned()));
        }
    }
    out.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    out
}

// ── Statement-span scanner ──────────────────────────────────────────────────────────

/// Given the byte offset of a top-level statement's subject, return the byte offset just
/// past its terminating `.`. A Turtle-aware scan that skips string literals (`"…"`,
/// `'…'`, and their triple-quoted forms), IRI refs (`<…>`), and line comments, and only
/// treats a `.` at bracket depth 0 followed by whitespace/EOF as the terminator (a
/// decimal point is always followed by a digit, so it is never mistaken for one).
fn statement_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = start;
    let mut depth: i32 = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' if bytes[i..].starts_with(b"\"\"\"") => i = skip_long_string(bytes, i, b"\"\"\"")?,
            b'\'' if bytes[i..].starts_with(b"'''") => i = skip_long_string(bytes, i, b"'''")?,
            b'"' => i = skip_short_string(bytes, i, b'"')?,
            b'\'' => i = skip_short_string(bytes, i, b'\'')?,
            b'<' => i = skip_iri(bytes, i)?,
            b'[' | b'(' => {
                depth += 1;
                i += 1;
            }
            b']' | b')' => {
                depth -= 1;
                i += 1;
            }
            b'.' if depth == 0 => {
                let is_terminator = match bytes.get(i + 1) {
                    None => true,
                    Some(&n) => matches!(n, b' ' | b'\t' | b'\n' | b'\r'),
                };
                if is_terminator {
                    return Some(i + 1);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// Advance past a short (`"…"` / `'…'`) string literal, honoring `\` escapes.
fn skip_short_string(bytes: &[u8], start: usize, quote: u8) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return Some(i + 1),
            b'\n' => return None, // an unterminated short string is malformed
            _ => i += 1,
        }
    }
    None
}

/// Advance past a long (triple-quoted) string literal, honoring `\` escapes.
fn skip_long_string(bytes: &[u8], start: usize, delim: &[u8]) -> Option<usize> {
    let mut i = start + delim.len();
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i..].starts_with(delim) {
            return Some(i + delim.len());
        }
        i += 1;
    }
    None
}

/// Advance past an `<…>` IRI ref.
fn skip_iri(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'>' => return Some(i + 1),
            b'\n' => return None,
            _ => i += 1,
        }
    }
    None
}

// ── Refuse-to-write self-check ──────────────────────────────────────────────────────

fn self_check(
    old_view: &DslView,
    rewritten: &str,
    legacy: &[LegacyCell],
) -> gmeow_errors::Result<()> {
    let new_ds = purrdf::parse_dataset(rewritten.as_bytes(), "text/turtle", None)
        .map_err(|e| err(format!("re-parse of the migrated source failed: {e}")))?;
    let new_view = DslView::new(&new_ds);

    // Gate 1: every field round-trips old (align*) → new (native).
    let old_set: BTreeSet<CellFacts> = legacy.iter().map(|c| c.facts.clone()).collect();
    let new_native = read_native(&new_view);
    let new_set: BTreeSet<CellFacts> = new_native.iter().cloned().collect();
    if old_set != new_set {
        return Err(err(format!(
            "field round-trip mismatch: {} distinct old cells vs {} distinct native cells; \
             only in old: {:?}; only in new: {:?}",
            old_set.len(),
            new_set.len(),
            old_set.difference(&new_set).collect::<Vec<_>>(),
            new_set.difference(&old_set).collect::<Vec<_>>(),
        )));
    }
    if legacy.len() != new_native.len() {
        return Err(err(format!(
            "cell count changed: {} legacy cells vs {} native cells",
            legacy.len(),
            new_native.len()
        )));
    }
    // Cross-check the count against the REAL Task-7 reader over the rewrite.
    let real = equivalence_cells(&new_view);
    if real.len() != new_native.len() {
        return Err(err(format!(
            "real Task-7 reader found {} cells but the bundled native reader found {}",
            real.len(),
            new_native.len()
        )));
    }

    // Gate 2: SSSOM byte-identity via the REAL reader + serializer.
    let old_sets = lower_all(old_view)?;
    let new_sets = lower_all(&new_view)?;
    if old_sets != new_sets {
        let mut differing: Vec<&String> = Vec::new();
        for file in old_sets.keys().chain(new_sets.keys()) {
            if old_sets.get(file) != new_sets.get(file) && !differing.contains(&file) {
                differing.push(file);
            }
        }
        return Err(err(format!(
            "SSSOM output is not byte-identical old-vs-new; differing set(s): {differing:?}"
        )));
    }
    Ok(())
}

/// Lower every `gmeow:TermEquivalence`/`gmeow:ProjectionMapping` in `view` to SSSOM TSV
/// via the SAME derivation the pipeline uses (transpile the materialized correspondence
/// set, then `lower_sssom`). Version/date are fixed placeholders — they cancel out in the
/// old-vs-new comparison.
fn lower_all(view: &DslView) -> gmeow_errors::Result<BTreeMap<String, String>> {
    let empty = purrdf::parse_dataset(b"", "application/n-triples", None)
        .map_err(|e| err(format!("build empty overlay view: {e}")))?;
    let empty_view = DslView::new(&empty);
    let (_program, lookup) = transpile_correspondences_indexed(view, &empty_view)?;
    let lowering = lower_sssom(view, "0.0.0", "0000-00-00", &lookup)?;
    Ok(lowering.sets)
}

fn err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Sssom {
        detail: format!("migrate-skos-match: {detail}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statement_end_stops_at_top_level_dot_not_a_decimal() {
        let src = "gmeow:x gmeow:confidence 0.95 ; gmeow:sssomFile \"a.b.tsv\" .\nNEXT";
        let end = statement_end(src, 0).expect("terminator found");
        assert_eq!(
            &src[..end],
            "gmeow:x gmeow:confidence 0.95 ; gmeow:sssomFile \"a.b.tsv\" ."
        );
    }

    #[test]
    fn statement_end_skips_period_inside_a_string() {
        let src = "gmeow:x gmeow:comment \"ends. mid.\" ; gmeow:confidence 1.0 .\n";
        let end = statement_end(src, 0).expect("terminator found");
        assert_eq!(&src[end..], "\n");
    }

    #[test]
    fn curie_uses_longest_prefix_and_falls_back_to_full_iri() {
        let prefixes = parse_prefixes(
            "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
             @prefix ex: <http://example.org/> .\n",
        );
        assert_eq!(
            curie("http://www.w3.org/2004/02/skos/core#closeMatch", &prefixes),
            "skos:closeMatch"
        );
        assert_eq!(
            curie("http://elsewhere.example/Thing", &prefixes),
            "<http://elsewhere.example/Thing>"
        );
    }
}
