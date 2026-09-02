// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Audit the **built** mdBook artifact emitted from GMEOW's source-book projection.
//!
//! This module deliberately does not embed or invoke mdBook. The external, pinned
//! presentation tool belongs to the `maint-mdbook-smoke` lane; this crate only verifies
//! the resulting bytes. That keeps both the core pipeline and `make check` independent of
//! mdBook while giving the off-gate lane a deterministic proof over the rendered corpus.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_errors::{Finding, Location, Report, Severity};

use crate::formats::{DocFormat, format_capabilities};
use crate::vendored_asset::{VendoredWasmAsset, capability_backing_assets};

const TOOL: &str = "gmeow-docs-mdbook-smoke";
const BOOT_PATH: &str = "mdbook-boot.js";
const CONTROLLER_PATH: &str = "assets/gmeow-docs.js";
const MDBOOK_AUXILIARY_TOC_PATH: &str = "toc.html";
const MDBOOK_SIDEBAR_IFRAME_CLASS: &str = "sidebar-iframe-inner";

/// A deterministic summary of one rendered-book audit.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedBookAudit {
    /// Number of rendered HTML pages inspected.
    pub html_pages: usize,
    /// Number of local `href`/`src` references inspected across those pages.
    pub local_references: usize,
    /// Number of distinct mdBook capability-backed wasm engines required and inspected.
    pub wasm_engines: usize,
    /// Structured findings. Any error means the rendered book is not publishable.
    pub report: Report,
}

/// Derive the wasm engines mdBook must pack from the canonical capability registry.
///
/// Deduplication is by the asset's stable name because one engine can back more than one
/// represented capability (the query engine backs both live SPARQL and interactivity).
/// No engine-name list is re-authored here.
#[must_use]
pub fn required_mdbook_engines() -> Vec<&'static VendoredWasmAsset> {
    let mut engines: BTreeMap<&'static str, &'static VendoredWasmAsset> = BTreeMap::new();
    for capability in format_capabilities(DocFormat::Mdbook).representable {
        for asset in capability_backing_assets(capability) {
            engines.insert(asset.name, *asset);
        }
    }
    engines.into_values().collect()
}

/// Audit a source-book tree and the HTML corpus built from it by pinned mdBook tooling.
///
/// The proof has three joined parts:
///
/// 1. every rendered HTML `href` and `src` stays inside the corpus (or is explicitly
///    external), resolves to an existing file, and names a real fragment when present;
/// 2. every rendered content page loads the copied boot shim, which loads the
///    byte-identical controller emitted in the source tree (mdBook's reserved
///    `toc.html` navigation iframe is link-checked but intentionally has no scripts); and
/// 3. the controller names the JavaScript and WebAssembly payload of each engine derived
///    from mdBook's represented capabilities, and every built payload is byte-identical
///    to its emitted source-tree counterpart.
#[must_use]
pub fn audit_rendered_book(source_root: &Path, rendered_root: &Path) -> RenderedBookAudit {
    let mut report = Report::new(TOOL);
    let inventory = Inventory::read(rendered_root, &mut report);
    let engines = required_mdbook_engines();

    let mut pages: BTreeMap<PathBuf, ParsedPage> = BTreeMap::new();
    for path in inventory
        .files
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "html"))
    {
        match std::fs::read(rendered_root.join(path)) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(html) => {
                    pages.insert(path.clone(), ParsedPage::new(&html));
                }
                Err(error) => add_error(
                    &mut report,
                    "docs/mdbook-non-utf8-html",
                    format!("rendered HTML is not UTF-8: {error}"),
                    Some(path),
                ),
            },
            Err(error) => add_error(
                &mut report,
                "docs/mdbook-read",
                format!("cannot read rendered HTML: {error}"),
                Some(path),
            ),
        }
    }
    if pages.is_empty() {
        add_error(
            &mut report,
            "docs/mdbook-empty-html",
            "rendered mdBook contains no readable HTML pages".to_string(),
            None,
        );
    }

    let rendered_boot = verify_runtime_chain(
        source_root,
        rendered_root,
        &inventory,
        &engines,
        &mut report,
    );

    let mut local_references = 0usize;
    for (page_path, page) in &pages {
        if let Some(boot) = rendered_boot.as_deref()
            && page.requires_runtime_boot(page_path)
            && !page.loads_script(page_path, boot)
        {
            add_error(
                &mut report,
                "docs/mdbook-missing-boot-reference",
                format!(
                    "rendered page does not load `{}` through a local script src",
                    boot.display()
                ),
                Some(page_path),
            );
        }

        for reference in &page.references {
            match resolve_reference(page_path, &reference.value) {
                Ok(ReferenceResolution::External) => {}
                Ok(ReferenceResolution::Local {
                    path,
                    fragment,
                    directory_hint,
                }) => {
                    local_references += 1;
                    let Some(target) = inventory.resolve_target(&path, directory_hint) else {
                        add_error(
                            &mut report,
                            "docs/mdbook-dangling-reference",
                            format!(
                                "local {} `{}` resolves to missing corpus path `{}`",
                                reference.attribute,
                                reference.value,
                                path.display()
                            ),
                            Some(page_path),
                        );
                        continue;
                    };
                    if let Some(fragment) = fragment.filter(|fragment| !fragment.is_empty()) {
                        let anchor_exists = if let Some(target_page) = pages.get(&target) {
                            Some(target_page.anchors.contains(&fragment))
                        } else {
                            anchors_from_file(rendered_root, &target, &mut report)
                                .map(|anchors| anchors.contains(&fragment))
                        };
                        if anchor_exists != Some(true) {
                            add_error(
                                &mut report,
                                "docs/mdbook-broken-fragment",
                                format!(
                                    "local {} `{}` resolves to `{}` but fragment `#{fragment}` is absent",
                                    reference.attribute,
                                    reference.value,
                                    target.display()
                                ),
                                Some(page_path),
                            );
                        }
                    }
                }
                Err(ReferenceError::RootEscape(path)) => {
                    local_references += 1;
                    add_error(
                        &mut report,
                        "docs/mdbook-root-escape",
                        format!(
                            "local {} `{}` escapes the rendered-book root while resolving `{path}`",
                            reference.attribute, reference.value
                        ),
                        Some(page_path),
                    );
                }
                Err(ReferenceError::Malformed(reason)) => {
                    local_references += 1;
                    add_error(
                        &mut report,
                        "docs/mdbook-malformed-reference",
                        format!(
                            "local {} `{}` cannot be resolved: {reason}",
                            reference.attribute, reference.value
                        ),
                        Some(page_path),
                    );
                }
            }
        }
    }

    report.normalize();
    RenderedBookAudit {
        html_pages: pages.len(),
        local_references,
        wasm_engines: engines.len(),
        report,
    }
}

