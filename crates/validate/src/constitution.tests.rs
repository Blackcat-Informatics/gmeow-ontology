// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// cargo-mutants (T9) surfaced surviving mutants in `literal_i64` /
// `literal_string` — the helpers had no direct coverage, so replacing their
// body with `None`/`Some(0)`/deleting the match arm went undetected. These
// tests pin both the literal path and the non-literal fallthrough, killing
// that mutant cluster.
/// Resolve the object term of the single triple `<s> <p> obj` in a tiny dataset,
/// where `obj` is the given Turtle object syntax.
fn object_term_ref(ds: &RdfDataset) -> TermRef<'_> {
    let q = ds
        .quads_for_pattern(None, None, None, GraphMatch::Any)
        .next()
        .expect("one triple");
    ds.resolve(q.o)
}

#[test]
fn literal_i64_parses_only_integer_literals() {
    let lit = store_from("<https://e/s> <https://e/p> \"42\" .");
    assert_eq!(literal_i64(object_term_ref(&lit)), Some(42));
    let neg = store_from("<https://e/s> <https://e/p> \"-7\" .");
    assert_eq!(literal_i64(object_term_ref(&neg)), Some(-7));
    let bad = store_from("<https://e/s> <https://e/p> \"notanint\" .");
    assert_eq!(literal_i64(object_term_ref(&bad)), None);
    let iri = store_from("<https://e/s> <https://e/p> <https://e/x> .");
    assert_eq!(literal_i64(object_term_ref(&iri)), None);
}

#[test]
fn literal_string_extracts_only_literal_lexical_values() {
    let lit = store_from("<https://e/s> <https://e/p> \"hello\" .");
    assert_eq!(
        literal_string(object_term_ref(&lit)),
        Some("hello".to_string())
    );
    let iri = store_from("<https://e/s> <https://e/p> <https://e/x> .");
    assert_eq!(literal_string(object_term_ref(&iri)), None);
}

fn store_from(ttl: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
}

const PREFIX: &str = "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n";

#[test]
fn unenforced_principle_is_an_error() {
    let store = store_from(&format!(
        "{PREFIX}meta:P1 a meta:Principle ; meta:number 1 ; meta:title \"Solo\" .\n"
    ));
    let msgs: Vec<String> = check_enforcement_coverage(&store)
        .into_iter()
        .map(|f| f.message)
        .collect();
    assert!(
        msgs.iter()
            .any(|m| m.contains("zero registered enforcement"))
    );
}

#[test]
fn practice_only_principle_warns_and_orphan_errors() {
    let store = store_from(&format!(
        "{PREFIX}\
             meta:P1 a meta:Principle ; meta:number 1 ; meta:title \"Honor\" ; meta:enforcedBy meta:rev .\n\
             meta:rev a meta:Practice .\n\
             meta:gate-orphan a meta:Gate .\n"
    ));
    let findings = check_enforcement_coverage(&store);
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("review practice"))
    );
    assert!(findings.iter().any(
        |f| f.code == "constitution.orphaned-enforcement" && f.message.contains("gate-orphan")
    ));
}

// ------------------------------------------------------------------
// Pure helper unit tests
// ------------------------------------------------------------------

#[test]
fn constitution_headings_extracts_numbered_sections() {
    let md = "# Preamble\n\n## 1. First\nbody\n## 2. Second thing\n";
    let got = constitution_headings(md);
    let mut expected = BTreeMap::new();
    expected.insert(1, "First".to_string());
    expected.insert(2, "Second thing".to_string());
    assert_eq!(got, expected);
}

#[test]
fn markdown_relations_read_marker_lines() {
    let md = "## 1. A\n\n**Superseded in part by Principle 2:** ok.\n\n## 2. B\nno marker.\n";
    let got = markdown_relations(md, "**Superseded in part by Principle");
    let mut expected: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    expected.insert(1, [2].into_iter().collect());
    assert_eq!(got, expected);
}

