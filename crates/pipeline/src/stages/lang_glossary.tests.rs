// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn glossary_table_cells_escape_structural_characters() {
    assert_eq!(cell("a | b"), "a \\| b");
    assert_eq!(cell("line1\nline2\tx"), "line1 line2 x");
    assert_eq!(
        localname("https://blackcatinformatics.ca/gmeow/EntityExistence"),
        "EntityExistence"
    );
}

#[test]
fn tbx_text_and_attributes_escape_xml() {
    assert_eq!(xml_text("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    assert_eq!(xml_attr("x\"y"), "x&quot;y");
}
