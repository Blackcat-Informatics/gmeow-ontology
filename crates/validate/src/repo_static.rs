// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust repository-static guards for source, workflow, and lane policy.
//!
//! These checks replace pytest files whose subject was the repository itself:
//! Python import surfaces, Makefile recipes, and GitHub workflow structure. The
//! gate is deliberately native Rust and fails hard in the existing `crate-check`
//! lane.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use gmeow_diagnostics::{Finding, Report, Severity};
use regex::Regex;
use serde_yaml::Value as Yaml;

const TOOL: &str = "repo-static";

const EXPORTER_MODULES: &[&str] = &["export.py"];
const EXPORTER_BANNED_IMPORTS: &[&str] = &["rdflib", "gmeow_rdf"];
const FORBIDDEN_LOADERS: &[&str] = &[
    "load_merged_graph",
    "shared_merged_graph",
    "load_mappings",
    "load_self_description",
    "load_tag_map",
];
const GTS_APP: &str = "gts_app";
const GTS_SUBCOMMANDS: &[&str] = &[
    "gts_info",
    "gts_verify",
    "gts_extract_key",
    "gts_to_nq",
    "gts_from_rdf",
    "gts_to_sqlite",
    "gts_to_duckdb",
];
const RDFLIB_KEEPERS: &[&str] = &["oracles/engine_crosscheck.py", "oracles/rl_agreement.py"];

const LANE_MAKE_TARGETS: &[&str] = &[
    "full-release",
    "maint-classic-cross-check",
    "maint-reason-hermit",
    "maint-explain",
    "maint-verify-docker",
    "maint-reasoning-cases",
    "maint-statements-docker-check",
    "maint-pull-images",
];
const LANE_TARGETS_THAT_MUST_HIT: &[&str] = &[
    "maint-classic-cross-check",
    "maint-reason-hermit",
    "maint-verify-docker",
    "maint-reasoning-cases",
    "maint-statements-docker-check",
    "maint-pull-images",
];
const LANE_SCRIPTS: &[&str] = &[
    "reasoning_cases.py",
    "statements_docker_check.py",
    "slme_cross_check.py",
    "pull-images.sh",
];
const DOCKER_PATTERNS: &[&str] = &[
    r"\bdocker\s+(?:run|pull|build|image|compose)\b",
    r"--mode\s+docker",
    r"obolibrary/robot",
    r"stain/jena",
    r"\bjava\s+-(?:jar|cp)\b",
    r"\b(?:javac|gradlew?)\b",
];

static DOCKER_REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    DOCKER_PATTERNS
        .iter()
        .map(|pattern| Regex::new(&format!("(?i){pattern}")).expect("static regex"))
        .collect()
});

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoStaticReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl RepoStaticReport {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    fn error(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
    }
}

pub fn check_repo_static(root: &Path) -> RepoStaticReport {
    let mut report = RepoStaticReport::default();
    check_narrow_waist(root, &mut report);
    check_lane_purity(root, &mut report);
    check_no_rdflib_in_runtime(root, &mut report);
    check_projection_compute_purity(root, &mut report);
    report
}

pub fn to_diagnostics_report(report: &RepoStaticReport) -> Report {
    let mut out = Report::new(TOOL);
    for message in &report.errors {
        out.add_finding(
            Finding::new(Severity::Error, "repo-static.violation", message.clone()).with_tool(TOOL),
        );
    }
    for message in &report.warnings {
        out.add_finding(
            Finding::new(
                Severity::Warning,
                "repo-static.observation",
                message.clone(),
            )
            .with_tool(TOOL),
        );
    }
    out
}

