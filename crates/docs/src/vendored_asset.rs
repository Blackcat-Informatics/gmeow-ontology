// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared vendored-wasm-asset harness.
//!
//! The docs site ships one or more **prebuilt** wasm engines (the offline SPARQL
//! playground runtime, purrdf; the repo-free Tier-1 validator, gmeow-validate-wasm)
//! as pinned `include_bytes!` build inputs under `crates/docs/assets/<name>/`. The
//! regeneration pipeline never rebuilds wasm, so nothing structurally forces a
//! vendored blob to stay in step with its source crate. Each such asset therefore
//! shares one ritual:
//!
//! 1. a set of vendored files (the wasm module, its wasm-bindgen JS glue, and the
//!    `.d.ts` type surface), each carrying a `.license` REUSE sidecar;
//! 2. a `DIGESTS.blake3` content-digest manifest pinning their exact bytes;
//! 3. emission of the runtime files into the rendered [`Site`] under
//!    `assets/<name>/` when the playground/interactive assets are present;
//! 4. an anti-rot test that proves the vendored `.wasm` is a real module, the JS
//!    glue still exposes the expected export surface, and the pinned digests match.
//!
//! This module captures that ritual ONCE. Each asset is a single [`VendoredWasmAsset`]
//! constant ([`PURRDF_ASSET`], [`VALIDATE_ASSET`]): the renderer calls
//! [`VendoredWasmAsset::emit_into`] to write it into the site, and the asset's
//! integration test calls [`VendoredWasmAsset::verify`] to gate it. There is exactly
//! one definition per asset — the emission descriptor and the anti-rot verifier read
//! from the same source of truth.
//!
//! [`Site`]: crate::render::Site

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::formats::{Capability, DocFormat, format_capabilities};

/// The digest-manifest filename pinning the vendored bytes, in every asset dir.
pub const DIGEST_MANIFEST: &str = "DIGESTS.blake3";

/// One expected export-surface probe: `needle` must appear verbatim in the vendored
/// `file`. Its absence means the vendored bindings predate (or drifted from) the API
/// the site depends on — a stale re-vendor.
#[derive(Debug, Clone, Copy)]
pub struct ExportCheck {
    /// The vendored filename to search (JS glue or `.d.ts`).
    pub file: &'static str,
    /// The substring that must be present.
    pub needle: &'static str,
    /// Guidance appended to the failure message.
    pub hint: &'static str,
}

