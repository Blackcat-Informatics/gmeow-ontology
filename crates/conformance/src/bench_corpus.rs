// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Loader for the committed engine-benchmark corpus (`conformance/logic/cases/bench/`).
//!
//! The bench corpus is a NEW sibling of the OWL-consistency `cases/external/` tree.
//! Each `cases/bench/<corpus>/` directory carries a `corpus.json`
//! ([`crate::external::corpus::CorpusMeta`]) whose declared SPDX license is audited
//! with the SAME [`audit_vendorable`] gate the external corpora use — a
//! non-vendorable (REFERENCE_ONLY / unknown) license is a HARD FAIL, never a
//! silently-loaded case. Each `<corpus>/<case>/` directory carries the four
//! benchmark artifacts:
//!
//! * `program.rules` — engine rule text. For `forward`/`existential` cases this is
//!   the world-scoped ternary `#[name(...)]` surface
//!   [`gmeow_logic::cost::run_native_forward`] / [`gmeow_logic::materialize::materialize_routed`]
//!   accept; for `backward` cases it is the goal-directed `.logic` query surface
//!   [`gmeow_logic::query_ir::parse_query_program`] parses and
//!   [`gmeow_logic::dispatch::dispatch_query`] consumes (the EDB is supplied
//!   separately from `input.nq`, not inlined in the query).
//! * `input.nq` — the world-scoped EDB as N-Quads.
//! * `expected/result.json` — the HAND-DERIVED, known-correct golden: a map from
//!   world IRI to `{ rows, digest? }`, where `rows` is the mathematically-correct
//!   count of derived facts (forward/existential) or goal answers (backward),
//!   authored by formula/hand — never an engine echo.
//! * `profile.json` — `{ "fragment": …, "engines": [ … ] }`.
//!
//! The `nary-existential` fragment (the ChaseBench/Nemo-KR2024 family shape) carries a
//! DIFFERENT EDB + rule surface instead of `program.rules` + `input.nq`: an n-ary
//! `program.rls` program ([`gmeow_logic::nary_rls::parse_nary_rls_program`]) plus a `data/`
//! directory of delimited (`<rel>.csv` / `<rel>.tsv`, optionally `.gz`) n-ary EDB relations
//! ([`gmeow_logic::nary_rls::load_nary_data_file`]). The same `program.rls` drives BOTH the
//! native reified-binary chase and the Nemo n-ary oracle; its `expected/result.json` golden
//! is the hand-derived de-reified closure tuple count keyed by a nominal world IRI.
//!
//! Loading is manual + hard-fail (matching `external/corpus.rs`): a missing artifact,
//! a wrong type, an unknown key, or an engine/fragment inconsistency is an error,
//! never a silent default. Ordering is deterministic: corpora then cases, both sorted
//! by directory name.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use gmeow_errors::Diag;

use crate::error::{CorpusInvalid, Io, ProfileInvalid};
use crate::external::corpus::{audit_vendorable, parse_corpus_meta};

/// The reasoning fragment a bench case exercises — it selects both the rule surface
/// (`program.rules`) and the engines the harness may drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fragment {
    /// Forward Datalog materialization (native / Nemo).
    Forward,
    /// Existential (value-inventing) TGD chase (native / Nemo).
    Existential,
    /// Goal-directed backward query (native / captured SLD golden).
    Backward,
    /// Fixed-arity n-ary multi-head existential TGD chase (native reified-binary lowering
    /// vs Nemo), driven from an n-ary `.rls` program + delimited (`data/<rel>.csv`) EDB —
    /// the ChaseBench/Nemo-KR2024 family shape. Distinct from [`Fragment::Existential`]
    /// (which is the ternary world-scoped surface over an `input.nq` EDB).
    NaryExistential,
}

impl Fragment {
    /// Parse the `profile.json` `fragment` token (hard-fail on an unknown token).
    fn parse(s: &str) -> gmeow_errors::Result<Fragment> {
        match s {
            "forward" => Ok(Fragment::Forward),
            "existential" => Ok(Fragment::Existential),
            "backward" => Ok(Fragment::Backward),
            "nary-existential" => Ok(Fragment::NaryExistential),
            other => Err(Diag::of_kind(ProfileInvalid {
                detail: format!(
                    "profile.json fragment must be \"forward\", \"existential\", \"backward\", \
                     or \"nary-existential\", got {other:?}"
                ),
            })),
        }
    }

