// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust-owned i18n authoring/compile helpers.
//!
//! This is the native authority for the developer i18n family: gettext catalog
//! extraction/export, PO linting, and English sync back into canonical sources.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gmeow_errors::{Diag, Result};
use gmeow_validate::distinctiveness::{distinctiveness_violations, skeleton};
use purrdf::slice::{ArtifactRole, SliceCatalog};
use regex::Regex;
use sha1::{Digest, Sha1};

use crate::error::{
    CatalogInconsistent, FileIo, PoParse, RdfFormat, RdfParse, TurtleUnescape, UnsupportedSource,
};
use crate::i18n::translation_integrity_issue;

const ENGLISH_TAG: &str = "x-gmeow-english";
use gmeow_ns::GMEOW_NS;
const RDFS_NS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SKOS_NS: &str = "http://www.w3.org/2004/02/skos/core#";
const DCTERMS_NS: &str = "http://purl.org/dc/terms/";

// The localizable-predicate set has a single authority in `gmeow-validate`; this
// re-export is an alias, not a second definition, so i18n consumers here and the
// Check-2 language-tag policy cannot drift.
pub use gmeow_validate::localizable::LOCALIZABLE_PREDICATES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoEntry {
    pub msgctxt: String,
    pub msgid: String,
    pub msgstr: String,
    pub fuzzy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationKey {
    pub slice_iri: String,
    pub term_iri: String,
    pub predicate: String,
    pub english_value: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub changed_files: Vec<PathBuf>,
    pub conflicts: Vec<String>,
    pub skipped: Vec<String>,
    pub unchanged: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct I18nLintReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub fuzzy_counts: BTreeMap<String, usize>,
    pub total_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExtractReport {
    pub groups: usize,
    pub total_keys: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MergeReport {
    pub po_files: usize,
    pub added: usize,
    pub output_note: String,
    pub turtle: String,
}

#[derive(Debug, Clone)]
struct RdfLiteralRow {
    subject: String,
    predicate: String,
    lexical: String,
    language: Option<String>,
}

/// Map a format token (name OR media type) to the native RDF media type the
/// gmeow-gts codecs accept. Mirrors the historical `parse_format` discrimination.
fn media_type_for_format(format: &str) -> Result<&'static str> {
    match format.to_ascii_lowercase().as_str() {
        "turtle" | "ttl" | "text/turtle" => Ok("text/turtle"),
        "ntriples" | "n-triples" | "nt" | "application/n-triples" => Ok("application/n-triples"),
        "nquads" | "n-quads" | "nq" | "application/n-quads" => Ok("application/n-quads"),
        "trig" | "application/trig" => Ok("application/trig"),
        other => Err(Diag::of_kind(RdfFormat {
            detail: format!("unsupported RDF format: {other}"),
        })),
    }
}

/// Parse RDF `bytes` of `format` natively and surface every `(named subject,
/// predicate, literal lexical, language)` row — the oxigraph-free twin of the old
/// `parse_rdf_literals`. Only literal objects on named subjects are surfaced (the
/// i18n family keys translations on those exactly).
fn parse_rdf_literals(bytes: &[u8], format: &str) -> Result<Vec<RdfLiteralRow>> {
    use purrdf::{DatasetView, GraphMatch, TermRef};

    let media_type = media_type_for_format(format)?;
    let dataset = purrdf::parse_dataset(bytes, media_type, None).map_err(|e| {
        Diag::of_kind(RdfParse {
            detail: format!("RDF parse error: {e}"),
        })
    })?;
    let mut rows = Vec::new();
    for quad in dataset.quads_for_pattern(None, None, None, GraphMatch::Any) {
        let TermRef::Iri(subject) = dataset.resolve(quad.s) else {
            continue;
        };
        let TermRef::Iri(predicate) = dataset.resolve(quad.p) else {
            continue;
        };
        let TermRef::Literal {
            lexical, language, ..
        } = dataset.resolve(quad.o)
        else {
            continue;
        };
        rows.push(RdfLiteralRow {
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            lexical: lexical.to_owned(),
            language: language.map(str::to_owned),
        });
    }
    Ok(rows)
}

pub fn expand_predicate(predicate: &str) -> String {
    let Some((prefix, local)) = predicate.split_once(':') else {
        return predicate.to_owned();
    };
    if local.starts_with("//") {
        return predicate.to_owned();
    }
    let ns = match prefix {
        "rdfs" => RDFS_NS,
        "skos" => SKOS_NS,
        "dcterms" | "dct" => DCTERMS_NS,
        "gmeow" => GMEOW_NS,
        _ => return predicate.to_owned(),
    };
    format!("{ns}{local}")
}

fn term_namespace(iri: &str) -> String {
    for delimiter in ['#', '/'] {
        if let Some(idx) = iri.rfind(delimiter) {
            return iri[..=idx].to_owned();
        }
    }
    iri.to_owned()
}

fn slice_iri_for_term(term_iri: &str) -> String {
    if term_iri.contains("/slices/") {
        let rest = if let Some(rest) = term_iri.strip_prefix(&format!("{GMEOW_NS}slices/")) {
            rest
        } else {
            term_iri
                .split_once("/slices/")
                .map(|(_, r)| r)
                .unwrap_or("")
        };
        if let Some(name) = rest.split('/').next() {
            return format!("{GMEOW_NS}slices/{name}");
        }
    }
    term_namespace(term_iri)
}

pub fn po_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

fn po_unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn po_string_body(token: &str) -> Result<String> {
    if token.starts_with("\"\"\"") && token.ends_with("\"\"\"") && token.len() >= 6 {
        return Ok(po_unescape(&token[3..token.len() - 3]));
    }
    if token.starts_with('"') && token.ends_with('"') && token.len() >= 2 {
        return Ok(po_unescape(&token[1..token.len() - 1]));
    }
    Err(Diag::of_kind(PoParse {
        detail: format!("invalid PO string token: {token:?}"),
    }))
}

fn parse_field_line(line: &str) -> Option<(&str, &str)> {
    for key in ["msgctxt", "msgid", "msgstr"] {
        if let Some(rest) = line.strip_prefix(key)
            && rest.starts_with(char::is_whitespace)
        {
            return Some((key, rest.trim()));
        }
    }
    None
}

pub fn parse_po(text: &str, require_msgctxt: bool) -> Result<Vec<PoEntry>> {
    static CONTINUATION_RE: OnceLock<Regex> = OnceLock::new();
    let continuation =
        CONTINUATION_RE.get_or_init(|| Regex::new(r#"^"(?:[^"\\]|\\.)*"$"#).expect("valid regex"));
    let mut entries = Vec::new();
    let mut fields: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current_key: Option<String> = None;
    let mut pending_fuzzy = false;
    let mut entry_fuzzy = false;

    fn flush(
        entries: &mut Vec<PoEntry>,
        fields: &mut BTreeMap<String, Vec<String>>,
        current_key: &mut Option<String>,
        entry_fuzzy: &mut bool,
        require_msgctxt: bool,
    ) -> Result<()> {
        if fields.is_empty() {
            *current_key = None;
            *entry_fuzzy = false;
            return Ok(());
        }
        let msgid = fields
            .get("msgid")
            .ok_or_else(|| {
                Diag::of_kind(PoParse {
                    detail: "PO entry missing msgid".to_owned(),
                })
            })?
            .iter()
            .map(|s| po_string_body(s))
            .collect::<Result<Vec<_>>>()?
            .join("");
        let msgctxt = fields
            .get("msgctxt")
            .map(|parts| {
                parts
                    .iter()
                    .map(|s| po_string_body(s))
                    .collect::<Result<Vec<_>>>()
                    .map(|v| v.join(""))
            })
            .transpose()?
            .unwrap_or_default();
        let msgstr = fields
            .get("msgstr")
            .map(|parts| {
                parts
                    .iter()
                    .map(|s| po_string_body(s))
                    .collect::<Result<Vec<_>>>()
                    .map(|v| v.join(""))
            })
            .transpose()?
            .unwrap_or_default();
        if !require_msgctxt || !msgctxt.is_empty() {
            entries.push(PoEntry {
                msgctxt,
                msgid,
                msgstr,
                fuzzy: *entry_fuzzy,
            });
        }
        fields.clear();
        *current_key = None;
        *entry_fuzzy = false;
        Ok(())
    }

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            flush(
                &mut entries,
                &mut fields,
                &mut current_key,
                &mut entry_fuzzy,
                require_msgctxt,
            )?;
            pending_fuzzy = false;
            continue;
        }
        if line.starts_with("#,") && line.contains("fuzzy") {
            pending_fuzzy = true;
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some((key, token)) = parse_field_line(line) {
            if key == "msgctxt" && fields.contains_key("msgid") {
                flush(
                    &mut entries,
                    &mut fields,
                    &mut current_key,
                    &mut entry_fuzzy,
                    require_msgctxt,
                )?;
            }
            if !token.starts_with('"') {
                current_key = None;
                continue;
            }
            fields
                .entry(key.to_owned())
                .or_default()
                .push(token.to_owned());
            current_key = Some(key.to_owned());
            if pending_fuzzy {
                entry_fuzzy = true;
                pending_fuzzy = false;
            }
            continue;
        }
        if continuation.is_match(line) {
            let Some(key) = current_key.as_ref() else {
                return Err(Diag::of_kind(PoParse {
                    detail: format!("PO continuation line without a field: {line:?}"),
                }));
            };
            fields.entry(key.clone()).or_default().push(line.to_owned());
            continue;
        }
        flush(
            &mut entries,
            &mut fields,
            &mut current_key,
            &mut entry_fuzzy,
            require_msgctxt,
        )?;
        pending_fuzzy = false;
    }
    flush(
        &mut entries,
        &mut fields,
        &mut current_key,
        &mut entry_fuzzy,
        require_msgctxt,
    )?;
    Ok(entries)
}

pub fn language_from_po(text: &str) -> Result<Option<String>> {
    for entry in parse_po(text, false)? {
        if entry.msgid.is_empty() {
            for line in entry.msgstr.split('\n') {
                if let Some(rest) = line.trim().strip_prefix("Language:") {
                    let code = rest.trim();
                    if !code.is_empty() {
                        return Ok(Some(code.to_owned()));
                    }
                }
            }
        }
    }
    Ok(None)
}

/// A `.po` entry that is a candidate reviewed translation: a real (non-empty)
/// translation that is NOT flagged `#, fuzzy` (human-reviewed, not machine-seeded).
pub fn is_candidate_translation(entry: &PoEntry) -> bool {
    !entry.fuzzy && !entry.msgstr.is_empty()
}

/// The target surface that counts as a LIVE translation when projecting a catalog entry
/// into the shipped `gmeow.gts` bundle (docs-rendering + translation-crossing corpora).
/// A machine-seeded `#, fuzzy` entry is not yet a reviewed translation, so it contributes
/// no live target — treated as not-yet-live (English fallback), byte-identical to an
/// untranslated entry. Single source of truth shared by both pipeline corpus builders so
/// the fuzzy-gating cannot drift between them or from the coverage axis.
pub fn live_translation_target(entry: &PoEntry) -> &str {
    if entry.fuzzy {
        ""
    } else {
        entry.msgstr.as_str()
    }
}

/// Whether a `.po` entry counts toward reviewed translation coverage: a candidate
/// translation that also passes the translation-integrity guard (not copied/hybrid
/// English). Single source of truth shared by the slice-quality translation axis and
/// the PO linter, so the reviewed-coverage policy cannot drift between them.
pub fn counts_as_reviewed_coverage(entry: &PoEntry, language: &str) -> bool {
    is_candidate_translation(entry)
        && crate::i18n::translation_has_integrity(language, &entry.msgid, &entry.msgstr)
}

fn po_header(lang: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("msgid \"\"\n");
    out.push_str("msgstr \"\"\n");
    out.push_str("\"Project-Id-Version: gmeow\\n\"\n");
    if let Some(lang) = lang {
        out.push_str(&format!("\"Language: {}\\n\"\n", po_escape(lang)));
    }
    out.push_str("\"MIME-Version: 1.0\\n\"\n");
    out.push_str("\"Content-Type: text/plain; charset=UTF-8\\n\"\n");
    out.push_str("\"Content-Transfer-Encoding: 8bit\\n\"\n\n");
    out
}

pub fn write_pot_text(entries: &[PoEntry]) -> String {
    write_po_like_text(entries, None)
}

pub fn write_po_text(entries: &[PoEntry], lang: &str) -> String {
    write_po_like_text(entries, Some(lang))
}

fn write_po_like_text(entries: &[PoEntry], lang: Option<&str>) -> String {
    let mut out = po_header(lang);
    for entry in entries {
        if entry.fuzzy {
            out.push_str("#, fuzzy\n");
        }
        if !entry.msgctxt.is_empty() {
            out.push_str(&format!("msgctxt \"{}\"\n", po_escape(&entry.msgctxt)));
        }
        out.push_str(&format!("msgid \"{}\"\n", po_escape(&entry.msgid)));
        out.push_str(&format!("msgstr \"{}\"\n\n", po_escape(&entry.msgstr)));
    }
    out
}

fn anchor_hash(text: &str) -> String {
    let digest = Sha1::digest(text.as_bytes());
    digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..12]
        .to_owned()
}

#[derive(Debug, Clone)]
struct MdSegment {
    text: String,
    trailing_blank_lines: usize,
}

fn split_markdown(text: &str) -> (Vec<MdSegment>, &'static str) {
    let newline = if text.contains("\r\n") {
        "\r\n"
    } else if text.contains('\r') {
        "\r"
    } else {
        "\n"
    };
    let mut segments = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut in_code = false;
    let mut pending_blanks = 0usize;

    fn flush(segments: &mut Vec<MdSegment>, current: &mut Vec<String>, trailing: usize) {
        if !current.is_empty() {
            segments.push(MdSegment {
                text: current.join("\n"),
                trailing_blank_lines: trailing,
            });
            current.clear();
        }
    }

    for line in text.split(newline) {
        let stripped = line.trim_start();
        if stripped.starts_with("```") {
            if in_code {
                current.push(line.to_owned());
                in_code = false;
                flush(&mut segments, &mut current, pending_blanks);
                pending_blanks = 0;
            } else {
                flush(&mut segments, &mut current, pending_blanks);
                pending_blanks = 0;
                in_code = true;
                current.push(line.to_owned());
            }
            continue;
        }
        if in_code {
            current.push(line.to_owned());
            continue;
        }
        if line.trim().is_empty() {
            if current.is_empty() {
                pending_blanks += 1;
            } else {
                flush(&mut segments, &mut current, pending_blanks);
                pending_blanks = 1;
            }
            continue;
        }
        if pending_blanks > 0 {
            if let Some(last) = segments.last_mut() {
                last.trailing_blank_lines += pending_blanks;
            }
            pending_blanks = 0;
        }
        current.push(line.to_owned());
    }
    flush(&mut segments, &mut current, pending_blanks);
    (segments, newline)
}

pub fn extract_markdown_text(text: &str, rel_path: &str) -> Vec<PoEntry> {
    let (segments, _) = split_markdown(text);
    segments
        .into_iter()
        .map(|segment| PoEntry {
            msgctxt: format!("{rel_path}|{}", anchor_hash(&segment.text)),
            msgid: segment.text,
            msgstr: String::new(),
            fuzzy: false,
        })
        .collect()
}

pub fn merge_markdown_text(source: &str, po_text: &str) -> Result<String> {
    let mut catalog = BTreeMap::new();
    for entry in parse_po(po_text, true)? {
        if let Some((_, hash)) = entry.msgctxt.rsplit_once('|') {
            catalog.insert(hash.to_owned(), entry.msgstr);
        }
    }
    let (segments, newline) = split_markdown(source);
    let mut out_lines = Vec::new();
    for segment in segments {
        let translation = catalog
            .get(&anchor_hash(&segment.text))
            .map(String::as_str)
            .unwrap_or("");
        let selected = if translation.trim().is_empty() {
            segment.text.as_str()
        } else {
            translation
        };
        out_lines.extend(selected.lines().map(str::to_owned));
        out_lines.extend(std::iter::repeat_with(String::new).take(segment.trailing_blank_lines));
    }
    Ok(out_lines.join(newline))
}

/// Every literal row of every authored Turtle source, parsed ONCE.
///
/// Three projections read this same corpus (the BCP-47 ↔ internal language map, the current
/// English values, and the declared homograph sources). Each used to walk
/// [`authored_turtle_files`] and re-parse every one of them, so a single `lint_po_files`
/// call paid the whole authored-Turtle parse three times over for three different views of
/// identical bytes. Parsing once and projecting three ways is the same answer for a third
/// of the work — and, more importantly, makes it impossible for the three views to be
/// taken of different reads.
fn authored_literal_rows(root: &Path) -> Vec<RdfLiteralRow> {
    let mut rows = Vec::new();
    for source in authored_turtle_files(root) {
        let Ok(bytes) = fs::read(&source) else {
            continue;
        };
        let Ok(parsed) = parse_rdf_literals(&bytes, "turtle") else {
            continue;
        };
        rows.extend(parsed);
    }
    rows
}

fn bcp47_to_internal_map_from_rows(rows: &[RdfLiteralRow]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::from([
        ("en".to_owned(), "x-gmeow-english".to_owned()),
        ("fr".to_owned(), "x-gmeow-french".to_owned()),
        ("zh".to_owned(), "x-gmeow-mandarin".to_owned()),
    ]);
    let mut by_subject: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    let bcp47_tag_pred = format!("{GMEOW_NS}bcp47Tag");
    let language_tag_pred = format!("{GMEOW_NS}languageTag");
    for row in rows {
        if row.predicate == bcp47_tag_pred || row.predicate == language_tag_pred {
            by_subject
                .entry(row.subject.clone())
                .or_default()
                .entry(row.predicate.clone())
                .or_default()
                .insert(row.lexical.clone());
        }
    }
    for props in by_subject.values() {
        let bcps = props.get(&bcp47_tag_pred).cloned().unwrap_or_default();
        let internals = props.get(&language_tag_pred).cloned().unwrap_or_default();
        if bcps.len() == 1 && internals.len() == 1 {
            out.insert(
                bcps.into_iter().next().unwrap().to_ascii_lowercase(),
                internals.into_iter().next().unwrap(),
            );
        }
    }
    out
}

/// The set of English source skeletons declared as terminology homographs
/// (`lang:DeclaredTerminologyHomograph` → `lang:homographSource`) in the authored
/// ontology. A source in this set is exempted from the glossary-consistency check
/// (its distinct senses legitimately render differently); it is the ontology-resident,
/// non-calibrated escape.
///
/// Read ONLY from authored TTL (`module.ttl`/`manifest.ttl`/`ontology/gmeow.ttl` via
/// [`authored_turtle_files`]), NEVER from the derived `graph/lang-glossary-corpus` nor the
/// twin fixtures under `tests/`. `lang:homographSource` is authored solely on homograph
/// declarations (never on the derived glossary entries, which carry `gmeow:glossarySource`),
/// so this predicate-scoped read can never pick up a derived entry literal and silently
/// widen the exempt set.
pub fn declared_homograph_sources(root: &Path) -> BTreeSet<String> {
    declared_homograph_sources_from_rows(&authored_literal_rows(root))
}

fn declared_homograph_sources_from_rows(rows: &[RdfLiteralRow]) -> BTreeSet<String> {
    const HOMOGRAPH_SOURCE_PRED: &str = "https://blackcatinformatics.ca/lang/homographSource";
    rows.iter()
        .filter(|row| row.predicate == HOMOGRAPH_SOURCE_PRED)
        .map(|row| skeleton(&row.lexical))
        .collect()
}

/// The authored Turtle files the homograph escape reads (`ontology/gmeow.ttl` plus every
/// slice `module.ttl`/`manifest.ttl`), sorted. Exposed so a stage that derives from
/// [`declared_homograph_sources`] can fold these exact bytes into its cache key — the
/// SAME read surface, never a re-authored guess.
pub fn authored_turtle_files(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let ontology = root.join("ontology/gmeow.ttl");
    if ontology.is_file() {
        paths.push(ontology);
    }
    if let Ok(groups) = fs::read_dir(root.join("slices")) {
        for group in groups.flatten() {
            if let Ok(slices) = fs::read_dir(group.path()) {
                for slice in slices.flatten() {
                    for name in ["module.ttl", "manifest.ttl"] {
                        let path = slice.path().join(name);
                        if path.is_file() {
                            paths.push(path);
                        }
                    }
                }
            }
        }
    }
    paths.sort();
    paths
}

fn collect_po_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let slices = root.join("slices");
    if let Ok(groups) = fs::read_dir(&slices) {
        for group in groups.flatten() {
            if let Ok(names) = fs::read_dir(group.path()) {
                for name in names.flatten() {
                    let i18n = name.path().join("i18n");
                    if let Ok(files) = fs::read_dir(i18n) {
                        for file in files.flatten() {
                            let path = file.path();
                            if path.extension().and_then(|s| s.to_str()) == Some("po") {
                                paths.push(path);
                            }
                        }
                    }
                }
            }
        }
    }
    let fixtures = root.join("tests/fixtures/i18n");
    if let Ok(files) = fs::read_dir(fixtures) {
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|s| s.to_str()) == Some("po") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
}

fn current_english_values_from_rows(
    rows: &[RdfLiteralRow],
) -> BTreeMap<(String, String), BTreeSet<String>> {
    let mut values: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        if row.language.as_deref() == Some(ENGLISH_TAG) {
            values
                .entry((row.subject.clone(), row.predicate.clone()))
                .or_default()
                .insert(row.lexical.clone());
        }
    }
    values
}

