# GMEOW ontology toolchain — canonical task runner.
# Every target shells into the `gmeow` CLI (src/gmeow_tools) or the dev tools;
# no logic lives in this file. Run `make help` for the target list.

.DEFAULT_GOAL := help
SHELL := /bin/bash

# Alignment target for `make extract` (license-checked). Override: make extract TARGET=foaf
TARGET ?= foaf

.PHONY: help install fmt lint validate reason explain extract mappings wikidata \
        wikidata-live metadata apache docs docs-full rdf12 quality normalize \
        build test check release clean pull-images

help: ## Show this help.
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-14s\033[0m %s\n", $$1, $$2}'

install: ## Sync the uv environment (runtime + dev deps).
	uv sync

fmt: ## Auto-format with ruff.
	uv run ruff format .

lint: ## Lint (ruff) and type-check (mypy).
	uv run ruff check .
	uv run ruff format --check .
	uv run mypy

validate: ## Validate syntax, term annotations, and SHACL (pure Python).
	uv run gmeow validate

reason: ## Merge, validate OWL 2 DL profile, and check ELK consistency (Docker).
	uv run gmeow reason --reasoner ELK

explain: ## Explain any unsatisfiable classes (HermiT, Docker).
	uv run gmeow explain

extract: ## Report import/extract policy for TARGET (refuses reference-only).
	uv run gmeow extract --target $(TARGET)

mappings: ## Build alignment axioms + VoID linksets from SSSOM; validate QID syntax.
	uv run gmeow mappings

wikidata: ## Validate Wikidata QID/PID syntax in the mappings (offline).
	uv run gmeow wikidata

wikidata-live: ## Also verify Wikidata ids resolve (network).
	uv run gmeow wikidata --existence

metadata: ## Generate VoID + DCAT dataset descriptions.
	uv run gmeow metadata

coverage: ## Report how much of the vendored entity slice GMEOW covers.
	uv run gmeow coverage --gaps

apache: ## Render the Apache content-negotiation include.
	uv run gmeow apache

crossref: ## Generate the CrossRef DOI deposit XML.
	uv run gmeow crossref

docs: ## Generate pyLODE HTML documentation.
	uv run gmeow docs

docs-full: ## Generate pyLODE + WIDOCO documentation (Docker).
	uv run gmeow docs --widoco

rdf12: ## Project the RDF 1.2 / rdf-star preview view (Jena, gated/skips if absent).
	uv run gmeow rdf12

quality: ## Run OOPS! pitfall scan (network, best-effort).
	uv run gmeow quality

normalize: ## Canonicalize the authored ontology sources (rewrites files).
	uv run gmeow normalize

build: ## Build all serializations + JSON-LD context + apache.conf into dist/.
	uv run gmeow build

export: ## Generate flattened exports (CSV/CSVW, Markdown, JSONL, llms.txt) into dist/.
	uv run gmeow export

test: ## Run the test suite.
	uv run pytest

check: lint validate reason mappings wikidata coverage test ## Full local quality gate.
	@echo "✓ all checks passed"

release: ## Reasoned closure (HermiT) + build + metadata + CrossRef deposit + RDF 1.2.
	uv run gmeow reason --reasoner hermit --full
	uv run gmeow build
	uv run gmeow metadata
	uv run gmeow crossref
	uv run gmeow rdf12

pull-images: ## Pre-pull the pinned Docker images (ROBOT, WIDOCO, Jena).
	bash scripts/pull-images.sh

clean: ## Remove generated artifacts.
	rm -rf dist docs/_generated
	@echo "✓ cleaned"
