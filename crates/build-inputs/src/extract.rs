// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reviewable, syntax-directed relocation of inline test modules. Production
//! identity never strips tokens: this migration creates real external modules.

use gmeow_build_inputs::{Result, checked_path, digest};
fn fail(error: impl std::fmt::Display) -> gmeow_build_inputs::InputError {
    gmeow_build_inputs::InputError(error.to_string())
}
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
use syn::{
    Item,
    spanned::Spanned,
    visit::{self, Visit},
};

/// An exact source replacement; application checks the original byte commitment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    pub path: String,
    pub before_sha256: Option<String>,
    pub contents: String,
}
/// Byte-preservation evidence for one moved module, including every function body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preservation {
    pub source: String,
    pub destination: String,
    pub module: String,
    pub original_range: [usize; 2],
    pub extracted_range: [usize; 2],
    pub original_body_sha256: String,
    pub extracted_body_sha256: String,
    pub function_bodies: Vec<(String, String)>,
}
/// A plan is an artifact for review. Unsupported forms block its application.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionPlan {
    pub sources: Vec<String>,
    pub replacements: Vec<Replacement>,
    pub preservation: Vec<Preservation>,
    pub blockers: Vec<String>,
}

/// Plan changes only in the explicitly supplied production file inventory. The
/// controller owns that selection; this utility does not infer Cargo targets.
pub fn plan(workspace: &Path, sources: &[String]) -> Result<ExtractionPlan> {
    let mut plan = ExtractionPlan {
        sources: sources.to_vec(),
        ..ExtractionPlan::default()
    };
    let mut destinations = BTreeSet::new();
    for source in sources {
        let path = checked_path(workspace, Path::new(source))?;
        let original = std::fs::read_to_string(&path).map_err(fail)?;
        let syntax = syn::parse_file(&original).map_err(fail)?;
        let mut scanner = ExtractionScanner {
            source,
            path: &path,
            original: &original,
            edits: vec![],
            inline_depth: 0,
            plan: &mut plan,
            destinations: &mut destinations,
        };
        scanner.visit_file(&syntax);
        scanner.edits.sort_by_key(|(start, _, _)| *start);
        for pair in scanner.edits.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err(fail(format!("overlapping extraction edits in {source}")));
            }
        }
        let mut replacement = original.clone();
        for (start, end, new) in scanner.edits.into_iter().rev() {
            replacement.replace_range(start..end, &new);
        }
        if replacement != original {
            syn::parse_file(&replacement)
                .map_err(|error| fail(format!("extraction produced invalid {source}: {error}")))?;
            plan.replacements.push(Replacement {
                path: source.clone(),
                before_sha256: Some(digest(original.as_bytes())),
                contents: replacement,
            });
        }
    }
    plan.replacements
        .sort_by(|left, right| left.path.cmp(&right.path));
    plan.preservation.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.module.cmp(&right.module))
    });
    plan.blockers.sort();
    plan.blockers.dedup();
    Ok(plan)
}

/// Apply exactly a reviewed plan. Any changed input, existing destination,
/// unsupported form or failed body commitment refuses the entire preflight.
pub fn apply(workspace: &Path, reviewed: &ExtractionPlan) -> Result<()> {
    let current = plan(workspace, &reviewed.sources)?;
    if current != *reviewed {
        return Err(fail(
            "reviewed extraction plan differs from the exact source-derived edits",
        ));
    }
    let plan = reviewed;
    if !plan.blockers.is_empty() {
        return Err(fail(
            "extraction plan contains unresolved semantic blockers",
        ));
    }
    for proof in &plan.preservation {
        if proof.original_body_sha256 != proof.extracted_body_sha256 {
            return Err(fail("test body preservation failed"));
        }
        let original =
            std::fs::read(checked_path(workspace, Path::new(&proof.source))?).map_err(fail)?;
        let extracted = plan
            .replacements
            .iter()
            .find(|replacement| replacement.path == proof.destination)
            .ok_or_else(|| fail("preservation destination has no replacement"))?;
        let original = original
            .get(proof.original_range[0]..proof.original_range[1])
            .ok_or_else(|| fail("invalid original preservation range"))?;
        let extracted = extracted
            .contents
            .as_bytes()
            .get(proof.extracted_range[0]..proof.extracted_range[1])
            .ok_or_else(|| fail("invalid extracted preservation range"))?;
        if original != extracted || digest(original) != proof.original_body_sha256 {
            return Err(fail("extraction changed the original test body"));
        }
    }
    for replacement in &plan.replacements {
        let relative = Path::new(&replacement.path);
        let parent = relative
            .parent()
            .ok_or_else(|| fail("extraction path has no parent"))?;
        checked_path(workspace, parent)?;
        let path = workspace.join(relative);
        match &replacement.before_sha256 {
            Some(before) => {
                checked_path(workspace, relative)?;
                if digest(&std::fs::read(path).map_err(fail)?) != *before {
                    return Err(fail(format!(
                        "extraction input changed: {}",
                        replacement.path
                    )));
                }
            }
            None if path.try_exists().map_err(fail)? => {
                return Err(fail(format!(
                    "extraction destination already exists: {}",
                    replacement.path
                )));
            }
            None => {}
        }
        syn::parse_file(&replacement.contents).map_err(fail)?;
    }
    for replacement in &plan.replacements {
        std::fs::write(workspace.join(&replacement.path), &replacement.contents).map_err(fail)?;
    }
    Ok(())
}