pub fn lint_po_files(root: &Path, max_fuzzy_ratio: f64) -> I18nLintReport {
    let mut report = I18nLintReport::default();
    // ONE parse of the authored Turtle corpus, three projections of it. The three used to
    // walk and re-parse every authored source independently.
    let authored = authored_literal_rows(root);
    let tag_map = bcp47_to_internal_map_from_rows(&authored);
    let current = current_english_values_from_rows(&authored);
    // Ontology-resident escape for the glossary-consistency check: English sources
    // explicitly declared homographs (distinct senses that legitimately render
    // differently). Read from authored TTL only.
    let homographs = declared_homograph_sources_from_rows(&authored);

    for path in collect_po_paths(root)
        .into_iter()
        .filter(|path| path.starts_with(root.join("slices")))
    {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                report
                    .errors
                    .push(format!("{rel}: cannot read PO file: {err}"));
                continue;
            }
        };
        let language = match language_from_po(&text) {
            Ok(Some(lang)) => lang.to_ascii_lowercase(),
            Ok(None) => {
                report
                    .errors
                    .push(format!("{rel}: missing Language header"));
                continue;
            }
            Err(err) => {
                report.errors.push(format!("{rel}: PO parse error: {err}"));
                continue;
            }
        };
        let Some(internal) = tag_map.get(&language).cloned() else {
            report.errors.push(format!(
                "{rel}: no GMEOW internal tag mapping for Language: {language}"
            ));
            continue;
        };
        let entries = match parse_po(&text, true) {
            Ok(entries) => entries,
            Err(err) => {
                report.errors.push(format!("{rel}: PO parse error: {err}"));
                continue;
            }
        };
        let mut total = 0usize;
        let mut fuzzy = 0usize;
        // (msgid_skeleton, msgstr_skeleton, msgctxt) for every candidate translation in
        // THIS catalog — the distinctiveness invariant runs over it after the loop: a
        // target skeleton shared across distinct source skeletons is a collapsed
        // distinction (a near-duplicate translation), while twin sources sharing one
        // translation are legitimate.
        let mut xlat_triples: Vec<(String, String, String)> = Vec::new();
        for entry in entries {
            total += 1;
            if entry.fuzzy {
                fuzzy += 1;
            }
            if is_candidate_translation(&entry) {
                if let Some(reason) =
                    translation_integrity_issue(&language, &entry.msgid, &entry.msgstr)
                {
                    report.errors.push(format!(
                        "{rel}: invalid translation {:?}: {reason}",
                        entry.msgctxt
                    ));
                }
                xlat_triples.push((
                    skeleton(&entry.msgid),
                    skeleton(&entry.msgstr),
                    entry.msgctxt.clone(),
                ));
            }
            if !entry.msgctxt.contains('|') {
                report
                    .errors
                    .push(format!("{rel}: invalid msgctxt {:?}", entry.msgctxt));
                continue;
            }
            let (term, predicate) = entry.msgctxt.split_once('|').unwrap();
            let predicate = expand_predicate(predicate);
            let key = (term.to_owned(), predicate.clone());
            match current.get(&key) {
                None => report.warnings.push(format!(
                    "{rel}: orphaned entry {:?}: no current @x-gmeow-english literal for {} {}",
                    entry.msgctxt, term, predicate
                )),
                Some(values) if !values.contains(&entry.msgid) => report.warnings.push(format!(
                    "{rel}: stale entry {:?}: msgid does not match current @x-gmeow-english literal",
                    entry.msgctxt
                )),
                Some(_) => {}
            }
        }
        // Glossary terminology CONSISTENCY — the functional-dependency DUAL of the
        // distinctiveness check below. Within THIS catalog (one slice, one language) one
        // English source translated two different ways across batches is a hard reject,
        // UNLESS the source is a declared homograph (its distinct senses legitimately
        // render differently). distinctiveness_violations groups by its 2nd column and
        // flags >=2 distinct 1st columns, so feeding (msgstr_skel, msgid_skel, msgctxt)
        // groups by the English SOURCE and flags a source that splits into >=2 renderings.
        // Borrows xlat_triples (the distinctiveness call below then consumes it).
        let glossary_triples = xlat_triples
            .iter()
            .filter(|(msgid_skel, _, _)| !homographs.contains(msgid_skel))
            .map(|(msgid_skel, msgstr_skel, ctx)| {
                (msgstr_skel.clone(), msgid_skel.clone(), ctx.clone())
            });
        for c in distinctiveness_violations(glossary_triples) {
            report.errors.push(format!(
                "{rel}: English source {:?} is translated {} different ways across batches — a per-slice glossary must translate one term consistently (lang:GlossaryTermInconsistency): {}",
                c.skeleton,
                c.members.len(),
                c.members.join(", ")
            ));
        }
        // Translation DISTINCTIVENESS: a msgstr skeleton shared across distinct msgid
        // sources means the translation collapsed a distinction the source made — a hard
        // reject. Twin sources (same English label on a class and its property twin)
        // sharing one translation legitimately do NOT collide (identical msgid skeleton).
        for c in distinctiveness_violations(xlat_triples) {
            report.errors.push(format!(
                "{rel}: msgstr {:?} collides across {} distinct sources — a translation must preserve every distinction its source makes: {}",
                c.skeleton,
                c.members.len(),
                c.members.join(", ")
            ));
        }
        if total > 0 {
            *report.total_counts.entry(internal.clone()).or_insert(0) += total;
            *report.fuzzy_counts.entry(internal.clone()).or_insert(0) += fuzzy;
            if fuzzy == total {
                report.errors.push(format!(
                    "{internal} has only fuzzy entries ({fuzzy}/{total})"
                ));
            } else {
                let ratio = (fuzzy as f64 / total as f64) * 100.0;
                if ratio > max_fuzzy_ratio {
                    report.errors.push(format!(
                        "{internal} is {ratio:.1}% fuzzy ({fuzzy}/{total}), over {max_fuzzy_ratio:.1}% limit"
                    ));
                }
            }
        }
    }

    report
}

