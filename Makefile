# GMEOW ontology toolchain — canonical task runner.
# Every target shells into the `gmeow` CLI (src/gmeow_tools) or the dev tools;
# no logic lives in this file. Run `make help` for the target list.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Alignment target for `make extract` (license-checked). Override: make extract TARGET=foaf
TARGET ?= foaf

# Checked-in generated artifacts (refreshed by make regenerate).
REGENERATED_PATHS := mappings/ projections/ queries/projections/ statements/ metadata/ apache/gmeow.conf dist/lpg/ dist/schemas/

# Stamp directory for tracking compilation freshness.
STAMPS_DIR := .stamps

# Source file lists (discovered via GNU Make wildcard — no $(shell)).
MAPPING_DSL_SRCS := $(wildcard mapping-dsl/*.ttl mapping-dsl/*/*.ttl)
STATEMENT_DSL_SRCS := $(wildcard statement-dsl/*.ttl)
ONTOLOGY_SRCS := $(wildcard ontology/*.ttl ontology/modules/*.ttl)
IMPORTS_SRCS := $(wildcard imports/*.ttl)

# Shared foundational modules used by almost all compilers.
COMMON_TOOL_SRCS := src/gmeow_tools/config.py src/gmeow_tools/graph.py src/gmeow_tools/self_desc.py

# Compiler-specific tool sources (granular — only rebuild when the relevant compiler changes).
MAPPING_COMPILE_SRCS := $(COMMON_TOOL_SRCS) $(wildcard src/gmeow_tools/mapping_*.py src/gmeow_tools/projection_lint.py)
STATEMENT_COMPILE_SRCS := $(COMMON_TOOL_SRCS) $(wildcard src/gmeow_tools/statement_*.py src/gmeow_tools/rdf12.py)
STATEMENT_PYOXIGRAPH_SRCS := $(COMMON_TOOL_SRCS) $(wildcard src/gmeow_tools/statement_*.py src/gmeow_tools/rdf12_pyoxigraph.py)
SCHEMA_COMPILE_SRCS := $(COMMON_TOOL_SRCS) $(wildcard src/gmeow_tools/schema_compile.py)
METADATA_COMPILE_SRCS := $(COMMON_TOOL_SRCS) $(wildcard src/gmeow_tools/metadata.py src/gmeow_tools/mappings.py)
APACHE_COMPILE_SRCS := $(COMMON_TOOL_SRCS) $(wildcard src/gmeow_tools/apache.py)
EXPORT_COMPILE_SRCS := $(COMMON_TOOL_SRCS) $(wildcard src/gmeow_tools/export.py src/gmeow_tools/jsonld_context.py src/gmeow_tools/mappings.py)
LPG_COMPILE_SRCS := $(COMMON_TOOL_SRCS) $(wildcard src/gmeow_tools/lpg.py)

# Override: make commit MESSAGE="feat: add foaf alignment"
MESSAGE ?= "chore: regenerate checked-in artifacts"

.PHONY: help install fmt lint validate crosscheck reason reason-hermit explain verify extract compile-mappings \
        compile-check compile-statements statements-check compile-statements-pyoxigraph statements-check-pyoxigraph \
        compile-schemas schemas-check mappings wikidata wikidata-live wikidata-coverage wikidata-audit \
        lint-alignment refresh-target-axioms metadata apache docs docs-full rdf12 rdf12-pyoxigraph quality \
        normalize build export lpg project test check release regenerate commit clean pull-images

help: ## Show this help.
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

install: ## Sync the uv environment (runtime + dev deps).
	uv sync

fmt: ## Auto-format with ruff.
	uv run ruff format .

lint: ## Lint (ruff), type-check (mypy), and full repo-hygiene suite (pre-commit).
	uv run ruff check .
	uv run ruff format --check .
	uv run mypy
	# Symmetry with CI: CI's `lint` job runs the whole pre-commit suite
	# (markdownlint, end-of-file-fixer, codespell, yamllint, shellcheck, …),
	# so the local gate must too — otherwise those lanes only fail in CI.
	uv run pre-commit run --all-files --show-diff-on-failure

validate: ## Validate syntax, term annotations, and SHACL (pure Python).
	uv run gmeow validate

crosscheck: ## Prove rdflib and pyoxigraph answer every committed query alike (no Docker).
	uv run gmeow crosscheck-queries

reason: ## Merge, validate OWL 2 DL profile, and check ELK consistency (Docker).
	uv run gmeow reason --reasoner ELK

reason-hermit: ## Sound + complete consistency check with HermiT (Docker).
	uv run gmeow reason --reasoner hermit

explain: ## Explain any unsatisfiable classes (HermiT, Docker).
	uv run gmeow explain

verify: ## Reasoned-graph negative tests (ROBOT verify over queries/verify/, Docker).
	uv run gmeow verify --reasoner ELK

extract: ## Report import/extract policy for TARGET (refuses reference-only).
	uv run gmeow extract --target $(TARGET)

# --------------------------------------------------------------------------- #
# Stamp-file compilation targets — no-ops when sources haven't changed.
# --------------------------------------------------------------------------- #

$(STAMPS_DIR)/compile-mappings: $(MAPPING_DSL_SRCS) $(ONTOLOGY_SRCS) $(IMPORTS_SRCS) $(MAPPING_COMPILE_SRCS)
	@mkdir -p $(STAMPS_DIR)
	uv run gmeow compile-mappings
	@touch $@

compile-mappings: $(STAMPS_DIR)/compile-mappings ## Compile the mapping DSL → SSSOM + EDOAL + FnO + SPARQL (in-place).

compile-check: compile-mappings ## Fail if the committed projection artifacts are stale vs the DSL.
	uv run gmeow compile-mappings --check

$(STAMPS_DIR)/compile-schemas: $(ONTOLOGY_SRCS) $(IMPORTS_SRCS) $(SCHEMA_COMPILE_SRCS)
	@mkdir -p $(STAMPS_DIR)
	uv run gmeow compile-schemas
	@touch $@

compile-schemas: $(STAMPS_DIR)/compile-schemas ## Compile canonical OWL → LinkML + JSON Schema / Pydantic / TS / GraphQL / OpenAPI.

schemas-check: compile-schemas ## Compile schemas as a sanity check (dist/ is git-ignored, so no drift gate).
	uv run gmeow compile-schemas --check

$(STAMPS_DIR)/compile-statements: $(STATEMENT_DSL_SRCS) $(ONTOLOGY_SRCS) $(IMPORTS_SRCS) $(STATEMENT_COMPILE_SRCS)
	@mkdir -p $(STAMPS_DIR)
	uv run gmeow compile-statements
	@touch $@

compile-statements: $(STAMPS_DIR)/compile-statements ## Compile statement-dsl/ → RDF 1.2 lead artifact + OWL downcast (Jena).

statements-check: compile-statements ## Fail if the committed statement artifacts are stale vs statement-dsl/ (Jena).
	uv run gmeow compile-statements --check

compile-statements-pyoxigraph: ## Compile statement-dsl/ → RDF 1.2 + OWL downcast (pyoxigraph cross-check).
	uv run gmeow compile-statements-pyoxigraph

statements-check-pyoxigraph: compile-statements ## Fail if pyoxigraph cross-check artifacts diverge from committed.
	uv run gmeow compile-statements-pyoxigraph --check

$(STAMPS_DIR)/metadata: $(STAMPS_DIR)/compile-mappings $(ONTOLOGY_SRCS) $(IMPORTS_SRCS) $(METADATA_COMPILE_SRCS)
	@mkdir -p $(STAMPS_DIR)
	uv run gmeow metadata
	@touch $@

metadata: $(STAMPS_DIR)/metadata ## Generate VoID + DCAT dataset descriptions.

$(STAMPS_DIR)/apache: $(ONTOLOGY_SRCS) $(APACHE_COMPILE_SRCS)
	@mkdir -p $(STAMPS_DIR)
	uv run gmeow apache
	@touch $@

apache: $(STAMPS_DIR)/apache ## Render the Apache content-negotiation include.

$(STAMPS_DIR)/export: $(STAMPS_DIR)/compile-mappings $(ONTOLOGY_SRCS) $(EXPORT_COMPILE_SRCS)
	@mkdir -p $(STAMPS_DIR)
	uv run gmeow export
	@touch $@

export: $(STAMPS_DIR)/export ## Generate flattened exports (CSV/CSVW, Markdown, JSONL, llms.txt) into dist/.

$(STAMPS_DIR)/lpg: $(STAMPS_DIR)/compile-statements $(LPG_COMPILE_SRCS)
	@mkdir -p $(STAMPS_DIR)
	uv run gmeow export lpg
	@touch $@

lpg: $(STAMPS_DIR)/lpg ## Export GMEOW to LPG formats (CSV, Neo4j, Cypher, GraphML) into dist/lpg/.

mappings-only: ## Build alignment axioms + VoID linksets (assumes SSSOM files present).
	uv run gmeow mappings

mappings: compile-mappings ## Build alignment axioms + VoID linksets from SSSOM; validate QID syntax.
	uv run gmeow mappings

lint-alignment: ## Lint SSSOM mappings for inverse / domain-range-mismatched targets (offline).
	uv run gmeow lint-alignment

refresh-target-axioms: ## Re-vendor minimal target-axiom snapshots (IMPORT_OK targets only).
	uv run gmeow refresh-target-axioms --target all

wikidata: ## Validate Wikidata QID/PID syntax in the mappings (offline).
	uv run gmeow wikidata

wikidata-live: ## Also verify Wikidata ids resolve (network).
	uv run gmeow wikidata --existence

wikidata-coverage: ## Report Wikidata mapping coverage by domain (offline).
	uv run gmeow wikidata-coverage

wikidata-audit: ## Audit fixtures and modules for Wikidata misuse (offline).
	uv run gmeow wikidata --fixtures

coverage: ## Report how much of the vendored entity slice GMEOW covers.
	uv run gmeow coverage --gaps

crossref: ## Generate the CrossRef DOI deposit XML.
	uv run gmeow crossref

docs: ## Generate pyLODE HTML documentation.
	uv run gmeow docs

docs-full: ## Generate pyLODE + WIDOCO documentation (Docker).
	uv run gmeow docs --widoco

rdf12: compile-statements ## Emit the RDF 1.2 / RDF* lead artifact + OWL downcast (alias of compile-statements; Jena).

rdf12-pyoxigraph: compile-statements-pyoxigraph ## Emit the RDF 1.2 / RDF* lead artifact + OWL downcast (pyoxigraph alias).

quality: ## Run OOPS! pitfall scan (network, best-effort).
	uv run gmeow quality

normalize: ## Canonicalize the authored ontology sources (rewrites files).
	uv run gmeow normalize

build: ## Build all serializations + JSON-LD context + apache.conf into dist/.
	uv run gmeow build

project: compile-mappings ## Project GMEOW data to pure schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profiles (FnO/EDOAL).
	uv run gmeow project

test: ## Run the test suite.
	uv run pytest -n auto

check: regenerate ## Full local quality gate (parallelized where safe).
	$(MAKE) -j$$(nproc 2>/dev/null || echo 4) lint validate crosscheck statements-check statements-check-pyoxigraph compile-check wikidata coverage reason reason-hermit verify mappings-only lint-alignment
	$(MAKE) test
	$(MAKE) schemas-check
	@echo "✓ all checks passed"

release: ## RDF 1.2 + OWL downcast → reasoned closure (HermiT) + build + metadata + CrossRef deposit.
	uv run gmeow compile-statements
	uv run gmeow reason --reasoner hermit --full
	uv run gmeow build
	uv run gmeow metadata
	uv run gmeow crossref

regenerate: compile-mappings compile-statements compile-schemas metadata apache export lpg ## Rebuild all checked-in generated artifacts from canonical sources.

commit: regenerate ## Regenerate artifacts, stage them, and commit.
	git add $(REGENERATED_PATHS)
	@if git diff --cached --quiet; then \
		echo "Nothing to commit."; exit 1; \
	else \
		git commit -m "$(MESSAGE)"; \
	fi
	@git diff --quiet || echo "Warning: unstaged changes remain. Stage them with 'git add' and commit separately if needed."

pull-images: ## Pre-pull the pinned Docker images (ROBOT, WIDOCO, Jena).
	bash scripts/pull-images.sh

clean: ## Remove generated artifacts.
	rm -rf dist docs/_generated $(STAMPS_DIR)
	@echo "✓ cleaned"
