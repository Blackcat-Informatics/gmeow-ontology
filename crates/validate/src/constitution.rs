// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native constitution-as-code gate (#809, #939).
//!
//! Ports the full Python ``gmeow_tools.constitution`` checks to Rust:
//! enforcement coverage, principle/heading sync, cited artifact/symbol/target/CLI
//! existence, and supersession marker sync. The non-graph checks now live here
//! too, using pure text parsing helpers instead of Python introspection.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use gmeow_diagnostics::{Finding, Severity};
use oxigraph::model::{NamedNode, NamedNodeRef, NamedOrBlankNode, Term};
use oxigraph::store::Store;
use regex::Regex;

use crate::model::rdf;

/// The governance meta namespace (`constitution.META`).
const META: &str = "https://blackcatinformatics.ca/gmeow/meta#";
/// The enforcement classes; `Practice` is the honor-system kind.
const ENFORCEMENT_KINDS: &[&str] = &["Lint", "TestSuite", "Shape", "Gate", "Practice"];
/// `rdfs:Class` — a node ALSO typed as this is a class declaration, not an
/// enforcement instance (mirrors the Python `(node, RDF.type, RDFS.Class)` skip).
const RDFS_CLASS: NamedNodeRef<'static> =
    NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#Class");

static HEADING_RE: OnceLock<Regex> = OnceLock::new();
static PRINCIPLE_REF_RE: OnceLock<Regex> = OnceLock::new();
static MAKEFILE_TARGET_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_CLASS_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_DEF_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_ASSIGN_RE: OnceLock<Regex> = OnceLock::new();
static CLI_DECORATOR_RE: OnceLock<Regex> = OnceLock::new();
static CLI_NAME_RE: OnceLock<Regex> = OnceLock::new();

/// One principle reconstructed from the manifest graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principle {
    pub iri: String,
    pub number: i64,
    pub title: String,
    pub enforced_by: Vec<String>,
    pub superseded_in_part_by: Vec<i64>,
    pub extends: Vec<i64>,
}

/// One enforcement mechanism declared in the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enforcement {
    pub iri: String,
    pub kind: String,
    pub artifacts: Vec<String>,
    pub symbols: Vec<String>,
    pub make_targets: Vec<String>,
    pub cli_commands: Vec<String>,
}

impl Enforcement {
    /// Local name used in diagnostic messages (IRI without the meta prefix).
    pub fn local_name(&self) -> &str {
        self.iri.strip_prefix(META).unwrap_or(&self.iri)
    }
}

fn meta(local: &str) -> NamedNode {
    NamedNode::new(format!("{META}{local}")).expect("valid meta IRI")
}

/// Whether `node` is also declared an `rdfs:Class` (a class definition to skip).
fn is_rdfs_class(store: &Store, node: &NamedNode) -> bool {
    store
        .quads_for_pattern(
            Some(node.as_ref().into()),
            Some(rdf::TYPE),
            Some(RDFS_CLASS.into()),
            None,
        )
        .next()
        .transpose()
        .expect("rdfs:Class lookup: in-memory store query failed")
        .is_some()
}

/// Collect all string-object values for `subject predicate_local`.
fn strings_for(store: &Store, subject: &NamedNode, predicate_local: &str) -> Vec<String> {
    let predicate = meta(predicate_local);
    let mut values: Vec<String> = store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            None,
            None,
        )
        .flatten()
        .filter_map(|q| literal_string(&q.object))
        .collect();
    values.sort();
    values
}

/// Resolve `subject predicate ?obj` where `?obj` is a Principle node to its
/// heading number.
fn principle_numbers(
    store: &Store,
    subject: &NamedNode,
    predicate: &NamedNode,
    iri_to_number: &BTreeMap<String, i64>,
) -> Vec<i64> {
    let mut numbers: Vec<i64> = store
        .quads_for_pattern(
            Some(subject.as_ref().into()),
            Some(predicate.as_ref()),
            None,
            None,
        )
        .flatten()
        .filter_map(|q| match q.object {
            Term::NamedNode(n) => iri_to_number.get(n.as_str()).copied(),
            _ => None,
        })
        .collect();
    numbers.sort();
    numbers.dedup();
    numbers
}

