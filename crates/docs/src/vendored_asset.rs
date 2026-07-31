// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared vendored-wasm-asset harness.
//!
//! The docs site ships one or more **prebuilt** wasm engines — today the console's two MCP
//! segments, which EVERY interactive surface dispatches through, plus the vendored
//! `purrdf` RDF/JS engine a consumer of the published tree can import directly — as pinned
//! `include_bytes!` build inputs under `crates/docs/assets/<name>/`. The
//! regeneration pipeline never rebuilds wasm, so nothing structurally forces a
//! vendored blob to stay in step with its source crate. Each such asset therefore
//! shares one ritual:
//!
//! 1. a set of vendored files (the wasm module, its wasm-bindgen JS glue, the `.d.ts`
//!    type surface, and the native↔wasm witness attestations), each carrying a
//!    `.license` REUSE sidecar;
//! 2. a `DIGESTS.blake3` content-digest manifest pinning their exact bytes;
//! 3. emission of the runtime files into the rendered [`Site`] under
//!    `assets/<name>/` when the playground/interactive assets are present;
//! 4. an anti-rot test that proves the vendored `.wasm` is a real module, the JS
//!    glue and the type surfaces still declare the SAME export set, and the pinned
//!    digests match.
//!
//! This module captures that ritual ONCE. Each asset is a single [`VendoredWasmAsset`]
//! constant ([`MCP_CORE_ASSET`], [`MCP_ASSET`], [`PURRDF_ASSET`]): the renderer calls
//! [`VendoredWasmAsset::emit_into`] to write it into the site, and the asset's
//! integration test calls [`VendoredWasmAsset::verify`] to gate it. There is exactly
//! one definition per asset — the emission descriptor and the anti-rot verifier read
//! from the same source of truth.
//!
//! [`Site`]: crate::render::Site

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_docs_model::formats::{Capability, DocFormat, format_capabilities};

/// The digest-manifest filename pinning the vendored bytes, in every asset dir.
pub const DIGEST_MANIFEST: &str = "DIGESTS.blake3";

/// The wasm-bindgen LOADER names, which are not part of an engine's export surface.
///
/// Every `--target web` glue module ends with `export { initSync, __wbg_init as default }`:
/// the synchronous and asynchronous instantiation entry points. They exist in every
/// wasm-bindgen output regardless of what the Rust crate exports, so counting them as
/// engine exports would make the export-set comparison below compare the loader instead of
/// the surface. They are excluded on the glue side only — a wrapper that re-exported one
/// would still be caught, because the wrapper's surface is compared against the package
/// `.d.ts` verbatim.
const WASM_BINDGEN_LOADER: &[&str] = &["initSync", "default"];

/// The vendored files whose declared export SETS must agree exactly.
///
/// Substring probes ("does the glue contain the text `dataset_query`?") answer a weaker
/// question than the one that matters: a re-vendor that ADDED a binding, RENAMED one the
/// wrapper still imports, or dropped one the hand-written `.d.ts` still promises passes
/// every probe while shipping a package whose type surface lies about its runtime. The
/// gate below compares SETS in both directions instead, so drift in either direction is a
/// named failure.
#[derive(Debug, Clone, Copy)]
pub struct ExportSurface {
    /// The wasm-bindgen `--target web` JS glue, relative to the asset dir.
    pub glue_js: &'static str,
    /// The wasm-bindgen `.d.ts` emitted beside [`glue_js`](Self::glue_js).
    pub glue_dts: &'static str,
    /// The ES-module wrapper that re-exports the glue at the package root and adds the
    /// isomorphic glue the synchronous wasm boundary cannot express (`ready()`, the
    /// tiered dispatcher, the Stream/Sink primitives).
    pub wrapper_mjs: &'static str,
    /// The HAND-WRITTEN package `.d.ts` — the type surface a TypeScript consumer of the
    /// package root sees — when the asset vendors one.
    ///
    /// Declared per asset rather than assumed, because vendored packages genuinely differ:
    /// a third-party npm package may ship a hand-written root `index.d.ts` alongside its
    /// generated one, while the gmeow-owned MCP segment wrappers publish the generated
    /// `pkg/*.d.ts` as their only type surface. `None` selects the smaller surface
    /// explicitly; it never skips a check that applies.
    pub package_dts: Option<&'static str>,
}

/// The names a JS/TS module exports, as a set.
///
/// Recognized forms, all at line start (the vendored files are generated or
/// house-formatted, so an indented `export` is not a thing that occurs):
/// `export class X`, `export function f`, `export async function f`, `export const K`,
/// and every name in an `export { a, b as c };` block (the EXPORTED name, `c`).
fn module_exports(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut in_block = false;
    for line in text.lines() {
        if in_block {
            if let Some(head) = line.split('}').next() {
                collect_specifiers(head, &mut names);
            }
            if line.contains('}') {
                in_block = false;
            }
            continue;
        }
        let Some(rest) = line.strip_prefix("export ") else {
            continue;
        };
        if let Some(spec) = rest.strip_prefix('{') {
            // `export { … };` — possibly spanning several lines.
            let head = spec.split('}').next().unwrap_or(spec);
            collect_specifiers(head, &mut names);
            in_block = !spec.contains('}');
            continue;
        }
        for keyword in [
            "async function ",
            "function ",
            "class ",
            "const ",
            "let ",
            "var ",
        ] {
            if let Some(tail) = rest.strip_prefix(keyword) {
                if let Some(name) = identifier(tail) {
                    names.insert(name);
                }
                break;
            }
        }
    }
    names
}

/// The value declarations of a `.d.ts` — `export class X` / `export function f`.
///
/// `export type` / `export interface` are deliberately NOT collected: they have no runtime
/// existence, so a module can never be compared against them. `export default function …`
/// is the wasm-bindgen loader and is filtered with the rest of it.
fn dts_value_exports(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("export ") else {
            continue;
        };
        for keyword in ["class ", "function "] {
            if let Some(tail) = rest.strip_prefix(keyword) {
                if let Some(name) = identifier(tail) {
                    names.insert(name);
                }
                break;
            }
        }
    }
    names
}

/// One named specifier of an import clause: the name the module being imported FROM must
/// still export, and the name the importing module binds it to locally.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ImportBinding {
    /// `a` in `import { a as b }` — what the glue must still provide.
    source: String,
    /// `b` in `import { a as b }` — the name the wrapper's own body and re-exports use.
    local: String,
}

/// Drop line and block comments so a commented-out import is never counted.
///
/// Only a line whose TRIMMED start is `//` is dropped: `//` also occurs inside every URL
/// literal in these files, and stripping from the first bare `//` would truncate them.
fn strip_js_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
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

