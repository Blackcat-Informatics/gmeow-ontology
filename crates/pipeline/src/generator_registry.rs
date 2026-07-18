// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The self-describing generator registry.
//!
//! The Python generator framework was retired when the DAG-driven pipeline
//! executor landed, but the Makefile still needs a machine-readable list
//! of retained-product paths. This module provides a static,
//! human-maintained registry that maps each logical generator to its canonical
//! source directories/files and output paths.
//!
//! The registry is intentionally simple: it does not drive the pipeline (the
//! dogfooded `gmeow:Pipeline` DAG in `slices/core/pipeline/module.ttl` does).
//! The git-ignored `generated/` projection is materialized by `make sync`;
//! `make commit` stages only the retained products
//! ([`RETAINED_PRODUCT_PATHS`]: `catalog-v001.xml`,
//! `packages/python/gmeow_models/`) without importing deleted Python modules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_errors::ResultExt;

use crate::cache::content_digest;

/// Static metadata for one logical artifact generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratorInfo {
    /// Logical generator name, e.g. `mappings` or `statements`.
    pub name: &'static str,
    /// Canonical source paths (repo-relative directories or files).
    pub sources: &'static [&'static str],
    /// Output paths (repo-relative directories or files) — mostly the
    /// git-ignored `generated/` projection; see [`RETAINED_PRODUCT_PATHS`]
    /// for the paths that remain tracked.
    pub outputs: &'static [&'static str],
    /// Names of other generators whose outputs are inputs to this one.
    pub dependencies: &'static [&'static str],
}

