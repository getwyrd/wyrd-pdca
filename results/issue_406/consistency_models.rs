//! **ADR-0041 consistency-checker models + workload + recorder** (issue #406, #329 slice 3).
//!
//! This is the load-bearing, C4-verify-flippable artifact for the consistency-checker
//! substrate. It exercises two things the merge gate can prove without a cluster or a JVM:
//!
//! 1. **The models reject crafted-inconsistent histories and accept valid ones** — flippable
//!    red→green assertions over hand-authored histories:
//!    * the **rw-register** model (ADR-0041 §Decision 1, ADR-0015 guarantee 2) rejects a torn
//!      read, a **read of a failed write's value** (a value no committed write ever wrote), a
//!      version regression / stale read, and a two-winners-at-one-commit-point history
//!      (distinct OR identical values), and accepts a linearizable one; and — the **contended
//!      path** (issue #406, iteration-2) — rejects a committed write with no captured commit
//!      version (`UnresolvedWrite`) and a vanished / lost committed write (`LostWrite`), so the
//!      commit-point checks are never silently switched off on the very ops that matter most;
//!    * the **list-append/set** model (ADR-0041 §Decision 2, ADR-0015 guarantee 1) rejects a
//!      lost-create, a resurrected-delete, and rename variants (a lost rename destination, a
//!      resurrected rename source) and accepts a valid create/delete/rename history;
//!    * the **session** read-your-writes / monotonic-read checks (ADR-0041 §Decision 3,
//!      ADR-0015 guarantee 3) reject a read-your-writes violation over the register and a
//!      monotonic-read violation over `meta:version`.
//!
//!    Each is genuinely flippable: weaken any model to accept-everything (or, for the
//!    targeted regressions, un-gate provenance / collapse the two-winners count / drop a
//!    rename branch) and its crafted-rejection assertion goes red **on real inputs** (not by
//!    a missing symbol — the module is present).
//!
//! 2. **The concurrent workload driver, run against the in-process `Gateway`, produces a
//!    non-vacuous recorded history the register model passes.** The workload drives real
//!    overwriting PUTs + reads of a small shared key set and directory create/delete/list
//!    against the same in-process gateway `crates/server/tests/closed_write_path.rs` drives
//!    (real redb metadata + fs chunks + in-memory coordination, ADR-0010). A barrier forces
//!    genuinely overlapping ops, and repeated overwrites of a hot key bump the inode `version`
//!    at the commit point past its `commit_create` value of 1. Every committed overwrite is
//!    recorded WITH its captured commit version — the shared hot key under a per-key mutex so
//!    each writer attributes its EXACT version even under multi-writer contention (the gateway
//!    PUT does not return it, and changing the gateway API is out of scope). So the produced
//!    history carries **no** `version=None` committed write, and the register model's
//!    version-keyed detections run on the contended ops rather than being switched off (issue
//!    #406, iteration-2 finding). The recorder serializes the history to the checker-compatible
//!    Elle op-map EDN, and the register model then passes it — the non-vacuous, checkable
//!    history ADR-0041's substrate exists to yield.
//!
//! The **live Elle/JVM verdict** over the serialized history is deferred to a privileged
//! off-Check job (ADR-0016/ADR-0041); [`verdict_dispatch`] models that routing decision as a
//! pure value here, the "deferred ≠ unbuilt" bar `xtask/src/metadata_faults.rs` sets. The
//! in-process gateway (`wyrd-gateway-core`'s `ObjectGateway`) exposes put/get/delete but **no
//! atomic rename**, so the workload drives create/delete/list; the rename model branch is
//! exercised by crafted histories + the recorder + serialization here, ready for the
//! wire-driven driver (#405) — see build notes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::Barrier;
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::InodeId;
use wyrd_core::read;
use wyrd_gateway_core::{GatewayError, ObjectGateway};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_server::Gateway;
use wyrd_testkit::consistency::{
    check_list_append, check_monotonic_reads, check_read_your_writes, check_register,
    max_observed_version, max_register_concurrency, namespace_to_edn, register_to_edn,
    verdict_dispatch, HistoryRecorder, Kind, NamespaceViolation, NsOp, NsViolationKind, RegF,
    RegOp, RegViolationKind, RegisterViolation, VerdictLeg, ELLE_RW_REGISTER, META_VERSION_KEY,
};
use wyrd_traits::{BoxError, CommitOutcome, MetadataStore, Result, WriteBatch};

// ─────────────────────────── (a) rw-register model ───────────────────────────

