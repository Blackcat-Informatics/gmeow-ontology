#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

# Exercise source discovery and scanner failures using tiny inert source files.
# Nothing compiles or executes the producer calls represented in these fixtures.
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
mkdir -p "$repo_root/.cache"
fixture_root=$(mktemp -d "$repo_root/.cache/corpus-scanner.XXXXXX")
trap 'rm -rf -- "$fixture_root"' EXIT
fixture="$fixture_root/repo"
mkdir -p "$fixture/crates/ignored/tests" "$fixture/scripts"
git init -q "$fixture"
cp "$repo_root/scripts/scan-cfg-test-producers.pl" "$fixture/scripts/"
cp "$repo_root/scripts/scan-doctest-producers.pl" "$fixture/scripts/"
cp "$repo_root/scripts/scan-authored-test-flows.pl" "$fixture/scripts/"
printf 'crates/ignored/\n' > "$fixture/.gitignore"
printf 'fn main() {}\n' > "$fixture/crates/build.rs"
cat > "$fixture/Makefile" <<'MAKE'
nextest: verify-test-fixtures
nextest-archive: verify-test-fixtures
maint-rust-heavy: verify-test-fixtures
MAKE

scan() {
    (cd "$fixture" && bash "$repo_root/scripts/lint-test-corpus-producers.sh")
}

reject() {
    local expected=$1
    if scan > "$fixture_root/result.log" 2>&1; then
        printf 'ERROR: corpus scanner accepted %s\n' "$expected" >&2
        exit 1
    fi
    if ! grep -Fq -- "$expected" "$fixture_root/result.log"; then
        cat "$fixture_root/result.log" >&2
        printf 'ERROR: corpus scanner failed without expected evidence: %s\n' "$expected" >&2
        exit 1
    fi
}

scan > "$fixture_root/result.log" 2>&1

# Producer names are complete identifiers, not suffixes in descriptive test names.
cat > "$fixture/crates/ignored/tests/identifier_boundaries.rs" <<'RUST'
#[test]
fn pending_source_preparation_is_not_certified_by_cached_observation() {}
RUST
scan > "$fixture_root/result.log" 2>&1
cat >> "$fixture/crates/ignored/tests/identifier_boundaries.rs" <<'RUST'
#[test]
fn calls_a_qualified_producer() { execution::cached_observation(); }
RUST
reject 'crates/ignored/tests/identifier_boundaries.rs'
rm "$fixture/crates/ignored/tests/identifier_boundaries.rs"

cat > "$fixture/crates/ignored/tests/attempt.rs" <<'RUST'
#[test]
fn attempt() { run_full(); }
RUST
reject 'crates/ignored/tests/attempt.rs'
rm "$fixture/crates/ignored/tests/attempt.rs"

for producer in observe_disjoint_clash observe_relcomp observe_characteristics; do
    cat > "$fixture/crates/ignored/tests/attempt.rs" <<RUST
#[test]
fn attempt() {
    // gmeow-test-input: synthetic-only
    gmeow_logic::coherence_observations::$producer(dataset);
}
RUST
    reject 'crates/ignored/tests/attempt.rs'
done
rm "$fixture/crates/ignored/tests/attempt.rs"

# Admission callbacks are producer execution even when the artifact usually hits.
cat > "$fixture/crates/ignored/tests/attempt.rs" <<'RUST'
#[test]
fn attempt() { admit_authenticated_corpus_artifact(root, receipt, "artifact", || produce()); }
RUST
reject 'crates/ignored/tests/attempt.rs'
rm "$fixture/crates/ignored/tests/attempt.rs"

cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
#[cfg(test)]
mod tests {
    // gmeow-test-input: synthetic-only
    fn attempt() { run_full(); }
}
RUST
reject 'crates/ignored/lib.rs'
rm "$fixture/crates/ignored/lib.rs"

cat > "$fixture/crates/ignored/README.md" <<'RUSTDOC'
```rust
run_full();
```
RUSTDOC
reject 'crates/ignored/README.md'
rm "$fixture/crates/ignored/README.md"

printf 'fn main() { run_full(); }\n' > "$fixture/crates/ignored/build.rs"
reject 'crates/ignored/build.rs:build-script:'
rm "$fixture/crates/ignored/build.rs"

