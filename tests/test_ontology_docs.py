# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""Tests for the ontology-docs generator (#440)."""

from __future__ import annotations

import re
from pathlib import Path
from xml.etree import ElementTree as ET

import pytest

from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.ontology_docs import build_ontology_docs, ontology_docs_inputs

_TICKET_REFERENCE_RE = re.compile(r"(?i)\b(?:issue|pr)\s+#\d+|#[0-9]+")


def test_ontology_docs_inputs_include_slice_design_docs() -> None:
    rel_inputs = {
        path.relative_to(PROJECT_ROOT).as_posix()
        for path in ontology_docs_inputs()
        if path.is_relative_to(PROJECT_ROOT)
    }
    assert "slices/core/logic/design/LOGIC.md" in rel_inputs


@pytest.mark.ci_only
def test_build_ontology_docs_creates_expected_tree(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    docs = out / "markdown"
    assert (out / "site" / "index.html").exists()
    assert (out / "site" / "assets" / "simple.css").exists()
    assert (out / "site" / "assets" / "gmeow.css").exists()
    assert (out / "site" / "favicon.svg").exists()
    assert (out / "site" / "diagrams" / "slices.svg").exists()
    assert (out / "site" / "diagrams" / "slices" / "kernel.svg").exists()
    assert (out / "site" / "terms" / "Person" / "index.html").exists()
    assert (docs / "index.md").exists()
    assert (docs / "adoption" / "index.md").exists()
    assert (docs / "adoption" / "schema.md").exists()
    assert (docs / "about.md").exists()
    assert (docs / "changelog.md").exists()
    assert (docs / "reference" / "classes" / "index.md").exists()
    assert (docs / "reference" / "properties" / "index.md").exists()
    assert (docs / "reference" / "individuals" / "index.md").exists()
    assert (docs / "reference" / "datatypes" / "index.md").exists()
    assert (docs / "slices" / "index.md").exists()
    assert (docs / "slices" / "logic" / "design" / "LOGIC.md").exists()
    assert (docs / "profiles" / "index.md").exists()
    assert (docs / "profiles" / "core.md").exists()
    assert (docs / "profiles" / "full.md").exists()
    assert (docs / "external" / "ontologies.md").exists()
    assert (docs / "external" / "terms.md").exists()
    assert (docs / "learning-paths" / "index.md").exists()
    assert (docs / "recipes" / "index.md").exists()
    assert (docs / "examples" / "index.md").exists()
    assert (docs / "recipes" / "person-names-and-display.md").exists()
    assert (docs / "references" / "index.md").exists()
    assert (out / "site" / "search-index.json").exists()
    assert (out / "site" / "llms-docs.txt").exists()
    assert (docs / "search-index.json").exists()
    assert (docs / "llms-docs.txt").exists()
    assert (docs / "concerns" / "index.md").exists()
    assert (docs / "linkages" / "index.md").exists()
    assert (docs / "statements" / "index.md").exists()
    assert (out / "site" / "changelog" / "index.html").exists()
    assert (out / "site" / "linkages" / "index.html").exists()
    assert (out / "site" / "visualization" / "index.html").exists()
    assert (out / "site" / "quality" / "oops-report" / "index.html").exists()
    for svg in sorted(out.rglob("*.svg")):
        ET.parse(svg)

    files = [p.relative_to(out).as_posix() for p in out.rglob("*") if p.is_file()]
    casefolded_files = [p.casefold() for p in files]
    assert len(casefolded_files) == len(set(casefolded_files))
    assert "mkdocs.yml" not in files
    assert not any(p.endswith((".js", ".js.map", ".css.map")) for p in files)
    for path in out.rglob("*"):
        if path.is_file() and path.suffix in {".html", ".md", ".svg", ".css"}:
            assert "x-gmeow-" not in path.read_text(encoding="utf-8")


@pytest.mark.ci_only
def test_build_ontology_docs_is_deterministic(tmp_path: Path) -> None:
    out1 = tmp_path / "tree1"
    out2 = tmp_path / "tree2"
    build_ontology_docs(out1)
    build_ontology_docs(out2)

    files1 = sorted(p.relative_to(out1) for p in out1.rglob("*") if p.is_file())
    files2 = sorted(p.relative_to(out2) for p in out2.rglob("*") if p.is_file())
    assert files1 == files2
    for rel in files1:
        assert (out1 / rel).read_bytes() == (out2 / rel).read_bytes()


@pytest.mark.ci_only
def test_index_contains_ontology_header_and_slice_stats(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)
    index = (out / "markdown" / "index.md").read_text(encoding="utf-8")

    assert "GMEOW" in index
    assert "Namespace:" in index
    assert "## Profiles" in index
    assert "## Slices" in index
    assert "Learning Paths" in index
    assert "Examples" in index
    assert "Adoption Targets" in index
    assert "Bibliography" in index
    assert "Linkages" in index
    assert "## Reference" in index
    assert "## Distribution" in index
    assert "gmeow export-docs --directory docs-out" in index
    assert "gmeow create-docs" not in index
    assert "recipes/index.md" in index
    assert "search-index.json" in index


@pytest.mark.ci_only
def test_slice_index_lists_manifest_metadata(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)
    slices_index = (out / "markdown" / "slices" / "index.md").read_text(
        encoding="utf-8"
    )

    assert "# Slices" in slices_index
    assert "| Slice | Tier | Profiles | Dependencies | Consumers |" in slices_index
    assert "[kernel](kernel.md)" in slices_index


@pytest.mark.ci_only
def test_reference_pages_have_term_metadata(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    # gmeow:Person is a core class that should always exist.
    person = out / "markdown" / "reference" / "classes" / "gmeow-Person.md"
    assert person.exists()
    text = person.read_text(encoding="utf-8")
    assert "gmeow:Person" in text
    assert "**IRI:**" in text
    assert "## Linkages" in text
    assert "## Practical Pattern" in text
    assert "## Common Companion Terms" in text
    assert "gmeow-classes.sssom.tsv" in text
    assert "schema:Person" in text


@pytest.mark.ci_only
def test_reference_pages_surface_advisory_usage_metadata(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    name_usage = out / "markdown" / "reference" / "classes" / "gmeow-NameUsage.md"
    text = name_usage.read_text(encoding="utf-8")

    assert "## Usage Advice" in text
    assert "### Use when" in text
    assert "name-bearing fact needs its own audience" in text
    assert "### How to use" in text
    assert "Mint one [`NameUsage`](gmeow-NameUsage.md) per" in text
    assert "### Use For Consumers" in text
    assert (
        "[`gmeow:consumerAgentMemory`](../individuals/gmeow-consumerAgentMemory.md)"
        in text
    )
    assert "### Avoid For Consumers" in text
    assert (
        "[`gmeow:consumerSchemaOrgJsonLd`](../individuals/"
        "gmeow-consumerSchemaOrgJsonLd.md)" in text
    )


@pytest.mark.ci_only
def test_public_identifier_references_are_linked(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    article_md = (
        out / "markdown" / "reference" / "classes" / "gmeow-Article.md"
    ).read_text(encoding="utf-8")
    article_html = (
        out / "site" / "reference" / "classes" / "gmeow-Article" / "index.html"
    ).read_text(encoding="utf-8")
    teleology_md = (out / "markdown" / "slices" / "teleology.md").read_text(
        encoding="utf-8"
    )
    names_md = (out / "markdown" / "slices" / "names.md").read_text(encoding="utf-8")
    eligible_md = (
        out / "markdown" / "reference" / "properties" / "gmeow-eligibleForConsumer.md"
    ).read_text(encoding="utf-8")

    assert "[wd:Q191067](https://www.wikidata.org/wiki/Q191067)" in article_md
    assert 'href="https://www.wikidata.org/wiki/Q191067"' in article_html
    assert "[Q4503831](https://www.wikidata.org/wiki/Q4503831)" in teleology_md
    assert "[`wdt:P6553`](https://www.wikidata.org/wiki/Property:P6553)" in names_md
    assert (
        "[`gmeow:ProjectionContext`](../classes/gmeow-ProjectionContext.md)"
        in eligible_md
    )
    teleology_md = (out / "markdown" / "slices" / "teleology.md").read_text(
        encoding="utf-8"
    )
    assert "[`gmeow:slices/events`](events.md)" in teleology_md
    assert (
        "[Principle 14](https://github.com/Blackcat-Informatics/"
        "gmeow-ontology/blob/main/CONSTITUTION.md#principle-14)" in teleology_md
    )
    assert "[`gufo:IntrinsicMode`](../external/terms.md#gufo-intrinsicmode)" in (
        teleology_md
    )
    assert "[`counterGoal`](../reference/properties/gmeow-counterGoal.md)" in (
        teleology_md
    )
    assert "[`Desire`](../reference/classes/gmeow-Desire.md)" in teleology_md
    assert "[PROV](../external/ontologies.md#target-prov)" in teleology_md
    assert "[P-Plan](../external/ontologies.md#target-pplan)" in teleology_md
    appellation_md = (
        out / "markdown" / "reference" / "classes" / "gmeow-Appellation.md"
    ).read_text(encoding="utf-8")
    assert "<td><code>subClassOf</code></td>" in appellation_md
    assert (
        '<td><a href="http://www.w3.org/ns/lemon/ontolex#LexicalEntry">'
        "<code>ontolex:LexicalEntry</code></a></td>" in appellation_md
    )
    assert "http://www.w3.org/ns/lemon/ontolex#LexicalEntry" in appellation_md
    assert "## External Equivalences" in appellation_md


@pytest.mark.ci_only
def test_external_catalogs_explain_linked_targets(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    ontologies = (out / "markdown" / "external" / "ontologies.md").read_text(
        encoding="utf-8"
    )
    terms = (out / "markdown" / "external" / "terms.md").read_text(encoding="utf-8")
    getting_started = (out / "markdown" / "getting-started.md").read_text(
        encoding="utf-8"
    )
    gts = (out / "markdown" / "slices" / "gts.md").read_text(encoding="utf-8")

    assert "PROV-O (`prov`)" in ontologies
    assert "P-Plan (`pplan`)" in ontologies
    assert "ConceptNet (`conceptnet`)" in ontologies
    assert "`gufo:IntrinsicMode`" in terms
    assert "desires, and intentions as features of an agent" in terms
    assert "slices/core/names/examples/person-names.ttl" in getting_started
    assert "slices/core/standpoint/examples/contested-authorship.ttl" in (
        getting_started
    )
    assert (
        "[`docs/GTS-SPEC.md`](https://github.com/Blackcat-Informatics/"
        "gmeow-ontology/blob/main/docs/GTS-SPEC.md)" in gts
    )


@pytest.mark.ci_only
def test_linkages_page_summarizes_mapping_dsl(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    linkages = (out / "markdown" / "linkages" / "index.md").read_text(encoding="utf-8")

    assert "# Linkages" in linkages
    assert "dsl/mappings/" in linkages
    assert "SSSOM Mapping Sets" in linkages
    assert "Projection Profiles" in linkages
    assert "Round Trip" in linkages
    assert "Adoption Targets" in linkages
    assert "gmeow-classes.sssom.tsv" in linkages
    assert "`schema-org`" in linkages


@pytest.mark.ci_only
def test_adoption_target_pages_are_generated_from_linkages(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    index = (out / "markdown" / "adoption" / "index.md").read_text(encoding="utf-8")
    schema = (out / "markdown" / "adoption" / "schema.md").read_text(encoding="utf-8")
    html = (out / "site" / "adoption" / "schema" / "index.html").read_text(
        encoding="utf-8"
    )

    assert "# Adoption Targets" in index
    assert "[Schema.org](schema.md) (`schema`)" in index
    assert "# Schema.org" in schema
    assert "**Prefix:** `schema`" in schema
    assert "## Coverage" in schema
    assert "## Source Terms" in schema
    assert "## Mapping Rows" in schema
    assert "../external/ontologies.md#target-schema" in schema
    assert "schema:Person" in schema
    assert 'href="../../adoption/"' in html


@pytest.mark.ci_only
def test_references_page_uses_generated_citation_ledger(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    references = (out / "markdown" / "references" / "index.md").read_text(
        encoding="utf-8"
    )
    html = (out / "site" / "references" / "index.html").read_text(encoding="utf-8")

    assert "# References" in references
    assert "metadata/references.ttl" in references
    assert "generated/references/references.csl.json" in references
    assert "generated/references/references.bib" in references
    assert "# GMEOW Citation Ledger" not in references
    assert "| Reference | Locator | Citation acts |" in references
    assert "Schema.org" in references
    assert "[link](http://qudt)" not in references
    assert "[link](https://blackcatinformatics)" not in references
    assert 'href="../references/"' in html


@pytest.mark.ci_only
def test_slice_design_docs_are_rendered_and_linked(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    logic = (out / "markdown" / "slices" / "logic.md").read_text(encoding="utf-8")
    design = (out / "markdown" / "slices" / "logic" / "design" / "LOGIC.md").read_text(
        encoding="utf-8"
    )

    assert "## Design Documents" in logic
    assert "GMEOW Logic" in logic
    assert "slices/core/logic/design/LOGIC.md" in logic
    assert "# Logic" in design
    assert "[logic](../../logic.md)" in design
    assert "## The document set" in design
    assert "[`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)" in design


@pytest.mark.ci_only
def test_recipes_and_examples_are_generated_from_slice_sources(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    recipe = (out / "markdown" / "recipes" / "person-names-and-display.md").read_text(
        encoding="utf-8"
    )
    names = (out / "markdown" / "slices" / "names.md").read_text(encoding="utf-8")

    assert "# Model Person Names Without a Preferred-Name Slot" in recipe
    assert "slices/core/names/examples/person-names.ttl" in recipe
    assert "```turtle" in recipe
    assert "[`gmeow:PersonName`](../reference/classes/gmeow-PersonName.md)" in recipe
    assert "## Examples" in names
    assert "person-names.ttl" in names
    assert "```turtle" in names


@pytest.mark.ci_only
def test_examples_catalog_links_slice_sources_and_terms(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    examples = (out / "markdown" / "examples" / "index.md").read_text(encoding="utf-8")
    html = (out / "site" / "examples" / "index.html").read_text(encoding="utf-8")

    assert "# Examples" in examples
    assert "slices/**/examples/*.ttl" in examples
    assert "[names](../slices/names.md)" in examples
    assert "person-names.ttl" in examples
    assert "gmeow-PersonName.md" in examples
    assert "github.com/Blackcat-Informatics/gmeow-ontology/blob/main/" in examples
    assert 'href="../examples/"' in html
    assert 'id="example-slices-core-names-examples-person-names"' in html


@pytest.mark.ci_only
def test_learning_paths_sequence_recipes_examples_terms_and_targets(
    tmp_path: Path,
) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    learning_paths = (out / "markdown" / "learning-paths" / "index.md").read_text(
        encoding="utf-8"
    )
    html = (out / "site" / "learning-paths" / "index.html").read_text(encoding="utf-8")

    assert "# Learning Paths" in learning_paths
    assert "Model a Person Without Flattening Identity" in learning_paths
    assert "person-names-and-display.md" in learning_paths
    assert "examples/index.md#example-slices-core-names-examples-person-names" in (
        learning_paths
    )
    assert "../reference/classes/gmeow-Person.md" in learning_paths
    assert "../external/ontologies.md#target-schema" in learning_paths
    assert 'href="../learning-paths/"' in html


@pytest.mark.ci_only
def test_term_pages_include_example_snippets_from_canonical_sources(
    tmp_path: Path,
) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    person_name = (
        out / "markdown" / "reference" / "classes" / "gmeow-PersonName.md"
    ).read_text(encoding="utf-8")

    assert "## Example Snippets" in person_name
    assert "slices/core/names/examples/person-names.ttl" in person_name
    assert "examples/index.md#example-slices-core-names-examples-person-names" in (
        person_name
    )
    assert "```turtle" in person_name
    assert "gmeow:PersonName" in person_name


@pytest.mark.ci_only
def test_static_search_indexes_include_terms_slices_and_recipes(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    search_index = (out / "site" / "search-index.json").read_text(encoding="utf-8")
    llms_docs = (out / "site" / "llms-docs.txt").read_text(encoding="utf-8")

    assert '"curie": "gmeow:PersonName"' in search_index
    assert '"kind": "learning-path"' in search_index
    assert '"kind": "adoption-target"' in search_index
    assert '"kind": "references"' in search_index
    assert '"kind": "slice-design"' in search_index
    assert '"path": "learning-paths/index.html#model-a-person"' in search_index
    assert '"path": "adoption/schema/index.html"' in search_index
    assert '"path": "slices/logic/design/LOGIC/index.html"' in search_index
    assert '"recipe": "person-names-and-display"' in search_index
    assert '"kind": "example"' in search_index
    assert (
        '"path": "examples/index.html#example-slices-core-names-examples-person-names"'
        in search_index
    )
    assert '"slice": "names"' in search_index
    assert "person-names-and-display: Model Person Names" in llms_docs
    assert "model-a-person: Model a Person Without Flattening Identity" in llms_docs
    assert "schema: Schema.org; linkage rows" in llms_docs
    assert "references: generated bibliography from metadata/references.ttl" in (
        llms_docs
    )
    assert "logic: Logic; source slices/core/logic/design/LOGIC.md" in llms_docs
    assert "slices/core/names/examples/person-names.ttl" in llms_docs
    assert "gmeow:PersonName" in llms_docs


@pytest.mark.ci_only
def test_public_markdown_hides_internal_ticket_references(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    for path in sorted((out / "markdown").rglob("*.md")):
        text = path.read_text(encoding="utf-8")
        assert _TICKET_REFERENCE_RE.search(text) is None, path


def test_external_ontology_catalog_has_specific_descriptions(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    catalog = (out / "markdown" / "external" / "ontologies.md").read_text(
        encoding="utf-8"
    )

    assert "External vocabulary or concept scheme used as a linkage" not in catalog
    assert "Basic Formal Ontology 2020" in catalog
    assert "Library of Congress bibliographic model" in catalog


@pytest.mark.ci_only
def test_html_links_are_directory_index_safe(tmp_path: Path) -> None:
    out = tmp_path / "ontology-docs"
    build_ontology_docs(out)

    index = (out / "site" / "index.html").read_text(encoding="utf-8")
    assert 'href="favicon.svg"' in index
    assert 'href="assets/simple.css"' in index
    assert 'href="assets/gmeow.css"' in index
    assert 'href="learning-paths/"' in index
    assert 'href="examples/"' in index
    assert 'href="adoption/"' in index
    assert 'href="references/"' in index
    assert "https://cdn.simplecss.org" not in index

    person = out / "site" / "reference" / "classes" / "gmeow-Person" / "index.html"
    html = person.read_text(encoding="utf-8")
    assert 'href="../../../"' in html

    slice_page = out / "site" / "slices" / "kernel" / "index.html"
    slice_html = slice_page.read_text(encoding="utf-8")
    assert 'src="../../diagrams/slices/kernel.svg"' in slice_html

    alias = out / "site" / "terms" / "Person" / "index.html"
    alias_html = alias.read_text(encoding="utf-8")
    assert 'http-equiv="refresh"' in alias_html
    assert "../../reference/classes/gmeow-Person/" in alias_html
