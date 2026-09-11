// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Report-only resource measurements and authenticated JUnit inventory evidence.
//!
//! The library owns the SHA-256, XML and resource-accounting implementations and
//! their synthetic contracts. CLI launchers share that implementation regardless
//! of whether Cargo builds the launchers' test harnesses.

pub mod junit;
pub mod sample;
