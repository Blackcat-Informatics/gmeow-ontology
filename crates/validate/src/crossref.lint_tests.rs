// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::check_duplicate_keys;

#[test]
fn detects_duplicate_citation_keys_in_dataset() {
    let xml = r#"
<dataset dataset_type="record">
  <doi_data>
    <doi>10.67342/26w4o</doi>
    <resource>https://example.invalid/</resource>
  </doi_data>
  <citation_list>
    <citation key="ref-dup" type="web_resource">
      <unstructured_citation>First duplicate.</unstructured_citation>
    </citation>
    <citation key="ref-dup" type="web_resource">
      <unstructured_citation>Second duplicate.</unstructured_citation>
    </citation>
  </citation_list>
</dataset>
"#;
    let mut problems: Vec<String> = vec![];
    check_duplicate_keys(xml, &mut problems);
    assert_eq!(
        problems.len(),
        1,
        "expected exactly one problem, got: {:?}",
        problems
    );
    assert!(
        problems[0].contains("duplicate citation keys"),
        "unexpected message: {}",
        problems[0]
    );
    assert!(
        problems[0].contains("10.67342/26w4o"),
        "message should name the dataset DOI: {}",
        problems[0]
    );
}

#[test]
fn no_false_positive_for_unique_citation_keys() {
    let xml = r#"
<dataset dataset_type="record">
  <doi_data>
    <doi>10.67342/26w4o</doi>
    <resource>https://example.invalid/</resource>
  </doi_data>
  <citation_list>
    <citation key="ref-a" type="web_resource">
      <unstructured_citation>First.</unstructured_citation>
    </citation>
    <citation key="ref-b" type="web_resource">
      <unstructured_citation>Second.</unstructured_citation>
    </citation>
  </citation_list>
</dataset>
"#;
    let mut problems: Vec<String> = vec![];
    check_duplicate_keys(xml, &mut problems);
    assert!(
        problems.is_empty(),
        "expected no problems for unique keys, got: {:?}",
        problems
    );
}