/// Collect the declared enforcement instances keyed by full IRI.
pub fn collect_enforcements(store: &Store) -> BTreeMap<String, Enforcement> {
    let mut enforcements = BTreeMap::new();
    for kind in ENFORCEMENT_KINDS {
        let type_node = meta(kind);
        for quad in store
            .quads_for_pattern(None, Some(rdf::TYPE), Some(type_node.as_ref().into()), None)
            .flatten()
        {
            if let NamedOrBlankNode::NamedNode(node) = quad.subject {
                if is_rdfs_class(store, &node) {
                    continue;
                }
                let iri = node.as_str().to_string();
                let enforcement = Enforcement {
                    iri: iri.clone(),
                    kind: (*kind).to_string(),
                    artifacts: strings_for(store, &node, "artifact"),
                    symbols: strings_for(store, &node, "symbol"),
                    make_targets: strings_for(store, &node, "makeTarget"),
                    cli_commands: strings_for(store, &node, "cliCommand"),
                };
                enforcements.insert(iri, enforcement);
            }
        }
    }
    enforcements
}

/// Collect the principles (number, title, enforced_by edges, relations).
pub fn collect_principles(store: &Store) -> Vec<Principle> {
    let principle_type = meta("Principle");
    let number_p = meta("number");
    let title_p = meta("title");
    let enforced_p = meta("enforcedBy");
    let superseded_p = meta("supersededInPartBy");
    let extends_p = meta("extends");

    let mut principles: Vec<Principle> = Vec::new();
    let mut iri_to_number: BTreeMap<String, i64> = BTreeMap::new();

    for quad in store
        .quads_for_pattern(
            None,
            Some(rdf::TYPE),
            Some(principle_type.as_ref().into()),
            None,
        )
        .flatten()
    {
        let NamedOrBlankNode::NamedNode(node) = quad.subject else {
            continue;
        };
        let iri = node.as_str().to_string();
        let number = store
            .quads_for_pattern(
                Some(node.as_ref().into()),
                Some(number_p.as_ref()),
                None,
                None,
            )
            .flatten()
            .find_map(|q| literal_i64(&q.object))
            .unwrap_or(-1);
        let title = store
            .quads_for_pattern(
                Some(node.as_ref().into()),
                Some(title_p.as_ref()),
                None,
                None,
            )
            .flatten()
            .find_map(|q| literal_string(&q.object))
            .unwrap_or_default();
        let mut enforced_by: Vec<String> = store
            .quads_for_pattern(
                Some(node.as_ref().into()),
                Some(enforced_p.as_ref()),
                None,
                None,
            )
            .flatten()
            .filter_map(|q| match q.object {
                Term::NamedNode(n) => Some(n.as_str().to_string()),
                _ => None,
            })
            .collect();
        enforced_by.sort();

        iri_to_number.insert(iri.clone(), number);
        principles.push(Principle {
            iri,
            number,
            title,
            enforced_by,
            superseded_in_part_by: Vec::new(),
            extends: Vec::new(),
        });
    }

    for principle in &mut principles {
        let node = NamedNode::new(&principle.iri).expect("principle IRI is valid");
        principle.superseded_in_part_by =
            principle_numbers(store, &node, &superseded_p, &iri_to_number);
        principle.extends = principle_numbers(store, &node, &extends_p, &iri_to_number);
    }

    principles.sort_by_key(|p| p.number);
    principles
}

fn literal_i64(term: &Term) -> Option<i64> {
    match term {
        Term::Literal(lit) => lit.value().parse().ok(),
        _ => None,
    }
}

fn literal_string(term: &Term) -> Option<String> {
    match term {
        Term::Literal(lit) => Some(lit.value().to_string()),
        _ => None,
    }
}

