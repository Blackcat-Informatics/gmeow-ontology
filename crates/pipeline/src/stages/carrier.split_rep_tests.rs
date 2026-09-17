// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// ONE AUTHORITY, both directions: every path
/// [`crate::stages::archive_blobs::lang_projection_members`] selects into the
/// `lang-projections-archive` is refused by [`opaque_already_carried`], and no other
/// path is. If the two ever disagreed, a member of the `lang:` family would either
/// ride BOTH archives (the same bytes in two differently-primed frames, which the
/// superset reverse sweep hard-fails) or NEITHER (a silently dropped deliverable).
///
/// The agreement is now STRUCTURAL — both sides call
/// `archive_blobs::is_lang_projection_member` — so what this pins is the SET: the
/// nested projection tree and the two non-RDF terminology surfaces are in, and the
/// near misses (a sibling directory that merely shares the prefix, the RDF
/// `.vartrans.ttl` that rides its own named graph, an unrelated projection) are out.
#[test]
fn the_lang_projection_family_is_the_same_set_the_archive_selects() {
    let mappings: BTreeMap<String, Vec<u8>> = [
        ("generated/projections/lang/ebnf/gmn.ebnf", &b"g"[..]),
        ("generated/projections/lang/bcp47-tags.ttl", b"t"),
        ("generated/projections/lang/gmn1/v1/deep/x.gmn", b"d"),
        // Near misses that must NOT be swept into the lang archive: a sibling
        // directory whose name merely shares the prefix, and an unrelated projection.
        ("generated/projections/lang-extra/x.ttl", b"n"),
        ("generated/projections/core-prefixes.ttl", b"c"),
        ("generated/n3/gmeow.n3", b"3"),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_string(), b.to_vec()))
    .collect();
    let glossary: BTreeMap<String, Vec<u8>> = [
        (crate::stages::lang_glossary::GLOSSARY_TABLE_PATH, &b"m"[..]),
        (crate::stages::lang_glossary::GLOSSARY_TBX_PATH, b"x"),
        // RDF: it rides graph/fanout/projections/glossary.vartrans.ttl, and a named
        // graph is never de-folded into bytes to widen a dictionary's population.
        (crate::stages::lang_glossary::GLOSSARY_VARTRANS_PATH, b"v"),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_string(), b.to_vec()))
    .collect();

    let selected = crate::stages::archive_blobs::lang_projection_members(&mappings, &glossary);
    let selected_paths: Vec<&str> = selected.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        selected_paths,
        [
            "generated/catalog/glossary.md",
            "generated/projections/glossary.tbx",
            "generated/projections/lang/bcp47-tags.ttl",
            "generated/projections/lang/ebnf/gmn.ebnf",
            "generated/projections/lang/gmn1/v1/deep/x.gmn",
        ],
        "the archive selects exactly the lang: deliverable family, sorted"
    );
    for path in mappings.keys().chain(glossary.keys()) {
        assert_eq!(
            opaque_already_carried(path),
            selected_paths.contains(&path.as_str()),
            "{path}: the generated-opaque archive's guard and the lang-projections \
                 archive's selector must name the SAME set"
        );
    }
}

/// The RDF members of the family (`.ttl`/`.nt`) are the reason the guard exists at
/// all: `take_opaque` already drops them via `is_rdf_member`, so only
/// [`opaque_already_carried`] can state that a NON-RDF lang projection is somebody
/// else's rep.
#[test]
fn a_non_rdf_lang_projection_is_refused_by_take_opaque() {
    let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    take_opaque(
        &mut members,
        [
            ("generated/projections/lang/ebnf/gmn.ebnf", &b"g"[..]),
            ("generated/projections/lang/tei/x.tei.xml", b"x"),
            (crate::stages::lang_glossary::GLOSSARY_TABLE_PATH, b"m"),
            (crate::stages::lang_glossary::GLOSSARY_TBX_PATH, b"b"),
            ("generated/references/refs.md", b"r"),
        ]
        .into_iter()
        .map(|(p, b)| (p.to_string(), b.to_vec()))
        .collect(),
    );
    assert_eq!(
        members.keys().collect::<Vec<_>>(),
        vec!["generated/references/refs.md"],
        "no lang-projection member may reach the generated-opaque archive"
    );
}

/// The statement layer's two byte projections are refused from the generated-opaque
/// archive the same way, and for the same reason: they are BYTE-DECORATED RDF, so
/// `take_opaque`'s `is_rdf_member` filter would drop them anyway — only
/// [`opaque_already_carried`] states that they are `statements-archive`'s members,
/// and the sink inserts them into no map but that archive's.
#[test]
fn the_statement_byte_projections_are_refused_from_the_opaque_archive() {
    for path in crate::stages::archive_blobs::STATEMENT_FILES {
        assert!(
            opaque_already_carried(path),
            "{path} rides statements-archive and must never double-carry"
        );
    }
    // A near miss under the same directory that no rep claims stays available to the
    // generated-opaque archive, so the guard is a member list rather than a prefix.
    assert!(!opaque_already_carried("generated/statements/other.json"));
}
