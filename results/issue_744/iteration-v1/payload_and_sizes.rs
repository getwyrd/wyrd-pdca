//! Derived payloads, size classes and the deployment tuning flags (proposal 0017 §4; #744).
//!
//! Binds two properties a months-long run depends on:
//!
//! - **Payloads are derived, never stored**: `bytes = f(seed, key, size, generation)`, the same
//!   in every process (pinned known-answer digests), with every input mattering — the
//!   generation above all, or a stale read after a same-size overwrite looks correct — and any
//!   byte range computable on its own, in bounded memory, wherever it sits in the object.
//! - **Size classes come from the tuning**, never from a hardcoded table: they move with
//!   `--chunk-size`, `--max-root-value-bytes` and `--segment-threshold-bytes`, whose defaults
//!   are echoed as defaults and checked here against the `wyrd-core` constants they mirror.
#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use wyrd_validate::sizes::derived_segment_threshold;
use wyrd_validate::{
    run, Boundary, OutOfRange, Payload, SizeClass, Source, Tuning, TuningError, TuningFlags,
    CHUNK_REF_BYTES, DEFAULT_CHUNK_SIZE, DEFAULT_MAX_ROOT_VALUE_BYTES, EXIT_INCONCLUSIVE,
    EXIT_USAGE, PIECE_BYTES, TUNING_FLAGS,
};

const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
const TIB: u64 = 1 << 40;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The whole payload, materialised. Only for the small sizes these tests use.
fn whole(payload: &Payload) -> Vec<u8> {
    payload.pieces().flatten().collect()
}

/// `[start, start + len)` produced directly.
fn direct(payload: &Payload, start: u64, len: usize) -> Vec<u8> {
    let mut buf = vec![0; len];
    payload
        .fill(start, &mut buf)
        .unwrap_or_else(|e| panic!("fill: {e}"));
    buf
}

// ---- (A) Payload determinism: known answers --------------------------------------------------

/// `(seed, key, size, generation, SHA-256 of the payload)`. The digests were produced by a
/// separate process, and checked against an independent implementation of the construction
/// written from `src/payload.rs`'s module docs alone. Any input the generator takes from
/// process state breaks them; so does any change to the construction, which would silently
/// invalidate every checkpoint ever written.
const KNOWN_ANSWERS: [(&str, &str, u64, u64, &str); 6] = [
    (
        "424242",
        "kat/empty",
        0,
        0,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ),
    (
        "424242",
        "kat/one",
        1,
        0,
        "1f184f101c67d585ba00d24cfde36863f747fbab9e84af7760ec771e9b36ca51",
    ),
    (
        "424242",
        "kat/block",
        32,
        3,
        "2e2ec4a04bbd9bedd9d0557cfbd8ae03a9a855ce49a407f494b1c145f138b0da",
    ),
    (
        "424242",
        "kat/4k",
        4096,
        1,
        "a2e4f16a4135b8f9d108370cbff07e956d716f0d2f4cc5a51ea7d1114f1bf8a3",
    ),
    (
        "seed-\u{fc}",
        "dir/obj with space",
        100_003,
        7,
        "863f70c69e72cb2ce14d4abe7100704f4fd06ccb5b2118e5908a1067c4572446",
    ),
    (
        "424242",
        "kat/multi-piece",
        3 * 64 * KIB + 5,
        2,
        "802a4f5a805613e4c78c7b325ddf5e554d30e06a376c5bfb0059cd8c919398aa",
    ),
];

#[test]
fn a_payload_digest_matches_its_known_answer() {
    for (seed, key, size, generation, expected) in KNOWN_ANSWERS {
        let payload = Payload::new(seed, key, size, generation);
        let bytes = whole(&payload);
        assert_eq!(
            hex(&Sha256::digest(&bytes)),
            expected,
            "({seed:?}, {key:?}, {size}, {generation})"
        );
        // The streaming digest the oracle and checkpoints key on is the digest of those bytes.
        assert_eq!(
            hex(&payload.sha256()),
            expected,
            "streaming digest of ({seed:?}, {key:?}, {size}, {generation})"
        );
    }
}

