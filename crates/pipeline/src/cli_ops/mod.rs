// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native CLI-glue for the remaining Table-2 `gmeow_tools` surfaces.
//!
//! Each concern here is a PyO3-free, consumer-safe driver a `gmeow` / `gmeow-dev`
//! binary calls by one stable name. Two kinds live in this module:
//!
//! 1. **Genuine ports** — surfaces with no prior native equivalent:
//!    * [`temporal`] — the TQL executor (the port of `gmeow_tools.temporal_query`):
//!      load a named parameterized SPARQL 1.1 temporal query and run it over the
//!      events model through the native [`purrdf::sparql::NativeSparqlEngine`], with
//!      injection-free parameter pre-binding.
//!    * [`okf_import`] — the OKF lift lane (the port of `gmeow_tools.okf_import`):
//!      fold a bundle directory in-process through purrdf 0.7.0's native OKF codec
//!      (`purrdf::lift_okf_bundle`, no external binary), lift the recognized
//!      `okf:` predicates to the `rdfs:`/`skos:`/`rdf:` surface, and drive the
//!      native MAXIMAL(G) back-half.
//!    * [`quality`] — the OOPS!/FOOPS! network scorers: blocking HTTP over the
//!      already-vendored `ureq`.
//!
//! 2. **Thin confirmations** — surfaces whose logic is ALREADY native; these
//!    [`confirmations`] wrappers only expose the confirmed native authority under
//!    one call name so a bin need not reach into the internal module, and never
//!    duplicate the native logic:
//!    * up-projection gate audit ← [`crate::up_projection_gates::gate_derived_audit`]
//!    * mappings compile ← [`crate::stages::mappings::compile_mappings`]
//!    * Turtle normalization ← `purrdf::turtle_normalize::canonical_turtle`

pub mod confirmations;
pub mod okf_import;
pub mod quality;
pub mod temporal;