/// Whether the identifier ending at `end` in `text` is a standalone word rather than the
/// tail of a longer one (so `notfrom"x"` never reads as a `from` clause).
fn is_word_start(text: &str, end: usize) -> bool {
    text[..end]
        .chars()
        .next_back()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '$'))
}

/// Every named specifier a module imports from the module specifier `from`.
///
/// Anchored on the QUOTED SPECIFIER and walked backwards (`rfind('}')` then `rfind('{')`),
/// exactly as the npm-packaging contract's twin does, rather than by accumulating lines
/// from an `import` keyword. Two forms the line accumulator missed:
///
/// * a clause whose `{` opens on a LATER line than the `import` keyword — the accumulator
///   armed only on `import … {` on one line, so such a statement was skipped entirely;
/// * a SINGLE-quoted specifier — a vendored third-party wrapper is formatted by its own
///   toolchain, and `'./glue.js'` is exactly as valid as `"./glue.js"`.
///
/// Both would have silently emptied the import set, and an empty set makes the
/// "wrapper exports something it neither imports nor declares" check fire for a reason
/// that has nothing to do with the vendored bytes.
fn module_import_bindings(text: &str, from: &str) -> BTreeSet<ImportBinding> {
    let clean = strip_js_comments(text);
    let mut out = BTreeSet::new();
    for quote in ['"', '\''] {
        let needle = format!("{quote}{from}{quote}");
        for (idx, _) in clean.match_indices(needle.as_str()) {
            let head = &clean[..idx];
            // The specifier must be the target of a `from` clause, not a string that
            // merely spells the same path.
            let before = head.trim_end();
            if !before.ends_with("from") || !is_word_start(before, before.len() - "from".len()) {
                continue;
            }
            let Some(close) = before.rfind('}') else {
                continue;
            };
            let Some(open) = before[..close].rfind('{') else {
                continue;
            };
            // …and the clause must belong to an `import` statement: everything between the
            // keyword and the brace is at most a default binding and a comma.
            let Some(keyword) = before[..open].rfind("import") else {
                continue;
            };
            if before[keyword + "import".len()..open].contains(';') {
                continue;
            }
            for raw in before[open + 1..close].split(',') {
                let spec = raw.trim();
                if spec.is_empty() {
                    continue;
                }
                let mut parts = spec.split_whitespace();
                let first = parts.next().unwrap_or_default();
                let renamed = parts.next() == Some("as");
                let local = match renamed {
                    true => parts.next().unwrap_or(first),
                    false => first,
                };
                let (Some(source), Some(local)) = (identifier(first), identifier(local)) else {
                    continue;
                };
                out.insert(ImportBinding { source, local });
            }
        }
    }
    out
}

/// The names a module IMPORTS from `from`, by their SOURCE name (`a` in `a as b`).
///
/// The source name is what the glue must still provide.
fn module_imports_from(text: &str, from: &str) -> BTreeSet<String> {
    module_import_bindings(text, from)
        .into_iter()
        .map(|binding| binding.source)
        .collect()
}

/// The names a module BINDS LOCALLY from `from` (`b` in `a as b`).
///
/// An aliased import is still a backing for a re-export: `import { a as b }` followed by
/// `export { b }` is exported, imported and correct. Comparing re-exports against the
/// SOURCE names alone reported `b` as unbacked.
fn module_import_locals(text: &str, from: &str) -> BTreeSet<String> {
    module_import_bindings(text, from)
        .into_iter()
        .map(|binding| binding.local)
        .collect()
}

/// Push the EXPORTED name of each `a` / `a as b` specifier in an `export { … }` list into
/// `names` — `b` when the clause renames, `a` otherwise.
///
/// (The import side reads BOTH halves of a specifier and so parses its own clauses in
/// [`module_import_bindings`]; this one only ever needs the exported name.)
fn collect_specifiers(list: &str, names: &mut BTreeSet<String>) {
    for raw in list.split(',') {
        let spec = raw.trim();
        if spec.is_empty() {
            continue;
        }
        let mut parts = spec.split_whitespace();
        let first = parts.next().unwrap_or_default();
        let renamed = parts.next() == Some("as");
        let name = match renamed {
            true => parts.next().unwrap_or(first),
            false => first,
        };
        if let Some(name) = identifier(name) {
            names.insert(name);
        }
    }
}

/// The leading JS identifier of `text`, or `None` when it does not start with one.
fn identifier(text: &str) -> Option<String> {
    let name: String = text
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Read a vendored file, panicking with the asset's refresh instruction on failure.
fn read_vendored(dir: &Path, name: &str, refresh_target: &str) -> String {
    std::fs::read_to_string(dir.join(name))
        .unwrap_or_else(|e| panic!("vendored {name} must exist (run make {refresh_target}): {e}"))
}

/// A pinned, vendored wasm engine bundle emitted into the docs site.
///
/// One constant per asset captures the whole ritual: the subdir, the runtime files
/// emitted into the site (with their `include_bytes!` bytes), the full set of
/// vendored files the digest manifest pins, the wasm module to structurally probe,
/// and the export surface the site depends on. Both the renderer
/// ([`emit_into`](Self::emit_into)) and the anti-rot test
/// ([`verify`](Self::verify)) read from this single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct VendoredWasmAsset {
    /// The asset subdir under `crates/docs/assets/` and the site emission prefix
    /// (`assets/<name>/`).
    pub name: &'static str,
    /// The runtime files emitted verbatim into the [`Site`](crate::render::Site):
    /// `(filename, bytes)`. The bytes are `include_bytes!` literals (the wasm module,
    /// its JS glue and the package wrapper); the `.d.ts` type surfaces are vendored and
    /// gated but not emitted.
    pub emitted_files: &'static [(&'static str, &'static [u8])],
    /// Every vendored filename the `DIGESTS.blake3` manifest pins — exactly the set
    /// the refresh maint target writes into the asset dir, the witness attestations
    /// included. A witness that is not digest-pinned can be edited freely and still pass
    /// attestation, which would make the attestation decorative.
    pub vendored_files: &'static [&'static str],
    /// The vendored wasm module filename (the `\0asm`-magic + size structural probe).
    pub wasm_file: &'static str,
    /// A plausible-size floor for the wasm module; guards an empty/stub blob.
    pub min_wasm_len: usize,
    /// The files whose declared export sets must agree — the anti-stale-re-vendor gate.
    pub export_surface: ExportSurface,
    /// The `make` target that rebuilds + re-vendors this asset. Referenced in failure
    /// messages, and PROVEN TO EXIST by [`check_refresh_targets`]: a descriptor must
    /// never print an instruction that cannot be followed.
    pub refresh_target: &'static str,
    /// The environment variable whose presence makes [`verify`](Self::verify) rewrite
    /// (bless) the digest manifest instead of comparing — set by the refresh target.
    pub bless_env: &'static str,
    /// The per-asset native↔wasm parity attestations (e.g. `WITNESS.mcp.json`): the
    /// committed native outputs the shipped wasm engine reproduces byte-for-byte. Their
    /// presence + digest-currency is gated by [`attestation_status`](Self::attestation_status)
    /// (F4/F5). For the gmeow-owned MCP segments the byte-identity is additionally EXECUTED
    /// on-gate by their Node parity lanes (`make check` → `wasm-parity`).
    ///
    /// A SLICE, not a single optional: an engine backs as many attestations as it has
    /// proven behaviours, and "one or none" was a shape the domain never had. An empty
    /// slice is a non-witnessed asset and is vacuously OK.
    pub witness_attestations: &'static [&'static str],
}