// ---- (B) Every input matters, at sizes where it can -----------------------------------------

#[test]
fn changing_any_one_input_changes_the_bytes() {
    for size in [4 * KIB, 100_003] {
        let base = whole(&Payload::new("424242", "b/key", size, 5));
        let len = base.len();
        let variants = [
            ("seed", whole(&Payload::new("424243", "b/key", size, 5))),
            ("key", whole(&Payload::new("424242", "b/key2", size, 5))),
            // The same-size overwrite: without the generation in the derivation it re-sends
            // identical bytes, and a read of the prior generation would look correct.
            (
                "generation",
                whole(&Payload::new("424242", "b/key", size, 6)),
            ),
        ];
        for (input, bytes) in variants {
            assert_eq!(bytes.len(), len, "{input} at {size}");
            assert_ne!(
                bytes, base,
                "changing the {input} left {size} bytes unchanged"
            );
        }
        // The size is an input too, not just a length: the longer payload is not the shorter
        // one extended.
        let longer = whole(&Payload::new("424242", "b/key", size + 1, 5));
        assert_ne!(
            &longer[..len],
            &base[..],
            "a {size}-byte payload is a prefix of the {}-byte one",
            size + 1
        );
    }
}

#[test]
fn at_zero_bytes_no_input_can_matter() {
    // The documented limit (module docs, for the oracle in #746): at 0 B every input gives the
    // same empty payload, so the bytes alone cannot tell generations apart there.
    assert!(whole(&Payload::new("424242", "z", 0, 1)).is_empty());
    assert!(whole(&Payload::new("other", "y", 0, 2)).is_empty());
}

// ---- (C) Exact length and random access ------------------------------------------------------

const ACCESS_SIZES: [u64; 9] = [0, 1, 31, 32, 33, 4096, 65_537, 100_003, 3 * 64 * KIB + 5];

#[test]
fn the_generator_yields_exactly_size_bytes() {
    for size in ACCESS_SIZES {
        let payload = Payload::new("424242", "c/len", size, 1);
        let bytes = whole(&payload);
        assert_eq!(bytes.len() as u64, size);
        assert_eq!(payload.size(), size);
    }
}

#[test]
fn split_pieces_concatenate_to_the_whole() {
    for size in ACCESS_SIZES {
        let payload = Payload::new("424242", "c/split", size, 1);
        let bytes = whole(&payload);
        let mut splits = vec![0, 1, 31, 32, 33, size / 2, size.saturating_sub(1), size];
        splits.retain(|split| *split <= size);
        for split in splits {
            let head = direct(&payload, 0, split as usize);
            let tail = direct(&payload, split, (size - split) as usize);
            assert_eq!(
                [head, tail].concat(),
                bytes,
                "size {size}, split at {split}"
            );
        }
    }
}

#[test]
fn any_range_produced_directly_equals_the_same_range_cut_from_the_whole() {
    for size in ACCESS_SIZES {
        let payload = Payload::new("424242", "c/range", size, 9);
        let bytes = whole(&payload);
        let n = bytes.len();
        let mut ranges = vec![(0, n), (0, 0), (n, n)];
        for (a, b) in [
            (1, 2),
            (5, 37),
            (31, 33),
            (32, 64),
            (33, 4097),
            (4095, 70_000),
            (65_535, 65_537),
            (n / 3, 2 * n / 3),
            (n.saturating_sub(7), n),
        ] {
            if a <= b && b <= n {
                ranges.push((a, b));
            }
        }
        for (a, b) in ranges {
            let expected = &bytes[a..b];
            assert_eq!(
                direct(&payload, a as u64, b - a),
                expected,
                "fill [{a}, {b}) of {size}"
            );
            let streamed: Vec<u8> = payload
                .range(a as u64, b as u64)
                .unwrap_or_else(|e| panic!("range [{a}, {b}) of {size}: {e}"))
                .flatten()
                .collect();
            assert_eq!(streamed, expected, "range [{a}, {b}) of {size}");
        }
    }
}