/// A pinned, vendored wasm engine bundle emitted into the docs site.
///
/// One constant per asset captures the whole ritual: the subdir, the runtime files
/// emitted into the site (with their `include_bytes!` bytes), the full set of
/// vendored files the digest manifest pins, the wasm module to structurally probe,
/// and the JS-export surface the site depends on. Both the renderer
/// ([`emit_into`](Self::emit_into)) and the anti-rot test
/// ([`verify`](Self::verify)) read from this single source of truth.
#[derive(Debug, Clone, Copy)]
pub struct VendoredWasmAsset {
    /// The asset subdir under `crates/docs/assets/` and the site emission prefix
    /// (`assets/<name>/`).
    pub name: &'static str,
    /// The runtime files emitted verbatim into the [`Site`](crate::render::Site):
    /// `(filename, bytes)`. The bytes are `include_bytes!` literals (the wasm module
    /// and its JS glue); the `.d.ts` type surface is vendored but not emitted.
    pub emitted_files: &'static [(&'static str, &'static [u8])],
    /// Every vendored filename the `DIGESTS.blake3` manifest pins — exactly the set
    /// the refresh maint target copies out of the built wasm package.
    pub vendored_files: &'static [&'static str],
    /// The vendored wasm module filename (the `\0asm`-magic + size structural probe).
    pub wasm_file: &'static str,
    /// A plausible-size floor for the wasm module; guards an empty/stub blob.
    pub min_wasm_len: usize,
    /// The export-surface probes proving the JS glue still exposes the API the site
    /// depends on.
    pub export_checks: &'static [ExportCheck],
    /// The `make` target that rebuilds + re-vendors this asset (referenced in
    /// failure messages).
    pub refresh_target: &'static str,
    /// The environment variable whose presence makes [`verify`](Self::verify) rewrite
    /// (bless) the digest manifest instead of comparing — set by the refresh target.
    pub bless_env: &'static str,
    /// A per-asset native↔wasm parity attestation path (e.g. `WITNESS.reason.nq`): the
    /// committed native output the shipped wasm engine reproduces byte-for-byte. Its
    /// presence + digest-currency is gated by [`attestation_status`](Self::attestation_status)
    /// (F4/F5). For the three gmeow-owned engines (validate/reason/gmn) the byte-identity
    /// is additionally EXECUTED on-gate by their Node parity lanes (`make check` →
    /// `wasm-parity`); the vendored sibling-repo purrdf engine's witness is its native
    /// `describe` output, with wasm parity owned upstream in the purrdf repo. `Option`
    /// so a future non-witnessed asset need not reshape the descriptor.
    pub witness_attestation: Option<&'static str>,
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

    /// Whether this asset's committed native↔wasm witness-attestation is present AND
    /// current (F4/F5). Current means: the attestation file exists and is non-empty,
    /// AND the on-disk vendored bytes match the pinned `DIGESTS.blake3` — so the engine
    /// the witness proved byte-equivalent to native is EXACTLY the engine that ships.
    ///
    /// An asset with no `witness_attestation` (a non-witnessed asset) is vacuously OK.
    /// Returns a message describing the first failure, so a missing/stale attestation is
    /// a reportable violation rather than a silent gap.
    pub fn attestation_status(&self) -> Result<(), String> {
        let Some(witness) = self.witness_attestation else {
            return Ok(());
        };
        let dir = self.asset_dir();
        match std::fs::read(dir.join(witness)) {
            Ok(bytes) if !bytes.is_empty() => {}
            Ok(_) => {
                return Err(format!(
                    "witness attestation '{witness}' for engine '{}' is empty",
                    self.name
                ));
            }
            Err(e) => {
                return Err(format!(
                    "witness attestation '{witness}' for engine '{}' is missing \
                     (run make {}): {e}",
                    self.name, self.refresh_target
                ));
            }
        }
        let committed = std::fs::read_to_string(dir.join(DIGEST_MANIFEST)).map_err(|e| {
            format!("{DIGEST_MANIFEST} for engine '{}' is missing: {e}", self.name)
        })?;
        if committed != self.current_manifest() {
            return Err(format!(
                "engine '{}' drifted from {DIGEST_MANIFEST}: the witness attestation \
                 '{witness}' no longer describes the shipped bytes (re-run make {})",
                self.name, self.refresh_target
            ));
        }
        Ok(())
    }

    /// The full anti-rot gate for this asset: the vendored `.wasm` is a real module
    /// (WebAssembly magic + plausible size), the JS glue still exposes every declared
    /// export, and the pinned `DIGESTS.blake3` describes the exact on-disk bytes.
    ///
    /// When the asset's [`bless_env`](Self::bless_env) is set, the manifest is
    /// rewritten from the current bytes instead of compared — the path the refresh
    /// maint target drives, so the pinned digests always describe the bytes that
    /// target produced (no external `b3sum` needed).
    ///
    /// # Panics
    ///
    /// Panics (fails the test) on any drift: a corrupt/undersized wasm module, a
    /// missing export surface, or a digest mismatch.
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

        // Export surface: the bindings still carry the API the site depends on.
        for check in self.export_checks {
            let text = std::fs::read_to_string(dir.join(check.file))
                .unwrap_or_else(|e| panic!("vendored {} must exist: {e}", check.file));
            assert!(
                text.contains(check.needle),
                "vendored {} lacks `{}` — {} (re-run make {})",
                check.file,
                check.needle,
                check.hint,
                self.refresh_target
            );
        }

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