impl VendoredWasmAsset {
    /// The site-relative path a vendored `filename` is emitted to.
    #[must_use]
    pub fn site_path(&self, filename: &str) -> String {
        format!("assets/{}/{filename}", self.name)
    }

    /// Emit this asset's runtime files into `files` under `assets/<name>/`.
    ///
    /// The renderer calls this, gated the same way the rest of the interactive
    /// playground assets are, so the emission logic lives once here.
    pub fn emit_into(&self, files: &mut BTreeMap<String, Vec<u8>>) {
        for (filename, bytes) in self.emitted_files {
            files.insert(self.site_path(filename), bytes.to_vec());
        }
    }

    /// The on-disk directory holding the vendored files
    /// (`crates/docs/assets/<name>/`), resolved from the crate manifest dir.
    #[must_use]
    pub fn asset_dir(&self) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(self.name)
    }

    /// The `DIGESTS.blake3` content for the current on-disk vendored bytes: one
    /// `<blake3-hex>  <filename>` line per vendored file, sorted by filename,
    /// LF-terminated.
    ///
    /// Ordered by filename (not by the formatted line) so a change to one file's hash
    /// never reshuffles the other rows — the manifest diff stays minimal.
    ///
    /// # Panics
    ///
    /// Panics if a vendored file cannot be read.
    #[must_use]
    pub fn current_manifest(&self) -> String {
        let mut names: Vec<&str> = self.vendored_files.to_vec();
        names.sort_unstable();
        let dir = self.asset_dir();
        let lines: Vec<String> = names
            .into_iter()
            .map(|name| {
                let bytes = std::fs::read(dir.join(name))
                    .unwrap_or_else(|e| panic!("vendored {name} must exist: {e}"));
                format!("{}  {name}", blake3::hash(&bytes).to_hex())
            })
            .collect();
        let mut out = lines.join("\n");
        out.push('\n');
        out
    }

    /// Whether this asset's committed native↔wasm witness-attestations are ALL present AND
    /// current (F4/F5). Current means: each attestation file exists and is non-empty,
    /// AND the on-disk vendored bytes match the pinned `DIGESTS.blake3` — so the engine
    /// the witnesses proved byte-equivalent to native is EXACTLY the engine that ships.
    /// The witnesses are themselves in `vendored_files`, so the digest comparison covers
    /// their bytes too: an edited witness is drift, not a pass.
    ///
    /// An asset with no witness attestations is vacuously OK.
    /// Returns `Some(message)` describing the first failure, or `None` when every
    /// attestation is present and current — a violation is a reportable message, not a
    /// silent gap. (`Option`, not `Result<_, String>`: `gmeow_errors::Diag` is the sole
    /// first-party error type, and this "current?" query has no error channel — a stale
    /// attestation is the answer, not a failure.)
    pub fn attestation_status(&self) -> Option<String> {
        if self.witness_attestations.is_empty() {
            return None;
        }
        let dir = self.asset_dir();
        for witness in self.witness_attestations {
            match std::fs::read(dir.join(witness)) {
                Ok(bytes) if !bytes.is_empty() => {}
                Ok(_) => {
                    return Some(format!(
                        "witness attestation '{witness}' for engine '{}' is empty",
                        self.name
                    ));
                }
                Err(e) => {
                    return Some(format!(
                        "witness attestation '{witness}' for engine '{}' is missing \
                         (run make {}): {e}",
                        self.name, self.refresh_target
                    ));
                }
            }
        }
        let committed = match std::fs::read_to_string(dir.join(DIGEST_MANIFEST)) {
            Ok(committed) => committed,
            Err(e) => {
                return Some(format!(
                    "{DIGEST_MANIFEST} for engine '{}' is missing: {e}",
                    self.name
                ));
            }
        };
        if committed != self.current_manifest() {
            return Some(format!(
                "engine '{}' drifted from {DIGEST_MANIFEST}: the witness attestations {:?} \
                 no longer describe the shipped bytes (re-run make {})",
                self.name, self.witness_attestations, self.refresh_target
            ));
        }
        None
    }

    /// The export-set agreement gate: the vendored glue, its `.d.ts`, the package wrapper
    /// and (when vendored) the hand-written package `.d.ts` all describe ONE surface.
    ///
    /// Four checks, each an EQUALITY or a named-difference subset:
    ///
    /// 1. the glue `.js` and its wasm-bindgen `.d.ts` export the same set (the loader
    ///    aside) — a `.d.ts` that outlived its `.js` is a type surface that lies;
    /// 2. every name the wrapper IMPORTS from the glue is still exported by it — the exact
    ///    failure a stale re-vendor produces;
    /// 3. every name the wrapper EXPORTS is either imported from the glue or declared
    ///    locally in the wrapper — a re-export of a vanished binding is a runtime
    ///    `SyntaxError` at module load, i.e. a dead engine;
    /// 4. when a hand-written package `.d.ts` is vendored, its value declarations EQUAL the
    ///    wrapper's export set.
    ///
    /// # Panics
    ///
    /// Panics (fails the test) naming the exact symmetric difference on any disagreement.
    pub fn verify_export_sets(&self) {
        let dir = self.asset_dir();
        let surface = &self.export_surface;
        let target = self.refresh_target;

        let glue_js = read_vendored(&dir, surface.glue_js, target);
        let glue_dts = read_vendored(&dir, surface.glue_dts, target);
        let wrapper = read_vendored(&dir, surface.wrapper_mjs, target);

        let loader: BTreeSet<String> = WASM_BINDGEN_LOADER
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let js_set: BTreeSet<String> = module_exports(&glue_js)
            .difference(&loader)
            .cloned()
            .collect();
        let dts_set: BTreeSet<String> = dts_value_exports(&glue_dts)
            .difference(&loader)
            .cloned()
            .collect();
        assert_eq!(
            js_set, dts_set,
            "vendored {} and {} export different sets — the generated .d.ts does not \
             describe the generated glue beside it (re-run make {target})",
            surface.glue_js, surface.glue_dts,
        );

        let imported = module_imports_from(&wrapper, &format!("./{}", surface.glue_js));
        assert!(
            !imported.is_empty(),
            "vendored {} imports nothing from ./{} — the wrapper is not wired to the \
             engine it is supposed to wrap (re-run make {target})",
            surface.wrapper_mjs,
            surface.glue_js,
        );
        let missing: Vec<&String> = imported.difference(&js_set).collect();
        assert!(
            missing.is_empty(),
            "vendored {} imports {missing:?} from ./{}, which no longer exports them — a \
             stale re-vendor (re-run make {target})",
            surface.wrapper_mjs,
            surface.glue_js,
        );

        let wrapper_exports = module_exports(&wrapper);
        // Every name the wrapper's own body can legally re-export: a locally-declared
        // binding, or the LOCAL half of an import specifier. The local half matters because
        // `import { a as b } … export { b }` is exported, imported and correct — comparing
        // re-exports against the SOURCE names alone reported `b` as unbacked.
        let mut local = module_import_locals(&wrapper, &format!("./{}", surface.glue_js));
        local.extend(wrapper.lines().filter_map(|line| {
            // `export` is optional here: a name the wrapper declares is locally backed
            // whether or not the declaration itself carries the keyword.
            let head = line.trim_start();
            let head = head.strip_prefix("export ").unwrap_or(head);
            // `var ` rides the list because `module_exports` recognizes `export var` — a
            // surface the backing scan could otherwise never account for.
            for keyword in [
                "async function ",
                "function ",
                "class ",
                "const ",
                "let ",
                "var ",
            ] {
                if let Some(tail) = head.strip_prefix(keyword) {
                    return identifier(tail);
                }
            }
            None
        }));
        let unbacked: Vec<&String> = wrapper_exports
            .iter()
            .filter(|name| !imported.contains(*name) && !local.contains(*name))
            .collect();
        assert!(
            unbacked.is_empty(),
            "vendored {} exports {unbacked:?}, which it neither imports from ./{} nor \
             declares locally (re-run make {target})",
            surface.wrapper_mjs,
            surface.glue_js,
        );

        if let Some(package_dts) = surface.package_dts {
            let declared = dts_value_exports(&read_vendored(&dir, package_dts, target));
            assert_eq!(
                wrapper_exports, declared,
                "vendored {} and {} declare different value surfaces — the hand-written \
                 type surface promises something the module does not export, or hides \
                 something it does (re-run make {target})",
                surface.wrapper_mjs, package_dts,
            );
        }
    }

    /// The full anti-rot gate for this asset: the vendored `.wasm` is a real module
    /// (WebAssembly magic + plausible size), the glue/wrapper/`.d.ts` export sets agree,
    /// and the pinned `DIGESTS.blake3` describes the exact on-disk bytes.
    ///
    /// When the asset's [`bless_env`](Self::bless_env) is set, the manifest is
    /// rewritten from the current bytes instead of compared — the path the refresh
    /// maint target drives, so the pinned digests always describe the bytes that
    /// target produced (no external `b3sum` needed).
    ///
    /// # Panics
    ///
    /// Panics (fails the test) on any drift: a corrupt/undersized wasm module, a
    /// disagreeing export surface, or a digest mismatch.
    pub fn verify(&self) {
        let dir = self.asset_dir();

        // Structural: real WebAssembly module, not a stub/placeholder.
        let wasm = std::fs::read(dir.join(self.wasm_file)).unwrap_or_else(|e| {
            panic!(
                "vendored {} must exist (run make {}): {e}",
                self.wasm_file, self.refresh_target
            )
        });
        assert!(
            wasm.len() >= 4 && &wasm[..4] == b"\0asm",
            "vendored {} does not start with the WebAssembly magic — corrupt or truncated",
            self.wasm_file
        );
        assert!(
            wasm.len() > self.min_wasm_len,
            "vendored {} is implausibly small ({} bytes) — a broken build was vendored",
            self.wasm_file,
            wasm.len()
        );

        // Export surface: the vendored files still describe ONE engine.
        self.verify_export_sets();

        // Digest: pin the exact bytes. The structural checks alone pass a
        // stale-but-still-functional engine; this gate does not.
        let manifest_path = dir.join(DIGEST_MANIFEST);
        let current = self.current_manifest();
        if std::env::var_os(self.bless_env).is_some() {
            std::fs::write(&manifest_path, &current)
                .unwrap_or_else(|e| panic!("write {DIGEST_MANIFEST} for {}: {e}", self.name));
            return;
        }
        let committed = std::fs::read_to_string(&manifest_path).unwrap_or_else(|e| {
            panic!(
                "missing {DIGEST_MANIFEST} (run make {}): {e}",
                self.refresh_target
            )
        });
        assert_eq!(
            committed, current,
            "vendored {} bytes drifted from {DIGEST_MANIFEST}: a vendored file changed \
             without re-running `make {}`. The structural checks pass a \
             stale-but-still-functional engine; this digest gate does not.",
            self.name, self.refresh_target
        );
    }
}

