// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The two normative documents the medium axis is answerable to, checked against the
//! DATA rather than against a copy of themselves.
//!
//! `docs/gts-narrow-waist.md` and `docs/PIPELINE_SPINE.md` are not commentary: CLAUDE.md
//! makes the spine canonical for anything touching `crates/pipeline` or `generated/`, and
//! the narrow waist is the rule set every producer is written against. Stale doctrine in
//! a normative document is a defect of the same kind as stale code — with the extra edge
//! that nothing else in the repository will ever contradict it.
//!
//! So the §6.2 clause below does not compare prose to prose. It DERIVES the expected path
//! set from the authored `gmeow:FanoutExtraction` rows in `slices/core/pipeline/module.ttl`
//! — the same rows the superset gate bijection-checks against the shipped segment header —
//! and requires §6.2 to name every one. A dictionary added with its fanout row but without
//! its documentation line reds here, which is the only arrangement under which the
//! document stays true by construction rather than by anyone remembering.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {relative}: {err}"))
}

/// The `generated/medium/*.zdict` paths the AUTHORED fanout rows declare.
///
/// Read out of `slices/core/pipeline/module.ttl`, never listed here: the point of the
/// assertion is that the document tracks the declaration, and a hardcoded list in the
/// test would be a third copy for the two to drift from.
fn declared_zdict_paths() -> BTreeSet<String> {
    let module = read("slices/core/pipeline/module.ttl");
    let mut out = BTreeSet::new();
    for line in module.lines() {
        let Some(rest) = line.split("gmeow:extractsPath \"").nth(1) else {
            continue;
        };
        let Some(path) = rest.split('"').next() else {
            continue;
        };
        if path.starts_with("generated/medium/") && path.ends_with(".zdict") {
            out.insert(path.to_string());
        }
    }
    out
}

/// The body of one `###`/`##` section of a Markdown document, bounded by the next heading
/// at the same or a shallower level.
fn section(document: &str, heading: &str) -> String {
    let start = document
        .find(heading)
        .unwrap_or_else(|| panic!("the document carries no heading {heading:?}"));
    let depth = heading.chars().take_while(|c| *c == '#').count();
    let body = &document[start + heading.len()..];
    let end = body
        .lines()
        .scan(0usize, |offset, line| {
            let at = *offset;
            *offset += line.len() + 1;
            Some((at, line))
        })
        .find(|(_, line)| {
            let hashes = line.chars().take_while(|c| *c == '#').count();
            hashes > 0 && hashes <= depth && line.chars().nth(hashes) == Some(' ')
        })
        .map_or(body.len(), |(at, _)| at);
    body[..end].to_string()
}