#[test]
fn a_range_outside_the_payload_is_refused_not_truncated() {
    let payload = Payload::new("424242", "c/bounds", 100, 1);
    let mut two = [0u8; 2];
    assert_eq!(
        payload.fill(99, &mut two),
        Err(OutOfRange {
            start: 99,
            end: Some(101),
            size: 100
        })
    );
    assert_eq!(two, [0, 0], "a refused fill wrote into the buffer");
    assert!(payload.fill(u64::MAX, &mut two).is_err());
    assert!(payload.range(10, 9).is_err());
    assert!(payload.range(0, 101).is_err());
    assert!(payload
        .range(100, 100)
        .is_ok_and(|mut r| r.next().is_none()));
}

// ---- (C′) Bounded state, direct access -------------------------------------------------------

#[test]
fn no_piece_exceeds_the_documented_bound_whatever_the_size() {
    assert_eq!(PIECE_BYTES, 64 * 1024);
    for size in [
        64 * KIB - 1,
        64 * KIB,
        64 * KIB + 1,
        3 * 64 * KIB + 5,
        MIB + 1,
    ] {
        let payload = Payload::new("424242", "c'/pieces", size, 1);
        let mut total = 0u64;
        for piece in payload.pieces() {
            assert!(piece.len() <= PIECE_BYTES, "{}-byte piece", piece.len());
            assert!(!piece.is_empty());
            total += piece.len() as u64;
        }
        assert_eq!(total, size);
    }
    // At sizes no test can stream whole, the first and last pieces are still bounded.
    for size in [10 * GIB + 1, TIB] {
        let payload = Payload::new("424242", "c'/huge", size, 1);
        assert_eq!(payload.pieces().next().map(|p| p.len()), Some(PIECE_BYTES));
        let tail: Vec<Vec<u8>> = payload
            .range(size - PIECE_BYTES as u64 - 3, size)
            .expect("tail range")
            .collect();
        assert_eq!(
            tail.iter().map(Vec::len).collect::<Vec<_>>(),
            [PIECE_BYTES, 3]
        );
    }
}

/// Run `work` on its own thread and fail if it has not finished within `limit` of wall time.
/// A generator that regenerates the prefix would take hours here; on its own thread it is
/// left running while this test fails, and the test process ends it when the run ends.
fn within<T: Send + 'static>(
    limit: Duration,
    what: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (done, result) = mpsc::channel();
    let started = Instant::now();
    std::thread::spawn(move || {
        let _ = done.send(work());
    });
    match result.recv_timeout(limit) {
        Ok(value) => value,
        Err(_) => panic!(
            "{what} not produced within {limit:?} (gave up after {:?}): the range is not \
             computed directly",
            started.elapsed()
        ),
    }
}

/// `(key, size, hex of the last 64 bytes)` at seed `424242`, generation 1; pinned like
/// [`KNOWN_ANSWERS`].
const TAIL_ANSWERS: [(&str, u64, &str); 2] = [
    (
        "kat/10g+1",
        10 * GIB + 1,
        "74a9d7960da87ac5b739ef864f2771446697a363db8c3348f45e35d9167f22ce\
         61b6f3d686f758670ff50fab3c77f0d8f71a7774f80aa2090932b471eeacccdd",
    ),
    (
        "kat/1t",
        TIB,
        "487bb9027e10c54f627d3a3b172cb5bc73982e1eaeacdd60d5b380fbca443326\
         b8ff6ef76280f8396a4ac496b2378e7643b2949def230b7fba9dbd7d13a1e4f8",
    ),
];