struct ExtractionScanner<'a> {
    source: &'a str,
    path: &'a Path,
    original: &'a str,
    edits: Vec<(usize, usize, String)>,
    inline_depth: usize,
    plan: &'a mut ExtractionPlan,
    destinations: &'a mut BTreeSet<String>,
}
impl ExtractionScanner<'_> {
    fn blocker(&mut self, what: &str, span: proc_macro2::Span) {
        self.plan
            .blockers
            .push(format!("{}:{}: {what}", self.source, span.start().line));
    }
    fn module(&mut self, module: &syn::ItemMod) -> Result<()> {
        let Some((brace, items)) = &module.content else {
            return Ok(());
        };
        if module.attrs.iter().any(|attribute| {
            attribute.path().is_ident("path") || (attribute.path().is_ident("cfg_attr") && matches!(&attribute.meta, syn::Meta::List(list) if tokens_contain_ident(list.tokens.clone(), "path")))
        }) {
            self.blocker("existing path attribute needs an explicit preserved path owner", module.span());
            return Ok(());
        }
        if self.inline_depth != 0 {
            self.blocker("test module nested inside an inline production module needs an explicit preserved path base",module.span());
            return Ok(());
        }
        let begin = offset(self.original, brace.span.open().end())?;
        let end = offset(self.original, brace.span.close().start())?;
        let body = &self.original[begin..end];
        // File-relative includes and insta snapshot directories stay unchanged by
        // placing the extracted module beside its former source file.
        let stem = self
            .path
            .file_stem()
            .and_then(|part| part.to_str())
            .ok_or_else(|| fail("non-UTF8 source stem"))?;
        let filename = format!("{stem}.{}.rs", module.ident);
        let destination = Path::new(self.source)
            .with_file_name(&filename)
            .to_string_lossy()
            .into_owned();
        if !self.destinations.insert(destination.clone()) {
            return Err(fail(format!(
                "duplicate extraction destination {destination}"
            )));
        }
        if self
            .path
            .with_file_name(&filename)
            .try_exists()
            .map_err(fail)?
        {
            return Err(fail(format!(
                "extraction destination already exists: {destination}"
            )));
        }
        // Changing the physical file changes the base of nested external modules.
        // These need an explicit reviewed relocation before an automatic edit.
        let mut external = ExternalModule(false);
        for item in items {
            external.visit_item(item);
        }
        if external.0 {
            self.blocker("inline test module declares external child modules; preserve their path bases explicitly",module.span());
            return Ok(());
        }
        let mut functions = FunctionBodies {
            source: self.original,
            entries: vec![],
            error: None,
        };
        for item in items {
            functions.visit_item(item);
        }
        if let Some(error) = functions.error {
            return Err(error);
        }
        let start = offset(self.original, module.span().start())?;
        let finish = offset(self.original, module.span().end())?;
        let prefix = &self.original[start..offset(self.original, brace.span.open().start())?];
        // Keep every original attribute, visibility, name and module path. Only
        // the inline body becomes an external path-qualified declaration.
        let declaration = format!("#[path = {filename:?}]\n{prefix};");
        self.edits.push((start, finish, declaration));
        let header = "// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>\n// SPDX-License-Identifier: AGPL-3.0-only\n";
        let contents = format!("{header}{body}");
        syn::parse_file(&contents).map_err(fail)?;
        self.plan.replacements.push(Replacement {
            path: destination.clone(),
            before_sha256: None,
            contents,
        });
        let body_digest = digest(body.as_bytes());
        self.plan.preservation.push(Preservation {
            source: self.source.into(),
            destination,
            module: module.ident.to_string(),
            original_range: [begin, end],
            extracted_range: [header.len(), header.len() + body.len()],
            original_body_sha256: body_digest.clone(),
            extracted_body_sha256: body_digest,
            function_bodies: functions.entries,
        });
        Ok(())
    }
}
impl<'ast> Visit<'ast> for ExtractionScanner<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        let attrs = item_attributes(item);
        if test_only(attrs) {
            if matches!(item, Item::Use(_)) {
                return;
            }
            if let Item::Mod(module) = item {
                if let Err(error) = self.module(module) {
                    self.blocker(&error.to_string(), module.span());
                }
            } else {
                self.blocker(
                    "test-only item requires explicit helper/module relocation",
                    item.span(),
                );
            }
            return;
        }
        visit::visit_item(self, item);
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if module.content.is_some() {
            self.inline_depth += 1;
            visit::visit_item_mod(self, module);
            self.inline_depth -= 1;
        }
    }
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if test_only(std::slice::from_ref(attribute)) {
            self.blocker("nested test-only field, method, attribute or expression requires explicit relocation",attribute.span());
        }
    }
}
struct ExternalModule(bool);
impl<'ast> Visit<'ast> for ExternalModule {
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        if module.content.is_none() {
            self.0 = true;
        } else {
            visit::visit_item_mod(self, module);
        }
    }
}
struct FunctionBodies<'a> {
    source: &'a str,
    entries: Vec<(String, String)>,
    error: Option<gmeow_build_inputs::InputError>,
}
impl FunctionBodies<'_> {
    fn body(&mut self, name: String, body: &syn::Block) {
        let result = (|| {
            let start = offset(self.source, body.span().start())?;
            let end = offset(self.source, body.span().end())?;
            Ok((name, digest(self.source[start..end].as_bytes())))
        })();
        match result {
            Ok(entry) => self.entries.push(entry),
            Err(error) => self.error = Some(error),
        }
    }
}
impl<'ast> Visit<'ast> for FunctionBodies<'_> {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.body(item.sig.ident.to_string(), &item.block);
        visit::visit_item_fn(self, item);
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.body(item.sig.ident.to_string(), &item.block);
        visit::visit_impl_item_fn(self, item);
    }
}
fn offset(source: &str, position: proc_macro2::LineColumn) -> Result<usize> {
    if position.line == 0 {
        return Err(fail("Rust spans use one-based lines"));
    }
    let mut lines = source.split_inclusive('\n');
    let start = lines
        .by_ref()
        .take(position.line - 1)
        .map(str::len)
        .sum::<usize>();
    let line = lines.next().unwrap_or("");
    // proc_macro2 columns count Unicode scalar values, not UTF-8 bytes.
    let column = line
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(line.len()))
        .nth(position.column)
        .ok_or_else(|| fail("invalid Rust source column"))?;
    let offset = start + column;
    if offset <= source.len() && source.is_char_boundary(offset) {
        Ok(offset)
    } else {
        Err(fail("invalid Rust source span"))
    }
}

