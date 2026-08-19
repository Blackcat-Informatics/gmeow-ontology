// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The test-gated contract for everything this repository publishes to npm.
//!
//! # The names are the shipped bytes
//!
//! There is no list of package names in this file. The published set is DISCOVERED by
//! walking the repository for `package.json` files and keeping those that do not declare
//! themselves `"private": true` — so the dev-only Playwright smoke manifest excludes
//! itself, by its own declaration, and a new package cannot be forgotten by a registry
//! nobody remembered to update. Every assertion below is then quantified over that
//! discovered set.
//!
//! # What is gated
//!
//! * every published package is scoped, carries the workspace version, and declares the
//!   metadata a public registry entry needs;
//! * export-set EQUALITY (not a substring probe) across the crate's `#[wasm_bindgen]`
//!   surface, the package entry's re-exports, and the hand-written `.d.ts`;
//! * Playwright appears in exactly one manifest — the dev-only smoke one — and in no
//!   published package's dependency graph at all;
//! * the release workflow publishes fail-closed: it installs node, the wasm32 target and
//!   the SAME pinned `wasm-bindgen-cli` / binaryen `ci.yml` uses, hard-fails by NAME on a
//!   missing `NPM_TOKEN`, and runs the native≡wasm parity lanes BEFORE `npm publish` —
//!   the ORDERING is asserted, not merely the presence of both steps;
//! * the Makefile carries the network-safe dry-run lane and the consumability lane;
//! * the documented CDN templates name EXACTLY the discovered package set — a package
//!   added or renamed without a documentation edit is a hard failure;
//! * neither the console nor the distribution design promises runtime CDN loading.
//!
//! The engine leg that needs the BUILT wasm-bindgen output (`pkg/<module>.d.ts`) rides the
//! per-package Node lane `crates/*/js/tests/exports.test.mjs`, which runs under
//! `make wasm-parity` where those bytes are guaranteed to exist. This file asserts the
//! same equality against the crate source that GENERATES them, so the contract is live on
//! the always-on Rust gate too.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

// ── repo-anchored readers (mirrors make_gate_contract.rs) ──────────────────────────

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is under <repo>/crates")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

fn makefile() -> String {
    read("Makefile")
}

fn ci_workflow() -> String {
    read(".github/workflows/ci.yml")
}

fn release_workflow() -> String {
    read(".github/workflows/release.yml")
}

/// Directory names never descended into when discovering packages. Mirrors the `PRUNED`
/// set in `scripts/npm-packaging.mjs`; the two discoveries must see the same tree.
const PRUNED: &[&str] = &[
    ".git",
    ".worktrees",
    "node_modules",
    "target",
    "dist",
    "generated",
    "ontology-docs",
    "coverage",
    "imports",
    ".venv",
    ".cache",
];

fn walk_manifests(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            if PRUNED.contains(&name.to_string_lossy().as_ref()) {
                continue;
            }
            walk_manifests(&path, found);
        } else if entry.file_name() == "package.json" {
            found.push(path);
        }
    }
}

/// One discovered `package.json`: its repo-relative path, its directory, and its parsed
/// contents.
struct Manifest {
    rel: String,
    dir: PathBuf,
    json: serde_json::Value,
}

impl Manifest {
    fn name(&self) -> &str {
        self.json["name"].as_str().expect("package.json has a name")
    }

    fn is_private(&self) -> bool {
        self.json["private"].as_bool() == Some(true)
    }

    /// A VS Code extension manifest: published by `vsce` to the Visual Studio
    /// Marketplace, not to the npm registry.
    fn is_marketplace_extension(&self) -> bool {
        !self.json["engines"]["vscode"].is_null()
    }
}

/// Every `package.json` in the repository, sorted by path.
fn all_manifests() -> Vec<Manifest> {
    let root = repo_root();
    let mut found = Vec::new();
    walk_manifests(&root, &mut found);
    found.sort();
    found
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            Manifest {
                rel: path
                    .strip_prefix(&root)
                    .expect("manifest is under the repo root")
                    .to_string_lossy()
                    .into_owned(),
                dir: path.parent().expect("a file has a parent").to_path_buf(),
                json: serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("parse {}: {e}", path.display())),
            }
        })
        .collect()
}

/// The manifests this repository publishes TO THE NPM REGISTRY.
///
/// Two exclusions, both read from the manifest's own bytes rather than from a name list:
/// `"private": true` (the dev-only Playwright smoke manifest excludes itself) and
/// `engines.vscode` (a VS Code extension, published by `vsce` to the Visual Studio
/// Marketplace on that registry's own cadence and metadata contract). Mirrors
/// `publishedPackages()` in `scripts/npm-packaging.mjs`; the two must agree.
fn published() -> Vec<Manifest> {
    all_manifests()
        .into_iter()
        .filter(|m| !m.is_private() && !m.is_marketplace_extension())
        .collect()
}