fn slice_group_name(root: &Path, slice_dir: &Path) -> (String, String) {
    let rel = slice_dir
        .strip_prefix(root.join("slices"))
        .ok()
        .and_then(|p| {
            let mut parts = p.components().filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str().map(str::to_owned),
                _ => None,
            });
            Some((parts.next()?, parts.next()?))
        });
    rel.unwrap_or_else(|| ("_core".to_owned(), "_".to_owned()))
}

fn collect_slice_terms(root: &Path) -> Result<BTreeMap<String, Vec<TranslationKey>>> {
    let catalog = SliceCatalog::discover(&root.join("slices"), gmeow_ns::gmeow_slice_vocab())?;
    let localizable: HashSet<&str> = LOCALIZABLE_PREDICATES.iter().copied().collect();
    let mut groups: BTreeMap<String, BTreeMap<(String, String), TranslationKey>> = BTreeMap::new();
    let mut english_seen: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();

    for record in catalog.records() {
        let slice_iri = record.manifest.slice_iri.clone();
        for artifact in record
            .artifacts
            .iter()
            .filter(|artifact| artifact.role == ArtifactRole::Module)
        {
            let rows = parse_rdf_literals(&artifact.content, "turtle")?;
            for row in rows {
                if !localizable.contains(row.predicate.as_str()) {
                    continue;
                }
                let key = (
                    slice_iri.clone(),
                    row.subject.clone(),
                    row.predicate.clone(),
                );
                if row.language.as_deref() == Some(ENGLISH_TAG) {
                    english_seen
                        .entry(key.clone())
                        .or_default()
                        .insert(row.lexical.clone());
                    if english_seen.get(&key).is_some_and(|s| s.len() > 1) {
                        return Err(Diag::of_kind(CatalogInconsistent {
                            detail: format!(
                                "multiple distinct @x-gmeow-english values for {} {} in {}",
                                row.subject, row.predicate, slice_iri
                            ),
                        }));
                    }
                    groups.entry(slice_iri.clone()).or_default().insert(
                        (row.subject.clone(), row.predicate.clone()),
                        TranslationKey {
                            slice_iri: slice_iri.clone(),
                            term_iri: row.subject,
                            predicate: row.predicate,
                            english_value: row.lexical,
                        },
                    );
                } else if row.language.is_none() {
                    groups
                        .entry(slice_iri.clone())
                        .or_default()
                        .entry((row.subject.clone(), row.predicate.clone()))
                        .or_insert_with(|| TranslationKey {
                            slice_iri: slice_iri.clone(),
                            term_iri: row.subject,
                            predicate: row.predicate,
                            english_value: row.lexical,
                        });
                }
            }
        }
    }

    Ok(groups
        .into_iter()
        .map(|(slice, entries)| (slice, entries.into_values().collect()))
        .collect())
}