/// The vendored `gmeow-mcp-core-wasm` segment — the console's FIRST-LOAD engine.
///
/// Emitted under `assets/mcp-core/`; refreshed by `make maint-refresh-mcp-core-asset`.
///
/// This descriptor and its sibling [`MCP_ASSET`] replaced THREE retired ones
/// (`VALIDATE_ASSET`, `REASON_ASSET`, `GMN_ASSET`), each of which vendored a bespoke
/// `#[wasm_bindgen]` shim per capability — a validator, a reasoner, a codec — with its own
/// export surface and its own controller code path. All three were duplicate capability:
/// the site now speaks ONE protocol (JSON-RPC over `handle_message`) to the SAME engine an
/// agent drives, so a capability the console has is a capability an agent has by
/// construction rather than by parallel maintenance.
///
/// [`PURRDF_ASSET`] is still vendored and is NOT an exception to that: no widget dispatches
/// to it, no capability is attested against it, and every query the site itself asks is
/// `query_local` on these segments. It ships as an importable RDF/JS surface for a consumer
/// of the published tree, which is a different thing from a second protocol.
///
/// The vendored set is a directory tree rather than a flat list: `index.mjs` imports
/// `./pkg/<mod>.js`, so the wasm-bindgen output keeps its `pkg/` subpath and the emitted
/// site layout is the package layout. [`VendoredWasmAsset::site_path`] joins the name
/// verbatim, so a `pkg/`-prefixed filename needs no special handling.
pub static MCP_CORE_ASSET: VendoredWasmAsset = VendoredWasmAsset {
    name: "mcp-core",
    emitted_files: &[
        ("index.mjs", include_bytes!("../assets/mcp-core/index.mjs")),
        (
            "pkg/gmeow_mcp_core_wasm.js",
            include_bytes!("../assets/mcp-core/pkg/gmeow_mcp_core_wasm.js"),
        ),
        (
            "pkg/gmeow_mcp_core_wasm_bg.wasm",
            include_bytes!("../assets/mcp-core/pkg/gmeow_mcp_core_wasm_bg.wasm"),
        ),
    ],
    vendored_files: &[
        "WITNESS.core-deferral.json",
        "index.mjs",
        "pkg/gmeow_mcp_core_wasm.d.ts",
        "pkg/gmeow_mcp_core_wasm.js",
        "pkg/gmeow_mcp_core_wasm_bg.wasm",
        "pkg/gmeow_mcp_core_wasm_bg.wasm.d.ts",
    ],
    wasm_file: "pkg/gmeow_mcp_core_wasm_bg.wasm",
    min_wasm_len: 1_000_000,
    export_surface: ExportSurface {
        glue_js: "pkg/gmeow_mcp_core_wasm.js",
        glue_dts: "pkg/gmeow_mcp_core_wasm.d.ts",
        wrapper_mjs: "index.mjs",
        // The gmeow-owned wrapper publishes the generated `pkg/*.d.ts` as its only type
        // surface; there is no second, hand-written one to agree with.
        package_dts: None,
    },
    refresh_target: "maint-refresh-mcp-core-asset",
    bless_env: "GMEOW_MCP_CORE_BLESS",
    // The native↔wasm deferral attestation (`WITNESS.core-deferral.json`): the typed
    // `mcp.segment-not-loaded` frame the core image returns for a reasoning tool, which the
    // shipped wasm reproduces byte-for-byte (proven by
    // `crates/mcp-core-wasm/tests/witness_core.rs` + its Node lane). It attests the ROUTING,
    // which is what the tiering rests on.
    witness_attestations: &["WITNESS.core-deferral.json"],
};