/// The workspace version — the single authority every published package must carry.
fn workspace_version() -> String {
    let root: toml::Value = toml::from_str(&read("Cargo.toml")).expect("parse root Cargo.toml");
    root["workspace"]["package"]["version"]
        .as_str()
        .expect("[workspace.package] declares a version")
        .to_string()
}

// ── JS / TypeScript surface parsing ───────────────────────────────────────────────

/// Strip line and block comments so a commented-out `export` is never counted.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.lines()
        .map(|line| match line.trim_start().starts_with("//") {
            true => "",
            false => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn identifier_after(rest: &str) -> Option<String> {
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    // A JS identifier never starts with a digit — the JS twin's `[A-Za-z_$][\w$]*` says so
    // too, and the two halves are held to agreeing by `the_two_esm_parsers_agree_…`.
    let starts_valid = name
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$');
    starts_valid.then_some(name)
}

/// The byte offsets of every `export` keyword that opens a statement — i.e. one preceded,
/// back to the start of its line, by nothing but whitespace, and followed by whitespace or
/// `{`.
///
/// This is the Rust spelling of the JS twin's `^\s*export` anchor (`RE_EXPORT_BLOCK` and
/// `DECLARED` in `scripts/npm-packaging.mjs` both use it). Anchoring matters: an unanchored
/// `find("export {")` also matches inside an expression, and the two halves would then
/// disagree about the same bytes.
fn statement_export_offsets(clean: &str) -> Vec<usize> {
    clean
        .match_indices("export")
        .filter(|(index, _)| {
            let head = &clean[..*index];
            let line_start = head.rfind('\n').map_or(0, |n| n + 1);
            head[line_start..].chars().all(char::is_whitespace)
        })
        .filter(|(index, _)| {
            let tail = &clean[index + "export".len()..];
            tail.starts_with(|c: char| c.is_whitespace() || c == '{')
        })
        .map(|(index, _)| index)
        .collect()
}

/// The set of VALUE names a module (or declaration file) exports.
///
/// Type-only declarations (`export interface`, `export type`) are excluded: they have no
/// runtime counterpart, so counting them would make a `.d.ts` carrying an interface
/// unequal to the module it describes for a reason that is not a defect.
///
/// Whitespace-TOLERANT in exactly the places the JS twin is (`\s+` between tokens, `\s*`
/// before a re-export brace, so `export{a}` and a declaration split across lines both
/// parse). The two used to differ there — Rust demanded a single space and `export {`,
/// the JS regex allowed any whitespace and `export{` — with nothing asserting they agreed;
/// `the_two_esm_parsers_agree_over_the_shared_corpus` now does.
fn exported_value_names(source: &str) -> BTreeSet<String> {
    const DECLARATION_KEYWORDS: [&str; 5] = ["function", "class", "const", "let", "var"];
    let clean = strip_comments(source);
    let mut names = BTreeSet::new();
    for index in statement_export_offsets(&clean) {
        let tail = clean[index + "export".len()..].trim_start();
        // `export { a, b as c };` — the EXPORTED name is the alias when one is present.
        if let Some(list) = tail.strip_prefix('{') {
            let Some(end) = list.find('}') else { continue };
            for clause in list[..end].split(',') {
                let clause = clause.trim();
                if clause.is_empty() {
                    continue;
                }
                let exported = match clause.split_whitespace().collect::<Vec<_>>().as_slice() {
                    [_, "as", alias, ..] => (*alias).to_string(),
                    [name, ..] => (*name).to_string(),
                    [] => continue,
                };
                names.insert(exported);
            }
            continue;
        }
        // `export [declare] [async] <keyword> <name>`.
        let mut tokens = tail.split_whitespace();
        let mut token = tokens.next();
        for optional in ["declare", "async"] {
            if token == Some(optional) {
                token = tokens.next();
            }
        }
        if !token.is_some_and(|t| DECLARATION_KEYWORDS.contains(&t)) {
            continue;
        }
        if let Some(name) = tokens.next().and_then(identifier_after) {
            names.insert(name);
        }
    }
    names
}

/// The `{ source -> local }` map of names a module imports from its colocated
/// wasm-bindgen bindings. The default import (wasm-bindgen's own `init`) is not an engine
/// export and is excluded by construction: only the braced clause is read.
fn engine_imports(source: &str) -> BTreeMap<String, String> {
    let clean = strip_comments(source);
    let mut map = BTreeMap::new();
    let Some(from) = clean.find("from \"./pkg/") else {
        return map;
    };
    let head = &clean[..from];
    let Some(open) = head.rfind('{') else {
        return map;
    };
    let Some(close) = head[open..].find('}') else {
        return map;
    };
    for clause in head[open + 1..open + close].split(',') {
        let clause = clause.trim();
        if clause.is_empty() {
            continue;
        }
        // Tokenized rather than split on the literal `" as "`, so a clause wrapped across
        // lines reads the same here as it does under the JS twin's `/\s+as\s+/`.
        let (src, local) = match clause.split_whitespace().collect::<Vec<_>>().as_slice() {
            [source, "as", alias, ..] => ((*source).to_string(), (*alias).to_string()),
            [name, ..] => ((*name).to_string(), (*name).to_string()),
            [] => continue,
        };
        map.insert(src, local);
    }
    map
}

/// The set of names a crate exports to JavaScript: every `pub fn` carrying a
/// `#[wasm_bindgen]` attribute. This is the GENERATOR of `pkg/<module>.d.ts`, so asserting
/// against it is asserting against the wasm-bindgen export set without needing the wasm
/// build; the per-package Node lane asserts the generated `.d.ts` itself.
fn wasm_bindgen_exports(lib_rs: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut armed = false;
    for line in lib_rs.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[wasm_bindgen") {
            armed = true;
            continue;
        }
        if !armed {
            continue;
        }
        if trimmed.starts_with("///") || trimmed.starts_with("#[") {
            continue;
        }
        if let Some(tail) = trimmed.strip_prefix("pub fn ")
            && let Some(name) = identifier_after(tail)
        {
            names.insert(name);
        }
        armed = false;
    }
    names
}

// ── 1. the published set ──────────────────────────────────────────────────────────

/// The repository publishes at least one package, and every published manifest is scoped
/// under the organization, so no unscoped name can be squatted or collide.
#[test]
fn every_published_package_is_scoped_and_public() {
    let packages = published();
    assert!(
        !packages.is_empty(),
        "the repository must publish at least one npm package"
    );
    for pkg in &packages {
        assert!(
            pkg.name().starts_with("@blackcatinformatics/"),
            "{}: published package `{}` is not scoped under @blackcatinformatics/",
            pkg.rel,
            pkg.name()
        );
        assert_eq!(
            pkg.json["publishConfig"]["access"].as_str(),
            Some("public"),
            "{}: a scoped package defaults to a RESTRICTED publish; \
             `publishConfig.access` must say `public` explicitly",
            pkg.rel
        );
        for field in [
            "description",
            "license",
            "author",
            "repository",
            "files",
            "exports",
            "types",
        ] {
            assert!(
                !pkg.json[field].is_null(),
                "{}: published package is missing `{field}`",
                pkg.rel
            );
        }
        assert_eq!(
            pkg.json["license"].as_str(),
            Some("AGPL-3.0-only"),
            "{}: published package must carry the repository license",
            pkg.rel
        );
    }
}

/// The exclusions from the npm set are DELIBERATE and each has a reason readable in the
/// excluded manifest's own bytes — never an accident of a walk that missed a directory.
#[test]
fn every_excluded_manifest_declares_why_it_is_excluded() {
    for manifest in all_manifests() {
        if !manifest.is_private() && !manifest.is_marketplace_extension() {
            continue;
        }
        if manifest.is_marketplace_extension() {
            assert!(
                !manifest.json["publisher"].is_null(),
                "{}: a manifest excluded as a VS Code extension must carry the \
                 `publisher` the Marketplace requires",
                manifest.rel
            );
            assert!(
                !manifest.is_private(),
                "{}: a Marketplace extension is published, so it must not also claim to \
                 be private",
                manifest.rel
            );
            continue;
        }
        assert!(
            manifest.is_private(),
            "{}: excluded manifests are excluded by `private: true` or by being a VS Code \
             extension, and this one is neither",
            manifest.rel
        );
    }
}

/// EVERY published package version equals the workspace version. A drifting npm version
/// is a package that claims to be an engine it is not.
#[test]
fn every_published_package_carries_the_workspace_version() {
    let expected = workspace_version();
    for pkg in published() {
        assert_eq!(
            pkg.json["version"].as_str(),
            Some(expected.as_str()),
            "{}: package version drifted from the workspace version",
            pkg.rel
        );
    }
}

/// Every file a package declares in `files` exists, and every `exports` target is one of
/// them — a manifest that packs a path it does not ship is a broken tarball.
#[test]
fn declared_files_exist_and_cover_every_exports_target() {
    for pkg in published() {
        let files: BTreeSet<String> = pkg.json["files"]
            .as_array()
            .expect("`files` is an array")
            .iter()
            .map(|v| v.as_str().expect("a files entry is a string").to_string())
            .collect();
        for file in &files {
            // `pkg/…` is the wasm-bindgen build output: git-ignored, produced by the
            // `*-wasm-pkg` lanes. Its presence is proven by `npm pack` in the dry-run
            // lane, not here.
            if file.starts_with("pkg/") {
                continue;
            }
            assert!(
                pkg.dir.join(file).is_file(),
                "{}: declared file `{file}` does not exist",
                pkg.rel
            );
        }
        let exports = pkg.json["exports"].as_object().expect("`exports` is a map");
        for (subpath, entry) in exports {
            for condition in ["import", "types"] {
                let target = entry[condition].as_str().unwrap_or_else(|| {
                    panic!("{}: exports[{subpath}] has no {condition}", pkg.rel)
                });
                let relative = target.trim_start_matches("./");
                assert!(
                    files.contains(relative),
                    "{}: exports[{subpath}].{condition} = `{target}` is not in `files`",
                    pkg.rel
                );
            }
        }
    }
}

// ── 2. export-set EQUALITY ────────────────────────────────────────────────────────

/// For EVERY published package and EVERY subpath its `exports` map declares, the module's
/// runtime export set and its `.d.ts` declared value set are the SAME set.
///
/// This is what catches a `bundle_dataset` that `index.mjs` exports and `index.d.ts`
/// never mentions: a typed consumer cannot reach it, so the capability is shipped and
/// invisible.
#[test]
fn runtime_exports_equal_the_declared_type_surface() {
    for pkg in published() {
        let exports = pkg.json["exports"].as_object().expect("`exports` is a map");
        for (subpath, entry) in exports {
            let module = std::fs::read_to_string(
                pkg.dir
                    .join(entry["import"].as_str().expect("import target")),
            )
            .expect("read the module entry");
            let types = std::fs::read_to_string(
                pkg.dir.join(entry["types"].as_str().expect("types target")),
            )
            .expect("read the declaration file");
            assert_eq!(
                exported_value_names(&module),
                exported_value_names(&types),
                "{} {subpath}: the module's runtime exports and its `types` declarations \
                 are not the same set",
                pkg.rel
            );
        }
    }
}

/// For every published package whose entry imports wasm-bindgen bindings, the set of
/// names it imports is EXACTLY the crate's `#[wasm_bindgen]` export set.
///
/// This is what catches a `glyph_legend` the engine exports and the package never
/// imports: shipped-but-unreachable capability, which no-optionality forbids.
#[test]
fn every_engine_export_is_imported_by_its_package() {
    let mut checked = 0usize;
    for pkg in published() {
        let entry_path = pkg.dir.join(
            pkg.json["exports"]["."]["import"]
                .as_str()
                .expect("a `.` export"),
        );
        let entry = std::fs::read_to_string(&entry_path).expect("read the package entry");
        let imports = engine_imports(&entry);
        if imports.is_empty() {
            continue;
        }
        // The crate directory is the package directory's parent: `crates/<crate>/js`.
        let lib_rs = pkg
            .dir
            .parent()
            .expect("js/ has a parent")
            .join("src/lib.rs");
        let engine = wasm_bindgen_exports(
            &std::fs::read_to_string(&lib_rs)
                .unwrap_or_else(|e| panic!("read {}: {e}", lib_rs.display())),
        );
        assert_eq!(
            imports.keys().cloned().collect::<BTreeSet<_>>(),
            engine,
            "{}: the package entry does not import EXACTLY the crate's #[wasm_bindgen] \
             export set — an engine export the package never imports is \
             shipped-but-unreachable capability",
            pkg.rel
        );

        // Every imported engine name must reach the consumer: re-exported under its own
        // name, or renamed through an `as` alias the module body actually uses. A rename
        // is a deliberate wrapper decision; a drop is a silent degradation.
        let exported = exported_value_names(&entry);
        let body = strip_comments(&entry);
        for (source, local) in &imports {
            if exported.contains(source) {
                continue;
            }
            assert_ne!(
                local, source,
                "{}: engine export `{source}` is imported but neither re-exported nor aliased",
                pkg.rel
            );
            assert!(
                body.matches(local.as_str()).count() > 1,
                "{}: engine export `{source}` is aliased to `{local}`, which the module \
                 body never uses — the capability is dropped, not renamed",
                pkg.rel
            );
        }
        checked += 1;
    }
    assert!(
        checked >= 1,
        "at least one published package must be a wasm engine package"
    );
}

/// The per-package Node lane that asserts the same equality against the GENERATED
/// wasm-bindgen `.d.ts` exists for every engine package, and the shared checker it drives
/// exists too. Without this, the engine leg could be deleted and the Rust half would
/// still pass while the strongest assertion silently stopped running.
#[test]
fn every_engine_package_carries_the_generated_dts_export_lane() {
    let checker = repo_root().join("scripts/npm-packaging.mjs");
    assert!(
        checker.is_file(),
        "the shared export-set checker scripts/npm-packaging.mjs must exist"
    );
    for pkg in published() {
        let entry = std::fs::read_to_string(
            pkg.dir.join(
                pkg.json["exports"]["."]["import"]
                    .as_str()
                    .expect("a `.` export"),
            ),
        )
        .expect("read the package entry");
        if engine_imports(&entry).is_empty() {
            continue;
        }
        let lane = pkg.dir.join("tests/exports.test.mjs");
        assert!(
            lane.is_file(),
            "{}: engine package has no tests/exports.test.mjs generated-.d.ts lane",
            pkg.rel
        );
        let source = std::fs::read_to_string(&lane).expect("read the export lane");
        assert!(
            source.contains("assertPackageExportSets"),
            "{}: the export lane must drive the shared checker, not a local re-implementation",
            pkg.rel
        );
    }
}

// ── 3. Playwright is dev-only ─────────────────────────────────────────────────────

/// Playwright appears in EXACTLY ONE manifest in the repository — the console's dev-only
/// smoke lane, which declares itself private — and in NO published package's dependency
/// graph of any kind. A browser driver in a shipped package is megabytes of dev tooling
/// installed onto every consumer.
#[test]
fn playwright_is_confined_to_the_private_smoke_manifest() {
    const DEP_FIELDS: &[&str] = &[
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ];
    let mut carriers = Vec::new();
    for manifest in all_manifests() {
        let mut mentions = false;
        for field in DEP_FIELDS {
            if let Some(map) = manifest.json[*field].as_object() {
                let hit = map.keys().any(|k| k.contains("playwright"));
                if hit {
                    mentions = true;
                    assert!(
                        manifest.is_private(),
                        "{}: a PUBLISHED package declares Playwright in `{field}`",
                        manifest.rel
                    );
                }
            }
        }
        if mentions {
            carriers.push(manifest.rel.clone());
        }
    }
    assert_eq!(
        carriers,
        vec!["crates/docs/assets/console/smoke/package.json".to_string()],
        "Playwright must be declared in exactly one manifest: the console's private \
         smoke lane"
    );
}

// ── 4. the release workflow publishes fail-closed, parity FIRST ───────────────────

fn line_index(source: &str, needle: &str) -> usize {
    source
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("release.yml carries a line containing `{needle}`"))
}

