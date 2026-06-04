<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# Contributing to GMEOW

Thanks for your interest in contributing. GMEOW (the Global Metadata and Entity
Ontology for the Web) and its publishing toolchain are maintained by Blackcat
Informatics® Inc. We welcome improvements to the ontology, the tooling, tests,
documentation, mappings, and release infrastructure.

## Code of Conduct

Please read the [Code of Conduct](CODE_OF_CONDUCT.md) before participating. We
expect respectful, professional collaboration. To report unacceptable behaviour,
email <conduct@blackcatinformatics.ca>.

## Ways to contribute

### Report bugs

Before opening an issue:

- Search existing issues first
- Verify the problem on the latest code in `main` when possible
- Capture a minimal reproduction

When filing a bug, include a clear title, exact reproduction steps, expected and
actual behaviour, relevant command output, and environment details (OS, Python
version, `uv` version, and whether Docker/ROBOT was involved).

### Suggest enhancements

Feature requests are welcome. Good enhancement reports describe the problem you
are trying to solve, the current limitation, the proposed behaviour, and concrete
examples of the expected result.

### Propose terms or mappings

For changes to the ontology itself — a new class/property, a refined definition,
or an alignment to an external vocabulary or Wikidata item — use the
**Term or mapping proposal** issue template. Vocabulary changes are reviewed for
OWL 2 DL conformance, gUFO grounding, and (for alignments) license compatibility.

### Submit pull requests

1. Fork the repository and create a branch from `main`.
2. Install dependencies with `make install` (`uv sync`).
3. Make the smallest coherent change that solves the problem.
4. Add or update tests when behaviour changes.
5. Update docs when outputs, flags, terms, or workflow change.
6. Complete the CLA Assistant check if prompted on your pull request.
7. Run the verification steps below before requesting review.

## Contributor License Agreement

External contributions are accepted under the project Contributor License
Agreement, enforced by CLA Assistant:

https://gist.github.com/paudley/55093187feb1a7cbc231e889ff6dda9e

When you open a pull request, CLA Assistant may comment with a signing link and
publish a status check. Follow that link and sign in with GitHub to accept the
agreement. After you accept it, CLA Assistant updates the pull request status.

The CLA confirms that you have the right to submit the contribution and grants
the project the rights needed to use and redistribute it — including, for the
dual-licensed work, the right for Blackcat Informatics to relicense it under the
separate proprietary terms described in [LICENSING.md](LICENSING.md). It does not
require you to provide support, updates, or future contributions.

The project also accepts Developer Certificate of Origin style sign-off trailers
as additional contribution evidence, but DCO sign-off does not replace the CLA
Assistant check when that check is required:

```text
Signed-off-by: Your Name <you@example.com>
```

## Governance and continuity

GMEOW is maintained by Blackcat Informatics® Inc. The project owners make
decisions through GitHub issues, pull requests, discussions, and release reviews.
The current project owners are listed in [`.github/CODEOWNERS`](.github/CODEOWNERS):

- `@paudley`
- `@ErinAudley`

Both project owners are directors of Blackcat Informatics® Inc. and have authority
to administer the repository for the company. This shared ownership is the
project's continuity mechanism: if one owner becomes unavailable, the other can
continue issue triage, pull request review, repository administration, and release
management.

## Development setup

### Prerequisites

- Git
- Python 3.13+
- `uv`
- Docker (for ROBOT/WIDOCO/Jena reasoning and documentation; the pure-Python
  steps run without it)

### Local setup

```bash
git clone https://github.com/<your-username>/gmeow-ontology.git
cd gmeow-ontology
make install
make pull-images   # optional: pre-pull the pinned Docker images
make help
```

## Project-specific guidance

- Author ontology changes in `ontology/modules/*.ttl`; every GMEOW term must
  carry an `rdfs:label` and a `skos:definition` and be grounded under a gUFO
  category. Keep the logical core in OWL 2 DL.
- Manage cross-ontology alignments as SSSOM tables in `mappings/`. Link by IRI
  freely; never copy axioms from a reference-only (NC/ND/share-alike/copyleft)
  source — the tooling refuses this by design.
- Keep RDF 1.2 / rdf-star content out of logical axioms; statement-level metadata
  belongs in OWL axiom annotations (the canonical layer).
- If the CLI or build outputs change, update [README.md](README.md).

## Coding style

- Python follows [PEP 8](https://peps.python.org/pep-0008/) and the stricter
  project rules enforced by Ruff and mypy (strict). Use Google-style docstrings
  and `pathlib.Path` (never bare path strings).
- Shell scripts must pass ShellCheck and start with `set -euo pipefail`.
- Turtle/OWL follows the conventions in the existing modules; run
  `make normalize` for canonical serialization when diffs get noisy.

The canonical verification command is `make check`.

## Verification

Before requesting review, make sure you:

- [ ] ran `make lint` (ruff + mypy)
- [ ] ran `make validate` (syntax, term annotations, SHACL)
- [ ] ran `make reason` after any ontology change (ELK consistency + OWL 2 DL profile)
- [ ] ran `make mappings` and `make wikidata` after any mapping change
- [ ] ran `uv run pytest`
- [ ] ran `make check` for the full repository gate
- [ ] updated tests for any behavioural change
- [ ] updated `README.md` if usage, flags, terms, or outputs changed

## Commit messages

We prefer [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` new functionality or new ontology terms
- `fix:` bug fixes
- `docs:` documentation-only changes
- `refactor:` internal restructuring without behaviour change
- `test:` test additions or updates
- `chore:` maintenance work

Examples:

```text
feat: add Account and credential classes to the entities module
fix: correct gUFO grounding for Agreement
docs: clarify the RDF 1.2 derived-view workflow
test: cover the Wikidata QID existence check
```

## Questions

For public questions, open an issue or discussion in the repository. For private
matters, email <oss@blackcatinformatics.ca>.

## License

GMEOW is dual-licensed (see [LICENSING.md](LICENSING.md)). By contributing, you
agree that your contributions to the tooling are licensed under
[Apache-2.0](LICENSE) and your contributions to the vocabulary under
[CC BY 4.0](LICENSE-ontology), subject to the CLA above.