    /// The lowercase wire token for this fragment.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Fragment::Forward => "forward",
            Fragment::Existential => "existential",
            Fragment::Backward => "backward",
            Fragment::NaryExistential => "nary-existential",
        }
    }
}

/// The hand-derived golden for one world: the known-correct row/answer count plus an
/// optional stable digest of the sorted correct result rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenWorld {
    /// The mathematically-correct count of derived facts / goal answers in this world.
    pub rows: u64,
    /// An optional stable hex digest of the sorted correct result rows.
    pub digest: Option<String>,
}

/// A loaded, typed benchmark case.
///
/// `Eq` is intentionally NOT derived: an [`Fragment::NaryExistential`] case carries a
/// [`gmeow_logic::nary::NaryTuple`] EDB whose `purrdf::TermValue` arguments are `PartialEq`
/// but not `Eq` (a float literal has no total equality), so the whole case is `PartialEq`
/// only. Nothing keys a `BenchCase` in a hashed/ordered set, so this loses nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchCase {
    /// The owning corpus directory name (`cases/bench/<corpus>/`).
    pub corpus: String,
    /// The case directory name (`<corpus>/<name>/`).
    pub name: String,
    /// The reasoning fragment this case exercises.
    pub fragment: Fragment,
    /// The engines the harness may drive this case through (as declared).
    pub engines: Vec<String>,
    /// The engine rule / query text — `program.rules` for the ternary
    /// forward/existential/backward fragments, or the n-ary `program.rls` for
    /// [`Fragment::NaryExistential`] — verbatim.
    pub rules: String,
    /// The world-scoped EDB as N-Quads (`input.nq`), verbatim. EMPTY for an
    /// [`Fragment::NaryExistential`] case, whose EDB is the n-ary `nary_edb` instead.
    pub edb_nq: String,
    /// The n-ary EDB (`data/<rel>.csv` files, arity-driven), for
    /// [`Fragment::NaryExistential`] cases ONLY — empty for every other fragment.
    pub nary_edb: Vec<gmeow_logic::nary::NaryTuple>,
    /// The hand-derived golden, keyed by world IRI (sorted).
    pub golden: std::collections::BTreeMap<String, GoldenWorld>,
}

/// The engine-benchmark corpus root, `conformance/logic/cases/bench/`.
#[must_use]
pub fn bench_cases_root() -> PathBuf {
    crate::paths::cases_root().join("bench")
}

/// Enumerate and load every committed bench case under [`bench_cases_root`],
/// auditing each corpus's license before any of its cases are admitted.
///
/// The returned vector is deterministically ordered: corpora sorted by directory
/// name, and within each corpus, cases sorted by directory name.
///
/// # Errors
///
/// Hard-fails when the bench root is missing, a `corpus.json` is unreadable / invalid
/// / declares a non-vendorable license, or any case artifact is missing / malformed.
pub fn load_bench_corpora() -> gmeow_errors::Result<Vec<BenchCase>> {
    load_bench_corpora_from(&bench_cases_root())
}

