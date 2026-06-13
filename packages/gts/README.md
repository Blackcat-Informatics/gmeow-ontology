# gts — Graph Transport Substrate

A single-file, language-independent transport for an **RDF 1.2** graph
(statements *and* statement-level metadata) together with any
content-addressed binary the graph references.

A GTS file is a CBOR Sequence of one or more **segments**, each an append-only
log: a header followed by frames chained by BLAKE3 content-id. Composition is
`cat` — concatenating valid GTS files yields a valid GTS file whose fold is
the value-union of the segment graphs.

This package is the Python reference implementation of the
[GTS specification](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/GTS-SPEC.md):
reader (fold, chain verification, opaque degradation, torn-append detection),
writer, COSE signing, N-Quads projection, and the frozen language-neutral
conformance corpus. A Rust engine implementing the same spec is gated against
the identical corpus and will ship inside this package as a native wheel.

## Install

```bash
pip install gmeow-gts
```

The installed package name is `gmeow-gts`; the import name and CLI binary both
remain `gts`, and GTS files keep the `.gts` extension.

## Library

```python
from pathlib import Path

import gts

graph = gts.read(Path("package.gts").read_bytes())
print(gts.to_nquads(graph))
```

## Command line

```text
gts info <file>...            per-segment composition ledger
gts fold <file>               fold to N-Quads on stdout
gts verify <file>...          verify chains; exit 1 on any diagnostic
gts cat -o <out> <file>...    validating composer: refuse degenerate inputs,
                              then byte-concatenate
```

`cat` output is the raw byte concatenation — validation added, transformation
never. It refuses dirty inputs, contributes-nothing segments, and compositions
whose suppressions hide every folded quad.

## Example: grounded agent memory

The `gts.examples.agent_memory` module shows how to build a tiny claim store
on top of GTS: every claim is a reified RDF 1.2 statement with confidence,
standpoint, source, and timestamp; revision is supersession, never deletion;
the file is always a valid, `gts verify`-able package.

```bash
pip install gmeow-gts
python -m gts.examples.agent_memory
```

```python
from gts.examples.agent_memory import Memory

mem = Memory("assistant.gts")
mem.store(
    "Patrick prefers explicit error handling over exceptions-as-flow",
    source="conversation 2026-06-10",
    confidence=0.8,
    according_to="claude-fable-5",
)
print([c.text for c in mem.recall("error handling")])
```

For `rdflib` interop, install the optional `rdf` extra:

```bash
pip install 'gmeow-gts[rdf]'
```

## License

Apache-2.0. © Blackcat Informatics® Inc.