#[test]
fn the_last_bytes_of_a_huge_payload_are_produced_directly() {
    for (key, size, expected) in TAIL_ANSWERS {
        let payload = Payload::new("424242", key, size, 1);
        let start = size - 64;
        let tail = within(Duration::from_secs(5), &format!("the tail of {key}"), {
            let payload = payload.clone();
            move || direct(&payload, start, 64)
        });
        assert_eq!(hex(&tail), expected, "{key}");
        // And it is the concatenation of its two halves, each produced directly (split off a
        // block boundary, so each half straddles one).
        let halves = within(Duration::from_secs(5), &format!("the halves of {key}"), {
            let payload = payload.clone();
            move || {
                [
                    direct(&payload, start, 27),
                    direct(&payload, start + 27, 37),
                ]
                .concat()
            }
        });
        assert_eq!(halves, tail, "{key}");
        // The streaming form of the same range starts directly too.
        let streamed = within(
            Duration::from_secs(5),
            &format!("the streamed tail of {key}"),
            {
                move || {
                    payload
                        .range(start, size)
                        .expect("tail range")
                        .flatten()
                        .collect::<Vec<u8>>()
                }
            },
        );
        assert_eq!(streamed, tail, "{key}");
    }
}

// ---- (D) Size classes come from the tuning ---------------------------------------------------

fn tuning(flags: TuningFlags) -> Tuning {
    Tuning::resolve(&flags).unwrap_or_else(|e| panic!("{flags:?} refused: {e}"))
}

fn classes(flags: TuningFlags) -> Vec<SizeClass> {
    tuning(flags).size_classes().expect("size classes")
}

/// The one size labelled `boundary`.
fn size_at(classes: &[SizeClass], boundary: Boundary) -> u64 {
    let found: Vec<u64> = classes
        .iter()
        .filter(|class| class.boundaries.contains(&boundary))
        .map(|class| class.size)
        .collect();
    assert_eq!(found.len(), 1, "{boundary:?} labels {found:?}");
    found[0]
}

fn assert_ascending_without_duplicates(classes: &[SizeClass]) {
    for pair in classes.windows(2) {
        assert!(pair[0].size < pair[1].size, "{classes:?}");
    }
}

/// The boundaries whose size does not depend on the tuning, with that size. `SubChunk`, which
/// labels two sizes, is checked on its own.
const FIXED: [(Boundary, u64); 7] = [
    (Boundary::Empty, 0),
    (Boundary::MinimumNonEmpty, 1),
    (Boundary::S3MinimumPart, 5 * MIB),
    (Boundary::SdkMultipartThreshold, 8 * MIB),
    (Boundary::S3MaxSinglePut, 5 * GIB),
    (Boundary::PastS3MaxSinglePut, 5 * GIB + 1),
    (Boundary::BeyondTenGib, 10 * GIB + 1),
];

fn assert_fixed_classes_unmoved(classes: &[SizeClass]) {
    for (boundary, size) in FIXED {
        assert_eq!(size_at(classes, boundary), size, "{boundary:?} moved");
    }
    let sub_chunk: Vec<u64> = classes
        .iter()
        .filter(|class| class.boundaries.contains(&Boundary::SubChunk))
        .map(|class| class.size)
        .collect();
    assert_eq!(sub_chunk, [4 * KIB, 64 * KIB]);
}