fn verify_runtime_chain(
    source_root: &Path,
    rendered_root: &Path,
    inventory: &Inventory,
    engines: &[&VendoredWasmAsset],
    report: &mut Report,
) -> Option<PathBuf> {
    if engines.len() != 4 {
        add_error(
            report,
            "docs/mdbook-engine-registry",
            format!(
                "mdBook's represented capabilities resolve to {} distinct wasm engines, expected exactly four",
                engines.len()
            ),
            None,
        );
    }

    let source_boot = read_required(
        source_root,
        Path::new(BOOT_PATH),
        report,
        "emitted boot shim",
    );
    let rendered_boot_path = find_rendered_boot(inventory, report);
    let built_boot = rendered_boot_path
        .as_deref()
        .and_then(|path| read_required(rendered_root, path, report, "rendered boot shim"));
    let boot_location = rendered_boot_path
        .as_deref()
        .unwrap_or(Path::new(BOOT_PATH));
    compare_copy(
        source_boot.as_deref(),
        built_boot.as_deref(),
        boot_location,
        report,
    );
    if built_boot.as_deref().is_none_or(|bytes| {
        std::str::from_utf8(bytes)
            .map(|text| !text.contains(CONTROLLER_PATH))
            .unwrap_or(true)
    }) {
        add_error(
            report,
            "docs/mdbook-boot-controller",
            format!(
                "rendered `{}` does not load `{CONTROLLER_PATH}`",
                boot_location.display()
            ),
            Some(boot_location),
        );
    }

    let source_controller_path = Path::new("src").join(CONTROLLER_PATH);
    let built_controller_path = Path::new(CONTROLLER_PATH);
    let source_controller = read_required(
        source_root,
        &source_controller_path,
        report,
        "emitted docs controller",
    );
    let built_controller = read_required(
        rendered_root,
        built_controller_path,
        report,
        "rendered docs controller",
    );
    compare_copy(
        source_controller.as_deref(),
        built_controller.as_deref(),
        built_controller_path,
        report,
    );
    let controller_text = built_controller
        .as_deref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok());

    for engine in engines {
        let js_files: Vec<&str> = engine
            .emitted_files
            .iter()
            .map(|(filename, _)| *filename)
            .filter(|filename| filename.ends_with(".js"))
            .collect();
        let wasm_files: Vec<&str> = engine
            .emitted_files
            .iter()
            .map(|(filename, _)| *filename)
            .filter(|filename| filename.ends_with(".wasm"))
            .collect();
        if js_files.len() != 1 || wasm_files.len() != 1 {
            add_error(
                report,
                "docs/mdbook-engine-registry",
                format!(
                    "engine `{}` must expose exactly one emitted JavaScript loader and one emitted wasm payload (found {} JS, {} wasm)",
                    engine.name,
                    js_files.len(),
                    wasm_files.len()
                ),
                None,
            );
        }

        for (filename, _) in engine.emitted_files {
            let relative = Path::new("assets").join(engine.name).join(filename);
            let source_relative = Path::new("src").join(&relative);
            let source = read_required(
                source_root,
                &source_relative,
                report,
                "emitted engine asset",
            );
            let built = read_required(rendered_root, &relative, report, "rendered engine asset");
            compare_copy(source.as_deref(), built.as_deref(), &relative, report);

            if built.as_deref().is_some_and(<[u8]>::is_empty) {
                add_error(
                    report,
                    "docs/mdbook-empty-engine-asset",
                    format!(
                        "rendered engine `{}` payload `{filename}` is empty",
                        engine.name
                    ),
                    Some(&relative),
                );
            }

            if filename.ends_with(".wasm")
                && built
                    .as_deref()
                    .is_some_and(|bytes| !bytes.starts_with(b"\0asm"))
            {
                add_error(
                    report,
                    "docs/mdbook-invalid-wasm",
                    format!(
                        "rendered engine `{}` payload `{filename}` lacks the WebAssembly magic",
                        engine.name
                    ),
                    Some(&relative),
                );
            }

            if (filename.ends_with(".js") || filename.ends_with(".wasm"))
                && controller_text.is_none_or(|controller| {
                    !controller.contains(&format!("./{}/{filename}", engine.name))
                })
            {
                add_error(
                    report,
                    "docs/mdbook-controller-engine",
                    format!(
                        "rendered controller does not reference engine `{}` payload `./{}/{filename}`",
                        engine.name, engine.name
                    ),
                    Some(built_controller_path),
                );
            }
        }
    }

    rendered_boot_path
}