/// The vendored `gmeow-mcp-wasm` segment — the console's DEMAND-LOADED reasoner.
///
/// Emitted under `assets/mcp/`; refreshed by `make maint-refresh-mcp-asset`. Fetched on the
/// first `tools/call` the core image defers, never as part of the first load — see
/// [`MCP_CORE_ASSET`] on why the two together replaced three engines.
pub static MCP_ASSET: VendoredWasmAsset = VendoredWasmAsset {
    name: "mcp",
    emitted_files: &[
        ("index.mjs", include_bytes!("../assets/mcp/index.mjs")),
        (
            "pkg/gmeow_mcp_wasm.js",
            include_bytes!("../assets/mcp/pkg/gmeow_mcp_wasm.js"),
        ),
        (
            "pkg/gmeow_mcp_wasm_bg.wasm",
            include_bytes!("../assets/mcp/pkg/gmeow_mcp_wasm_bg.wasm"),
        ),
    ],
    vendored_files: &[
        "WITNESS.mcp.json",
        "index.mjs",
        "pkg/gmeow_mcp_wasm.d.ts",
        "pkg/gmeow_mcp_wasm.js",
        "pkg/gmeow_mcp_wasm_bg.wasm",
        "pkg/gmeow_mcp_wasm_bg.wasm.d.ts",
    ],
    wasm_file: "pkg/gmeow_mcp_wasm_bg.wasm",
    min_wasm_len: 1_000_000,
    export_surface: ExportSurface {
        glue_js: "pkg/gmeow_mcp_wasm.js",
        glue_dts: "pkg/gmeow_mcp_wasm.d.ts",
        wrapper_mjs: "index.mjs",
        package_dts: None,
    },
    refresh_target: "maint-refresh-mcp-asset",
    bless_env: "GMEOW_MCP_BLESS",
    // The native↔wasm attestation (`WITNESS.mcp.json`): a real `conjecture_test` frame
    // answered by the segment, byte-identical both to the shipped wasm's answer and to what
    // the FULL native engine returns for the same frame (proven by
    // `crates/mcp-wasm/tests/witness_mcp.rs` + its Node lane).
    witness_attestations: &["WITNESS.mcp.json"],
};

/// The vendored `purrdf` browser engine — the standalone RDF-1.2 + SPARQL runtime.
///
/// Emitted under `assets/purrdf/`; refreshed by `make maint-refresh-purrdf-asset`.
///
/// The one vendored engine this repository does not author. purrdf is the sibling RDF-1.2
/// kernel (Principle 18), published to npm as [`PURRDF_NPM_PACKAGE`] under MIT OR
/// Apache-2.0 — which is why every file here carries THAT identifier and not the
/// AGPL-3.0-only the rest of this crate does: a REUSE sidecar states the license the bytes
/// are actually under, and an upstream package relicensed by the act of vendoring it is a
/// licensing claim nobody made.
///
/// It is not a second protocol surface. Every interactive widget the site and the console
/// ship dispatches JSON-RPC to the MCP segments above, and nothing here changes that: what
/// this asset adds is the RDF/JS surface a CONSUMER of the published tree can import
/// directly (`assets/purrdf/index.mjs`) to run SPARQL over their OWN dataset offline — the
/// one question `query_local` does not answer, because its scopes are the shipped bundle
/// and the caller's frame, not a standing dataset with an RDF/JS API. It therefore backs no
/// [`Capability`] and carries no witness attestation: it proves nothing about gmeow's own
/// engine, and claiming otherwise would make an attestation decorative.
///
/// Refreshed by LOWER BOUND, never by exact pin: `maint-refresh-purrdf-asset` resolves the
/// newest published version satisfying the Makefile's `PURRDF_NPM_MIN` and vendors that.
/// [`UPSTREAM_RECORD`] records which version those bytes are, is digest-pinned like every
/// other vendored file, and is checked against the declared floor by
/// [`check_vendored_lower_bound`].
pub static PURRDF_ASSET: VendoredWasmAsset = VendoredWasmAsset {
    name: "purrdf",
    emitted_files: &[
        ("index.mjs", include_bytes!("../assets/purrdf/index.mjs")),
        (
            "pkg/purrdf_wasm.js",
            include_bytes!("../assets/purrdf/pkg/purrdf_wasm.js"),
        ),
        (
            "pkg/purrdf_wasm_bg.wasm",
            include_bytes!("../assets/purrdf/pkg/purrdf_wasm_bg.wasm"),
        ),
    ],
    vendored_files: &[
        "UPSTREAM.txt",
        "index.d.ts",
        "index.mjs",
        "pkg/purrdf_wasm.d.ts",
        "pkg/purrdf_wasm.js",
        "pkg/purrdf_wasm_bg.wasm",
        "pkg/purrdf_wasm_bg.wasm.d.ts",
    ],
    wasm_file: "pkg/purrdf_wasm_bg.wasm",
    min_wasm_len: 4_000_000,
    export_surface: ExportSurface {
        glue_js: "pkg/purrdf_wasm.js",
        glue_dts: "pkg/purrdf_wasm.d.ts",
        wrapper_mjs: "index.mjs",
        // Upstream ships a HAND-WRITTEN root type surface beside the generated one — the
        // types a TypeScript consumer of the package root sees — so it is gated against the
        // wrapper's exports rather than assumed to agree with them.
        package_dts: Some("index.d.ts"),
    },
    refresh_target: "maint-refresh-purrdf-asset",
    bless_env: "GMEOW_PURRDF_BLESS",
    // No native↔wasm attestation: this engine is not gmeow's, backs no declared capability,
    // and there is no native gmeow output for it to reproduce. An empty slice is a
    // non-witnessed asset and is vacuously OK — see `witness_attestations`.
    witness_attestations: &[],
};

