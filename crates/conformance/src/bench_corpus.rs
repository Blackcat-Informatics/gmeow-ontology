// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Loader for the committed engine-benchmark corpus (`conformance/logic/cases/bench/`).
//!
//! The bench corpus is a NEW sibling of the OWL-consistency `cases/external/` tree.
//! Each `cases/bench/<corpus>/` directory carries a `corpus.json`
//! ([`crate::external::corpus::CorpusMeta`]) whose declared SPDX license is audited
//! with the SAME `audit_vendorable` gate the external corpora use — a
//! non-vendorable (REFERENCE_ONLY / unknown) license is a HARD FAIL, never a
//! silently-loaded case. Each `<corpus>/<case>/` directory carries the four
//! benchmark artifacts:
//!
//! * `program.rules` — corpus-local fixture text. Forward and existential cases are
//!   parsed here into typed engine inputs before evaluation; backward cases use the
//!   goal-directed `.logic` query surface
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
//! The `incremental` and `incremental-grounding` fragments additionally carry
//! `delta.nq`: one single-world
//! insertion batch. The harness prepares a fixed-rule session from `input.nq`, applies
//! `delta.nq`, checks the resulting closure against a clean native rebuild, retracts the
//! same batch, and checks that the base closure is recovered. Its golden `rows` is the
//! derived-row count after insertion.
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
    /// Forward Datalog materialization.
    Forward,
    /// Existential (value-inventing) native TGD chase.
    Existential,
    /// Goal-directed backward query (native / captured SLD golden).
    Backward,
    /// Signed insert/retract maintenance of finite positive binary Datalog, compared
    /// against clean native rebuilds rather than a secondary engine.
    Incremental,
    /// Signed maintenance of the fully-ground WFS/stable-model solver slice. The
    /// grounder is incremental; the non-monotone solver remains from scratch and is
    /// reported explicitly per shot.
    IncrementalGrounding,
}