/// The vendored purrdf wasm engine — the offline docs SPARQL playground runtime.
///
/// Emitted under `assets/purrdf/`; refreshed by `make maint-refresh-purrdf-asset`.
/// Behaviour (does a query actually evaluate?) is covered by the purrdf Node lane;
/// this descriptor drives the structural + digest anti-rot gate
/// (`crates/docs/tests/purrdf_asset.rs`).
pub static PURRDF_ASSET: VendoredWasmAsset = VendoredWasmAsset {
    name: "purrdf",
    emitted_files: &[
        (
            "gmeow_rdf_wasm.js",
            include_bytes!("../assets/purrdf/gmeow_rdf_wasm.js"),
        ),
        (
            "gmeow_rdf_wasm_bg.wasm",
            include_bytes!("../assets/purrdf/gmeow_rdf_wasm_bg.wasm"),
        ),
    ],
    vendored_files: &[
        "gmeow_rdf_wasm.d.ts",
        "gmeow_rdf_wasm.js",
        "gmeow_rdf_wasm_bg.wasm",
        "gmeow_rdf_wasm_bg.wasm.d.ts",
    ],
    wasm_file: "gmeow_rdf_wasm_bg.wasm",
    min_wasm_len: 100_000,
    export_checks: &[
        ExportCheck {
            file: "gmeow_rdf_wasm.js",
            needle: "query(sparql, base)",
            hint: "vendored bindings lack the Dataset.query method",
        },
        ExportCheck {
            file: "gmeow_rdf_wasm.js",
            needle: "dataset_query",
            hint: "vendored bindings lack the dataset_query wasm import",
        },
        ExportCheck {
            file: "gmeow_rdf_wasm.d.ts",
            needle: "query(sparql: string, base?: string | null): string",
            hint: "vendored .d.ts lacks the query type signature",
        },
    ],
    refresh_target: "maint-refresh-purrdf-asset",
    bless_env: "GMEOW_PURRDF_BLESS",
    // The bundle-explorer describe attestation (`WITNESS.describe.nt`): the native
    // purrdf describe of a deterministic term over the object-level core bundle the
    // wasm engine reproduces (proven by `crates/validate/tests/witness_explore.rs`;
    // the vendored engine IS purrdf's parity-proven wasm build). Task 14 consumes it
    // to gate the explorer's interactive capability.
    witness_attestation: Some("WITNESS.describe.nt"),
};

/// The vendored gmeow-validate-wasm engine — the repo-free Tier-1 GMEOW validator
/// (SHACL + OntoUML disciplines over a `gmeow.gts` bundle) compiled to wasm32.
///
/// Emitted under `assets/validate/`; refreshed by `make maint-refresh-validate-asset`.
/// Behaviour (does a validation actually run?) is covered by the validate-wasm Node
/// lane; this descriptor drives the structural + digest anti-rot gate
/// (`crates/docs/tests/validate_asset.rs`).
pub static VALIDATE_ASSET: VendoredWasmAsset = VendoredWasmAsset {
    name: "validate",
    emitted_files: &[
        (
            "gmeow_validate_wasm.js",
            include_bytes!("../assets/validate/gmeow_validate_wasm.js"),
        ),
        (
            "gmeow_validate_wasm_bg.wasm",
            include_bytes!("../assets/validate/gmeow_validate_wasm_bg.wasm"),
        ),
    ],
    vendored_files: &[
        "gmeow_validate_wasm.d.ts",
        "gmeow_validate_wasm.js",
        "gmeow_validate_wasm_bg.wasm",
        "gmeow_validate_wasm_bg.wasm.d.ts",
    ],
    wasm_file: "gmeow_validate_wasm_bg.wasm",
    min_wasm_len: 100_000,
    export_checks: &[
        ExportCheck {
            file: "gmeow_validate_wasm.js",
            needle: "export function validate(data, format, gts, namespace, origin)",
            hint: "vendored bindings lack the validate export",
        },
        ExportCheck {
            file: "gmeow_validate_wasm.d.ts",
            needle: "export function validate(data: string, format: string, gts: Uint8Array, namespace: string, origin: string): string",
            hint: "vendored .d.ts lacks the validate type signature",
        },
        ExportCheck {
            file: "gmeow_validate_wasm.js",
            needle: "export function bundle_dataset(gts)",
            hint: "vendored bindings lack the bundle_dataset export (the browser bundle-read surface)",
        },
        ExportCheck {
            file: "gmeow_validate_wasm.d.ts",
            needle: "export function bundle_dataset(gts: Uint8Array): string",
            hint: "vendored .d.ts lacks the bundle_dataset type signature",
        },
    ],
    refresh_target: "maint-refresh-validate-asset",
    bless_env: "GMEOW_VALIDATE_BLESS",
    // The native↔wasm parity attestation (`WITNESS.validate.json`): the byte-identical
    // Tier-1 findings the native validator produced and the wasm engine must
    // reproduce (proven by `crates/validate-wasm/js/tests/witness.test.mjs` +
    // `crates/validate/tests/witness_parity.rs`). Task 14 consumes it to gate the
    // interactive validate Capability.
    witness_attestation: Some("WITNESS.validate.json"),
};

