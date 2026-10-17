// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Monotonic execution observations, separate from logical time and verdicts.
//! Browser builds require the host's Performance clock; no fabricated duration or
//! weaker wall-clock fallback is permitted.

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub(crate) use std::time::Instant;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
use wasm_bindgen::prelude::*;

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[wasm_bindgen]
extern "C" {
    /// Read the host's monotonic time in milliseconds.
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now() -> f64;
}

/// A reading of the required browser/worker/Node monotonic clock, in milliseconds.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub(crate) struct Instant(f64);

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
impl Instant {
    /// Read the host clock; missing clock support is a host contract failure.
    pub(crate) fn now() -> Self {
        let reading = performance_now();
        assert!(
            reading.is_finite() && reading >= 0.0,
            "invalid Performance clock reading"
        );
        Self(reading)
    }

    /// Measure elapsed execution time without changing reasoning semantics.
    pub(crate) fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64((Self::now().0 - self.0) / 1000.0)
    }
}