# Repository-bound language helpers stay forbidden under synthetic markers and
# in test modules named outside the conventional tests/ directory.
cat > "$fixture/crates/ignored/grammar_corpus_tests.rs" <<'RUST'
#[test]
fn attempt() {
    // gmeow-test-input: synthetic-only
    let dataset = lang_module_dataset();
    GmnDictionary::from_dataset(&dataset);
}
RUST
reject 'crates/ignored/grammar_corpus_tests.rs'
rm "$fixture/crates/ignored/grammar_corpus_tests.rs"

cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
#[cfg(test)]
mod tests {
    // gmeow-test-input: synthetic-only
    fn leg() { GmnMigration::from_dataset(&demonstrator_dataset()); }
}
RUST
reject 'crates/ignored/lib.rs'
rm "$fixture/crates/ignored/lib.rs"

# #[test] alone is a test root, even outside cfg(test) and a tests/ directory.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
#[test]
fn standalone() { run_full(); }
RUST
reject 'crates/ignored/lib.rs'
rm "$fixture/crates/ignored/lib.rs"

# Standalone tests cannot invoke the admitted producer through a child CLI either.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
#[test]
fn standalone() {
    // gmeow-test-input: synthetic-only
    std::process::Command::new("gmeow-dev").arg("medium-sweep").status();
}
RUST
reject 'crates/ignored/lib.rs'
rm "$fixture/crates/ignored/lib.rs"

# Renamed helpers carry authored path -> bytes -> parsed model across parameters
# and return values. No helper's spelling is in the producer-call blacklist.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
fn locate() -> PathBuf { root().join("slices").join("grounding/lang/module.ttl") }
fn fetch(path: &Path) -> Vec<u8> { std::fs::read(path).unwrap() }
fn assemble(bytes: &[u8]) -> Model { GmnDictionary::from_dataset(&parse_dataset(bytes)) }
#[test]
fn attempt() {
    let bytes = fetch(&locate());
    // gmeow-test-input: synthetic-only
    assemble(&bytes);
}
RUST
reject 'authored-source bytes reach parse_dataset through local test flow'
reject 'authored-source bytes reach GmnDictionary::from_dataset through local test flow'
rm "$fixture/crates/ignored/lib.rs"

# Inlining tests beside a local parser must keep the same authored-byte seal.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
fn parse_tptp(source: &str) -> Vec<Formula> { tokenize(source).into_formulas() }
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_repository_source() {
        const CORPUS: &[&str] = &[include_str!("../../../conformance/problem.p")];
        for source in CORPUS { parse_tptp(source); }
    }
}
RUST
reject 'authored-source bytes reach parse_tptp through local test flow'
rm "$fixture/crates/ignored/lib.rs"

# Constants and grammar include macros are source flows, including raw strings.
cat > "$fixture/crates/ignored/tests/attempt.rs" <<'RUST'
const SOURCE: &str = include_str!(r#"../../../slices/grounding/lang/grammars/turtle.ebnf"#);
fn renamed(source: &str) { EbnfBridge.to_grammar(source.as_bytes()); }
#[test]
fn attempt() { renamed(SOURCE); }
RUST
reject 'authored-source bytes reach to_grammar through local test flow'
rm "$fixture/crates/ignored/tests/attempt.rs"

# File readers can return bytes or fill an explicitly borrowed mutable buffer.
cat > "$fixture/crates/ignored/tests/attempt.rs" <<'RUST'
#[test]
fn attempt() {
    let path = root().join("slices/core/module.ttl");
    let mut bytes = Vec::new();
    let mut file = std::fs::File::open(path).unwrap();
    file.read_to_end(&mut bytes).unwrap();
    parse_dataset(&bytes);
    let mut replacement = Vec::new();
    replacement = std::fs::read(root().join("dsl/statements/input.ttl")).unwrap();
    compile_native(replacement);
}
RUST
reject 'authored-source bytes reach parse_dataset through local test flow'
reject 'authored-source bytes reach compile_native through local test flow'

for reader in with_owl_rdfs_projection shacl_reader_view; do
    cat > "$fixture/crates/ignored/lib.rs" <<EOF
#[test]
fn lowers_authored_corpus() {
    let source = std::fs::read("slices/grounding/logic/module.ttl").unwrap();
    let dataset = source.into();
    $reader(&dataset);
}
EOF
    reject "authored-source bytes reach $reader through local test flow"
done

rm "$fixture/crates/ignored/tests/attempt.rs"

# Other canonical roots and direct lowering/building sinks obey the same seal.
for source_root in dsl imports conformance; do
    cat > "$fixture/crates/ignored/tests/attempt.rs" <<RUST
#[test]
fn attempt() {
    let bytes = include_bytes!("../../../$source_root/input.ttl");
    let alias = bytes;
    lower_native(alias);
    build_dictionary(alias);
}
RUST
    reject 'authored-source bytes reach lower_native through local test flow'
    reject 'authored-source bytes reach build_dictionary through local test flow'
done
rm "$fixture/crates/ignored/tests/attempt.rs"

# Native dataset constructors and loop-bound corpus paths cannot evade the seal.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
fn load(rel: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    purrdf::dataset_from_bytes(&std::fs::read(path).unwrap(), NativeRdfFormat::NQuads);
}
#[test]
fn attempt() {
    for rel in ["../../conformance/logic/cases/one/input.nq"] { load(rel); }
}
RUST
reject 'authored-source bytes reach purrdf::dataset_from_bytes through local test flow'
rm "$fixture/crates/ignored/lib.rs"

# Child CLI arguments retain authored origin through helpers and argument arrays.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
fn locate() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices/grounding/lang/module.ttl")
}
#[test]
fn attempt() {
    Command::new("gmeow").arg(locate());
    let arguments = ["--program-file", "conformance/logic/program.ttl"];
    Command::new("gmeow").args(arguments);
}
RUST
reject 'authored-source bytes reach arg through local test flow'
reject 'authored-source bytes reach args through local test flow'
rm "$fixture/crates/ignored/lib.rs"

