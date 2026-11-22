// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! JavaScript host inputs for SPARQL in the browser engines.

#![cfg(target_arch = "wasm32")]

use purrdf::sparql::host_source::{self, HostSource};
use wasm_bindgen::JsError;

#[derive(Debug)]
struct BrowserHost;

impl HostSource for BrowserHost {
    fn unix_millis(&self) -> f64 {
        js_sys::Date::now()
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "each Math.random draw scaled by 2^32 is in the u32 range"
    )]
    fn entropy_seed(&self) -> u64 {
        let word = || u64::from((js_sys::Math::random() * 4_294_967_296.0) as u32);
        (word() << 32) | word()
    }
}

/// Install the browser host source for one WebAssembly instance.
///
/// # Errors
/// Returns an error if the instance has already installed a source.
pub fn install() -> Result<(), JsError> {
    host_source::install(&BrowserHost).map_err(|error| JsError::new(&error.to_string()))
}

/// Register the SPARQL host at startup in a WebAssembly module that evaluates queries.
#[macro_export]
macro_rules! install_sparql_host_on_start {
    () => {
        /// Install the evaluator's clock and entropy when the module starts.
        ///
        /// # Errors
        /// Returns an error if the instance has already installed a source.
        #[::wasm_bindgen::prelude::wasm_bindgen(start)]
        pub fn install_sparql_host() -> Result<(), ::wasm_bindgen::JsError> {
            $crate::install()
        }
    };
}