/// The vendored gmeow-reason-wasm engine — the native GMEOW structured-DL reasoner
/// (`gmeow-logic`) compiled to wasm32, run SERIALLY in the browser (byte-identical to
/// the parallel native chase). Emitted under `assets/reason/`; refreshed by
/// `make maint-refresh-reason-asset`.
pub static REASON_ASSET: VendoredWasmAsset = VendoredWasmAsset {
    name: "reason",
    emitted_files: &[
        (
            "gmeow_reason_wasm.js",
            include_bytes!("../assets/reason/gmeow_reason_wasm.js"),
        ),
        (
            "gmeow_reason_wasm_bg.wasm",
            include_bytes!("../assets/reason/gmeow_reason_wasm_bg.wasm"),
        ),
    ],
    vendored_files: &[
        "gmeow_reason_wasm.d.ts",
        "gmeow_reason_wasm.js",
        "gmeow_reason_wasm_bg.wasm",
        "gmeow_reason_wasm_bg.wasm.d.ts",
    ],
    wasm_file: "gmeow_reason_wasm_bg.wasm",
    min_wasm_len: 100_000,
    export_checks: &[
        ExportCheck {
            file: "gmeow_reason_wasm.js",
            needle: "export function reason(data, format)",
            hint: "vendored bindings lack the reason export",
        },
        ExportCheck {
            file: "gmeow_reason_wasm.d.ts",
            needle: "export function reason(data: string, format: string): string",
            hint: "vendored .d.ts lacks the reason type signature",
        },
        ExportCheck {
            file: "gmeow_reason_wasm.js",
            needle: "export function conjecture(kb, kb_format, formula, standpoint)",
            hint: "vendored bindings lack the conjecture export (W4 conjecture playground)",
        },
        ExportCheck {
            file: "gmeow_reason_wasm.d.ts",
            needle: "export function conjecture(kb: string, kb_format: string, formula: string, standpoint: string): string",
            hint: "vendored .d.ts lacks the conjecture type signature (W4 conjecture playground)",
        },
    ],
    refresh_target: "maint-refresh-reason-asset",
    bless_env: "GMEOW_REASON_BLESS",
    // The native↔wasm reasoning parity attestation (`WITNESS.reason.nq`): the reasoned
    // closure the native chase produces and the wasm engine reproduces (proven by
    // `crates/reason-wasm/tests/witness_reason.rs` + the Node lane). Task 14 consumes
    // it to gate the LiveReasoning capability.
    witness_attestation: Some("WITNESS.reason.nq"),
};