# Authentication permits reading the existing corpus, not reasoning it again.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
fn selected_bundle() { gmeow_bundle_import::load_authenticated_repository_bundle(root()).unwrap().dataset }
#[test]
fn attempt() {
    let dataset = selected_bundle();
    let input = prepare_reasoning_input(dataset.as_ref()).unwrap();
    reason_program(program, input, domains);
}
RUST
reject 'authored-source bytes reach prepare_reasoning_input through local test flow'
reject 'authored-source bytes reach reason_program through local test flow'
rm "$fixture/crates/ignored/lib.rs"

# Exact artifact selection does not authorize another parser or reasoning pass.
for reader in authenticated_artifact authenticated_artifacts load_authenticated_corpus_artifact load_authenticated_corpus_archive load_authenticated_source_bytes; do
    cat > "$fixture/crates/ignored/lib.rs" <<RUST
#[test]
fn attempt() {
    let bytes = fixture::$reader(root(), "selected-product");
    let dataset = restore_pack(bytes).unwrap();
    reason_all(prepare_reasoning_input(dataset).unwrap(), domains);
}
RUST
    reject 'authored-source bytes reach reason_all through local test flow'
done
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
fn authenticated_artifact() { read_selected_action() }
#[test]
fn attempt() { parse_dataset(authenticated_artifact()); }
RUST
scan > "$fixture_root/result.log" 2>&1
cat >> "$fixture/crates/ignored/lib.rs" <<'RUST'
#[test]
fn semantic_reproduction() {
    reason_all(prepare_reasoning_input(parse_dataset(authenticated_artifact())).unwrap(), domains);
}
RUST
reject 'authored-source bytes reach prepare_reasoning_input through local test flow'
reject 'authored-source bytes reach reason_all through local test flow'
rm "$fixture/crates/ignored/lib.rs"

# A shared native view helper must not mix independent invocation arguments.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
fn identity(value: Data) -> Data { value }
fn nested(value: Data) -> Data { identity(value) }
#[test]
fn independent_inputs() {
    let bundle = gmeow_bundle_import::load_authenticated_repository_bundle(root()).unwrap().dataset;
    let inspected = nested(bundle);
    assert!(inspected.quad_count() > 0);
    parse_dataset(nested(b"<urn:s> <urn:p> <urn:o> ."));
}
RUST
scan > "$fixture_root/result.log" 2>&1
cat >> "$fixture/crates/ignored/lib.rs" <<'RUST'
fn recurse_left(value: Data) -> Data { recurse_right(value) }
fn recurse_right(value: Data) -> Data { if control { recurse_left(value) } else { value } }
#[test]
fn actual_corpus_parser() {
    let bundle = gmeow_bundle_import::load_authenticated_repository_bundle(root()).unwrap().dataset;
    parse_dataset(nested(bundle.clone()));
    parse_dataset(recurse_left(bundle));
}
RUST
reject 'authored-source bytes reach parse_dataset through local test flow'
if [[ $(grep -c 'authored-source bytes reach parse_dataset through local test flow' "$fixture_root/result.log") != 2 ]]; then
    cat "$fixture_root/result.log" >&2
    printf 'ERROR: independent and recursive local flows were not distinguished\n' >&2
    exit 1
