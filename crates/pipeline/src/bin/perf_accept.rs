// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Grade repeated paired performance samples without turning time into correctness.
//!
//! This report-only tool fails its invocation when the predeclared optimization
//! acceptance contract is not demonstrated.  It is deliberately absent from
//! `make check`: ontology correctness never depends on runner speed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug)]
struct Args {
    slow_node_class: String,
    comparison_node_class: String,
    headline_cache_state: String,
    required_cache_states: BTreeSet<String>,
    min_pairs: usize,
    slow_speedup_target: f64,
    comparison_regression_limit_pct: f64,
    semantic_identities: Vec<(String, String)>,
    work_counters: Vec<(String, String)>,
    output: PathBuf,
    samples: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct Sample {
    path: PathBuf,
    value: serde_json::Value,
    pair_id: String,
    variant: String,
    sample_index: u64,
    node_class: String,
    cache_state: String,
    wall_ms: f64,
    work: BTreeMap<String, f64>,
}

#[derive(Debug, Clone)]
struct Pair {
    baseline: Sample,
    candidate: Sample,
}

fn main() {
    let result = parse_args().and_then(run);
    match result {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("perf-accept: {error}");
            std::process::exit(2);
        }
    }
}

fn parse_args() -> Result<Args, String> {
    let mut slow_node_class = None;
    let mut comparison_node_class = None;
    let mut headline_cache_state = "cold".to_string();
    let mut required_cache_states = BTreeSet::from([
        "cold".to_string(),
        "partial".to_string(),
        "warm".to_string(),
    ]);
    let mut min_pairs = 3_usize;
    let mut slow_speedup_target = 2.0_f64;
    let mut comparison_regression_limit_pct = 5.0_f64;
    let mut semantic_identities = Vec::new();
    let mut work_counters = Vec::new();
    let mut output = None;
    let mut samples = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let value = |args: &mut std::iter::Skip<std::env::Args>, name: &str| {
            args.next()
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match argument.as_str() {
            "--slow-node-class" => {
                slow_node_class = Some(value(&mut args, "--slow-node-class")?);
            }
            "--comparison-node-class" => {
                comparison_node_class = Some(value(&mut args, "--comparison-node-class")?);
            }
            "--headline-cache-state" => {
                headline_cache_state = value(&mut args, "--headline-cache-state")?;
            }
            "--required-cache-states" => {
                required_cache_states = value(&mut args, "--required-cache-states")?
                    .split(',')
                    .filter(|state| !state.is_empty())
                    .map(ToString::to_string)
                    .collect();
            }
            "--min-pairs" => {
                let raw = value(&mut args, "--min-pairs")?;
                min_pairs = raw
                    .parse()
                    .map_err(|error| format!("invalid --min-pairs {raw:?}: {error}"))?;
            }
            "--slow-speedup-target" => {
                let raw = value(&mut args, "--slow-speedup-target")?;
                slow_speedup_target = raw
                    .parse()
                    .map_err(|error| format!("invalid --slow-speedup-target {raw:?}: {error}"))?;
            }
            "--comparison-regression-limit-pct" => {
                let raw = value(&mut args, "--comparison-regression-limit-pct")?;
                comparison_regression_limit_pct = raw.parse().map_err(|error| {
                    format!("invalid --comparison-regression-limit-pct {raw:?}: {error}")
                })?;
            }
            "--semantic-identity" => semantic_identities.push(parse_named_pointer(
                &value(&mut args, "--semantic-identity")?,
                "--semantic-identity",
            )?),
            "--work-counter" => work_counters.push(parse_named_pointer(
                &value(&mut args, "--work-counter")?,
                "--work-counter",
            )?),
            "--output" => output = Some(PathBuf::from(value(&mut args, "--output")?)),
            _ if argument.starts_with('-') => {
                return Err(format!("unknown argument {argument:?}"));
            }
            _ => samples.push(PathBuf::from(argument)),
        }
    }
    if !(3..=5).contains(&min_pairs) {
        return Err("--min-pairs must be between 3 and 5".to_string());
    }
    if required_cache_states.is_empty() || !required_cache_states.contains(&headline_cache_state) {
        return Err("required cache states must include the headline cache state".to_string());
    }
    if semantic_identities.is_empty() {
        return Err("at least one --semantic-identity NAME=/json/pointer is required".to_string());
    }
    if work_counters.is_empty() {
        return Err("at least one --work-counter NAME=/json/pointer is required".to_string());
    }
    if samples.is_empty() {
        return Err("at least one sample JSON path is required".to_string());
    }
    let slow_node_class = slow_node_class.ok_or("missing --slow-node-class")?;
    let comparison_node_class = comparison_node_class.ok_or("missing --comparison-node-class")?;
    if slow_node_class == comparison_node_class {
        return Err("slow and comparison node classes must be distinct".to_string());
    }
    Ok(Args {
        slow_node_class,
        comparison_node_class,
        headline_cache_state,
        required_cache_states,
        min_pairs,
        slow_speedup_target,
        comparison_regression_limit_pct,
        semantic_identities,
        work_counters,
        output: output.ok_or("missing --output")?,
        samples,
    })
}

