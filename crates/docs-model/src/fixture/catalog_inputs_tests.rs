// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Docs qualification rejects stale authored input, spliced catalog receipts
/// and bytes that disagree with a catalog commitment.
#[test]
fn docs_fixture_rejects_stale_inputs_and_spliced_catalogs() {
    let constraints: &[u8] = b"supplied constraints";
    let manifest: &[u8] = b"supplied manifest";
    let inputs = |generation: &str| {
        CATALOG_ARTIFACTS
            .into_iter()
            .zip([constraints, manifest])
            .flat_map(move |((stage, path), bytes)| {
                [
                    ActionInput::Upstream {
                        producer: stage.to_owned(),
                        entity: Some(path.to_owned()),
                        receipt_digest: Some(format!("{stage}-{generation}")),
                        product_digest: format!("{stage}-product"),
                    },
                    ActionInput::Raw {
                        logical_path: path.to_owned(),
                        file_kind: FileKind::File,
                        executable: false,
                        digest: purrdf::ContentDigest::of(bytes).to_hex(),
                    },
                ]
            })
            .collect::<Vec<_>>()
    };
    let selected =
        |model_inputs| model_context_for_inputs("authored-source".to_owned(), model_inputs);
    let live = selected(inputs("first"));
    let verify = |model, authored: &str, generation| {
        verify_catalog_model_context(model, authored.to_owned(), inputs(generation))
    };
    verify(&live, "authored-source", "first").expect("matching authored and catalog inputs");
    assert!(verify(&live, "authored-source", "second").is_err());
    assert!(verify(&live, "edited-source", "first").is_err());
    assert!(verify(&selected(Vec::new()), "authored-source", "first").is_err());
    verify_catalog_bytes(&live, [constraints, manifest]).expect("matching catalog bytes");
    assert!(verify_catalog_bytes(&live, [b"different constraints", manifest]).is_err());
}
