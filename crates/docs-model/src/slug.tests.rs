// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn slugify_is_filesystem_safe() {
    assert_eq!(slugify("HasOwner"), "hasowner");
    assert_eq!(slugify("Cat 9 Lives!"), "cat-9-lives");
    assert_eq!(slugify("--weird--"), "weird");
    assert_eq!(slugify(""), "unnamed");
}

#[test]
fn align_tag_handles_trailing_separators() {
    assert_eq!(
        align_tag("http://www.w3.org/2004/02/skos/core#closeMatch"),
        "closeMatch"
    );
    assert_eq!(
        align_tag("http://www.w3.org/2002/07/owl#equivalentClass"),
        "equivalentClass"
    );
    // trailing separator must not yield an empty tag
    assert_eq!(align_tag("http://example.org/vocab#"), "vocab");
    assert_eq!(align_tag("http://example.org/vocab/"), "vocab");
    // no separator at all -> whole predicate
    assert_eq!(align_tag("bareword"), "bareword");
}