#[test]
fn the_default_tuning_gives_the_proposal_table() {
    let c = MIB;
    let t = 158 * MIB;
    let expected = vec![
        (0, vec![Boundary::Empty]),
        (1, vec![Boundary::MinimumNonEmpty]),
        (4 * KIB, vec![Boundary::SubChunk]),
        (64 * KIB, vec![Boundary::SubChunk]),
        (c - 1, vec![Boundary::BelowChunk]),
        (c, vec![Boundary::AtChunk]),
        (c + 1, vec![Boundary::AboveChunk]),
        (5 * MIB, vec![Boundary::S3MinimumPart]),
        (8 * MIB, vec![Boundary::SdkMultipartThreshold]),
        (64 * c, vec![Boundary::MultiChunk]),
        (t - 1, vec![Boundary::BelowSegmentation]),
        (t, vec![Boundary::AtSegmentation]),
        (t + 1, vec![Boundary::AboveSegmentation]),
        (5 * GIB, vec![Boundary::S3MaxSinglePut]),
        (5 * GIB + 1, vec![Boundary::PastS3MaxSinglePut]),
        (10 * GIB + 1, vec![Boundary::BeyondTenGib]),
    ];
    let actual: Vec<(u64, Vec<Boundary>)> = classes(TuningFlags::default())
        .into_iter()
        .map(|class| (class.size, class.boundaries))
        .collect();
    assert_eq!(actual, expected);
    // Every class carries a label in words.
    for boundary in expected.iter().flat_map(|(_, b)| b) {
        assert!(!boundary.label().is_empty());
    }
}

#[test]
fn a_different_chunk_size_moves_the_chunk_classes_and_nothing_fixed() {
    let c = 4 * MIB;
    let classes = classes(TuningFlags {
        chunk_size: Some(c),
        ..TuningFlags::default()
    });
    assert_ascending_without_duplicates(&classes);
    assert_eq!(size_at(&classes, Boundary::BelowChunk), c - 1);
    assert_eq!(size_at(&classes, Boundary::AtChunk), c);
    assert_eq!(size_at(&classes, Boundary::AboveChunk), c + 1);
    assert_eq!(size_at(&classes, Boundary::MultiChunk), 64 * c);
    // The derived threshold follows the chunk size: 158 chunks of 4 MiB.
    assert_eq!(size_at(&classes, Boundary::AtSegmentation), 158 * c);
    assert_eq!(size_at(&classes, Boundary::BelowSegmentation), 158 * c - 1);
    assert_eq!(size_at(&classes, Boundary::AboveSegmentation), 158 * c + 1);
    assert_fixed_classes_unmoved(&classes);
}

#[test]
fn only_the_root_budget_changed_moves_only_the_derived_threshold() {
    // 31,500 is below today's 50,000: ⌊31,500 / 315⌋ = 100 chunks of 1 MiB.
    let classes = classes(TuningFlags {
        max_root_value_bytes: Some(31_500),
        ..TuningFlags::default()
    });
    assert_ascending_without_duplicates(&classes);
    assert_eq!(size_at(&classes, Boundary::AtSegmentation), 100 * MIB);
    assert_eq!(
        size_at(&classes, Boundary::BelowSegmentation),
        100 * MIB - 1
    );
    assert_eq!(
        size_at(&classes, Boundary::AboveSegmentation),
        100 * MIB + 1
    );
    assert_eq!(size_at(&classes, Boundary::BelowChunk), MIB - 1);
    assert_eq!(size_at(&classes, Boundary::AtChunk), MIB);
    assert_eq!(size_at(&classes, Boundary::AboveChunk), MIB + 1);
    assert_eq!(size_at(&classes, Boundary::MultiChunk), 64 * MIB);
    assert_fixed_classes_unmoved(&classes);
}

#[test]
fn a_given_threshold_is_used_exactly_whatever_the_root_budget() {
    let t = 123_456_789;
    for budget in [None, Some(31_500), Some(200)] {
        let resolved = tuning(TuningFlags {
            max_root_value_bytes: budget,
            segment_threshold_bytes: Some(t),
            ..TuningFlags::default()
        });
        assert_eq!(
            resolved.segment_threshold_bytes.value, t,
            "budget {budget:?}"
        );
        assert_eq!(resolved.segment_threshold_bytes.source, Source::Given);
        let classes = resolved.size_classes().expect("size classes");
        assert_eq!(size_at(&classes, Boundary::AtSegmentation), t);
        assert_eq!(size_at(&classes, Boundary::BelowSegmentation), t - 1);
        assert_eq!(size_at(&classes, Boundary::AboveSegmentation), t + 1);
    }
}