fn check_narrow_waist(root: &Path, report: &mut RepoStaticReport) {
    let src = root.join("src").join("gmeow_tools");
    for module in EXPORTER_MODULES {
        let rel = format!("src/gmeow_tools/{module}");
        let Some(text) = read_required(root, &rel, report) else {
            continue;
        };
        let code = strip_python_non_code(&text);
        let imported = python_imported_top_modules(&code);
        let offending = EXPORTER_BANNED_IMPORTS
            .iter()
            .filter(|name| imported.contains(**name))
            .copied()
            .collect::<Vec<_>>();
        if !offending.is_empty() {
            report.error(format!("{rel} imports {}", offending.join(", ")));
        }

        let referenced = python_identifiers(&code);
        let loaders = FORBIDDEN_LOADERS
            .iter()
            .filter(|name| referenced.contains(**name))
            .copied()
            .collect::<Vec<_>>();
        if !loaders.is_empty() {
            report.error(format!(
                "{rel} references canonical-source loader(s): {}",
                loaders.join(", ")
            ));
        }
    }

    let cli_path = src.join("cli.py");
    let rel = "src/gmeow_tools/cli.py";
    let text = match fs::read_to_string(&cli_path) {
        Ok(text) => text,
        Err(err) => {
            report.error(format!("{rel}: cannot read: {err}"));
            return;
        }
    };
    let code = strip_python_non_code(&text);
    let assigned = python_assigned_names(&code);
    if assigned.contains(GTS_APP) {
        report.error(format!(
            "{rel} still assigns the retired {GTS_APP:?} Typer app"
        ));
    }

    let defined = python_defined_functions(&code);
    let legacy = GTS_SUBCOMMANDS
        .iter()
        .filter(|name| defined.contains(**name))
        .copied()
        .collect::<Vec<_>>();
    if !legacy.is_empty() {
        report.error(format!(
            "{rel} still defines legacy GTS subcommand function(s): {}",
            legacy.join(", ")
        ));
    }

    if has_gts_app_command_decorator(&code) {
        report.error(format!(
            "{rel} still registers a command on the retired {GTS_APP:?} app"
        ));
    }
}

fn check_no_rdflib_in_runtime(root: &Path, report: &mut RepoStaticReport) {
    let src = root.join("src").join("gmeow_tools");
    let allowed = RDFLIB_KEEPERS
        .iter()
        .copied()
        .collect::<BTreeSet<&'static str>>();
    let mut actual = BTreeSet::new();
    for path in python_files(&src, report) {
        let rel = slash_path(path.strip_prefix(&src).unwrap_or(&path));
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        let code = strip_python_non_code(&text);
        let imports = python_imported_top_modules(&code);
        if imports.contains("rdflib") {
            actual.insert(rel);
        }
    }

    let allowed_owned = allowed
        .iter()
        .map(|s| (*s).to_owned())
        .collect::<BTreeSet<_>>();
    let offenders = actual
        .difference(&allowed_owned)
        .cloned()
        .collect::<Vec<_>>();
    if !offenders.is_empty() {
        report.error(format!(
            "first-party modules must use gmeow_rdf.compat.rdflib, not upstream rdflib: {}",
            offenders.join(", ")
        ));
    }

    let actual_refs = actual.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if actual_refs != allowed {
        let expected = allowed.iter().copied().collect::<Vec<_>>().join(", ");
        let found = actual.iter().cloned().collect::<Vec<_>>().join(", ");
        report.error(format!(
            "rdflib keeper allow-list is stale: expected {{{expected}}}, found {{{found}}}"
        ));
    }
}

/// The SHACL-AF / RDFQuery **computational** (derivation) constructs. Computation is
/// authored in the `logic:` canon and PROJECTED to these surfaces under `generated/`
/// (Principle 17, `design/LOGIC-SHACL-AF.md`); a hand-authored occurrence in the authored
/// sources is a forbidden second source of truth unless it carries a `logic:formalizes`
/// back-reference to its `logic:` origin (the Hybrid-placement convention). Note this is
/// the *derivation* vocabulary only — the SHACL *constraint* vocabulary
/// (`sh:sparql` / `sh:SPARQLTarget` / `sh:SPARQLConstraint`) is validation, not
/// computation, and is deliberately NOT listed here.
const PROJECTION_COMPUTE_TOKENS: &[&str] = &[
    "sh:rule",
    "sh:SPARQLRule",
    "sh:TripleRule",
    "sh:JSRule",
    "sh:js",
    "sh:values",
];