pub fn extract_catalog(
    root: &Path,
    output_dir: &Path,
    lang: Option<&str>,
    terms_only: bool,
) -> Result<ExtractReport> {
    let catalog = SliceCatalog::discover(&root.join("slices"), gmeow_ns::gmeow_slice_vocab())?;
    let by_iri: BTreeMap<String, (String, String)> = catalog
        .records()
        .iter()
        .map(|r| {
            (
                r.manifest.slice_iri.clone(),
                slice_group_name(root, &r.slice_dir),
            )
        })
        .collect();
    let groups = collect_slice_terms(root)?;
    let mut total_keys = 0usize;
    for (slice_iri, keys) in &groups {
        total_keys += keys.len();
        let (group, name) = by_iri
            .get(slice_iri)
            .cloned()
            .unwrap_or_else(|| ("_core".to_owned(), slice_iri_for_term(slice_iri)));
        let path = if let Some(lang) = lang {
            output_dir
                .join("slices")
                .join(group)
                .join(name)
                .join("i18n")
                .join(format!("{lang}.po"))
        } else {
            output_dir
                .join("slices")
                .join(group)
                .join(format!("{name}.pot"))
        };
        let entries: Vec<PoEntry> = keys
            .iter()
            .map(|key| PoEntry {
                msgctxt: format!("{}|{}", key.term_iri, key.predicate),
                msgid: key.english_value.clone(),
                msgstr: String::new(),
                fuzzy: false,
            })
            .collect();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = lang
            .map(|l| write_po_text(&entries, l))
            .unwrap_or_else(|| write_pot_text(&entries));
        fs::write(&path, text).map_err(|e| {
            Diag::of_kind(FileIo {
                detail: format!("{}: {e}", path.display()),
            })
        })?;
    }

    if !terms_only {
        let docs_output = output_dir.join("docs");
        fs::create_dir_all(&docs_output)?;
        let mut md_sources = Vec::new();
        collect_markdown_sources(root, &mut md_sources);
        for source in md_sources {
            let rel = source.strip_prefix(root).unwrap_or(&source);
            let text = fs::read_to_string(&source)?;
            let entries = extract_markdown_text(&text, &rel.to_string_lossy());
            let path = if let Some(lang) = lang {
                docs_output.join(format!("{}.{}.po", rel.display(), lang))
            } else {
                docs_output.join(format!("{}.pot", rel.display()))
            };
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let body = lang
                .map(|l| write_po_text(&entries, l))
                .unwrap_or_else(|| write_pot_text(&entries));
            fs::write(&path, body).map_err(|e| {
                Diag::of_kind(FileIo {
                    detail: format!("{}: {e}", path.display()),
                })
            })?;
        }

        let template_entries: Vec<PoEntry> = crate::i18n::UI_TEMPLATES
            .iter()
            .map(|(key, value)| PoEntry {
                msgctxt: format!("ontology-docs-template|{key}"),
                msgid: (*value).to_owned(),
                msgstr: String::new(),
                fuzzy: false,
            })
            .collect();
        let template_path = if let Some(lang) = lang {
            output_dir.join(format!("ontology-docs-templates.{lang}.po"))
        } else {
            output_dir.join("ontology-docs-templates.pot")
        };
        let body = lang
            .map(|l| write_po_text(&template_entries, l))
            .unwrap_or_else(|| write_pot_text(&template_entries));
        fs::write(&template_path, body).map_err(|e| {
            Diag::of_kind(FileIo {
                detail: format!("{}: {e}", template_path.display()),
            })
        })?;
    }

    Ok(ExtractReport {
        groups: groups.len(),
        total_keys,
    })
}

fn collect_markdown_sources(root: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(groups) = fs::read_dir(root.join("slices")) {
        for group in groups.flatten() {
            if let Ok(slices) = fs::read_dir(group.path()) {
                for slice in slices.flatten() {
                    let docs = slice.path().join("docs.md");
                    if docs.is_file() {
                        out.push(docs);
                    }
                }
            }
        }
    }
    if let Ok(docs) = fs::read_dir(root.join("docs")) {
        for doc in docs.flatten() {
            let path = doc.path();
            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                out.push(path);
            }
        }
    }
    let readme = root.join("README.md");
    if readme.is_file() {
        out.push(readme);
    }
    out.sort();
}

#[derive(Debug, Clone)]
pub struct CatalogExportEntry {
    pub slice: String,
    pub slice_path: String,
    pub term_iri: String,
    pub predicate: String,
    pub language: String,
    pub msgid: String,
    pub msgstr: String,
    pub fuzzy: bool,
}

pub fn iter_po_catalogs(root: &Path) -> Result<Vec<CatalogExportEntry>> {
    let mut out = Vec::new();
    for path in collect_po_paths(root)
        .into_iter()
        .filter(|p| p.starts_with(root.join("slices")))
    {
        let text = fs::read_to_string(&path)?;
        let language = language_from_po(&text)?.unwrap_or_default();
        let slice_dir = path.parent().and_then(Path::parent);
        let slice = path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned();
        let slice_path = slice_dir
            .and_then(|p| p.strip_prefix(root).ok())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        for entry in parse_po(&text, true)? {
            let Some((term, predicate)) = entry.msgctxt.split_once('|') else {
                continue;
            };
            out.push(CatalogExportEntry {
                slice: slice.clone(),
                slice_path: slice_path.clone(),
                term_iri: term.to_owned(),
                predicate: predicate.to_owned(),
                language: language.clone(),
                msgid: entry.msgid,
                msgstr: entry.msgstr,
                fuzzy: entry.fuzzy,
            });
        }
    }
    Ok(out)
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

pub fn export_csv(root: &Path, output: Option<&Path>) -> Result<String> {
    let mut text = String::from("slice,term_iri,predicate,language,msgid,msgstr,fuzzy\n");
    for row in iter_po_catalogs(root)? {
        text.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_escape(&row.slice),
            csv_escape(&row.term_iri),
            csv_escape(&row.predicate),
            csv_escape(&row.language),
            csv_escape(&row.msgid),
            csv_escape(&row.msgstr),
            if row.fuzzy { "true" } else { "false" }
        ));
    }
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &text)?;
    }
    Ok(text)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn export_xliff(root: &Path, output: Option<&Path>) -> Result<String> {
    let rows = iter_po_catalogs(root)?;
    let mut by_file: BTreeMap<(String, String), Vec<CatalogExportEntry>> = BTreeMap::new();
    for row in rows {
        by_file
            .entry((row.slice_path.clone(), row.language.clone()))
            .or_default()
            .push(row);
    }
    let mut text = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    text.push_str("<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\n");
    for ((slice_path, language), rows) in by_file {
        text.push_str(&format!(
            "  <file original=\"{}\" source-language=\"en\" target-language=\"{}\" datatype=\"plaintext\">\n",
            xml_escape(&slice_path),
            xml_escape(&language)
        ));
        text.push_str("    <body>\n");
        for row in rows {
            let id = format!("{}|{}", row.term_iri, row.predicate);
            let state = if row.fuzzy {
                "needs-review-translation"
            } else {
                "translated"
            };
            text.push_str(&format!(
                "      <trans-unit id=\"{}\">\n        <source>{}</source>\n        <target state=\"{state}\">{}</target>\n        <note>Term: {} Predicate: {}</note>\n      </trans-unit>\n",
                xml_escape(&id),
                xml_escape(&row.msgid),
                xml_escape(&row.msgstr),
                xml_escape(&row.term_iri),
                xml_escape(&row.predicate)
            ));
        }
        text.push_str("    </body>\n  </file>\n");
    }
    text.push_str("</xliff>\n");
    if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &text)?;
    }
    Ok(text)
}

fn nt_literal(value: &str, lang: &str) -> String {
    format!("\"{}\"@{}", po_escape(value), lang)
}

pub fn merge_terms(root: &Path, output: Option<&Path>, lang: Option<&str>) -> Result<MergeReport> {
    // One parse of the authored Turtle corpus, two projections of it (see
    // [`authored_literal_rows`]).
    let authored = authored_literal_rows(root);
    let tag_map = bcp47_to_internal_map_from_rows(&authored);
    let base_values = current_english_values_from_rows(&authored);
    let mut added = 0usize;
    let mut text = String::new();
    let mut po_count = 0usize;
    for path in collect_po_paths(root)
        .into_iter()
        .filter(|p| p.starts_with(root.join("slices")))
    {
        let po_text = fs::read_to_string(&path)?;
        let language = language_from_po(&po_text)?
            .unwrap_or_default()
            .to_ascii_lowercase();
        if lang.is_some_and(|wanted| wanted.to_ascii_lowercase() != language) {
            continue;
        }
        let Some(internal) = tag_map.get(&language) else {
            return Err(Diag::of_kind(CatalogInconsistent {
                detail: format!(
                    "{}: no internal language tag for {language}",
                    path.display()
                ),
            }));
        };
        po_count += 1;
        for entry in parse_po(&po_text, true)? {
            if entry.msgstr.is_empty() {
                continue;
            }
            if let Some(reason) =
                translation_integrity_issue(&language, &entry.msgid, &entry.msgstr)
            {
                return Err(Diag::of_kind(CatalogInconsistent {
                    detail: format!(
                        "{}: invalid translation {:?}: {reason}",
                        path.display(),
                        entry.msgctxt
                    ),
                }));
            }
            let Some((term, predicate)) = entry.msgctxt.split_once('|') else {
                return Err(Diag::of_kind(CatalogInconsistent {
                    detail: format!("{}: invalid msgctxt {:?}", path.display(), entry.msgctxt),
                }));
            };
            let predicate = expand_predicate(predicate);
            if !base_values.contains_key(&(term.to_owned(), predicate.clone())) {
                return Err(Diag::of_kind(CatalogInconsistent {
                    detail: format!(
                        "{}: unknown term/predicate {} {}",
                        path.display(),
                        term,
                        predicate
                    ),
                }));
            }
            text.push_str(&format!(
                "<{}> <{}> {} .\n",
                term,
                predicate,
                nt_literal(&entry.msgstr, internal)
            ));
            added += 1;
        }
    }
    let output_note = if let Some(path) = output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &text)?;
        path.display().to_string()
    } else {
        "stdout".to_owned()
    };
    Ok(MergeReport {
        po_files: po_count,
        added,
        output_note,
        turtle: text,
    })
}