/// A valid, linearizable single-key register history is accepted: two overwrites (v1 then
/// v2) and reads that observe them in real-time-monotone order.
#[test]
fn register_accepts_a_linearizable_history() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(1)),
        RegOp::read_invoke(2, 1, "k"),
        RegOp::read_ok(3, 1, "k", 10, 1), // reads v1 after w1, before w2 — no stale read
        RegOp::write_invoke(4, 0, "k", 20),
        RegOp::write_ok(5, 0, "k", 20, Some(2)),
        RegOp::read_invoke(6, 2, "k"),
        RegOp::read_ok(7, 2, "k", 20, 2),
    ];
    assert!(
        check_register(&ops).is_ok(),
        "a linearizable register history must be accepted"
    );
}

/// A **torn read** — a read that returns a value the commit point never produced — is
/// rejected. Flip: a model that skips value-provenance accepts this, so the assertion is red.
#[test]
fn register_rejects_a_torn_read() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(1)),
        RegOp::read_invoke(2, 1, "k"),
        RegOp::read_ok(3, 1, "k", 99, 1), // value 99 was never written
    ];
    let err = check_register(&ops).expect_err("a torn read must be rejected");
    assert_eq!(err.kind, RegViolationKind::TornRead, "{err}");
}

/// A read that returns the value of a **definitely-FAILED write** — a value no committed
/// write ever wrote — is a torn read (issue #406 regression). The commit point never produced
/// `(99, v2)`: write 99 lost its CAS (`Fail` = "definitely did not take effect"), and version
/// 2 was never committed. A checker whose whole value is its correctness MUST reject this.
///
/// Flip (the model-weakening red this pins): if the value-provenance domain is built from
/// **all** writes instead of only `ok` ones (drop the `&& c.ok` in `check_register`'s
/// `written` loop), value 99 is admitted and `check_register` FALSE-ACCEPTS — this assertion
/// goes red on a real input, not by a missing symbol.
#[test]
fn register_rejects_a_read_of_a_failed_writes_value() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(1)),
        RegOp::write_invoke(2, 1, "k", 99),
        RegOp::write_fail(3, 1, "k", 99), // lost the commit CAS — 99 was never committed
        RegOp::read_invoke(4, 2, "k"),
        RegOp::read_ok(5, 2, "k", 99, 2), // reads 99 @ v2 — neither ever existed at the commit point
    ];
    let err = check_register(&ops).expect_err("a read of a failed write's value must be rejected");
    assert_eq!(err.kind, RegViolationKind::TornRead, "{err}");
}

/// A **stale read / version regression** — a read that completed observing a newer version,
/// followed by a strictly-later read observing an older one — is rejected.
#[test]
fn register_rejects_a_version_regression() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(1)),
        RegOp::write_invoke(2, 0, "k", 20),
        RegOp::write_ok(3, 0, "k", 20, Some(2)),
        RegOp::read_invoke(4, 1, "k"),
        RegOp::read_ok(5, 1, "k", 20, 2), // sees v2, completes at index 5
        RegOp::read_invoke(6, 2, "k"),    // begins at index 6 (after 5)
        RegOp::read_ok(7, 2, "k", 10, 1), // regresses to v1
    ];
    let err = check_register(&ops).expect_err("a version regression must be rejected");
    assert_eq!(err.kind, RegViolationKind::VersionRegression, "{err}");
}

/// A **two-winners-at-one-commit-point** history — two distinct writes both committing the
/// same inode version — is rejected (exactly-one-writer-wins, ADR-0015 guarantee 2).
#[test]
fn register_rejects_two_winners_at_one_commit_point() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(2)),
        RegOp::write_invoke(2, 1, "k", 20),
        RegOp::write_ok(3, 1, "k", 20, Some(2)), // same version 2, different value
    ];
    let err = check_register(&ops).expect_err("two winners at one version must be rejected");
    assert_eq!(err.kind, RegViolationKind::TwoWinners, "{err}");
}

/// Two writes that both won the same commit-point version with **identical values** are still
/// two winners (a same-value double-commit). Exactly-one-writer-wins is about *how many*
/// writes produced a version, not whether their values differ (issue #406 improvement).
///
/// Flip (the model-weakening red this pins): if Pass 1 counts *distinct values* rather than
/// *occurrences* (a `BTreeSet<u64>` of length > 1), this identical-value double-commit is not
/// flagged and `check_register` FALSE-ACCEPTS — this assertion goes red.
#[test]
fn register_rejects_two_winners_with_identical_values() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(2)),
        RegOp::write_invoke(2, 1, "k", 10),
        RegOp::write_ok(3, 1, "k", 10, Some(2)), // same version 2, SAME value 10
    ];
    let err = check_register(&ops)
        .expect_err("two winners at one version with identical values must be rejected");
    assert_eq!(err.kind, RegViolationKind::TwoWinners, "{err}");
}

