// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-validate` — the Rust host for the GMEOW validation-path lints.
//!
//! This crate carries the two lowest-risk lints —
//! per-file Turtle syntax checking ([`store::parse_file_dataset`]) and the Principle 5
//! `owl:sameAs`-to-external-entity ban (store-scan consumers) —
//! keeping both checks on the native validation path end-to-end.
//!
//! # Platform posture
//!
//! The repo-free **Tier-1** validator ([`data_validate`]) compiles for
//! `wasm32-unknown-unknown`: it runs SHACL + the OntoUML disciplines over an
//! in-memory RDF data graph and a `gmeow.gts` byte blob, with no reasoner,
//! filesystem, threading, or PyO3 coupling. The wasm-clean core modules
//! ([`model`], [`codes`], [`store`], [`gufo`], [`findings`], [`data_validate`],
//! [`report_bridge`]) are compiled on every target.
//!
//! So is the whole REPORT-SHAPING path around that core: the advisory split
//! ([`advisory`]), the constructive [`abductive`] tier, finding [`enrich`]ment, the
//! per-term [`guidance`] reader, [`remediation`], [`distinctiveness`], and
//! [`language_tags`]. A browser-run `validate_local` therefore returns the same
//! enriched, advisory-split report the native CLI does, rather than a raw one.
//!
//! The remainder — the slice-authoring dev gate ([`validate_all`], with the
//! native DL reasoner + rayon), the repo-lint guards, the DSL phases, and the
//! Wikidata/HTTP lanes — is **native-only** (`#[cfg(not(target_arch = "wasm32"))]`),
//! because each is coupled to the repository on disk or to a native-only engine.
//! The Tier-2 `--deep` semantic pass is excluded from the wasm surface by contract,
//! not degraded: the wasm boundary reaches validation solely through
//! [`data_validate::run_tier1`].
//!
//! # Engine core separation
//!
//! The engine modules are PyO3-free so the rlib links into the native Rust
//! toolchain (and the wasm target) without Python.

// Wasm-clean Tier-1 core: compiled on every target.
pub mod codes;
pub mod data_validate;
pub mod error;
pub mod findings;
pub mod gufo;
// The loop-closure enrichment join (correspondence of a validation Report against
// the documentation projection). Pure serde over the wasm-clean Finding/Report model
// and `rule_catalog::help_uri_for` — no RDF store, pipeline carrier, or reasoner — so
// the WASM-interactive-docs sibling reuses it verbatim.
pub mod local_oracle;
// The single authority for the localizable-predicate set (wasm-clean: a pure const).
// gmeow-docs re-exports it; the pipeline/slice consumers import it from here.
pub mod localizable;
pub mod model;
pub mod projection_profiles;
pub mod report_bridge;
// The rule-identity registry's anchor transform (`help_uri_for` / `slugify`) is pure
// string work over `crate::codes` + the `gmeow_errors` model — wasm-clean — so it is
// compiled on every target for `local_oracle` (and the wasm docs sibling) to resolve
// a finding code to its catalog help URI.
pub mod rule_catalog;
pub mod store;