#[test]
fn makefile_targets_skips_pattern_rules() {
    let mk = "all:\n\t@echo ok\n\n%.o: %.c\n\tcc $< -o $@\n\ncheck:\n";
    let got = makefile_targets(mk);
    assert!(got.contains("all"));
    assert!(got.contains("check"));
    assert!(!got.contains("%.o"));
}

#[test]
fn python_top_level_names_finds_definitions_and_assignments() {
    let py = "class Foo:\n    pass\n\ndef bar():\n    pass\n\nasync def baz():\n    pass\n\nX: int = 1\nY = 2\n";
    let got = python_top_level_names(py);
    assert!(got.contains("Foo"));
    assert!(got.contains("bar"));
    assert!(got.contains("baz"));
    assert!(got.contains("X"));
    assert!(got.contains("Y"));
    assert!(!got.contains("pass"));
}

#[test]
fn python_top_level_names_ignores_nested_symbols() {
    let py = r#"
def outer():
    def inner():
        pass
    class NestedClass:
        pass
    nested_var = 1
class TopClass:
    def method(self):
        pass
top_level = 42
"#;
    let got = python_top_level_names(py);
    assert!(got.contains("outer"));
    assert!(got.contains("TopClass"));
    assert!(got.contains("top_level"));
    assert!(!got.contains("inner"), "nested def should not be collected");
    assert!(
        !got.contains("NestedClass"),
        "nested class should not be collected"
    );
    assert!(
        !got.contains("nested_var"),
        "nested assignment should not be collected"
    );
    assert!(
        !got.contains("method"),
        "method inside class should not be collected"
    );
}

#[test]
fn rust_item_names_finds_all_forms_and_excludes_noise() {
    let rust = r###"
/// Doc link to [`ghost_doc`] must not count as a definition.
pub fn real_fn() {}
const REAL_CONST: u32 = 1;
static mut REAL_STATIC: u32 = 2;
struct RealStruct;
enum RealEnum { A }
trait RealTrait { type RealAssoc; fn real_trait_method(&self); }
macro_rules! real_macro { () => {}; }
impl RealStruct {
    pub fn real_method(&self) {
        // a call site, not a definition:
        ghost_call();
        let _ghost_str = "ghost_string_name";
        let _c = '"'; // a quote inside a char literal must not open a string
    }
}
mod tests {
    #[test]
    fn real_nested_test() {}
}
"###;
    let got = rust_item_names(rust);
    for want in [
        "real_fn",
        "REAL_CONST",
        "REAL_STATIC",
        "RealStruct",
        "RealEnum",
        "RealTrait",
        "RealAssoc",
        "real_trait_method",
        "real_macro",
        "real_method",
        "real_nested_test",
    ] {
        assert!(got.contains(want), "missing item {want}: {got:?}");
    }
    for ghost in ["ghost_doc", "ghost_call", "ghost_string_name", "fn"] {
        assert!(!got.contains(ghost), "ghost {ghost} leaked: {got:?}");
    }
}

#[test]
fn strip_rust_comments_and_strings_blanks_noise_keeps_code() {
    let src = "fn a() {}\n// fn commented\nlet s = \"fn instr\";\n/* fn block */ fn b() {}\n";
    let stripped = strip_rust_comments_and_strings(src);
    // Real definitions survive; commented / in-string `fn NAME` are gone.
    let names = rust_item_names(src);
    assert!(names.contains("a") && names.contains("b"), "{names:?}");
    assert!(
        !names.contains("commented") && !names.contains("instr"),
        "{names:?}"
    );
    // Line structure preserved (newline count unchanged).
    assert_eq!(src.matches('\n').count(), stripped.matches('\n').count());
}