// --- (a′) the CONTENDED path: version=None committed writes must not be a free pass ---
//
// These four reds pin the iteration-2 carry-forward. Each is a MODEL-WEAKENING red on a real
// input, not a missing symbol: revert only the guard named in each doc and the crafted history
// FALSE-ACCEPTS while `consistency.rs` still compiles. Together they lock in that the register
// model can no longer be fooled on the contended path, where a recorder that failed to capture
// a committed write's commit version records `version=None`.

/// **Hole 1 — stale read after an unversioned committed overwrite.** A committed overwrite
/// whose commit version was not captured (`None`), then a read that returns the OLD version.
/// Pass 3 (version regression) excludes `version=None` writes, so without the unresolved-write
/// guard the stale read is never compared against the overwrite and the model FALSE-ACCEPTS.
///
/// Flip: delete Pass 0 (the `UnresolvedWrite` loop) in `check_register` — `consistency.rs`
/// still compiles, and this history is accepted → red.
#[test]
fn register_rejects_a_stale_read_after_an_unversioned_overwrite() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(1)),
        RegOp::write_invoke(2, 1, "k", 20),
        RegOp::write_ok(3, 1, "k", 20, None), // committed overwrite, version NOT captured
        RegOp::read_invoke(4, 2, "k"),
        RegOp::read_ok(5, 2, "k", 10, 1), // stale: reads v1 after the v2 overwrite committed
    ];
    let err = check_register(&ops)
        .expect_err("a committed overwrite with no resolvable commit version must be rejected");
    assert_eq!(err.kind, RegViolationKind::UnresolvedWrite, "{err}");
}

/// **Hole 2 — a superseded value reads clean via an unversioned overwrite.** Value provenance
/// is per-key, so `20` is "known written"; but the overwrite that produced it recorded no
/// version, so `version_value` has no `(k, 2)` entry and a read claiming `20 @ v2` slips through
/// the per-key (not per-`(key, version)`) provenance. The unresolved-write guard rejects the
/// unverifiable committed overwrite before the hole can be exploited.
///
/// Flip: delete Pass 0 — the read is admitted (`20` is in the per-key set, `(k, 2)` is unseen)
/// → red.
#[test]
fn register_rejects_a_superseded_value_read_via_an_unversioned_overwrite() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(1)),
        RegOp::write_invoke(2, 1, "k", 20),
        RegOp::write_ok(3, 1, "k", 20, None), // committed but version not captured
        RegOp::read_invoke(4, 2, "k"),
        RegOp::read_ok(5, 2, "k", 20, 2), // 20 @ v2 — v2 was never resolved to a value
    ];
    let err = check_register(&ops)
        .expect_err("a read keyed to an unresolved commit version must be rejected");
    assert_eq!(err.kind, RegViolationKind::UnresolvedWrite, "{err}");
}

/// **Hole 3 — a lost / vanished committed write.** The register has no delete: once a write
/// COMMITS the key is present for every later read. A read that finds the key ABSENT after a
/// committed write to it completed in real time is a lost write. Without the lost-write pass
/// the absent read is skipped ("observes nothing") and the model FALSE-ACCEPTS. This one is
/// **version-independent** — it fires even though the committed write carried a version, so
/// lost-write detection is never gated on the contended path.
///
/// Flip: delete Pass 2b (the `LostWrite` block) — the absent read is skipped → red.
#[test]
fn register_rejects_a_lost_write_absent_read() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(1)), // committed, completes at index 1
        RegOp::read_invoke(2, 1, "k"),           // begins at index 2 (after the write)
        RegOp::read_absent(3, 1, "k"),           // key vanished — a lost committed write
    ];
    let err = check_register(&ops).expect_err("a lost committed write must be rejected");
    assert_eq!(err.kind, RegViolationKind::LostWrite, "{err}");
}

/// **Hole 4 — two unversioned winners.** Two committed overwrites of a key whose versions were
/// not captured. They MIGHT both have won the same commit-point version (two winners); the
/// model cannot rule it out. Pass 1 was keyed on `Some(version)`, so it simply did not count
/// them and the model FALSE-ACCEPTED. The unresolved-write guard rejects: an uncountable
/// committed overwrite is not certifiable.
///
/// Flip: delete Pass 0 — neither write is counted by Pass 1 (both `version=None`) → red.
#[test]
fn register_rejects_two_unversioned_winners() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, None),
        RegOp::write_invoke(2, 1, "k", 20),
        RegOp::write_ok(3, 1, "k", 20, None),
    ];
    let err = check_register(&ops)
        .expect_err("committed overwrites with no resolvable version must be rejected");
    assert_eq!(err.kind, RegViolationKind::UnresolvedWrite, "{err}");
}

// ─────────────────────────── (b) list-append / set model ───────────────────────────