pub fn sync_english_file(po_path: &Path, source_path: &Path, dry_run: bool) -> Result<SyncReport> {
    let po_text = fs::read_to_string(po_path)?;
    let source_text = fs::read_to_string(source_path)?;
    if source_path.extension().and_then(|s| s.to_str()) == Some("md")
        || po_path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.ends_with(".md.po"))
    {
        return sync_markdown(po_path, source_path, &po_text, &source_text, dry_run);
    }
    if source_path.extension().and_then(|s| s.to_str()) == Some("ttl")
        || po_path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.ends_with(".ttl.po") || n == "en.po")
    {
        return sync_turtle(po_path, source_path, &po_text, &source_text, dry_run);
    }
    Err(Diag::of_kind(UnsupportedSource {
        detail: format!("unsupported source file type: {}", source_path.display()),
    }))
}

fn sync_markdown(
    _po_path: &Path,
    source_path: &Path,
    po_text: &str,
    source_text: &str,
    dry_run: bool,
) -> Result<SyncReport> {
    let mut report = SyncReport::default();
    let mut text = source_text.to_owned();
    for entry in parse_po(po_text, false)? {
        if entry.msgid.is_empty() {
            continue;
        }
        let positions: Vec<_> = text
            .match_indices(&entry.msgid)
            .map(|(idx, _)| idx)
            .collect();
        if positions.len() > 1 {
            report.skipped.push(format!(
                "ambiguous segment {:?}: {} occurrences",
                entry.msgid,
                positions.len()
            ));
            continue;
        }
        if positions.is_empty() {
            if entry.msgid == entry.msgstr {
                report.skipped.push(format!(
                    "source changed, PO unchanged for segment {:?}",
                    entry.msgid
                ));
            } else {
                report.conflicts.push(format!(
                    "conflict: source and PO both changed for segment {:?}",
                    entry.msgid
                ));
            }
            continue;
        }
        if entry.msgid == entry.msgstr {
            report.unchanged.push(entry.msgid);
            continue;
        }
        let start = positions[0];
        text.replace_range(start..start + entry.msgid.len(), &entry.msgstr);
    }
    if text != source_text {
        report.changed_files.push(source_path.to_path_buf());
        if !dry_run {
            fs::write(source_path, text)?;
        }
    }
    Ok(report)
}

fn turtle_unescape(value: &str) -> Result<String> {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            return Err(Diag::of_kind(TurtleUnescape {
                detail: "invalid Turtle escape sequence".to_owned(),
            }));
        };
        match next {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            'u' => {
                let hex: String = chars.by_ref().take(4).collect();
                if hex.len() != 4 {
                    return Err(Diag::of_kind(TurtleUnescape {
                        detail: "invalid Turtle escape sequence".to_owned(),
                    }));
                }
                let code = u32::from_str_radix(&hex, 16).map_err(|e| {
                    Diag::of_kind(TurtleUnescape {
                        detail: format!("invalid Turtle escape sequence: {e}"),
                    })
                })?;
                let scalar = char::from_u32(code).ok_or_else(|| {
                    Diag::of_kind(TurtleUnescape {
                        detail: "invalid Turtle Unicode scalar".to_owned(),
                    })
                })?;
                out.push(scalar);
            }
            'U' => {
                let hex: String = chars.by_ref().take(8).collect();
                if hex.len() != 8 {
                    return Err(Diag::of_kind(TurtleUnescape {
                        detail: "invalid Turtle escape sequence".to_owned(),
                    }));
                }
                let code = u32::from_str_radix(&hex, 16).map_err(|e| {
                    Diag::of_kind(TurtleUnescape {
                        detail: format!("invalid Turtle escape sequence: {e}"),
                    })
                })?;
                let scalar = char::from_u32(code).ok_or_else(|| {
                    Diag::of_kind(TurtleUnescape {
                        detail: "invalid Turtle Unicode scalar".to_owned(),
                    })
                })?;
                out.push(scalar);
            }
            other => {
                return Err(Diag::of_kind(TurtleUnescape {
                    detail: format!("invalid Turtle escape sequence: \\{other}"),
                }));
            }
        }
    }
    Ok(out)
}

fn turtle_escape_single(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn turtle_escape_triple(value: &str) -> String {
    value.replace('\\', "\\\\").replace("\"\"\"", "\\\"\"\"")
}

fn extract_prefixes(text: &str) -> BTreeMap<String, String> {
    static PREFIX_RE: OnceLock<Regex> = OnceLock::new();
    let re = PREFIX_RE.get_or_init(|| {
        Regex::new(r"@prefix\s+([A-Za-z_][A-Za-z0-9_-]*)?\s*:\s*<([^>]+)>\s*\.").unwrap()
    });
    let mut prefixes = BTreeMap::new();
    for cap in re.captures_iter(text) {
        prefixes.insert(
            cap.get(1).map(|m| m.as_str()).unwrap_or("").to_owned(),
            cap.get(2).map(|m| m.as_str()).unwrap_or("").to_owned(),
        );
    }
    prefixes
}

fn iri_text_forms(iri: &str, prefixes: &BTreeMap<String, String>) -> Vec<String> {
    let mut forms = vec![format!("<{iri}>")];
    for (prefix, namespace) in prefixes {
        let Some(local) = iri.strip_prefix(namespace) else {
            continue;
        };
        if local.is_empty()
            || !local
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            || !local
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        {
            continue;
        }
        forms.push(format!("{prefix}:{local}"));
    }
    forms
}

/// The byte offset one CODEPOINT past `i`.
///
/// The Turtle scanners in this module walk a byte cursor and then slice `text[i..]`, which
/// PANICS the instant the cursor lands inside a multi-byte codepoint — i.e. on any authored
/// Turtle containing a single non-ASCII character (a `é`, a `—`, a CJK label, any of which
/// the localized slice sources are full of). Advancing by codepoints instead of by bytes is
/// what makes the cursor a valid slice index at every step.
///
/// `i` must already be a char boundary; the walk keeps it one.
fn next_char_boundary(text: &str, i: usize) -> usize {
    let mut next = i + 1;
    while next < text.len() && !text.is_char_boundary(next) {
        next += 1;
    }
    next
}

fn skip_triple_quoted(text: &str, mut i: usize, end: usize, quote: &str) -> usize {
    i += quote.len();
    while i < end {
        if text.as_bytes()[i] == b'\\' && i + 1 < end {
            // The ESCAPED character may itself be multi-byte (`\é`), so step over the
            // backslash and then over one whole codepoint.
            i = next_char_boundary(text, i + 1);
        } else if text[i..].starts_with(quote) {
            return i + quote.len();
        } else {
            i = next_char_boundary(text, i);
        }
    }
    end
}

fn skip_single_quoted(text: &str, mut i: usize, end: usize, quote: u8) -> usize {
    i += 1;
    while i < end {
        let ch = text.as_bytes()[i];
        if ch == b'\\' && i + 1 < end {
            i = next_char_boundary(text, i + 1);
        } else if ch == quote {
            return i + 1;
        } else {
            i = next_char_boundary(text, i);
        }
    }
    end
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Iri,
    Prefixed,
    Bnode,
    BnStart,
    BnEnd,
    Sep,
    Dot,
    KeywordA,
}

#[derive(Debug, Clone)]
struct TurtleToken {
    kind: TokenKind,
    value: String,
}

fn is_name_start(ch: u8) -> bool {
    ch.is_ascii_alphabetic() || ch == b'_'
}

fn is_name_continue(ch: u8) -> bool {
    ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'-'
}

fn parse_prefixed_token(text: &str, i: usize, end: usize) -> Option<(usize, String)> {
    let bytes = text.as_bytes();
    let mut j = i;
    if j < end && is_name_start(bytes[j]) {
        j += 1;
        while j < end && is_name_continue(bytes[j]) {
            j += 1;
        }
    }
    if j >= end || bytes[j] != b':' {
        return None;
    }
    j += 1;
    let local_start = j;
    if j >= end || !is_name_start(bytes[j]) {
        return None;
    }
    j += 1;
    while j < end && is_name_continue(bytes[j]) {
        j += 1;
    }
    if local_start == j {
        return None;
    }
    Some((j, text[i..j].to_owned()))
}

fn tokenize_turtle(text: &str, end: usize) -> Vec<TurtleToken> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < end {
        let ch = bytes[i];
        if ch.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if ch == b'#' {
            while i < end && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if text[i..].starts_with("\"\"\"") {
            i = skip_triple_quoted(text, i, end, "\"\"\"");
            continue;
        }
        if text[i..].starts_with("'''") {
            i = skip_triple_quoted(text, i, end, "'''");
            continue;
        }
        if ch == b'"' || ch == b'\'' {
            i = skip_single_quoted(text, i, end, ch);
            continue;
        }
        if ch == b'<'
            && let Some(offset) = text[i..end].find('>')
        {
            let j = i + offset + 1;
            tokens.push(TurtleToken {
                kind: TokenKind::Iri,
                value: text[i..j].to_owned(),
            });
            i = j;
            continue;
        }
        if ch == b'_' && i + 1 < end && bytes[i + 1] == b':' {
            let mut j = i + 2;
            if j < end && is_name_start(bytes[j]) {
                j += 1;
                while j < end && is_name_continue(bytes[j]) {
                    j += 1;
                }
                tokens.push(TurtleToken {
                    kind: TokenKind::Bnode,
                    value: text[i..j].to_owned(),
                });
                i = j;
                continue;
            }
        }
        let simple = match ch {
            b'[' => Some(TokenKind::BnStart),
            b']' => Some(TokenKind::BnEnd),
            b';' | b',' => Some(TokenKind::Sep),
            b'.' => Some(TokenKind::Dot),
            _ => None,
        };
        if let Some(kind) = simple {
            tokens.push(TurtleToken {
                kind,
                value: text[i..i + 1].to_owned(),
            });
            i += 1;
            continue;
        }
        if ch == b'a'
            && (i == 0 || !is_name_continue(bytes[i - 1]))
            && (i + 1 == end || !is_name_continue(bytes[i + 1]))
        {
            tokens.push(TurtleToken {
                kind: TokenKind::KeywordA,
                value: "a".to_owned(),
            });
            i += 1;
            continue;
        }
        if (is_name_start(ch) || ch == b':')
            && let Some((j, value)) = parse_prefixed_token(text, i, end)
        {
            tokens.push(TurtleToken {
                kind: TokenKind::Prefixed,
                value,
            });
            i = j;
            continue;
        }
        // By CODEPOINT: this loop slices `text[i..]` on every iteration, so a byte step
        // through a non-ASCII character panics on the next one.
        i = next_char_boundary(text, i);
    }
    tokens
}

fn is_subject_kind(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Iri | TokenKind::Prefixed | TokenKind::Bnode | TokenKind::BnStart
    )
}

