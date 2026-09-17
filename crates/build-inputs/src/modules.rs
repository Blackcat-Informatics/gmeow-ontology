// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{InputInventory, InputRole, Result, UnitSelection, checked_path, fail, relative};
use proc_macro2::{TokenStream, TokenTree};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::{
    Attribute, Expr, Item, Meta, Token,
    parse::Parser,
    punctuated::Punctuated,
    visit::{self, Visit},
};

pub(crate) fn collect(workspace: &Path, inventory: &mut InputInventory) -> Result<()> {
    collect_selected(workspace, inventory, false)
}

pub(crate) fn production_extraction_paths(
    workspace: &Path,
    selection: &crate::ProductionSelection,
) -> Result<Vec<String>> {
    selection.validate()?;
    let mut inventory = InputInventory::empty(selection.clone());
    collect_selected(workspace, &mut inventory, true)?;
    Ok(inventory
        .files
        .into_iter()
        .filter(|(_, input)| {
            input.roles.contains(&InputRole::Rust)
                || input.roles.contains(&InputRole::BuildController)
        })
        .map(|(path, _)| path)
        .collect())
}

fn collect_selected(
    workspace: &Path,
    inventory: &mut InputInventory,
    extraction_planning: bool,
) -> Result<()> {
    let units = inventory.selection.units.clone();
    let mut excluded = BTreeSet::new();
    for unit in &units {
        let Some(source) = &unit.source else { continue };
        let path = checked_path(workspace, Path::new(source))?;
        let mut scanner = Scanner {
            workspace,
            inventory,
            unit,
            file: path.clone(),
            directory: path
                .parent()
                .ok_or_else(|| fail("crate root has no parent"))?
                .to_path_buf(),
            attribute_directory: path.parent().unwrap().to_path_buf(),
            stack: Vec::new(),
            seen: BTreeSet::new(),
            excluded: &mut excluded,
            macros: BTreeSet::new(),
            semantic: false,
            extraction_planning,
            error: None,
        };
        scanner.file(&path, true)?;
    }
    for excluded in excluded {
        if inventory
            .files
            .get(&excluded)
            .is_some_and(|input| input.roles.contains(&InputRole::Embedded))
        {
            return Err(fail(format!(
                "production embeds excluded test implementation: {excluded}"
            )));
        }
    }
    Ok(())
}
pub(crate) fn semantic_sources(workspace: &Path, roots: &[&str]) -> Result<crate::NativeSources> {
    let (inventory, excluded) = semantic_inventory(workspace, roots, false)?;
    Ok(crate::NativeSources {
        files: inventory
            .files
            .into_iter()
            .map(|(name, file)| (name, file.sha256))
            .collect(),
        excluded: excluded
            .into_iter()
            .map(|name| (name, "external test/doc-only module declaration".into()))
            .collect(),
    })
}

/// A migration input list, never an admitted semantic contract. The exact same
/// module resolver is used, while existing inline test implementation is left for
/// the syntax-directed planner to relocate or report as a blocker.
pub(crate) fn extraction_paths(workspace: &Path, roots: &[&str]) -> Result<Vec<String>> {
    let (inventory, _) = semantic_inventory(workspace, roots, true)?;
    Ok(inventory
        .files
        .into_iter()
        .filter(|(_, input)| input.roles.contains(&InputRole::Rust))
        .map(|(path, _)| path)
        .collect())
}