/// A valid namespace history is accepted: two creates, a list reflecting both, a delete, and
/// a list reflecting the removal.
#[test]
fn list_append_accepts_a_valid_history() {
    let ops = vec![
        NsOp::create_invoke(0, 0, "a"),
        NsOp::create_ok(1, 0, "a"),
        NsOp::create_invoke(2, 0, "b"),
        NsOp::create_ok(3, 0, "b"),
        NsOp::list_invoke(4, 1),
        NsOp::list_ok(5, 1, vec!["a".into(), "b".into()]),
        NsOp::delete_invoke(6, 0, "a"),
        NsOp::delete_ok(7, 0, "a"),
        NsOp::list_invoke(8, 1),
        NsOp::list_ok(9, 1, vec!["b".into()]),
    ];
    assert!(
        check_list_append(&ops).is_ok(),
        "a valid namespace history must be accepted"
    );
}

/// A **lost create** — a name created and never removed, absent from a later list — is
/// rejected (no lost create, ADR-0015 guarantee 1).
#[test]
fn list_append_rejects_a_lost_create() {
    let ops = vec![
        NsOp::create_invoke(0, 0, "a"),
        NsOp::create_ok(1, 0, "a"),
        NsOp::list_invoke(2, 1),
        NsOp::list_ok(3, 1, vec![]), // "a" is missing though never deleted
    ];
    let err = check_list_append(&ops).expect_err("a lost create must be rejected");
    assert_eq!(err.kind, NsViolationKind::LostCreate, "{err}");
}

/// A **resurrected delete** — a deleted name reappearing in a later list with no re-create —
/// is rejected (no resurrected delete, ADR-0015 guarantee 1).
#[test]
fn list_append_rejects_a_resurrected_delete() {
    let ops = vec![
        NsOp::create_invoke(0, 0, "a"),
        NsOp::create_ok(1, 0, "a"),
        NsOp::delete_invoke(2, 0, "a"),
        NsOp::delete_ok(3, 0, "a"),
        NsOp::list_invoke(4, 1),
        NsOp::list_ok(5, 1, vec!["a".into()]), // "a" resurrected after its delete
    ];
    let err = check_list_append(&ops).expect_err("a resurrected delete must be rejected");
    assert_eq!(err.kind, NsViolationKind::ResurrectedDelete, "{err}");
}

/// A valid **rename** (a single dirent mutation: remove source, add destination) is accepted
/// — `a` renamed to `b`, and a later list shows `b` present and `a` gone.
#[test]
fn list_append_accepts_a_valid_rename() {
    let ops = vec![
        NsOp::create_invoke(0, 0, "a"),
        NsOp::create_ok(1, 0, "a"),
        NsOp::rename_invoke(2, 0, "a", "b"),
        NsOp::rename_ok(3, 0, "a", "b"),
        NsOp::list_invoke(4, 1),
        NsOp::list_ok(5, 1, vec!["b".into()]), // "a" moved to "b"
    ];
    assert!(
        check_list_append(&ops).is_ok(),
        "a valid rename must be accepted"
    );
}

/// A rename whose **destination is lost** — `a` renamed to `b`, then a list omits `b` with no
/// delete of `b` — is a lost create of the rename's destination (exercises the rename ADD
/// branch). Flip: if the rename branch does not add its destination to the add-set, `b` is
/// untracked and this FALSE-ACCEPTS → red.
#[test]
fn list_append_rejects_a_rename_that_loses_its_destination() {
    let ops = vec![
        NsOp::create_invoke(0, 0, "a"),
        NsOp::create_ok(1, 0, "a"),
        NsOp::rename_invoke(2, 0, "a", "b"),
        NsOp::rename_ok(3, 0, "a", "b"),
        NsOp::list_invoke(4, 1),
        NsOp::list_ok(5, 1, vec![]), // "b" was renamed-in but the list omits it
    ];
    let err =
        check_list_append(&ops).expect_err("a rename whose destination is lost must be rejected");
    assert_eq!(err.kind, NsViolationKind::LostCreate, "{err}");
}

/// A rename whose **source is resurrected** — `a` renamed to `b`, then a list still shows `a`
/// with no re-create — is a resurrected delete of the rename's source (exercises the rename
/// REMOVE branch). Flip: if the rename branch does not remove its source, `a` is not
/// definitely-deleted and this FALSE-ACCEPTS → red.
#[test]
fn list_append_rejects_a_rename_source_resurrection() {
    let ops = vec![
        NsOp::create_invoke(0, 0, "a"),
        NsOp::create_ok(1, 0, "a"),
        NsOp::rename_invoke(2, 0, "a", "b"),
        NsOp::rename_ok(3, 0, "a", "b"),
        NsOp::list_invoke(4, 1),
        NsOp::list_ok(5, 1, vec!["a".into(), "b".into()]), // "a" renamed away but still present
    ];
    let err =
        check_list_append(&ops).expect_err("a rename whose source is resurrected must be rejected");
    assert_eq!(err.kind, NsViolationKind::ResurrectedDelete, "{err}");
}