/// The generated artifact registry.
///
/// Sources and outputs are repo-relative. The ordering is alphabetical by name
/// for deterministic output.
pub const GENERATORS: &[GeneratorInfo] = &[
    GeneratorInfo {
        name: "apache",
        sources: &["dsl/mappings/"],
        outputs: &["generated/apache/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "bench",
        sources: &["bench/"],
        outputs: &["generated/bench/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "catalog",
        sources: &["slices/", "metadata/"],
        outputs: &["generated/catalog/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "docs",
        sources: &["slices/", "docs/"],
        outputs: &["ontology-docs/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "evals",
        sources: &["evals/"],
        outputs: &["generated/evals/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "export",
        sources: &["generated/dist/gmeow.gts"],
        outputs: &[
            "generated/context.jsonld",
            "generated/rdf-loss-matrix.json",
            "generated/transcode-loss-matrix.json",
            "generated/transcode-matrix.json",
        ],
        dependencies: &["gts"],
    },
    GeneratorInfo {
        name: "frame-shapes",
        sources: &["slices/", "shapes/"],
        outputs: &["generated/shapes/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "gts",
        sources: &[
            "slices/",
            "dsl/mappings/",
            "dsl/statements/",
            "imports/",
            "metadata/",
            "shapes/",
            "queries/",
            "evals/",
            "bench/",
        ],
        outputs: &["generated/dist/gmeow.gts"],
        dependencies: &[
            "mappings",
            "statements",
            "logic-compile",
            "reason",
            "validate",
        ],
    },
    GeneratorInfo {
        name: "lpg",
        sources: &["slices/"],
        outputs: &["generated/lpg/"],
        dependencies: &["gts"],
    },
    GeneratorInfo {
        name: "logic-compile",
        sources: &["slices/", "imports/"],
        outputs: &[
            "generated/logic/",
            "generated/owl/",
            "generated/datalog/",
            "generated/n3/",
            "generated/cl/",
            "generated/shacl-af/",
            "generated/foundation/",
        ],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "mappings",
        sources: &["dsl/mappings/", "slices/", "imports/"],
        outputs: &[
            "generated/mappings/",
            "generated/projections/",
            "generated/queries/projections/",
        ],
        dependencies: &["logic-compile"],
    },
    GeneratorInfo {
        name: "matrix",
        sources: &["slices/"],
        outputs: &["generated/module-status.md"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "metadata",
        sources: &["metadata/", "dsl/mappings/"],
        outputs: &["generated/metadata/"],
        dependencies: &["mappings"],
    },
    GeneratorInfo {
        name: "okf",
        sources: &["dsl/mappings/"],
        outputs: &["generated/okf/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "parquet",
        sources: &["generated/dist/gmeow.gts"],
        outputs: &["generated/parquet/"],
        dependencies: &["gts"],
    },
    GeneratorInfo {
        name: "profiles",
        sources: &["slices/"],
        outputs: &["generated/profiles/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "reason",
        sources: &["slices/", "imports/"],
        outputs: &["generated/diagnostics/"],
        dependencies: &["logic-compile"],
    },
    GeneratorInfo {
        name: "references",
        sources: &["metadata/references.ttl"],
        outputs: &["generated/references/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "research-objects",
        sources: &["slices/"],
        outputs: &["generated/research-objects/"],
        dependencies: &["mappings"],
    },
    GeneratorInfo {
        name: "schemas",
        sources: &["slices/", "shapes/"],
        outputs: &["generated/schemas/"],
        dependencies: &["gts"],
    },
    GeneratorInfo {
        name: "statements",
        sources: &["dsl/statements/", "slices/", "imports/"],
        outputs: &["generated/statements/"],
        dependencies: &[],
    },
    GeneratorInfo {
        name: "validate",
        sources: &["slices/", "shapes/", "imports/"],
        outputs: &["generated/diagnostics/"],
        dependencies: &["logic-compile"],
    },
];

/// The retained product paths derived from the registry — the outputs `make commit`
/// still stages after `generated/` became an ignored local projection.
///
/// The `generated/` carrier/fanout tree is NO LONGER a committed input: it is a
/// git-ignored local/release product materialized by `make sync` and reconstructed
/// from `generated/dist/gmeow.gts`, so it is absent here. What remains tracked are the
/// two retained products that ride outside that projection: the root OASIS catalog and
/// the generated Python model package. External documentation (`ontology-docs/` and
/// `dist/gmeow-docs/`) is intentionally ephemeral and therefore absent.
pub const RETAINED_PRODUCT_PATHS: &[&str] = &["catalog-v001.xml", "packages/python/gmeow_models/"];

/// Return every generator name, sorted.
pub fn generator_names() -> Vec<&'static str> {
    GENERATORS.iter().map(|g| g.name).collect()
}

/// Look up a generator by name.
pub fn generator_by_name(name: &str) -> Option<&'static GeneratorInfo> {
    GENERATORS.iter().find(|g| g.name == name)
}

/// Return the repo-relative output paths for every generator, deduplicated and
/// sorted. Directories are returned with a trailing slash so callers can
/// distinguish them from files.
pub fn all_output_paths() -> Vec<&'static str> {
    let mut paths: Vec<_> = GENERATORS.iter().flat_map(|g| g.outputs).copied().collect();
    paths.sort_unstable();
    paths.dedup();
    paths
}

/// Return [`RETAINED_PRODUCT_PATHS`] as a list. This is the tracked-product path set
/// `make commit` stages; `generated/` is not among them (it is an ignored projection).
pub fn retained_product_paths() -> Vec<&'static str> {
    RETAINED_PRODUCT_PATHS.to_vec()
}

/// Compute a stable SHA-256 source hash for a generator from its declared
/// sources.
///
/// For files the hash folds the repo-relative path and the file bytes. For
/// directories it folds every regular file under the directory, sorted by
/// repo-relative path. Missing sources are skipped (a generator whose sources
/// are not present in the working tree still returns a hash of the paths that
/// do exist, or an empty-string hash if none exist).
pub fn source_hash(root: &Path, generator: &GeneratorInfo) -> gmeow_errors::Result<String> {
    let mut entries: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for src in generator.sources {
        let path = root.join(src);
        if path.is_dir() {
            collect_dir(&path, root, &mut entries)?;
        } else if path.is_file() {
            let bytes =
                std::fs::read(&path).with_ctx(|| format!("cannot read {}", path.display()))?;
            entries.push((PathBuf::from(src), bytes));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let rel_strings: Vec<String> = entries
        .iter()
        .map(|(rel, _)| rel.to_string_lossy().into_owned())
        .collect();
    let mut fields: Vec<&[u8]> = Vec::with_capacity(entries.len() * 2);
    for ((_rel, bytes), rel_str) in entries.iter().zip(&rel_strings) {
        fields.push(rel_str.as_bytes());
        fields.push(bytes.as_slice());
    }
    Ok(content_digest(&fields))
}

/// Metadata record emitted by `gmeow-dev sync --mode update --outputs generated --metadata`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratorMetadata {
    pub name: String,
    pub sources: Vec<String>,
    pub outputs: Vec<String>,
    pub dependencies: Vec<String>,
    pub source_hash: String,
}

/// Build a metadata record for every registered generator.
pub fn generator_metadata(root: &Path) -> gmeow_errors::Result<Vec<GeneratorMetadata>> {
    let mut out = Vec::with_capacity(GENERATORS.len());
    for generator in GENERATORS {
        let hash = source_hash(root, generator)?;
        out.push(GeneratorMetadata {
            name: generator.name.to_string(),
            sources: generator.sources.iter().map(|s| s.to_string()).collect(),
            outputs: generator.outputs.iter().map(|s| s.to_string()).collect(),
            dependencies: generator
                .dependencies
                .iter()
                .map(|s| s.to_string())
                .collect(),
            source_hash: hash,
        });
    }
    Ok(out)
}

/// Recursively collect (repo-relative path, bytes) pairs for every regular file
/// under `dir`, skipping hidden directories and the pipeline cache.
fn collect_dir(
    dir: &Path,
    root: &Path,
    out: &mut Vec<(PathBuf, Vec<u8>)>,
) -> gmeow_errors::Result<()> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .with_ctx(|| format!("cannot read directory {}", current.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip hidden directories and the gitignored pipeline cache.
            if path.is_dir() {
                if file_name.starts_with('.') || file_name == "__pycache__" {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            let bytes =
                std::fs::read(&path).with_ctx(|| format!("cannot read {}", path.display()))?;
            out.push((rel, bytes));
        }
    }
    Ok(())
}

/// Topological order of generators respecting [`GeneratorInfo::dependencies`].
///
/// Returns names in dependency-first order (a dependency appears before its
/// dependents). Cycles are broken deterministically by falling back to
/// alphabetical order and returning the cycle members as a diagnostic.
pub fn generator_order() -> (Vec<&'static str>, Option<Vec<&'static str>>) {
    let mut deps: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for generator in GENERATORS {
        deps.insert(generator.name, generator.dependencies.to_vec());
        dependents.entry(generator.name).or_default();
    }
    for (&name, ds) in &deps {
        for &d in ds {
            dependents.entry(d).or_default().push(name);
        }
    }

    let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
    for generator in GENERATORS {
        in_degree.insert(generator.name, 0);
    }
    for (&name, ds) in &deps {
        *in_degree.get_mut(name).unwrap() += ds.len();
    }

    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(n, _)| *n)
        .collect();
    queue.sort_unstable();

    let mut result = Vec::with_capacity(GENERATORS.len());
    while let Some(name) = queue.pop() {
        result.push(name);
        if let Some(children) = dependents.get(name) {
            for &child in children {
                if let Some(d) = in_degree.get_mut(child) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(child);
                        queue.sort_unstable();
                    }
                }
            }
        }
    }

    if result.len() == GENERATORS.len() {
        return (result, None);
    }

    let remaining: Vec<&str> = in_degree
        .keys()
        .filter(|n| !result.contains(n))
        .copied()
        .collect();
    let mut fallback: Vec<&str> = GENERATORS.iter().map(|g| g.name).collect();
    fallback.sort_unstable();
    (fallback, Some(remaining))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_expected_generators() {
        let names: Vec<_> = generator_names();
        assert!(names.contains(&"mappings"));
        assert!(names.contains(&"statements"));
        assert!(names.contains(&"gts"));
        assert!(names.contains(&"docs"));
    }

    #[test]
    fn mappings_outputs_match_issue_example() {
        let generator = generator_by_name("mappings").unwrap();
        assert!(generator.outputs.contains(&"generated/mappings/"));
        assert!(generator.outputs.contains(&"generated/projections/"));
        assert!(
            generator
                .outputs
                .contains(&"generated/queries/projections/")
        );
    }

    #[test]
    fn retained_product_paths_exclude_the_ignored_generated_projection() {
        // `generated/` is a git-ignored local projection materialized by `make sync`,
        // never staged by `make commit`; only the two retained products remain tracked.
        assert_eq!(
            retained_product_paths(),
            vec!["catalog-v001.xml", "packages/python/gmeow_models/"]
        );
    }

    #[test]
    fn generator_order_puts_dependencies_first() {
        let (order, cycle) = generator_order();
        assert!(
            cycle.is_none(),
            "generator dependency graph has a cycle: {cycle:?}"
        );
        let pos = |name| order.iter().position(|n| *n == name).unwrap();
        assert!(pos("logic-compile") < pos("mappings"));
        assert!(pos("mappings") < pos("metadata"));
    }
}