/// The EXECUTED publish command, not a mention of it in prose. Ordering assertions must
/// anchor on the real step, so a comment that happens to say "npm publish" can never make
/// a parity-after-publish workflow look ordered correctly.
const PUBLISH_COMMAND: &str = "npm publish --access public --provenance";

/// The publish step exists, is gated on the `v*` tag push the workflow already triggers
/// on, and runs `npm publish` with public access and provenance — composing with the
/// `id-token: write` permission the workflow already grants for attestation.
#[test]
fn release_publishes_scoped_packages_with_provenance() {
    let release = release_workflow();
    assert!(
        release.contains("tags: [\"v*\"]"),
        "the release workflow must still trigger on v* tags"
    );
    assert!(
        release.contains("--access public"),
        "a scoped package needs `npm publish --access public`"
    );
    assert!(
        release.contains("--provenance"),
        "the publish must emit an npm provenance attestation"
    );
    assert!(
        release.contains("id-token: write"),
        "npm provenance needs the `id-token: write` permission"
    );
    assert!(
        release.contains(PUBLISH_COMMAND),
        "the release workflow must actually publish"
    );
    // The dry-run flag must NOT reach the release publish: a release that quietly
    // published nothing is the worst kind of green.
    let publish_line = release
        .lines()
        .find(|line| line.contains(PUBLISH_COMMAND))
        .expect("a publish line");
    assert!(
        !publish_line.contains("--dry-run"),
        "the release publish must not be a dry run"
    );
}

