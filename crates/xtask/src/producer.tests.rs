// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use serde_json::json;

/// Create an admitted synthetic runtime unit with the selected workspace check policy.
fn runtime_unit(workspace: bool) -> CompilationUnit {
    CompilationUnit {
        package: if workspace {
            "path+file://<workspace>/crates/example#0.1.0"
        } else {
            "registry+https://example.invalid/index#upstream@1.0.0"
        }
        .into(),
        target: "example".into(),
        target_kinds: vec!["lib".into()],
        mode: "build".into(),
        platform: None,
        features: vec![],
        profile: json!({
            "name": "pipeline", "opt_level": "3", "lto": "fat",
            "codegen_units": 1, "debuginfo": 0, "incremental": false,
            "debug_assertions": workspace, "overflow_checks": workspace,
            "strip": {"resolved": {"Named": "symbols"}}
        }),
        dependencies: vec![],
    }
}

/// Supply a tiny recipe for publication tests without resolving tools or repository inputs.
fn publication_recipe(root: &Path) -> ExecutableRecipe {
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='1.0.0'\n",
    )
    .unwrap();
    std::fs::write(root.join("Cargo.lock"), "version=4\npackage=[]\n").unwrap();
    std::fs::write(root.join("lib.rs"), "pub fn fixture() {}\n").unwrap();
    let source_inventory = InputInventory::collect(
        root,
        &ProductionSelection {
            schema: gmeow_build_inputs::SCHEMA,
            units: vec![UnitSelection {
                package: "path+file://<workspace>#fixture@1".into(),
                manifest: Some("Cargo.toml".into()),
                source: Some("lib.rs".into()),
                target: "fixture".into(),
                kinds: vec!["lib".into()],
                cfg: CfgContext::from_rustc("unix", &[], true).unwrap(),
                dependencies: vec![],
                dependency_names: vec![],
                controller: false,
            }],
            roots: vec![0],
            policy_files: vec![],
        },
    )
    .unwrap();
    ExecutableRecipe {
        schema: 2,
        profile: "pipeline".into(),
        source_digest: source_inventory.digest().unwrap(),
        source_inventory,
        rustc: "compiler".into(),
        cargo: "cargo".into(),
        compiler_environment: BTreeMap::new(),
        units: Vec::new(),
        roots: Vec::new(),
    }
}

fn publication_resolution(root: &Path, recipe: &ExecutableRecipe) -> CargoResolutionEvidence {
    CargoResolutionInputs::capture(root, ["Cargo.toml".into()].into_iter().collect())
        .unwrap()
        .bind(&recipe.source_inventory.selection)
        .unwrap()
}

fn change_source(recipe: &mut ExecutableRecipe) {
    recipe
        .source_inventory
        .files
        .get_mut("lib.rs")
        .unwrap()
        .sha256 = "1".repeat(64);
    recipe.source_digest = recipe.source_inventory.digest().unwrap();
}

/// Treat an absent receipt as a build miss; reject substituted or missing receipted bytes.
#[test]
fn interrupted_publication_is_a_miss_but_present_receipts_must_authenticate() {
    let scratch = tempfile::tempdir().expect("scratch");
    let binary = scratch.path().join("gmeow-dev");
    let path = binary.with_extension("receipt.json");
    let recipe = publication_recipe(scratch.path());
    assert!(!staged_is_fresh(&binary, &path, &recipe).expect("empty build miss"));
    std::fs::write(&binary, b"linked executable").expect("binary");
    assert!(!staged_is_fresh(&binary, &path, &recipe).expect("unreceipted build miss"));
    ExecutableReceipt {
        schema: 2,
        resolution: publication_resolution(scratch.path(), &recipe),
        recipe: recipe.clone(),
        executable_sha256: sha256_file(&binary).expect("digest"),
    }
    .write(&path)
    .expect("receipt");
    assert!(staged_is_fresh(&binary, &path, &recipe).expect("authenticated hit"));
    let mut changed = recipe.clone();
    change_source(&mut changed);
    assert!(!staged_is_fresh(&binary, &path, &changed).expect("stale build miss"));
    assert!(verify(scratch.path(), &binary, &path, &changed).is_err());
    std::fs::write(&binary, b"substituted bytes").expect("replace");
    assert!(staged_is_fresh(&binary, &path, &recipe).is_err());
    std::fs::remove_file(&binary).expect("remove binary");
    assert!(staged_is_fresh(&binary, &path, &recipe).is_err());
    std::fs::write(&binary, b"linked executable").expect("restore binary");
    std::fs::write(&path, b"malformed receipt").expect("corrupt receipt");
    assert!(staged_is_fresh(&binary, &path, &recipe).is_err());
}

