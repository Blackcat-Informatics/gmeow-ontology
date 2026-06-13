"""Vocabulary-surface integrity gates (issue #199).

Principle 4 (One canonical source) + Principle 7 (Verified by construction):
- Every authored ontology module must be in the root owl:imports closure.
- Coverage fixtures must use only declared GMEOW terms.
- Docs examples must not mint non-vocabulary gmeow: IRIs in user-copyable blocks.
"""

from __future__ import annotations

import re
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest
from rdflib import OWL, RDF, Graph, Namespace, URIRef

from gmeow_tools.config import (
    CATALOG_FILE,
    FIXTURES_DIR,
    MAPPING_DSL_DIR,
    NAMESPACE,
    ONTOLOGY_FILE,
    PROJECT_ROOT,
    SHAPES_FILE,
    SLICE_VOCABULARY_FILE,
    STATEMENT_DSL_DIR,
)
from gmeow_tools.graph import iter_module_files

GMEOW = Namespace(NAMESPACE)
GMEOW_MODULES = NAMESPACE + "modules/"
GMEOW_EXAMPLES = NAMESPACE + "examples/"

# Directories that contain authored docs (not _generated).
DOCS_DIR = PROJECT_ROOT / "docs"

# DSL source directories whose terms are legitimate in docs prose.


# Terms that are intentionally retired but still mentioned in docs as historical.
_RETIRED_DOCS_TERMS = {
    NAMESPACE + name
    for name in (
        "alternateName",
        "gender",
        "sex",
    )
}

# Regex to find gmeow:LocalName in Turtle fenced blocks and inline code.
_TURTLE_BLOCK_RE = re.compile(r"```turtle\n(.*?)\n```", re.DOTALL)
_INLINE_TERM_RE = re.compile(r"`gmeow:([A-Za-z][A-Za-z0-9_]*)`")


def _parse_ttl(path: Path) -> Graph:
    return Graph().parse(path, format="turtle")


def _subjects(graph: Graph) -> set[str]:
    return {str(s) for s in graph.subjects() if isinstance(s, URIRef)}


def _gmeow_vocabulary_terms(graph: Graph) -> set[str]:
    """Return URIRefs under the GMEOW namespace that look like vocabulary terms.

    Filters out example-instance IRIs (``/examples/…``) and module ontology IRIs
    (``/modules/…``).
    """
    terms: set[str] = set()
    for triple in graph:
        for node in triple:
            if isinstance(node, URIRef) and str(node).startswith(NAMESPACE):
                s = str(node)
                if s.startswith(GMEOW_EXAMPLES) or s.startswith(GMEOW_MODULES):
                    continue
                terms.add(s)
    return terms


# --------------------------------------------------------------------------- #
# Check A — module import/catalog closure
# --------------------------------------------------------------------------- #


def test_root_imports_are_exactly_the_core_profile() -> None:
    """The root IRI IS the core profile (#330): its owl:imports must equal
    the tierCore slice set exactly — an extension in the root, or a core
    slice missing from it, is a gated failure, never silent drift."""
    from gmeow_tools.slices import discover_slices

    root = _parse_ttl(ONTOLOGY_FILE)
    imports = {
        str(o) for o in root.objects(predicate=OWL.imports) if isinstance(o, URIRef)
    }
    core = {s.iri for s in discover_slices().values() if s.is_core}
    assert imports == core, (
        f"root/core drift — extra: {sorted(imports - core)}; "
        f"missing: {sorted(core - imports)}"
    )


