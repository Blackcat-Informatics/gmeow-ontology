# gmeow — grounded agent memory in five minutes

Store, recall, and revise what your agent learns — with confidence,
standpoint, and provenance attached to every claim, persisted in a single
verifiable file. No Docker, no database, no RDF knowledge required.

```bash
pip install gmeow
```

```python
from gmeow import Memory

mem = Memory("assistant.gts")                       # a GTS ai-package on disk

claim = mem.store(
    "Patrick prefers explicit error handling over exceptions-as-flow",
    source="conversation 2026-06-10",
    confidence=0.8,
    according_to="claude-fable-5",                  # the asserting standpoint
)

mem.recall("error handling preferences", min_confidence=0.5)

mem.revise(
    claim,
    reason="user stated the opposite for scripts",
    superseded_by=mem.store(
        "For one-off scripts Patrick is fine with exceptions-as-flow",
        confidence=0.9,
        according_to="claude-fable-5",
    ),
)
```

## What makes it different

- **Claims, not strings.** Every memory is a reified RDF 1.2 statement under
  the hood — confidence, asserting standpoint, source, and timestamp are
  first-class, so contradicting claims from different standpoints coexist
  instead of overwriting each other.
- **Revision is supersession, never deletion.** `revise` hides a claim and
  records what replaced it; the full audit trail stays in the file and is
  recoverable (`recall(..., include_suppressed=True)`).
- **The file is the proof.** An `assistant.gts` is a [GTS](https://github.com/Blackcat-Informatics/gmeow-ontology/blob/main/docs/GTS-SPEC.md)
  package: append-only, hash-chained, damage-detecting. Every write is a
  crash-safe byte-append; `mem.verify()` (or the `gts` CLI) attests integrity.
- **Graph when you want it.** `pip install 'gmeow[rdf]'` and
  `mem.to_rdflib()` gives an RDF 1.1 projection of the dataset (rdflib has no
  RDF 1.2 yet, so the quoted-triple binding lines are dropped — the loss is
  declared, and the GTS file itself stays full-fidelity); the
  [GMEOW ontology](https://blackcatinformatics.ca/gmeow/) gives the
  vocabulary meaning across tools.

## License

Apache-2.0. © Blackcat Informatics® Inc.
