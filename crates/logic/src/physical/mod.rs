// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native physical execution core (#1090, F3).
//!
//! The destination is ONE native Rust engine: Scryer/Nemo/oxigraph are
//! bootstrap oracles, not the runtime. This module hosts that engine's working
//! representation — starting with the columnar [`RelationStore`] and the single
//! oxigraph → columnar bridge [`extract_edb`].
//!
//! # Phase dead code
//!
//! Like [`crate::rule_ir`], the early rungs of this engine land before the
//! forward/backward evaluators that consume them, so the not-yet-wired surface
//! allows `dead_code` module-internally rather than scattering per-item attributes
//! that would be unwound the next rung.
#![allow(dead_code)]

mod store;

// Phase-A: these are the engine's public-to-crate surface, consumed by the
// forward/backward evaluators landing on the next rung. Until then the re-export is
// unused crate-wide, so allow it here rather than dropping the intended API.
#[allow(unused_imports)]
pub(crate) use store::{extract_edb, Bound, RelationStore};