/// Enumerate and load every committed bench case under an explicit `root`, auditing
/// each corpus's license before any of its cases are admitted.
///
/// This is the root-parameterized form of [`load_bench_corpora`] (which delegates
/// here with [`bench_cases_root`]). It is the seam a later fetch lane points at a
/// full fetched-distribution corpus root: the `bench-engines` harness's
/// `--corpus-dir <path>` flag flows straight into this function, so the exact same
/// hard-fail loader and license audit govern committed and fetched corpora alike.
///
/// The returned vector is deterministically ordered: corpora sorted by directory
/// name, and within each corpus, cases sorted by directory name.
///
/// # Errors
///
/// Hard-fails when `root` is missing, a `corpus.json` is unreadable / invalid /
/// declares a non-vendorable license, or any case artifact is missing / malformed.
pub fn load_bench_corpora_from(root: &Path) -> gmeow_errors::Result<Vec<BenchCase>> {
    if !root.is_dir() {
        return Err(Diag::of_kind(Io {
            detail: format!(
                "engine-benchmark corpus root does not exist: {}. Expected \
                 conformance/logic/cases/bench/ (or an explicit --corpus-dir) to be present.",
                root.display()
            ),
        }));
    }

    let mut out: Vec<BenchCase> = Vec::new();
    for corpus_dir in sorted_subdirs(root)? {
        let corpus_name = dir_name(&corpus_dir)?;

        // License gate: audit the declared license BEFORE admitting any case (a
        // REFERENCE_ONLY / unknown license is a hard fail, never a silent skip).
        let corpus_json = corpus_dir.join("corpus.json");
        let meta = parse_corpus_meta(&read_json(&corpus_json)?).map_err(|e| {
            Diag::of_kind(CorpusInvalid {
                detail: format!("{}: {e}", corpus_json.display()),
            })
        })?;
        audit_vendorable(&meta)?;

        for case_dir in sorted_subdirs(&corpus_dir)? {
            let name = dir_name(&case_dir)?;
            out.push(load_case(&corpus_name, &name, &case_dir)?);
        }
    }
    Ok(out)
}

/// Load one case directory into a typed [`BenchCase`].
///
/// The rule-text and EDB artifacts depend on the fragment: the ternary
/// forward/existential/backward fragments carry `program.rules` + an `input.nq` N-Quads
/// EDB, while the [`Fragment::NaryExistential`] fragment carries an n-ary `program.rls`
/// program + a `data/` directory of delimited (`<rel>.csv`) n-ary EDB relations. The
/// profile is read FIRST so the shape is known before the shape-specific artifacts load.
fn load_case(corpus: &str, name: &str, case_dir: &Path) -> gmeow_errors::Result<BenchCase> {
    let (fragment, engines) = parse_profile(&read_json(&case_dir.join("profile.json"))?)?;
    let golden = parse_golden(&read_json(&case_dir.join("expected").join("result.json"))?)?;

    // Every backward case must explicitly list the native engine. Its committed
    // digest is the retained SLD answer-set reference; no live secondary engine is
    // constructed by the corpus loader.
    if fragment == Fragment::Backward && !engines.iter().any(|e| e == "native") {
        return Err(Diag::of_kind(ProfileInvalid {
            detail: format!(
                "{corpus}/{name}: a backward-fragment case must list the native engine"
            ),
        }));
    }

    // Shape-specific rule text + EDB.
    let (rules, edb_nq, nary_edb) = match fragment {
        Fragment::NaryExistential => {
            let rules = read_to_string(&case_dir.join("program.rls"))?;
            // Resolve the delimited EDB relation stems against the SAME `@prefix` map the
            // Nemo front-end resolves the rule-atom CURIEs against, so a CURIE stem
            // (`nf:isMainClass.csv`) names the SAME relation IRI as its rule atom — without
            // this the reified body atoms never join the EDB and the native chase silently
            // derives nothing.
            let prefixes = gmeow_logic::nary_rls::parse_rls_prefixes(&rules);
            let nary_edb = load_nary_edb_dir(corpus, name, &case_dir.join("data"), &prefixes)?;
            (rules, String::new(), nary_edb)
        }
        Fragment::Forward | Fragment::Existential | Fragment::Backward => {
            let rules = read_to_string(&case_dir.join("program.rules"))?;
            let edb_nq = read_to_string(&case_dir.join("input.nq"))?;
            (rules, edb_nq, Vec::new())
        }
    };

    Ok(BenchCase {
        corpus: corpus.to_owned(),
        name: name.to_owned(),
        fragment,
        engines,
        rules,
        edb_nq,
        nary_edb,
        golden,
    })
}