/// ``## N. Title`` headings of CONSTITUTION.md, as ``{number: title}``.
pub fn constitution_headings(md_text: &str) -> BTreeMap<i64, String> {
    let re = HEADING_RE
        .get_or_init(|| Regex::new(r"(?m)^## (\d+)\. (.+?)\s*$").expect("valid heading regex"));
    re.captures_iter(md_text)
        .filter_map(|cap| {
            let number: i64 = cap[1].parse().ok()?;
            Some((number, cap[2].to_string()))
        })
        .collect()
}

/// Map each principle's heading number to the target numbers named in `marker`.
///
/// A relation is read from a bold marker line inside that principle's section;
/// the `from` number is the enclosing ``## N. Title`` heading, the targets are
/// every ``Principle N`` on the marker line.
pub fn markdown_relations(md_text: &str, marker: &str) -> BTreeMap<i64, BTreeSet<i64>> {
    let heading_re = HEADING_RE
        .get_or_init(|| Regex::new(r"(?m)^## (\d+)\. (.+?)\s*$").expect("valid heading regex"));
    let principle_ref = PRINCIPLE_REF_RE
        .get_or_init(|| Regex::new(r"Principle (\d+)").expect("valid principle ref regex"));

    let headings: Vec<(i64, usize, usize)> = heading_re
        .captures_iter(md_text)
        .filter_map(|cap| {
            let number: i64 = cap[1].parse().ok()?;
            let m = cap.get(0).expect("heading match");
            Some((number, m.start(), m.end()))
        })
        .collect();

    let mut relations: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for (idx, (number, _start, end)) in headings.iter().enumerate() {
        let section_end = headings.get(idx + 1).map(|h| h.1).unwrap_or(md_text.len());
        let section = &md_text[*end..section_end];
        for line in section.lines() {
            if line.trim_start().starts_with(marker) {
                let targets: BTreeSet<i64> = principle_ref
                    .find_iter(line)
                    .filter_map(|m| {
                        m.as_str()
                            .strip_prefix("Principle ")
                            .and_then(|n| n.parse().ok())
                    })
                    .collect();
                if !targets.is_empty() {
                    relations.entry(*number).or_default().extend(targets);
                }
            }
        }
    }
    relations
}

/// Makefile target names of the form ``name:``.
pub fn makefile_targets(makefile_text: &str) -> BTreeSet<String> {
    let re = MAKEFILE_TARGET_RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z][A-Za-z0-9_-]*):").expect("valid make target regex")
    });
    makefile_text
        .lines()
        .filter_map(|line| re.captures(line).map(|cap| cap[1].to_string()))
        .collect()
}