/// The npm package the vendored purrdf engine is refreshed from.
pub const PURRDF_NPM_PACKAGE: &str = "@blackcatinformatics/purrdf";

/// The Make variable carrying the vendored purrdf engine's version LOWER BOUND.
pub const PURRDF_MIN_VAR: &str = "PURRDF_NPM_MIN";

/// The file in a vendored asset dir recording exactly which upstream release was vendored,
/// as `<package>@<version>`. Written by the refresh target and digest-pinned like every
/// other vendored file, so it cannot be edited to claim a version the bytes are not.
pub const UPSTREAM_RECORD: &str = "UPSTREAM.txt";

/// Every vendored engine this crate ships, in emission order.
///
/// The single inventory the refresh-target gate and the render layer both read, so an asset
/// added to one is an asset the other sees.
pub static VENDORED_ASSETS: &[&VendoredWasmAsset] = &[&MCP_CORE_ASSET, &MCP_ASSET, &PURRDF_ASSET];

/// A dotted-numeric version as its components, or `None` when it is not one.
fn version_parts(version: &str) -> Option<Vec<u64>> {
    version
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

/// Whether `have` is at least `floor`, comparing dotted-numeric versions component-wise.
///
/// A shorter version is padded with zeros (`0.9` ≥ `0.8.3`, `0.8` < `0.8.3`), which is the
/// ordering npm's own range resolution uses for the release versions this repository
/// vendors. `None` when either side is not dotted-numeric — an unparsable version is a
/// reportable failure, never a silent pass.
fn version_at_least(have: &str, floor: &str) -> Option<bool> {
    let (have, floor) = (version_parts(have)?, version_parts(floor)?);
    let width = have.len().max(floor.len());
    let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    for index in 0..width {
        match at(&have, index).cmp(&at(&floor, index)) {
            std::cmp::Ordering::Less => return Some(false),
            std::cmp::Ordering::Greater => return Some(true),
            std::cmp::Ordering::Equal => {}
        }
    }
    Some(true)
}

/// The value of a `NAME ?= value` / `NAME := value` / `NAME = value` assignment in
/// `makefile`, trimmed.
fn make_variable<'a>(makefile: &'a str, name: &str) -> Option<&'a str> {
    makefile
        .lines()
        .filter(|line| !line.starts_with('\t'))
        .find_map(|line| {
            let rest = line.trim_start().strip_prefix(name)?;
            let rest = rest.trim_start();
            for operator in ["?=", ":=", "::=", "="] {
                if let Some(value) = rest.strip_prefix(operator) {
                    return Some(value.trim());
                }
            }
            None
        })
}

/// Prove the VENDORED purrdf release satisfies the LOWER BOUND the Makefile declares.
///
/// D3's rule is "lower bound, always newest — no exact pin", and a floor nothing checks is
/// not a floor: the refresh target could be run against a downgraded version, or the floor
/// could be raised to require a capability while the vendored bytes stayed behind it, and
/// nothing in the tree would notice. This reads the declared [`PURRDF_MIN_VAR`] out of the
/// Makefile and the vendored release out of [`UPSTREAM_RECORD`] (whose bytes are pinned by
/// `DIGESTS.blake3`, so the record cannot drift from the blob it describes) and compares
/// them.
///
/// `upstream` is the record's contents; returns one message per violation, empty is a pass.
#[must_use]
pub fn check_vendored_lower_bound(makefile: &str, upstream: &str) -> Vec<String> {
    let Some(floor) = make_variable(makefile, PURRDF_MIN_VAR) else {
        return vec![format!(
            "the Makefile declares no {PURRDF_MIN_VAR} — the vendored purrdf engine has no \
             lower bound to be refreshed against, so `make {}` has nothing to resolve",
            PURRDF_ASSET.refresh_target
        )];
    };
    let record = upstream.trim();
    let Some((package, version)) = record.rsplit_once('@') else {
        return vec![format!(
            "{UPSTREAM_RECORD} reads {record:?}, which is not `<package>@<version>` (run \
             make {})",
            PURRDF_ASSET.refresh_target
        )];
    };
    let mut errors = Vec::new();
    if package != PURRDF_NPM_PACKAGE {
        errors.push(format!(
            "{UPSTREAM_RECORD} records package '{package}', but the vendored purrdf engine \
             is refreshed from '{PURRDF_NPM_PACKAGE}'"
        ));
    }
    match version_at_least(version, floor) {
        Some(true) => {}
        Some(false) => errors.push(format!(
            "the vendored purrdf engine is {version}, below the declared \
             {PURRDF_MIN_VAR}={floor} — re-run make {} to take the newest release that \
             satisfies the floor",
            PURRDF_ASSET.refresh_target
        )),
        None => errors.push(format!(
            "cannot compare the vendored purrdf version {version:?} against \
             {PURRDF_MIN_VAR}={floor:?} — one of them is not a dotted-numeric version"
        )),
    }
    errors
}