/// mdBook 0.5 fingerprints static assets by default, so an emitted
/// `mdbook-boot.js` is rendered as `mdbook-boot-<fingerprint>.js`. Select that
/// concrete rendered path once, then require every page to load the same file and
/// require its bytes to equal the canonical source shim.
fn find_rendered_boot(inventory: &Inventory, report: &mut Report) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = inventory
        .files
        .iter()
        .filter(|path| {
            path.parent()
                .is_some_and(|parent| parent.as_os_str().is_empty())
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(is_rendered_boot_name)
        })
        .cloned()
        .collect();
    match candidates.as_slice() {
        [path] => Some(path.clone()),
        [] => {
            add_error(
                report,
                "docs/mdbook-missing-asset",
                format!(
                    "rendered book contains neither `{BOOT_PATH}` nor its mdBook-fingerprinted form"
                ),
                Some(Path::new(BOOT_PATH)),
            );
            None
        }
        _ => {
            add_error(
                report,
                "docs/mdbook-ambiguous-boot",
                format!(
                    "rendered book contains multiple boot-shim candidates: {}",
                    candidates
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None,
            );
            None
        }
    }
}

fn is_rendered_boot_name(name: &str) -> bool {
    if name == BOOT_PATH {
        return true;
    }
    name.strip_prefix("mdbook-boot-")
        .and_then(|suffix| suffix.strip_suffix(".js"))
        .is_some_and(|fingerprint| {
            fingerprint.len() == 8 && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn read_required(
    root: &Path,
    relative: &Path,
    report: &mut Report,
    description: &str,
) -> Option<Vec<u8>> {
    match std::fs::read(root.join(relative)) {
        Ok(bytes) => Some(bytes),
        Err(error) => {
            add_error(
                report,
                "docs/mdbook-missing-asset",
                format!(
                    "cannot read {description} `{}`: {error}",
                    relative.display()
                ),
                Some(relative),
            );
            None
        }
    }
}

fn compare_copy(source: Option<&[u8]>, built: Option<&[u8]>, relative: &Path, report: &mut Report) {
    if let (Some(source), Some(built)) = (source, built)
        && source != built
    {
        add_error(
            report,
            "docs/mdbook-asset-drift",
            format!(
                "rendered asset `{}` is not byte-identical to the emitted source-book asset",
                relative.display()
            ),
            Some(relative),
        );
    }
}

fn anchors_from_file(
    rendered_root: &Path,
    relative: &Path,
    report: &mut Report,
) -> Option<BTreeSet<String>> {
    match std::fs::read(rendered_root.join(relative)) {
        Ok(bytes) => match std::str::from_utf8(&bytes) {
            Ok(text) => Some(collect_anchors(&parse_tags(text))),
            Err(error) => {
                add_error(
                    report,
                    "docs/mdbook-unverifiable-fragment",
                    format!(
                        "fragment target `{}` is not UTF-8 and cannot expose an HTML/SVG id: {error}",
                        relative.display()
                    ),
                    Some(relative),
                );
                None
            }
        },
        Err(error) => {
            add_error(
                report,
                "docs/mdbook-read",
                format!("cannot read fragment target: {error}"),
                Some(relative),
            );
            None
        }
    }
}

fn add_error(report: &mut Report, code: &str, message: String, path: Option<&Path>) {
    let mut finding = Finding::new(Severity::Error, code, message).with_tool(TOOL);
    if let Some(path) = path {
        finding.add_location(Location::new(
            Some(path.to_string_lossy().into_owned()),
            None,
            None,
            None,
        ));
    }
    report.add_finding(finding);
}

#[derive(Debug, Default)]
struct Inventory {
    files: BTreeSet<PathBuf>,
    directories: BTreeSet<PathBuf>,
}

impl Inventory {
    fn read(root: &Path, report: &mut Report) -> Self {
        let mut inventory = Self::default();
        inventory.directories.insert(PathBuf::new());
        match std::fs::symlink_metadata(root) {
            Ok(metadata) if metadata.is_dir() => {
                inventory.walk(root, Path::new(""), report);
            }
            Ok(_) => add_error(
                report,
                "docs/mdbook-render-root",
                format!("rendered-book root `{}` is not a directory", root.display()),
                None,
            ),
            Err(error) => add_error(
                report,
                "docs/mdbook-render-root",
                format!(
                    "cannot inspect rendered-book root `{}`: {error}",
                    root.display()
                ),
                None,
            ),
        }
        inventory
    }

    fn walk(&mut self, root: &Path, relative: &Path, report: &mut Report) {
        let directory = root.join(relative);
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                add_error(
                    report,
                    "docs/mdbook-read-directory",
                    format!("cannot read rendered directory: {error}"),
                    Some(relative),
                );
                return;
            }
        };
        let mut readable_entries = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => readable_entries.push(entry),
                Err(error) => add_error(
                    report,
                    "docs/mdbook-read-directory-entry",
                    format!("cannot read rendered directory entry: {error}"),
                    Some(relative),
                ),
            }
        }
        readable_entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in readable_entries {
            let child = relative.join(entry.file_name());
            match entry.file_type() {
                Ok(kind) if kind.is_symlink() => add_error(
                    report,
                    "docs/mdbook-symlink",
                    "rendered corpus contains a symlink; link validation never follows symlinks"
                        .to_string(),
                    Some(&child),
                ),
                Ok(kind) if kind.is_dir() => {
                    self.directories.insert(child.clone());
                    self.walk(root, &child, report);
                }
                Ok(kind) if kind.is_file() => {
                    self.files.insert(child);
                }
                Ok(_) => add_error(
                    report,
                    "docs/mdbook-special-file",
                    "rendered corpus contains a non-file, non-directory entry".to_string(),
                    Some(&child),
                ),
                Err(error) => add_error(
                    report,
                    "docs/mdbook-file-type",
                    format!("cannot inspect rendered entry: {error}"),
                    Some(&child),
                ),
            }
        }
    }

    fn resolve_target(&self, path: &Path, directory_hint: bool) -> Option<PathBuf> {
        if !directory_hint && self.files.contains(path) {
            return Some(path.to_path_buf());
        }
        if self.directories.contains(path) {
            let index = path.join("index.html");
            if self.files.contains(&index) {
                return Some(index);
            }
        }
        None
    }
}