/// Load every delimited n-ary EDB relation under `data_dir` into one [`gmeow_logic::nary::NaryTuple`]
/// vector, in deterministic (sorted-filename) order. Each `<rel>.csv` / `<rel>.tsv`
/// (optionally `.gz`) file is one relation; the loader is arity-strict and hard-fails on a
/// malformed / non-uniform file. An empty (or missing) `data/` directory is a HARD FAIL —
/// an n-ary case with no EDB is a corpus defect, never a silently-empty run.
fn load_nary_edb_dir(
    corpus: &str,
    name: &str,
    data_dir: &Path,
    prefixes: &std::collections::BTreeMap<String, String>,
) -> gmeow_errors::Result<Vec<gmeow_logic::nary::NaryTuple>> {
    if !data_dir.is_dir() {
        return Err(Diag::of_kind(CorpusInvalid {
            detail: format!(
                "{corpus}/{name}: an nary-existential case must carry a data/ directory of \
                 delimited n-ary EDB relations, found none at {}",
                data_dir.display()
            ),
        }));
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(data_dir)
        .map_err(|e| {
            Diag::of_kind(Io {
                detail: format!("cannot read directory {}: {e}", data_dir.display()),
            })
        })?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_file())
        .collect();
    files.sort();

    let mut edb: Vec<gmeow_logic::nary::NaryTuple> = Vec::new();
    for path in &files {
        let tuples = gmeow_logic::nary_rls::load_nary_data_file(path, prefixes).map_err(|e| {
            Diag::of_kind(CorpusInvalid {
                detail: format!("{corpus}/{name}: {} — {e}", path.display()),
            })
        })?;
        edb.extend(tuples);
    }
    if edb.is_empty() {
        return Err(Diag::of_kind(CorpusInvalid {
            detail: format!(
                "{corpus}/{name}: the data/ directory carries no n-ary EDB tuples — an \
                 nary-existential case must have at least one fact"
            ),
        }));
    }
    Ok(edb)
}

/// Parse `profile.json` (`{ "fragment": …, "engines": [ … ] }`) — closed surface,
/// hard-fail on unknown keys / wrong types / an empty engine list.
fn parse_profile(value: &Value) -> gmeow_errors::Result<(Fragment, Vec<String>)> {
    let obj = as_object(value, "profile.json")?;
    reject_unknown_keys(obj, &["fragment", "engines"], "profile.json")?;

    let fragment = Fragment::parse(&required_string(obj, "fragment", "profile.json")?)?;

    let engines_val = obj.get("engines").ok_or_else(|| {
        Diag::of_kind(ProfileInvalid {
            detail: "profile.json is missing the required array field \"engines\"".to_string(),
        })
    })?;
    let engines_arr = engines_val.as_array().ok_or_else(|| {
        Diag::of_kind(ProfileInvalid {
            detail: "profile.json \"engines\" must be an array of strings".to_string(),
        })
    })?;
    let mut engines: Vec<String> = Vec::with_capacity(engines_arr.len());
    for e in engines_arr {
        let s = e.as_str().ok_or_else(|| {
            Diag::of_kind(ProfileInvalid {
                detail: "profile.json \"engines\" must contain only strings".to_string(),
            })
        })?;
        engines.push(s.to_owned());
    }
    if engines.is_empty() {
        return Err(Diag::of_kind(ProfileInvalid {
            detail: "profile.json \"engines\" must list at least one engine".to_string(),
        }));
    }
    Ok((fragment, engines))
}

/// Parse `expected/result.json` — a map from world IRI to `{ rows, digest? }`.
fn parse_golden(
    value: &Value,
) -> gmeow_errors::Result<std::collections::BTreeMap<String, GoldenWorld>> {
    let obj = as_object(value, "expected/result.json")?;
    if obj.is_empty() {
        return Err(Diag::of_kind(CorpusInvalid {
            detail: "expected/result.json must carry at least one world entry".to_string(),
        }));
    }
    let mut golden = std::collections::BTreeMap::new();
    for (world, entry) in obj {
        let inner = entry.as_object().ok_or_else(|| {
            Diag::of_kind(CorpusInvalid {
                detail: format!(
                    "expected/result.json world {world:?} must map to an object {{ rows, digest? }}"
                ),
            })
        })?;
        reject_unknown_keys(
            inner,
            &["rows", "digest"],
            "expected/result.json world entry",
        )?;
        let rows = inner.get("rows").and_then(Value::as_u64).ok_or_else(|| {
            Diag::of_kind(CorpusInvalid {
                detail: format!(
                    "expected/result.json world {world:?} is missing the required \
                         unsigned-integer field \"rows\""
                ),
            })
        })?;
        let digest = match inner.get("digest") {
            None => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => {
                return Err(Diag::of_kind(CorpusInvalid {
                    detail: format!(
                        "expected/result.json world {world:?} \"digest\" must be a string"
                    ),
                }));
            }
        };
        golden.insert(world.clone(), GoldenWorld { rows, digest });
    }
    Ok(golden)
}

