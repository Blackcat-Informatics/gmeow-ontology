// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Hermetic tests for the cache machinery itself — model builds belong only
//! to the explicit producer. These pin the model envelope round
//! trip, the content-addressing contract, the derived implementation closure,
//! and the integrity-violation panic so a key/envelope regression fails here,
//! not as a confusing downstream golden.
use super::*;
use gmeow_build_inputs::{CfgContext, InputInventory, ProductionSelection, SCHEMA, UnitSelection};

fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// All crates in these inert fixtures have one unconditional library target.
/// Their explicitly constructed unit selection stands in for Cargo's output.
fn cache_key(root: &Path) -> String {
    let manifests =
        gmeow_build_inputs::declared_path_dependency_manifests(root, "crates/docs/Cargo.toml")
            .unwrap();
    let units: Vec<_> = manifests
        .iter()
        .map(|manifest| {
            let directory = Path::new(manifest).parent().unwrap();
            UnitSelection {
                package: format!("path+file://<workspace>/{}#fixture@1", directory.display()),
                manifest: Some(manifest.clone()),
                source: Some(directory.join("src/lib.rs").to_str().unwrap().to_owned()),
                target: "fixture".into(),
                kinds: vec!["lib".into()],
                cfg: CfgContext::from_rustc(
                    "unix\ntarget_arch=\"x86_64\"\ntarget_os=\"linux\"",
                    &[],
                    true,
                )
                .unwrap(),
                dependencies: vec![],
                dependency_names: vec![],
                controller: false,
            }
        })
        .collect();
    let selection = ProductionSelection {
        schema: SCHEMA,
        roots: (0..units.len()).collect(),
        units,
        policy_files: vec![],
    };
    let implementation = InputInventory::collect(root, &selection)
        .unwrap()
        .digest()
        .unwrap();
    cache_key_with_implementation(root, &implementation)
}

fn model_context(root: &Path) -> ActionContext {
    model_context_for_digest(cache_key(root))
}
fn cache_path(root: &Path) -> PathBuf {
    cache_path_for_context(root, &model_context(root))
}

/// A fresh inert implementation fixture with no ontology corpus. The root is
/// owned by the returned
/// [`tempfile::TempDir`], which removes the whole tree when it drops — on
/// success, on panic, and on early return. Uniqueness comes from the guard,
/// so the tag is purely a readable name for the root inside it. Callers must
/// bind the guard (`let (_tmp, root) = temp_root("key");`); binding it to a
/// bare `_` drops it immediately and deletes the root out from under the test.
fn temp_root(tag: &str) -> (tempfile::TempDir, PathBuf) {
    let guard = tempfile::tempdir().expect("create temp dir");
    let root = guard.path().join(tag);
    fs::create_dir_all(&root).expect("create temp root");
    write(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/docs']\n",
    );
    write(&root, "Cargo.lock", "version=4\npackage=[]\n");
    write(
        &root,
        "crates/docs/Cargo.toml",
        "[package]\nname='fixture'\nversion='1.0.0'\n",
    );
    write(&root, "crates/docs/src/lib.rs", "pub fn fixture() {}\n");
    (guard, root)
}

