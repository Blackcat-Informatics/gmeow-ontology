// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The COMPOSITIONAL identity law of the medium: for every DECLARED chain,
//! `decode ∘ encode = id`.
//!
//! `medium_bundle` audits the ONE producer-authenticated shipped artifact read-only.
//! This suite proves the complementary law about the MEDIUM without rerunning the DAG:
//! every chain the registry declares round-trips over an input space chosen to cross
//! every codec boundary, so a future rep, dictionary, or medium inherits the guarantee.
//!
//! # What "declared" means here
//!
//! The chains are READ off `slices/core/gts/module.ttl` through the production
//! [`MediumRegistry`] — `gmeow:mediumCodec` resolved to its wire name by the same
//! `codec_wire_name` the emitter and the measurement use, at the medium's own
//! `gmeow:mediumZstdLevel`, crossed with the dictionary bound
//! (`gmeow:mediumDictionary`) each medium declares. Enumerating the chains in the test
//! instead would put a second source of truth beside the declaration and would keep
//! passing after a medium's codec changed.
//!
//! # What backs the input corpus
//!
//! Two sources, because generated inputs alone would only ever exercise shapes the test
//! author thought of:
//!
//! * a DECLARED generator ([`generated_inputs`]) whose cases sit exactly on the
//!   `zstd-rsyncable` 64 KiB cut grid (one byte under, on, one over, and two blocks
//!   plus a remainder) and span the entropy range from a single repeated byte to
//!   incompressible pseudo-random bytes;
//! * the committed FROZEN corpora ([`FROZEN_CORPORA`]) — the GTS wire seed (read as its
//!   own on-wire frames as well as whole), the fuzz seed corpus, the 50 GMN-1
//!   conformance vectors and the transcode conformance corpus. See that constant for what
//!   is and is not claimed about purrdf's own `vectors/` corpus.
//!
//! # Why the dictionary arm is a separate law
//!
//! Priming changes the CODE, not the framing. An unprimed decode of a primed payload does
//! not fail cleanly — it yields plausible-looking bytes no checksum inside the payload
//! would catch — so "round-trips under its own dictionary" is only half the claim. The
//! other half, asserted below, is that the WRONG priming does not silently succeed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gmeow_pipeline::medium::registry::{DictionaryStrategy, MediumRegistry};
use gmeow_pipeline::medium::train;
use purrdf::gts::codec::{Codec, EncodeOptions, decode_chain, encode_chain_with_options};

/// The `zstd-rsyncable` block size the GTS spec fixes (§8.4). Not imported: it is the
/// boundary the input generator is DEFINED against, and a silent upstream change to it
/// must show up as cases that no longer straddle a cut rather than as cases that quietly
/// moved with it.
const RSYNCABLE_BLOCK_SIZE: usize = 65_536;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// The live medium axis, read off the authored slice.
///
/// A unit-scale test has no carrier to read, so the declaration is parsed here; the
/// production path always reads the in-memory dataset, and
/// `medium::registry::tests::the_live_gts_slice_reads_as_a_complete_registry` is where
/// the two are proved to agree.
fn live_registry() -> MediumRegistry {
    let module = repo_root().join("slices/core/gts/module.ttl");
    let text = std::fs::read_to_string(&module).expect("the gts slice is readable");
    let dataset = purrdf::parse_dataset(
        text.as_bytes(),
        "text/turtle",
        Some("https://blackcatinformatics.ca/gmeow/"),
    )
    .expect("the gts slice parses as Turtle");
    MediumRegistry::from_dataset(&dataset).expect("the live medium axis reads")
}

/// One declared chain: its wire codec names in encode order, and the level the medium
/// declares.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DeclaredChain {
    medium: String,
    names: Vec<String>,
    level: i32,
}

/// Every chain the medium registry declares, deduplicated by `(names, level)` but
/// carrying the medium that declared it for the failure message.
fn declared_chains(registry: &MediumRegistry) -> Vec<DeclaredChain> {
    let mut out: Vec<DeclaredChain> = registry
        .media()
        .values()
        .map(|medium| DeclaredChain {
            medium: medium.iri.clone(),
            names: vec![
                medium
                    .codec_wire_name()
                    .unwrap_or_else(|err| {
                        panic!("<{}> declares an unspellable codec: {err}", medium.iri)
                    })
                    .to_string(),
            ],
            level: medium.zstd_level,
        })
        .collect();
    out.sort();
    out.dedup_by(|a, b| a.names == b.names && a.level == b.level);
    out
}

