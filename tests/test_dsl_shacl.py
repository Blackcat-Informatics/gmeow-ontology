"""Tests for RDF-native SHACL validation of the mapping and statement DSL sources.

Each test confirms that malformed DSL cells yield structured SHACL diagnostics
(focus node, path, message, source file) before the Python graph-walkers
produce their own errors.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools.mapping_dsl import CompileError, load_dsl
from gmeow_tools.statement_dsl import load_statement_dsl

_MALFORMED_MAPPING_TTL = """
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix semapv: <https://w3id.org/semapv/vocab/> .

# Missing alignSubject — should trigger a SHACL violation.
gmeow:eqBad001 a gmeow:TermEquivalence ;
    gmeow:alignPredicate owl:equivalentClass ;
    gmeow:alignObject <https://schema.org/Person> ;
    gmeow:confidence 1.0 ;
    gmeow:sssomFile "gmeow-bad.sssom.tsv" .
"""

_MALFORMED_STATEMENT_TTL = """
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <https://blackcatinformatics.ca/gmeow/examples/> .

# Both qObject and qObjectLiteral present — should trigger a SHACL violation.
ex:claim-bad a gmeow:StatementMetadata ;
    rdfs:label "Bad statement"@en ;
    gmeow:qSubject ex:alice ;
    gmeow:qPredicate gmeow:knowsLanguage ;
    gmeow:qObject ex:lang-toki-pona ;
    gmeow:qObjectLiteral "toki"^^xsd:string ;
    gmeow:annotation
        [ gmeow:annProperty gmeow:confidence ; gmeow:annValue 0.6 ] .
"""


class TestMappingDslShacl:
    def test_malformed_term_equivalence_shacl_diagnostic(self, tmp_path: Path) -> None:
        """A TermEquivalence missing alignSubject must fail with a SHACL diagnostic."""
        (tmp_path / "test.ttl").write_text(_MALFORMED_MAPPING_TTL, encoding="utf-8")
        with pytest.raises(CompileError) as exc_info:
            load_dsl(tmp_path)
        msg = str(exc_info.value)
        assert "mapping DSL SHACL violations" in msg
        assert "focus=" in msg
        assert "path=" in msg
        assert "msg=" in msg
        assert "source=" in msg
        assert "alignSubject" in msg

    def test_valid_mapping_dsl_passes_shacl(self) -> None:
        """The real mapping DSL must pass SHACL validation without exception."""
        # load_dsl defaults to MAPPING_DSL_DIR — if this raises, the real DSL
        # is non-conformant to the new shapes (a bug in the shapes, not the DSL).
        dsl = load_dsl()
        assert len(dsl.equivalences) > 0
        assert len(dsl.projections) > 0


class TestStatementDslShacl:
    def test_malformed_statement_shacl_diagnostic(self, tmp_path: Path) -> None:
        """A StatementMetadata with both qObject and qObjectLiteral must fail."""
        (tmp_path / "test.ttl").write_text(_MALFORMED_STATEMENT_TTL, encoding="utf-8")
        with pytest.raises(CompileError) as exc_info:
            load_statement_dsl(tmp_path)
        msg = str(exc_info.value)
        assert "statement DSL SHACL violations" in msg
        assert "focus=" in msg
        assert "msg=" in msg
        assert "source=" in msg
        assert "qObject" in msg or "qObjectLiteral" in msg

    def test_valid_statement_dsl_passes_shacl(self) -> None:
        """The real statement DSL must pass SHACL validation without exception."""
        dsl = load_statement_dsl()
        assert len(dsl.cells) > 0