// The rest of the crate. A module carrying `#[cfg(not(target_arch = "wasm32"))]` is
// native-only because it pulls a native-only crate (gmeow-logic, rayon, ureq,
// gmeow-slice) or the filesystem: the slice-authoring dev gate, the repo-lint guards,
// the DSL phases, the Tier-2 reasoner path. A module below WITHOUT that attribute is
// wasm-clean and compiled on every target — its own comment says which browser surface
// requires it. The two sets are listed together, in one alphabetical run, so the gate
// on each module sits next to the module it gates.
// The constructive "what to ADD" tier. Wasm-clean: `gmeow_errors` + `purrdf` +
// `gmeow_logic_compile`'s formula frontend + `sha2`, all of which cross-compile. Gated
// only by its former block; the consumer advisory surface carries it on both targets.
pub mod abductive;
// The advisory-tier emission seam. Wasm-clean: its whole dependency set is
// `gmeow_errors` + `purrdf`'s SHACL report model + `sha2`. It was gated only by sitting
// inside the native-only block; the browser-runnable MCP `validate_local` / `advise`
// surface applies the very same advisory split, so it belongs on both targets.
pub mod advisory;
// The ontology-surface authoring gates (shape-IRI ownership, profile/catalog
// closure, term-declaration + language-tag discipline, graft isolation, slice
// discipline) — native-only: they read the on-disk slice/shape corpus via
// `purrdf::slice` + `purrdf::shapes`.
#[cfg(not(target_arch = "wasm32"))]
pub mod authoring_integrity;
#[cfg(not(target_arch = "wasm32"))]
pub mod box_roles;
#[cfg(not(target_arch = "wasm32"))]
pub mod cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod compliance;
#[cfg(not(target_arch = "wasm32"))]
pub mod constitution;
#[cfg(not(target_arch = "wasm32"))]
pub mod coverage;
#[cfg(not(target_arch = "wasm32"))]
pub mod crate_layering;
#[cfg(not(target_arch = "wasm32"))]
pub mod crossref;
// Pure computation (its only import is `std::collections`) — no filesystem, no
// network. It was gated only by sitting inside the native-only block; the
// browser-side slice-quality axes need it, so it belongs on both targets.
pub mod distinctiveness;
#[cfg(not(target_arch = "wasm32"))]
pub mod dsl;
#[cfg(not(target_arch = "wasm32"))]
pub mod dsl_shacl;
// The single proof-carrying enrichment entry point (`enrich_findings`) over a
// consumer Report — attaches rule identity + registry-authored remediation. Shared by
// the CLI validate/verify path, the pipeline validate stage, and the browser-runnable
// `validate_local`, so none of those surfaces can drift from the others. Wasm-clean
// (its imports are `gmeow_errors` + `purrdf` + `rule_catalog`), so it is compiled on
// every target.
pub mod enrich;
// The per-term usage-guidance reader: joins a finding onto the
// `gmeow:howToUse`/`gmeow:useWhen`/`gmeow:avoidWhen` prose authored on ontology terms,
// from both the finding's rule-governing term(s) and its own `documented_terms`. Reads
// an `RdfDataset` directly. Wasm-clean (`gmeow_errors` + `purrdf` only) and read by
// `enrich`, so it is compiled on every target.
pub mod guidance;
#[cfg(not(target_arch = "wasm32"))]
pub mod instance;
// Pure computation over an `RdfDataset` / literal set (its only imports are
// `std::collections` and `purrdf`) — no filesystem, no network. Like
// `distinctiveness` it was gated only by sitting inside the native-only block; the
// read-side bundle views need the internal-tag → BCP-47 remap and the public-text
// selection/fallback on both targets, so it belongs on both.
pub mod language_tags;
#[cfg(not(target_arch = "wasm32"))]
pub mod lint;
#[cfg(not(target_arch = "wasm32"))]
pub mod mapping_eval;
// The registry-authored remediation attachment. Wasm-clean (`gmeow_errors` +
// `rule_catalog`) and read by `enrich`, so it is compiled on every target.
pub mod remediation;
#[cfg(not(target_arch = "wasm32"))]
pub mod repo_static;
#[cfg(not(target_arch = "wasm32"))]
pub mod self_desc;
#[cfg(not(target_arch = "wasm32"))]
pub mod shape_grounding;
#[cfg(not(target_arch = "wasm32"))]
pub mod shape_oracle;
#[cfg(not(target_arch = "wasm32"))]
pub mod signature;
#[cfg(not(target_arch = "wasm32"))]
pub mod slice_ownership;
// The peerage-aware projection of `slice_ownership`'s undeclared-dependency
// diagnostics: joins an undeclared semantic edge to the grounding-peerage
// relation + seam registry so a registered `lang:`/`math:`/`logic:` crossing is
// suppressed instead of HARD-FAILing, while an unregistered one still gates.
#[cfg(not(target_arch = "wasm32"))]
pub mod slice_peerage;
#[cfg(not(target_arch = "wasm32"))]
pub mod statement;
#[cfg(not(target_arch = "wasm32"))]
pub mod time_util;
#[cfg(not(target_arch = "wasm32"))]
pub mod validate_all;
#[cfg(not(target_arch = "wasm32"))]
pub mod wikidata_audit;