def test_full_profile_imports_every_slice() -> None:
    """<…/gmeow/full> aggregates the root (core) plus every extension —
    no slice can exist outside its profiles (#330)."""
    from gmeow_tools.config import FULL_PROFILE_FILE, ONTOLOGY_IRI
    from gmeow_tools.slices import discover_slices

    full = _parse_ttl(FULL_PROFILE_FILE)
    imports = {
        str(o) for o in full.objects(predicate=OWL.imports) if isinstance(o, URIRef)
    }
    slices = discover_slices()
    extensions = {s.iri for s in slices.values() if not s.is_core}
    assert imports == {ONTOLOGY_IRI} | extensions

    # closure sanity: root(core) + extensions covers every discovered slice
    core = {s.iri for s in slices.values() if s.is_core}
    assert core | extensions == {s.iri for s in slices.values()}


def test_named_profiles_are_slim_and_dependency_closed() -> None:
    """Named profiles (#330) are SLIM: declared members plus their
    sliceDependsOn closure — never the root import — and the emitted
    document equals the closure recomputed from the manifests."""
    from gmeow_tools.config import NAMED_PROFILE_NS, ONTOLOGY_IRI, PROFILES_DIR
    from gmeow_tools.profiles_gen import dependency_closure, group_named_profiles
    from gmeow_tools.slices import discover_slices

    slices = discover_slices()
    named = group_named_profiles(slices)
    assert {"claims", "memory", "narrative"} <= set(named)

    kernel_iri = NAMESPACE + "slices/kernel"
    for name, members in named.items():
        doc = _parse_ttl(PROFILES_DIR / f"{name}.ttl")
        profile_iri = URIRef(NAMED_PROFILE_NS + name)
        imports = {
            str(o)
            for o in doc.objects(profile_iri, OWL.imports)
            if isinstance(o, URIRef)
        }
        assert ONTOLOGY_IRI not in imports, f"{name}: imports the whole core"
        assert imports == set(dependency_closure(members, slices)), (
            f"{name}: emitted imports drift from the manifest closure"
        )
        for iri in members:
            missing = set(slices[iri].depends_on) - imports
            assert not missing, f"{name}: member {iri} deps escape: {missing}"
        assert kernel_iri in imports, f"{name}: kernel never arrived"


def test_claims_profile_is_genuinely_sub_core() -> None:
    """The slim ruling made measurable: claims ⊂ core, strictly, and
    carries no extension."""
    from gmeow_tools.config import PROFILES_DIR
    from gmeow_tools.slices import discover_slices

    slices = discover_slices()
    core = {s.iri for s in slices.values() if s.is_core}
    doc = _parse_ttl(PROFILES_DIR / "claims.ttl")
    imports = {
        str(o) for o in doc.objects(predicate=OWL.imports) if isinstance(o, URIRef)
    }
    assert imports < core, "claims must be a strict subset of core"


def test_profile_membership_is_multi_valued() -> None:
    """A slice may serve several profiles — evidence is in claims AND
    memory (the kernel-adjacent multi-membership the registry must keep)."""
    from gmeow_tools.profiles_gen import group_named_profiles
    from gmeow_tools.slices import discover_slices

    named = group_named_profiles(discover_slices())
    evidence = NAMESPACE + "slices/evidence"
    assert evidence in named["claims"]
    assert evidence in named["memory"]


def test_catalog_maps_every_named_profile() -> None:
    """ROBOT/Protégé offline resolution covers the named profile IRIs."""
    from gmeow_tools.config import NAMED_PROFILE_NS
    from gmeow_tools.profiles_gen import group_named_profiles
    from gmeow_tools.slices import discover_slices

    catalog = ET.parse(CATALOG_FILE)
    ns = {"catalog": "urn:oasis:names:tc:entity:xmlns:xml:catalog"}
    catalog_iris = {uri.get("name") for uri in catalog.findall(".//catalog:uri", ns)}
    for name in group_named_profiles(discover_slices()):
        assert NAMED_PROFILE_NS + name in catalog_iris