/// The publish hard-fails when the token is absent, and NAMES the secret so the operator
/// knows what to configure — mirroring the existing GPG signing step's fail-closed shape.
#[test]
fn release_hard_fails_by_name_on_a_missing_npm_token() {
    let release = release_workflow();
    assert!(
        release.contains("secrets.NPM_TOKEN"),
        "the publish step must read secrets.NPM_TOKEN"
    );
    assert!(
        release.contains("::error::NPM_TOKEN secret is not configured"),
        "the missing-token failure must name the NPM_TOKEN secret, like the GPG step names \
         GMEOW_RELEASE_SIGNING_KEY"
    );
    let guard = line_index(&release, "::error::NPM_TOKEN secret is not configured");
    let publish = line_index(&release, PUBLISH_COMMAND);
    assert!(
        guard < publish,
        "the NPM_TOKEN guard must run BEFORE npm publish (guard at line {guard}, publish at \
         line {publish})"
    );
}

/// The native≡wasm parity lanes run BEFORE anything is published. This asserts the
/// ORDERING, not merely that both steps exist: publishing bytes that never passed parity
/// is exactly the failure mode a "both are present" check would wave through.
#[test]
fn release_runs_wasm_parity_before_publishing() {
    let release = release_workflow();
    let parity = line_index(&release, "make wasm-parity");
    let publish = line_index(&release, PUBLISH_COMMAND);
    assert!(
        parity < publish,
        "the wasm parity lanes must run BEFORE npm publish (parity at line {parity}, publish \
         at line {publish})"
    );
}