/// Prove every descriptor's printed [`refresh_target`](VendoredWasmAsset::refresh_target)
/// is a REAL target in `makefile`.
///
/// Every failure message in this module ends with "run make `<target>`". A descriptor that
/// prints an instruction nobody can follow is worse than one that prints none: it sends the
/// reader to a target that does not exist, and it hides the fact that the asset has no
/// supported refresh path at all — which is exactly how a vendored blob becomes
/// unrefreshable and then permanently stale. This gate closes that loop by DERIVING the
/// target set from the descriptors and looking each one up in the Makefile, so a new asset
/// cannot be added without its refresh target, and a target cannot be renamed out from
/// under a descriptor.
///
/// A target is present when a line begins `<name>:` — GNU make's rule syntax, which cannot
/// be indented (a leading tab makes it a recipe line, not a rule).
///
/// Returns one message per missing target; an empty vector is a pass.
#[must_use]
pub fn check_refresh_targets(makefile: &str) -> Vec<String> {
    let declared: BTreeSet<&str> = makefile
        .lines()
        .filter(|line| !line.starts_with('\t'))
        .filter_map(|line| line.split_once(':'))
        .map(|(head, _)| head.trim())
        .filter(|head| !head.is_empty() && !head.contains(char::is_whitespace))
        .collect();
    VENDORED_ASSETS
        .iter()
        .filter(|asset| !declared.contains(asset.refresh_target))
        .map(|asset| {
            format!(
                "vendored asset '{}' names refresh target `make {}`, which the Makefile does \
                 not declare — the descriptor prints an instruction that cannot be followed",
                asset.name, asset.refresh_target
            )
        })
        .collect()
}

/// The vendored engines whose native↔wasm witness-attestation backs each interactive
/// capability (F4/F5). An interactive capability may be REPRESENTED by a format only if
/// every backing engine's attestation is present + current — that is what makes the
/// capability a realized, proven surface rather than a decorative self-claim:
///
/// * `LiveSparql` — the playground's and the explorer's SPARQL are both `query_local` on
///   the MCP core segment. This set used to name a second engine (the vendored purrdf wasm)
///   on the reading that a standalone query over a caller-supplied graph was a distinct
///   capability. It is not one the site exercises: both surfaces query the SHIPPED bundle,
///   which the core segment is booted over, and `scope: "bundle"` answers every result form
///   they ask for — so the second engine was a second attestation surface for one
///   capability, and both are now the core segment's. [`PURRDF_ASSET`] is still SHIPPED
///   (an importable RDF/JS surface for a consumer of the published tree) and deliberately
///   still absent from this map: an engine no site surface dispatches to must not be
///   allowed to satisfy — or to block — a capability claim the site makes.
/// * `Interactivity` — every interactive widget dispatches through the MCP core segment:
///   the validate buttons are `validate_local`, the GMN transcode is
///   `encode_gmn1`/`gmn_expand`/`gmn_glyph_legend`, the copy-as bar is `convert`.
/// * `LiveReasoning` — the in-browser structured-DL chase is `reason_graph` and the
///   conjecture playground is `conjecture_test`, both served by the demand-loaded MCP
///   reasoning segment. The CORE segment backs it too: the console cannot reach the
///   reasoning segment except by first-load core dispatching the deferral signal, so a
///   stale core attestation would break live reasoning as surely as a stale reasoning one.
///
/// The non-interactive capabilities (`SearchIndex`, `Diagrams`, `CrossLinkFidelity`) are
/// not engine-backed, so they require no attestation.
///
/// Dispatch is on the [`Capability`] ALONE — there is no surface parameter, because the
/// question "which engine must be proven for this capability to be honest?" is a property
/// of the capability and not of the format that claims it.
#[must_use]
pub fn capability_backing_assets(cap: Capability) -> &'static [&'static VendoredWasmAsset] {
    const LIVE_SPARQL: &[&VendoredWasmAsset] = &[&MCP_CORE_ASSET];
    const INTERACTIVITY: &[&VendoredWasmAsset] = &[&MCP_CORE_ASSET];
    const LIVE_REASONING: &[&VendoredWasmAsset] = &[&MCP_CORE_ASSET, &MCP_ASSET];
    const NONE: &[&VendoredWasmAsset] = &[];
    match cap {
        Capability::LiveSparql => LIVE_SPARQL,
        Capability::Interactivity => INTERACTIVITY,
        Capability::LiveReasoning => LIVE_REASONING,
        Capability::SearchIndex | Capability::Diagrams | Capability::CrossLinkFidelity => NONE,
    }
}