// ── Small shared helpers (manual, hard-fail) ────────────────────────────────────

/// The immediate subdirectories of `dir`, sorted by name (deterministic order).
fn sorted_subdirs(dir: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut subs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| {
            Diag::of_kind(Io {
                detail: format!("cannot read directory {}: {e}", dir.display()),
            })
        })?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    subs.sort();
    Ok(subs)
}

/// The final path component as a `String` (hard-fail on a non-UTF-8 name).
fn dir_name(dir: &Path) -> gmeow_errors::Result<String> {
    dir.file_name()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .ok_or_else(|| {
            Diag::of_kind(Io {
                detail: format!("directory {} has no valid UTF-8 name", dir.display()),
            })
        })
}

/// Read a file to a `String` (hard-fail on I/O error).
fn read_to_string(path: &Path) -> gmeow_errors::Result<String> {
    std::fs::read_to_string(path).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("cannot read {}: {e}", path.display()),
        })
    })
}

/// Read and parse a JSON file (hard-fail on I/O or parse error).
fn read_json(path: &Path) -> gmeow_errors::Result<Value> {
    let text = read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| {
        Diag::of_kind(CorpusInvalid {
            detail: format!("cannot parse {}: {e}", path.display()),
        })
    })
}

/// Coerce a JSON value to an object (hard-fail otherwise).
fn as_object<'a>(value: &'a Value, what: &str) -> gmeow_errors::Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| {
        Diag::of_kind(CorpusInvalid {
            detail: format!("{what} must be a JSON object"),
        })
    })
}

/// Reject any key outside `allowed` (closed surface).
fn reject_unknown_keys(
    obj: &Map<String, Value>,
    allowed: &[&str],
    what: &str,
) -> gmeow_errors::Result<()> {
    let mut unknown: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|k| !allowed.contains(k))
        .collect();
    unknown.sort_unstable();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(Diag::of_kind(CorpusInvalid {
            detail: format!("{what} has unknown key(s) {unknown:?}; allowed keys are {allowed:?}"),
        }))
    }
}