def test_landing_page_profiles_section_lists_every_profile() -> None:
    """The recorded #330 sub-decision: the human page stays unified and
    gains a Profiles section covering core, full, and every named profile."""
    from gmeow_tools.config import FULL_PROFILE_IRI, NAMED_PROFILE_NS, ONTOLOGY_IRI
    from gmeow_tools.docs import profiles_section_html
    from gmeow_tools.profiles_gen import group_named_profiles
    from gmeow_tools.slices import discover_slices

    html = profiles_section_html()
    assert ONTOLOGY_IRI in html and FULL_PROFILE_IRI in html
    for name in group_named_profiles(discover_slices()):
        assert NAMED_PROFILE_NS + name in html


def test_dcat_describes_every_profile_as_a_dataset() -> None:
    """Machine-discoverable composition: full + each named profile is a
    dcat:Dataset with its IRI as the landing page (#330)."""
    from rdflib.namespace import DCAT

    from gmeow_tools.config import DCAT_FILE, FULL_PROFILE_IRI, NAMED_PROFILE_NS
    from gmeow_tools.profiles_gen import group_named_profiles
    from gmeow_tools.slices import discover_slices

    dcat = _parse_ttl(DCAT_FILE)
    full = URIRef(FULL_PROFILE_IRI)
    assert (full, RDF.type, DCAT.Dataset) in dcat
    assert (full, DCAT.landingPage, full) in dcat
    for name in group_named_profiles(discover_slices()):
        profile = URIRef(NAMED_PROFILE_NS + name)
        assert (profile, RDF.type, DCAT.Dataset) in dcat
        assert (profile, DCAT.landingPage, profile) in dcat


def test_all_modules_are_in_catalog() -> None:
    """Every ontology/modules/*.ttl owl:Ontology must be mapped in catalog."""
    catalog = ET.parse(CATALOG_FILE)
    ns = {"catalog": "urn:oasis:names:tc:entity:xmlns:xml:catalog"}
    catalog_iris = {uri.get("name") for uri in catalog.findall(".//catalog:uri", ns)}

    missing: list[str] = []
    for module_path in iter_module_files():
        module_graph = _parse_ttl(module_path)
        ontology_subjects = list(module_graph.subjects(RDF.type, OWL.Ontology))
        module_iri = str(ontology_subjects[0])
        if module_iri not in catalog_iris:
            missing.append(module_iri)

    assert not missing, f"Modules missing from catalog-v001.xml: {missing}"


def test_module_iri_matches_filename() -> None:
    """Each module's owl:Ontology IRI follows its location.

    Flat modules: ``…/gmeow/modules/<stem>``. Slice modules
    (``slices/<group>/<name>/module.ttl``, #287): ``…/gmeow/slices/<name>`` —
    the unified slice IRI, named by the slice directory, never the group.
    """
    mismatches: list[tuple[str, str]] = []
    for module_path in iter_module_files():
        module_graph = _parse_ttl(module_path)
        ontology_subjects = list(module_graph.subjects(RDF.type, OWL.Ontology))
        module_iri = str(ontology_subjects[0])
        if module_path.name == "module.ttl":
            expected = f"{NAMESPACE}slices/{module_path.parent.name}"
        else:
            expected = GMEOW_MODULES + module_path.stem
        if module_iri != expected:
            mismatches.append((module_path.name, module_iri))

    assert not mismatches, f"Module IRI / filename mismatches: {mismatches}"


# --------------------------------------------------------------------------- #
# Check B — fixture GMEOW terms must be declared
# --------------------------------------------------------------------------- #


def _declared_ontology_terms() -> set[str]:
    """Build the set of GMEOW terms declared as subjects in the ontology."""
    declared: set[str] = set()
    for path in [ONTOLOGY_FILE, *iter_module_files()]:
        declared.update(_subjects(_parse_ttl(path)))
    # Exclude module ontology IRIs themselves — they are not vocabulary terms.
    declared = {s for s in declared if not s.startswith(GMEOW_MODULES)}
    return declared


@pytest.fixture(scope="session")
def declared_ontology_terms() -> set[str]:
    return _declared_ontology_terms()