/// **The write→read proof, across the real serde boundary.** A model envelope
/// published by [`ActionStore`] and read back by the loader that serves warm hits
/// must VERIFY.
///
/// An in-memory `from_model(..).into_model(..)` round trip cannot see this class:
/// the digest is folded over a re-serialization of the payload, so a payload field
/// that does not survive JSON — one whose `skip_serializing_if` has no matching
/// `default`, or a map whose iteration order is not the wire order — folds to one
/// value before the write and another after the read, and the guard fires on every
/// warm hit even though nothing was edited. That is exactly the failure the SHACL
/// verdict's own self-digest shipped with (its digest was folded over the pre-render
/// report while the file carried the normalized one), so the docs fixture's analogous
/// guard is proven here rather than assumed. The renderer's `CachedSite` twin of this
/// proof lives beside its envelope in `gmeow_docs::fixture`.
#[test]
fn a_model_envelope_written_to_disk_verifies_when_read_back() {
    let (_tmp, root) = temp_root("disk-round-trip");
    let model_path = cache_path(&root);
    let model = DocsModel::default();
    let cached = CachedModel::from_model(&model);
    let bytes = serde_json::to_vec(&cached).unwrap();
    let context = model_context(&root);
    let store = action_store(&root);
    store
        .publish(
            &context,
            cached.digest.clone(),
            model_payload(&context),
            &bytes,
        )
        .unwrap();
    let hit = store
        .get::<DocsActionPayload>(&context)
        .unwrap()
        .expect("warm model action");
    let recovered = decode_model(&model_path, &hit.bytes).unwrap();
    assert_eq!(
        recovered.available_languages, model.available_languages,
        "the reattached i18n fields survive the disk round trip"
    );
}

/// A consumer loads the selected producer action even when its checkout differs;
/// it may neither derive a replacement identity nor accept a changed receipt.
#[test]
fn selected_model_survives_consumer_source_edits_and_refuses_receipt_changes() {
    let (_tmp, root) = temp_root("selected-model");
    let context = model_context(&root);
    let cached = CachedModel::from_model(&DocsModel::default());
    let bytes = serde_json::to_vec(&cached).unwrap();
    let store = action_store(&root);
    let receipt = store
        .publish(
            &context,
            cached.digest.clone(),
            model_payload(&context),
            &bytes,
        )
        .unwrap();
    let selected = SelectedAction::from_receipt(&receipt);
    fs::create_dir_all(root.join("slices")).unwrap();
    fs::write(
        root.join("slices/changed.ttl"),
        b"different consumer checkout",
    )
    .unwrap();
    assert_ne!(
        context,
        model_context(&root),
        "the producer must invalidate on changed inputs"
    );
    let (_, identity) = load_selected_model(&root, &selected).unwrap();
    assert_eq!(identity.receipt_digest, receipt.digest());

    let mut wrong = selected.clone();
    wrong.receipt_digest = "0".repeat(64);
    assert!(load_selected_model(&root, &wrong).is_err());
    let mut wrong = selected.clone();
    wrong.product_digest = "wrong product".to_string();
    assert!(load_selected_model(&root, &wrong).is_err());
    let mut wrong = selected.clone();
    wrong.context.codec = "unselected codec".to_string();
    assert!(load_selected_model(&root, &wrong).is_err());
    fs::remove_file(store.receipt_path(&context.key())).unwrap();
    assert!(load_selected_model(&root, &selected).is_err());
    assert!(
        !store.receipt_path(&context.key()).exists(),
        "a miss never publishes a replacement"
    );
}

/// The model envelope carries the payload guard, over the whole reconstructed payload.
///
/// This is the whole point of the payload digest: the key content-addresses the
/// INPUTS, so editing the cached OUTPUT leaves it satisfied. `.cache/` is gitignored
/// and persists, so an entry edited once would keep being served — and the
/// `DocMaturity` quality axis reads its coverage computation straight out of it.
#[test]
#[should_panic(expected = "tampered docs-fixture model cache")]
fn an_edited_model_envelope_is_refused() {
    let mut cached = CachedModel::from_model(&DocsModel::default());
    // The hand-edit: claim a language the builder never found.
    cached.body.available_languages.push("klingon".to_string());
    let _ = cached.into_model(Path::new("<in-memory>"));
}