/// The publish job installs node and the wasm32 target, and pins `wasm-bindgen-cli` and
/// binaryen to the SAME versions `ci.yml` uses. The pins are READ from ci.yml, so a bump
/// there that forgets release.yml fails here rather than producing release bytes a
/// different toolchain generated.
#[test]
fn release_installs_the_same_pinned_wasm_toolchain_as_ci() {
    let ci = ci_workflow();
    let release = release_workflow();

    let wasm_bindgen_pin = ci
        .lines()
        .find_map(|line| line.trim().strip_prefix("tool: wasm-bindgen-cli@"))
        .expect("ci.yml pins wasm-bindgen-cli")
        .trim()
        .to_string();
    // ci.yml DERIVES the binaryen version from the Makefile (`make print-binaryen-ver`)
    // rather than repeating the literal, because the two drifted once and every wasm-opt
    // rule died on an unknown option. The contract is therefore that BOTH workflows derive
    // it the same way — a literal in either one is the drift this replaced.
    let binaryen_pin = "make --no-print-directory print-binaryen-ver".to_string();
    assert!(
        ci.contains(&binaryen_pin),
        "ci.yml must DERIVE the binaryen version from the Makefile, not repeat the literal"
    );

    assert!(
        release.contains(&format!("wasm-bindgen-cli@{wasm_bindgen_pin}")),
        "release.yml must install the SAME pinned wasm-bindgen-cli@{wasm_bindgen_pin} ci.yml uses"
    );
    assert!(
        release.contains(&binaryen_pin),
        "release.yml must derive the binaryen version the same way ci.yml does ({binaryen_pin}), \
         so the two cannot drift"
    );
    assert!(
        release.contains("targets: wasm32-unknown-unknown"),
        "release.yml must install the wasm32-unknown-unknown target — without it the \
         package build cannot run at all"
    );
    assert!(
        release.contains("actions/setup-node@"),
        "release.yml must install node — the parity witness lanes and npm both need it"
    );

    // Every installation must precede the parity lanes that consume it.
    let parity = line_index(&release, "make wasm-parity");
    for needed in [
        "actions/setup-node@",
        "targets: wasm32-unknown-unknown",
        &format!("wasm-bindgen-cli@{wasm_bindgen_pin}"),
        &binaryen_pin,
    ] {
        assert!(
            line_index(&release, needed) < parity,
            "release.yml installs `{needed}` after `make wasm-parity`, which needs it"
        );
    }
}