/// The `HistoryRecorder` can **record** a rename (invoke + ok), the model checks the recorded
/// history, and the serialization renders the `[:rename …]` op-map — closing the "rename is
/// unrecordable / unexercised" gap the recorder had. Drives the real `rename_invoke`/
/// `rename_ok` recorder API, not hand-built `NsOp`s.
#[test]
fn recorder_records_and_checks_and_serializes_a_rename() {
    let mut rec = HistoryRecorder::new();
    rec.create_invoke(0, "a");
    rec.create_ok(0, "a");
    rec.rename_invoke(0, "a", "b");
    rec.rename_ok(0, "a", "b");
    rec.list_invoke(1);
    rec.list_ok(1, vec!["b".into()]);

    check_list_append(&rec.namespace).expect("the recorded rename history is linearizable");

    let edn = namespace_to_edn(&rec.namespace);
    assert!(
        edn.contains("[:rename :dir \"a\" \"b\"]"),
        "the recorder's rename must serialize to the list-append rename op-map: {edn}"
    );
}

// ─────────────────────────── (c) session checks ───────────────────────────

/// A session that does not violate its own view is accepted by both session checks.
#[test]
fn session_accepts_read_your_writes_and_monotonic_reads() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 50),
        RegOp::write_ok(1, 0, "k", 50, Some(3)),
        RegOp::read_invoke(2, 0, "k"),
        RegOp::read_ok(3, 0, "k", 50, 3), // reads its own write
        RegOp::read_invoke(4, 0, "k"),
        RegOp::read_ok(5, 0, "k", 50, 4), // never goes backwards
    ];
    assert!(check_read_your_writes(&ops).is_ok());
    assert!(check_monotonic_reads(&ops).is_ok());
}

/// **Read-your-writes** is violated when a session reads a version older than one it wrote.
#[test]
fn session_rejects_a_read_your_writes_violation() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 50),
        RegOp::write_ok(1, 0, "k", 50, Some(5)),
        RegOp::read_invoke(2, 0, "k"),
        RegOp::read_ok(3, 0, "k", 30, 3), // reads v3 after writing v5 — RYW broken
    ];
    assert!(
        check_read_your_writes(&ops).is_err(),
        "a read older than the session's own write must be rejected"
    );
}

/// **Monotonic reads** is violated when a session's successive reads of `meta:version` go
/// backwards — the high-water-mark clause of ADR-0015 guarantee 3.
#[test]
fn session_rejects_a_monotonic_read_violation_over_meta_version() {
    let ops = vec![
        RegOp::read_invoke(0, 0, META_VERSION_KEY),
        RegOp::read_ok(1, 0, META_VERSION_KEY, 5, 5),
        RegOp::read_invoke(2, 0, META_VERSION_KEY),
        RegOp::read_ok(3, 0, META_VERSION_KEY, 2, 2), // meta:version went backwards
    ];
    assert!(
        check_monotonic_reads(&ops).is_err(),
        "a backwards meta:version read must be rejected"
    );
}

// ─────────────────────────── model/serialization plumbing ───────────────────────────

/// The serialization is the checker-compatible Elle `rw-register` op-map EDN, and the verdict
/// routing keeps the JVM leg off the unprivileged gate (ADR-0016/ADR-0041).
#[test]
fn register_serializes_to_elle_edn_and_verdict_routes_off_check() {
    let ops = vec![
        RegOp::write_invoke(0, 0, "k", 10),
        RegOp::write_ok(1, 0, "k", 10, Some(1)),
        RegOp::read_invoke(2, 1, "k"),
        RegOp::read_ok(3, 1, "k", 10, 1),
    ];
    let edn = register_to_edn(&ops);
    assert!(edn.contains(":type :invoke"), "EDN op lifecycle: {edn}");
    assert!(edn.contains("[:w \"k\" 10]"), "EDN write mop: {edn}");
    assert!(edn.contains("[:r \"k\" 10]"), "EDN read mop: {edn}");

    // The unprivileged gate always runs the pure Rust models; the Elle/JVM verdict is
    // representable but selected only in the privileged off-Check job.
    assert_eq!(verdict_dispatch(false), VerdictLeg::InProcessModels);
    assert_eq!(
        verdict_dispatch(true),
        VerdictLeg::OffCheckElle {
            checker: ELLE_RW_REGISTER
        }
    );
}

// ─────────────────────────── (d) non-vacuous in-process workload ───────────────────────────

