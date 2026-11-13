// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn inputs(native: bool, admitted: &[&str]) -> RecipeInputs {
    RecipeInputs {
        admitted: admitted.iter().map(|flag| (*flag).to_owned()).collect(),
        root: PathBuf::from("/work/gmeow"),
        cargo_home: PathBuf::from("/home/dev/.cargo"),
        target_dir: PathBuf::from("/opt/target/gmeow"),
        build_dir: Some(PathBuf::from("/opt/build/1234")),
        native,
        revision: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        tree: Tree::Clean,
        bundle: PathBuf::from("/work/gmeow/generated/dist/gmeow.gts"),
        codebook: PathBuf::from("/work/gmeow/generated/projections/lang/gmn-codebook.cbor"),
    }
}

/// Protects the portable distribution contract: whatever CPU a host configuration
/// or wrapper injects (split or joined spelling) is removed, exactly one
/// `x86-64-v3` selection reaches rustc, every build root is remapped (most specific
/// last), and the Cargo command carries precisely that recipe with `RUSTFLAGS`
/// removed and the matching portable C flags forced.
#[test]
fn portable_recipe_forces_the_baseline_and_remaps_every_build_root() {
    let recipe = recipe(&inputs(
        false,
        &[
            "-D",
            "warnings",
            "-C",
            "target-cpu=native",
            "-Ctarget-cpu=znver5",
            "--codegen=target-cpu=haswell",
            "-Clink-self-contained=+linker",
        ],
    ));
    assert_eq!(
        recipe.rustflags,
        [
            "-D",
            "warnings",
            "-Clink-self-contained=+linker",
            "-Ctarget-cpu=x86-64-v3",
            "-Cstrip=symbols",
            "-Dwarnings",
            "--remap-path-prefix=/work/gmeow=/gmeow",
            "--remap-path-prefix=/home/dev/.cargo=/cargo",
            "--remap-path-prefix=/opt/target/gmeow=/target",
            "--remap-path-prefix=/opt/build/1234=/build",
        ]
    );
    assert_eq!(recipe.c_flags, "-O2 -march=x86-64-v3 -mtune=generic");
    assert_eq!(recipe.cpu_baseline, "x86-64-v3");

    let command = cargo_command(OsStr::new("cargo"), Path::new("/work/gmeow"), &recipe);
    let envs: Vec<(String, Option<String>)> = command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect();
    let encoded = recipe.rustflags.join("\u{1f}");
    let mut expected_envs = vec![
        ("CARGO_ENCODED_RUSTFLAGS", Some(encoded.as_str())),
        ("RUSTFLAGS", None),
        (
            "GMEOW_SOURCE_REVISION",
            Some("0123456789abcdef0123456789abcdef01234567"),
        ),
        ("GMEOW_SOURCE_TREE", Some("clean")),
        ("GMEOW_BUILD_CPU_BASELINE", Some("x86-64-v3")),
        (
            "GMEOW_BUNDLE_PATH",
            Some("/work/gmeow/generated/dist/gmeow.gts"),
        ),
        (
            "GMEOW_CODEBOOK_PATH",
            Some("/work/gmeow/generated/projections/lang/gmn-codebook.cbor"),
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.map(str::to_owned)))
    .collect::<Vec<_>>();
    let mut envs = envs;
    envs.sort();
    expected_envs.sort();
    assert_eq!(envs, expected_envs);
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        [
            "--config",
            "env.CFLAGS.value=\"-O2 -march=x86-64-v3 -mtune=generic\"",
            "--config",
            "env.CXXFLAGS.value=\"-O2 -march=x86-64-v3 -mtune=generic\"",
            "--config",
            "env.CFLAGS.force=true",
            "--config",
            "env.CXXFLAGS.force=true",
            "--config",
            "profile.release.strip=\"symbols\"",
            "build",
            "--locked",
            "--release",
            "-p",
            "gmeow-cli",
            "--bin",
            "gmeow",
            "--message-format=json-render-diagnostics",
        ]
    );
}

/// Protects the opt-in host tuning: only `--native` yields a native CPU and native
/// C flags, and it is recorded as such for `build.rs` and the receipt.
#[test]
fn only_an_explicit_native_build_is_host_tuned() {
    let recipe = recipe(&inputs(true, &["-Ctarget-cpu=x86-64-v3"]));
    assert_eq!(recipe.cpu_baseline, "native");
    assert_eq!(recipe.c_flags, "-O2 -march=native -mtune=native");
    assert_eq!(
        recipe
            .rustflags
            .iter()
            .filter(|flag| names_target_cpu(flag))
            .collect::<Vec<_>>(),
        ["-Ctarget-cpu=native"]
    );
}

/// Protects the install gate: a dirty or host-tuned staged build never reaches a
/// PATH directory unless the matching consent flag is given, and a clean portable
/// build needs none.
#[test]
fn install_refuses_dirty_or_native_builds_without_consent() {
    let receipt =
        |tree: &str, cpu: &str| json!({ "source": { "tree": tree }, "cpu_baseline": cpu });
    assert!(admit_install(&receipt("clean", "x86-64-v3"), false, false).is_ok());

    let dirty = receipt("dirty", "x86-64-v3");
    let refused = admit_install(&dirty, false, false).expect_err("dirty refused");
    assert!(refused.to_string().contains("--allow-dirty"), "{refused}");
    assert!(admit_install(&dirty, true, false).is_ok());

    let native = receipt("clean", "native");
    let refused = admit_install(&native, false, false).expect_err("native refused");
    assert!(refused.to_string().contains("--allow-native"), "{refused}");
    assert!(admit_install(&native, false, true).is_ok());
    assert!(admit_install(&native, true, false).is_err());
}

/// Protects the command surface: unknown or repeated flags are refused rather than
/// ignored, and `run` requires the separator before the forwarded arguments.
#[test]
fn malformed_cli_invocations_are_refused() {
    let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    assert!(parse(&args(&["build", "--native", "--native"])).is_err());
    assert!(parse(&args(&["build", "--portable"])).is_err());
    assert!(parse(&args(&["run", "version"])).is_err());
    assert!(parse(&args(&["install", "--bin-dir"])).is_err());
    assert_eq!(
        parse(&args(&["run", "--", "version", "--verbose"])).expect("run"),
        Operation::Run(args(&["version", "--verbose"]))
    );
}