#[test]
fn obsolete_receipts_require_a_new_build_but_never_pass_readonly_admission() {
    let scratch = tempfile::tempdir().unwrap();
    let binary = scratch.path().join("gmeow-dev");
    let path = binary.with_extension("receipt.json");
    let recipe = publication_recipe(scratch.path());
    std::fs::write(&binary, b"obsolete executable").unwrap();
    std::fs::write(&path, br#"{"schema":1,"recipe":{"schema":1}}"#).unwrap();
    assert!(!staged_is_fresh(&binary, &path, &recipe).unwrap());
    assert!(verify(scratch.path(), &binary, &path, &recipe).is_err());
    std::fs::write(&path, br#"{"schema":2,"recipe":{"schema":2}}"#).unwrap();
    assert!(staged_is_fresh(&binary, &path, &recipe).is_err());
}

#[test]
fn development_readmission_refreshes_only_evidence_and_preserves_binary_and_recipe() {
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path();
    let recipe = publication_recipe(root);
    let binary = root.join("gmeow-dev");
    let path = binary.with_extension("receipt.json");
    std::fs::write(&binary, b"unchanged optimized executable").unwrap();
    let original = ExecutableReceipt {
        schema: 2,
        resolution: publication_resolution(root, &recipe),
        recipe: recipe.clone(),
        executable_sha256: sha256_file(&binary).unwrap(),
    };
    original.write(&path).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='1.0.0'\n[dev-dependencies]\nhelper='1'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        "version=4\n[[package]]\nname='helper'\nversion='1.0.0'\n",
    )
    .unwrap();
    assert!(verify(root, &binary, &path, &recipe).is_err());
    assert_eq!(
        ExecutableReceipt::read(&path).unwrap(),
        original,
        "read-only admission never refreshes evidence"
    );
    let mut current = recipe.clone();
    current.source_inventory =
        InputInventory::collect(root, &recipe.source_inventory.selection).unwrap();
    current.source_digest = current.source_inventory.digest().unwrap();
    assert_eq!(recipe, current);
    let admitted = AdmittedProducer {
        resolution: publication_resolution(root, &current),
        recipe: current,
    };
    assert!(
        staged_is_fresh(&binary, &path, &admitted.recipe).unwrap(),
        "explicit fresh resolution needs no O3 rebuild"
    );
    refresh_resolution(root, &binary, &path, &admitted).unwrap();
    verify(root, &binary, &path, &admitted.recipe).unwrap();
    let refreshed = ExecutableReceipt::read(&path).unwrap();
    assert_ne!(original.resolution, refreshed.resolution);
    assert_eq!(original.recipe, refreshed.recipe);
    assert_eq!(original.executable_sha256, refreshed.executable_sha256);
    assert_eq!(
        std::fs::read(&binary).unwrap(),
        b"unchanged optimized executable"
    );
    let mut different = admitted;
    change_source(&mut different.recipe);
    assert!(refresh_resolution(root, &binary, &path, &different).is_err());
    assert_eq!(ExecutableReceipt::read(&path).unwrap(), refreshed);
}

/// Recover an interrupted replacement of an authenticated pair through an unreceipted miss.
#[test]
fn interrupted_replacement_retires_old_receipt_before_publishing_new_bytes() {
    let scratch = tempfile::tempdir().expect("scratch");
    let binary = scratch.path().join("gmeow-dev");
    let path = binary.with_extension("receipt.json");
    let temporary = binary.with_extension("prepared");
    let old_recipe = publication_recipe(scratch.path());
    std::fs::write(&binary, b"old executable").expect("old binary");
    ExecutableReceipt {
        schema: 2,
        resolution: publication_resolution(scratch.path(), &old_recipe),
        recipe: old_recipe.clone(),
        executable_sha256: sha256_file(&binary).expect("old digest"),
    }
    .write(&path)
    .expect("old receipt");
    assert!(staged_is_fresh(&binary, &path, &old_recipe).expect("old authenticated pair"));

    let mut new_recipe = old_recipe;
    std::fs::write(scratch.path().join("lib.rs"), "pub fn changed() {}\n").unwrap();
    new_recipe.source_inventory =
        InputInventory::collect(scratch.path(), &new_recipe.source_inventory.selection).unwrap();
    new_recipe.source_digest = new_recipe.source_inventory.digest().unwrap();
    std::fs::write(&temporary, b"new executable").expect("prepared replacement");
    replace_staged_executable(&temporary, &binary, &path).expect("replace executable");
    // Stop at the actual production boundary before the new receipt is published.
    assert_eq!(
        std::fs::read(&binary).expect("new bytes"),
        b"new executable"
    );
    assert!(!path.exists(), "old evidence must not survive replacement");
    assert!(!staged_is_fresh(&binary, &path, &new_recipe).expect("recoverable build miss"));
    assert!(verify(scratch.path(), &binary, &path, &new_recipe).is_err());

    std::fs::write(&temporary, b"new executable").expect("prepared retry");
    replace_staged_executable(&temporary, &binary, &path).expect("retry without a receipt");
    ExecutableReceipt {
        schema: 2,
        resolution: publication_resolution(scratch.path(), &new_recipe),
        recipe: new_recipe.clone(),
        executable_sha256: sha256_file(&binary).expect("new digest"),
    }
    .write(&path)
    .expect("new receipt");
    verify(scratch.path(), &binary, &path, &new_recipe).expect("recovered authenticated pair");
    assert!(staged_is_fresh(&binary, &path, &new_recipe).expect("recovered fresh hit"));
}

/// Reject weakened dependency optimization and disabled workspace runtime checks.
#[test]
fn runtime_dependency_policy_is_checked_transitively() {
    let mut root = runtime_unit(true);
    root.target_kinds = vec!["bin".into()];
    root.dependencies = vec![1];
    let dependency = runtime_unit(false);
    validate_units(&[root.clone(), dependency.clone()], &[0]).expect("admitted");
    for (key, weakened) in [
        ("opt_level", json!("2")),
        ("lto", json!("thin")),
        ("codegen_units", json!(16)),
        ("debuginfo", json!(1)),
        ("incremental", json!(true)),
    ] {
        let mut changed = dependency.clone();
        changed.profile[key] = weakened;
        assert!(
            validate_units(&[root.clone(), changed], &[0]).is_err(),
            "{key}"
        );
    }
    for key in ["debug_assertions", "overflow_checks"] {
        let mut changed = root.clone();
        changed.profile[key] = json!(false);
        assert!(
            validate_units(&[changed, dependency.clone()], &[0]).is_err(),
            "{key}"
        );
    }
}

/// Permit a host macro's distinct build profile without admitting the same profile for runtime code.
#[test]
fn host_build_tools_are_separate_from_runtime_code() {
    let mut root = runtime_unit(true);
    root.dependencies = vec![1];
    let mut build_tool = runtime_unit(false);
    build_tool.target_kinds = vec!["proc-macro".into()];
    build_tool.profile["opt_level"] = json!("0");
    validate_units(&[root.clone(), build_tool.clone()], &[0]).expect("host tool");
    build_tool.target_kinds = vec!["lib".into()];
    assert!(validate_units(&[root, build_tool], &[0]).is_err());
}

fn compilation_unit(name: &str, features: &[&str], dependencies: Vec<usize>) -> CompilationUnit {
    let mut unit = runtime_unit(false);
    unit.package = format!("registry+https://example.invalid/index#{name}@1.0.0");
    unit.target = name.to_owned();
    unit.features = features
        .iter()
        .map(|feature| (*feature).to_owned())
        .collect();
    unit.dependencies = dependencies;
    unit
}

fn source_unit(name: &str, features: &[&str], dependencies: Vec<usize>) -> UnitSelection {
    let features: Vec<_> = features
        .iter()
        .map(|feature| (*feature).to_owned())
        .collect();
    UnitSelection {
        package: format!("registry+https://example.invalid/index#{name}@1.0.0"),
        manifest: None,
        source: None,
        target: name.to_owned(),
        kinds: vec!["lib".into()],
        cfg: CfgContext::from_rustc("unix", &features, false).unwrap(),
        dependency_names: dependencies
            .iter()
            .map(|_| "dependency".to_owned())
            .collect(),
        dependencies,
        controller: true,
    }
}

/// Cargo may exchange two identically-presented host units while retaining their
/// different recursive feature lineages. Raw indices must never move a recipe.
#[test]
fn cargo_unit_graph_identity_is_independent_of_interchangeable_indices() {
    let mut compilation_a = vec![
        compilation_unit("proc-macro2", &["default", "span-locations"], vec![]),
        compilation_unit("proc-macro2", &["proc-macro"], vec![]),
        compilation_unit("quote", &["proc-macro"], vec![0]),
        compilation_unit("quote", &["proc-macro"], vec![1]),
    ];
    let mut compilation_b = compilation_a.clone();
    compilation_b[2].dependencies = vec![1];
    compilation_b[3].dependencies = vec![0];
    let mut compilation_roots_a = vec![2, 3];
    let mut compilation_roots_b = vec![2, 3];
    canonicalize_compilation_graph(&mut compilation_a, &mut compilation_roots_a).unwrap();
    canonicalize_compilation_graph(&mut compilation_b, &mut compilation_roots_b).unwrap();
    assert_eq!(compilation_a, compilation_b);
    assert_eq!(compilation_roots_a, compilation_roots_b);

    let mut source_a = vec![
        source_unit("proc-macro2", &["default", "span-locations"], vec![]),
        source_unit("proc-macro2", &["proc-macro"], vec![]),
        source_unit("quote", &["proc-macro"], vec![0]),
        source_unit("quote", &["proc-macro"], vec![1]),
    ];
    let mut source_b = source_a.clone();
    source_b[2].dependencies = vec![1];
    source_b[3].dependencies = vec![0];
    let mut source_roots_a = vec![2, 3];
    let mut source_roots_b = vec![2, 3];
    canonicalize_source_graph(&mut source_a, &mut source_roots_a).unwrap();
    canonicalize_source_graph(&mut source_b, &mut source_roots_b).unwrap();
    assert_eq!(source_a, source_b);
    assert_eq!(source_roots_a, source_roots_b);
}

/// A genuine recursive feature-lineage change remains visible after canonicalization.
#[test]
fn canonical_unit_graph_retains_semantic_dependency_changes() {
    let mut left = vec![
        compilation_unit("dependency", &["first"], vec![]),
        compilation_unit("root", &[], vec![0]),
    ];
    let mut right = left.clone();
    right[0].features = vec!["second".into()];
    let mut left_roots = vec![1];
    let mut right_roots = vec![1];
    canonicalize_compilation_graph(&mut left, &mut left_roots).unwrap();
    canonicalize_compilation_graph(&mut right, &mut right_roots).unwrap();
    assert_ne!(left, right);
}

/// Reject alternate codegen flag spellings while retaining unrelated warning and linker settings.
#[test]
fn rustflags_cannot_override_admitted_codegen_policy() {
    for flags in [
        vec!["-C", "opt-level=2"],
        vec!["-Clto=thin"],
        vec!["--codegen=codegen-units=16"],
        vec!["--codegen", "debuginfo=2"],
        vec!["-Zcodegen-backend=cranelift"],
        vec!["-g"],
        vec!["-Ctarget-feature=-avx2"],
    ] {
        assert!(validate_flags(&flags.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err());
    }
    validate_flags(&["-Dwarnings".into(), "-Clink-self-contained=+linker".into()])
        .expect("non-competing flags");
}

/// Resolve isolated Cargo configuration without modifying the process environment.
fn fixture_flags(contents: &str, env: &[(&str, &str)]) -> Result<Vec<String>> {
    let scratch = tempfile::tempdir().expect("configuration directory");
    let cargo_dir = scratch.path().join(".cargo");
    std::fs::create_dir(&cargo_dir).expect("Cargo directory");
    std::fs::write(cargo_dir.join("config.toml"), contents).expect("configuration");
    let config = cargo_config2::Config::load_with_options(
        scratch.path(),
        cargo_config2::ResolveOptions::default()
            .cargo_home(None)
            .host_triple("x86_64-unknown-linux-gnu")
            .env(env.iter().copied()),
    )
    .map_err(fail)?;
    admitted_config_flags(&config)
}

/// Default configurations remain buildable, while inherited policy cannot weaken O3/LTO.
#[test]
fn producer_admits_absent_target_flags_and_checks_build_defaults() {
    assert!(
        fixture_flags("", &[])
            .expect("compiler defaults")
            .is_empty()
    );
    assert_eq!(
        fixture_flags("[build]\nrustflags = ['-Dwarnings']\n", &[])
            .expect("inherited warning policy"),
        ["-Dwarnings"]
    );
    for source in [
        "[build]\nrustflags = ['-Copt-level=2']\n",
        "[target.x86_64-unknown-linux-gnu]\nrustflags = ['-Clto=thin']\n",
        "[target.'cfg(unix)']\nrustflags = ['-Ccodegen-units=16']\n",
    ] {
        assert!(fixture_flags(source, &[]).is_err(), "{source}");
    }
    assert!(fixture_flags("[build]\nrustflags = [12]\n", &[]).is_err());
}

/// Admission applies to the effective configuration, including the CI environment.
#[test]
fn producer_admits_effective_flags_instead_of_overridden_defaults() {
    let source = "[build]\nrustflags = ['-Copt-level=2']\n";
    for env in [
        vec![("CARGO_ENCODED_RUSTFLAGS", "-Dwarnings")],
        vec![("RUSTFLAGS", "-Dwarnings")],
        vec![(
            "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS",
            "-Dwarnings",
        )],
    ] {
        assert_eq!(
            fixture_flags(source, &env).expect("selected flags"),
            ["-Dwarnings"]
        );
    }
    assert!(fixture_flags("", &[("CARGO_BUILD_RUSTFLAGS", "-Clto=thin")]).is_err());
    let error = fixture_flags(
        "[build]\nrustflags = [\"-Dwarnings\", \"\\u001f-Clto=thin\"]",
        &[],
    )
    .expect_err("encoded separator must be rejected before invoking Cargo");
    assert!(error.to_string().contains("encoded argument separator"));
}

/// Track runtime sources and added embedded queries while excluding workflow-only edits.
#[test]
fn producer_inventory_tracks_embedded_queries_and_runtime_dependencies() {
    // Synthetic source tree only: inventory discovery must never invoke a
    // compiler, pipeline stage, or corpus fixture producer.
    let scratch = tempfile::tempdir().expect("source tree");
    let root = scratch.path();
    for (path, contents) in [
        ("Cargo.toml", "[workspace]"),
        ("Cargo.lock", "version=4\npackage=[]\n"),
        (
            "crates/gmeow-dev-cli/Cargo.toml",
            "[dependencies]\nruntime = { path = \"../runtime\" }\n[dev-dependencies]\ntest-only = { path = \"../test-only\" }",
        ),
        ("crates/gmeow-dev-cli/src/main.rs", "fn main() {}"),
        ("crates/runtime/Cargo.toml", "[package]\nname = \"runtime\""),
        ("crates/runtime/src/lib.rs", "pub fn runtime() {}"),
        ("crates/logic/Cargo.toml", "[package]\nname='logic'\n"),
        ("crates/logic/build.rs", "fn main() {}"),
        (
            "crates/logic/src/lib.rs",
            "include!(concat!(env!(\"OUT_DIR\"),\"/verify_queries.rs\"));",
        ),
        ("crates/runtime/src/bin/tool.rs", "fn main() {}"),
        ("queries/verify/root.rq", "ASK {}"),
        ("slices/group/one/queries/verify/first.rq", "ASK {}"),
    ] {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(path, contents).expect("source");
    }
    let units = [
        ("gmeow-dev-cli", "main.rs", vec![1, 2]),
        ("runtime", "lib.rs", vec![]),
        ("logic", "lib.rs", vec![]),
    ]
    .into_iter()
    .map(|(name, source, dependencies)| UnitSelection {
        package: format!("path+file://<workspace>/crates/{name}#{name}@1"),
        manifest: Some(format!("crates/{name}/Cargo.toml")),
        source: Some(format!("crates/{name}/src/{source}")),
        target: name.into(),
        kinds: vec!["lib".into()],
        cfg: CfgContext::from_rustc("unix", &[], true).unwrap(),
        dependencies,
        dependency_names: vec![],
        controller: false,
    })
    .collect();
    let selection = ProductionSelection {
        schema: gmeow_build_inputs::SCHEMA,
        units,
        roots: vec![0],
        policy_files: vec![],
    };
    let initial = InputInventory::collect(root, &selection).unwrap();
    assert!(initial.files.contains_key("crates/runtime/src/lib.rs"));
    assert!(!initial.files.contains_key("crates/runtime/src/bin/tool.rs"));
    let before = initial.digest().unwrap();
    std::fs::write(root.join("Makefile"), "producer-build:\n").expect("workflow edit");
    assert_eq!(
        before,
        InputInventory::collect(root, &selection)
            .unwrap()
            .digest()
            .unwrap()
    );
    std::fs::write(
        root.join("slices/group/one/queries/verify/second.rq"),
        "ASK { ?s ?p ?o }",
    )
    .expect("new embedded query");
    assert_ne!(
        before,
        InputInventory::collect(root, &selection)
            .unwrap()
            .digest()
            .unwrap()
    );
}