#[test]
fn cache_key_is_deterministic_and_content_sensitive() {
    let (_tmp, root) = temp_root("key");
    fs::create_dir_all(root.join("slices")).unwrap();
    fs::write(root.join("slices/a.ttl"), b"v1").unwrap();
    let k1 = cache_key(&root);
    assert_eq!(
        k1,
        cache_key(&root),
        "key must be stable for identical inputs"
    );
    fs::write(root.join("slices/a.ttl"), b"v2").unwrap();
    assert_ne!(
        k1,
        cache_key(&root),
        "key must change when an input byte changes"
    );

    let input_key = cache_key(&root);
    fs::create_dir_all(root.join("crates/docs/src")).unwrap();
    write(
        &root,
        "crates/docs/src/lib.rs",
        "mod render; pub fn fixture() {}\n",
    );
    write(&root, "crates/docs/src/render.rs", "pub fn render() {}\n");
    assert_ne!(
        input_key,
        cache_key(&root),
        "key must change when fixture implementation bytes change"
    );
}

#[test]
fn derived_console_trees_do_not_join_the_fixture_key() {
    let (_tmp, root) = temp_root("derived-console");
    let assets = root.join("crates/docs/assets");
    fs::create_dir_all(assets.join("console/pkg")).unwrap();
    fs::create_dir_all(assets.join("console/smoke/node_modules/tool")).unwrap();
    fs::write(assets.join("console/pkg/gmeow.gts"), b"derived-v1").unwrap();
    fs::write(
        assets.join("console/smoke/node_modules/tool/index.js"),
        b"installed-v1",
    )
    .unwrap();
    let base = cache_key(&root);

    fs::write(assets.join("console/pkg/gmeow.gts"), b"derived-v2").unwrap();
    fs::write(
        assets.join("console/smoke/node_modules/tool/index.js"),
        b"installed-v2",
    )
    .unwrap();
    assert_eq!(
        base,
        cache_key(&root),
        "producer output and installed test dependencies are not fixture inputs"
    );

    fs::write(assets.join("gmeow.css"), b"authored asset").unwrap();
    write(
        &root,
        "crates/docs/src/lib.rs",
        "const CSS: &str = include_str!(\"../assets/gmeow.css\");\n",
    );
    assert_ne!(
        base,
        cache_key(&root),
        "an authored renderer asset must still invalidate the fixture"
    );
}

/// The repository root of THIS checkout (`crates/docs-model/` → up two).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/docs-model has a grandparent")
        .to_path_buf()
}

/// THE cache-correctness gate: a dependency crate's bytes are in the key, and a
/// NEWLY ADDED dependency edge joins the key with nothing to remember.
///
/// The hole this pins is not hypothetical: the key once hashed a hand-written
/// crate list, a crate split moved the documentation model into a crate absent
/// from that list, and every later edit to the moved code read back a stale
/// cached model. The closure is derived from the manifests now, so this test
/// fails for any list-based key regression.
#[test]
fn a_dependency_crates_sources_join_the_cache_key() {
    let (_tmp, root) = temp_root("dep-closure");
    let docs = root.join("crates/docs");
    fs::create_dir_all(docs.join("src")).unwrap();
    fs::create_dir_all(root.join("crates/alpha/src")).unwrap();
    fs::write(
        docs.join("Cargo.toml"),
        b"[dependencies]\ngmeow-alpha = { path = \"../alpha\" }\n",
    )
    .unwrap();
    write(
        &root,
        "crates/alpha/Cargo.toml",
        "[package]\nname='alpha'\nversion='1.0.0'\n",
    );
    fs::write(
        root.join("crates/alpha/src/lib.rs"),
        b"pub const VERSION: u8 = 1;",
    )
    .unwrap();

    let base = cache_key(&root);
    fs::write(
        root.join("crates/alpha/src/lib.rs"),
        b"pub const VERSION: u8 = 2;",
    )
    .unwrap();
    let edited = cache_key(&root);
    assert_ne!(
        base, edited,
        "editing a dependency crate's source must invalidate the fixture cache"
    );

    // A brand-new dependency edge — the exact change that silently rotted a
    // hand-written list — joins the key on the next run.
    fs::create_dir_all(root.join("crates/beta/src")).unwrap();
    write(
        &root,
        "crates/beta/Cargo.toml",
        "[package]\nname='beta'\nversion='1.0.0'\n",
    );
    fs::write(
        root.join("crates/beta/src/lib.rs"),
        b"pub const VERSION: u8 = 1;",
    )
    .unwrap();
    let unreferenced = cache_key(&root);
    assert_eq!(
        edited, unreferenced,
        "a crate nothing depends on is not compiled in and must not be hashed"
    );
    fs::write(
        docs.join("Cargo.toml"),
        b"[dependencies]\ngmeow-alpha = { path = \"../alpha\" }\n\
              gmeow-beta = { path = \"../beta\" }\n",
    )
    .unwrap();
    let with_beta = cache_key(&root);
    assert_ne!(
        unreferenced, with_beta,
        "declaring the dependency must pull the new crate into the key"
    );
    fs::write(
        root.join("crates/beta/src/lib.rs"),
        b"pub const VERSION: u8 = 2;",
    )
    .unwrap();
    let beta_edited = cache_key(&root);
    assert_ne!(
        with_beta, beta_edited,
        "the newly declared crate's later edits must invalidate the cache too"
    );

    // A dependency crate's selected production MANIFEST is hashed too: a
    // feature flip or a version bump there changes what compiles into the model
    // without touching a single `.rs` byte.
    fs::write(root.join("crates/beta/Cargo.toml"), b"[package]\n").unwrap();
    assert_ne!(
        beta_edited,
        cache_key(&root),
        "editing a dependency crate's Cargo.toml must invalidate the cache"
    );
}

