// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `okf` export leaf: Open Knowledge Format projection (`dist/`, gitignored).
//!
//! Projects the folded GMEOW term surface into a conformant OKF bundle under
//! `dist/gmeow-okf/` — one Markdown document per concept (YAML frontmatter +
//! `[text](path)` body links), per-category indexes, and a root `index.md` that
//! carries the in-band lossy declaration.
//!
//! The bundle structure is GMEOW-specific (Class/Property/Individual docs with the
//! six recognized OKF frontmatter keys plus `okf:<key>` extensions). The generator
//! builds this layout itself — it does NOT call the `gts to-okf` codec (which
//! projects an already-OKF-profile graph). So this is a direct structural
//! projection, not a codec call. Output is the git-ignored `dist/` tree, so the
//! bar is structural validity + determinism: terms arrive sorted, keys are
//! fixed-then-sorted, and the YAML emitter is a total function over any term
//! value (no wall-clock content enters the bytes).

use std::collections::BTreeMap;

use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct};
use crate::stages::export::{Term, collect_term_surface, read_fold_upstream};

/// The bundle directory name under `dist/`.
pub const OKF_DIR_NAME: &str = "gmeow-okf";

const LOSSY_NOTE: &str = "> LOSSY projection: the flat GMEOW term surface (label, definition, advisories, and IS-A / domain / range / sub-property links). The logic axioms, the RDF 1.2 statement/reification layer, and the full alignment graph are dropped — the grounding-slice sources carried by GTS are canonical.";

fn category_type(category: &str) -> &'static str {
    match category {
        "class" => "Class",
        "property" => "Property",
        "individual" => "Individual",
        _ => "Thing",
    }
}

fn category_dir(category: &str) -> &'static str {
    match category {
        "class" => "classes",
        "property" => "properties",
        "individual" => "individuals",
        _ => "things",
    }
}

/// The document stem for a term — its CURIE local part.
fn slug(term_curie: &str) -> String {
    term_curie
        .split_once(':')
        .map(|(_, l)| l)
        .unwrap_or(term_curie)
        .to_string()
}

/// The bundle-relative POSIX path of a term's document (`classes/Foo.md`).
pub(crate) fn doc_relpath(term: &Term) -> String {
    format!("{}/{}.md", category_dir(term.category), slug(&term.curie))
}

/// A POSIX relative link from one bundle document to another.
fn relative_link(from_path: &str, to_path: &str) -> String {
    let base_parts: Vec<&str> = {
        let parent = from_path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        if parent.is_empty() {
            Vec::new()
        } else {
            parent.split('/').collect()
        }
    };
    let target_parts: Vec<&str> = to_path.split('/').collect();
    let mut common = 0;
    for (a, b) in base_parts.iter().zip(
        target_parts
            .iter()
            .take(target_parts.len().saturating_sub(1)),
    ) {
        if a != b {
            break;
        }
        common += 1;
    }
    let ups: Vec<String> =
        std::iter::repeat_n("..".to_string(), base_parts.len() - common).collect();
    let downs: Vec<String> = target_parts[common..]
        .iter()
        .map(|s| s.to_string())
        .collect();
    if ups.is_empty() && downs.is_empty() {
        target_parts.last().unwrap_or(&"").to_string()
    } else {
        let mut parts = ups;
        parts.extend(downs);
        parts.join("/")
    }
}

// ── YAML frontmatter value model (scalar / list / bool) ────────────────────────

enum Yaml {
    Str(String),
    Bool(bool),
    List(Vec<String>),
}

/// Serialize a mapping of scalar/bool/list values as block YAML: keys emit in
/// insertion order (not sorted), one entry per line, Unicode preserved verbatim,
/// and every scalar routed through `yaml_scalar` so it round-trips losslessly.
fn yaml_dump(entries: &[(String, Yaml)]) -> String {
    let mut out = String::new();
    for (key, value) in entries {
        match value {
            Yaml::Str(s) => {
                out.push_str(&yaml_key(key));
                out.push_str(": ");
                out.push_str(&yaml_scalar(s));
                out.push('\n');
            }
            Yaml::Bool(b) => {
                out.push_str(&yaml_key(key));
                out.push_str(": ");
                out.push_str(if *b { "true" } else { "false" });
                out.push('\n');
            }
            Yaml::List(items) => {
                out.push_str(&yaml_key(key));
                out.push_str(":\n");
                for item in items {
                    out.push_str("- ");
                    out.push_str(&yaml_scalar(item));
                    out.push('\n');
                }
            }
        }
    }
    out
}