/// The root inode every object key binds under — the flat namespace root
/// (`crates/server/src/lib.rs:40`).
const ROOT: InodeId = 0;
/// Register keyspace prefix (the small shared key set the workload overwrites and reads).
const HOT: &str = "reg/hot";
/// Namespace ("directory") prefix; a name `n` is the object key `dir/n`, its dirent
/// `dirent:0/dir/n`.
const DIR_PREFIX: &str = "dir/";
/// Number of concurrent processes/sessions. `>= 2` so the barrier forces overlapping ops.
const PROCESSES: u32 = 3;

/// A `MetadataStore` that shares one redb database by `Arc`, so the in-process workload can
/// hand the gateway a writer handle **and** keep a reader handle to observe the committed
/// inode `version` at the commit point — redb serves concurrent read transactions alongside
/// the gateway's writes. The gateway takes its store by value (`Gateway::new`), so the shared
/// handle must be a `MetadataStore` in its own right (ADR-0010: the gateway is generic over
/// the seam, unchanged by this consumer).
#[derive(Clone)]
struct SharedRedb(Arc<RedbMetadataStore>);

#[async_trait]
impl MetadataStore for SharedRedb {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.0.get(key).await
    }
    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        self.0.scan(prefix).await
    }
    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        self.0.commit(batch).await
    }
}

type WorkGateway = Gateway<SharedRedb, FsChunkStore, MemCoordination>;

/// A unique payload for `value` whose byte length **is** `value` — so the committed inode
/// `size` recovers the value token from a single inode snapshot, tying `(value, version)`
/// together atomically without a separate object read that could race an overwrite.
fn payload(value: u64) -> Vec<u8> {
    vec![0u8; value as usize]
}

/// Whether a gateway PUT error is the commit-point CAS conflict (a concurrent writer won).
fn is_conflict(err: &BoxError) -> bool {
    err.downcast_ref::<GatewayError>() == Some(&GatewayError::Conflict)
}

/// Observe a register key's committed `(value, version)` from ONE inode snapshot: `version`
/// is the real commit-point version, `size` is the value token (see [`payload`]). `None` if
/// the key is absent. This drives the production read path (`wyrd_core::read`).
async fn observe(store: &RedbMetadataStore, key: &str) -> Option<(u64, u64)> {
    let inode_id = read::resolve(store, ROOT, key).await.ok()??;
    let inode = read::read_inode(store, inode_id).await.ok()??;
    Some((inode.size, inode.version))
}

/// Overwrite `key` with `value` through the real gateway PUT and capture the writer's **exact
/// commit version** — the iteration-2 carry-forward's PREFERRED fix (capture the real commit
/// version even under contention instead of dropping it to `None`, which switched the
/// register model's commit-point detection OFF on exactly the contended ops).
///
/// The gateway `put_object` returns `()`, not the commit version, and changing the gateway API
/// is out of scope (brief *Scope*: this slice is a **consumer** of the existing gateway API).
/// So the version is captured at the consumer by observing the just-committed inode **under
/// mutual exclusion for a shared key**: while we hold `key_lock`, no other writer can interpose
/// between our commit and our read-back, so `observe` returns OUR OWN value and thus OUR OWN
/// commit version. An **uncontended** key (single writer) passes `key_lock = None` — no other
/// writer can interpose, so its read-back already sees its own value. Either way the inode
/// `version` genuinely climbs 1→2→… across the multi-writer overwrites of the same inode (real
/// commit-point bumps, ADR-0041 §Decision 1), and every committed write is recorded WITH its
/// real version — so the produced history carries no `version=None` committed write for the
/// model to false-accept. Bounded retry so a pathological storm fails fast rather than hanging.
async fn versioned_put(
    gw: &WorkGateway,
    store: &RedbMetadataStore,
    key_lock: Option<&tokio::sync::Mutex<()>>,
    key: &str,
    value: u64,
) -> u64 {
    let _guard = match key_lock {
        Some(m) => Some(m.lock().await),
        None => None,
    };
    for _ in 0..200 {
        match gw.put_object(key, &payload(value)).await {
            Ok(()) => {
                let (size, version) = observe(store, key)
                    .await
                    .expect("a just-committed key must be observable");
                assert_eq!(
                    size, value,
                    "serialized commit+observe must read back our own write (value {value})"
                );
                return version;
            }
            Err(e) if is_conflict(&e) => continue,
            Err(e) => panic!("unexpected gateway PUT error: {e}"),
        }
    }
    panic!("PUT to {key} never committed within the retry bound");
}