fi
rm "$fixture/crates/ignored/lib.rs"

# Path metadata, authenticated observations, source-code inspections, uncalled
# production helpers, and explicit tiny literal parsing are permitted. Comments
# and string contents are not executable calls; Rust chars cannot unbalance items.
cat > "$fixture/crates/ignored/lib.rs" <<'RUST'
fn unused_producer() { parse_dataset(include_bytes!("../../../slices/core/module.ttl")); }
#[test]
fn controls() {
    let identity = "slices/grounding/lang/module.ttl";
    let bytes = authenticated_artifact(root(), identity, "pipeline/observations.json");
    serde_json::from_slice(&bytes);
    let packed = load_authenticated_corpus_artifact(root(), "native.purrpack");
    assert!(restore_pack(&packed).unwrap().quad_count() > 0);
    let snapshot = gmeow_bundle_import::load_authenticated_repository_bundle(root()).unwrap();
    assert!(snapshot.dataset.owned_quads().any(|quad| quad.predicate == expected));
    let tiny = b"<urn:s> <urn:p> <urn:o> .";
    parse_dataset(tiny);
    let shadowed = std::fs::read(root().join("slices/core/module.ttl")).unwrap();
    let shadowed = b"<urn:tiny> <urn:p> <urn:o> .";
    parse_dataset(shadowed);
    let source = include_str!("../src/grammar.rs");
    assert!(source.contains("parse_grammar"));
    let bracket = '\u{7b}';
    let documentation = r#"parse_dataset(include_bytes!(\"slices/core/module.ttl\"))"#;
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("slices/controlled/module.ttl");
    std::fs::write(&path, tiny).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    parse_dataset(&bytes);
    Command::new("gmeow").arg(&path);
    Command::new("gmeow").args(["--input", "generated/observed.ttl"]);
    Command::new("gmeow").arg("https://example.org/slices/user-term");
}
RUST
scan > "$fixture_root/result.log" 2>&1
rm "$fixture/crates/ignored/lib.rs"

# Temporary destinations cannot launder authored bytes into a synthetic fixture.
cat > "$fixture/crates/ignored/tests/attempt.rs" <<'RUST'
#[test]
fn attempt() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::copy(root().join("slices/core/module.ttl"), tmp.path().join("input.ttl"));
    let bytes = include_bytes!("../../../dsl/statements/input.ttl");
    std::fs::write(tmp.path().join("input.ttl"), bytes);
}
RUST
reject 'authored-source bytes reach std::fs::copy through local test flow'
reject 'authored-source bytes reach std::fs::write through local test flow'
rm "$fixture/crates/ignored/tests/attempt.rs"

# A broken lexical analysis is a gate failure, never an empty finding set.
printf '#[test]\nfn broken() { parse_dataset("tiny");\n' > "$fixture/crates/ignored/lib.rs"
reject 'unclosed Rust delimiter'
rm "$fixture/crates/ignored/lib.rs"

# An unavailable parser is a broken gate, including when the matching source is
# a production item with no test findings. Failure cannot mean "no matches".
printf 'fn producer() { run_full(); }\n' > "$fixture/crates/ignored/lib.rs"
mv "$fixture/scripts/scan-cfg-test-producers.pl" "$fixture_root/"
reject 'scan-cfg-test-producers.pl'
mv "$fixture_root/scan-cfg-test-producers.pl" "$fixture/scripts/"
mv "$fixture/scripts/scan-doctest-producers.pl" "$fixture_root/"
reject 'scan-doctest-producers.pl'
mv "$fixture_root/scan-doctest-producers.pl" "$fixture/scripts/"
mv "$fixture/scripts/scan-authored-test-flows.pl" "$fixture_root/"
reject 'scan-authored-test-flows.pl'
mv "$fixture_root/scan-authored-test-flows.pl" "$fixture/scripts/"
rm "$fixture/crates/ignored/lib.rs"

mv "$fixture/crates" "$fixture/absent-crates"
reject 'corpus-producer source discovery failed'
mv "$fixture/absent-crates" "$fixture/crates"
scan > "$fixture_root/result.log" 2>&1
printf 'corpus scanner regressions OK: authored local flows, ignored sources and tool failures are enforced\n'