/// A YAML mapping key (always a plain identifier in this generator).
fn yaml_key(key: &str) -> String {
    yaml_scalar(key)
}

/// Emit a YAML scalar that round-trips losslessly through any YAML 1.1 reader.
///
/// Three resolutions, in order of precedence:
///   1. a double-quoted scalar with escapes when the value carries a newline,
///      tab, or any other control character (the quote style a YAML emitter must
///      use when escapes are required) — so a multi-line definition is encoded,
///      not refused;
///   2. a single-quoted scalar when the value is escape-free but plain-unsafe
///      (reserved words, number- or sexagesimal-shaped tokens, indicator-led or
///      structurally ambiguous strings);
///   3. otherwise the bare plain scalar.
///
/// This is a total function: every possible term value has a correct encoding.
fn yaml_scalar(s: &str) -> String {
    if needs_double_quote(s) {
        double_quoted(s)
    } else if needs_quoting(s) {
        // Single-quote style: double internal single quotes.
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

/// Whether a scalar carries a character that neither a plain nor a single-quoted
/// scalar can represent without escaping (newline, carriage return, tab, or any
/// other control character).
fn needs_double_quote(s: &str) -> bool {
    s.chars().any(char::is_control)
}

/// A double-quoted YAML scalar with the minimal escape set: backslash and double
/// quote, the `\n` / `\r` / `\t` shortcuts, and `\xXX` / `\uXXXX` for any other
/// control character.
fn double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let code = c as u32;
                if code <= 0xff {
                    out.push_str(&format!("\\x{code:02x}"));
                } else {
                    out.push_str(&format!("\\u{code:04x}"));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Whether a control-free string must be single-quoted to round-trip as a YAML
/// plain scalar (reserved resolutions, number- or sexagesimal-shaped tokens,
/// indicator-led or structurally ambiguous strings, leading/trailing space).
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Reserved plain-scalar resolutions.
    let lower = s.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "null" | "~" | "true" | "false" | "yes" | "no" | "on" | "off" | "y" | "n"
    ) {
        return true;
    }
    // Number-, date-, or base-60-shaped tokens resolve to a non-string when left
    // plain in a YAML 1.1 reader, so they must be quoted to survive as strings.
    if resolves_to_yaml_number(s) || looks_like_yaml_timestamp(s) || looks_like_sexagesimal(s) {
        return true;
    }
    let first = s.chars().next().unwrap();
    if "!&*[]{},#|>@`\"'%-?:".contains(first) {
        return true;
    }
    // Indicators / structural chars that break a plain scalar mid-string or edge.
    s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || s.starts_with(' ')
        || s.ends_with(' ')
}

/// Whether a control-free string is read as a YAML 1.1 number (integer, float, or
/// special float) rather than a string. This models the *reader's* grammar, which
/// is wider than Rust's `f64`: it also resolves radix integers (`0x` / `0o` / `0b`,
/// digit-grouping underscores allowed), underscore digit groups (`1_000`), and the
/// `.inf` / `.nan` special-float spellings — all of which `f64::parse` rejects, so
/// a bare emission would round-trip back as a different type.
fn resolves_to_yaml_number(s: &str) -> bool {
    // Special floats: `.inf`, `+.inf`, `-.inf`, `.nan` (any case).
    if matches!(
        s.to_ascii_lowercase().as_str(),
        ".inf" | "+.inf" | "-.inf" | ".nan"
    ) {
        return true;
    }
    // Canonical decimal integer / float, including exponent forms (this also covers
    // leading-zero decimals such as `017`, which still resolve to a number).
    if s.parse::<f64>().is_ok() {
        return true;
    }
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    // Radix integers: hex / octal / binary.
    if let Some(rest) = body.strip_prefix("0x") {
        return !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_hexdigit() || b == b'_');
    }
    if let Some(rest) = body.strip_prefix("0o") {
        return !rest.is_empty() && rest.bytes().all(|b| matches!(b, b'0'..=b'7' | b'_'));
    }
    if let Some(rest) = body.strip_prefix("0b") {
        return !rest.is_empty() && rest.bytes().all(|b| matches!(b, b'0' | b'1' | b'_'));
    }
    // Underscore digit groups (`1_000`, `3_000.5`). Identifiers such as `has_part`
    // strip to a non-number, so this only fires on genuine numerics.
    s.contains('_') && s.replace('_', "").parse::<f64>().is_ok()
}