/// The F4/F5 attestation gate. [`format_capabilities`] declares the per-format capability
/// partition statically; this is a SEPARATE guard that HARD-FAILS the build if any format
/// REPRESENTS an interactive capability whose backing engine lacks a present, current
/// native↔wasm witness-attestation ([`VendoredWasmAsset::attestation_status`]). Composed
/// with two other on-gate facts — the `wasm-parity` lane, which RUNS the native≡wasm parity
/// for the gmeow-owned engines on every `make check`, and the digest pin, which ties the
/// shipped bytes AND the witnesses to the attested build (the `maint-refresh-*-asset`
/// targets re-pin only after `*-pkg-test` passes) — it enforces the conjunction
/// "the format declares the capability AND its engine's parity is proven-and-current."
/// So a represented interactive `logic:preservationKind` cannot ship without proven parity:
/// a missing or stale attestation is a HARD FAIL that forbids the capability, never a silent
/// drop.
///
/// Returns one message per (format, capability, engine) violation; an empty vector is a
/// pass. Wired onto the `crate-check` gate surface alongside the loss-lattice gate.
#[must_use]
pub fn check_capability_attestations() -> Vec<String> {
    let mut errors = Vec::new();
    for fmt in DocFormat::ALL {
        for cap in format_capabilities(fmt).representable {
            for asset in capability_backing_assets(cap) {
                if let Some(e) = asset.attestation_status() {
                    errors.push(format!(
                        "format '{}' represents interactive capability '{}', but its backing \
                         engine's witness-attestation is not current: {e}",
                        fmt.slug(),
                        cap.slug(),
                    ));
                }
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_exports_reads_declarations_and_reexport_blocks() {
        let text =
            "export function ready(a) {}\nexport const K = 1;\nexport {\n  a,\n  b as c,\n};\n";
        let names = module_exports(text);
        assert_eq!(
            names.iter().map(String::as_str).collect::<Vec<_>>(),
            ["K", "a", "c", "ready"],
            "a re-export block contributes its EXPORTED names, `b as c` as `c`"
        );
    }

    #[test]
    fn module_imports_from_keeps_the_source_name() {
        let text =
            "import wasmInit, {\n  ready as snapshotLoaded,\n  mcp,\n} from \"./pkg/x.js\";\n";
        let names = module_imports_from(text, "./pkg/x.js");
        assert_eq!(
            names.iter().map(String::as_str).collect::<Vec<_>>(),
            ["mcp", "ready"],
            "`ready as snapshotLoaded` must be recorded as `ready` — the name the glue \
             must still export"
        );
        assert!(
            module_imports_from(text, "./pkg/other.js").is_empty(),
            "an import from a different module contributes nothing"
        );
        assert_eq!(
            module_import_locals(text, "./pkg/x.js")
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["mcp", "snapshotLoaded"],
            "the LOCAL half of `ready as snapshotLoaded` is what the wrapper's body and \
             its re-exports name"
        );
    }

    /// The two import spellings the line-accumulating scanner silently returned NOTHING
    /// for — an empty import set makes the "exports something it neither imports nor
    /// declares" check fire for a reason that has nothing to do with the vendored bytes.
    #[test]
    fn module_imports_from_reads_a_late_brace_and_a_single_quoted_specifier() {
        let late_brace = "import\n  {\n  mcp,\n  ready,\n}\n  from \"./pkg/x.js\";\n";
        assert_eq!(
            module_imports_from(late_brace, "./pkg/x.js")
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["mcp", "ready"],
            "a clause whose `{{` opens on a later line than the `import` keyword is still \
             an import"
        );

        let single_quoted = "import { mcp, ready } from './pkg/x.js';\n";
        assert_eq!(
            module_imports_from(single_quoted, "./pkg/x.js")
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["mcp", "ready"],
            "a single-quoted module specifier is exactly as valid as a double-quoted one"
        );
    }

    /// …and the widened scanner does not start counting things that are not imports.
    #[test]
    fn module_imports_from_ignores_comments_and_bare_strings() {
        let commented = "// import { ghost } from \"./pkg/x.js\";\n\
                         /* import { phantom } from \"./pkg/x.js\"; */\n\
                         import { mcp } from \"./pkg/x.js\";\n";
        assert_eq!(
            module_imports_from(commented, "./pkg/x.js")
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["mcp"],
            "a commented-out import contributes nothing"
        );

        let bare_string = "const spec = { a } = notfrom\"./pkg/x.js\";\n";
        assert!(
            module_imports_from(bare_string, "./pkg/x.js").is_empty(),
            "a string that merely spells the module path is not a `from` clause"
        );
    }

    #[test]
    fn dts_value_exports_ignores_type_only_declarations() {
        let text = "export type A = string;\nexport interface B { x: number }\n\
                    export class C {}\nexport function d(): void;\n";
        let names = dts_value_exports(text);
        assert_eq!(
            names.iter().map(String::as_str).collect::<Vec<_>>(),
            ["C", "d"],
            "types and interfaces have no runtime existence, so they are not comparable \
             against a module's exports"
        );
    }

    #[test]
    fn a_missing_refresh_target_is_reported_by_name() {
        // The real Makefile is checked by `crates/docs/tests/refresh_targets_exist.rs`; here
        // the NEGATIVE direction is proven against a Makefile that declares none of them.
        let errors = check_refresh_targets("all:\n\techo hi\n");
        assert_eq!(
            errors.len(),
            VENDORED_ASSETS.len(),
            "every descriptor whose refresh target is absent must be reported: {errors:?}"
        );
        for asset in VENDORED_ASSETS {
            assert!(
                errors.iter().any(|e| e.contains(asset.refresh_target)),
                "the failure must NAME the missing target `{}`",
                asset.refresh_target
            );
        }
        // A recipe line that happens to contain a colon is not a rule.
        assert!(
            check_refresh_targets("\tmaint-refresh-mcp-core-asset: not a rule\n")
                .iter()
                .any(|e| e.contains("maint-refresh-mcp-core-asset")),
            "a tab-indented line is a recipe, never a target declaration"
        );
    }

    #[test]
    fn version_ordering_pads_the_shorter_side_with_zeros() {
        assert_eq!(version_at_least("0.9.0", "0.8.3"), Some(true));
        assert_eq!(version_at_least("0.8.3", "0.8.3"), Some(true));
        assert_eq!(version_at_least("0.8.2", "0.8.3"), Some(false));
        assert_eq!(version_at_least("0.9", "0.8.3"), Some(true));
        assert_eq!(version_at_least("0.8", "0.8.3"), Some(false));
        assert_eq!(version_at_least("1.0.0", "0.99.99"), Some(true));
        assert_eq!(
            version_at_least("0.9.0-rc.1", "0.8.3"),
            None,
            "an unparsable version is reported, never silently accepted"
        );
    }

    #[test]
    fn make_variable_reads_every_assignment_operator() {
        assert_eq!(make_variable("A ?= 1\n", "A"), Some("1"));
        assert_eq!(make_variable("A := 2\n", "A"), Some("2"));
        assert_eq!(make_variable("A = 3\n", "A"), Some("3"));
        assert_eq!(
            make_variable("\tA ?= tabbed\n", "A"),
            None,
            "a tab-indented line is a recipe, never a variable assignment"
        );
        assert_eq!(make_variable("B ?= 1\n", "A"), None);
    }

    /// The lower-bound gate reports, by name, each way the vendored record can be wrong.
    #[test]
    fn the_lower_bound_gate_names_every_violation() {
        let makefile = format!("{PURRDF_MIN_VAR} ?= 0.8.3\n");
        assert!(
            check_vendored_lower_bound(&makefile, &format!("{PURRDF_NPM_PACKAGE}@0.9.0\n"))
                .is_empty(),
            "the newest release satisfying the floor is a pass"
        );

        let below = check_vendored_lower_bound(&makefile, &format!("{PURRDF_NPM_PACKAGE}@0.8.1"));
        assert!(
            below.iter().any(|e| e.contains("below the declared")),
            "a downgraded vendored engine must be reported: {below:?}"
        );

        let wrong = check_vendored_lower_bound(&makefile, "@someone/else@0.9.0");
        assert!(
            wrong.iter().any(|e| e.contains("records package")),
            "a record naming a different package must be reported: {wrong:?}"
        );

        let malformed = check_vendored_lower_bound(&makefile, "0.9.0");
        assert!(
            malformed.iter().any(|e| e.contains("<package>@<version>")),
            "a record that is not `<package>@<version>` must be reported: {malformed:?}"
        );

        let unbounded =
            check_vendored_lower_bound("all:\n", &format!("{PURRDF_NPM_PACKAGE}@0.9.0"));
        assert!(
            unbounded.iter().any(|e| e.contains(PURRDF_MIN_VAR)),
            "a Makefile with no declared floor must be reported: {unbounded:?}"
        );
    }
}