// ── 5. the Make lanes ─────────────────────────────────────────────────────────────

fn make_target_body(source: &str, target: &str) -> String {
    let prefix = format!("{target}:");
    let start = source
        .lines()
        .position(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("missing Make target {target}"));
    source
        .lines()
        .skip(start + 1)
        .take_while(|line| line.starts_with('\t') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The dry-run lane is network-safe (`--dry-run`, never a real publish) and drives the
/// discovered package set rather than a hand-maintained list.
#[test]
fn makefile_carries_a_network_safe_dry_run_lane() {
    let make = makefile();
    let body = make_target_body(&make, "npm-publish-dry");
    assert!(
        body.contains("--dry-run"),
        "npm-publish-dry must pass --dry-run"
    );
    assert!(
        !body.contains("npm publish --access public --provenance\n"),
        "npm-publish-dry must never perform a real publish"
    );
    assert!(
        make.contains("npm-publish-dry npm-consumable"),
        "both npm lanes must be declared .PHONY together with the other lanes"
    );
}

/// The consumability lane packs, installs the tarball into a throwaway project, and runs
/// the witness against the INSTALLED package — never against the working tree.
#[test]
fn makefile_carries_the_consumability_lane() {
    let body = make_target_body(&makefile(), "npm-consumable");
    assert!(
        body.contains("npm-consumability.mjs"),
        "npm-consumable must drive the consumability driver"
    );
    let driver = repo_root().join("scripts/npm-consumability.mjs");
    assert!(
        driver.is_file(),
        "scripts/npm-consumability.mjs (the consumability driver) must exist"
    );
    let source = std::fs::read_to_string(&driver).expect("read the consumability driver");
    for needle in ["npm pack", "npm install", "node_modules"] {
        assert!(
            source.contains(needle),
            "the consumability driver must `{needle}` — a lane that tests the working \
             tree instead of the installed tarball proves nothing about consumability"
        );
    }
}

// ── 6. the CDN documentation cannot drift ─────────────────────────────────────────

/// Extract every package name that appears immediately after a CDN prefix in `source`.
///
/// A CDN specifier is `[@scope/]name[@version][/path]`. The scope segment is consumed
/// first (its `/` is part of the NAME, not a path separator), and the name then ends at
/// the version `@` or the first path `/`.
fn cdn_named_packages(source: &str, prefix: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find(prefix) {
        let tail = &rest[at + prefix.len()..];
        let spec: String = tail
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '`' | ')' | '<' | '>'))
            .collect();
        let scope_len = match spec.strip_prefix('@') {
            Some(after) => after.find('/').map_or(spec.len(), |i| i + 2),
            None => 0,
        };
        let body = &spec[scope_len..];
        let stop = body.find(['@', '/']).unwrap_or(body.len());
        let name = format!("{}{}", &spec[..scope_len], &body[..stop]);
        if !name.is_empty() {
            names.insert(name);
        }
        rest = tail;
    }
    names
}