/// Whether a string opens with a YAML 1.1 date or timestamp (`YYYY-M-D`, then
/// either end-of-string or a `T` / `t` / space time separator). A reader resolves
/// such a plain scalar to a timestamp, so the emitter must quote it.
fn looks_like_yaml_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    // `YYYY-`
    if b.len() < 8 || !b[..4].iter().all(u8::is_ascii_digit) || b[4] != b'-' {
        return false;
    }
    let mut i = 5;
    let month_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if !(1..=2).contains(&(i - month_start)) || i >= b.len() || b[i] != b'-' {
        return false;
    }
    i += 1;
    let day_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if !(1..=2).contains(&(i - day_start)) {
        return false;
    }
    i == b.len() || matches!(b[i], b'T' | b't' | b' ' | b'\t')
}

/// Whether a string matches the YAML 1.1 base-60 (sexagesimal) int/float grammar
/// — `[-+]?[0-9][0-9_]*(:[0-5]?[0-9])+(\.[0-9_]*)?`, e.g. `12:30`, `1:2:3`,
/// `12:30.5`. A YAML 1.1 reader folds such a plain scalar to a number, so the
/// emitter must quote it to preserve the string.
fn looks_like_sexagesimal(s: &str) -> bool {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    // An optional fractional tail attaches to the final field.
    let digits = match body.split_once('.') {
        Some((d, frac)) if frac.chars().all(|c| c.is_ascii_digit() || c == '_') => d,
        Some(_) => return false,
        None => body,
    };
    let mut fields = digits.split(':');
    // First field: a base-10 digit lead, then digits (underscores allowed in
    // YAML 1.1). A leading underscore (`_12:30`) is NOT a valid sexagesimal token.
    match fields.next() {
        Some(h)
            if h.starts_with(|c: char| c.is_ascii_digit())
                && h.chars().all(|c| c.is_ascii_digit() || c == '_') => {}
        _ => return false,
    }
    // At least one `:field`, each a base-60 digit `[0-5]?[0-9]`.
    let mut had_colon = false;
    for f in fields {
        had_colon = true;
        let bytes = f.as_bytes();
        let ok = match bytes.len() {
            1 => bytes[0].is_ascii_digit(),
            2 => (b'0'..=b'5').contains(&bytes[0]) && bytes[1].is_ascii_digit(),
            _ => false,
        };
        if !ok {
            return false;
        }
    }
    had_colon
}

// ── frontmatter + body ─────────────────────────────────────────────────────────

fn frontmatter(term: &Term, version: &str) -> Vec<(String, Yaml)> {
    let mut fm: Vec<(String, Yaml)> = vec![(
        "type".into(),
        Yaml::Str(category_type(term.category).into()),
    )];
    if !term.label.is_empty() {
        fm.push(("title".into(), Yaml::Str(term.label.clone())));
    }
    if !term.definition.is_empty() {
        fm.push(("description".into(), Yaml::Str(term.definition.clone())));
    }
    fm.push(("resource".into(), Yaml::Str(term.iri.clone())));
    if !term.box_roles.is_empty() {
        let mut tags = term.box_roles.clone();
        tags.sort();
        fm.push(("tags".into(), Yaml::List(tags)));
    }
    fm.push(("version".into(), Yaml::Str(version.to_string())));
    fm.push(("curie".into(), Yaml::Str(term.curie.clone())));

    // Category-specific + shared advisory extensions, sorted by key.
    let mut extension: BTreeMap<String, Yaml> = BTreeMap::new();
    match term.category {
        "class" => {
            if !term.parents.is_empty() {
                extension.insert("parents".into(), Yaml::List(term.parents.clone()));
            }
        }
        "property" => {
            if !term.prop_kind.is_empty() {
                extension.insert("prop_kind".into(), Yaml::Str(term.prop_kind.to_string()));
            }
            if !term.domain.is_empty() {
                extension.insert("domain".into(), Yaml::Str(term.domain.clone()));
            }
            if !term.range.is_empty() {
                extension.insert("range".into(), Yaml::Str(term.range.clone()));
            }
            if term.functional {
                extension.insert("functional".into(), Yaml::Bool(true));
            }
            if !term.sub_property_of.is_empty() {
                extension.insert(
                    "sub_property_of".into(),
                    Yaml::List(term.sub_property_of.clone()),
                );
            }
        }
        "individual" if !term.types.is_empty() => {
            extension.insert("types".into(), Yaml::List(term.types.clone()));
        }
        _ => {}
    }
    for (key, value) in [
        ("alignments", &term.alignments),
        ("scope_notes", &term.scope_notes),
        ("examples", &term.examples),
        ("use_when", &term.use_when),
        ("avoid_when", &term.avoid_when),
        ("how_to_use", &term.how_to_use),
        ("use_for_consumer", &term.use_for_consumer),
        ("avoid_for_consumer", &term.avoid_for_consumer),
    ] {
        if !value.is_empty() {
            extension.insert(key.into(), Yaml::List(value.clone()));
        }
    }
    // BTreeMap drains in sorted key order, so extension keys emit deterministically.
    for (key, value) in extension {
        fm.push((key, value));
    }
    fm
}

