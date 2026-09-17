// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact, immutable inputs shared by the fourteen language projection contracts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gmeow_errors::ResultExt;
use serde::de::DeserializeOwned;

use super::{gmn_gate, gmn_pack, grammar_observations};

const STAGE: &str = "stage-mappings";
const PRODUCT_PREFIX: &str = "generated/projections/lang/gmn1/v";
const PRODUCTS: [&str; 3] = [
    "conformance-pack.ttl",
    "token-metrics.ttl",
    "verbalizations.ttl",
];

struct LanguageFixtures {
    root: PathBuf,
    selector_sha256: String,
    action_key: String,
    pack: gmn_pack::Observations,
    gates: gmn_gate::Observations,
    grammars: grammar_observations::Observations,
    products: BTreeMap<String, String>,
    shipped_artifacts: BTreeMap<String, String>,
}

fn fixtures() -> &'static LanguageFixtures {
    // Cache terminal errors too: a failed authenticated load must not multiply its
    // I/O when the named runner continues collecting the other contract failures.
    static FIXTURES: OnceLock<Result<LanguageFixtures, gmeow_errors::Diag>> = OnceLock::new();
    let root = gmeow_conformance::paths::repo_root();
    let selector_sha256 = std::env::var(crate::fixture::STAGE_FIXTURE_MANIFEST_SHA256_ENV)
        .expect("language contracts require the exact producer selector digest");
    let fixtures = FIXTURES
        .get_or_init(|| load(&root, &selector_sha256))
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated language contract inputs: {error}"));
    assert_eq!(
        fixtures.root, root,
        "shared language inputs cannot change repository roots"
    );
    assert_eq!(
        fixtures.selector_sha256, selector_sha256,
        "shared language action {} cannot cross selector identities",
        fixtures.action_key
    );
    fixtures
}

pub(crate) fn pack() -> &'static gmn_pack::Observations {
    &fixtures().pack
}

pub(crate) fn gates() -> &'static gmn_gate::Observations {
    &fixtures().gates
}

pub(crate) fn shipped_artifacts() -> &'static BTreeMap<String, String> {
    &fixtures().shipped_artifacts
}

pub(super) fn grammar(name: &str) -> &'static grammar_observations::GrammarObservation {
    fixtures()
        .grammars
        .grammars
        .get(name)
        .expect("required authored grammar")
}

pub(crate) fn product(suffix: &str) -> &'static str {
    fixtures()
        .products
        .get(suffix)
        .expect("selected native language product")
}

fn load(root: &Path, selector_sha256: &str) -> Result<LanguageFixtures, gmeow_errors::Diag> {
    // Resolve versioned paths from authenticated selector metadata. This is only
    // selection; authenticated_artifacts below still verifies the complete action
    // blob and ALL artifact commitments before any selected bytes are retained.
    let receipt = crate::fixture::selected_stage_receipt(root, STAGE)?;
    let mut selected_major = None;
    let mut product_paths = BTreeMap::new();
    for suffix in PRODUCTS {
        let matching: Vec<_> = receipt
            .logical_artifacts
            .iter()
            .filter_map(|entity| {
                let versioned = entity.identity.strip_prefix(PRODUCT_PREFIX)?;
                let (major, artifact) = versioned.split_once('/')?;
                (artifact == suffix).then_some((major, entity.identity.as_str()))
            })
            .collect();
        let [(major, path)] = matching.as_slice() else {
            return Err(fail(format!(
                "selected action requires exactly one {suffix}, found {}",
                matching.len()
            )));
        };
        if major.is_empty() || !major.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(fail(format!(
                "selected language artifact has malformed dialect major {major:?}"
            )));
        }
        if selected_major.is_some_and(|selected| selected != *major) {
            return Err(fail(
                "selected language products disagree on their version subtree",
            ));
        }
        selected_major = Some(*major);
        product_paths.insert(suffix.to_owned(), (*path).to_owned());
    }
    let mut paths = vec![
        gmn_pack::CHANNEL,
        grammar_observations::CHANNEL,
        gmn_gate::CHANNEL,
    ];
    paths.extend(product_paths.values().map(String::as_str));
    let mut artifacts = crate::fixture::authenticated_artifacts(root, STAGE, &paths)?;
    let pack: gmn_pack::Observations = decode(&mut artifacts, gmn_pack::CHANNEL)?;
    let gates = decode(&mut artifacts, gmn_gate::CHANNEL)?;
    let grammars = decode(&mut artifacts, grammar_observations::CHANNEL)?;
    if selected_major != Some(pack.major.as_str()) {
        return Err(fail(
            "native pack observations disagree with the selected artifact version",
        ));
    }
    // The same complete action authentication verified these commitments. Keep
    // only their small path/digest pairs, never every shipped document's bytes.
    let version_dir = format!("{PRODUCT_PREFIX}{}/", pack.major);
    let shipped_artifacts = receipt
        .logical_artifacts
        .iter()
        .filter(|entity| {
            entity
                .identity
                .strip_prefix(&version_dir)
                .is_some_and(|suffix| suffix.ends_with(".gmn") && !suffix.contains('/'))
        })
        .map(|entity| (entity.identity.clone(), entity.digest.clone()))
        .collect();
    let mut products = BTreeMap::new();
    for (suffix, path) in product_paths {
        let bytes = artifacts
            .remove(&path)
            .ok_or_else(|| fail(format!("authenticated language action omitted {path}")))?;
        let text =
            String::from_utf8(bytes).with_ctx(|| format!("native language product {path}"))?;
        products.insert(suffix, text);
    }
    if !artifacts.is_empty() {
        return Err(fail(
            "language fixture selection retained unrequested artifacts",
        ));
    }
    Ok(LanguageFixtures {
        root: root.to_path_buf(),
        selector_sha256: selector_sha256.to_owned(),
        action_key: receipt.action_key,
        pack,
        gates,
        grammars,
        products,
        shipped_artifacts,
    })
}

fn decode<T: DeserializeOwned>(
    artifacts: &mut BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<T, gmeow_errors::Diag> {
    let bytes = artifacts
        .remove(path)
        .ok_or_else(|| fail(format!("authenticated language action omitted {path}")))?;
    serde_json::from_slice(&bytes).with_ctx(|| format!("native language observations {path}"))
}

fn fail(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: STAGE.to_owned(),
        message: message.into(),
    })
}
