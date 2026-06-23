// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `okf` export leaf (#861 P4): Open Knowledge Format projection (dist/, gitignored).
//!
//! A genuine Rust port of `src/gmeow_tools/okf_export.py` (#780): projects the
//! folded GMEOW term surface into a conformant OKF bundle under
//! `dist/gmeow-okf/` — one Markdown document per concept (YAML frontmatter +
//! `[text](path)` body links), per-category indexes, and a root `index.md` that
//! carries the in-band lossy declaration.
//!
//! The bundle structure is GMEOW-specific (Class/Property/Individual docs with the
//! six recognized OKF frontmatter keys plus `okf:<key>` extensions). The Python
//! generator builds this layout itself — it does NOT call the `gts to-okf` codec
//! (which projects an already-OKF-profile graph). So this is a direct structural
//! port, not a codec call. Output is git-ignored `dist/`, so the bar is
//! structural validity + determinism, not byte-parity (terms arrive sorted, keys
//! are fixed-then-sorted, bodies carry no wall-clock content).

use std::collections::BTreeMap;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::export::{collect_term_surface, read_fold_upstream, Term};

/// The bundle directory name under `dist/` (#780, Task 2).
pub const OKF_DIR_NAME: &str = "gmeow-okf";

const LOSSY_NOTE: &str = "> LOSSY projection: the flat GMEOW term surface (label, definition, advisories, and IS-A / domain / range / sub-property links). The OWL axioms, the RDF-star statement/reification layer, and the full alignment graph are dropped — the GTS/OWL source is canonical.";

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
fn doc_relpath(term: &Term) -> String {
    format!("{}/{}.md", category_dir(term.category), slug(&term.curie))
}

/// A POSIX relative link from one bundle document to another (mirror `_relative_link`).
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

// ── YAML frontmatter value model (mirror yaml.safe_dump scalar/list/bool) ───────

enum Yaml {
    Str(String),
    Bool(bool),
    List(Vec<String>),
}

/// PyYAML `safe_dump` with `sort_keys=False, default_flow_style=False,
/// allow_unicode=True, width=10**9` over a mapping of scalar/bool/list values.
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

/// Emit a YAML scalar the way PyYAML `safe_dump(allow_unicode=True)` would:
/// plain when safe, otherwise single-quoted (PyYAML's preferred quote style).
fn yaml_scalar(s: &str) -> String {
    if needs_quoting(s) {
        // PyYAML single-quote style: double internal single quotes.
        format!("'{}'", s.replace('\'', "''"))
    } else {
        s.to_string()
    }
}

/// Whether a string must be quoted to round-trip as a YAML plain scalar.
fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    // Reserved plain-scalar resolutions and indicator-led strings.
    let lower = s.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "null" | "~" | "true" | "false" | "yes" | "no" | "on" | "off"
    ) {
        return true;
    }
    // Looks like a number.
    if s.parse::<f64>().is_ok() {
        return true;
    }
    let first = s.chars().next().unwrap();
    if "!&*[]{},#|>@`\"'%-?:".contains(first) {
        return true;
    }
    // Indicators / structural chars that break a plain scalar mid-string.
    if s.contains(": ") || s.contains(" #") || s.ends_with(':') {
        return true;
    }
    s.chars().any(|c| matches!(c, '\n' | '\t')) || s.starts_with(' ') || s.ends_with(' ')
}

// ── frontmatter + body (mirror _frontmatter / _body) ───────────────────────────

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
    // BTreeMap drains in sorted key order — matches Python `sorted(extension)`.
    for (key, value) in extension {
        fm.push((key, value));
    }
    fm
}

/// In-bundle relation targets (relation, target term) where the target is a
/// document in the bundle (mirror `_link_targets`).
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

fn render_doc(frontmatter: &[(String, Yaml)], body: &str) -> String {
    let fm = yaml_dump(frontmatter);
    format!("---\n{fm}---\n{body}")
}

fn index_doc(title: &str, entries: &[(String, String)], lossy_note: &str) -> String {
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
pub(crate) fn render_okf(title: &str, version: &str, terms: &[Term]) -> BTreeMap<String, Vec<u8>> {
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
        let doc = render_doc(&frontmatter(term, version), &body(term, &by_curie));
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
        let idx = index_doc(&format!("GMEOW {}", category_dir(category)), &entries, "");
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
        index_doc(&format!("{title} (OKF)"), &root_entries, LOSSY_NOTE).into_bytes(),
    );
    out
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
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "okf.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let graph = read_fold_upstream(input.upstream)?;
        let (title, version, terms) = collect_term_surface(&graph)?;
        let artifacts = render_okf(&title, &version, &terms);
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn okf_bundle_round_trips_structurally() {
        let root = repo_root();
        let graph = crate::stages::export::read_fold(&root).expect("read fold");
        let (title, version, terms) = collect_term_surface(&graph).expect("terms");
        assert!(!terms.is_empty(), "no terms collected");
        let arts = render_okf(&title, &version, &terms);

        // Root index + at least the class index must exist and carry the lossy note.
        let root_index = arts
            .get(&format!("dist/{OKF_DIR_NAME}/index.md"))
            .expect("root index present");
        let root_text = String::from_utf8(root_index.clone()).unwrap();
        assert!(root_text.starts_with("---\n"), "root index has frontmatter");
        assert!(
            root_text.contains("LOSSY projection"),
            "root index carries lossy note"
        );

        // Every per-term doc has a valid frontmatter block (--- … ---) and a type.
        let mut term_docs = 0;
        for (path, bytes) in &arts {
            if path.ends_with("/index.md") {
                continue;
            }
            let text = String::from_utf8(bytes.clone()).unwrap();
            assert!(text.starts_with("---\n"), "{path} missing opening fence");
            let rest = &text[4..];
            let close = rest
                .find("\n---\n")
                .unwrap_or_else(|| panic!("{path} missing closing fence"));
            let fm = &rest[..close];
            assert!(fm.contains("type:"), "{path} frontmatter has no type");
            assert!(
                fm.contains("resource:"),
                "{path} frontmatter has no resource"
            );
            term_docs += 1;
        }
        assert!(term_docs > 100, "expected many term docs, got {term_docs}");

        // Determinism: a second render is byte-identical.
        let arts2 = render_okf(&title, &version, &terms);
        assert_eq!(arts, arts2, "okf render is not deterministic");

        // A class doc links its parents under ## Relations with a relative path.
        let has_relation = arts.iter().any(|(p, b)| {
            p.contains("/classes/")
                && !p.ends_with("index.md")
                && String::from_utf8_lossy(b).contains("## Relations")
        });
        assert!(
            has_relation,
            "expected at least one class doc with relations"
        );
    }
}