#[test]
fn variant_to_kebab_matches_clap_default_rename() {
    assert_eq!(variant_to_kebab("Version"), "version");
    assert_eq!(variant_to_kebab("SliceQuality"), "slice-quality");
    assert_eq!(
        variant_to_kebab("VerifyReleaseBundle"),
        "verify-release-bundle"
    );
    assert_eq!(variant_to_kebab("Mcp"), "mcp");
    assert_eq!(variant_to_kebab("BoxRoles"), "box-roles");
    assert_eq!(variant_to_kebab("I18n"), "i18n");
    assert_eq!(variant_to_kebab("ExportCsv"), "export-csv");
}

#[test]
fn cli_command_names_from_rust_reads_variants_and_forms() {
    let rust = "\
            #[derive(Debug, Subcommand)]\n\
            pub enum Commands {\n\
            \x20   /// bare variant.\n\
            \x20   Version,\n\
            \x20   /// tuple variant.\n\
            \x20   Info(InfoArgs),\n\
            \x20   /// struct variant with a field carrying its own attr.\n\
            \x20   Sync {\n\
            \x20       #[arg(long = \"mode\")]\n\
            \x20       mode: String,\n\
            \x20   },\n\
            }\n";
    let got = cli_command_names_from_rust(rust);
    assert!(got.contains("version"));
    assert!(got.contains("info"));
    assert!(got.contains("sync"));
    // A field attribute inside a variant body must not leak as a command.
    assert!(!got.contains("mode"));
}

#[test]
fn cli_command_names_from_rust_honors_command_name_override() {
    let rust = "\
            #[derive(Debug, Subcommand)]\n\
            pub enum Commands {\n\
            \x20   /// override wins over the kebab of the identifier.\n\
            \x20   #[command(name = \"sync-now\")]\n\
            \x20   SyncNow,\n\
            \x20   /// a non-name command attr must not become an override.\n\
            \x20   #[command(disable_help_flag = true)]\n\
            \x20   Gts {\n\
            \x20       #[arg(trailing_var_arg = true)]\n\
            \x20       args: Vec<String>,\n\
            \x20   },\n\
            }\n";
    let got = cli_command_names_from_rust(rust);
    assert!(got.contains("sync-now"));
    assert!(!got.contains("sync_now"));
    assert!(got.contains("gts"));
}

#[test]
fn cli_command_names_from_rust_includes_subapp_groups_and_nested() {
    let rust = "\
            #[derive(Debug, Subcommand)]\n\
            pub enum Commands {\n\
            \x20   /// group carrier variant.\n\
            \x20   Logic {\n\
            \x20       #[command(subcommand)]\n\
            \x20       command: LogicCommands,\n\
            \x20   },\n\
            }\n\
            #[derive(Debug, Subcommand)]\n\
            pub enum LogicCommands {\n\
            \x20   /// backward goal resolution.\n\
            \x20   Query,\n\
            \x20   /// compile pipeline.\n\
            \x20   Compile,\n\
            }\n";
    let got = cli_command_names_from_rust(rust);
    // Sub-app group name surfaces from the top-level carrier variant.
    assert!(got.contains("logic"));
    // Nested sub-app subcommands surface from the nested enum.
    assert!(got.contains("query"));
    assert!(got.contains("compile"));
}

#[test]
fn cli_surface_command_names_reads_both_rust_bins() {
    let tmp = tempfile::tempdir().unwrap();
    let public = tmp.path().join("crates/gmeow-cli/src");
    let dev = tmp.path().join("crates/gmeow-dev-cli/src");
    fs::create_dir_all(&public).unwrap();
    fs::create_dir_all(&dev).unwrap();
    fs::write(
            public.join("lib.rs"),
            "#[derive(Subcommand)]\npub enum Commands {\n    #[command(name = \"verify-release-bundle\")]\n    VerifyReleaseBundle,\n}\n",
        )
        .unwrap();
    fs::write(
            dev.join("lib.rs"),
            "#[derive(Subcommand)]\npub enum Commands {\n    #[command(name = \"release-bundle\")]\n    ReleaseBundle,\n}\n",
        )
        .unwrap();

    let got = cli_surface_command_names(tmp.path());
    assert!(got.contains("verify-release-bundle"));
    assert!(got.contains("release-bundle"));
}

