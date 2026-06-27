// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! External-corpus ingestion adapter (#753, X1 keystone of epic #752).
//!
//! Lowers third-party standard correctness suites — TPTP SZS problems and W3C
//! `manifest.ttl` entailment tests — into the runner's verdict schema and per-case
//! anatomy, so the language-neutral conformance runner can be graded against
//! *external ground truth* rather than only its own self-generated goldens.
//!
//! The discovery host (`crate::discover`) is category-agnostic, so a lowered case
//! under `conformance/logic/cases/external/<corpus>/<case>/` auto-registers. This
//! module is the format adapter that lowers INTO that anatomy:
//!
//! * [`status`] — the single SZS/manifest → [`serialize::VerdictStatus`](crate::serialize::VerdictStatus)
//!   mapping table.
//! * [`szs`] — TPTP `% SZS status` ingestion.
//! * [`manifest`] — W3C `mf:` entailment-manifest ingestion (dogfoods
//!   `gmeow_rdf::parse_dataset`).
//! * [`lower`] — pure lowering: the AC1 `runner_verdict_json` surface and the
//!   consistency-case input scaffold.
//!
//! Zero-defer: lowered Lane-A cases are decided by the native engine (the
//! consistency path's `gaps`-empty guard enforces it); heavy third-party corpora
//! that exceed the native fragment are the Lane-B (`make maint-classic-cross-check`)
//! destination, vendored by X2–X5 on top of the convention established here.

pub mod lower;
pub mod manifest;
pub mod status;
pub mod szs;

pub use lower::{lower_consistency_inputs, runner_verdict_json, LoweredInputs};
pub use manifest::{parse_entailment_manifest, EntailmentKind, ManifestEntry};
pub use status::{outcome_for_szs, ExternalOutcome};
pub use szs::{outcome_from_szs, parse_szs_status};