def test_coverage_fixtures_use_only_declared_terms(
    declared_ontology_terms: set[str],
) -> None:
    """Coverage fixtures must not use undeclared GMEOW vocabulary terms."""
    undeclared: dict[str, list[str]] = {}
    for fixture_path in sorted(FIXTURES_DIR.glob("*.ttl")):
        fixture_graph = _parse_ttl(fixture_path)
        terms = _gmeow_vocabulary_terms(fixture_graph)
        bad = sorted(terms - declared_ontology_terms)
        if bad:
            undeclared[fixture_path.name] = bad

    messages = []
    for name, bad in undeclared.items():
        messages.append(f"  {name}: {bad}")
    assert not undeclared, "Undeclared GMEOW terms in coverage fixtures:\n" + "\n".join(
        messages
    )


# --------------------------------------------------------------------------- #
# Check C — docs/examples lint for authored GMEOW terms
# --------------------------------------------------------------------------- #


def _dsl_subjects(dsl_dir: Path) -> set[str]:
    """Collect all subjects from every TTL file under a DSL directory."""
    subjects: set[str] = set()
    for path in sorted(dsl_dir.rglob("*.ttl")):
        subjects.update(_subjects(_parse_ttl(path)))
    return subjects


def _docs_allowlist() -> set[str]:
    """Terms that are legitimate in docs prose."""
    allowed = _declared_ontology_terms()
    # Shapes are referenced in reasoning/rights/standpoints docs.
    if SHAPES_FILE.exists():
        allowed.update(_subjects(_parse_ttl(SHAPES_FILE)))
    # Mapping DSL terms are referenced in projections docs.
    allowed.update(_dsl_subjects(MAPPING_DSL_DIR))
    # Statement DSL terms are referenced in statement docs.
    allowed.update(_dsl_subjects(STATEMENT_DSL_DIR))
    # Slice DSL terms (manifest fields) are referenced in the CLI roll-up doc.
    if SLICE_VOCABULARY_FILE.exists():
        allowed.update(_subjects(_parse_ttl(SLICE_VOCABULARY_FILE)))
    # Retired terms intentionally mentioned in migration/retirement prose.
    allowed.update(_RETIRED_DOCS_TERMS)
    return allowed


def _find_gmeow_terms_in_markdown(path: Path) -> set[str]:
    """Scan a Markdown file for gmeow: terms in fenced turtle blocks and inline code."""
    text = path.read_text(encoding="utf-8")
    found: set[str] = set()

    # Fenced turtle blocks
    for block in _TURTLE_BLOCK_RE.findall(text):
        for match in _INLINE_TERM_RE.finditer(block):
            found.add(NAMESPACE + match.group(1))
        # Also catch bare gmeow:LocalName in Turtle (not backticked)
        for match in re.finditer(r"\bgmeow:([A-Za-z][A-Za-z0-9_]*)\b", block):
            found.add(NAMESPACE + match.group(1))

    # Inline backticked terms outside blocks (e.g. `gmeow:hasName`)
    for match in _INLINE_TERM_RE.finditer(text):
        found.add(NAMESPACE + match.group(1))

    return found


@pytest.fixture(scope="session")
def docs_allowlist() -> set[str]:
    return _docs_allowlist()


def test_docs_examples_use_only_allowed_terms(docs_allowlist: set[str]) -> None:
    """User-copyable docs examples must not use unallowlisted gmeow: IRIs."""
    unallowed: dict[str, list[str]] = {}
    for doc_path in sorted(DOCS_DIR.glob("*.md")):
        terms = _find_gmeow_terms_in_markdown(doc_path)
        bad = sorted(terms - docs_allowlist)
        if bad:
            unallowed[doc_path.name] = bad

    messages = []
    for name, bad in unallowed.items():
        messages.append(f"  {name}: {bad}")
    assert not unallowed, "Unallowlisted gmeow: terms in docs examples:\n" + "\n".join(
        messages
    )
