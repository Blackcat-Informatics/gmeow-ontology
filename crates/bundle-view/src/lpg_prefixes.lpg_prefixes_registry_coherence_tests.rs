// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;

use gmeow_logic_compile::ingest::PREFIX_REGISTRY;

use super::PREFIXES_BY_LEN;

/// GMEOW-local namespaces live under this host.
const GMEOW_HOST: &str = "https://blackcatinformatics.ca/";

/// The affect-classifier label registries (`gmeow-goemotions`/`gmeow-hf`/
/// `gmeow-labelset`) are GMEOW-local per the host filter, but they are external
/// per-model label identities used only by the affect ingest/conformance
/// machinery (see `crates/affect-ingest`, `crates/validate/tests/conformance_cases/conformance_affect*`)
/// — they never surface in an LPG/consumer/metadata/export projection, so
/// `PREFIXES_BY_LEN` deliberately omits them. Every OTHER GMEOW-local namespace
/// (the four grounding namespaces plus `vcardx`) is required.
const REGISTRY_EXEMPT_PREFIXES: &[&str] = &["gmeow-goemotions", "gmeow-hf", "gmeow-labelset"];

#[test]
fn mirrors_every_non_exempt_gmeow_local_registry_namespace() {
    let table: BTreeMap<&str, &str> = PREFIXES_BY_LEN.iter().copied().collect();

    for (prefix, ns) in PREFIX_REGISTRY.iter().copied() {
        if !ns.starts_with(GMEOW_HOST) || REGISTRY_EXEMPT_PREFIXES.contains(&prefix) {
            continue;
        }
        match table.get(prefix) {
            Some(mirrored_ns) => assert_eq!(
                *mirrored_ns, ns,
                "PREFIXES_BY_LEN prefix `{prefix}` maps to `{mirrored_ns}`, but the \
                     canonical PREFIX_REGISTRY maps `{prefix}` to `{ns}` — \
                     crates/bundle-view/src/lpg_prefixes.rs has drifted from \
                     gmeow_logic_compile::ingest::PREFIX_REGISTRY for GMEOW-local prefix \
                     `{prefix}`",
            ),
            None => panic!(
                "PREFIXES_BY_LEN is missing GMEOW-local prefix `{prefix}` (namespace \
                     `{ns}`) that is present in the canonical PREFIX_REGISTRY — add it to \
                     PREFIXES_BY_LEN in crates/bundle-view/src/lpg_prefixes.rs",
            ),
        }
    }
}

#[test]
fn all_four_grounding_prefixes_present_with_expected_iris() {
    let table: BTreeMap<&str, &str> = PREFIXES_BY_LEN.iter().copied().collect();
    for (prefix, expected_ns) in [
        ("gmeow", "https://blackcatinformatics.ca/gmeow/"),
        ("logic", "https://blackcatinformatics.ca/logic/"),
        ("math", "https://blackcatinformatics.ca/math/"),
        ("lang", "https://blackcatinformatics.ca/lang/"),
    ] {
        assert_eq!(
            table.get(prefix).copied(),
            Some(expected_ns),
            "PREFIXES_BY_LEN must contain the grounding prefix `{prefix}` mapped to \
                 `{expected_ns}` — the four grounding namespaces (gmeow/logic/math/lang) are \
                 contractually required to be present",
        );
    }
}
