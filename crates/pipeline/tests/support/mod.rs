// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared test-support modules for the `crates/pipeline` integration tests.
//!
//! Each integration-test file that needs a helper does `mod support;` and reaches into
//! the submodules below. Because every `tests/*.rs` file is its own crate, this module is
//! recompiled into each test binary that declares it; a submodule not referenced by a given
//! test is (harmlessly) dead in that binary, so each submodule is `#![allow(dead_code)]` at
//! its own head rather than here.

pub mod flagship_discharge;
pub mod math_projection_producer;