/// The Hybrid-placement back-reference that legalizes a hand-authored projection-surface
/// construct: it names the `logic:` source the construct is the projection of.
const PROJECTION_FORMALIZES_BACKREF: &str = "logic:formalizes";

/// Computation-surfaces-are-projections purity gate (Principles 17/4/12,
/// `design/LOGIC-SHACL-AF.md` / `design/LOGIC-RDFQUERY.md`): scan the authored RDF sources
/// (`slices/` + `dsl/`, `.ttl` only — NOT `generated/`, NOT prose `.md` docs) for
/// computational SHACL-AF / RDFQuery vocabulary. Any file carrying such a construct must
/// also carry a `logic:formalizes` back-reference to its `logic:` source; otherwise it is a
/// hand-authored computational projection — a forbidden second source of truth — and the
/// gate fails.
fn check_projection_compute_purity(root: &Path, report: &mut RepoStaticReport) {
    let mut ttl_files = Vec::new();
    for sub in ["slices", "dsl"] {
        let dir = root.join(sub);
        if dir.is_dir() {
            collect_ttl_files(&dir, report, &mut ttl_files);
        }
    }
    ttl_files.sort();
    for path in ttl_files {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                report.error(format!("{}: cannot read: {err}", path.display()));
                continue;
            }
        };
        let hits: Vec<&str> = PROJECTION_COMPUTE_TOKENS
            .iter()
            .copied()
            .filter(|tok| text.contains(*tok))
            .collect();
        if hits.is_empty() {
            continue;
        }
        if !text.contains(PROJECTION_FORMALIZES_BACKREF) {
            let rel = slash_path(path.strip_prefix(root).unwrap_or(&path));
            report.error(format!(
                "{rel} hand-authors computational SHACL-AF/RDFQuery vocabulary ({}) without a \
                 `{PROJECTION_FORMALIZES_BACKREF}` back-reference: computation is authored in the \
                 logic: canon and PROJECTED to these surfaces under generated/ (Principle 17), \
                 never hand-authored as a second source of truth (design/LOGIC-SHACL-AF.md)",
                hits.join(", ")
            ));
        }
    }
}