fn parse_named_pointer(raw: &str, option: &str) -> Result<(String, String), String> {
    let (name, pointer) = raw
        .split_once('=')
        .ok_or_else(|| format!("{option} must be NAME=/json/pointer"))?;
    if name.is_empty() || !pointer.starts_with('/') {
        return Err(format!("{option} must be NAME=/json/pointer"));
    }
    Ok((name.to_string(), pointer.to_string()))
}

fn run(args: Args) -> Result<bool, String> {
    let mut slots: BTreeMap<(String, String, String, u64), BTreeMap<String, Sample>> =
        BTreeMap::new();
    for path in &args.samples {
        let sample = read_sample(path, &args.work_counters)?;
        let key = (
            sample.node_class.clone(),
            sample.cache_state.clone(),
            sample.pair_id.clone(),
            sample.sample_index,
        );
        if slots
            .entry(key)
            .or_default()
            .insert(sample.variant.clone(), sample)
            .is_some()
        {
            return Err(format!(
                "duplicate variant in paired sample slot: {}",
                path.display()
            ));
        }
    }

    let mut groups: BTreeMap<(String, String), Vec<Pair>> = BTreeMap::new();
    for (key, mut variants) in slots {
        if variants.len() != 2
            || !variants.contains_key("baseline")
            || !variants.contains_key("candidate")
        {
            return Err(format!(
                "incomplete pair node={} cache={} pair={} index={}",
                key.0, key.1, key.2, key.3
            ));
        }
        let pair = Pair {
            baseline: variants.remove("baseline").expect("checked"),
            candidate: variants.remove("candidate").expect("checked"),
        };
        verify_pair(&pair, &args.semantic_identities)?;
        groups.entry((key.0, key.1)).or_default().push(pair);
    }

    let mut failures = Vec::new();
    let mut summaries = Vec::new();
    for node in [&args.slow_node_class, &args.comparison_node_class] {
        for cache in &args.required_cache_states {
            let key = (node.to_string(), cache.clone());
            let pairs = groups
                .get(&key)
                .ok_or_else(|| format!("no complete samples for node={node} cache={cache}"))?;
            if pairs.len() < args.min_pairs {
                return Err(format!(
                    "node={node} cache={cache} has {} pairs; at least {} are required",
                    pairs.len(),
                    args.min_pairs
                ));
            }
            let baseline_median = median(pairs.iter().map(|pair| pair.baseline.wall_ms).collect());
            let candidate_median =
                median(pairs.iter().map(|pair| pair.candidate.wall_ms).collect());
            let speedup = baseline_median / candidate_median;
            let mut counters = BTreeMap::new();
            for (counter, _) in &args.work_counters {
                let baseline = median(
                    pairs
                        .iter()
                        .map(|pair| pair.baseline.work[counter])
                        .collect(),
                );
                let candidate = median(
                    pairs
                        .iter()
                        .map(|pair| pair.candidate.work[counter])
                        .collect(),
                );
                if candidate > baseline {
                    failures.push(format!(
                        "causal counter {counter} increased on node={node} cache={cache}: {baseline} -> {candidate}"
                    ));
                }
                counters.insert(
                    counter.clone(),
                    serde_json::json!({
                        "baseline_median": baseline,
                        "candidate_median": candidate,
                        "reduction": baseline - candidate,
                    }),
                );
            }
            summaries.push(serde_json::json!({
                "node_class": node,
                "cache_state": cache,
                "pair_count": pairs.len(),
                "baseline_median_wall_ms": baseline_median,
                "candidate_median_wall_ms": candidate_median,
                "speedup": speedup,
                "work_counters": counters,
                "pairs": pairs.iter().map(pair_json).collect::<Vec<_>>(),
            }));
        }
    }

    let slow_headline = summary(
        &summaries,
        &args.slow_node_class,
        &args.headline_cache_state,
    )?;
    let slow_speedup = number(slow_headline, "/speedup")?;
    if slow_speedup < args.slow_speedup_target {
        failures.push(format!(
            "slow-node speedup {slow_speedup:.3}x is below {:.3}x",
            args.slow_speedup_target
        ));
    }
    let reduced_counter = slow_headline
        .pointer("/work_counters")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|counters| {
            counters
                .values()
                .any(|counter| number(counter, "/reduction").is_ok_and(|value| value > 0.0))
        });
    if !reduced_counter {
        failures.push("no declared causal-work counter decreased on the slow headline".to_string());
    }

    let comparison_headline = summary(
        &summaries,
        &args.comparison_node_class,
        &args.headline_cache_state,
    )?;
    let baseline = number(comparison_headline, "/baseline_median_wall_ms")?;
    let candidate = number(comparison_headline, "/candidate_median_wall_ms")?;
    let comparison_regression_pct = (candidate / baseline - 1.0) * 100.0;
    if comparison_regression_pct > args.comparison_regression_limit_pct {
        failures.push(format!(
            "comparison-node regression {comparison_regression_pct:.3}% exceeds {:.3}%",
            args.comparison_regression_limit_pct
        ));
    }

    let accepted = failures.is_empty();
    let report = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "command": "perf-accept",
        "accepted": accepted,
        "contract": {
            "slow_node_class": args.slow_node_class,
            "comparison_node_class": args.comparison_node_class,
            "headline_cache_state": args.headline_cache_state,
            "required_cache_states": args.required_cache_states,
            "minimum_complete_pairs": args.min_pairs,
            "slow_speedup_target": args.slow_speedup_target,
            "comparison_regression_limit_pct": args.comparison_regression_limit_pct,
            "semantic_identities": args.semantic_identities,
            "work_counters": args.work_counters,
        },
        "comparison_regression_pct": comparison_regression_pct,
        "failures": failures,
        "groups": summaries,
    });
    write_json_atomic(&args.output, &report)?;
    println!("{}", args.output.display());
    Ok(accepted)
}

