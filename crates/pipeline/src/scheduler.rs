// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The level-parallel scheduler (#861 P2).
//!
//! Stages run by topological level (from [`crate::graph::StageGraph`]); within a
//! level, independent stages run in parallel (rayon), except `Reason` stages,
//! which serialize under the process-wide [`ENGINE_LOCK`] because the underlying
//! Nemo/Scryer engines hold their own global locks. The final dataset merge is
//! value-interned (order-independent), so the result is identical regardless of
//! completion order — the determinism the P2 tests pin.
//!
//! P1 ships only the engine lock; the rayon level executor lands in P2.

use std::sync::{LazyLock, Mutex};

/// Serializes execution of every `Reason` stage. Mirrors the `CHASE_LOCK` in
/// `gmeow-logic` (the Nemo/Scryer engines are not concurrency-safe). A permit,
/// not data — results are returned, never stored behind the lock.
pub static ENGINE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
