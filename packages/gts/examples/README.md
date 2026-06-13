# GTS agent-memory example

A small, runnable example of **grounded agent memory** built directly on the
`gmeow-gts` engine.

Install the engine:

```bash
pip install gmeow-gts
```

Run the example:

```bash
python -m gts.examples.agent_memory
```

Or import it in your own code:

```python
from gts.examples.agent_memory import Memory

mem = Memory("assistant.gts")
mem.store("Cats are mammals", confidence=0.99, according_to="zoology")
print([c.text for c in mem.recall("cats")])
```

For `rdflib` interop, install the optional `rdf` extra:

```bash
pip install 'gmeow-gts[rdf]'
```

See the source in [`../src/gts/examples/agent_memory.py`](../src/gts/examples/agent_memory.py).
