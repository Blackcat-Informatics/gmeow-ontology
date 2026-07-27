// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The documented-dependency gate: a crate's hand-written `# Direct dependencies`
//! doc list must SET-EQUAL its real direct dependency set.
//!
//! `gmeow-mcp` and `gmeow-mcp-dev` both carry, in their crate-level `//!` docs, a
//! bullet list naming every direct dependency with the reason it is there. That list
//! is the crate's boundary contract — it is how a reader learns that the consumer MCP
//! engine may not grow an edge to the build executor, and it is the thing a reviewer
//! reads instead of the manifest. Prose that no machine checks rots: three successive
//! dependency audits on this surface disagreed with the manifest. This test makes the
//! list FALSIFIABLE.
//!
//! # What is compared
//!
//! * **The documented set** — every `//! * \`name\` — reason` bullet inside the
//!   `# Direct dependencies` section of the crate's `src/lib.rs` doc comment. The
//!   section runs from its heading to the next `//! #` heading (or the end of the doc
//!   block).
//! * **The real set** — `cargo tree -p <crate> --depth 1 --prefix none --format '{p}'`
//!   restricted to `-e normal` edges, minus the crate itself. Depth 1 is the DIRECT
//!   set, not the transitive closure: the doc list is a statement about the edges this
//!   crate's manifest declares, not about everything that ends up in the binary.
//!
//! # How `[dev-dependencies]` is handled, and why
//!
//! `cargo tree --depth 1` prints dev-dependencies too (under a `[dev-dependencies]`
//! header — which `--prefix none` suppresses, so the two kinds are indistinguishable
//! in that output). This gate therefore does NOT read the unfiltered output: it asks
//! cargo for the `normal` edge set and the `dev` edge set SEPARATELY.
//!
//! The `# Direct dependencies` list is gated against the **normal** edges only,
//! because that list documents what the SHIPPED crate links — the boundary a consumer
//! inherits. A dev-dependency is test scaffolding: it never reaches a consumer, and
//! listing it in the shipped-boundary section would misstate the boundary. To keep
//! that a checked claim rather than an assumption, the gate ALSO asserts negatively
//! that no dev-dependency name appears in the documented list, so a dev-only crate can
//! never be smuggled in as if it were a shipped edge. Both directions of every
//! comparison are reported by name, so a drift says precisely what to add or remove.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace root: this test's crate is `crates/mcp`, so the root is two levels up.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/mcp has a workspace root two levels up")
        .to_path_buf()
}

/// Every dependency name cargo reports at depth 1 for `package` over the given edge
/// kind (`normal` / `dev` / `build`), with the package itself removed.
///
/// `--format '{p}'` prints `<name> <version> [<source>]`; the name is the first
/// whitespace-delimited token. `--prefix none` drops the tree drawing so every line is
/// exactly one package. The first line is always the queried package itself.
fn direct_deps(package: &str, edge_kind: &str) -> BTreeSet<String> {
    let output = Command::new(env!("CARGO"))
        .current_dir(workspace_root())
        .args([
            "tree",
            "-p",
            package,
            "--depth",
            "1",
            "--prefix",
            "none",
            "--format",
            "{p}",
            "-e",
            edge_kind,
        ])
        .output()
        .unwrap_or_else(|e| panic!("`cargo tree -p {package} -e {edge_kind}` failed to run: {e}"));
    assert!(
        output.status.success(),
        "`cargo tree -p {package} -e {edge_kind}` exited {:?}:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout)
        .unwrap_or_else(|e| panic!("`cargo tree -p {package}` emitted non-UTF-8: {e}"));

    let mut names: BTreeSet<String> = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // A `[dev-dependencies]` / `[build-dependencies]` section header, should a
        // future cargo emit one under `--prefix none`: it names no package.
        if line.starts_with('[') {
            continue;
        }
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        if name == package {
            continue;
        }
        names.insert(name.to_owned());
    }
    names
}