#[test]
fn two_classes_that_coincide_appear_once_carrying_both_labels() {
    // A 4 KiB chunk lands `C` on the 4 KiB sub-chunk class.
    let classes = classes(TuningFlags {
        chunk_size: Some(4 * KIB),
        ..TuningFlags::default()
    });
    assert_ascending_without_duplicates(&classes);
    let at_4k: Vec<&SizeClass> = classes.iter().filter(|c| c.size == 4 * KIB).collect();
    assert_eq!(at_4k.len(), 1, "{classes:?}");
    assert_eq!(at_4k[0].boundaries, [Boundary::SubChunk, Boundary::AtChunk]);
    // A 1-byte chunk lands `C − 1` on 0 B and `C` on 1 B.
    let classes = self::classes(TuningFlags {
        chunk_size: Some(1),
        segment_threshold_bytes: Some(10 * GIB),
        ..TuningFlags::default()
    });
    assert_ascending_without_duplicates(&classes);
    assert_eq!(classes[0].size, 0);
    assert_eq!(
        classes[0].boundaries,
        [Boundary::Empty, Boundary::BelowChunk]
    );
    assert_eq!(classes[1].size, 1);
    assert_eq!(
        classes[1].boundaries,
        [Boundary::MinimumNonEmpty, Boundary::AtChunk]
    );
}

#[test]
fn a_tuning_that_cannot_produce_a_table_is_refused_not_clamped() {
    let refused = |flags: TuningFlags| Tuning::resolve(&flags).expect_err("accepted");
    // A budget smaller than one chunk record derives no threshold.
    assert_eq!(
        refused(TuningFlags {
            max_root_value_bytes: Some(CHUNK_REF_BYTES - 1),
            ..TuningFlags::default()
        }),
        TuningError::NoChunkRefFits {
            max_root_value_bytes: CHUNK_REF_BYTES - 1
        }
    );
    // Sizes past 64 bits.
    for flags in [
        TuningFlags {
            chunk_size: Some(u64::MAX),
            segment_threshold_bytes: Some(GIB),
            ..TuningFlags::default()
        },
        TuningFlags {
            chunk_size: Some(1 << 60),
            segment_threshold_bytes: Some(GIB),
            ..TuningFlags::default()
        },
        TuningFlags {
            chunk_size: Some(1 << 60),
            ..TuningFlags::default()
        },
        TuningFlags {
            segment_threshold_bytes: Some(u64::MAX),
            ..TuningFlags::default()
        },
    ] {
        assert!(
            matches!(refused(flags), TuningError::Overflow(_)),
            "{flags:?}"
        );
    }
    assert_eq!(
        refused(TuningFlags {
            chunk_size: Some(0),
            ..TuningFlags::default()
        }),
        TuningError::Zero("chunk-size")
    );
}

// ---- (E) The tuning flags ---------------------------------------------------------------------

/// The ten required flags, as `cli_surface.rs` gives them.
const GIVEN: [(&str, &str); 10] = [
    ("endpoint", "http://127.0.0.1:9000"),
    ("region", "eu-central-1"),
    ("bucket", "validate-bucket"),
    ("scenario", "smoke"),
    ("duration", "2m"),
    ("workers", "7"),
    ("seed", "424242"),
    ("out", "/var/lib/wyrd-validate/out"),
    ("run-id", "run-0017"),
    ("driver-placement", "external"),
];

fn argv(pairs: &[(&str, &str)]) -> Vec<String> {
    pairs
        .iter()
        .flat_map(|(flag, value)| [format!("--{flag}"), (*value).to_string()])
        .collect()
}

fn wyrd_pair() -> HashMap<&'static str, OsString> {
    HashMap::from([
        ("WYRD_S3_ACCESS_KEY", OsString::from("WYRDKEYID0001")),
        ("WYRD_S3_SECRET_KEY", OsString::from("wyrd-secret")),
    ])
}