#[derive(Debug)]
struct ParsedPage {
    references: Vec<HtmlReference>,
    script_sources: Vec<String>,
    anchors: BTreeSet<String>,
    is_mdbook_sidebar_iframe: bool,
}

impl ParsedPage {
    fn new(html: &str) -> Self {
        let tags = parse_tags(html);
        let mut references = Vec::new();
        let mut script_sources = Vec::new();
        for tag in &tags {
            for (name, value) in &tag.attributes {
                if name == "href" || name == "src" {
                    references.push(HtmlReference {
                        attribute: name.clone(),
                        value: decode_html_entities(value),
                    });
                }
                if tag.name == "script" && name == "src" {
                    script_sources.push(decode_html_entities(value));
                }
            }
        }
        Self {
            references,
            script_sources,
            anchors: collect_anchors(&tags),
            is_mdbook_sidebar_iframe: tags.iter().any(|tag| {
                tag.name == "body"
                    && tag.attributes.iter().any(|(name, value)| {
                        name == "class"
                            && decode_html_entities(value)
                                .split_ascii_whitespace()
                                .any(|class| class == MDBOOK_SIDEBAR_IFRAME_CLASS)
                    })
            }),
        }
    }

    /// mdBook emits one root `toc.html` as a no-script fallback sidebar iframe.
    /// Requiring `additional-js` there would assert behavior the pinned renderer does
    /// not provide. Both the reserved path and renderer-owned body class must match so
    /// an ordinary authored page cannot acquire the exemption accidentally.
    fn requires_runtime_boot(&self, page_path: &Path) -> bool {
        page_path != Path::new(MDBOOK_AUXILIARY_TOC_PATH) || !self.is_mdbook_sidebar_iframe
    }