fn collect_ttl_files(dir: &Path, report: &mut RepoStaticReport, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            report.error(format!("{}: cannot read directory: {err}", dir.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                report.error(format!(
                    "{}: cannot read directory entry: {err}",
                    dir.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            collect_ttl_files(&path, report, out);
        } else if path.extension().is_some_and(|ext| ext == "ttl") {
            out.push(path);
        }
    }
}

fn check_lane_purity(root: &Path, report: &mut RepoStaticReport) {
    check_required_ci_jobs(root, report);
    check_classic_cross_check_workflow(root, report);
    check_makefile_lane_purity(root, report);
}

fn check_required_ci_jobs(root: &Path, report: &mut RepoStaticReport) {
    let rel = ".github/workflows/ci.yml";
    let Some(text) = read_required(root, rel, report) else {
        return;
    };
    let Some(ci) = parse_yaml(rel, &text, report) else {
        return;
    };
    let Some(jobs) = yaml_get(&ci, "jobs").and_then(Yaml::as_mapping) else {
        report.error(format!("{rel}: missing jobs mapping"));
        return;
    };
    let Some(quality) = yaml_map_get(jobs, "quality") else {
        report.error(format!("{rel}: missing jobs.quality"));
        return;
    };
    let needs = match yaml_get(quality, "needs") {
        Some(Yaml::Sequence(items)) => items
            .iter()
            .filter_map(Yaml::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        Some(Yaml::String(one)) => vec![one.clone()],
        _ => {
            report.error(format!("{rel}: jobs.quality.needs must list gate jobs"));
            return;
        }
    };
    if needs.is_empty() {
        report.error(format!("{rel}: jobs.quality.needs must not be empty"));
        return;
    }

    let mut required_jobs = needs.clone();
    required_jobs.push("quality".to_owned());
    for job_name in &required_jobs {
        let Some(job) = yaml_map_get(jobs, job_name) else {
            report.error(format!("{rel}: quality needs missing job {job_name:?}"));
            continue;
        };
        let blob = recursive_yaml_text(job);
        let hits = forbidden_hits(&blob);
        if !hits.is_empty() {
            report.error(format!(
                "required CI job {job_name:?} reaches Docker/Java: {}",
                hits.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        let lowered = blob.to_lowercase();
        for token in [
            "make maint-classic-cross-check",
            "--reasoner hermit",
            "--reasoner elk",
        ] {
            if lowered.contains(token) {
                report.error(format!(
                    "required CI job {job_name:?} invokes the oracle lane: {token:?}"
                ));
            }
        }
    }

    if needs.iter().any(|need| need == "classic-cross-check") {
        report.error(format!(
            "{rel}: classic-cross-check must not appear in quality.needs"
        ));
    }
}

fn check_classic_cross_check_workflow(root: &Path, report: &mut RepoStaticReport) {
    let rel = ".github/workflows/classic-cross-check.yml";
    let Some(text) = read_required(root, rel, report) else {
        return;
    };
    let Some(workflow) = parse_yaml(rel, &text, report) else {
        return;
    };
    let Some(triggers) = yaml_get(&workflow, "on").and_then(Yaml::as_mapping) else {
        report.error(format!("{rel}: unexpected `on:` shape"));
        return;
    };
    let trigger_keys = triggers
        .keys()
        .filter_map(Yaml::as_str)
        .collect::<BTreeSet<_>>();
    if trigger_keys.contains("push") {
        report.error("classic-cross-check oracle lane must not run on push");
    }
    let allowed = BTreeSet::from(["schedule", "workflow_dispatch", "pull_request"]);
    let unexpected = trigger_keys
        .difference(&allowed)
        .copied()
        .collect::<Vec<_>>();
    if !unexpected.is_empty() {
        report.error(format!(
            "classic-cross-check has unexpected trigger(s): {}",
            unexpected.join(", ")
        ));
    }

    if trigger_keys.contains("pull_request") {
        let Some(jobs) = yaml_get(&workflow, "jobs").and_then(Yaml::as_mapping) else {
            report.error(format!("{rel}: missing jobs mapping"));
            return;
        };
        for (name, job) in jobs {
            let job_name = name.as_str().unwrap_or("<non-string>");
            let gated = yaml_get(job, "if")
                .map(recursive_yaml_text)
                .is_some_and(|text| text.contains("label"));
            if !gated {
                report.error(format!(
                    "classic-cross-check pull_request job {job_name:?} must gate on a label"
                ));
            }
        }
    }

    if forbidden_hits(&text).is_empty() && !text.contains("make maint-classic-cross-check") {
        report.error("classic-cross-check workflow no longer invokes the Docker/Java lane");
    }
}

fn check_makefile_lane_purity(root: &Path, report: &mut RepoStaticReport) {
    let rel = "Makefile";
    let Some(text) = read_required(root, rel, report) else {
        return;
    };
    let recipes = makefile_recipes(&text);
    let lane_targets = LANE_MAKE_TARGETS.iter().copied().collect::<BTreeSet<_>>();

    for (target, lines) in &recipes {
        if lane_targets.contains(target.as_str()) {
            continue;
        }
        let hits = forbidden_hits(&lines.join("\n"));
        if !hits.is_empty() {
            report.error(format!(
                "non-lane Makefile target {target:?} reaches Docker/Java: {}",
                hits.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
        let invoked = makefile_invoked_targets(lines);
        let intruders = invoked
            .iter()
            .filter(|target| lane_targets.contains(target.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !intruders.is_empty() {
            report.error(format!(
                "non-lane Makefile target {target:?} invokes oracle-lane target(s): {}",
                intruders.join(", ")
            ));
        }
    }

    if !recipes.contains_key("check") {
        report.error("Makefile: the `check` target vanished");
        return;
    }

    for target in LANE_TARGETS_THAT_MUST_HIT {
        let Some(lines) = recipes.get(*target) else {
            report.error(format!("expected lane target {target:?} is gone"));
            continue;
        };
        if forbidden_hits(&lines.join("\n")).is_empty() {
            report.error(format!(
                "lane target {target:?} no longer carries a Docker/Java token"
            ));
        }
    }
}

fn read_required(root: &Path, rel: &str, report: &mut RepoStaticReport) -> Option<String> {
    let path = root.join(rel);
    match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(err) => {
            report.error(format!("{rel}: cannot read: {err}"));
            None
        }
    }
}

fn parse_yaml(rel: &str, text: &str, report: &mut RepoStaticReport) -> Option<Yaml> {
    match serde_yaml::from_str::<Yaml>(text) {
        Ok(value) => Some(value),
        Err(err) => {
            report.error(format!("{rel}: cannot parse YAML: {err}"));
            None
        }
    }
}

fn yaml_get<'a>(value: &'a Yaml, key: &str) -> Option<&'a Yaml> {
    value
        .as_mapping()
        .and_then(|mapping| yaml_map_get(mapping, key))
}

fn yaml_map_get<'a>(mapping: &'a serde_yaml::Mapping, key: &str) -> Option<&'a Yaml> {
    mapping.get(Yaml::String(key.to_owned()))
}

fn recursive_yaml_text(value: &Yaml) -> String {
    match value {
        Yaml::String(s) => s.clone(),
        Yaml::Number(n) => n.to_string(),
        Yaml::Bool(b) => b.to_string(),
        Yaml::Sequence(items) => items
            .iter()
            .map(recursive_yaml_text)
            .collect::<Vec<_>>()
            .join("\n"),
        Yaml::Mapping(map) => map
            .iter()
            .flat_map(|(k, v)| [recursive_yaml_text(k), recursive_yaml_text(v)])
            .collect::<Vec<_>>()
            .join("\n"),
        Yaml::Null | Yaml::Tagged(_) => String::new(),
    }
}

fn forbidden_hits(text: &str) -> BTreeSet<String> {
    let mut hits = BTreeSet::new();
    for (pattern, re) in DOCKER_PATTERNS.iter().zip(DOCKER_REGEXES.iter()) {
        if re.is_match(text) {
            hits.insert((*pattern).to_owned());
        }
    }
    let lowered = text.to_lowercase();
    hits.extend(
        LANE_SCRIPTS
            .iter()
            .filter(|script| lowered.contains(&script.to_lowercase()))
            .map(|script| (*script).to_owned()),
    );
    hits
}

fn makefile_invoked_targets(lines: &[String]) -> BTreeSet<String> {
    let token_re = Regex::new(r"[A-Za-z][A-Za-z0-9_-]*").expect("static regex");
    lines
        .iter()
        .filter(|line| line.contains("$(MAKE)") || line.contains("${MAKE}"))
        .flat_map(|line| token_re.find_iter(line).map(|m| m.as_str().to_owned()))
        .collect()
}

fn makefile_recipes(text: &str) -> BTreeMap<String, Vec<String>> {
    let mut recipes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if line.starts_with('\t') {
            if let Some(target) = &current {
                recipes
                    .entry(target.clone())
                    .or_default()
                    .push(line.to_owned());
            }
            continue;
        }
        if let Some(target) = makefile_target_name(line) {
            recipes.entry(target.clone()).or_default();
            current = Some(target);
        } else if !line.trim().is_empty() && !line.starts_with('#') {
            current = None;
        }
    }
    recipes
}

fn makefile_target_name(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let first = *bytes.first()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut end = 1;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'-')
    {
        end += 1;
    }
    let mut cursor = end;
    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b':') || bytes.get(cursor + 1) == Some(&b'=') {
        return None;
    }
    Some(line[..end].to_owned())
}

fn python_imported_top_modules(code: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let import_re = Regex::new(r"(?m)^\s*import\s+([^\n]+)").expect("static regex");
    for caps in import_re.captures_iter(code) {
        for part in caps[1].split(',') {
            let name = part
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .split('.')
                .next()
                .unwrap_or_default();
            if is_identifier(name) {
                out.insert(name.to_owned());
            }
        }
    }
    let from_re =
        Regex::new(r"(?m)^\s*from\s+([A-Za-z_][A-Za-z0-9_\.]*)\s+import\b").expect("static regex");
    for caps in from_re.captures_iter(code) {
        if let Some(name) = caps[1].split('.').next().filter(|name| is_identifier(name)) {
            out.insert(name.to_owned());
        }
    }
    out
}

fn python_identifiers(code: &str) -> BTreeSet<String> {
    let re = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").expect("static regex");
    re.find_iter(code).map(|m| m.as_str().to_owned()).collect()
}

fn python_assigned_names(code: &str) -> BTreeSet<String> {
    let re = Regex::new(r"(?m)^\s*([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=!+\-*/%^&<>\n]+)?=\s*[^=\n]")
        .expect("static regex");
    re.captures_iter(code)
        .map(|cap| cap[1].to_owned())
        .collect()
}

fn python_defined_functions(code: &str) -> BTreeSet<String> {
    let re = Regex::new(r"(?m)^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")
        .expect("static regex");
    re.captures_iter(code)
        .map(|cap| cap[1].to_owned())
        .collect()
}

fn has_gts_app_command_decorator(code: &str) -> bool {
    Regex::new(r"(?m)^\s*@\s*gts_app\s*\.\s*command\b")
        .expect("static regex")
        .is_match(code)
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn strip_python_non_code(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0;
    while idx < chars.len() {
        let ch = chars[idx];
        if ch == '#' {
            while idx < chars.len() && chars[idx] != '\n' {
                idx += 1;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            idx = skip_python_string(&chars, idx, &mut out);
            continue;
        }
        out.push(ch);
        idx += 1;
    }
    out
}

fn skip_python_string(chars: &[char], start: usize, out: &mut String) -> usize {
    let quote = chars[start];
    let triple = chars.get(start + 1) == Some(&quote) && chars.get(start + 2) == Some(&quote);
    let mut idx = start + if triple { 3 } else { 1 };
    while idx < chars.len() {
        if chars[idx] == '\n' {
            out.push('\n');
            idx += 1;
            continue;
        }
        if chars[idx] == '\\' {
            idx = (idx + 2).min(chars.len());
            continue;
        }
        if triple {
            if chars[idx] == quote
                && chars.get(idx + 1) == Some(&quote)
                && chars.get(idx + 2) == Some(&quote)
            {
                return idx + 3;
            }
            idx += 1;
        } else if chars[idx] == quote {
            return idx + 1;
        } else {
            idx += 1;
        }
    }
    idx
}

fn python_files(src: &Path, report: &mut RepoStaticReport) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_python_files(src, report, &mut out);
    out.sort();
    out
}

fn collect_python_files(dir: &Path, report: &mut RepoStaticReport, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            report.error(format!("{}: cannot read directory: {err}", dir.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                report.error(format!(
                    "{}: cannot read directory entry: {err}",
                    dir.display()
                ));
                continue;
            }
        };
        let path = entry.path();
        if path.is_dir() {
            collect_python_files(&path, report, out);
        } else if path.extension().is_some_and(|ext| ext == "py") {
            out.push(path);
        }
    }
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn write_minimal_repo(root: &Path) {
        write(
            &root.join("src/gmeow_tools/export.py"),
            "from __future__ import annotations\n\nVALUE = 1\n",
        );
        write(
            &root.join("src/gmeow_tools/cli.py"),
            "from __future__ import annotations\n\ndef main() -> None:\n    pass\n",
        );
        write(
            &root.join("src/gmeow_tools/oracles/engine_crosscheck.py"),
            "import rdflib\n",
        );
        write(
            &root.join("src/gmeow_tools/oracles/rl_agreement.py"),
            "from rdflib import Graph\n",
        );
        write(
            &root.join(".github/workflows/ci.yml"),
            "on:\n  push:\n  pull_request:\njobs:\n  lint:\n    steps:\n      - run: make lint\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
        );
        write(
            &root.join(".github/workflows/classic-cross-check.yml"),
            "on:\n  schedule:\n    - cron: '0 0 * * *'\n  workflow_dispatch:\n  pull_request:\njobs:\n  oracle:\n    if: contains(github.event.pull_request.labels.*.name, 'classic-cross-check')\n    steps:\n      - run: make maint-classic-cross-check\n",
        );
        write(
            &root.join("Makefile"),
            "check:\n\t$(MAKE) lint\nlint:\n\ttrue\nmaint-classic-cross-check:\n\tdocker run obolibrary/robot\nmaint-reason-hermit:\n\tdocker run obolibrary/robot\nmaint-explain:\n\ttrue\nmaint-verify-docker:\n\tdocker run obolibrary/robot\nmaint-reasoning-cases:\n\tdocker run obolibrary/robot\nmaint-statements-docker-check:\n\tdocker run stain/jena\nmaint-pull-images:\n\tdocker pull obolibrary/robot\n",
        );
    }

    #[test]
    fn python_scanner_ignores_comments_and_strings() {
        let code = strip_python_non_code(
            "import os\n# import rdflib\nTEXT = \"import gmeow_rdf\"\n'''load_merged_graph'''\n",
        );
        let imports = python_imported_top_modules(&code);
        let names = python_identifiers(&code);
        assert!(imports.contains("os"));
        assert!(!imports.contains("rdflib"));
        assert!(!names.contains("load_merged_graph"));
    }

    #[test]
    fn python_assignment_scanner_ignores_comparisons() {
        let names = python_assigned_names(
            "if gts_app == other:\n    pass\nif gts_app != other:\n    pass\nother = 1\n",
        );
        assert!(!names.contains(GTS_APP));
        assert!(names.contains("other"));
    }

    #[test]
    fn python_assignment_scanner_keeps_annotated_assignment() {
        let names = python_assigned_names("gts_app: typer.Typer = typer.Typer()\n");
        assert!(names.contains(GTS_APP));
    }

    #[test]
    fn projection_compute_purity_flags_unbacked_construct_and_passes_a_backed_one() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let module = root.join("slices/core/demo/module.ttl");

        // A hand-authored SHACL-AF derivation rule with NO logic:formalizes back-reference
        // is a forbidden second source of truth → the gate must fail.
        write(
            &module,
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:S a sh:NodeShape ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
        );
        let mut report = RepoStaticReport::default();
        check_projection_compute_purity(root, &mut report);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("computational SHACL-AF") && e.contains("module.ttl")),
            "a hand-authored sh:SPARQLRule without logic:formalizes must be flagged; got {:?}",
            report.errors
        );

        // The SAME construct WITH a logic:formalizes back-reference is the legal Hybrid
        // placement (it names its logic: source) → the gate must pass.
        write(
            &module,
            "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             ex:S a sh:NodeShape ;\n    \
                 logic:formalizes ex:someLogicRule ;\n    \
                 sh:rule [ a sh:SPARQLRule ; \
                 sh:construct \"\"\"CONSTRUCT { ?x ex:p ?y } WHERE { ?x ex:q ?y }\"\"\" ] .\n",
        );
        let mut backed = RepoStaticReport::default();
        check_projection_compute_purity(root, &mut backed);
        assert!(
            backed.errors.is_empty(),
            "a logic:formalizes-backed construct must pass the purity gate; got {:?}",
            backed.errors
        );
    }

    #[test]
    fn minimal_repo_passes() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        let report = check_repo_static(temp.path());
        assert!(report.ok(), "{:?}", report.errors);
    }

    #[test]
    fn narrow_waist_flags_exporter_imports_and_loader_names() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("src/gmeow_tools/export.py"),
            "import rdflib\n\ndef f(graph_mod):\n    return graph_mod.load_merged_graph()\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report.errors.iter().any(|e| e.contains("imports rdflib")));
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("load_merged_graph")));
    }

    #[test]
    fn public_cli_legacy_gts_surface_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("src/gmeow_tools/cli.py"),
            "gts_app = object()\n\n@gts_app.command()\ndef gts_info():\n    pass\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report.errors.iter().any(|e| e.contains("gts_app")));
        assert!(report.errors.iter().any(|e| e.contains("gts_info")));
    }

    #[test]
    fn rdflib_runtime_offender_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("src/gmeow_tools/bad.py"),
            "import rdflib\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("bad.py") && e.contains("upstream rdflib")));
    }

    #[test]
    fn required_ci_docker_token_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join(".github/workflows/ci.yml"),
            "on:\n  pull_request:\njobs:\n  lint:\n    steps:\n      - run: docker run obolibrary/robot\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("required CI job") && e.contains("docker")));
    }

    #[test]
    fn required_ci_job_container_token_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join(".github/workflows/ci.yml"),
            "on:\n  pull_request:\njobs:\n  lint:\n    container: obolibrary/robot\n    steps:\n      - run: make lint\n  quality:\n    needs: [lint]\n    steps:\n      - run: echo all-good\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("required CI job") && e.contains("obolibrary/robot")));
    }

    #[test]
    fn classic_cross_check_pull_request_gate_must_use_job_if() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join(".github/workflows/classic-cross-check.yml"),
            "on:\n  pull_request:\njobs:\n  oracle:\n    steps:\n      - name: Print labels\n        run: make maint-classic-cross-check\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("pull_request job") && e.contains("label")));
    }

    #[test]
    fn make_check_oracle_target_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("Makefile"),
            "check:\n\t$(MAKE) maint-classic-cross-check\nmaint-classic-cross-check:\n\tdocker run obolibrary/robot\nmaint-reason-hermit:\n\tdocker run obolibrary/robot\nmaint-explain:\n\ttrue\nmaint-verify-docker:\n\tdocker run obolibrary/robot\nmaint-reasoning-cases:\n\tdocker run obolibrary/robot\nmaint-statements-docker-check:\n\tdocker run stain/jena\nmaint-pull-images:\n\tdocker pull obolibrary/robot\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("target \"check\"") && e.contains("maint-classic-cross-check")));
    }

    #[test]
    fn non_check_make_oracle_target_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("Makefile"),
            "check:\n\t$(MAKE) lint\nci:\n\t$(MAKE) maint-verify-docker\nlint:\n\ttrue\nmaint-classic-cross-check:\n\tdocker run obolibrary/robot\nmaint-reason-hermit:\n\tdocker run obolibrary/robot\nmaint-explain:\n\ttrue\nmaint-verify-docker:\n\tdocker run obolibrary/robot\nmaint-reasoning-cases:\n\tdocker run obolibrary/robot\nmaint-statements-docker-check:\n\tdocker run stain/jena\nmaint-pull-images:\n\tdocker pull obolibrary/robot\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("target \"ci\"") && e.contains("maint-verify-docker")));
    }

    #[test]
    fn brace_make_oracle_target_fails() {
        let temp = tempfile::tempdir().unwrap();
        write_minimal_repo(temp.path());
        write(
            &temp.path().join("Makefile"),
            "check:\n\t$(MAKE) lint\nci:\n\t${MAKE} maint-verify-docker\nlint:\n\ttrue\nmaint-classic-cross-check:\n\tdocker run obolibrary/robot\nmaint-reason-hermit:\n\tdocker run obolibrary/robot\nmaint-explain:\n\ttrue\nmaint-verify-docker:\n\tdocker run obolibrary/robot\nmaint-reasoning-cases:\n\tdocker run obolibrary/robot\nmaint-statements-docker-check:\n\tdocker run stain/jena\nmaint-pull-images:\n\tdocker pull obolibrary/robot\n",
        );
        let report = check_repo_static(temp.path());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("target \"ci\"") && e.contains("maint-verify-docker")));
    }

    #[test]
    fn live_repo_static_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("validate crate should live under crates/");
        let report = check_repo_static(root);
        assert!(report.ok(), "{:?}", report.errors);
    }
}