/// The library entry point over the Wyrd credential pair: (status, stdout, stderr).
fn lib_run(args: &[String]) -> (u8, String, String) {
    let vars = wyrd_pair();
    let lookup = move |name: &str| vars.get(name).cloned();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run(args, &lookup, &mut out, &mut err);
    (
        status,
        String::from_utf8(out).expect("utf-8"),
        String::from_utf8(err).expect("utf-8"),
    )
}

fn lines(stdout: &str) -> Vec<&str> {
    stdout.lines().collect()
}

#[test]
fn absent_tuning_flags_are_echoed_as_defaults_and_derived() {
    let (status, stdout, stderr) = lib_run(&argv(&GIVEN));
    assert_eq!(status, EXIT_INCONCLUSIVE, "{stderr}");
    let lines = lines(&stdout);
    for expected in [
        "  --chunk-size = 1048576 (default)",
        "  --max-root-value-bytes = 50000 (default)",
        "  --segment-threshold-bytes = 165675008 (derived)",
    ] {
        assert!(lines.contains(&expected), "missing `{expected}`:\n{stdout}");
    }
    // The ten required flags are still echoed each with its value.
    for (flag, value) in GIVEN {
        let line = format!("  --{flag} = {value}");
        assert!(lines.contains(&line.as_str()), "{stdout}");
    }
}

#[test]
fn given_tuning_flags_are_used_and_echoed_as_given() {
    let mut args = argv(&GIVEN);
    args.extend(argv(&[("chunk-size", "4194304")]));
    let (status, stdout, stderr) = lib_run(&args);
    assert_eq!(status, EXIT_INCONCLUSIVE, "{stderr}");
    let lines = lines(&stdout);
    for expected in [
        "  --chunk-size = 4194304 (given)",
        "  --max-root-value-bytes = 50000 (default)",
        // Derived from the given chunk size: 158 × 4 MiB.
        "  --segment-threshold-bytes = 662700032 (derived)",
    ] {
        assert!(lines.contains(&expected), "missing `{expected}`:\n{stdout}");
    }

    // All three given, in among the required flags.
    let mut args = argv(&GIVEN[..4]);
    args.extend(argv(&[
        ("segment-threshold-bytes", "123456789"),
        ("max-root-value-bytes", "31500"),
        ("chunk-size", "2097152"),
    ]));
    args.extend(argv(&GIVEN[4..]));
    let (status, stdout, stderr) = lib_run(&args);
    assert_eq!(status, EXIT_INCONCLUSIVE, "{stderr}");
    let lines = self::lines(&stdout);
    for expected in [
        "  --chunk-size = 2097152 (given)",
        "  --max-root-value-bytes = 31500 (given)",
        "  --segment-threshold-bytes = 123456789 (given)",
    ] {
        assert!(lines.contains(&expected), "missing `{expected}`:\n{stdout}");
    }
}

/// `args` refused with a usage error naming `--flag`, with `phrase` on stderr and nothing echoed.
fn assert_refused(args: &[String], flag: &str, phrase: &str) {
    let (status, stdout, stderr) = lib_run(args);
    assert_eq!(status, EXIT_USAGE, "{args:?} accepted:\n{stdout}");
    assert!(stdout.is_empty(), "a refused run echoed:\n{stdout}");
    assert!(
        stderr.contains(&format!("`--{flag}`")),
        "{flag} not named: {stderr}"
    );
    assert!(stderr.contains(phrase), "`{phrase}` not in: {stderr}");
}