/// The documented jsdelivr and unpkg templates name EXACTLY the discovered published
/// package set — no stale name, no missing one. This is the drift gate: adding or
/// renaming a package without touching the CDN documentation fails here.
#[test]
fn cdn_documentation_names_exactly_the_published_packages() {
    let doc = read("docs/design/external-docs-distribution.md");
    let expected: BTreeSet<String> = published().iter().map(|p| p.name().to_string()).collect();

    for prefix in ["https://cdn.jsdelivr.net/npm/", "https://unpkg.com/"] {
        let documented = cdn_named_packages(&doc, prefix);
        assert_eq!(
            documented, expected,
            "the documented `{prefix}` templates do not name exactly the published set"
        );
    }

    let version = workspace_version();
    for name in &expected {
        assert!(
            doc.contains(&format!("https://cdn.jsdelivr.net/npm/{name}@{version}/")),
            "the jsdelivr template for `{name}` must pin the workspace version {version}"
        );
        assert!(
            doc.contains(&format!("https://unpkg.com/{name}@{version}/")),
            "the unpkg template for `{name}` must pin the workspace version {version}"
        );
    }
}

/// Neither the console nor the distribution design promises runtime CDN loading. The
/// console is offline-first: every byte it runs is fetched from its own origin, so a CDN
/// URL is an INSTALL-time convenience, never a runtime dependency.
#[test]
fn runtime_cdn_loading_is_documented_as_forbidden() {
    for (file, source) in [
        (
            "crates/docs/assets/console/README.md",
            read("crates/docs/assets/console/README.md"),
        ),
        (
            "docs/design/external-docs-distribution.md",
            read("docs/design/external-docs-distribution.md"),
        ),
    ] {
        assert!(
            source.contains("No runtime CDN loading"),
            "{file} must carry the `No runtime CDN loading` statement"
        );
    }
}

// ── The two halves of the ESM surface parser agree ─────────────────────────────────

/// The shared corpus both ESM-surface parsers are run over.
///
/// Every entry is a form the two halves used to treat DIFFERENTLY, plus the ordinary forms
/// that must keep working:
///
/// * `export{a}` with no space — the JS `RE_EXPORT_BLOCK` allowed it (`export\s*\{`), the
///   Rust half demanded the literal `"export {"`;
/// * a declaration whose tokens are split across lines — the JS `\s+` spans newlines, the
///   Rust half demanded a single space;
/// * an `export` that is NOT at a statement position — the JS regexes are line-anchored,
///   the Rust half searched the whole text unanchored;
/// * an engine import that is not the FIRST import in the file — the JS `engineImports`
///   regex used to span the preceding statements and capture their braces;
/// * a clause aliased across a line break, and comment forms.
const ESM_CORPUS: &[(&str, &str)] = &[
    (
        "plain declarations",
        "export function ready(a) {}\nexport const K = 1;\nexport class C {}\n",
    ),
    (
        "re-export block, spaced and unspaced",
        "export { a, b as c };\nexport{d,e as f};\n",
    ),
    (
        "declaration file forms",
        "export type A = string;\nexport interface B { x: number }\n\
         export declare function g(): void;\nexport declare const H: number;\n\
         export default function __wbg_init(): Promise<void>;\n",
    ),
    (
        "async and var",
        "export async function boot() {}\nexport var legacy = 1;\n",
    ),
    (
        "tokens split across lines",
        "export\n  async\n  function\n  spread() {}\n",
    ),
    (
        "an export keyword that is not at a statement position",
        "const o = { export: 1 };\nfoo(); export { real };\n",
    ),
    (
        "engine import preceded by unrelated imports",
        "import { unrelatedA, unrelatedB } from \"./other.mjs\";\n\
         import { third } from \"../shared/util.mjs\";\n\
         import wasmInit, { mcp, ready as loaded } from \"./pkg/gmeow_mcp_wasm.js\";\n\
         export { mcp, loaded };\n",
    ),
    (
        "engine import clause wrapped across lines",
        "import init, {\n  version,\n  ready\n    as\n    loaded,\n} from \"./pkg/x.js\";\n",
    ),
    ("no engine import at all", "export const only = 1;\n"),
    (
        "comments never contribute",
        "// export function ghost() {}\n/* export { phantom }; */\nexport function real2() {}\n",
    ),
];