    fn loads_script(&self, page_path: &Path, expected_path: &Path) -> bool {
        self.script_sources.iter().any(|source| {
            matches!(
                resolve_reference(page_path, source),
                Ok(ReferenceResolution::Local { path, .. }) if path == expected_path
            )
        })
    }
}

#[derive(Debug)]
struct HtmlReference {
    attribute: String,
    value: String,
}

#[derive(Debug)]
struct HtmlTag {
    name: String,
    attributes: Vec<(String, String)>,
}

fn collect_anchors(tags: &[HtmlTag]) -> BTreeSet<String> {
    tags.iter()
        .flat_map(|tag| &tag.attributes)
        .filter(|(name, _)| name == "id" || name == "name")
        .map(|(_, value)| decode_html_entities(value))
        .collect()
}

/// Parse only opening-tag names and attribute values from controlled mdBook HTML.
/// Comments, declarations, and closing tags are ignored; quoted `>` characters remain
/// inside their attribute. No DOM normalization is needed for link/anchor validation.
fn parse_tags(html: &str) -> Vec<HtmlTag> {
    let bytes = html.as_bytes();
    let mut tags = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = bytes[cursor..].iter().position(|byte| *byte == b'<') {
        let start = cursor + offset;
        if bytes[start..].starts_with(b"<!--") {
            cursor = find_bytes(&bytes[start + 4..], b"-->")
                .map_or(bytes.len(), |end| start + 4 + end + 3);
            continue;
        }
        let mut quote = None;
        let mut end = start + 1;
        while end < bytes.len() {
            match (quote, bytes[end]) {
                (None, b'\'' | b'"') => quote = Some(bytes[end]),
                (Some(open), close) if open == close => quote = None,
                (None, b'>') => break,
                _ => {}
            }
            end += 1;
        }
        if end == bytes.len() {
            break;
        }
        let mut raw_element = None;
        if let Ok(content) = std::str::from_utf8(&bytes[start + 1..end])
            && let Some(tag) = parse_opening_tag(content)
        {
            if !content.trim_end().ends_with('/')
                && matches!(tag.name.as_str(), "script" | "style" | "textarea" | "title")
            {
                raw_element = Some(tag.name.clone());
            }
            tags.push(tag);
        }
        cursor = end + 1;
        if let Some(name) = raw_element {
            let closing = format!("</{name}");
            cursor = find_ascii_case_insensitive(&bytes[cursor..], closing.as_bytes()).map_or(
                bytes.len(),
                |closing_offset| {
                    let closing_start = cursor + closing_offset;
                    bytes[closing_start..]
                        .iter()
                        .position(|byte| *byte == b'>')
                        .map_or(bytes.len(), |closing_end| closing_start + closing_end + 1)
                },
            );
        }
    }
    tags
}