fn read_sample(path: &Path, counters: &[(String, String)]) -> Result<Sample, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    if value
        .pointer("/schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(2)
        || value
            .pointer("/command")
            .and_then(serde_json::Value::as_str)
            != Some("perf-sample")
    {
        return Err(format!(
            "{} is not a perf-sample schema v2 document",
            path.display()
        ));
    }
    if value
        .pointer("/observations/exit/success")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Err(format!("{} measured a failed command", path.display()));
    }
    let mut work = BTreeMap::new();
    for (name, pointer) in counters {
        work.insert(
            name.clone(),
            number(&value, pointer)
                .map_err(|error| format!("{} work counter {name}: {error}", path.display()))?,
        );
    }
    let text = |pointer: &str| -> Result<String, String> {
        value
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string)
            .ok_or_else(|| format!("{} lacks string {pointer}", path.display()))
    };
    let (pair_id, variant, node_class, cache_state) = (
        text("/sample_identity/pair_id")?,
        text("/sample_identity/variant")?,
        text("/sample_identity/node_class")?,
        text("/sample_identity/cache_state")?,
    );
    let sample_index = value
        .pointer("/sample_identity/sample_index")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format!("{} lacks sample index", path.display()))?;
    let wall_ms = number(&value, "/observations/wall_ms")?;
    if wall_ms == 0.0 {
        return Err(format!("{} records a zero wall time", path.display()));
    }
    Ok(Sample {
        path: path.to_path_buf(),
        pair_id,
        variant,
        sample_index,
        node_class,
        cache_state,
        wall_ms,
        value,
        work,
    })
}

