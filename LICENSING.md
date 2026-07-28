# Licensing

GMEOW is **dual-licensed**. Blackcat Informatics® Inc. is the sole copyright
holder (© 2026) and makes the work available under the open-source terms below
**and** reserves the right to grant separate commercial/proprietary licenses.

## Open-source terms

| Component | Scope | License |
|---|---|---|
| **Tooling code & Rust core** | This repository, excluding the vocabulary (`src/`, `crates/`, tests, build scripts, CI, docs tooling) | [AGPL-3.0-only](./LICENSE) |
| **GMEOW vocabulary** | The ontology in `ontology/`, the slices, mappings, and its published serializations | [CC BY 4.0](./LICENSE-ontology) |
| **Documentation** | The prose docs (`docs/`, `*.md`, slice `docs.md`) | [CC BY 4.0](./LICENSE-ontology) |

The **GTS format engine** lives in the separate
[`gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts) repository (consumed
here as a published package) and is licensed **Apache-2.0 OR MIT**; it is not covered by
the AGPL terms above.

The **RDF 1.2 kernel** lives in the separate
[`purrdf`](https://github.com/Blackcat-Informatics/purrdf) repository and is licensed
**MIT OR Apache-2.0**; it is consumed here as a Rust library and is not covered by the
AGPL terms above. No purrdf build is vendored into this repository: the documentation
site's interactive surfaces run on the GMEOW-owned MCP wasm segments under
`crates/docs/assets/mcp-core/` and `crates/docs/assets/mcp/`, which are AGPL-3.0-only like
the rest of this repository.

The vendored third-party file `imports/gufo.ttl` (gUFO) is under the MIT License;
its notice is preserved in that file and summarized in [`NOTICE`](./NOTICE).

## Proprietary / commercial licensing

The open licenses above are offered **in addition to — not in place of** —
Blackcat Informatics®' right, as copyright holder, to license either the tooling
code or the vocabulary under separate commercial or proprietary terms. Granting
the open licenses does not revoke or limit this reservation.

To obtain a proprietary license, contact **licensing@blackcatinformatics.ca**.

## Trademarks

"BLACKCAT INFORMATICS" (word mark, CIPO TMA1066935) and the black-cat-silhouette &
Sierpinski-triangle design mark (CIPO TMA1233860) are registered trademarks of
Blackcat Informatics® Inc. "GMEOW" is **not** a trademark. Neither open license
grants any right to use these marks or logos — the **AGPL-3.0** grants no trademark
rights, and see **CC BY 4.0 §2(b)**. Nominative references (e.g. "compatible with
GMEOW") are permitted; uses implying endorsement or origin are not.

## Contributions

Contributions to GMEOW tooling/code are accepted under AGPL-3.0-only and, under the
project CLA, under terms that permit Blackcat Informatics® Inc. to relicense them
under separate proprietary/commercial terms.

Contributions to ontology content, slices, mappings, and published vocabulary
artifacts are accepted under CC-BY-4.0 and, where required by the CLA, under terms
that permit Blackcat Informatics® Inc. to publish, sublicense, and commercially
license the contributed material.

Contributions to gmeow-gts are accepted under Apache-2.0 OR MIT and, under the
project CLA, under terms that permit separate proprietary/commercial licensing.

A Contributor License Agreement may be required before substantial contributions
are merged.

## Copyright notice

> Copyright © 2026 Blackcat Informatics® Inc. All rights reserved, except as
> expressly granted under the licenses above.
