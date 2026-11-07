# Issue #2615: Refinement readings: a link resolution and a time join (LinkResolution, TimeJoin, recoveredAt, timeJoinBasis)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, terms

## Body

Refinement readings: a link resolution and a time join.

lillith_memetics' refiner (process 2, linking and the time join) writes
two readings GMEOW has no class or properties for. They are defined
locally in its `lillith-observations` slice, same IRIs, until GMEOW
holds them.

1. `gmeow:LinkResolution`, a `gmeow:Refinement`: an observer's
   resolution of one reference a record makes (an identifier it
   carries, the record it replies to) against what the holder's stores
   hold. It uses existing terms for the rest: `gmeow:observedFeature`
   (the referring record), `gmeow:relationPredicate`,
   `gmeow:relatedThing` / `gmeow:relatedName` (the reference as
   written), `gmeow:refinementOutcome`, `gmeow:observationResult` and
   `gmeow:refinementCandidate`. GMEOW's refinement outcomes have no
   class for the reading itself, so a reader cannot tell a link
   resolution from any other refinement of the same record.
2. `gmeow:TimeJoin`, a `gmeow:Refinement`: a time recovered for a record
   a claim states without an assertion time, joined from a timed record
   it shares an identity with. New terms: `gmeow:recoveredAt`
   (`xsd:dateTime`, the join's time, never the record's own assertion
   time), `gmeow:timeJoinBasis` to a value vocabulary
   `gmeow:TimeJoinBasis` (`gmeow:timeJoinRecord`, the record's own timed
   claims; `gmeow:timeJoinTurn`, a shared `gmeow:chatTurn`;
   `gmeow:timeJoinPacket`, a shared `gmeow:originatingPacket`), and
   `gmeow:timeJoinedTo` (the timed record the time was read from).
   `gmeow:relationAt` keeps a source's own assertion-time value, so it
   cannot carry a recovered one.


## Comments (1)

### paudley — 2026-11-07T19:59:00Z

Addition: a fourth basis value, `gmeow:timeJoinText` (a `gmeow:TimeJoinBasis`): another record states the same text, of at least thirty characters, and holds an assertion time; the earliest such time. Order of preference: record, turn, text, packet.

