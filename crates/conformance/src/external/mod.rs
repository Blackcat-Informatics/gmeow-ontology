// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! External-corpus ingestion adapter.
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
//! * [`tptp`] — TPTP FOF/CNF *problem-body* ingestion: a native FOF/CNF parser
//!   that lowers a problem into the full-FOL [`Formula`](gmeow_logic_compile::ir::Formula)
//!   IR, applies the FOL-negation reduction, and lowers the EL/DL-expressible
//!   fragment to a world-scoped OWL-RDF EDB the native DL engine decides. A
//!   construct outside that fragment is a typed capability gap, never a silent skip.
//! * [`manifest`] — W3C `mf:` entailment-manifest ingestion (dogfoods
//!   `purrdf::parse_dataset`).
//! * [`ontouml`] — FAIR OntoUML/UFO catalog ingestion: a native Turtle reader
//!   lifts the metamodel into a typed model, lowers it onto the world-scoped
//!   all-IRI `logic:` stereotype ABox the foundation-discipline chase consumes,
//!   and grades the fired discipline set against a documented anti-pattern label.
//!   A stereotype or mediation shape outside the five-discipline fragment is a
//!   typed capability gap, never a silent skip.
//! * [`lower`] — the pure AC1 `runner_verdict_json` surface (declared external
//!   outcome → runner verdict value).
//!
//! Zero-defer: lowered Lane-A cases are decided by the native engine (the
//! consistency path's `gaps`-empty guard enforces it); heavy third-party corpora
//! that exceed the native fragment are the Lane-B (`make -C validations/classic-cross-check validate`)
//! destination, graded live on top of the convention established here.

pub mod lower;
pub mod manifest;
pub mod ontouml;
pub mod status;
pub mod szs;
pub mod tptp;

pub use lower::runner_verdict_json;
pub use manifest::{
    ManifestEntry, ManifestTestKind, OntologyDoc, manifest_entries, parse_test_manifest,
    parse_test_manifest_rdfxml,
};
pub use ontouml::{
    DisciplineVerdict, Generalization, LOGIC_NS as ONTOUML_LOGIC_NS, Mediation, ONTOUML_NS,
    OntoClass, OntoumlError, OntoumlModel, VIOLATION_PRED, compare, fired_disciplines,
    lower_and_evaluate, lower_model, native_verdict_string, parse_ontouml_model,
};
pub use status::{ExternalOutcome, outcome_for_szs};
pub use szs::{outcome_from_szs, parse_szs_status};
pub use tptp::{
    AnnotatedFormula, LoweredProblem, LoweringGap, TPTP_NS, TptpError, TptpRole, TptpSource,
    TstpTerm, lower_and_decide, lower_problem, lower_to_fol_program, parse_tptp,
};
