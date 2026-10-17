# Lossless DL service boundary

The pinned PurRDF 7734b013 exposes Certified<T>, its complete DlCertificate and
optional ServiceProof. GMEOW currently calls into_parts (explicitly discarding
proof), then copies only completeness, boundaries, rounds, budget and decision
count into CertifiedAnswer. Work counts, preparation/storage/cancellation evidence,
search-shape measurements and proof are lost at this internal boundary.

Remove CertifiedAnswer entirely and return the upstream Certified<T> from every
service. Update all actual CLI and test callers to use answer()/certificate();
keep the exact existing CLI grading rules through the upstream completeness API.
Retain the GMEOW diagnostic mapper. Do not add a compatibility alias, duplicate
certificate, new engine, dependency, hidden recording switch or fabricated proof.

Expose explicit proof-recording construction and its native proof context through
the existing diagnostic adapter, needed by the selected native delegation work.
The nonrecording constructor stays an explicit upstream capability selection;
absence of recording is not an empty proof. Boundary controls must prove a
recorded result retains its proof and full certificate and can be checked against
its actual source/question. Preserve malformed/no-model/execution distinctions.

Use focused synthetic service and artifact/result tests, strict producer-independent
Clippy and normal hooks. Full authenticated CLI qualification remains hosted.
This is a source/proof-fidelity prerequisite for 1803 and the original 1770 plan,
not full class-engine delegation, prepared-canon completion or a speedup claim.
