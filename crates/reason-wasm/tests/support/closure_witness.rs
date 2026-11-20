// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure native computation of the small, explicit browser closure witness.

/// A self-contained EDB whose structured-DL closure is non-empty: an RDFS subclass
/// axiom + a typed individual entails the individual's membership in the superclass.
pub const INPUT: &str = "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix ex: <https://example.org/> .\n\
     ex:Cat rdfs:subClassOf ex:Animal .\n\
     ex:Animal rdfs:subClassOf ex:Organism .\n\
     ex:felix rdf:type ex:Cat .\n";

/// The exact pipeline the wasm `reason(data, format)` shim runs.
pub fn reason_artifact(data: &str, format: &str) -> String {
    gmeow_reason_wasm::reason(data, format)
        .expect("the tiny caller-owned theory publishes a complete admitted artifact")
}