impl Fragment {
    /// Parse the `profile.json` `fragment` token (hard-fail on an unknown token).
    fn parse(s: &str) -> gmeow_errors::Result<Fragment> {
        match s {
            "forward" => Ok(Fragment::Forward),
            "existential" => Ok(Fragment::Existential),
            "backward" => Ok(Fragment::Backward),
            "incremental" => Ok(Fragment::Incremental),
            "incremental-grounding" => Ok(Fragment::IncrementalGrounding),
            other => Err(Diag::of_kind(ProfileInvalid {
                detail: format!(
                    "profile.json fragment must be \"forward\", \"existential\", \"backward\", \
                     \"incremental\", or \"incremental-grounding\", got {other:?}"
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
            Fragment::Incremental => "incremental",
            Fragment::IncrementalGrounding => "incremental-grounding",
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
    /// The engine rule / query text from `program.rules`, verbatim.
    pub rules: String,
    /// The world-scoped EDB as N-Quads (`input.nq`), verbatim.
    pub edb_nq: String,
    /// The signed insertion fixture (`delta.nq`) for [`Fragment::Incremental`] and
    /// [`Fragment::IncrementalGrounding`] cases ONLY — empty for every other
    /// fragment. The same rows are retracted after insertion to prove parity.
    pub delta_nq: String,
    /// The hand-derived golden, keyed by world IRI (sorted).
    pub golden: std::collections::BTreeMap<String, GoldenWorld>,
}

impl BenchCase {
    /// Parse this corpus fixture into the canonical typed logic program consumed by
    /// the native forward, incremental, and grounding seams.
    ///
    /// The compact `program.rules` syntax is a storage format local to the benchmark
    /// corpus. It never crosses into the engine API.
    pub fn canonical_program(&self) -> gmeow_errors::Result<gmeow_logic_compile::ir::LogicProgram> {
        use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};

        let rules = parse_fixture_rules(&self.rules)?;
        let mut canonical = Vec::with_capacity(rules.len());
        for rule in rules {
            if rule.head.subject.existential
                || rule.head.object.existential
                || rule
                    .body
                    .iter()
                    .any(|atom| atom.subject.existential || atom.object.existential)
            {
                return Err(fixture_err(format!(
                    "{}/{}: existential variable found where a finite canonical program was required",
                    self.corpus, self.name
                )));
            }
            let head = fixture_axiom(&rule.head)?;
            let body = rule
                .body
                .iter()
                .map(fixture_axiom)
                .collect::<gmeow_errors::Result<Vec<LogicAxiom>>>()?;
            let scope = ContextualScope {
                provenance: Some(rule.rule_iri),
                ..ContextualScope::default()
            };
            canonical.push(LogicRule::new(head, body, Vec::new(), scope));
        }
        Ok(LogicProgram::new(Vec::new(), canonical, Vec::new(), None))
    }

    /// Parse this existential corpus fixture into structured restricted-chase rules.
    ///
    /// The returned rules are the typed public boundary; compact fixture parsing is
    /// contained in this corpus adapter.
    pub fn existential_rules(
        &self,
    ) -> gmeow_errors::Result<Vec<gmeow_logic::materialize::StructuredExistentialRule>> {
        use gmeow_logic::materialize::{StructuredAtom, StructuredExistentialRule, StructuredTerm};

        fn term(term: &FixtureTerm) -> StructuredTerm {
            if term.value.starts_with('?') {
                StructuredTerm::var(&term.value)
            } else {
                StructuredTerm::named(&term.value)
            }
        }

        fn atom(atom: &FixtureAtom) -> StructuredAtom {
            StructuredAtom::new(term(&atom.subject), &atom.predicate, term(&atom.object))
        }

        parse_fixture_rules(&self.rules)?
            .into_iter()
            .map(|rule| {
                if rule
                    .body
                    .iter()
                    .any(|body| body.negated || body.subject.existential || body.object.existential)
                {
                    return Err(fixture_err(format!(
                        "{}/{}: existential benchmark bodies must be positive and frontier-bound",
                        self.corpus, self.name
                    )));
                }
                Ok(StructuredExistentialRule {
                    rule_iri: rule.rule_iri,
                    body: rule.body.iter().map(atom).collect(),
                    head: vec![atom(&rule.head)],
                    distinct: Vec::new(),
                    witness_frontier: None,
                })
            })
            .collect()
    }
}

#[derive(Debug)]
struct FixtureTerm {
    value: String,
    existential: bool,
}

#[derive(Debug)]
struct FixtureAtom {
    subject: FixtureTerm,
    predicate: String,
    object: FixtureTerm,
    negated: bool,
}

#[derive(Debug)]
struct FixtureRule {
    rule_iri: String,
    head: FixtureAtom,
    body: Vec<FixtureAtom>,
}

fn fixture_err(detail: String) -> Diag {
    Diag::of_kind(CorpusInvalid { detail })
}

fn fixture_axiom(atom: &FixtureAtom) -> gmeow_errors::Result<gmeow_logic_compile::ir::LogicAxiom> {
    gmeow_logic_compile::ir::LogicAxiom::new(
        &atom.subject.value,
        &atom.predicate,
        &atom.object.value,
        false,
        atom.negated,
        gmeow_logic_compile::ir::ContextualScope::default(),
    )
}

fn parse_fixture_rules(source: &str) -> gmeow_errors::Result<Vec<FixtureRule>> {
    let mut rules = Vec::new();
    let mut name = None;
    let mut statement = String::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('%') {
            continue;
        }
        if let Some(inner) = trimmed
            .strip_prefix("#[name(\"")
            .and_then(|value| value.strip_suffix("\")]"))
        {
            name = Some(inner.to_owned());
            continue;
        }
        if !statement.is_empty() {
            statement.push(' ');
        }
        statement.push_str(trimmed);
        if !trimmed.ends_with('.') {
            continue;
        }
        let text = statement.trim_end_matches('.').trim();
        let (head, body) = text
            .split_once(":-")
            .ok_or_else(|| fixture_err(format!("benchmark rule lacks ':-': {text}")))?;
        rules.push(FixtureRule {
            rule_iri: name
                .take()
                .unwrap_or_else(|| format!("urn:gmeow:benchmark-rule:{}", rules.len())),
            head: parse_fixture_atom(head, false)?,
            body: split_fixture_atoms(body)
                .into_iter()
                .map(|text| {
                    let negated = text.trim_start().starts_with('~');
                    parse_fixture_atom(text, negated)
                })
                .collect::<gmeow_errors::Result<Vec<_>>>()?,
        });
        statement.clear();
    }
    if !statement.trim().is_empty() {
        return Err(fixture_err(format!(
            "unterminated benchmark rule: {statement}"
        )));
    }
    Ok(rules)
}

fn split_fixture_atoms(body: &str) -> Vec<&str> {
    let mut atoms = Vec::new();
    let mut depth = 0u32;
    let mut start = 0usize;
    for (index, byte) in body.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                atoms.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    atoms.push(&body[start..]);
    atoms
}

fn parse_fixture_atom(text: &str, negated: bool) -> gmeow_errors::Result<FixtureAtom> {
    let text = text.trim().strip_prefix('~').unwrap_or(text.trim());
    let open = text
        .find('(')
        .ok_or_else(|| fixture_err(format!("invalid benchmark atom: {text}")))?;
    let close = text
        .rfind(')')
        .ok_or_else(|| fixture_err(format!("invalid benchmark atom: {text}")))?;
    let predicate = parse_fixture_iri(text[..open].trim())?;
    let args = text[open + 1..close]
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    if args.len() != 3 {
        return Err(fixture_err(format!(
            "benchmark atom must be named-ternary, got {} arguments: {text}",
            args.len()
        )));
    }
    if !args[2].starts_with('?') {
        return Err(fixture_err(format!(
            "benchmark atom world slot must be a variable: {text}"
        )));
    }
    Ok(FixtureAtom {
        subject: parse_fixture_term(args[0])?,
        predicate,
        object: parse_fixture_term(args[1])?,
        negated,
    })
}

fn parse_fixture_iri(text: &str) -> gmeow_errors::Result<String> {
    text.strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .map(str::to_owned)
        .ok_or_else(|| fixture_err(format!("benchmark predicate must be an IRI: {text}")))
}

fn parse_fixture_term(text: &str) -> gmeow_errors::Result<FixtureTerm> {
    if let Some(variable) = text.strip_prefix('?') {
        return Ok(FixtureTerm {
            value: format!("?{variable}"),
            existential: false,
        });
    }
    if let Some(variable) = text.strip_prefix('!') {
        return Ok(FixtureTerm {
            value: format!("?{variable}"),
            existential: true,
        });
    }
    Ok(FixtureTerm {
        value: parse_fixture_iri(text)?,
        existential: false,
    })
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
/// Every fragment carries `program.rules` plus a world-scoped `input.nq`; incremental
/// fragments additionally carry `delta.nq`. The profile is read first so the loader
/// knows whether the delta artifact is required.
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
    let (rules, edb_nq, delta_nq) = match fragment {
        Fragment::Incremental | Fragment::IncrementalGrounding => {
            let rules = read_to_string(&case_dir.join("program.rules"))?;
            let edb_nq = read_to_string(&case_dir.join("input.nq"))?;
            let delta_nq = read_to_string(&case_dir.join("delta.nq"))?;
            (rules, edb_nq, delta_nq)
        }
        Fragment::Forward | Fragment::Existential | Fragment::Backward => {
            let rules = read_to_string(&case_dir.join("program.rules"))?;
            let edb_nq = read_to_string(&case_dir.join("input.nq"))?;
            (rules, edb_nq, String::new())
        }
    };

    Ok(BenchCase {
        corpus: corpus.to_owned(),
        name: name.to_owned(),
        fragment,
        engines,
        rules,
        edb_nq,
        delta_nq,
        golden,
    })
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
        for want in ["chasebench-mini", "relational-core-mini"] {
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
            // Every retained case carries a world-scoped N-Quads EDB.
            match c.fragment {
                Fragment::Incremental | Fragment::IncrementalGrounding => {
                    assert!(
                        !c.edb_nq.trim().is_empty(),
                        "{}/{}: empty incremental input.nq",
                        c.corpus,
                        c.name
                    );
                    assert!(
                        !c.delta_nq.trim().is_empty(),
                        "{}/{}: empty incremental delta.nq",
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
                        c.delta_nq.is_empty(),
                        "{}/{}: a non-incremental case carries no delta.nq",
                        c.corpus,
                        c.name
                    );
                }
            }
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

        // The existential (chasebench) cases parse into typed rules and RUN through
        // the value-inventing chase router (the golden itself is authored by hand,
        // not echoed from this run).
        for c in cases.iter().filter(|c| c.fragment == Fragment::Existential) {
            let dataset = purrdf::parse_dataset(c.edb_nq.as_bytes(), "application/n-quads", None)
                .unwrap_or_else(|e| panic!("{}/{}: EDB must parse: {e}", c.corpus, c.name));
            let rules = c.existential_rules().unwrap_or_else(|e| {
                panic!(
                    "{}/{}: existential TGD fixture must parse: {e}",
                    c.corpus, c.name
                )
            });
            gmeow_logic::materialize::materialize_existential_rules(
                dataset.as_ref(),
                &rules,
                gmeow_logic::materialize::MaterializationLimits::default(),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "{}/{}: existential typed rules must run: {e}",
                    c.corpus, c.name
                )
            });
        }

        for c in cases.iter().filter(|c| {
            matches!(
                c.fragment,
                Fragment::Forward | Fragment::Incremental | Fragment::IncrementalGrounding
            )
        }) {
            c.canonical_program().unwrap_or_else(|e| {
                panic!(
                    "{}/{}: forward fixture must lower to canonical IR: {e}",
                    c.corpus, c.name
                )
            });
        }
    }
}