fn item_attributes(item: &Item) -> &[syn::Attribute] {
    match item {
        Item::Const(v) => &v.attrs,
        Item::Enum(v) => &v.attrs,
        Item::ExternCrate(v) => &v.attrs,
        Item::Fn(v) => &v.attrs,
        Item::ForeignMod(v) => &v.attrs,
        Item::Impl(v) => &v.attrs,
        Item::Macro(v) => &v.attrs,
        Item::Mod(v) => &v.attrs,
        Item::Static(v) => &v.attrs,
        Item::Struct(v) => &v.attrs,
        Item::Trait(v) => &v.attrs,
        Item::TraitAlias(v) => &v.attrs,
        Item::Type(v) => &v.attrs,
        Item::Union(v) => &v.attrs,
        Item::Use(v) => &v.attrs,
        _ => &[],
    }
}
fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attribute| {
        attribute.path().is_ident("test")
            || attribute.path().is_ident("bench")
            || match &attribute.meta {
                syn::Meta::List(list) if list.path.is_ident("cfg") => {
                    syn::parse2::<syn::Meta>(list.tokens.clone())
                        .is_ok_and(|condition| test_condition(&condition))
                }
                _ => false,
            }
    })
}
fn test_condition(meta: &syn::Meta) -> bool {
    use syn::parse::Parser as _;
    match meta {
        syn::Meta::Path(path) => {
            path.is_ident("test") || path.is_ident("doc") || path.is_ident("doctest")
        }
        syn::Meta::List(list) => {
            let Ok(values) =
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
            else {
                return false;
            };
            if list.path.is_ident("all") {
                values.iter().any(test_condition)
            } else if list.path.is_ident("any") {
                !values.is_empty() && values.iter().all(test_condition)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn tokens_contain_ident(tokens: proc_macro2::TokenStream, expected: &str) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident) => ident == expected,
        proc_macro2::TokenTree::Group(group) => tokens_contain_ident(group.stream(), expected),
        _ => false,
    })
}