/// A dev-dependency is linked into a crate's TESTS, never into the library that
/// builds the cached model / site / book, so its bytes must not be in the key —
/// otherwise every edit to the test-only `gmeow-mcp` query executor would throw
/// away the whole fixture.
#[test]
fn dev_dependencies_are_not_hashed() {
    let (_tmp, root) = temp_root("dev-deps");
    let docs = root.join("crates/docs");
    fs::create_dir_all(docs.join("src")).unwrap();
    fs::create_dir_all(root.join("crates/testonly/src")).unwrap();
    fs::write(
        docs.join("Cargo.toml"),
        b"[dev-dependencies]\ngmeow-testonly = { path = \"../testonly\" }\n",
    )
    .unwrap();
    fs::write(root.join("crates/testonly/src/lib.rs"), b"v1").unwrap();
    let base = cache_key(&root);
    fs::write(root.join("crates/testonly/src/lib.rs"), b"v2").unwrap();
    assert_eq!(
        base,
        cache_key(&root),
        "a dev-dependency must not be hashed"
    );
}

/// The derived closure over the LIVE manifests is genuinely transitively closed
/// and reaches the documentation model. A crate that declares a path dependency
/// the closure does not contain would be a crate whose edits are invisible to
/// the cache — the defect class this whole derivation exists to make impossible.
///
/// It also pins the direction of the split: the closure is rooted at the RENDERER
/// (`crates/docs`) and reaches THIS crate, so the renderer's bytes are folded into
/// the key its site/book caches hang off, and the model crate's bytes are folded
/// into the key its own cache hangs off.
#[test]
fn live_manifest_closure_is_closed_and_reaches_the_model() {
    let root = repo_root();
    let manifests =
        gmeow_build_inputs::declared_path_dependency_manifests(&root, "crates/docs/Cargo.toml")
            .unwrap();
    assert!(
        manifests.contains("crates/docs/Cargo.toml"),
        "renderer is the closure root: {manifests:?}"
    );
    assert!(
        manifests.contains("crates/docs-model/Cargo.toml"),
        "model is in the renderer closure: {manifests:?}"
    );
    for manifest in &manifests {
        let dependency_closure =
            gmeow_build_inputs::declared_path_dependency_manifests(&root, manifest).unwrap();
        assert!(
            dependency_closure.is_subset(&manifests),
            "{manifest} escapes the fixture dependency closure"
        );
    }
}