fn is_predicate_introducer_kind(kind: TokenKind) -> bool {
    is_subject_kind(kind) || matches!(kind, TokenKind::BnEnd | TokenKind::Sep)
}

fn extract_context(
    text: &str,
    pos: usize,
    subject_forms: &[String],
    predicate_forms: &[String],
) -> (Option<String>, Option<String>) {
    let tokens = tokenize_turtle(text, pos);
    let mut predicate: Option<String> = None;
    let mut predicate_idx: Option<usize> = None;

    for i in (0..tokens.len()).rev() {
        let token = &tokens[i];
        if token.kind == TokenKind::Dot {
            break;
        }
        if token.kind == TokenKind::Sep {
            continue;
        }
        if !matches!(
            token.kind,
            TokenKind::Iri | TokenKind::Prefixed | TokenKind::KeywordA
        ) {
            continue;
        }
        if i == 0 {
            continue;
        }
        let prev = &tokens[i - 1];
        if !is_predicate_introducer_kind(prev.kind) {
            continue;
        }
        if prev.kind == TokenKind::Sep && prev.value != ";" {
            continue;
        }
        predicate = Some(token.value.clone());
        predicate_idx = Some(i);
        break;
    }

    let Some(predicate_idx) = predicate_idx else {
        return (None, predicate);
    };
    if !predicate
        .as_ref()
        .is_some_and(|p| predicate_forms.iter().any(|form| form == p))
    {
        return (None, predicate);
    }

    let mut subject = None;
    for i in (0..predicate_idx).rev() {
        let token = &tokens[i];
        if token.kind == TokenKind::Dot {
            break;
        }
        if !is_subject_kind(token.kind) {
            continue;
        }
        if token.kind == TokenKind::BnStart || i == 0 || tokens[i - 1].kind == TokenKind::Dot {
            subject = Some(token.value.clone());
            break;
        }
    }
    if subject
        .as_ref()
        .is_some_and(|s| !subject_forms.iter().any(|form| form == s))
    {
        return (None, predicate);
    }
    (subject, predicate)
}

#[derive(Debug, Clone, Copy)]
enum QuoteStyle {
    Single,
    Triple,
}

#[derive(Debug, Clone)]
struct LiteralCandidate {
    start: usize,
    end: usize,
    style: QuoteStyle,
    decoded: String,
}

fn suffix_end(text: &str, mut i: usize) -> usize {
    let bytes = text.as_bytes();
    while i < text.len() {
        let ch = bytes[i];
        if ch.is_ascii_whitespace()
            || matches!(
                ch,
                b'.' | b',' | b';' | b'[' | b']' | b'{' | b'}' | b'(' | b')'
            )
        {
            break;
        }
        i += 1;
    }
    i
}

fn english_literal_candidates(text: &str) -> Vec<LiteralCandidate> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let english_tag_suffix = format!("@{ENGLISH_TAG}");
    while i < text.len() {
        let ch = bytes[i];
        if ch == b'#' {
            while i < text.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if text[i..].starts_with("\"\"\"") {
            let lexical_start = i + 3;
            let quoted_end = skip_triple_quoted(text, i, text.len(), "\"\"\"");
            if quoted_end <= lexical_start || quoted_end > text.len() {
                break;
            }
            let lexical_end = quoted_end.saturating_sub(3);
            let end = suffix_end(text, quoted_end);
            if text[quoted_end..end] == english_tag_suffix
                && let Ok(decoded) = turtle_unescape(&text[lexical_start..lexical_end])
            {
                out.push(LiteralCandidate {
                    start: i,
                    end,
                    style: QuoteStyle::Triple,
                    decoded,
                });
            }
            i = quoted_end.max(i + 3);
            continue;
        }
        if ch == b'"' {
            let lexical_start = i + 1;
            let quoted_end = skip_single_quoted(text, i, text.len(), b'"');
            if quoted_end <= lexical_start || quoted_end > text.len() {
                break;
            }
            let lexical_end = quoted_end.saturating_sub(1);
            let end = suffix_end(text, quoted_end);
            if text[quoted_end..end] == english_tag_suffix
                && let Ok(decoded) = turtle_unescape(&text[lexical_start..lexical_end])
            {
                out.push(LiteralCandidate {
                    start: i,
                    end,
                    style: QuoteStyle::Single,
                    decoded,
                });
            }
            i = quoted_end.max(i + 1);
            continue;
        }
        // By CODEPOINT: the two `text[i..].starts_with(…)` probes above slice at the
        // cursor, so a byte step through a non-ASCII character panics on the next pass.
        i = next_char_boundary(text, i);
    }
    out
}

fn replace_literal_in_text(
    text: &str,
    subject: &str,
    predicate: &str,
    old_value: &str,
    new_value: &str,
) -> Result<String> {
    let prefixes = extract_prefixes(text);
    let subject_forms = iri_text_forms(subject, &prefixes);
    let predicate_forms = iri_text_forms(predicate, &prefixes);
    let scoped: Vec<LiteralCandidate> = english_literal_candidates(text)
        .into_iter()
        .filter(|candidate| {
            let (found_subject, found_predicate) =
                extract_context(text, candidate.start, &subject_forms, &predicate_forms);
            found_subject
                .as_ref()
                .is_some_and(|s| subject_forms.iter().any(|form| form == s))
                && found_predicate
                    .as_ref()
                    .is_some_and(|p| predicate_forms.iter().any(|form| form == p))
        })
        .collect();

    if scoped.is_empty() {
        return Err(Diag::of_kind(CatalogInconsistent {
            detail: format!("no @x-gmeow-english literal for {subject}|{predicate}"),
        }));
    }
    if scoped.len() > 1 {
        return Err(Diag::of_kind(CatalogInconsistent {
            detail: format!(
                "conflict: ambiguous literal for {subject} {predicate}: {} occurrences in source text",
                scoped.len()
            ),
        }));
    }
    let candidate = &scoped[0];
    if candidate.decoded != old_value {
        return Err(Diag::of_kind(CatalogInconsistent {
            detail: format!(
                "conflict: literal for {subject} {predicate} is {:?}, expected {:?}",
                candidate.decoded, old_value
            ),
        }));
    }
    if candidate.decoded == new_value {
        return Ok(text.to_owned());
    }
    let replacement = match candidate.style {
        QuoteStyle::Single => format!("\"{}\"@{ENGLISH_TAG}", turtle_escape_single(new_value)),
        QuoteStyle::Triple => format!(
            "\"\"\"{}\"\"\"@{ENGLISH_TAG}",
            turtle_escape_triple(new_value)
        ),
    };
    let mut out = String::with_capacity(text.len() + replacement.len());
    out.push_str(&text[..candidate.start]);
    out.push_str(&replacement);
    out.push_str(&text[candidate.end..]);
    Ok(out)
}

fn current_english_values_from_text(
    text: &str,
) -> Result<BTreeMap<(String, String), BTreeSet<String>>> {
    let rows = parse_rdf_literals(text.as_bytes(), "turtle")?;
    let mut values: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for row in rows {
        if row.language.as_deref() == Some(ENGLISH_TAG) {
            values
                .entry((row.subject, row.predicate))
                .or_default()
                .insert(row.lexical);
        }
    }
    Ok(values)
}