/// In-bundle relation targets (relation, target term) where the target is a
/// document in the bundle.
fn link_targets<'a>(
    term: &Term,
    by_curie: &'a BTreeMap<String, Term>,
) -> Vec<(&'static str, &'a Term)> {
    let mut refs: Vec<(&'static str, &str)> = Vec::new();
    match term.category {
        "class" => {
            for p in &term.parents {
                refs.push(("subClassOf", p));
            }
        }
        "property" => {
            if !term.domain.is_empty() {
                refs.push(("domain", &term.domain));
            }
            if !term.range.is_empty() {
                refs.push(("range", &term.range));
            }
            for p in &term.sub_property_of {
                refs.push(("subPropertyOf", p));
            }
        }
        "individual" => {
            for t in &term.types {
                refs.push(("type", t));
            }
        }
        _ => {}
    }
    let mut out: Vec<(&'static str, &'a Term)> = Vec::new();
    for (relation, reference) in refs {
        if let Some(target) = by_curie.get(reference) {
            out.push((relation, target));
        }
    }
    out
}

fn body(term: &Term, by_curie: &BTreeMap<String, Term>) -> String {
    let mut lines: Vec<String> = Vec::new();
    if !term.definition.is_empty() {
        lines.push(term.definition.clone());
        lines.push(String::new());
    }

    let section = |heading: &str, items: &[String], lines: &mut Vec<String>| {
        if items.is_empty() {
            return;
        }
        lines.push(format!("## {heading}"));
        lines.push(String::new());
        for item in items {
            lines.push(format!("- {item}"));
        }
        lines.push(String::new());
    };
    section("Scope notes", &term.scope_notes, &mut lines);
    section("Use when", &term.use_when, &mut lines);
    section("Avoid when", &term.avoid_when, &mut lines);
    section("How to use", &term.how_to_use, &mut lines);
    section("Examples", &term.examples, &mut lines);

    let links = link_targets(term, by_curie);
    if !links.is_empty() {
        lines.push("## Relations".into());
        lines.push(String::new());
        let from_path = doc_relpath(term);
        for (relation, target) in links {
            let rel_path = relative_link(&from_path, &doc_relpath(target));
            let label = if target.label.is_empty() {
                &target.curie
            } else {
                &target.label
            };
            lines.push(format!("- {relation}: [{label}]({rel_path})"));
        }
        lines.push(String::new());
    }
    lines.join("\n").trim_end_matches('\n').to_string() + "\n"
}

fn render_doc(frontmatter: &[(String, Yaml)], body: &str) -> Result<String, gmeow_errors::Diag> {
    let fm = yaml_dump(frontmatter);
    Ok(format!("---\n{fm}---\n{body}"))
}

fn index_doc(
    title: &str,
    entries: &[(String, String)],
    lossy_note: &str,
) -> Result<String, gmeow_errors::Diag> {
    let fm = vec![
        ("type".to_string(), Yaml::Str("Index".into())),
        ("title".to_string(), Yaml::Str(title.to_string())),
    ];
    let mut lines: Vec<String> = Vec::new();
    if !lossy_note.is_empty() {
        lines.push(lossy_note.to_string());
        lines.push(String::new());
    }
    for (label, rel_path) in entries {
        lines.push(format!("- [{label}]({rel_path})"));
    }
    let body = lines.join("\n").trim_end_matches('\n').to_string() + "\n";
    render_doc(&fm, &body)
}

// ── bundle assembly ──────────────────────────────────────────────────────────────

/// Render the OKF bundle as logical-path → bytes, keyed under `dist/gmeow-okf/…`.
pub(crate) fn render_okf(
    title: &str,
    version: &str,
    terms: &[Term],
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    let by_curie: BTreeMap<String, Term> =
        terms.iter().map(|t| (t.curie.clone(), t.clone())).collect();

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let prefix = format!("dist/{OKF_DIR_NAME}");

    // Per-term documents + per-category membership lists (in term order).
    let mut by_category: BTreeMap<&str, Vec<&Term>> = BTreeMap::new();
    for category in ["class", "property", "individual"] {
        by_category.insert(category, Vec::new());
    }
    for term in terms {
        let rel = doc_relpath(term);
        let doc = render_doc(&frontmatter(term, version), &body(term, &by_curie))?;
        out.insert(format!("{prefix}/{rel}"), doc.into_bytes());
        by_category.entry(term.category).or_default().push(term);
    }

    // Per-directory indexes (links relative to the index — siblings).
    for category in ["class", "property", "individual"] {
        let members = &by_category[category];
        if members.is_empty() {
            continue;
        }
        let entries: Vec<(String, String)> = members
            .iter()
            .map(|m| {
                let label = if m.label.is_empty() {
                    m.curie.clone()
                } else {
                    m.label.clone()
                };
                (label, format!("{}.md", slug(&m.curie)))
            })
            .collect();
        let idx = index_doc(&format!("GMEOW {}", category_dir(category)), &entries, "")?;
        out.insert(
            format!("{prefix}/{}/index.md", category_dir(category)),
            idx.into_bytes(),
        );
    }

    // Root index — links to each non-empty category index, carrying the lossy note.
    let root_entries: Vec<(String, String)> = ["class", "property", "individual"]
        .into_iter()
        .filter(|c| !by_category[c].is_empty())
        .map(|c| {
            (
                format!("{title} — {}", category_dir(c)),
                format!("{}/index.md", category_dir(c)),
            )
        })
        .collect();
    out.insert(
        format!("{prefix}/index.md"),
        index_doc(&format!("{title} (OKF)"), &root_entries, LOSSY_NOTE)?.into_bytes(),
    );
    Ok(out)
}

// ── Stage impl ───────────────────────────────────────────────────────────────────

/// The `stage-export-okf` export-leaf stage.
pub struct OkfStage {
    consumes: Vec<String>,
}

impl OkfStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
        }
    }
}