#[test]
fn the_narrow_waist_document_states_the_two_axes_and_carries_no_retired_python_doctrine() {
    let document = read("docs/gts-narrow-waist.md");

    // (1) The two-axes section exists, under exactly the heading it is referred to by.
    const HEADING: &str = "## Two axes: dialect and medium";
    assert!(
        document.contains(HEADING),
        "docs/gts-narrow-waist.md must carry the {HEADING:?} section"
    );
    let two_axes = section(&document, HEADING);

    // (2) Both axes are modelled as logic:Correspondences told apart by their
    //     PRESERVATION JUDGMENT, and all three judgments are named.
    for required in [
        "logic:Correspondence",
        "gmeow:mediumCorrespondence",
        "logic:SectionRetraction",
        "logic:ExactPreservation",
        "gmeow:gmnCorrNormalToGmn",
        "gmeow:gmnCorrGmnToCompacted",
        "logic:ValidationOnly",
        "dec ∘ enc = id",
    ] {
        assert!(
            two_axes.contains(required),
            "the two-axes section must name {required:?} — the axes are told apart BY the \
             preservation judgment, so a section that omits one of them has not made the \
             distinction it claims to make"
        );
    }

    // (3) The load-bearing sentence, stated plainly.
    const NORMAL_FORM: &str = "**GMN-0 is the existing normal form; media are encodings.**";
    assert!(
        document.contains(NORMAL_FORM),
        "docs/gts-narrow-waist.md must state {NORMAL_FORM:?} — a reader who takes the medium \
         axis for a new dialect will look for a translation where there is only an encoding"
    );

    // (4) GMN-2 is honestly placed: not preservation-preserving at all.
    assert!(
        two_axes.contains("new claim about older claims"),
        "the two-axes section must say that GMN-2 compaction is a NEW claim about older \
         claims rather than a lossy view of them"
    );

    // (5) The declared reader-capability set of the SHIPPED bundle.
    assert!(
        document.contains("reader-capability set"),
        "docs/gts-narrow-waist.md must record the bundle's declared reader-capability set — \
         Principle 13 makes the reader contract a property of the deliverable"
    );
    for required in [
        "gmeow:requiresReaderCapability",
        "gmeow:mediumProfileDistL12",
        "gmeow:mediumProfileStoreL12",
        "gmeow:mediumProfileBaselineL12",
        "`{zstd-dictionary, zstd-rsyncable}`",
        "non-baseline",
    ] {
        assert!(
            document.contains(required),
            "the reader-capability record must name {required:?}"
        );
    }

    // (6) Rule 6's refinement: the dictionary is a PARAMETER of the one mandated codec,
    //     the assignment is total, and an unresolvable dictionary is a hard failure.
    for required in [
        "one entry per\n   `(codec, dictionary)` pair",
        "The mandate is on the chain, not on the arity",
        "is a hard failure",
        "total",
    ] {
        assert!(
            document.contains(required),
            "Rule 6 must state {required:?}"
        );
    }

    // (7) NO retired Python doctrine survives. Rules 1-3 described `gmeow_tools` shims and
    //     rdflib imports that no longer exist in the tree at all.
    let stale: Vec<(usize, &str)> = document
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("gmeow_tools") || line.contains("rdflib"))
        .map(|(index, line)| (index + 1, line))
        .collect();
    assert!(
        stale.is_empty(),
        "docs/gts-narrow-waist.md still describes the retired Python exporter surface at \
         {stale:?} — a normative document that describes code the repository deleted is a \
         defect, not documentation debt"
    );
}

#[test]
fn the_spine_documents_every_shipped_dictionary_as_a_fanout_row() {
    let document = read("docs/PIPELINE_SPINE.md");
    const HEADING: &str = "### 6.2 Worked instance — the medium dictionaries";
    assert!(
        document.contains(HEADING),
        "docs/PIPELINE_SPINE.md must carry the {HEADING:?} worked instance, following §6.1's \
         template"
    );
    let body = section(&document, HEADING);

    // Both producers, named.
    for producer in ["stage-archive-blobs", "stage-medium-dictionaries"] {
        assert!(
            body.contains(producer),
            "§6.2 must name the producer {producer:?}"
        );
    }

    // The header-dict fanout family and its non-`.zdict` sibling.
    for required in [
        "header-dict",
        "\"dct\"",
        "generated/medium/dictionary-effect.ttl",
        "rdf-fanout",
    ] {
        assert!(body.contains(required), "§6.2 must name {required:?}");
    }

    // The in-band bytes as the canonical form, carried exactly once.
    assert!(
        body.contains("carried exactly once"),
        "§6.2 must state that the dictionary bytes are carried exactly once — routing them \
         through the archive as well would be the one way the law breaks while every other \
         assertion still holds"
    );

    // Superset-gate coverage.
    assert!(
        body.contains("bijection") && body.contains("§7"),
        "§6.2 must state the superset gate's per-family bijection coverage"
    );

    // The path family, DERIVED from the authored fanout rows: a dictionary that gains a
    // row without a documentation line reds here.
    let declared = declared_zdict_paths();
    assert!(
        declared.len() >= 5,
        "the fanout-row extraction looks broken: found {declared:?}"
    );
    let undocumented: Vec<&String> = declared
        .iter()
        .filter(|path| !body.contains(path.as_str()))
        .collect();
    assert!(
        undocumented.is_empty(),
        "§6.2 does not name {undocumented:?}, which slices/core/pipeline/module.ttl declares \
         as header-dict fanout rows — the document must track the declaration, so a new \
         dictionary is undocumented until this section names its path"
    );

    // …and nothing is documented that is NOT declared, or the section would be advertising
    // a path the gate does not reconstruct. A `*` token is the FAMILY rather than a
    // member, so it names no row and is not held to one.
    for line in body.lines() {
        for token in line.split('`') {
            if token.contains('*') {
                continue;
            }
            if token.starts_with("generated/medium/") && token.ends_with(".zdict") {
                assert!(
                    declared.contains(token),
                    "§6.2 names {token:?}, which no gmeow:FanoutExtraction row declares"
                );
            }
        }
    }
}