fn semantic_inventory(
    workspace: &Path,
    roots: &[&str],
    extraction_planning: bool,
) -> Result<(InputInventory, BTreeSet<String>)> {
    let selection = crate::ProductionSelection {
        schema: crate::SCHEMA,
        units: vec![],
        roots: vec![],
        policy_files: vec![],
    };
    let mut inventory = InputInventory {
        schema: crate::SCHEMA,
        selection,
        files: BTreeMap::new(),
        manifests: BTreeMap::new(),
        packages: BTreeMap::new(),
        memberships: BTreeMap::new(),
        generated: BTreeMap::new(),
        environment_owners: BTreeMap::new(),
    };
    let mut excluded = BTreeSet::new();
    for source in roots {
        let path = checked_path(workspace, Path::new(source))?;
        let manifest = path
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| fail("kernel root has no manifest owner"))?
            .join("Cargo.toml");
        let unit = UnitSelection {
            package: "native-semantic-kernel".into(),
            manifest: Some(relative(workspace, &manifest)?),
            source: Some((*source).into()),
            target: "semantic-kernel".into(),
            kinds: vec!["lib".into()],
            cfg: crate::CfgContext::from_rustc("", &[], true)?,
            dependencies: vec![],
            dependency_names: vec![],
            controller: false,
        };
        let mut scanner = Scanner {
            workspace,
            inventory: &mut inventory,
            unit: &unit,
            file: path.clone(),
            directory: path.parent().unwrap().to_path_buf(),
            attribute_directory: path.parent().unwrap().to_path_buf(),
            stack: Vec::new(),
            seen: BTreeSet::new(),
            excluded: &mut excluded,
            macros: BTreeSet::new(),
            semantic: true,
            extraction_planning,
            error: None,
        };
        scanner.file(&path, true)?;
    }
    for path in &excluded {
        if inventory.files.contains_key(path) {
            return Err(fail(format!(
                "semantic kernel embeds excluded test/doc source {path}"
            )));
        }
    }
    Ok((inventory, excluded))
}