/// One exact-file observation after a formatter-only pass.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormattedPreservation {
    pub path: String,
    pub planned_sha256: String,
    pub formatted_sha256: String,
    pub canonical_sha256: String,
    pub formatter: String,
    pub edition: String,
}

/// Verify both versions through the same configured rustfmt, including its
/// legitimate trailing-comma and layout rules. Compare entire canonical files;
/// no custom token stripping or assertion normalization participates. Source
/// identity still hashes whole original bytes. This is migration evidence only.
pub fn verify_format(
    workspace: &Path,
    reviewed: &ExtractionPlan,
) -> Result<Vec<FormattedPreservation>> {
    if !reviewed.blockers.is_empty() {
        return Err(fail("cannot verify a blocked extraction plan"));
    }
    let formatter = std::process::Command::new("rustfmt")
        .arg("--version")
        .output()
        .map_err(fail)?;
    if !formatter.status.success() {
        return Err(fail("cannot identify the migration formatter"));
    }
    let formatter = String::from_utf8(formatter.stdout)
        .map_err(fail)?
        .trim()
        .to_owned();
    let mut evidence = Vec::new();
    for replacement in &reviewed.replacements {
        let actual =
            std::fs::read_to_string(checked_path(workspace, Path::new(&replacement.path))?)
                .map_err(fail)?;
        syn::parse_file(&actual).map_err(fail)?;
        syn::parse_file(&replacement.contents).map_err(fail)?;
        let edition = source_edition(workspace, &replacement.path)?;
        let directory = workspace
            .join(&replacement.path)
            .parent()
            .ok_or_else(|| fail("source has no parent"))?
            .to_path_buf();
        let expected = canonical_format(&directory, &edition, &replacement.contents)?;
        let observed = canonical_format(&directory, &edition, &actual)?;
        if expected != observed {
            return Err(fail(format!(
                "formatting changed the canonical planned source: {}",
                replacement.path
            )));
        }
        evidence.push(FormattedPreservation {
            path: replacement.path.clone(),
            planned_sha256: digest(replacement.contents.as_bytes()),
            formatted_sha256: digest(actual.as_bytes()),
            canonical_sha256: digest(&observed),
            formatter: formatter.clone(),
            edition,
        });
    }
    Ok(evidence)
}

fn source_edition(workspace: &Path, source: &str) -> Result<String> {
    let root: toml::Value = std::fs::read_to_string(workspace.join("Cargo.toml"))
        .map_err(fail)?
        .parse()
        .map_err(fail)?;
    let mut directory = workspace
        .join(source)
        .parent()
        .ok_or_else(|| fail("source has no parent"))?
        .to_path_buf();
    loop {
        let manifest = directory.join("Cargo.toml");
        if manifest.try_exists().map_err(fail)? {
            let package: toml::Value = std::fs::read_to_string(&manifest)
                .map_err(fail)?
                .parse()
                .map_err(fail)?;
            let edition = package
                .get("package")
                .and_then(|package| package.get("edition"));
            if let Some(edition) = edition.and_then(toml::Value::as_str) {
                return Ok(edition.to_owned());
            }
            if edition
                .and_then(|edition| edition.get("workspace"))
                .and_then(toml::Value::as_bool)
                == Some(true)
            {
                return root
                    .get("workspace")
                    .and_then(|workspace| workspace.get("package"))
                    .and_then(|package| package.get("edition"))
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| fail("missing workspace Rust edition"));
            }
            if package.get("package").is_some() {
                return Ok("2015".into());
            }
        }
        if directory == workspace || !directory.pop() {
            return Err(fail("source has no Cargo edition owner"));
        }
    }
}

fn canonical_format(directory: &Path, edition: &str, source: &str) -> Result<Vec<u8>> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    let mut child = Command::new("rustfmt")
        .current_dir(directory)
        .args([
            "--emit",
            "stdout",
            "--edition",
            edition,
            "--config",
            "skip_children=true",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(fail)?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| fail("formatter stdin missing"))?;
    // Drain output concurrently: a large test module can exceed both pipe buffers.
    let result = std::thread::scope(|scope| -> Result<std::process::Output> {
        let writer = scope.spawn(move || stdin.write_all(source.as_bytes()));
        let output = child.wait_with_output().map_err(fail)?;
        writer
            .join()
            .map_err(|_| fail("formatter input writer panicked"))?
            .map_err(fail)?;
        Ok(output)
    })?;
    if !result.status.success() {
        return Err(fail(format!(
            "canonical rustfmt failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )));
    }
    Ok(result.stdout)
}