fn parse_opening_tag(content: &str) -> Option<HtmlTag> {
    let bytes = content.as_bytes();
    let mut cursor = 0usize;
    skip_ascii_whitespace(bytes, &mut cursor);
    if cursor == bytes.len() || matches!(bytes[cursor], b'/' | b'!' | b'?') {
        return None;
    }
    let name_start = cursor;
    while cursor < bytes.len()
        && !bytes[cursor].is_ascii_whitespace()
        && !matches!(bytes[cursor], b'/' | b'>')
    {
        cursor += 1;
    }
    let name = content[name_start..cursor].to_ascii_lowercase();
    let mut attributes = Vec::new();
    while cursor < bytes.len() {
        skip_ascii_whitespace(bytes, &mut cursor);
        if cursor == bytes.len() || bytes[cursor] == b'/' {
            break;
        }
        let attribute_start = cursor;
        while cursor < bytes.len()
            && !bytes[cursor].is_ascii_whitespace()
            && !matches!(bytes[cursor], b'=' | b'/')
        {
            cursor += 1;
        }
        if attribute_start == cursor {
            cursor += 1;
            continue;
        }
        let attribute = content[attribute_start..cursor].to_ascii_lowercase();
        skip_ascii_whitespace(bytes, &mut cursor);
        let mut value = String::new();
        if cursor < bytes.len() && bytes[cursor] == b'=' {
            cursor += 1;
            skip_ascii_whitespace(bytes, &mut cursor);
            if cursor < bytes.len() && matches!(bytes[cursor], b'\'' | b'"') {
                let quote = bytes[cursor];
                cursor += 1;
                let value_start = cursor;
                while cursor < bytes.len() && bytes[cursor] != quote {
                    cursor += 1;
                }
                value = content[value_start..cursor].to_string();
                cursor += usize::from(cursor < bytes.len());
            } else {
                let value_start = cursor;
                while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
                    cursor += 1;
                }
                value = content[value_start..cursor].to_string();
            }
        }
        attributes.push((attribute, value));
    }
    Some(HtmlTag { name, attributes })
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: &mut usize) {
    while *cursor < bytes.len() && bytes[*cursor].is_ascii_whitespace() {
        *cursor += 1;
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

#[derive(Debug)]
enum ReferenceResolution {
    External,
    Local {
        path: PathBuf,
        fragment: Option<String>,
        directory_hint: bool,
    },
}

#[derive(Debug)]
enum ReferenceError {
    RootEscape(String),
    Malformed(String),
}

fn resolve_reference(
    page_path: &Path,
    reference: &str,
) -> Result<ReferenceResolution, ReferenceError> {
    if is_external_reference(reference) {
        return Ok(ReferenceResolution::External);
    }
    let (path_and_query, raw_fragment) = reference
        .split_once('#')
        .map_or((reference, None), |(path, fragment)| (path, Some(fragment)));
    let raw_path = path_and_query
        .split_once('?')
        .map_or(path_and_query, |(path, _)| path);
    let decoded_path = percent_decode(raw_path)?;
    let fragment = raw_fragment.map(percent_decode).transpose()?;
    if decoded_path.contains(['\0', '\\']) || fragment.as_deref().is_some_and(|f| f.contains('\0'))
    {
        return Err(ReferenceError::Malformed(
            "NUL and backslash are forbidden in local URLs".to_string(),
        ));
    }

    let absolute = decoded_path.starts_with('/');
    let directory_hint = decoded_path.ends_with('/');
    let mut target = if absolute {
        PathBuf::new()
    } else {
        page_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf()
    };
    for segment in decoded_path.trim_start_matches('/').split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if !target.pop() {
                    return Err(ReferenceError::RootEscape(reference.to_string()));
                }
            }
            segment => target.push(segment),
        }
    }
    if decoded_path.is_empty() {
        target = page_path.to_path_buf();
    }
    Ok(ReferenceResolution::Local {
        path: target,
        fragment,
        directory_hint,
    })
}