struct Scanner<'a> {
    workspace: &'a Path,
    inventory: &'a mut InputInventory,
    unit: &'a UnitSelection,
    file: PathBuf,
    directory: PathBuf,
    attribute_directory: PathBuf,
    stack: Vec<PathBuf>,
    seen: BTreeSet<(PathBuf, PathBuf)>,
    excluded: &'a mut BTreeSet<String>,
    macros: BTreeSet<String>,
    semantic: bool,
    extraction_planning: bool,
    error: Option<crate::InputError>,
}
impl Scanner<'_> {
    fn file(&mut self, path: &Path, root: bool) -> Result<()> {
        self.file_with_directory(path, root, None)
    }
    fn file_with_directory(
        &mut self,
        path: &Path,
        root: bool,
        include_directory: Option<PathBuf>,
    ) -> Result<()> {
        if self.stack.iter().any(|active| active == path) {
            return Err(fail(format!("cyclic module/include: {}", path.display())));
        }
        let relative = relative(self.workspace, path)?;
        let path = checked_path(self.workspace, Path::new(&relative))?;
        let directory = include_directory.unwrap_or_else(|| {
            if root || path.file_name().is_some_and(|name| name == "mod.rs") {
                path.parent().unwrap().to_path_buf()
            } else {
                path.parent().unwrap().join(path.file_stem().unwrap())
            }
        });
        if !self.seen.insert((path.clone(), directory.clone())) {
            return Ok(());
        }
        self.inventory.add_file(
            self.workspace,
            &relative,
            &self.unit.package,
            if self.unit.controller {
                InputRole::BuildController
            } else {
                InputRole::Rust
            },
        )?;
        let contents = std::fs::read_to_string(&path).map_err(fail)?;
        let file = syn::parse_file(&contents)
            .map_err(|e| fail(format!("parse {}: {e}", path.display())))?;
        let prior_file = std::mem::replace(&mut self.file, path.clone());
        let prior_directory = std::mem::replace(&mut self.directory, directory);
        let prior_attribute_directory = std::mem::replace(
            &mut self.attribute_directory,
            path.parent().unwrap().to_path_buf(),
        );
        self.stack.push(path);
        let prior_macros = self.macros.clone();
        self.visit_file(&file);
        self.macros = prior_macros;
        self.stack.pop();
        self.file = prior_file;
        self.directory = prior_directory;
        self.attribute_directory = prior_attribute_directory;
        self.error.take().map_or(Ok(()), Err)
    }
    fn condition(&self, condition: &Meta) -> Result<bool> {
        if self.semantic {
            Ok(self.unit.cfg.portable(condition)?.yes)
        } else {
            self.unit.cfg.evaluate(condition)
        }
    }
    fn attributes(&self, attrs: &[Attribute]) -> Result<Option<Vec<Meta>>> {
        let mut active = Vec::new();
        let mut pending: Vec<_> = attrs.iter().map(|attr| attr.meta.clone()).rev().collect();
        while let Some(meta) = pending.pop() {
            if meta.path().is_ident("cfg") {
                let Meta::List(list) = &meta else {
                    return Err(fail("invalid cfg attribute"));
                };
                let condition = syn::parse2(list.tokens.clone()).map_err(fail)?;
                if !self.condition(&condition)? {
                    return Ok(None);
                }
            } else if meta.path().is_ident("cfg_attr") {
                let Meta::List(list) = &meta else {
                    return Err(fail("invalid cfg_attr"));
                };
                let mut values = Punctuated::<Meta, Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
                    .map_err(fail)?
                    .into_iter();
                let condition = values.next().ok_or_else(|| fail("empty cfg_attr"))?;
                if self.condition(&condition)? {
                    pending.extend(values.collect::<Vec<_>>().into_iter().rev());
                }
            } else {
                active.push(meta);
            }
        }
        Ok(Some(active))
    }
    fn module_path(&self, module: &syn::ItemMod, attrs: &[Meta]) -> Result<PathBuf> {
        let selected: Vec<_> = attrs
            .iter()
            .filter(|meta| meta.path().is_ident("path"))
            .collect();
        if selected.len() > 1 {
            return Err(fail("ambiguous module path attributes"));
        }
        if let Some(Meta::NameValue(pair)) = selected.first().copied() {
            return self.resolve(
                &self
                    .attribute_directory
                    .join(crate::cfg::string(&pair.value)?),
            );
        }
        let file = self.directory.join(format!("{}.rs", module.ident));
        let nested = self.directory.join(module.ident.to_string()).join("mod.rs");
        match (
            file.try_exists().map_err(fail)?,
            nested.try_exists().map_err(fail)?,
        ) {
            (true, false) => self.resolve(&file),
            (false, true) => self.resolve(&nested),
            _ => Err(fail(format!(
                "missing or ambiguous module {} declared by {}",
                module.ident,
                self.file.display()
            ))),
        }
    }
    fn resolve(&self, path: &Path) -> Result<PathBuf> {
        let rel = path.strip_prefix(self.workspace).map_err(fail)?;
        checked_path(self.workspace, rel)
    }
    fn expression(&self, expression: &Expr) -> Result<String> {
        match expression {
            Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) => Ok(value.value()),
            Expr::Macro(expr) if expr.mac.path.is_ident("concat") => {
                let args = Punctuated::<Expr, Token![,]>::parse_terminated
                    .parse2(expr.mac.tokens.clone())
                    .map_err(fail)?;
                args.iter()
                    .map(|arg| self.expression(arg))
                    .collect::<Result<Vec<_>>>()
                    .map(|values| values.concat())
            }
            Expr::Macro(expr) if expr.mac.path.is_ident("env") => {
                let name: syn::LitStr = syn::parse2(expr.mac.tokens.clone()).map_err(fail)?;
                match name.value().as_str() {
                    "CARGO_MANIFEST_DIR" => Ok(self
                        .workspace
                        .join(
                            self.unit
                                .manifest
                                .as_ref()
                                .ok_or_else(|| fail("missing manifest owner"))?,
                        )
                        .parent()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()),
                    "OUT_DIR" => Ok("<generated>".to_owned()),
                    other => Err(fail(format!(
                        "compile-time path environment {other} has no input owner"
                    ))),
                }
            }
            _ => Err(fail(format!(
                "unresolved include expression in {}",
                self.file.display()
            ))),
        }
    }
    fn included(&mut self, name: &str, tokens: TokenStream) -> Result<()> {
        let expression: Expr = syn::parse2(tokens).map_err(fail)?;
        let value = self.expression(&expression)?;
        if let Some(generated) = value.strip_prefix("<generated>/") {
            if self.semantic
                && generated == "native_semantic_sources.rs"
                && self.unit.manifest.as_deref() == Some("crates/logic/Cargo.toml")
            {
                return Ok(());
            }
            if self.extraction_planning
                && generated == "native_semantic_sources.rs"
                && self.unit.manifest.as_deref() == Some("crates/logic/Cargo.toml")
            {
                for path in crate::native_extraction_paths(self.workspace)? {
                    self.inventory.add_file(
                        self.workspace,
                        &path,
                        "native-semantic-kernel",
                        InputRole::Rust,
                    )?;
                }
                return Ok(());
            }
            return crate::embedded::generated(
                self.workspace,
                self.inventory,
                self.unit,
                generated,
            );
        }
        let path = self.file.parent().unwrap().join(value);
        let path = self.resolve(&path)?;
        if name == "include" {
            self.file_with_directory(&path, false, Some(self.directory.clone()))?;
        } else {
            self.inventory.add_file(
                self.workspace,
                &relative(self.workspace, &path)?,
                &self.unit.package,
                InputRole::Embedded,
            )?;
        }
        Ok(())
    }
    fn tokens(&mut self, tokens: TokenStream) -> Result<()> {
        let tokens: Vec<_> = tokens.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            if let (
                Some(TokenTree::Ident(name)),
                Some(TokenTree::Punct(bang)),
                Some(TokenTree::Group(group)),
            ) = (
                tokens.get(index),
                tokens.get(index + 1),
                tokens.get(index + 2),
            ) {
                // A keyword followed by unary negation (for example
                // `if !($condition)`) is not a macro path. Keep visiting its
                // groups so nested input-bearing expressions are still owned.
                if bang.as_char() == '!'
                    && syn::parse2::<syn::Ident>(TokenStream::from(TokenTree::Ident(name.clone())))
                        .is_ok()
                {
                    self.macro_input(&name.to_string(), group.stream())?;
                    index += 3;
                    continue;
                }
            }
            if let TokenTree::Group(group) = &tokens[index] {
                self.tokens(group.stream())?;
            }
            index += 1;
        }
        Ok(())
    }
    fn macro_input(&mut self, name: &str, tokens: TokenStream) -> Result<()> {
        match name {
            "include" | "include_str" | "include_bytes" => self.included(name, tokens),
            "env" | "option_env" => {
                let arguments = Punctuated::<Expr, Token![,]>::parse_terminated
                    .parse2(tokens)
                    .map_err(fail)?;
                let name = crate::cfg::string(
                    arguments
                        .first()
                        .ok_or_else(|| fail("compile-time environment macro has no name"))?,
                )?;
                let owner = crate::embedded::environment_owner(self.unit, &name)?;
                self.inventory
                    .environment_owners
                    .entry(name)
                    .or_default()
                    .insert(owner);
                Ok(())
            }
            // syn's Token! constructs a punctuation/keyword token type; its
            // implementation is bound by this exact selected dependency.
            "Token" if self.unit.dependency_names.iter().any(|name| name == "syn") => {
                self.tokens(tokens)
            }
            // MiniJinja's context constructor expands only supplied values;
            // its exact selected registry implementation owns this capability.
            "context"
                if self
                    .unit
                    .dependency_names
                    .iter()
                    .any(|name| name == "minijinja") =>
            {
                self.tokens(tokens)
            }
            "macro_rules" => {
                if contains_source_declaration(tokens.clone()) {
                    Err(fail(format!(
                        "source-producing declarative macro needs an explicit owner in {}",
                        self.file.display()
                    )))
                } else {
                    self.tokens(tokens)
                }
            }
            // These syntax-only macros are bound to the selected std/registry/local
            // package implementation. Their nested include expressions still count.
            "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
            | "vec"
            | "format"
            | "format_args"
            | "write"
            | "writeln"
            | "print"
            | "println"
            | "eprint"
            | "eprintln"
            | "panic"
            | "unreachable"
            | "unimplemented"
            | "matches"
            | "stringify"
            | "concat"
            | "cfg"
            | "file"
            | "line"
            | "column"
            | "module_path"
            | "thread_local"
            | "json"
            | "define_diag_kind"
            | "assert_not_impl_all"
            | "info"
            | "warn"
            | "debug"
            | "trace"
            | "error"
            | "bail"
            | "ensure"
            | "quote"
            | "parse_macro_input"
            | "submit" => self.tokens(tokens),
            local if self.macros.contains(local) => self.tokens(tokens),
            _ => Err(fail(format!(
                "macro {name}! has no selected source-input capability owner in {}",
                self.file.display()
            ))),
        }
    }
    fn inactive(&mut self, item: &Item, attrs: &[Attribute]) -> Result<()> {
        let mentions_test = attrs.iter().any(|attr| meta_mentions_test(&attr.meta));
        if mentions_test {
            // Registration is whole-file-hashed wiring. Implementations remain
            // behind external test modules; reexporting them does not embed them.
            if matches!(item, Item::Use(_)) {
                return Ok(());
            }
            if let Item::Mod(module) = item {
                if module.content.is_none() {
                    let active: Vec<_> = attrs
                        .iter()
                        .filter(|attr| !attr.path().is_ident("cfg"))
                        .map(|attr| attr.meta.clone())
                        .collect();
                    if let Ok(path) = self.module_path(module, &active) {
                        excluded_module(
                            self.workspace,
                            &path,
                            self.excluded,
                            &mut BTreeSet::new(),
                        )?;
                    }
                    return Ok(());
                }
            }
            if self.extraction_planning {
                return Ok(());
            }
            return Err(fail(format!(
                "inline test implementation must be extracted from {}",
                self.file.display()
            )));
        }
        Ok(())
    }
}
impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if self.error.is_some() {
            return;
        }
        let attrs = item_attributes(item);
        if attrs.iter().any(|attribute| {
            attribute.path().is_ident("test") || attribute.path().is_ident("bench")
        }) {
            if self.extraction_planning {
                return;
            }
            self.error = Some(fail(format!(
                "standalone test implementation must be extracted from {}",
                self.file.display()
            )));
            return;
        }
        if matches!(item, Item::Verbatim(_)) {
            self.error = Some(fail(format!(
                "unsupported Rust item in {}",
                self.file.display()
            )));
            return;
        }
        match self.attributes(attrs) {
            Ok(Some(effective)) => {
                for meta in effective {
                    if let Meta::NameValue(pair) = meta {
                        if pair.path.is_ident("doc") {
                            self.visit_expr(&pair.value);
                        }
                    }
                }
                visit::visit_item(self, item)
            }
            Ok(None) => {
                if let Err(e) = self.inactive(item, attrs) {
                    self.error = Some(e)
                }
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
        let result = (|| {
            let Some(attrs) = self.attributes(&module.attrs)? else {
                return Ok(());
            };
            if let Some((_, items)) = &module.content {
                let previous = self.directory.clone();
                let previous_macros = self.macros.clone();
                let previous_attribute = self.attribute_directory.clone();
                let paths: Vec<_> = attrs
                    .iter()
                    .filter(|meta| meta.path().is_ident("path"))
                    .collect();
                if paths.len() > 1 {
                    return Err(fail("ambiguous inline module path attributes"));
                }
                self.directory = if let Some(Meta::NameValue(pair)) = paths.first().copied() {
                    self.resolve(
                        &self
                            .attribute_directory
                            .join(crate::cfg::string(&pair.value)?),
                    )?
                } else {
                    self.directory.join(module.ident.to_string())
                };
                self.attribute_directory = self.directory.clone();
                for item in items {
                    self.visit_item(item);
                }
                self.directory = previous;
                self.macros = previous_macros;
                self.attribute_directory = previous_attribute;
                Ok(())
            } else {
                let path = self.module_path(module, &attrs)?;
                self.file(&path, false)
            }
        })();
        if let Err(e) = result {
            self.error = Some(e);
        }
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        let previous = self.macros.clone();
        visit::visit_block(self, block);
        self.macros = previous;
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if item.mac.path.is_ident("macro_rules") {
            if let Some(name) = &item.ident {
                self.macros.insert(name.to_string());
            }
        }
        visit::visit_item_macro(self, item);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if self.error.is_some() {
            return;
        }
        let name = mac
            .path
            .segments
            .last()
            .map(|part| part.ident.to_string())
            .unwrap_or_default();
        if let Err(e) = self.macro_input(&name, mac.tokens.clone()) {
            self.error = Some(e);
        }
    }
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        if self.error.is_some() {
            return;
        }
        if self.extraction_planning && meta_mentions_test(&attr.meta) {
            return;
        }
        if attr.path().is_ident("cfg_attr") && meta_mentions_test(&attr.meta) {
            self.error = Some(fail(format!(
                "test-dependent inline attribute must be extracted from {}",
                self.file.display()
            )));
            return;
        }
        if attr.path().is_ident("cfg") && meta_mentions_test(&attr.meta) {
            match self.attributes(std::slice::from_ref(attr)) {
                Ok(None) => {
                    self.error = Some(fail(format!(
                        "inline test implementation must be extracted from {}",
                        self.file.display()
                    )));
                    return;
                }
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
                _ => {}
            }
        }
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(pair) = &attr.meta {
                self.visit_expr(&pair.value);
            }
        }
    }
    fn visit_expr(&mut self, expression: &'ast Expr) {
        if self.error.is_some() {
            return;
        }
        let attrs = expression_attributes(expression);
        match self.attributes(attrs) {
            Ok(None) => {
                if !self.extraction_planning
                    && attrs.iter().any(|attr| meta_mentions_test(&attr.meta))
                {
                    self.error = Some(fail(format!(
                        "inline test expression must be extracted from {}",
                        self.file.display()
                    )));
                }
            }
            Ok(Some(_)) => visit::visit_expr(self, expression),
            Err(error) => self.error = Some(error),
        }
    }
    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attrs = match item {
            syn::ImplItem::Const(v) => &v.attrs,
            syn::ImplItem::Fn(v) => &v.attrs,
            syn::ImplItem::Type(v) => &v.attrs,
            syn::ImplItem::Macro(v) => &v.attrs,
            _ => {
                visit::visit_impl_item(self, item);
                return;
            }
        };
        match self.attributes(attrs) {
            Ok(None)
                if !self.extraction_planning
                    && attrs.iter().any(|attr| meta_mentions_test(&attr.meta)) =>
            {
                self.error = Some(fail(format!(
                    "inline test impl item must be extracted from {}",
                    self.file.display()
                )));
            }
            Ok(None) => {}
            Ok(Some(_)) => visit::visit_impl_item(self, item),
            Err(error) => self.error = Some(error),
        }
    }
    fn visit_field(&mut self, field: &'ast syn::Field) {
        match self.attributes(&field.attrs) {
            Ok(None) => {
                if !self.extraction_planning
                    && field
                        .attrs
                        .iter()
                        .any(|attr| meta_mentions_test(&attr.meta))
                {
                    self.error = Some(fail(format!(
                        "test-only field must move behind a test accessor: {}",
                        self.file.display()
                    )));
                }
            }
            Ok(Some(_)) => visit::visit_field(self, field),
            Err(error) => self.error = Some(error),
        }
    }
}
fn item_attributes(item: &Item) -> &[Attribute] {
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
fn meta_mentions_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => {
            path.is_ident("test") || path.is_ident("doc") || path.is_ident("doctest")
        }
        Meta::List(list) if list.path.is_ident("cfg") => tokens_mention_test(list.tokens.clone()),
        Meta::List(list) if list.path.is_ident("cfg_attr") => {
            let Ok(values) =
                Punctuated::<Meta, Token![,]>::parse_terminated.parse2(list.tokens.clone())
            else {
                return false;
            };
            let mut values = values.iter();
            let condition_mentions_test = match values.next() {
                Some(Meta::Path(path)) => {
                    path.is_ident("test") || path.is_ident("doc") || path.is_ident("doctest")
                }
                Some(Meta::List(condition)) => tokens_mention_test(condition.tokens.clone()),
                _ => false,
            };
            condition_mentions_test || values.any(meta_mentions_test)
        }
        _ => false,
    }
}
fn tokens_mention_test(tokens: TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Ident(name) => name == "test" || name == "doc" || name == "doctest",
        TokenTree::Group(group) => tokens_mention_test(group.stream()),
        _ => false,
    })
}
fn contains_source_declaration(tokens: TokenStream) -> bool {
    let tokens: Vec<_> = tokens.into_iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Ident(name)
                if matches!(
                    name.to_string().as_str(),
                    "mod" | "include" | "include_str" | "include_bytes"
                ) =>
            {
                return true;
            }
            TokenTree::Group(group) if contains_source_declaration(group.stream()) => return true,
            TokenTree::Punct(dollar) if dollar.as_char() == '$' => {
                if matches!(tokens.get(index + 1), Some(TokenTree::Ident(_)))
                    && matches!(tokens.get(index + 2), Some(TokenTree::Punct(bang)) if bang.as_char() == '!')
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Classify the external test module tree by syntax, never by its directory name.
/// Only path ownership is recorded; test bytes do not participate in any identity.
fn excluded_module(
    workspace: &Path,
    path: &Path,
    excluded: &mut BTreeSet<String>,
    active: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    let path = checked_path(workspace, Path::new(&relative(workspace, path)?))?;
    if !active.insert(path.clone()) {
        return Err(fail(format!("cyclic excluded module {}", path.display())));
    }
    excluded.insert(relative(workspace, &path)?);
    let source = std::fs::read_to_string(&path).map_err(fail)?;
    let syntax = syn::parse_file(&source).map_err(fail)?;
    let directory = if path.file_name().is_some_and(|name| name == "mod.rs") {
        path.parent().unwrap().to_path_buf()
    } else {
        path.parent().unwrap().join(path.file_stem().unwrap())
    };
    excluded_children(
        workspace,
        &syntax.items,
        &directory,
        path.parent().unwrap(),
        excluded,
        active,
    )?;
    active.remove(&path);
    Ok(())
}
fn excluded_children(
    workspace: &Path,
    items: &[Item],
    directory: &Path,
    attribute_directory: &Path,
    excluded: &mut BTreeSet<String>,
    active: &mut BTreeSet<PathBuf>,
) -> Result<()> {
    for item in items {
        if let Item::Mod(module) = item {
            let explicit = module
                .attrs
                .iter()
                .find_map(|attr| match &attr.meta {
                    Meta::NameValue(pair) if pair.path.is_ident("path") => {
                        Some(crate::cfg::string(&pair.value))
                    }
                    _ => None,
                })
                .transpose()?;
            let nested = explicit.as_ref().map_or_else(
                || directory.join(module.ident.to_string()),
                |path| attribute_directory.join(path),
            );
            if let Some((_, items)) = &module.content {
                excluded_children(workspace, items, &nested, &nested, excluded, active)?;
            } else {
                let candidates = if explicit.is_some() {
                    vec![nested]
                } else {
                    vec![
                        directory.join(format!("{}.rs", module.ident)),
                        nested.join("mod.rs"),
                    ]
                };
                for candidate in candidates {
                    if candidate.try_exists().map_err(fail)? {
                        excluded_module(workspace, &candidate, excluded, active)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn expression_attributes(expression: &Expr) -> &[Attribute] {
    match expression {
        Expr::Array(v) => &v.attrs,
        Expr::Assign(v) => &v.attrs,
        Expr::Async(v) => &v.attrs,
        Expr::Await(v) => &v.attrs,
        Expr::Binary(v) => &v.attrs,
        Expr::Block(v) => &v.attrs,
        Expr::Break(v) => &v.attrs,
        Expr::Call(v) => &v.attrs,
        Expr::Cast(v) => &v.attrs,
        Expr::Closure(v) => &v.attrs,
        Expr::Const(v) => &v.attrs,
        Expr::Continue(v) => &v.attrs,
        Expr::Field(v) => &v.attrs,
        Expr::ForLoop(v) => &v.attrs,
        Expr::Group(v) => &v.attrs,
        Expr::If(v) => &v.attrs,
        Expr::Index(v) => &v.attrs,
        Expr::Infer(v) => &v.attrs,
        Expr::Let(v) => &v.attrs,
        Expr::Lit(v) => &v.attrs,
        Expr::Loop(v) => &v.attrs,
        Expr::Macro(v) => &v.attrs,
        Expr::Match(v) => &v.attrs,
        Expr::MethodCall(v) => &v.attrs,
        Expr::Paren(v) => &v.attrs,
        Expr::Path(v) => &v.attrs,
        Expr::Range(v) => &v.attrs,
        Expr::RawAddr(v) => &v.attrs,
        Expr::Reference(v) => &v.attrs,
        Expr::Repeat(v) => &v.attrs,
        Expr::Return(v) => &v.attrs,
        Expr::Struct(v) => &v.attrs,
        Expr::Try(v) => &v.attrs,
        Expr::TryBlock(v) => &v.attrs,
        Expr::Tuple(v) => &v.attrs,
        Expr::Unary(v) => &v.attrs,
        Expr::Unsafe(v) => &v.attrs,
        Expr::While(v) => &v.attrs,
        Expr::Yield(v) => &v.attrs,
        _ => &[],
    }
}