/// The decode-side catalog entry for a chain, optionally primed.
fn decode_catalog(chain: &DeclaredChain, dict: Option<&[u8]>) -> Vec<Codec> {
    chain
        .names
        .iter()
        .map(|name| Codec {
            name: name.clone(),
            cls: "compress".to_string(),
            dct: dict.map(<[u8]>::to_vec),
            level: Some(chain.level),
        })
        .collect()
}

/// A deterministic, dependency-free pseudo-random byte stream (a 64-bit xorshift). Its
/// output is incompressible enough to exercise the expansion arm of the codec, and it is
/// reproducible from `seed` alone so a failure is replayable.
fn pseudo_random(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.extend_from_slice(&state.to_le_bytes());
    }
    out.truncate(len);
    out
}

/// The DECLARED generator: every input case, with the reason it is in the corpus.
///
/// The lengths straddle the `zstd-rsyncable` cut grid deliberately. A chain that lost the
/// grid (one frame instead of N, or an off-by-one block split) round-trips perfectly on a
/// short input and fails only where a block boundary lands, so a corpus that never
/// crossed 64 KiB would prove the law exactly where it cannot break.
fn generated_inputs() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = vec![
        ("empty".to_string(), Vec::new()),
        ("one-byte".to_string(), vec![0x47]),
        ("one-nul".to_string(), vec![0]),
        (
            "all-256-byte-values".to_string(),
            (0..=255u8).collect::<Vec<u8>>(),
        ),
    ];
    for (label, len) in [
        ("block-minus-one", RSYNCABLE_BLOCK_SIZE - 1),
        ("block-exact", RSYNCABLE_BLOCK_SIZE),
        ("block-plus-one", RSYNCABLE_BLOCK_SIZE + 1),
        ("two-blocks-plus-remainder", RSYNCABLE_BLOCK_SIZE * 2 + 977),
    ] {
        // Maximally compressible: one repeated byte.
        out.push((format!("{label}/uniform"), vec![0xA5; len]));
        // Incompressible: pseudo-random.
        out.push((format!("{label}/random"), pseudo_random(0x9E37_79B9, len)));
        // RDF-shaped, the population the medium actually codes.
        let mut rdf = Vec::with_capacity(len + 128);
        let mut index = 0u32;
        while rdf.len() < len {
            rdf.extend_from_slice(
                format!(
                    "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {index} in the gmeow ontology\" .\n",
                    index % 41
                )
                .as_bytes(),
            );
            index += 1;
        }
        rdf.truncate(len);
        out.push((format!("{label}/rdf"), rdf));
    }
    out
}

/// The committed, FROZEN corpora this law is exercised against, in canonical order.
///
/// Each is a frozen conformance/seed corpus the repository already ships and already
/// treats as immutable evidence, so the medium's identity law is measured over material
/// the build actually produced and consumes rather than only over shapes this file
/// invented. (purrdf's own `vectors/` GTS conformance corpus lives in the upstream
/// repository and is not vendored here; what IS reused verbatim is the codec — every
/// encode and decode below goes through `purrdf::gts::codec`, the exact entry points the
/// terminal emitter uses, so the byte-level behaviour under test is upstream's frozen
/// implementation and never a re-implementation.)
const FROZEN_CORPORA: &[(&str, &str)] = &[
    ("gts-wire", "fuzz/seeds/gts"),
    ("fuzz-seeds", "fuzz/seeds"),
    ("gmn1-vectors", "slices/grounding/lang/tests/gmn1-vectors"),
    ("transcode-corpus", "crates/pipeline/tests/transcode_corpus"),
];

/// Every regular file under `dir`, recursively, in sorted order.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| !path.is_symlink())
        .collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// The FROZEN input corpus: every committed file of [`FROZEN_CORPORA`], plus the