/// The dependency names bulleted under the `# Direct dependencies` heading of
/// `lib_rs`'s crate-level doc comment.
///
/// The section is delimited by its own heading and the next `//! #` heading (or the end
/// of the `//!` block), so a later doc section that happens to contain a backticked
/// crate name cannot leak into the set.
fn documented_deps(lib_rs: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(lib_rs)
        .unwrap_or_else(|e| panic!("read {}: {e}", lib_rs.display()));

    let mut in_section = false;
    let mut names: BTreeSet<String> = BTreeSet::new();
    for raw in text.lines() {
        let Some(doc) = raw.strip_prefix("//!") else {
            // The crate-level doc block is contiguous and comes first; once it ends,
            // so does any chance of finding the section.
            if in_section {
                break;
            }
            continue;
        };
        let doc = doc.trim();
        if let Some(heading) = doc.strip_prefix("# ") {
            if heading.trim() == "Direct dependencies" {
                in_section = true;
            } else if in_section {
                break;
            }
            continue;
        }
        if !in_section {
            continue;
        }
        // A bullet is `* \`name\` — reason`; continuation lines are indented prose and
        // carry no leading `*`, so they are skipped without matching.
        let Some(bullet) = doc.strip_prefix("* ") else {
            continue;
        };
        let bullet = bullet.trim_start();
        let Some(rest) = bullet.strip_prefix('`') else {
            panic!(
                "{}: `# Direct dependencies` bullet does not start with a backticked \
                 crate name: {bullet:?}",
                lib_rs.display()
            );
        };
        let Some(end) = rest.find('`') else {
            panic!(
                "{}: `# Direct dependencies` bullet has an unterminated backtick: {bullet:?}",
                lib_rs.display()
            );
        };
        names.insert(rest[..end].to_owned());
    }

    assert!(
        in_section,
        "{} carries no `# Direct dependencies` section in its crate-level docs — the \
         documented-dependency gate has nothing to check against, which is itself the \
         drift it exists to catch",
        lib_rs.display()
    );
    names
}

/// Render one direction of a set difference as a stable, copy-pasteable bullet list.
fn render(items: &BTreeSet<String>) -> String {
    if items.is_empty() {
        return "(none)".to_owned();
    }
    items
        .iter()
        .map(|name| format!("\n    - {name}"))
        .collect::<String>()
}

/// Assert `package`'s documented list set-equals its real direct (normal-edge)
/// dependency set, and that it names no dev-dependency.
fn check(package: &str, crate_dir: &str) {
    let lib_rs = workspace_root()
        .join("crates")
        .join(crate_dir)
        .join("src")
        .join("lib.rs");
    let documented = documented_deps(&lib_rs);
    let actual = direct_deps(package, "normal");
    let dev = direct_deps(package, "dev");

    let undocumented: BTreeSet<String> = actual.difference(&documented).cloned().collect();
    let stale: BTreeSet<String> = documented.difference(&actual).cloned().collect();

    assert!(
        undocumented.is_empty() && stale.is_empty(),
        "{package}: the `# Direct dependencies` list in {} has drifted from \
         `cargo tree -p {package} --depth 1 -e normal`.\n\
         \n  MISSING from the doc list (add a bullet with its justification):{}\
         \n  STALE in the doc list (no such direct dependency — remove the bullet):{}\n",
        lib_rs.display(),
        render(&undocumented),
        render(&stale),
    );

    let dev_in_docs: BTreeSet<String> = documented.intersection(&dev).cloned().collect();
    assert!(
        dev_in_docs.is_empty(),
        "{package}: the `# Direct dependencies` list in {} names dev-dependencies. That \
         section documents what the SHIPPED crate links; a `[dev-dependencies]` entry \
         never reaches a consumer and must be justified in the manifest instead.\n\
         \n  DEV-ONLY entries wrongly listed as shipped dependencies:{}\n",
        lib_rs.display(),
        render(&dev_in_docs),
    );
}

/// The consumer MCP engine's documented boundary must equal its real one.
#[test]
fn gmeow_mcp_documented_dependencies_match_cargo_tree() {
    check("gmeow-mcp", "mcp");
}

/// The repo-reading dev-tool crate's documented boundary must equal its real one. It
/// is gated from THIS crate's test suite rather than duplicated into
/// `crates/mcp-dev/tests/`: the parser and the differ are one implementation, and a
/// second copy would be exactly the drift-prone duplication this gate exists to stop.
#[test]
fn gmeow_mcp_dev_documented_dependencies_match_cargo_tree() {
    check("gmeow-mcp-dev", "mcp-dev");
}