#[test]
fn each_bad_tuning_value_is_refused_by_name() {
    for flag in TUNING_FLAGS {
        let with = |value: &str| {
            let mut args = argv(&GIVEN);
            args.extend([format!("--{flag}"), value.to_string()]);
            args
        };
        assert_refused(&with("0"), flag, "must be a positive integer, not 0");
        assert_refused(&with("000"), flag, "must be a positive integer, not 0");
        for not_an_integer in ["1MiB", "+5", "-5", " 5", "5 ", "1.5", "0x10", "1_000"] {
            assert_refused(
                &with(not_an_integer),
                flag,
                &format!("needs a positive integer byte count, not `{not_an_integer}`"),
            );
        }
        assert_refused(
            &with("18446744073709551616"),
            flag,
            "value `18446744073709551616` does not fit in 64 bits",
        );
        assert_refused(&with(""), flag, "was given an empty value");
        assert_refused(&with("--typo"), flag, "`--typo`");

        // Repeated, even with the same value.
        let mut args = with("4096");
        args.extend([format!("--{flag}"), "4096".to_string()]);
        assert_refused(&args, flag, "given more than once");

        // The last token, with no value after it.
        let mut args = argv(&GIVEN);
        args.push(format!("--{flag}"));
        assert_refused(&args, flag, "needs a value");
    }
}

#[test]
fn a_tuning_that_cannot_produce_a_table_is_refused_at_the_command_line() {
    let mut args = argv(&GIVEN);
    args.extend(argv(&[("max-root-value-bytes", "314")]));
    assert_refused(
        &args,
        "max-root-value-bytes",
        "holds no 315-byte chunk record",
    );
}

#[test]
fn the_ten_required_flags_stay_required_with_the_tuning_flags_given() {
    let mut args = argv(&GIVEN[1..]);
    args.extend(argv(&[("chunk-size", "4096")]));
    assert_refused(&args, "endpoint", "missing required flag(s): `--endpoint`");
}

#[test]
fn the_real_binary_echoes_the_tuning_with_its_sources() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wyrd-validate"));
    let mut args = argv(&GIVEN);
    args.extend(argv(&[("max-root-value-bytes", "31500")]));
    command.args(&args).env_clear().envs(wyrd_pair());
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let output = command.output().expect("spawn wyrd-validate");
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert_eq!(
        output.status.code(),
        Some(i32::from(EXIT_INCONCLUSIVE)),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines = lines(&stdout);
    for expected in [
        "  --chunk-size = 1048576 (default)",
        "  --max-root-value-bytes = 31500 (given)",
        "  --segment-threshold-bytes = 104857600 (derived)",
    ] {
        assert!(lines.contains(&expected), "missing `{expected}`:\n{stdout}");
    }
}

// ---- (F) The defaults have not drifted from the constants they mirror ------------------------

#[test]
fn the_mirrored_defaults_match_the_core_constants() {
    // `wyrd-core` is a dev-dependency only: the shipped binary links no `wyrd-*` crate.
    assert_eq!(
        DEFAULT_MAX_ROOT_VALUE_BYTES,
        wyrd_core::metadata::MAX_ROOT_VALUE_BYTES as u64,
        "the root budget default drifted from MAX_ROOT_VALUE_BYTES"
    );
    assert_eq!(
        CHUNK_REF_BYTES,
        wyrd_core::multipart::MAX_CHUNKREF_BYTES as u64,
        "the chunk-record width drifted from MAX_CHUNKREF_BYTES"
    );
    assert_eq!(
        DEFAULT_MAX_ROOT_VALUE_BYTES / CHUNK_REF_BYTES,
        u64::from(wyrd_core::multipart::MAX_MAP_CHUNKS),
        "the server's flat-map capacity is no longer ⌊root budget / chunk-ref width⌋"
    );
    let derived = derived_segment_threshold(DEFAULT_CHUNK_SIZE, DEFAULT_MAX_ROOT_VALUE_BYTES)
        .expect("default threshold");
    assert_eq!(
        derived,
        wyrd_core::multipart::max_single_put_bytes(DEFAULT_CHUNK_SIZE),
        "the derived default threshold drifted from max_single_put_bytes at a 1 MiB chunk"
    );
    assert_eq!(DEFAULT_CHUNK_SIZE, MIB);
    assert_eq!(
        tuning(TuningFlags::default()).segment_threshold_bytes.value,
        derived
    );
}