// ------------------------------------------------------------------
// Integration tests over temp directories
// ------------------------------------------------------------------

fn write_pair(
    tmp: &tempfile::TempDir,
    manifest_ttl: &str,
    constitution_md: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let prefixes = "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";
    let manifest = tmp.path().join("constitution.ttl");
    fs::write(&manifest, format!("{prefixes}{manifest_ttl}")).unwrap();
    let constitution = tmp.path().join("CONSTITUTION.md");
    fs::write(&constitution, constitution_md).unwrap();
    (manifest, constitution)
}

#[test]
fn zero_enforcement_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" .\n",
        "## 1. Be good\n\nprose\n",
    );
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("zero registered enforcement"))
    );
}

#[test]
fn practice_only_principle_warns_not_errors() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:practice-x a meta:Practice ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:practice-x .\n",
        "## 1. Be good\n\nprose\n",
    );
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(
        findings
            .iter()
            .any(|f| f.severity == Severity::Warning
                && f.message.contains("only by review practice"))
    );
    assert!(
        !findings
            .iter()
            .any(|f| f.message.contains("zero registered enforcement"))
    );
}

#[test]
fn stale_artifact_reference_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-x a meta:Gate ; meta:artifact \"no/such/file.py\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n",
        "## 1. Be good\n\nprose\n",
    );
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("'no/such/file.py' does not exist"))
    );
}

#[test]
fn stale_symbol_make_target_and_cli_command_are_errors() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let py_dir = tmp.path().join("src/gmeow_tools");
    fs::create_dir_all(&py_dir).unwrap();
    fs::write(py_dir.join("validate.py"), "def real_function(): pass\n").unwrap();

    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-x a meta:Gate ;\n\
             meta:artifact \"src/gmeow_tools/validate.py\" ;\n\
             meta:symbol \"no_such_function\" ;\n\
             meta:makeTarget \"no-such-target\" ;\n\
             meta:cliCommand \"no-such-command\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n",
        "## 1. Be good\n\nprose\n",
    );
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
    assert!(text.contains("'no_such_function' not found"), "{text}");
    assert!(text.contains("Makefile target 'no-such-target'"), "{text}");
    assert!(text.contains("CLI command 'no-such-command'"), "{text}");
}

#[test]
fn stale_rust_symbol_only_in_comment_or_string_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let rs_dir = tmp.path().join("crates/x/src");
    fs::create_dir_all(&rs_dir).unwrap();
    // `ghost_symbol` appears only in a comment, a string, and a call site —
    // never as an item definition. `real_item` is a genuine impl method.
    fs::write(
        rs_dir.join("lib.rs"),
        "// ghost_symbol is only named here\n\
             pub struct S;\n\
             impl S { pub fn real_item(&self) { let _ = \"ghost_symbol\"; ghost_symbol(); } }\n",
    )
    .unwrap();

    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-ghost a meta:Gate ;\n\
             meta:artifact \"crates/x/src/lib.rs\" ;\n\
             meta:symbol \"ghost_symbol\" .\n\
             meta:gate-real a meta:Gate ;\n\
             meta:artifact \"crates/x/src/lib.rs\" ;\n\
             meta:symbol \"real_item\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-ghost, meta:gate-real .\n",
        "## 1. Be good\n\nprose\n",
    );
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
    // The comment/string/call-site-only symbol is now rejected …
    assert!(text.contains("'ghost_symbol' not found"), "{text}");
    // … while the genuine impl-method definition resolves (no finding).
    assert!(!text.contains("'real_item' not found"), "{text}");
}