#[test]
fn manifest_path_deps_reads_every_non_dev_section() {
    let (_tmp, root) = temp_root("manifest-sections");
    write(
        &root,
        "crates/docs/Cargo.toml",
        r#"
[dependencies]
gmeow-a = { path = "../a" }
# gmeow-commented = { path = "../commented" }
pathological = "1"
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
gmeow-b = { path = "../b" }
[build-dependencies]
gmeow-c = { path = "../c" }
[dev-dependencies]
gmeow-d = { path = "../d" }
[target.'cfg(unix)'.dev-dependencies]
gmeow-e = { path = "../e" }
"#,
    );
    for name in ["a", "b", "c"] {
        write(&root, &format!("crates/{name}/Cargo.toml"), "[package]\n");
    }
    assert_eq!(
        gmeow_build_inputs::declared_path_dependency_manifests(&root, "crates/docs/Cargo.toml")
            .unwrap(),
        ["a", "b", "c", "docs"]
            .into_iter()
            .map(|name| format!("crates/{name}/Cargo.toml"))
            .collect(),
    );
}

#[test]
fn lexical_normalization_dedupes_equivalent_crate_paths() {
    let (_tmp, root) = temp_root("equivalent-paths");
    write(
        &root,
        "crates/docs/Cargo.toml",
        "[dependencies]\na={path='../docs-model'}\nb={path='.././docs/../docs-model'}\nn={path='.././ns'}\n",
    );
    for name in ["docs-model", "ns"] {
        write(&root, &format!("crates/{name}/Cargo.toml"), "[package]\n");
    }
    assert_eq!(
        gmeow_build_inputs::declared_path_dependency_manifests(&root, "crates/docs/Cargo.toml")
            .unwrap(),
        ["docs", "docs-model", "ns"]
            .into_iter()
            .map(|name| format!("crates/{name}/Cargo.toml"))
            .collect(),
    );
}

/// The competency-query resolution boundary the model enforces is exactly a set
/// of directories this key walks in full — otherwise a `gmeow:cqQueryFile` could
/// name a file whose text changes without moving the key.
#[test]
fn competency_query_roots_are_hashed() {
    for boundary in COMPETENCY_QUERY_ROOTS {
        let dir = boundary.trim_end_matches('/');
        let (_tmp, root) = temp_root(&format!("cq-{dir}"));
        fs::create_dir_all(root.join(dir)).unwrap();
        let before = cache_key(&root);
        fs::write(root.join(dir).join("q.rq"), b"SELECT * {}").unwrap();
        assert_ne!(
            before,
            cache_key(&root),
            "a competency query under {boundary} must be folded into the cache key"
        );
    }
}

#[test]
fn model_cache_path_is_the_shared_action_receipt() {
    let (_tmp, root) = temp_root("paths");
    let path = cache_path(&root);
    assert_eq!(path.parent().unwrap().file_name().unwrap(), "receipts");
    let name = path.file_name().unwrap().to_string_lossy();
    assert_eq!(name.len(), 69, "64 hex digits plus .json");
    assert!(name.ends_with(".json"));
    assert!(name[..64].bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[test]
fn present_but_corrupt_model_receipt_reaches_decode_and_is_refused() {
    let (_tmp, root) = temp_root("corrupt-model");
    let context = model_context(&root);
    let cached = CachedModel::from_model(&DocsModel::default());
    let store = action_store(&root);
    let receipt = store
        .publish(
            &context,
            cached.digest.clone(),
            model_payload(&context),
            &serde_json::to_vec(&cached).unwrap(),
        )
        .unwrap();
    let selected = SelectedAction::from_receipt(&receipt);
    assert!(load_selected_model(&root, &selected).is_ok());

    let path = store.receipt_path(&context.key());
    let corrupt = b"{ not valid json";
    fs::write(&path, corrupt).unwrap();
    let Err(error) = load_selected_model(&root, &selected) else {
        panic!("corrupt receipt must refuse");
    };
    assert!(
        error.to_string().starts_with("action cache JSON:"),
        "{error}"
    );
    assert_eq!(
        fs::read(path).unwrap(),
        corrupt,
        "read-only refusal cannot repair the receipt"
    );
}