/// individual on-wire frame payloads of the committed GTS artifact.
///
/// The GTS seed is read as RAW frames rather than folded back through the reader on
/// purpose: it is a deliberately TRUNCATED head (it is a fuzz seed), so a strict fold
/// would refuse it, and what this law needs from it is byte material the emitter actually
/// produced — not a readable graph.
fn frozen_inputs() -> Vec<(String, Vec<u8>)> {
    use ciborium::value::Value;
    use purrdf::gts::wire::{iter_items, map_get};

    let root = repo_root();
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    for (label, rel) in FROZEN_CORPORA {
        let dir = root.join(rel);
        let mut files = Vec::new();
        collect_files(&dir, &mut files);
        assert!(
            !files.is_empty(),
            "the frozen corpus {label} at {rel} is a committed input of this gate — an empty \
             (or missing) corpus is a HARD FAIL rather than a smaller law"
        );
        for path in files {
            let bytes = std::fs::read(&path).unwrap_or_else(|err| {
                panic!("{}: unreadable frozen vector: {err}", path.display())
            });
            if bytes.is_empty() {
                continue;
            }
            let name = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            // The GTS wire artifact contributes its own frames as well as its whole bytes.
            if path.extension().is_some_and(|ext| ext == "gts") {
                let (items, _torn) = iter_items(&bytes);
                for (offset, item) in &items {
                    let Value::Map(entries) = item else { continue };
                    if let Some(Value::Bytes(payload)) = map_get(entries, "d")
                        && !payload.is_empty()
                    {
                        out.push((format!("{name}#frame@{offset}"), payload.clone()));
                    }
                }
            }
            out.push((name, bytes));
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// One dictionary per declared strategy, trained over the generated RDF-shaped corpus —
/// the priming arm's inputs.
fn training_dictionaries() -> Vec<(String, Vec<u8>)> {
    let owned: Vec<Vec<u8>> = (0..512u32)
        .map(|index| {
            format!(
                "<https://blackcatinformatics.ca/gmeow/term{}> \
                 <https://blackcatinformatics.ca/gmeow/definition> \
                 \"a definition of term {index} in the gmeow ontology\" .\n",
                index % 41
            )
            .into_bytes()
        })
        .collect();
    let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
    [
        DictionaryStrategy::Trained,
        DictionaryStrategy::RawContent,
        DictionaryStrategy::TermTable,
    ]
    .into_iter()
    .map(|strategy| {
        (
            strategy.to_string(),
            train::build(strategy, &corpus, 4096)
                .unwrap_or_else(|err| panic!("the {strategy} dictionary trains: {err}")),
        )
    })
    .collect()
}

#[test]
fn every_declared_chain_round_trips_over_the_generated_and_frozen_corpora() {
    let registry = live_registry();
    let chains = declared_chains(&registry);
    assert!(
        !chains.is_empty(),
        "the medium registry declares no chain — the law below would hold vacuously"
    );
    // Non-vacuity in the other direction: the mandated chain must be among them, or the
    // property is being proved about a codec the bundle does not write.
    let names: BTreeSet<&str> = chains
        .iter()
        .flat_map(|chain| chain.names.iter().map(String::as_str))
        .collect();
    assert!(
        names.contains("zstd-rsyncable"),
        "the mandated chain is absent from the declared set {names:?}"
    );
    for chain in &chains {
        assert_eq!(
            chain.level, 12,
            "<{}> declares level {} — the property is measured on the MANDATED chain, so a \
             level the bundle does not write would describe bytes nobody ships",
            chain.medium, chain.level
        );
    }

    let mut inputs = generated_inputs();
    let frozen = frozen_inputs();
    assert!(
        frozen.len() >= 64,
        "the frozen corpora contributed {} case(s) — too thin to back the generated ones",
        frozen.len()
    );
    let frozen_bytes: usize = frozen.iter().map(|(_, bytes)| bytes.len()).sum();
    println!(
        "corpus: {} generated case(s), {} frozen case(s) / {frozen_bytes} B",
        inputs.len(),
        frozen.len()
    );
    inputs.extend(frozen);

    let dictionaries = training_dictionaries();
    let mut checked = 0usize;
    for chain in &chains {
        for (label, input) in &inputs {
            // The UNPRIMED arm.
            let encoded = encode_chain_with_options(
                &chain.names,
                input,
                EncodeOptions {
                    zstd_level: Some(chain.level),
                    dict: None,
                },
            )
            .unwrap_or_else(|err| {
                panic!("{label} through {:?}: encode failed: {err}", chain.names)
            });
            let decoded =
                decode_chain(&decode_catalog(chain, None), &encoded).unwrap_or_else(|err| {
                    panic!("{label} through {:?}: decode failed: {err}", chain.names)
                });
            assert_eq!(
                decoded.len(),
                input.len(),
                "{label} through {:?}: the round trip changed the payload LENGTH",
                chain.names
            );
            assert_eq!(
                blake3::hash(&decoded),
                blake3::hash(input),
                "{label} through {:?}: dec ∘ enc is not the identity",
                chain.names
            );
            checked += 1;

            // The PRIMED arm, once per declared dictionary strategy.
            for (strategy, dict) in &dictionaries {
                let encoded = encode_chain_with_options(
                    &chain.names,
                    input,
                    EncodeOptions {
                        zstd_level: Some(chain.level),
                        dict: Some(dict),
                    },
                )
                .unwrap_or_else(|err| {
                    panic!(
                        "{label} through {:?} primed by {strategy}: encode failed: {err}",
                        chain.names
                    )
                });
                let decoded = decode_chain(&decode_catalog(chain, Some(dict)), &encoded)
                    .unwrap_or_else(|err| {
                        panic!(
                            "{label} through {:?} primed by {strategy}: decode failed: {err}",
                            chain.names
                        )
                    });
                assert_eq!(
                    blake3::hash(&decoded),
                    blake3::hash(input),
                    "{label} through {:?} primed by {strategy}: dec ∘ enc is not the identity — \
                     priming must change the CODE and nothing else",
                    chain.names
                );
                checked += 1;
            }
        }
    }
    println!(
        "{checked} (chain × input × priming) identity case(s) over {} chain(s)",
        chains.len()
    );
    assert!(checked > 0, "the law was never exercised");
}

/// The other half of the priming claim: a payload primed with one dictionary must NOT
/// silently decode to something else under the wrong priming.
///
/// This is the assertion that makes the identity law worth having. If an unprimed decode
/// of a primed payload merely produced different bytes, every digest in the bundle would
/// catch it — but the failure this axis actually has to exclude is the one where the
/// decode *appears* to work. So the requirement is explicit: the wrong dictionary either
/// REFUSES or produces bytes that are demonstrably not the payload; it never quietly
/// returns the payload.
#[test]
fn a_mis_primed_decode_never_silently_returns_the_payload() {
    let registry = live_registry();
    let chains = declared_chains(&registry);
    let dictionaries = training_dictionaries();
    let (_, primary) = dictionaries
        .first()
        .expect("at least one dictionary trains");
    let (_, other) = dictionaries.last().expect("at least one dictionary trains");
    assert_ne!(
        primary, other,
        "the two arms must use DIFFERENT dictionary bytes, or the check is vacuous"
    );

    // A payload with enough shared vocabulary for priming to actually engage.
    let payload: Vec<u8> = (0..4096u32)
        .flat_map(|index| {
            format!(
                "<https://blackcatinformatics.ca/gmeow/term{}> \
                 <https://blackcatinformatics.ca/gmeow/definition> \"d{index}\" .\n",
                index % 41
            )
            .into_bytes()
        })
        .collect();

    let mut exercised = 0usize;
    for chain in &chains {
        let encoded = encode_chain_with_options(
            &chain.names,
            &payload,
            EncodeOptions {
                zstd_level: Some(chain.level),
                dict: Some(primary),
            },
        )
        .expect("the primed encode succeeds");

        for (arm, catalog) in [
            ("unprimed", decode_catalog(chain, None)),
            ("wrong dictionary", decode_catalog(chain, Some(other))),
        ] {
            match decode_chain(&catalog, &encoded) {
                Err(_) => {}
                Ok(decoded) => assert_ne!(
                    blake3::hash(&decoded),
                    blake3::hash(&payload),
                    "a {arm} decode of a payload primed with a DIFFERENT dictionary returned the \
                     payload through {:?} — if that were reachable the priming would not be part \
                     of the code, and the medium axis's whole refusal discipline would be \
                     decoration",
                    chain.names
                ),
            }
            exercised += 1;
        }
    }
    assert!(exercised > 0, "no mis-primed decode was exercised");
}