/// Top-level `def`, `class`, assignment, and annotated-assignment names.
pub fn python_top_level_names(py_text: &str) -> BTreeSet<String> {
    let class_re = PYTHON_CLASS_RE.get_or_init(|| {
        Regex::new(r"^class\s+([A-Za-z_][A-Za-z0-9_]*)\b").expect("valid class regex")
    });
    let def_re = PYTHON_DEF_RE.get_or_init(|| {
        Regex::new(r"^(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\b").expect("valid def regex")
    });
    let assign_re = PYTHON_ASSIGN_RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=]*|=)").expect("valid assign regex")
    });

    let keywords: BTreeSet<&str> = [
        "if", "elif", "else", "for", "while", "try", "except", "finally", "with", "return",
        "raise", "assert", "import", "from", "pass", "break", "continue", "global", "nonlocal",
        "del", "yield", "async", "await", "class", "def", "lambda", "as", "or", "and", "is",
        "True", "False", "None", "match", "case",
    ]
    .iter()
    .copied()
    .collect();

    let mut names = BTreeSet::new();
    for line in py_text.lines() {
        let stripped = line.trim_start();
        if let Some(cap) = class_re.captures(stripped) {
            names.insert(cap[1].to_string());
            continue;
        }
        if let Some(cap) = def_re.captures(stripped) {
            names.insert(cap[1].to_string());
            continue;
        }
        if let Some(cap) = assign_re.captures(stripped) {
            let name = &cap[1];
            if !keywords.contains(name) {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// Every command registered on the gmeow-dev Typer app.
pub fn cli_command_names(cli_dev_text: &str) -> BTreeSet<String> {
    let decorator_re = CLI_DECORATOR_RE.get_or_init(|| {
        Regex::new(r"^@app\.command\((.*)\)\s*(?:#.*)?$").expect("valid decorator regex")
    });
    let name_re = CLI_NAME_RE.get_or_init(|| {
        Regex::new(r#"name\s*=\s*(?:"([^"]*)"|'([^']*)'|([A-Za-z_][A-Za-z0-9_]*))"#)
            .expect("valid name regex")
    });
    let def_re = PYTHON_DEF_RE.get_or_init(|| {
        Regex::new(r"^(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\b").expect("valid def regex")
    });

    let mut names = BTreeSet::new();
    let mut pending: bool = false;

    for line in cli_dev_text.lines() {
        let stripped = line.trim();
        if let Some(cap) = decorator_re.captures(stripped) {
            let inner = cap.get(1).map_or("", |m| m.as_str()).trim();
            if inner.is_empty() {
                pending = true;
            } else if let Some(name_cap) = name_re.captures(inner) {
                let name = name_cap
                    .get(1)
                    .or_else(|| name_cap.get(2))
                    .or_else(|| name_cap.get(3))
                    .map(|m| m.as_str())
                    .unwrap_or("");
                if !name.is_empty() {
                    names.insert(name.to_string());
                }
                pending = false;
            } else {
                pending = true;
            }
            continue;
        }
        if pending {
            if stripped.is_empty() || stripped.starts_with('#') || stripped.starts_with('@') {
                continue;
            }
            if let Some(cap) = def_re.captures(stripped) {
                let fname = cap.get(1).expect("function name").as_str();
                names.insert(fname.replace('_', "-"));
            }
            pending = false;
        }
    }
    names
}

/// Run the enforcement-coverage check over the parsed manifest store.
pub fn check_enforcement_coverage(store: &Store) -> Vec<Finding> {
    let enforcements = collect_enforcements(store);
    let principles = collect_principles(store);

    let mut findings = Vec::new();
    let mut cited: BTreeSet<String> = BTreeSet::new();

    for principle in &principles {
        let mut any_known = false;
        let mut has_non_practice = false;
        for e in &principle.enforced_by {
            match enforcements.get(e) {
                Some(enforcement) => {
                    any_known = true;
                    has_non_practice |= enforcement.kind != "Practice";
                    cited.insert(e.clone());
                }
                None => findings.push(error(
                    "undeclared-enforcement",
                    format!(
                        "principle {} cites undeclared enforcement {e}",
                        principle.number
                    ),
                )),
            }
        }
        if !any_known {
            findings.push(error(
                "principle-unenforced",
                format!(
                    "principle {} ({}) has zero registered enforcement",
                    principle.number,
                    py_repr(&principle.title)
                ),
            ));
        } else if !has_non_practice {
            findings.push(
                Finding::new(
                    Severity::Warning,
                    "constitution.honor-system",
                    format!(
                        "principle {} ({}) is enforced only by review practice (honor system)",
                        principle.number,
                        py_repr(&principle.title)
                    ),
                )
                .with_tool("constitution"),
            );
        }
    }

    for orphan in enforcements.keys() {
        if !cited.contains(orphan) {
            findings.push(error(
                "orphaned-enforcement",
                format!("orphaned enforcement {orphan} maps to no principle — why does it exist?"),
            ));
        }
    }

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    findings
}

/// Manifest principles and CONSTITUTION.md headings must agree exactly.
pub fn check_principle_sync(
    principles: &[Principle],
    headings: &BTreeMap<i64, String>,
) -> Vec<Finding> {
    let declared: BTreeMap<i64, &Principle> = principles.iter().map(|p| (p.number, p)).collect();
    let mut findings = Vec::new();

    for number in headings.keys() {
        if !declared.contains_key(number) {
            findings.push(error(
                "missing-manifest-entry",
                format!(
                    "principle {number} ({}) has no manifest entry in governance/constitution.ttl",
                    py_repr(&headings[number])
                ),
            ));
        }
    }

    for number in declared.keys() {
        if !headings.contains_key(number) {
            let principle = declared[number];
            findings.push(error(
                "absent-from-constitution",
                format!(
                    "manifest declares principle {number} ({}) absent from CONSTITUTION.md",
                    py_repr(&principle.title)
                ),
            ));
        }
    }

    for number in declared.keys() {
        if let Some(md_title) = headings.get(number) {
            let principle = declared[number];
            if principle.title != *md_title {
                findings.push(error(
                    "title-drift",
                    format!(
                        "principle {number} title drift: manifest says {}, CONSTITUTION.md says {}",
                        py_repr(&principle.title),
                        py_repr(md_title)
                    ),
                ));
            }
        }
    }

    findings
}

/// Whether `symbol` is defined in any artifact (AST for `.py`, verbatim else).
fn symbol_defined(
    symbol: &str,
    artifacts: &[String],
    root: &Path,
    py_cache: &mut BTreeMap<String, BTreeSet<String>>,
    text_cache: &mut BTreeMap<String, String>,
) -> bool {
    for artifact in artifacts {
        let path = root.join(artifact);
        if !path.is_file() {
            continue;
        }
        let is_python = path.extension().map(|e| e == "py").unwrap_or(false);
        if is_python {
            let names = py_cache.entry(artifact.clone()).or_insert_with(|| {
                fs::read_to_string(&path)
                    .map(|text| python_top_level_names(&text))
                    .unwrap_or_default()
            });
            if names.contains(symbol) {
                return true;
            }
        } else {
            let text = text_cache
                .entry(artifact.clone())
                .or_insert_with(|| fs::read_to_string(&path).unwrap_or_default());
            if text.contains(symbol) {
                return true;
            }
        }
    }
    false
}

/// Every cited artifact / symbol / make target / CLI command must exist.
pub fn check_references(enforcements: &BTreeMap<String, Enforcement>, root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut py_cache: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut text_cache: BTreeMap<String, String> = BTreeMap::new();

    let makefile_text = fs::read_to_string(root.join("Makefile")).unwrap_or_default();
    let make_targets = makefile_targets(&makefile_text);

    let cli_dev_text =
        fs::read_to_string(root.join("src/gmeow_tools/cli_dev.py")).unwrap_or_default();
    let cli_commands = cli_command_names(&cli_dev_text);

    for enforcement in enforcements.values() {
        let name = enforcement.local_name();
        for artifact in &enforcement.artifacts {
            if !root.join(artifact).exists() {
                findings.push(error(
                    "stale-artifact",
                    format!(
                        "{name}: cited artifact {} does not exist",
                        py_repr(artifact)
                    ),
                ));
            }
        }
        for symbol in &enforcement.symbols {
            if !symbol_defined(
                symbol,
                &enforcement.artifacts,
                root,
                &mut py_cache,
                &mut text_cache,
            ) {
                findings.push(error(
                    "stale-symbol",
                    format!(
                        "{name}: symbol {} not found in any cited artifact",
                        py_repr(symbol)
                    ),
                ));
            }
        }
        for target in &enforcement.make_targets {
            if !make_targets.contains(target) {
                findings.push(error(
                    "stale-make-target",
                    format!("{name}: Makefile target {} does not exist", py_repr(target)),
                ));
            }
        }
        for command in &enforcement.cli_commands {
            if !cli_commands.contains(command) {
                findings.push(error(
                    "stale-cli-command",
                    format!(
                        "{name}: gmeow CLI command {} is not registered",
                        py_repr(command)
                    ),
                ));
            }
        }
    }

    findings
}

fn format_list(set: &BTreeSet<i64>) -> String {
    if set.is_empty() {
        "∅".to_string()
    } else {
        format!("{:?}", set.iter().collect::<Vec<_>>())
    }
}

/// Python-style ``repr`` for strings: single-quoted, escaping ``\`` and ``'``.
fn py_repr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

fn compare_relation(
    prop: &str,
    md_relations: &BTreeMap<i64, BTreeSet<i64>>,
    ttl_relations: &BTreeMap<i64, BTreeSet<i64>>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for number in md_relations
        .keys()
        .chain(ttl_relations.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let md = md_relations.get(&number).cloned().unwrap_or_default();
        let ttl = ttl_relations.get(&number).cloned().unwrap_or_default();
        if md != ttl {
            findings.push(error(
                "relation-drift",
                format!(
                    "principle {number} meta:{prop} drift: CONSTITUTION.md marker names {}, governance/constitution.ttl names {}",
                    format_list(&md),
                    format_list(&ttl)
                ),
            ));
        }
    }
    findings
}

/// The bold supersession markers in CONSTITUTION.md must match the TTL relations.
pub fn check_supersession(md_text: &str, principles: &[Principle]) -> Vec<Finding> {
    let mut findings = Vec::new();

    let md_superseded = markdown_relations(md_text, "**Superseded in part by Principle");
    let ttl_superseded: BTreeMap<i64, BTreeSet<i64>> = principles
        .iter()
        .map(|p| {
            (
                p.number,
                p.superseded_in_part_by
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>(),
            )
        })
        .filter(|(_, set)| !set.is_empty())
        .collect();
    findings.extend(compare_relation(
        "supersededInPartBy",
        &md_superseded,
        &ttl_superseded,
    ));

    let md_extends = markdown_relations(md_text, "**Extends Principle");
    let ttl_extends: BTreeMap<i64, BTreeSet<i64>> = principles
        .iter()
        .map(|p| (p.number, p.extends.iter().copied().collect::<BTreeSet<_>>()))
        .filter(|(_, set)| !set.is_empty())
        .collect();
    findings.extend(compare_relation("extends", &md_extends, &ttl_extends));

    findings
}

fn load_store_from_ttl(ttl: &str) -> Result<Store, String> {
    use oxigraph::io::{RdfFormat, RdfParser};
    let store = Store::new().map_err(|e| e.to_string())?;
    for triple in RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(ttl.as_bytes())
    {
        let triple = triple.map_err(|e| e.to_string())?;
        store.insert(&triple).map_err(|e| e.to_string())?;
    }
    Ok(store)
}

/// Run every constitution-as-code check into one granular finding list.
pub fn constitution_full_report(
    manifest_path: &Path,
    constitution_path: &Path,
    root: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    let ttl = match fs::read_to_string(manifest_path) {
        Ok(text) => text,
        Err(e) => {
            findings.push(error(
                "manifest-unreadable",
                format!("{}: cannot read: {e}", manifest_path.display()),
            ));
            return findings;
        }
    };

    let store = match load_store_from_ttl(&ttl) {
        Ok(store) => store,
        Err(e) => {
            findings.push(error(
                "manifest-parse",
                format!("{}: does not parse: {e}", manifest_path.display()),
            ));
            return findings;
        }
    };

    let md_text = match fs::read_to_string(constitution_path) {
        Ok(text) => text,
        Err(e) => {
            findings.push(error(
                "constitution-unreadable",
                format!("{}: cannot read: {e}", constitution_path.display()),
            ));
            return findings;
        }
    };

    let enforcements = collect_enforcements(&store);
    let principles = collect_principles(&store);
    let headings = constitution_headings(&md_text);

    findings.extend(check_enforcement_coverage(&store));
    findings.extend(check_principle_sync(&principles, &headings));
    findings.extend(check_references(&enforcements, root));
    findings.extend(check_supersession(&md_text, &principles));

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    findings
}

/// Build one `constitution.<code>` error finding.
fn error(code: &str, message: String) -> Finding {
    Finding::new(Severity::Error, format!("constitution.{code}"), message).with_tool("constitution")
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::model::Literal;

    // cargo-mutants (T9, #790) surfaced surviving mutants in `literal_i64` /
    // `literal_string` — the helpers had no direct coverage, so replacing their
    // body with `None`/`Some(0)`/deleting the match arm went undetected. These
    // tests pin both the literal path and the non-literal fallthrough, killing
    // that mutant cluster.
    #[test]
    fn literal_i64_parses_only_integer_literals() {
        assert_eq!(
            literal_i64(&Term::Literal(Literal::new_simple_literal("42"))),
            Some(42)
        );
        assert_eq!(
            literal_i64(&Term::Literal(Literal::new_simple_literal("-7"))),
            Some(-7)
        );
        assert_eq!(
            literal_i64(&Term::Literal(Literal::new_simple_literal("notanint"))),
            None
        );
        assert_eq!(
            literal_i64(&Term::NamedNode(NamedNode::new("https://e/x").unwrap())),
            None
        );
    }

    #[test]
    fn literal_string_extracts_only_literal_lexical_values() {
        assert_eq!(
            literal_string(&Term::Literal(Literal::new_simple_literal("hello"))),
            Some("hello".to_string())
        );
        assert_eq!(
            literal_string(&Term::NamedNode(NamedNode::new("https://e/x").unwrap())),
            None
        );
    }

    fn store_from(ttl: &str) -> Store {
        use oxigraph::io::{RdfFormat, RdfParser};
        let store = Store::new().unwrap();
        for triple in RdfParser::from_format(RdfFormat::Turtle)
            .lenient()
            .for_reader(ttl.as_bytes())
        {
            store.insert(&triple.unwrap()).unwrap();
        }
        store
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
        assert!(msgs
            .iter()
            .any(|m| m.contains("zero registered enforcement")));
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
        assert!(findings
            .iter()
            .any(|f| f.severity == Severity::Warning && f.message.contains("review practice")));
        assert!(findings
            .iter()
            .any(|f| f.code == "constitution.orphaned-enforcement"
                && f.message.contains("gate-orphan")));
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
    fn cli_command_names_extracts_decorated_commands() {
        let py = "\n@app.command()\ndef first_command():\n    pass\n\n@app.command(name=\"second-cmd\")\ndef second_command():\n    pass\n\n@app.command(name='third-cmd')\ndef third_command():\n    pass\n";
        let got = cli_command_names(py);
        assert!(got.contains("first-command"));
        assert!(got.contains("second-cmd"));
        assert!(got.contains("third-cmd"));
        assert!(!got.contains("second_command"));
    }

    #[test]
    fn cli_command_names_skips_blank_comment_decorator_before_def() {
        let py = "\n\
            @app.command()\n\
            \n\
            # a comment\n\
            @some_other_decorator\n\
            def spaced_command():\n\
                pass\n";
        let got = cli_command_names(py);
        assert!(got.contains("spaced-command"));
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
        assert!(findings
            .iter()
            .any(|f| f.message.contains("zero registered enforcement")));
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
        assert!(findings
            .iter()
            .any(|f| f.severity == Severity::Warning
                && f.message.contains("only by review practice")));
        assert!(!findings
            .iter()
            .any(|f| f.message.contains("zero registered enforcement")));
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
        assert!(findings
            .iter()
            .any(|f| f.message.contains("'no/such/file.py' does not exist")));
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
        assert!(findings
            .iter()
            .any(|f| f.message.contains("undeclared enforcement")));
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
        assert!(!findings
            .iter()
            .any(|f| f.message.contains("supersededInPartBy drift")));
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
        assert!(findings.iter().any(|f| f
            .message
            .contains("principle 2 meta:supersededInPartBy drift")));
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
        assert!(findings.iter().any(|f| f
            .message
            .contains("principle 2 meta:supersededInPartBy drift")));
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
}