/// Build a temp repo with one workspace crate `foo` containing `real_test`
/// (a `#[test]` calling `reached_fn`) and `lonely` (reached by nothing).
fn write_binding_repo(tmp: &tempfile::TempDir, makefile: &str, manifest_ttl: &str) {
    fs::write(tmp.path().join("Makefile"), makefile).unwrap();
    let foo = tmp.path().join("crates/foo/src");
    fs::create_dir_all(&foo).unwrap();
    fs::write(
        tmp.path().join("crates/foo/Cargo.toml"),
        "[package]\nname = \"foo\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    fs::write(
            foo.join("lib.rs"),
            "pub fn reached_fn() {}\n\
             pub fn lonely() {}\n\
             #[cfg(test)]\nmod t {\n    use super::*;\n    #[test]\n    fn real_test() { reached_fn(); }\n}\n",
        )
        .unwrap();
    let prefixes = "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";
    fs::write(
        tmp.path().join("constitution.ttl"),
        format!("{prefixes}{manifest_ttl}"),
    )
    .unwrap();
    fs::write(
        tmp.path().join("CONSTITUTION.md"),
        "## 1. Be good\n\nprose\n",
    )
    .unwrap();
}

/// Recognize the exact producer dispatch form without crediting build, inspection, or echoed text.
#[test]
fn producer_launcher_binds_only_the_executed_cli_operation() {
    for command in [
        "cargo xtask producer run -- validate",
        "env BUNDLE_DIGEST=abc cargo xtask producer run -- validate --strict",
    ] {
        let words = command
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(
            classify_command(&words),
            ExecKind::Subcommand("validate".into())
        );
    }
    for command in [
        "cargo xtask producer build",
        "cargo xtask producer recipe",
        "cargo xtask producer verify",
        "cargo xtask producer run --",
        "cargo xtask producer run -- --help",
        "cargo xtask other run -- validate",
        "echo cargo xtask producer run -- validate",
    ] {
        let words = command
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(classify_command(&words), ExecKind::Opaque, "{command}");
    }
}

#[test]
fn unbound_symbol_fires_when_target_cannot_run_it() {
    let tmp = tempfile::tempdir().unwrap();
    // `runit` runs the workspace test `real_test` (which reaches `reached_fn`
    // but NOT `lonely`). Gate `reach` cites a symbol on that call path;
    // gate `unreach` cites `lonely`, which nothing the target runs reaches.
    write_binding_repo(
        &tmp,
        "runit: ## workflow\n\tcargo nextest run -p foo\n",
        "meta:reach a meta:Gate ;\n\
             meta:artifact \"crates/foo/src/lib.rs\" ; meta:symbol \"reached_fn\" ; meta:makeTarget \"runit\" .\n\
             meta:unreach a meta:Gate ;\n\
             meta:artifact \"crates/foo/src/lib.rs\" ; meta:symbol \"lonely\" ; meta:makeTarget \"runit\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:reach, meta:unreach .\n",
    );
    let findings = constitution_full_report(
        &tmp.path().join("constitution.ttl"),
        &tmp.path().join("CONSTITUTION.md"),
        tmp.path(),
    );
    let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
    // `lonely` is cited but no cited target runs it → fires.
    assert!(
        text.contains("unreach: symbol 'lonely' has no static call path"),
        "{text}"
    );
    // `reached_fn` is on the test's call path → bound, no finding.
    assert!(!text.contains("reach: symbol 'reached_fn'"), "{text}");
}

#[test]
fn xtask_check_binds_tests_run_by_its_declared_make_targets() {
    let tmp = tempfile::tempdir().unwrap();
    write_binding_repo(
        &tmp,
        "check: ## workflow\n\tcargo xtask check\nrust-gate:\n\tcargo nextest run -p foo\n",
        "meta:reach a meta:Gate ;\n\
             meta:artifact \"crates/foo/src/lib.rs\" ; meta:symbol \"reached_fn\" ; meta:makeTarget \"check\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:reach .\n",
    );
    let xtask = tmp.path().join("crates/xtask/src");
    fs::create_dir_all(&xtask).unwrap();
    fs::write(
            xtask.join("main.rs"),
            "const CHECK_DAG: &[Task] = &[Task { name: \"tests\", target: \"rust-gate\", dependencies: &[] }];\n",
        )
        .unwrap();

    let findings = constitution_full_report(
        &tmp.path().join("constitution.ttl"),
        &tmp.path().join("CONSTITUTION.md"),
        tmp.path(),
    );
    let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
    assert!(!text.contains("unbound-symbol"), "{text}");
    assert!(!text.contains("reach: symbol 'reached_fn'"), "{text}");
}

