<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Contributing to GMEOW

Thanks for your interest in contributing. GMEOW (the Global Metadata and Entity
Ontology for the Web) and its publishing toolchain are maintained by Blackcat
Informatics® Inc. We welcome improvements to the ontology, the tooling, tests,
documentation, mappings, and release infrastructure.

## Code of Conduct

Please read the [Code of Conduct](CODE_OF_CONDUCT.md) before participating. We
expect respectful, professional collaboration. To report unacceptable behaviour,
email <conduct@blackcatinformatics.ca>.

## Principles

GMEOW is governed by [`CONSTITUTION.md`](CONSTITUTION.md) — twelve normative principles every
design decision and pull request is measured against. Read it before proposing terms, mappings,
or tooling changes, and cite the relevant principle(s) by number in issues and PRs.

## Ways to contribute

### Report bugs

Before opening an issue:

- Search existing issues first
- Verify the problem on the latest code in `main` when possible
- Capture a minimal reproduction

When filing a bug, include a clear title, exact reproduction steps, expected and
actual behaviour, relevant command output, and environment details (OS, Rust
toolchain version, and whether Docker/ROBOT was involved).

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
2. Install dependencies with `make install`.
3. Make the smallest coherent change that solves the problem.
4. Add or update tests when behaviour changes.
5. Update docs when outputs, flags, terms, or workflow change.
6. Cite the [Constitution](CONSTITUTION.md) principle(s) your change embodies or affects (by
   number) in the PR description. A change that appears to conflict with a principle must be
   revised to comply or include the amendment to `CONSTITUTION.md` that permits it.
7. Complete the CLA Assistant check if prompted on your pull request.
8. Run the verification steps below before requesting review.

## Contributor License Agreement

External contributions are accepted under the project Contributor License
Agreement, enforced by CLA Assistant:

https://gist.github.com/paudley/55093187feb1a7cbc231e889ff6dda9e

When you open a pull request, CLA Assistant may comment with a signing link and
publish a status check. Follow that link and sign in with GitHub to accept the
agreement. After you accept it, CLA Assistant updates the pull request status.

The CLA confirms that you have the right to submit the contribution and grants
the project the rights needed to use and redistribute it — including, for the
dual-licensed work, the right for Blackcat Informatics® to relicense it under the
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
- A recent stable Rust toolchain (`cargo`)
- Docker (for the ROBOT `extract` / WIDOCO documentation tooling; native
  reasoning and the native pipeline steps all run without it)

### Local setup

```bash
git clone https://github.com/<your-username>/gmeow-ontology.git
cd gmeow-ontology
make install
make help
```

## Project-specific guidance

- Author ontology changes in `ontology/modules/*.ttl`; every GMEOW term must
  carry an `rdfs:label` and a `skos:definition` and be grounded under a gUFO
  category. Keep the logical core in OWL 2 DL.
- Author cross-ontology alignments **in the mapping DSL** under `mapping-dsl/`
  (`equivalences/` for 1:1 SSSOM links, `projections/` for the lossy downcasts),
  then run `make check`. The `mappings/*.sssom.tsv`,
  `projections/*.edoal.ttl`, `projections/functions.fno.ttl`, and
  `queries/projections/*.rq` are **generated — do not edit them by hand** (CI's
  `make check-sync` fails on drift). Link by IRI freely; never copy axioms
  from a reference-only (NC/ND/share-alike/copyleft) source — the tooling refuses
  this by design.
- Statement-level metadata is **RDF 1.2 / RDF\*** in GMEOW's model, and it is the
  **canonical** form (Principles 2–3). Author it once in `dsl/statements/` — the
  RDF 1.2-shaped Turtle DSL — then run `make check`. The RDF 1.2 / RDF\*
  serialization **and** the OWL 2 axiom-annotation form (`owl:Axiom` +
  `owl:annotatedSource/Property/Target`) are both **generated — do not hand-author
  either** (CI's `make check-sync` fails on drift). The OWL form is the
  reasoning-lossless downcast the OWL 2 DL reasoners consume; the logical TBox stays
  OWL 2 DL.
- If the CLI or build outputs change, update [README.md](README.md).

## Coding style

- Rust follows the standard formatting and lint rules enforced by `cargo fmt`
  and `cargo clippy` (warnings-as-errors).
- Shell scripts must pass ShellCheck and start with `set -euo pipefail`.
- Turtle/OWL follows the conventions in the existing modules; run
  `make normalize` for canonical serialization when diffs get noisy.

The canonical verification command is `make check`.

## Verification

Before requesting review, make sure you:

- [ ] ran `make lint`
- [ ] ran `make validate` (syntax, term annotations, SHACL)
- [ ] ran `make reason` after any ontology change (native EL/DL profile)
- [ ] ran `make mappings` and `make wikidata` after any `mapping-dsl/` change
- [ ] ran `make rust-test`
- [ ] ran `make check` for the full repository gate — it materializes
      `generated/` itself (through its single producer, `check-sync`) and then
      gates, so it is the ONLY command needed after a canonical-source change;
      never regenerate first and gate second (that runs the pipeline twice
      against one host-global lock, which is why `make regen` refuses)
- [ ] updated tests for any behavioural change
- [ ] updated `README.md` if usage, flags, terms, or outputs changed
- [ ] cited the affected Constitution principle(s) in the PR description

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
docs: document the RDF 1.2-first authoring workflow
test: cover the Wikidata QID existence check
```

## Questions

For public questions, open an issue or discussion in the repository. For private
matters, email <oss@blackcatinformatics.ca>.

## License

GMEOW is dual-licensed (see [LICENSING.md](LICENSING.md)). By contributing, you agree:

- Contributions to GMEOW tooling/code are accepted under
  [AGPL-3.0-only](LICENSE) and, under the project CLA, under terms that permit
  Blackcat Informatics® Inc. to relicense them under separate
  proprietary/commercial terms.
- Contributions to ontology content, slices, mappings, and published vocabulary
  artifacts are accepted under [CC-BY-4.0](LICENSE-ontology) and, where required
  by the CLA, under terms that permit Blackcat Informatics® Inc. to publish,
  sublicense, and commercially license the contributed material.
- Contributions to [`gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts)
  are accepted under Apache-2.0 OR MIT and, under the project CLA, under terms that
  permit separate proprietary/commercial licensing.