impl Default for OkfStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for OkfStage {
    fn id(&self) -> &str {
        "stage-export-okf"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn cache_policy(&self) -> CachePolicy {
        // Measured contribution: 19.8 MB serialized for a ~3.6 s deterministic fold.
        // Rebuilding is cheaper than cache publication plus hydration.
        CachePolicy::Recompute
    }
    fn impl_version(&self) -> &str {
        "okf.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let graph = read_fold_upstream(input.upstream)?;
        let (title, version, terms) = collect_term_surface(graph.as_ref())?;
        let artifacts = render_okf(&title, &version, &terms)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            artifacts,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_scalar_plain_when_safe() {
        assert_eq!(yaml_scalar("Dog"), "Dog");
        // A colon NOT followed by a space is a legal plain scalar (CURIEs, IRIs).
        assert_eq!(yaml_scalar("gmeow:Dog"), "gmeow:Dog");
        assert_eq!(
            yaml_scalar("https://example.org/Dog"),
            "https://example.org/Dog"
        );
        // Multi-dot version strings are not numbers — stay plain.
        assert_eq!(yaml_scalar("1.0.0"), "1.0.0");
        // Unicode is emitted directly (allow_unicode).
        assert_eq!(yaml_scalar("café"), "café");
    }

    #[test]
    fn yaml_scalar_single_quotes_plain_unsafe() {
        assert_eq!(yaml_scalar(""), "''");
        assert_eq!(yaml_scalar("yes"), "'yes'");
        assert_eq!(yaml_scalar("Null"), "'Null'");
        assert_eq!(yaml_scalar("42"), "'42'");
        // Exponent / non-finite floats would resolve to numbers if left plain.
        assert_eq!(yaml_scalar("1e3"), "'1e3'");
        assert_eq!(yaml_scalar("inf"), "'inf'");
        assert_eq!(yaml_scalar("nan"), "'nan'");
        // Indicator-led, mid-string `: `, and trailing `:`.
        assert_eq!(yaml_scalar("- leading"), "'- leading'");
        assert_eq!(yaml_scalar("key: value"), "'key: value'");
        assert_eq!(yaml_scalar("trailing:"), "'trailing:'");
        // An apostrophe is legal mid-plain-scalar — it needs no quoting on its own.
        assert_eq!(yaml_scalar("it's"), "it's");
        // But when the value is single-quoted for another reason, it doubles.
        assert_eq!(yaml_scalar("key: it's"), "'key: it''s'");
    }

    #[test]
    fn yaml_scalar_quotes_sexagesimal() {
        // YAML 1.1 folds these to int/float (e.g. `12:30` → 750) unless quoted.
        assert_eq!(yaml_scalar("12:30"), "'12:30'");
        assert_eq!(yaml_scalar("1:2:3"), "'1:2:3'");
        assert_eq!(yaml_scalar("12:30:00"), "'12:30:00'");
        assert_eq!(yaml_scalar("12:30.5"), "'12:30.5'");
        // A non-numeric head or an out-of-range base-60 field is NOT sexagesimal.
        assert_eq!(yaml_scalar("a:b:c"), "a:b:c");
        assert_eq!(yaml_scalar("12:99"), "12:99");
        // A leading underscore is not a valid base-60 first field — stays plain.
        assert_eq!(yaml_scalar("_12:30"), "_12:30");
    }

    #[test]
    fn yaml_scalar_quotes_yaml11_number_forms() {
        // Radix integers and underscore digit groups: a YAML 1.1 reader folds these
        // to integers/floats, but Rust's `f64::parse` rejects them, so a bare
        // emission would silently change type on read.
        for n in ["0x1f", "0b101", "0o17", "1_000", "3_000.5"] {
            assert_eq!(yaml_scalar(n), format!("'{n}'"), "{n} must be quoted");
        }
        // Special-float spellings (`.inf` / `.nan` family, any case).
        for n in [".inf", "+.inf", "-.inf", ".nan", ".NaN"] {
            assert_eq!(yaml_scalar(n), format!("'{n}'"), "{n} must be quoted");
        }
        // Single-character booleans `y` / `n` resolve to bool in YAML 1.1.
        for n in ["y", "n", "Y", "N"] {
            assert_eq!(yaml_scalar(n), format!("'{n}'"), "{n} must be quoted");
        }
        // Underscored identifiers / CURIE locals are NOT numbers — stay plain.
        assert_eq!(yaml_scalar("has_part"), "has_part");
        assert_eq!(yaml_scalar("P1_2"), "P1_2");
    }

    #[test]
    fn yaml_scalar_quotes_timestamps() {
        // YAML 1.1 resolves a `YYYY-M-D` lead to a timestamp, not a string.
        assert_eq!(yaml_scalar("2001-12-14"), "'2001-12-14'");
        assert_eq!(
            yaml_scalar("2001-12-14T10:00:00Z"),
            "'2001-12-14T10:00:00Z'"
        );
        assert_eq!(yaml_scalar("2001-1-1 10:00:00"), "'2001-1-1 10:00:00'");
        // A dotted version string is not a date, and a year with a non-date tail
        // is not a timestamp lead — both stay plain.
        assert_eq!(yaml_scalar("1.0.0"), "1.0.0");
        assert_eq!(yaml_scalar("2001-mixed"), "2001-mixed");
    }

    #[test]
    fn yaml_scalar_double_quotes_control_chars() {
        // A multi-line definition is ENCODED (formerly a hard build failure).
        assert_eq!(yaml_scalar("line one\nline two"), "\"line one\\nline two\"");
        assert_eq!(yaml_scalar("a\tb"), "\"a\\tb\"");
        assert_eq!(yaml_scalar("a\rb"), "\"a\\rb\"");
        // A bell (U+0007) escapes as \x07.
        assert_eq!(yaml_scalar("a\u{7}b"), "\"a\\x07b\"");
        // Double quotes and backslashes escape inside the double-quoted form.
        assert_eq!(yaml_scalar("say \"hi\"\nbye"), "\"say \\\"hi\\\"\\nbye\"");
    }
}