/// The vendored `gmeow-gmn-wasm` engine — the shipped GMN-0↔GMN-1 codec + glyph
/// symbology compiled to wasm32, run client-side so the docs GMN transcode widget turns
/// authored RDF into the token-compact GMN-1 surface and back with the SAME codec the
/// on-gate authority ships. Refreshed by `make maint-refresh-gmn-asset`.
pub static GMN_ASSET: VendoredWasmAsset = VendoredWasmAsset {
    name: "gmn",
    emitted_files: &[
        (
            "gmeow_gmn_wasm.js",
            include_bytes!("../assets/gmn/gmeow_gmn_wasm.js"),
        ),
        (
            "gmeow_gmn_wasm_bg.wasm",
            include_bytes!("../assets/gmn/gmeow_gmn_wasm_bg.wasm"),
        ),
    ],
    vendored_files: &[
        "gmeow_gmn_wasm.d.ts",
        "gmeow_gmn_wasm.js",
        "gmeow_gmn_wasm_bg.wasm",
        "gmeow_gmn_wasm_bg.wasm.d.ts",
    ],
    wasm_file: "gmeow_gmn_wasm_bg.wasm",
    min_wasm_len: 100_000,
    export_checks: &[
        ExportCheck {
            file: "gmeow_gmn_wasm.js",
            needle: "export function to_gmn1(data, format)",
            hint: "vendored bindings lack the to_gmn1 export",
        },
        ExportCheck {
            file: "gmeow_gmn_wasm.js",
            needle: "export function from_gmn1(gmn1_text)",
            hint: "vendored bindings lack the from_gmn1 export",
        },
        ExportCheck {
            file: "gmeow_gmn_wasm.js",
            needle: "export function glyph_legend()",
            hint: "vendored bindings lack the glyph_legend export the widget's legend needs",
        },
        ExportCheck {
            file: "gmeow_gmn_wasm.d.ts",
            needle: "export function to_gmn1(data: string, format: string): string",
            hint: "vendored .d.ts lacks the to_gmn1 type signature",
        },
    ],
    refresh_target: "maint-refresh-gmn-asset",
    bless_env: "GMEOW_GMN_BLESS",
    // The native↔wasm GMN transcode parity attestation (`WITNESS.gmn1.txt`): the GMN-1
    // surface the native codec writes and the wasm engine reproduces, and which reads
    // back to the input's canonical N-Quads byte-for-byte (proven by
    // `crates/gmn-wasm/tests/witness_gmn.rs` + the Node lane). Task 14 consumes it to
    // gate the GMN transcode capability.
    witness_attestation: Some("WITNESS.gmn1.txt"),
};

/// The vendored engines whose native↔wasm witness-attestation backs each interactive
/// capability (F4/F5). An interactive capability may be REPRESENTED by a format only if
/// every backing engine's attestation is present + current — that is what makes the
/// capability a realized, proven surface rather than a decorative self-claim:
///
/// * `LiveSparql` — the offline SPARQL playground runs on the purrdf engine.
/// * `Interactivity` — the interactive widgets (playground + the Tier-1 validate
///   buttons) run on purrdf + the validator.
/// * `LiveReasoning` — the in-browser structured-DL chase + the GMN transcode run on the
///   reasoner + the GMN codec.
///
/// The non-interactive capabilities (`SearchIndex`, `Diagrams`, `CrossLinkFidelity`) are
/// not engine-backed, so they require no attestation.
#[must_use]
pub fn capability_backing_assets(cap: Capability) -> &'static [&'static VendoredWasmAsset] {
    const LIVE_SPARQL: &[&VendoredWasmAsset] = &[&PURRDF_ASSET];
    const INTERACTIVITY: &[&VendoredWasmAsset] = &[&PURRDF_ASSET, &VALIDATE_ASSET];
    const LIVE_REASONING: &[&VendoredWasmAsset] = &[&REASON_ASSET, &GMN_ASSET];
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
/// for the gmeow-owned engines (validate/reason/gmn) on every `make check`, and the digest
/// pin, which ties the shipped bytes to the attested build (the `maint-refresh-*-asset`
/// targets re-pin only after `*-pkg-test` passes) — it enforces the conjunction
/// "the format declares the capability AND its engine's parity is proven-and-current."
/// So a represented interactive `logic:preservationKind` cannot ship without proven parity:
/// a missing or stale attestation is a HARD FAIL that forbids the capability, never a silent
/// drop. (The vendored sibling-repo purrdf engine's parity is owned upstream; its witness is
/// the native `describe` output, digest-pinned here.)
///
/// Returns one message per (format, capability, engine) violation; an empty vector is a
/// pass. Wired onto the `crate-check` gate surface alongside the loss-lattice gate.
#[must_use]
pub fn check_capability_attestations() -> Vec<String> {
    let mut errors = Vec::new();
    for fmt in DocFormat::ALL {
        for cap in format_capabilities(fmt).representable {
            for asset in capability_backing_assets(cap) {
                if let Err(e) = asset.attestation_status() {
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