fn sync_turtle(
    _po_path: &Path,
    source_path: &Path,
    po_text: &str,
    source_text: &str,
    dry_run: bool,
) -> Result<SyncReport> {
    let mut report = SyncReport::default();
    let mut text = source_text.to_owned();
    let mut current = current_english_values_from_text(source_text)?;
    for entry in parse_po(po_text, true)? {
        let Some((subject, predicate)) = entry.msgctxt.split_once('|') else {
            report
                .skipped
                .push(format!("malformed identity {:?}", entry.msgctxt));
            continue;
        };
        let predicate = expand_predicate(predicate);
        let key = (subject.to_owned(), predicate.clone());
        let current_values = current.get(&key).cloned().unwrap_or_default();
        if current_values.is_empty() {
            report
                .skipped
                .push(format!("no @x-gmeow-english literal for {}", entry.msgctxt));
            continue;
        }
        if current_values.len() > 1 {
            report.conflicts.push(format!(
                "conflict: multiple distinct @x-gmeow-english literals for {}",
                entry.msgctxt
            ));
            continue;
        }
        let current_value = current_values.into_iter().next().unwrap();
        if entry.msgid == current_value && entry.msgid == entry.msgstr {
            report.unchanged.push(entry.msgctxt);
            continue;
        }
        if entry.msgid == current_value && entry.msgid != entry.msgstr {
            match replace_literal_in_text(&text, subject, &predicate, &entry.msgid, &entry.msgstr) {
                Ok(updated) => {
                    text = updated;
                    current
                        .entry(key)
                        .and_modify(|values| {
                            values.clear();
                            values.insert(entry.msgstr.clone());
                        })
                        .or_insert_with(|| BTreeSet::from([entry.msgstr.clone()]));
                }
                Err(reason) if reason.message().starts_with("conflict:") => {
                    report.conflicts.push(reason.to_string())
                }
                Err(reason) => report.skipped.push(reason.to_string()),
            }
        } else if entry.msgid != current_value && entry.msgid == entry.msgstr {
            report.skipped.push(format!(
                "source changed, PO unchanged for {}",
                entry.msgctxt
            ));
        } else if current_value == entry.msgstr {
            report.unchanged.push(entry.msgctxt);
        } else {
            report.conflicts.push(format!(
                "conflict: source and PO both changed for {}",
                entry.msgctxt
            ));
        }
    }
    if text != source_text {
        report.changed_files.push(source_path.to_path_buf());
        if !dry_run {
            fs::write(source_path, text)?;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_po_reads_fuzzy_and_language() {
        let text = "msgid \"\"\nmsgstr \"\"\n\"Language: fr\\n\"\n\n#, fuzzy\nmsgctxt \"x|rdfs:label\"\nmsgid \"A\"\nmsgstr \"B\"\n";
        assert_eq!(language_from_po(text).unwrap(), Some("fr".to_owned()));
        let entries = parse_po(text, true).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].fuzzy);
        assert_eq!(entries[0].msgstr, "B");
    }

    #[test]
    fn live_translation_target_gates_fuzzy_seeds() {
        // The shared fuzzy-gating policy consumed by both pipeline corpus builders: a
        // reviewed entry contributes its msgstr; a machine-seeded `#, fuzzy` entry
        // contributes NO live target (English fallback), byte-identical to untranslated.
        let reviewed = PoEntry {
            msgctxt: "x|rdfs:label".to_owned(),
            msgid: "A".to_owned(),
            msgstr: "B".to_owned(),
            fuzzy: false,
        };
        assert_eq!(live_translation_target(&reviewed), "B");
        let seeded = PoEntry {
            fuzzy: true,
            ..reviewed
        };
        assert_eq!(
            live_translation_target(&seeded),
            "",
            "a #, fuzzy seed contributes no live target to the shipped bundle"
        );
    }

    /// Non-ASCII authored Turtle does not crash the byte-walking scanners.
    ///
    /// Both scanners advance a byte cursor and then slice `text[i..]`. A byte step through
    /// a multi-byte codepoint leaves the cursor INSIDE it, and the next slice panics with
    /// `byte index N is not a char boundary`. Every source below carries a character that
    /// reproduced exactly that — a `é`, an em dash, and CJK — in the positions the
    /// scanners walk: inside a literal, between statements, and inside a comment.
    #[test]
    fn the_turtle_scanners_survive_non_ascii_sources() {
        const SOURCES: [&str; 6] = [
            // Non-ASCII OUTSIDE any literal or comment — here inside an IRI, which the
            // cursor walks one step at a time between its quote probes. This is the source
            // that reaches the scanners' own fall-through step.
            "@prefix ex: <http://\u{4f8b}/> .\nex:a rdfs:label \"plain\"@x-gmeow-english .\n",
            // Non-ASCII in a BARE token: the tokenizer's name predicates are ASCII-only, so
            // the prefixed-name reader stops at the accent and the cursor falls through on
            // the character itself — the tokenizer's own fall-through step.
            "@prefix ex: <http://ex/> .\nex:na\u{ef}ve a ex:Thing .\n\
             ex:a rdfs:label \"plain\"@x-gmeow-english .\n",
            // Non-ASCII inside a single-quoted English literal.
            "@prefix ex: <http://ex/> .\nex:a rdfs:label \"café — 日本語\"@x-gmeow-english .\n",
            // Non-ASCII inside a triple-quoted literal.
            "@prefix ex: <http://ex/> .\nex:a skos:definition \"\"\"Ünicode — ok\"\"\"@x-gmeow-english .\n",
            // Non-ASCII OUTSIDE any literal: in a comment, which the cursor walks byte by
            // byte before it ever reaches a quote.
            "# a comment with é and — in it\n@prefix ex: <http://ex/> .\nex:a rdfs:label \"plain\"@x-gmeow-english .\n",
            // A backslash-escaped non-ASCII character: the escape skip must step over one
            // whole codepoint, not one byte.
            "@prefix ex: <http://ex/> .\nex:a rdfs:label \"esc \\é done\"@x-gmeow-english .\n",
        ];
        for source in SOURCES {
            // The candidate scanner…
            let candidates = english_literal_candidates(source);
            for candidate in &candidates {
                // Every recorded span must be a valid slice of the source, or the caller
                // (`replace_literal_in_text`) panics on the rewrite instead of the scan.
                assert!(
                    source.is_char_boundary(candidate.start)
                        && source.is_char_boundary(candidate.end),
                    "candidate span {}..{} is not on codepoint boundaries of {source:?}",
                    candidate.start,
                    candidate.end
                );
                let _ = &source[candidate.start..candidate.end];
            }
            // …and the tokenizer, which walks the same cursor.
            let _ = tokenize_turtle(source, source.len());
        }

        // Non-vacuity: the scanner really does FIND the non-ASCII English literals, so a
        // scanner that silently returned nothing could not pass this test.
        let found = english_literal_candidates(SOURCES[2]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].decoded, "café — 日本語");
    }

    #[test]
    fn markdown_extract_uses_stable_hash() {
        let entries = extract_markdown_text("# Title\n\nBody.", "README.md");
        assert_eq!(entries.len(), 2);
        assert!(entries[0].msgctxt.starts_with("README.md|"));
        assert_eq!(entries[0].msgid, "# Title");
    }

    #[test]
    fn csv_escape_quotes_commas() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
    }

    #[test]
    fn turtle_replace_disambiguates_by_subject_and_predicate() {
        let source = r#"@prefix ex: <http://example.org/> .

ex:s ex:p "shared"@x-gmeow-english ;
    ex:q "shared"@x-gmeow-english .
"#;
        let updated = replace_literal_in_text(
            source,
            "http://example.org/s",
            "http://example.org/p",
            "shared",
            "changed",
        )
        .unwrap();
        assert!(updated.contains(r#"ex:p "changed"@x-gmeow-english"#));
        assert!(updated.contains(r#"ex:q "shared"@x-gmeow-english"#));
    }

    #[test]
    fn turtle_replace_preserves_triple_quoted_style() {
        let source = r#"@prefix ex: <http://example.org/> .

ex:s ex:p """old value"""@x-gmeow-english .
"#;
        let updated = replace_literal_in_text(
            source,
            "http://example.org/s",
            "http://example.org/p",
            "old value",
            "new value",
        )
        .unwrap();
        assert!(updated.contains(r#""""new value"""@x-gmeow-english"#));
    }

    #[test]
    fn turtle_sync_reports_source_already_at_po_value_as_unchanged() {
        let source = r#"@prefix ex: <http://example.org/> .

ex:s ex:p "hand-edited value"@x-gmeow-english .
"#;
        let po = r#"msgctxt "http://example.org/s|http://example.org/p"
msgid "old value"
msgstr "hand-edited value"
"#;
        let report = sync_turtle(
            Path::new("test.po"),
            Path::new("module.ttl"),
            po,
            source,
            true,
        )
        .unwrap();
        assert!(report.changed_files.is_empty());
        assert!(report.conflicts.is_empty());
        assert!(report.skipped.is_empty());
        assert_eq!(
            report.unchanged,
            vec!["http://example.org/s|http://example.org/p"]
        );
    }

    #[test]
    fn turtle_sync_conflicts_when_source_and_po_both_changed() {
        let source = r#"@prefix ex: <http://example.org/> .

ex:s ex:p "current value"@x-gmeow-english .
"#;
        let po = r#"msgctxt "http://example.org/s|http://example.org/p"
msgid "old value"
msgstr "proposed value"
"#;
        let report = sync_turtle(
            Path::new("test.po"),
            Path::new("module.ttl"),
            po,
            source,
            true,
        )
        .unwrap();
        assert!(report.changed_files.is_empty());
        assert_eq!(report.conflicts.len(), 1);
        assert!(report.conflicts[0].contains("source and PO both changed"));
    }

    /// A fresh, empty repo root for one test, owned by the returned
    /// [`tempfile::TempDir`] so the tree is removed when that guard drops — on
    /// success, on panic, and on early return. Uniqueness comes from the guard;
    /// `name` is only a readable label for the root inside it. Callers must bind
    /// the guard (`let (_tmp, root) = test_root("…");`); a bare `_` binding drops
    /// it at once and deletes the root out from under the test.
    fn test_root(name: &str) -> (tempfile::TempDir, PathBuf) {
        let guard = tempfile::tempdir().expect("create temp dir");
        let root = guard.path().join(name);
        fs::create_dir_all(&root).unwrap();
        (guard, root)
    }

    fn write_minimal_ontology(root: &Path) {
        let ontology = root.join("ontology/gmeow.ttl");
        fs::create_dir_all(ontology.parent().unwrap()).unwrap();
        fs::write(
            ontology,
            r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

gmeow:eventTypeAdoption rdfs:label "adoption"@x-gmeow-english .
gmeow:chainId rdfs:label "chain id"@x-gmeow-english .
gmeow:placeTypeCity rdfs:label "city"@x-gmeow-english .
"#,
        )
        .unwrap();
    }

    fn write_test_po(root: &Path, name: &str, body: &str) {
        let path = root.join("slices/core/test/i18n").join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn po_body(entries: &[(&str, &str, &str, bool)]) -> String {
        let mut lines = vec![
            "msgid \"\"".to_owned(),
            "msgstr \"\"".to_owned(),
            "\"Language: fr\\n\"".to_owned(),
            "\"MIME-Version: 1.0\\n\"".to_owned(),
            "\"Content-Type: text/plain; charset=UTF-8\\n\"".to_owned(),
            "\"Content-Transfer-Encoding: 8bit\\n\"".to_owned(),
            String::new(),
        ];
        for (ctx, msgid, msgstr, fuzzy) in entries {
            if *fuzzy {
                lines.push("#, fuzzy".to_owned());
            }
            lines.push(format!("msgctxt \"{ctx}\""));
            lines.push(format!("msgid \"{msgid}\""));
            lines.push(format!("msgstr \"{msgstr}\""));
            lines.push(String::new());
        }
        lines.join("\n")
    }

    #[test]
    fn xliff_export_uses_actual_slice_path() {
        let (_tmp, root) = test_root("xliff-slice-path");
        let po_path = root.join("slices/extensions/example/i18n/fr.po");
        fs::create_dir_all(po_path.parent().unwrap()).unwrap();
        fs::write(
            &po_path,
            po_body(&[(
                "https://blackcatinformatics.ca/gmeow/exampleTerm|rdfs:label",
                "example label",
                "example label translated",
                false,
            )]),
        )
        .unwrap();

        let text = export_xliff(&root, None).unwrap();
        assert!(text.contains("original=\"slices/extensions/example\""));
        assert!(!text.contains("original=\"slices/core/example\""));
        // A non-fuzzy entry is emitted as an XLIFF `translated` target state.
        assert!(text.contains("<target state=\"translated\">example label translated</target>"));
    }

    #[test]
    fn lint_valid_catalog_reports_no_errors() {
        let (_tmp, root) = test_root("lint-valid");
        write_minimal_ontology(&root);
        write_test_po(
            &root,
            "valid_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
                    "adoption",
                    "adoption",
                    false,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                    "chain id",
                    "identifiant de chaine",
                    false,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(report.total_counts.get("x-gmeow-french"), Some(&2));
        assert_eq!(report.fuzzy_counts.get("x-gmeow-french"), Some(&0));
    }

    #[test]
    fn lint_flags_collapsed_translation_distinction() {
        // Two DISTINCT English sources translated to the SAME target — the translation
        // collapsed a distinction the source made. A hard reject.
        let (_tmp, root) = test_root("lint-collapsed");
        write_minimal_ontology(&root);
        write_test_po(
            &root,
            "collapsed_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
                    "adoption",
                    "pareil",
                    false,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                    "chain id",
                    "pareil",
                    false,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        let collisions: Vec<&String> = report
            .errors
            .iter()
            .filter(|e| e.contains("collides across"))
            .collect();
        assert_eq!(
            collisions.len(),
            1,
            "one distinctiveness error: {:?}",
            report.errors
        );
        assert!(
            collisions[0].contains("pareil") && collisions[0].contains("distinct sources"),
            "names the shared target: {collisions:?}"
        );
    }

    #[test]
    fn lint_passes_twin_source_shared_translation() {
        // A class and its property twin share ONE English label, so sharing ONE target
        // translation is legitimate (identical msgid skeleton) and must NOT be flagged.
        let (_tmp, root) = test_root("lint-twin");
        write_minimal_ontology(&root);
        write_test_po(
            &root,
            "twin_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/PValue|rdfs:label",
                    "p-value",
                    "valeur p",
                    false,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/pValue|rdfs:label",
                    "p-value",
                    "valeur p",
                    false,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        assert!(
            !report.errors.iter().any(|e| e.contains("collides across")),
            "twin sources sharing one translation must not red: {:?}",
            report.errors
        );
    }

    /// An ontology whose terms carry the English labels the glossary-consistency tests
    /// render ("read" twice as a homograph pair, "play" once), so entries do not orphan.
    fn write_glossary_ontology(root: &Path) {
        let ontology = root.join("ontology/gmeow.ttl");
        fs::create_dir_all(ontology.parent().unwrap()).unwrap();
        fs::write(
            ontology,
            r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .

gmeow:readPresent rdfs:label "read"@x-gmeow-english .
gmeow:readPast rdfs:label "read"@x-gmeow-english .
gmeow:playMedia rdfs:label "play"@x-gmeow-english .
"#,
        )
        .unwrap();
    }

    /// Author a `lang:DeclaredTerminologyHomograph` per source into a slice `module.ttl`
    /// (which `authored_turtle_files` scans), so the real `lint_po_files` loader exempts them.
    fn write_declared_homographs(root: &Path, sources: &[&str]) {
        let path = root.join("slices/core/test/module.ttl");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut ttl = String::from(
            "@prefix lang: <https://blackcatinformatics.ca/lang/> .\n@prefix ex: <http://example.org/h/> .\n\n",
        );
        for (i, s) in sources.iter().enumerate() {
            ttl.push_str(&format!(
                "ex:hg{i} a lang:DeclaredTerminologyHomograph ; lang:homographSource \"{s}\" ; lang:homographConcept ex:c{i}a , ex:c{i}b .\n"
            ));
        }
        fs::write(path, ttl).unwrap();
    }

    #[test]
    fn lint_flags_glossary_inconsistency() {
        // One English source ("read") translated two different ways ("lire" / "lu") across
        // batches — the cross-batch terminology-consistency violation (the dual of the
        // distinctiveness collapse). A hard reject via lang:GlossaryTermInconsistency.
        let (_tmp, root) = test_root("lint-glossary-inconsistent");
        write_glossary_ontology(&root);
        write_test_po(
            &root,
            "glossary_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/readPresent|rdfs:label",
                    "read",
                    "lire",
                    false,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/readPast|rdfs:label",
                    "read",
                    "lu",
                    false,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        let hits: Vec<&String> = report
            .errors
            .iter()
            .filter(|e| e.contains("different ways across batches"))
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "one glossary-consistency error: {:?}",
            report.errors
        );
        assert!(
            hits[0].contains("\"read\"") && hits[0].contains("lang:GlossaryTermInconsistency"),
            "names the source and the failure class: {hits:?}"
        );
    }

    #[test]
    fn lint_passes_consistent_glossary() {
        // One English source rendered ONE consistent way across batches — no violation.
        let (_tmp, root) = test_root("lint-glossary-consistent");
        write_glossary_ontology(&root);
        write_test_po(
            &root,
            "glossary_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/readPresent|rdfs:label",
                    "read",
                    "lire",
                    false,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/readPast|rdfs:label",
                    "read",
                    "lire",
                    false,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.contains("different ways across batches")),
            "a consistent glossary must not red: {:?}",
            report.errors
        );
    }

    #[test]
    fn lint_passes_declared_homograph() {
        // "read" is DECLARED a homograph, so its two senses may render differently
        // ("lire" / "lu") without a consistency violation. The real lint_po_files loader
        // reads the declaration from authored TTL (write_declared_homographs), not a
        // hand-built set — the production module.ttl -> gate flow.
        let (_tmp, root) = test_root("lint-glossary-homograph");
        write_glossary_ontology(&root);
        write_declared_homographs(&root, &["read"]);
        write_test_po(
            &root,
            "glossary_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/readPresent|rdfs:label",
                    "read",
                    "lire",
                    false,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/readPast|rdfs:label",
                    "read",
                    "lu",
                    false,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.contains("different ways across batches")),
            "a declared homograph must be exempt: {:?}",
            report.errors
        );
    }

    #[test]
    fn lint_flags_inconsistency_despite_unrelated_homograph() {
        // Guardrail: an UNRELATED declared homograph ("play") must not widen the exempt set
        // and mask a genuine "read" inconsistency — the exempt-set read is source-keyed and
        // authored-TTL-only, so only the exact declared source is exempted.
        let (_tmp, root) = test_root("lint-glossary-unrelated-homograph");
        write_glossary_ontology(&root);
        write_declared_homographs(&root, &["play"]);
        write_test_po(
            &root,
            "glossary_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/readPresent|rdfs:label",
                    "read",
                    "lire",
                    false,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/readPast|rdfs:label",
                    "read",
                    "lu",
                    false,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("different ways across batches") && e.contains("\"read\"")),
            "an unrelated homograph must not mask the read inconsistency: {:?}",
            report.errors
        );
    }

    #[test]
    fn lint_excludes_fuzzy_from_distinctiveness() {
        // Fuzzy entries are not candidate translations, so a fuzzy collapsed pair is not
        // a distinctiveness violation (consistent with the rest of the lint's exclusions).
        let (_tmp, root) = test_root("lint-fuzzy-excluded");
        write_minimal_ontology(&root);
        write_test_po(
            &root,
            "fuzzy_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
                    "adoption",
                    "pareil",
                    true,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                    "chain id",
                    "pareil",
                    true,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        assert!(
            !report.errors.iter().any(|e| e.contains("collides across")),
            "fuzzy entries are excluded from the distinctiveness check: {:?}",
            report.errors
        );
    }

    #[test]
    fn lint_reports_orphaned_and_stale_entries_as_warnings() {
        let (_tmp, root) = test_root("lint-stale");
        write_minimal_ontology(&root);
        write_test_po(
            &root,
            "stale_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/NonExistentLintTerm|rdfs:label",
                    "missing term",
                    "terme manquant",
                    false,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/placeTypeCity|rdfs:label",
                    "old city label",
                    "ancienne etiquette",
                    false,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.warnings.len(), 2);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("orphaned"))
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("stale"))
        );
    }

    #[test]
    fn lint_rejects_copied_or_hybrid_english_as_translation() {
        let (_tmp, root) = test_root("lint-english-leak");
        write_minimal_ontology(&root);
        write_test_po(
            &root,
            "leaked_fr.po",
            &po_body(&[(
                "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                "chain id",
                "chain id",
                false,
            )]),
        );
        let report = lint_po_files(&root, 100.0);
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert!(report.errors[0].contains("copied into msgstr"));
    }

    #[test]
    fn lint_all_fuzzy_catalog_is_error() {
        let (_tmp, root) = test_root("lint-fuzzy");
        write_test_po(
            &root,
            "fuzzy_fr.po",
            &po_body(&[
                (
                    "https://blackcatinformatics.ca/gmeow/eventTypeAdoption|rdfs:label",
                    "adoption",
                    "adoption",
                    true,
                ),
                (
                    "https://blackcatinformatics.ca/gmeow/chainId|rdfs:label",
                    "chain id",
                    "identifiant de chaine",
                    true,
                ),
            ]),
        );
        let report = lint_po_files(&root, 100.0);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("x-gmeow-french has only fuzzy entries"));
    }

    #[test]
    fn lint_missing_language_header_is_error() {
        let (_tmp, root) = test_root("lint-no-lang");
        write_test_po(
            &root,
            "no_lang.po",
            "msgid \"\"\nmsgstr \"\"\n\nmsgctxt \"x|rdfs:label\"\nmsgid \"x\"\nmsgstr \"y\"\n",
        );
        let report = lint_po_files(&root, 100.0);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("missing Language header"));
    }
}