/// The JS half's answers for [`ESM_CORPUS`], obtained by RUNNING
/// `scripts/npm-packaging.mjs` — never by re-implementing or scraping it.
///
/// A missing/failing `node` is a HARD FAIL: this gate's whole content is that the two
/// implementations agree, and an implementation that could not be run has not agreed with
/// anything.
fn js_parser_answers() -> serde_json::Value {
    let module = repo_root().join("scripts/npm-packaging.mjs");
    let corpus = serde_json::to_string(&ESM_CORPUS.iter().map(|(_, src)| *src).collect::<Vec<_>>())
        .expect("serialize the corpus");
    let script = format!(
        r#"import {{ exportedValueNames, engineImports }} from {module};
const corpus = {corpus};
const out = corpus.map((source) => ({{
  exports: [...exportedValueNames(source)].sort(),
  imports: engineImports(source).sort((a, b) => (a.source < b.source ? -1 : 1)),
}}));
process.stdout.write(JSON.stringify(out));
"#,
        module = serde_json::to_string(&format!("file://{}", module.display()))
            .expect("quote the module URL"),
    );
    let output = std::process::Command::new("node")
        .args(["--input-type=module", "--eval", &script])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "cannot run `node` to exercise scripts/npm-packaging.mjs ({e}). This gate \
                 proves the Rust and JS halves of the ESM surface parser agree; without a \
                 node runtime it proves nothing, so it fails rather than skipping."
            )
        });
    assert!(
        output.status.success(),
        "scripts/npm-packaging.mjs failed under node: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "the JS half emitted unparsable JSON ({e}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

/// The Rust and JS ESM-surface parsers are two implementations of one contract, and
/// nothing used to hold them to it: they carried different tolerances (`export{` vs
/// `"export "`, `\s+` vs a single space, an anchored vs unanchored `export` scan), so the
/// always-on Rust gate and the wasm-parity Node lane could reach different verdicts about
/// the same shipped bytes. This runs BOTH over one corpus and asserts equality.
#[test]
fn the_two_esm_parsers_agree_over_the_shared_corpus() {
    let js = js_parser_answers();
    let js_rows = js.as_array().expect("the JS half emits an array");
    assert_eq!(
        js_rows.len(),
        ESM_CORPUS.len(),
        "the JS half answered {} of {} corpus cases",
        js_rows.len(),
        ESM_CORPUS.len()
    );

    for ((label, source), js_row) in ESM_CORPUS.iter().zip(js_rows) {
        let js_exports: Vec<String> = js_row["exports"]
            .as_array()
            .expect("exports is an array")
            .iter()
            .map(|v| v.as_str().expect("an export name is a string").to_string())
            .collect();
        let rust_exports: Vec<String> = exported_value_names(source).into_iter().collect();
        assert_eq!(
            rust_exports, js_exports,
            "{label}: the Rust and JS export-name parsers disagree — one of \
             crates/gmeow-dev-cli/tests/npm_packaging_contract.rs and \
             scripts/npm-packaging.mjs is wrong about the shipped bytes"
        );

        let js_imports: Vec<(String, String)> = js_row["imports"]
            .as_array()
            .expect("imports is an array")
            .iter()
            .map(|v| {
                (
                    v["source"]
                        .as_str()
                        .expect("source is a string")
                        .to_string(),
                    v["local"].as_str().expect("local is a string").to_string(),
                )
            })
            .collect();
        let rust_imports: Vec<(String, String)> = engine_imports(source).into_iter().collect();
        assert_eq!(
            rust_imports, js_imports,
            "{label}: the Rust and JS engine-import parsers disagree"
        );
    }

    // Non-vacuity: the corpus really does exercise both parsers, so an agreement over two
    // empty answer sets can never stand in for agreement.
    assert!(
        ESM_CORPUS
            .iter()
            .any(|(_, source)| !exported_value_names(source).is_empty()),
        "the corpus must contain at least one exporting module"
    );
    assert!(
        ESM_CORPUS
            .iter()
            .any(|(_, source)| !engine_imports(source).is_empty()),
        "the corpus must contain at least one engine import"
    );
}