fn verify_pair(pair: &Pair, semantic: &[(String, String)]) -> Result<(), String> {
    for pointer in [
        "/sample_identity/cache_classes",
        "/sample_identity/partial_change",
        "/sample_identity/measured_command",
        "/sample_identity/dependency_resolution_sha256",
        "/sample_identity/rustc_vv",
        "/sample_identity/cargo_version",
        "/sample_identity/nextest_version",
        "/sample_identity/build_environment",
        "/sample_identity/runner_image",
    ] {
        equal_pointer(pair, pointer, pointer)?;
    }
    for (name, pointer) in semantic {
        equal_pointer(pair, pointer, name)?;
    }
    Ok(())
}

fn equal_pointer(pair: &Pair, pointer: &str, label: &str) -> Result<(), String> {
    let baseline = pair.baseline.value.pointer(pointer).ok_or_else(|| {
        format!(
            "{} lacks paired identity {label}",
            pair.baseline.path.display()
        )
    })?;
    let candidate = pair.candidate.value.pointer(pointer).ok_or_else(|| {
        format!(
            "{} lacks paired identity {label}",
            pair.candidate.path.display()
        )
    })?;
    if baseline != candidate {
        return Err(format!(
            "paired identity {label} differs between {} and {}",
            pair.baseline.path.display(),
            pair.candidate.path.display()
        ));
    }
    Ok(())
}

fn pair_json(pair: &Pair) -> serde_json::Value {
    serde_json::json!({
        "pair_id": pair.baseline.pair_id,
        "sample_index": pair.baseline.sample_index,
        "baseline": {
            "path": pair.baseline.path,
            "wall_ms": pair.baseline.wall_ms,
            "work": pair.baseline.work,
        },
        "candidate": {
            "path": pair.candidate.path,
            "wall_ms": pair.candidate.wall_ms,
            "work": pair.candidate.work,
        },
        "speedup": pair.baseline.wall_ms / pair.candidate.wall_ms,
    })
}

fn summary<'a>(
    summaries: &'a [serde_json::Value],
    node: &str,
    cache: &str,
) -> Result<&'a serde_json::Value, String> {
    summaries
        .iter()
        .find(|summary| summary["node_class"] == node && summary["cache_state"] == cache)
        .ok_or_else(|| format!("missing summary node={node} cache={cache}"))
}

fn number(value: &serde_json::Value, pointer: &str) -> Result<f64, String> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_f64)
        .filter(|number| number.is_finite() && !number.is_sign_negative())
        .ok_or_else(|| format!("{pointer} is not a finite non-negative number"))
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

fn write_json_atomic(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| format!("create {}: {error}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("create temporary acceptance report: {error}"))?;
    serde_json::to_writer_pretty(&mut temporary, value)
        .map_err(|error| format!("serialize acceptance report: {error}"))?;
    temporary
        .write_all(b"\n")
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|error| format!("flush acceptance report: {error}"))?;
    temporary.persist(path).map_err(|error| {
        format!(
            "publish acceptance report {}: {}",
            path.display(),
            error.error
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_handles_odd_and_even_complete_pair_counts() {
        assert_eq!(median(vec![9.0, 1.0, 5.0]), 5.0);
        assert_eq!(median(vec![9.0, 1.0, 5.0, 3.0]), 4.0);
    }

    #[test]
    fn missing_or_changed_pair_identity_fails_closed() {
        let value = serde_json::json!({
            "sample_identity": {
                "cache_classes": {"cargo": "cold"},
                "partial_change": null,
                "measured_command": ["make", "check"],
                "dependency_resolution_sha256": "a",
                "rustc_vv": "rustc",
                "cargo_version": "cargo",
                "nextest_version": "nextest",
                "build_environment": {},
                "runner_image": "image",
                "generated_tree": {"sha256": "product"}
            }
        });
        let sample = |variant: &str, value: serde_json::Value| Sample {
            path: PathBuf::from(format!("{variant}.json")),
            value,
            pair_id: "pair".to_string(),
            variant: variant.to_string(),
            sample_index: 1,
            node_class: "node".to_string(),
            cache_state: "cold".to_string(),
            wall_ms: 1.0,
            work: BTreeMap::new(),
        };
        let baseline = sample("baseline", value.clone());
        let mut changed = value;
        changed["sample_identity"]["runner_image"] = serde_json::json!("other");
        let candidate = sample("candidate", changed);
        let pair = Pair {
            baseline,
            candidate,
        };
        assert!(
            verify_pair(
                &pair,
                &[(
                    "generated".to_string(),
                    "/sample_identity/generated_tree/sha256".to_string()
                )]
            )
            .is_err()
        );
    }
}