#[test]
fn off_lane_target_fires_for_undocumented_unreachable_target() {
    let tmp = tempfile::tempdir().unwrap();
    // `orphan-target` is undocumented and reachable from no gate lane;
    // `runit` is a documented workflow, so it stays on-lane.
    write_binding_repo(
        &tmp,
        "orphan-target:\n\techo hi\nrunit: ## workflow\n\tcargo nextest run -p foo\n",
        "meta:dead a meta:Gate ; meta:makeTarget \"orphan-target\" .\n\
             meta:live a meta:Gate ; meta:makeTarget \"runit\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:dead, meta:live .\n",
    );
    let findings = constitution_full_report(
        &tmp.path().join("constitution.ttl"),
        &tmp.path().join("CONSTITUTION.md"),
        tmp.path(),
    );
    let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
    assert!(
        text.contains(
            "dead: Makefile target 'orphan-target' exists but is reachable from no gate lane"
        ),
        "{text}"
    );
    assert!(!text.contains("live: Makefile target 'runit'"), "{text}");
}

#[test]
fn orphaned_enforcement_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-used a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:gate-orphan a meta:Lint ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-used .\n",
        "## 1. Be good\n\nprose\n",
    );
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(findings.iter().any(|f| {
        f.message.contains("orphaned enforcement") && f.message.contains("gate-orphan")
    }));
}

#[test]
fn title_drift_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be excellent\" ;\n\
             meta:enforcedBy meta:gate-x .\n",
        "## 1. Be good\n\nprose\n",
    );
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(findings.iter().any(|f| f.message.contains("title drift")));
}

#[test]
fn undeclared_enforcement_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    let (manifest, constitution) = write_pair(
        &tmp,
        "meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:nonexistent-gate .\n",
        "## 1. Be good\n\nprose\n",
    );
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("undeclared enforcement"))
    );
}

#[test]
fn supersession_matching_pair_passes() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n\
             meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
             meta:enforcedBy meta:gate-x ; meta:supersededInPartBy meta:Principle1 .\n";
    let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\n**Superseded in part by Principle 1:** because reasons.\n";
    let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(
        !findings
            .iter()
            .any(|f| f.message.contains("supersededInPartBy drift"))
    );
}

#[test]
fn supersession_markdown_only_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n\
             meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
             meta:enforcedBy meta:gate-x .\n";
    let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\n**Superseded in part by Principle 1:** because reasons.\n";
    let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(findings.iter().any(|f| {
        f.message
            .contains("principle 2 meta:supersededInPartBy drift")
    }));
}

#[test]
fn supersession_ttl_only_is_an_error() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n\
             meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
             meta:enforcedBy meta:gate-x ; meta:supersededInPartBy meta:Principle1 .\n";
    let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\nno marker here.\n";
    let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(findings.iter().any(|f| {
        f.message
            .contains("principle 2 meta:supersededInPartBy drift")
    }));
}

#[test]
fn extends_matching_pair_passes() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
    let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n\
             meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
             meta:enforcedBy meta:gate-x ; meta:extends meta:Principle1 .\n";
    let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\n**Extends Principle 1.**\n";
    let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
    let findings = constitution_full_report(&manifest, &constitution, tmp.path());
    assert!(!findings.iter().any(|f| f.message.contains("extends drift")));
}

#[test]
fn real_repo_constitution_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let manifest = root.join("governance").join("constitution.ttl");
    let constitution = root.join("CONSTITUTION.md");
    let findings = constitution_full_report(&manifest, &constitution, root);
    let errors: Vec<_> = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.message.clone())
        .collect();
    assert!(errors.is_empty(), "{:#?}", errors);
}
