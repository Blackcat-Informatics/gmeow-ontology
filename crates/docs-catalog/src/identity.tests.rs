// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn dist_iri_hangs_off_the_declared_base() {
    assert_eq!(
        dist_iri("site"),
        "https://blackcatinformatics.ca/gmeow/distribution/dist/site"
    );
}

#[test]
fn local_name_recovers_every_catalog_subject_tail() {
    assert_eq!(
        local_name("https://blackcatinformatics.ca/gmeow/distribution/family/doc-render"),
        "doc-render"
    );
    assert_eq!(
        local_name("https://blackcatinformatics.ca/gmeow/consumerPublicSite"),
        "consumerPublicSite"
    );
    assert_eq!(local_name("bare"), "bare");
}

#[test]
fn nt_literal_escapes_the_five_reserved_forms() {
    assert_eq!(
        triple_lit("s", "p", "a\"b\\c\nd\re\tf"),
        "<s> <p> \"a\\\"b\\\\c\\nd\\re\\tf\" ."
    );
}

#[test]
fn triple_is_the_plain_iri_form() {
    assert_eq!(triple("s", "p", "o"), "<s> <p> <o> .");
}