/// The committed directory entry set — the real namespace read the list model checks: scan
/// the shared store's `dirent:0/dir/` keys (a consistent cut, `crates/testkit/src/lib.rs`
/// `contract_scan_is_consistent_cut`).
async fn list_dir(store: &RedbMetadataStore) -> Vec<String> {
    let prefix = format!("dirent:{ROOT}/{DIR_PREFIX}");
    store
        .scan(prefix.as_bytes())
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(k, _)| {
            std::str::from_utf8(&k)
                .ok()
                .and_then(|s| s.strip_prefix(&prefix))
                .map(str::to_string)
        })
        .collect()
}

/// One workload process: a barriered contended overwrite of the hot key (forces overlap),
/// then a read + further overwrites, an **uncontended** per-process register key (whose
/// single-writer own-writes give the session checks real data to compare against — the RYW
/// floor and monotonic last-version comparisons execute on produced values, and a correct
/// gateway passes them, which is what the workload asserts), and directory create/delete/list.
///
/// Every committed overwrite is recorded WITH its captured commit version (via
/// [`versioned_put`]); the shared HOT key is written under `hot_lock` so its version is
/// attributed exactly even under multi-writer contention (iteration-2 carry-forward). The
/// produced register history therefore contains **no** `version=None` committed write, so the
/// register model's contended-path detections (stale read / two winners / superseded value /
/// lost write) run on real captured versions rather than being switched off.
async fn run_process(
    p: u32,
    gw: Arc<WorkGateway>,
    store: Arc<RedbMetadataStore>,
    barrier: Arc<Barrier>,
    hot_lock: Arc<tokio::sync::Mutex<()>>,
    values: Arc<AtomicU64>,
    recorder: Arc<Mutex<HistoryRecorder>>,
) {
    // ── Round 1: a barriered overwrite of the HOT key. Every process records its invoke
    //    before the barrier releases and its ok after — so all invokes precede all oks and
    //    the recorded history is guaranteed to contain overlapping ops (not a serial log). ──
    let v0 = values.fetch_add(1, Ordering::Relaxed);
    recorder.lock().unwrap().write_invoke(p, HOT, v0);
    barrier.wait().await;
    let ver0 = versioned_put(&gw, &store, Some(&hot_lock), HOT, v0).await;
    recorder.lock().unwrap().write_ok(p, HOT, v0, Some(ver0));

    // A read of the contended hot key.
    recorder.lock().unwrap().read_invoke(p, HOT);
    match observe(&store, HOT).await {
        Some((value, version)) => recorder.lock().unwrap().read_ok(p, HOT, value, version),
        None => recorder.lock().unwrap().read_absent(p, HOT),
    }

    // Another contended overwrite of the hot key (keeps the version climbing under contention).
    let v1 = values.fetch_add(1, Ordering::Relaxed);
    recorder.lock().unwrap().write_invoke(p, HOT, v1);
    let ver1 = versioned_put(&gw, &store, Some(&hot_lock), HOT, v1).await;
    recorder.lock().unwrap().write_ok(p, HOT, v1, Some(ver1));

    // An uncontended per-process register key: only this process writes it, so its commit
    // versions are always observable and its reads are its own writes — the read-your-writes
    // and monotonic-read comparison logic runs on real produced own-writes (a correct gateway
    // never trips the reject there, which the produced-history assertions confirm).
    let pk = format!("reg/p{p}");
    for _ in 0..2 {
        let v = values.fetch_add(1, Ordering::Relaxed);
        recorder.lock().unwrap().write_invoke(p, &pk, v);
        let ver = versioned_put(&gw, &store, None, &pk, v).await;
        recorder.lock().unwrap().write_ok(p, &pk, v, Some(ver));

        recorder.lock().unwrap().read_invoke(p, &pk);
        match observe(&store, &pk).await {
            Some((value, version)) => recorder.lock().unwrap().read_ok(p, &pk, value, version),
            None => recorder.lock().unwrap().read_absent(p, &pk),
        }
    }

    // ── Namespace: create two entries, delete one, then list the committed directory. The
    //    in-process gateway has no atomic rename (`ObjectGateway` is put/get/delete), so the
    //    workload drives create/delete/list; the rename model branch is covered by the
    //    crafted histories above. ──
    for k in 0..2 {
        let name = format!("p{p}-{k}");
        recorder.lock().unwrap().create_invoke(p, &name);
        let key = format!("{DIR_PREFIX}{name}");
        match gw.put_object(&key, b"x").await {
            Ok(()) => recorder.lock().unwrap().create_ok(p, &name),
            Err(e) if is_conflict(&e) => recorder.lock().unwrap().create_fail(p, &name),
            Err(e) => panic!("unexpected create error: {e}"),
        }
    }
    let gone = format!("p{p}-0");
    recorder.lock().unwrap().delete_invoke(p, &gone);
    gw.delete_object(&format!("{DIR_PREFIX}{gone}"))
        .await
        .expect("delete must succeed");
    recorder.lock().unwrap().delete_ok(p, &gone);

    recorder.lock().unwrap().list_invoke(p);
    let names = list_dir(&store).await;
    recorder.lock().unwrap().list_ok(p, names);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workload_against_the_in_process_gateway_yields_a_nonvacuous_checkable_history() {
    let dir = tempfile::tempdir().expect("temp chunk dir");
    let store = Arc::new(RedbMetadataStore::in_memory().expect("in-memory redb"));
    let gw = Arc::new(
        Gateway::new(
            SharedRedb(store.clone()),
            FsChunkStore::open(dir.path()).expect("fs chunk store"),
            MemCoordination::new(),
        )
        .with_chunk_size(64),
    );
    gw.recover()
        .await
        .expect("recover id allocators (fresh store: a no-op)");

    let barrier = Arc::new(Barrier::new(PROCESSES as usize));
    // Serializes the commit+observe of the shared HOT key so each writer attributes its EXACT
    // commit version even under multi-writer contention (the gateway PUT does not return it).
    let hot_lock = Arc::new(tokio::sync::Mutex::new(()));
    let values = Arc::new(AtomicU64::new(1)); // value tokens are payload lengths >= 1
    let recorder = Arc::new(Mutex::new(HistoryRecorder::new()));

    let mut tasks = Vec::new();
    for p in 0..PROCESSES {
        tasks.push(tokio::spawn(run_process(
            p,
            gw.clone(),
            store.clone(),
            barrier.clone(),
            hot_lock.clone(),
            values.clone(),
            recorder.clone(),
        )));
    }
    for t in tasks {
        t.await.expect("workload process must not panic");
    }

    // A final read (a fresh observer session) pins the hot key's climbed version into the
    // history and asserts the overwrites really bumped the commit point past 1.
    {
        let observer = PROCESSES;
        recorder.lock().unwrap().read_invoke(observer, HOT);
        let (value, version) = observe(&store, HOT).await.expect("hot key exists");
        recorder
            .lock()
            .unwrap()
            .read_ok(observer, HOT, value, version);
        assert!(
            version >= 2,
            "concurrent overwrites must bump the inode version past its commit_create value of 1"
        );
    }

    let history = Arc::try_unwrap(recorder)
        .expect("all workload tasks have completed")
        .into_inner()
        .expect("recorder mutex");

    // The register model PASSES the produced history — the non-vacuous, checkable history
    // ADR-0041's substrate exists to yield.
    check_register(&history.register).unwrap_or_else(|e: RegisterViolation| {
        panic!("produced register history not linearizable: {e}")
    });
    check_read_your_writes(&history.register)
        .unwrap_or_else(|e| panic!("produced history breaks read-your-writes: {e}"));
    check_monotonic_reads(&history.register)
        .unwrap_or_else(|e| panic!("produced history breaks monotonic reads: {e}"));
    check_list_append(&history.namespace).unwrap_or_else(|e: NamespaceViolation| {
        panic!("produced namespace history not linearizable: {e}")
    });

    // Non-vacuous: genuinely overlapping ops AND a real commit-point version bump.
    assert!(
        max_register_concurrency(&history.register) >= 2,
        "the history must contain genuinely concurrent, overlapping ops (not a serial log)"
    );
    assert!(
        max_observed_version(&history.register) >= 2,
        "a real overwrite must have bumped the inode version at the commit point"
    );

    // Contended-path teeth (issue #406, iteration-2): EVERY committed write in the produced
    // history carries its real commit version — none was dropped to `None`. This is what makes
    // `check_register` above a NON-VACUOUS pass on the contended ops: with a version on every
    // committed write, its version-keyed detections (two winners / superseded value / version
    // regression) are ACTIVE, not switched off. The history is genuinely contended, too: the
    // hot key was overwritten by multiple processes past its create version.
    let committed_writes: Vec<&RegOp> = history
        .register
        .iter()
        .filter(|o| matches!(o.kind, Kind::Ok) && o.f == RegF::Write)
        .collect();
    assert!(
        committed_writes.len() >= (PROCESSES as usize) * 2,
        "the workload must produce real committed overwrites (contended hot key + per-process keys)"
    );
    assert!(
        committed_writes.iter().all(|o| o.version.is_some()),
        "every committed write must record its captured commit version (no version=None on the contended path)"
    );

    // The recorder serializes to the checker-compatible Elle op-map format.
    let edn = register_to_edn(&history.register);
    assert!(
        edn.contains(":type :invoke") && edn.contains("[:w "),
        "register EDN: {edn}"
    );
    let ns_edn = namespace_to_edn(&history.namespace);
    assert!(ns_edn.contains("[:append :dir "), "namespace EDN: {ns_edn}");

    drop(dir);
}