/// Read a required string field (hard-fail on missing / wrong type).
fn required_string(
    obj: &Map<String, Value>,
    key: &str,
    what: &str,
) -> gmeow_errors::Result<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            Diag::of_kind(ProfileInvalid {
                detail: format!("{what} is missing the required string field {key:?}"),
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every committed `corpus.json` under `cases/bench/` passes the license audit,
    /// the loader loads ≥1 case per corpus, and every loaded case carries a non-empty
    /// hand-derived golden with a positive row count. Ordering is deterministic.
    #[test]
    fn bench_corpus_loads_audited_cases_with_nonempty_goldens() {
        let cases = load_bench_corpora().expect("bench corpus must load");
        assert!(!cases.is_empty(), "the bench corpus must contain cases");

        // Deterministic order: (corpus, name) is non-decreasing.
        let keys: Vec<(String, String)> = cases
            .iter()
            .map(|c| (c.corpus.clone(), c.name.clone()))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "bench cases must be sorted by (corpus, name)");

        // ≥1 case per committed corpus.
        let corpora: std::collections::BTreeSet<&str> =
            cases.iter().map(|c| c.corpus.as_str()).collect();
        for want in [
            "chasebench-mini",
            "nary-mini",
            "nemo-kr2024-mini",
            "relational-core-mini",
        ] {
            assert!(
                corpora.contains(want),
                "expected a loaded case from corpus {want:?}; loaded corpora: {corpora:?}"
            );
            let n = cases.iter().filter(|c| c.corpus == want).count();
            assert!(n >= 1, "corpus {want:?} must contribute >= 1 case, got {n}");
        }

        // Every golden is non-empty with a positive, hand-derived row count, and the
        // artifacts are non-empty.
        for c in &cases {
            assert!(
                !c.golden.is_empty(),
                "{}/{}: empty golden",
                c.corpus,
                c.name
            );
            assert!(
                c.golden.values().all(|g| g.rows > 0),
                "{}/{}: golden must carry a positive row count",
                c.corpus,
                c.name
            );
            assert!(
                !c.rules.trim().is_empty(),
                "{}/{}: empty rule text",
                c.corpus,
                c.name
            );
            assert!(
                !c.engines.is_empty(),
                "{}/{}: empty engine list",
                c.corpus,
                c.name
            );
            // The EDB carrier is fragment-specific: the ternary fragments carry a non-empty
            // `input.nq`; the n-ary fragment carries a non-empty `nary_edb` (and no N-Quads).
            match c.fragment {
                Fragment::NaryExistential => {
                    assert!(
                        c.edb_nq.is_empty(),
                        "{}/{}: an nary-existential case has no input.nq EDB",
                        c.corpus,
                        c.name
                    );
                    assert!(
                        !c.nary_edb.is_empty(),
                        "{}/{}: an nary-existential case must carry a non-empty n-ary EDB",
                        c.corpus,
                        c.name
                    );
                }
                _ => {
                    assert!(
                        !c.edb_nq.trim().is_empty(),
                        "{}/{}: empty input.nq",
                        c.corpus,
                        c.name
                    );
                    assert!(
                        c.nary_edb.is_empty(),
                        "{}/{}: a ternary-fragment case carries no n-ary EDB",
                        c.corpus,
                        c.name
                    );
                }
            }
        }

        // The nary-existential cases parse (n-ary `.rls`) and RUN through the reified
        // native chase over their committed CSV EDB — confirming the committed program +
        // data are real engine input (the golden itself is authored by hand, not echoed).
        for c in cases
            .iter()
            .filter(|c| c.fragment == Fragment::NaryExistential)
        {
            let rules = gmeow_logic::nary_rls::parse_nary_rls_program(&c.rules)
                .unwrap_or_else(|e| panic!("{}/{}: n-ary .rls must parse: {e}", c.corpus, c.name));
            gmeow_logic::nary::run_native_nary_forward(&c.nary_edb, &rules).unwrap_or_else(|e| {
                panic!(
                    "{}/{}: n-ary program must run natively over its CSV EDB: {e}",
                    c.corpus, c.name
                )
            });
        }

        // The required goal-directed backward native case is present with a captured
        // full-answer digest, over the parseable query surface.
        let backward: Vec<&BenchCase> = cases
            .iter()
            .filter(|c| c.fragment == Fragment::Backward)
            .collect();
        assert!(
            !backward.is_empty(),
            "the bench corpus must include >= 1 backward native case"
        );
        for c in &backward {
            assert!(
                c.engines.iter().any(|e| e == "native"),
                "{}/{}: backward case must list the native engine",
                c.corpus,
                c.name
            );
            assert!(
                c.golden.values().all(|g| g.digest.is_some()),
                "{}/{}: backward case must carry a captured answer-set digest",
                c.corpus,
                c.name
            );
            // Confirm the query text parses on the native production surface.
            gmeow_logic::query_ir::parse_query_program(&c.rules).unwrap_or_else(|e| {
                panic!(
                    "{}/{}: backward query text must parse as a QProgram: {e}",
                    c.corpus, c.name
                )
            });
        }

        // The existential (chasebench) cases parse and RUN through the value-inventing
        // chase router (confirming the committed TGD text is real engine input — the
        // golden itself is authored by hand, not echoed from this run).
        for c in cases.iter().filter(|c| c.fragment == Fragment::Existential) {
            gmeow_logic::materialize::materialize_routed(
                &c.rules,
                &c.edb_nq,
                None,
                None,
                None,
                Some("PositiveHornProfile"),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "{}/{}: existential TGD text must parse and run: {e}",
                    c.corpus, c.name
                )
            });
        }
    }
}