fn is_external_reference(reference: &str) -> bool {
    if reference.starts_with("//") {
        return true;
    }
    let Some(colon) = reference.find(':') else {
        return false;
    };
    let first_separator = reference.find(['/', '?', '#']).unwrap_or(reference.len());
    if colon > first_separator {
        return false;
    }
    let scheme = &reference[..colon];
    !scheme.is_empty()
        && scheme.as_bytes()[0].is_ascii_alphabetic()
        && scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

fn percent_decode(value: &str) -> Result<String, ReferenceError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] == b'%' {
            if cursor + 2 >= bytes.len() {
                return Err(ReferenceError::Malformed(format!(
                    "truncated percent escape at byte {cursor}"
                )));
            }
            let high = hex(bytes[cursor + 1]).ok_or_else(|| {
                ReferenceError::Malformed(format!("invalid percent escape at byte {cursor}"))
            })?;
            let low = hex(bytes[cursor + 2]).ok_or_else(|| {
                ReferenceError::Malformed(format!("invalid percent escape at byte {cursor}"))
            })?;
            decoded.push((high << 4) | low);
            cursor += 3;
        } else {
            decoded.push(bytes[cursor]);
            cursor += 1;
        }
    }
    String::from_utf8(decoded).map_err(|error| {
        ReferenceError::Malformed(format!("percent-decoded URL is not UTF-8: {error}"))
    })
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_html_entities(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let entity_start = &rest[start + 1..];
        let Some(end) = entity_start.find(';') else {
            decoded.push_str(&rest[start..]);
            return decoded;
        };
        let entity = &entity_start[..end];
        let replacement = match entity {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        if let Some(replacement) = replacement {
            decoded.push(replacement);
        } else {
            decoded.push('&');
            decoded.push_str(entity);
            decoded.push(';');
        }
        rest = &entity_start[end + 1..];
    }
    decoded.push_str(rest);
    decoded
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    const RENDERED_BOOT_PATH: &str = "mdbook-boot-deadbeef.js";

    struct Fixture {
        _temp: TempDir,
        source: PathBuf,
        rendered: PathBuf,
    }

    fn write(root: &Path, relative: impl AsRef<Path>, bytes: impl AsRef<[u8]>) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture file has a parent")).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let rendered = temp.path().join("rendered");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&rendered).unwrap();

        let boot = b"import(new URL(\"assets/gmeow-docs.js\", document.currentScript.src));\n";
        write(&source, BOOT_PATH, boot);
        write(&rendered, RENDERED_BOOT_PATH, boot);

        let mut controller = String::new();
        for engine in required_mdbook_engines() {
            for (filename, _) in engine.emitted_files {
                let bytes: &[u8] = if filename.ends_with(".wasm") {
                    b"\0asm\x01\0\0\0"
                } else {
                    b"export default true;\n"
                };
                let relative = Path::new("assets").join(engine.name).join(filename);
                write(&source, Path::new("src").join(&relative), bytes);
                write(&rendered, &relative, bytes);
                controller.push_str(&format!(
                    "const _{} = './{}/{}';\n",
                    engine.name, engine.name, filename
                ));
            }
        }
        write(
            &source,
            Path::new("src").join(CONTROLLER_PATH),
            controller.as_bytes(),
        );
        write(&rendered, CONTROLLER_PATH, controller.as_bytes());

        write(&rendered, "book.css", b"body {}\n");
        write(&rendered, "image.svg", b"<svg id=\"image\"></svg>\n");
        write(
            &rendered,
            "index.html",
            br##"<!doctype html><html><head><link href="book.css"><script src="mdbook-boot-deadbeef.js"></script></head><body id="home section"><a href="guide/#details">Guide</a><a href="#home%20section">Home</a></body></html>"##,
        );
        write(
            &rendered,
            "guide/index.html",
            br##"<!doctype html><html><head><script src="../mdbook-boot-deadbeef.js"></script></head><body><h2 id="details">Details</h2><a href="../index.html#home%20section">Home</a><img src="../image.svg"></body></html>"##,
        );

        Fixture {
            _temp: temp,
            source,
            rendered,
        }
    }

    fn has_code(audit: &RenderedBookAudit, code: &str) -> bool {
        audit
            .report
            .findings
            .iter()
            .any(|finding| finding.code == code)
    }

    #[test]
    fn complete_rendered_corpus_and_four_engine_chain_pass() {
        let fixture = fixture();
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(audit.report.ok(), "{:?}", audit.report.legacy_errors());
        assert_eq!(audit.html_pages, 2);
        assert!(audit.local_references >= 7);
        assert_eq!(audit.wasm_engines, 4);
        assert_eq!(
            required_mdbook_engines()
                .iter()
                .map(|asset| asset.name)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["gmn", "query", "reason", "validate"])
        );
    }

    #[test]
    fn missing_target_and_cross_page_fragment_fail() {
        let fixture = fixture();
        write(
            &fixture.rendered,
            "guide/index.html",
            br#"<html><script src="../mdbook-boot-deadbeef.js"></script><a href="missing.html">Missing</a><a href="../index.html#absent">Bad anchor</a></html>"#,
        );
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(has_code(&audit, "docs/mdbook-dangling-reference"));
        assert!(has_code(&audit, "docs/mdbook-broken-fragment"));
    }

    #[test]
    fn root_escape_fails_even_when_the_outside_file_exists() {
        let fixture = fixture();
        fs::write(
            fixture.rendered.parent().unwrap().join("outside.html"),
            b"outside",
        )
        .unwrap();
        write(
            &fixture.rendered,
            "guide/index.html",
            br#"<html><script src="../mdbook-boot-deadbeef.js"></script><a href="../../outside.html">Escape</a></html>"#,
        );
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(has_code(&audit, "docs/mdbook-root-escape"));
    }

    #[test]
    fn missing_and_mutated_engine_payloads_fail() {
        let missing = fixture();
        let engine = required_mdbook_engines()[0];
        let wasm = engine
            .emitted_files
            .iter()
            .map(|(filename, _)| *filename)
            .find(|filename| filename.ends_with(".wasm"))
            .unwrap();
        fs::remove_file(missing.rendered.join("assets").join(engine.name).join(wasm)).unwrap();
        let audit = audit_rendered_book(&missing.source, &missing.rendered);
        assert!(has_code(&audit, "docs/mdbook-missing-asset"));

        let mutated = fixture();
        write(
            &mutated.rendered,
            Path::new("assets").join(engine.name).join(wasm),
            b"\0asmDIFFERENT",
        );
        let audit = audit_rendered_book(&mutated.source, &mutated.rendered);
        assert!(has_code(&audit, "docs/mdbook-asset-drift"));

        let empty = fixture();
        let js = engine
            .emitted_files
            .iter()
            .map(|(filename, _)| *filename)
            .find(|filename| filename.ends_with(".js"))
            .unwrap();
        write(
            &empty.rendered,
            Path::new("assets").join(engine.name).join(js),
            b"",
        );
        let audit = audit_rendered_book(&empty.source, &empty.rendered);
        assert!(has_code(&audit, "docs/mdbook-empty-engine-asset"));
    }

    #[test]
    fn broken_boot_and_controller_loading_chain_fails() {
        let broken_boot = fixture();
        write(
            &broken_boot.rendered,
            RENDERED_BOOT_PATH,
            b"console.log('no controller');\n",
        );
        let audit = audit_rendered_book(&broken_boot.source, &broken_boot.rendered);
        assert!(has_code(&audit, "docs/mdbook-boot-controller"));

        let broken_controller = fixture();
        write(
            &broken_controller.rendered,
            CONTROLLER_PATH,
            b"export default true;\n",
        );
        let audit = audit_rendered_book(&broken_controller.source, &broken_controller.rendered);
        assert!(has_code(&audit, "docs/mdbook-controller-engine"));
    }

    #[test]
    fn every_page_must_load_the_boot_shim() {
        let fixture = fixture();
        write(
            &fixture.rendered,
            "guide/index.html",
            br#"<html><a href="../index.html">Home</a></html>"#,
        );
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(has_code(&audit, "docs/mdbook-missing-boot-reference"));
    }

    #[test]
    fn mdbook_auxiliary_toc_is_link_checked_without_requiring_boot() {
        let fixture = fixture();
        write(
            &fixture.rendered,
            MDBOOK_AUXILIARY_TOC_PATH,
            br#"<html><body class="sidebar-iframe-inner"><a href="index.html">Home</a></body></html>"#,
        );
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(audit.report.ok(), "{:?}", audit.report.legacy_errors());
        assert_eq!(audit.html_pages, 3);

        write(
            &fixture.rendered,
            MDBOOK_AUXILIARY_TOC_PATH,
            br#"<html><body class="sidebar-iframe-inner"><a href="missing.html">Missing</a></body></html>"#,
        );
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(has_code(&audit, "docs/mdbook-dangling-reference"));
    }

    #[test]
    fn ordinary_toc_path_does_not_bypass_the_boot_requirement() {
        let fixture = fixture();
        write(
            &fixture.rendered,
            MDBOOK_AUXILIARY_TOC_PATH,
            br#"<html><body><a href="index.html">Home</a></body></html>"#,
        );
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(has_code(&audit, "docs/mdbook-missing-boot-reference"));
    }

    #[test]
    fn raw_script_markup_is_ignored_and_unquoted_paths_are_checked() {
        let fixture = fixture();
        write(
            &fixture.rendered,
            "index.html",
            br#"<html><script src=mdbook-boot-deadbeef.js></script><script>const example = '<a href="missing.html">';</script><body id="home section"><a href=guide/index.html#details>Guide</a></body></html>"#,
        );
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(audit.report.ok(), "{:?}", audit.report.legacy_errors());
    }

    #[test]
    fn an_empty_rendered_html_corpus_fails() {
        let fixture = fixture();
        fs::remove_file(fixture.rendered.join("index.html")).unwrap();
        fs::remove_file(fixture.rendered.join("guide/index.html")).unwrap();
        let audit = audit_rendered_book(&fixture.source, &fixture.rendered);
        assert!(has_code(&audit, "docs/mdbook-empty-html"));
    }
}
