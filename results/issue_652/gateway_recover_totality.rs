//! `Gateway::recover()` — the step `Gateway::new`'s caller runs **before the gateway serves
//! anything** (`crates/server/src/lib.rs:123-125`) — must be **total** (issue #652): no
//! arrangement of the metadata store's contents may make it refuse, because an `Err` here
//! costs every *healthy* object its availability, not just a damaged one.
//!
//! `metadata::high_water_marks` (`crates/core/src/metadata.rs:2117`, the walk `recover`
//! drives) failed that in two independent ways before this patch:
//!
//! 1. It `decode(&value)?`-ed every `inode:` record, so one undecodable value made the
//!    whole call `Err` (`crates/core/src/metadata.rs:2081` pre-patch).
//! 2. It read its namespaces with [`MetadataStore::scan`], which is complete-or-fail-loud at
//!    `SCAN_CAP` (`crates/traits/src/lib.rs:286`) rather than the bounded-page seam
//!    `scan_page` exists to escape (`crates/traits/src/lib.rs:1086-1087`), so a store too
//!    large to `scan` also stopped the gateway from starting.
//!
//! Both are driven through the **signature-stable** `Gateway::recover() -> Result<()>`
//! rather than `high_water_marks` directly, deliberately: this patch changes
//! `high_water_marks`'s own signature (it no longer returns a chunk-id floor — see below),
//! so a test naming it would fail to COMPILE on the pre-fix tree and be scored as a pass
//! rather than a red assertion.
//!
//! A third criterion this target binds: the chunk-id floor `high_water_marks` used to
//! compute alongside the inode mark is gone. It has had no caller since #487 (`fdd34f1`,
//! 2026-07-08) moved chunk-id minting to a coordination-free scheme (every id `mint_chunk_id`
//! mints is ≥ 2^127, `crates/server/src/lib.rs:238-241`) and rewrote `recover`'s callsite to
//! bind it to a discarded, unused name (pre-patch `crates/server/src/lib.rs:124`). A number
//! nobody reads is not a safety property (issue #635/#652 Scope) — this patch removes it
//! rather than repairing it, which also removes the two unbounded `scan`s (`pending:`,
//! `orphan:`) that existed only to feed it. A repo-wide search for that discarded binding's
//! identifier over the patched tree's `crates/` returns nothing; that search is the
//! mechanical proof and is run at Check, not duplicated here as a Rust assertion.
//!
//! Composition (`RedbMetadataStore` + `FsChunkStore` + `MemCoordination`) and the
//! seed-legacy-store technique both mirror the existing precedent this target must not
//! regress, `crates/server/tests/s3_http_wire.rs:665-751`.

#![forbid(unsafe_code)]

use std::io;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::inode_key;
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_server::logging::{dispatch, LogConfig};
use wyrd_server::Gateway;
use wyrd_traits::{CommitOutcome, MetadataStore, Result, ScanCapExceeded, ScanPage, WriteBatch};

type Backend = Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>;

/// `cli::NEXT_INODE_KEY` is crate-private; the on-disk key it single-sources is
/// `meta:next_inode` (`crates/server/src/cli.rs:56`, mirrors `s3_http_wire.rs:671`).
const NEXT_INODE_KEY: &[u8] = b"meta:next_inode";

/// A `MakeWriter` that appends every emitted line into a shared buffer, so a test can read
/// back exactly what a subscriber wrote. Mirrors
/// `crates/server/tests/log_span_correlation.rs:21-45`.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'w> tracing_subscriber::fmt::MakeWriter<'w> for Capture {
    type Writer = Self;
    fn make_writer(&'w self) -> Self::Writer {
        self.clone()
    }
}

/// A [`MetadataStore`] double whose [`scan`](MetadataStore::scan) always refuses as if the
/// namespace exceeded `SCAN_CAP`, while every other method — crucially
/// [`scan_page`](MetadataStore::scan_page) — forwards unchanged to a real
/// [`RedbMetadataStore`]. Criterion 2 needs this: the only way `Gateway::recover()` can
/// still return `Ok(())` over this store is by reading its namespaces through `scan_page`
/// and never calling `scan` at all.
struct ScanCapExceededStore {
    inner: RedbMetadataStore,
}

#[async_trait]
impl MetadataStore for ScanCapExceededStore {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        self.inner.get(key).await
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        Err(Box::new(ScanCapExceeded {
            cap: wyrd_traits::SCAN_CAP,
            prefix: prefix.to_vec(),
        }))
    }

    async fn scan_page(
        &self,
        prefix: &[u8],
        after: Option<&[u8]>,
        limit: usize,
    ) -> Result<ScanPage> {
        self.inner.scan_page(prefix, after, limit).await
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        self.inner.commit(batch).await
    }
}

/// Criterion 1 — **total over damage**. A healthy committed object survives `recover()`
/// alongside a raw, undecodable `inode:` value, `recover()` still returns `Ok(())`, the
/// damaged record's id (read from its **key**, which decodes even when the value does not)
/// raises the allocator floor, and the record is attributed on the audit seam rather than
/// silently skipped.
///
/// Pre-fix RED: `high_water_marks` decodes every `inode:` value with `decode(&value)?`
/// (pre-patch `crates/core/src/metadata.rs:2081`), so the damaged record makes the whole
/// scan `Err`, `Gateway::recover()` propagates it, and this test's first `expect` fails.
#[test]
fn recover_is_total_over_an_undecodable_inode_record_and_attributes_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("meta.redb");
    let frags = dir.path().join("frags");

    // A healthy committed object (inode 1) plus a raw, undecodable `inode:` value at a
    // HIGHER id than the healthy object's — so the allocator floor this test checks for
    // must come from the damaged record's KEY, not from the healthy object.
    const DAMAGED_INODE: u64 = 50;
    {
        let gateway = Backend::new(
            RedbMetadataStore::open(&db_path).expect("redb"),
            FsChunkStore::open(&frags).expect("fs store"),
            MemCoordination::new(),
        );
        pollster::block_on(gateway.put_object("bucket/healthy", b"still readable"))
            .expect("PUT the healthy object");
    }
    {
        let meta = RedbMetadataStore::open(&db_path).expect("redb reopen to seed damage");
        pollster::block_on(meta.commit(WriteBatch::new().put(
            inode_key(DAMAGED_INODE),
            Bytes::from_static(b"not json at all"),
        )))
        .expect("seed the undecodable inode record");
    }

    // recover() over the damaged store, with tracing captured so the attribution can be
    // asserted rather than merely trusted.
    let recovered = {
        let gateway = Backend::new(
            RedbMetadataStore::open(&db_path).expect("redb reopen"),
            FsChunkStore::open(&frags).expect("fs reopen"),
            MemCoordination::new(),
        );
        let capture = Capture::default();
        let dispatch = dispatch(
            &LogConfig::new(Some("warn"), None).unwrap(),
            capture.clone(),
            tracing_subscriber::layer::Identity::new(),
        );
        let outcome =
            tracing::dispatcher::with_default(&dispatch, || pollster::block_on(gateway.recover()));
        outcome.as_ref().expect(
            "recover() must be Ok(()) despite one undecodable inode: record — criterion 1 \
             (totality over damage)",
        );
        let log = capture.contents();
        assert!(
            log.contains("inode:50"),
            "the damaged record must be attributed on the audit seam (its key logged), not \
             silently swallowed — captured log: {log}"
        );

        // The healthy object still reads back byte-identically — the damaged record cost
        // it nothing.
        assert_eq!(
            pollster::block_on(gateway.get_object("bucket/healthy"))
                .expect("get the healthy object")
                .as_deref(),
            Some(&b"still readable"[..]),
            "the healthy object must survive recover() over a store that also holds a \
             damaged inode: record, byte-identical",
        );
        outcome
    };
    assert!(recovered.is_ok());

    // The persisted allocator was seeded strictly above the damaged record's key id: its id
    // is recovered from the key even though the value never decoded.
    let next_inode: u64 = {
        let meta = RedbMetadataStore::open(&db_path).expect("redb reopen to inspect counter");
        let counter = pollster::block_on(meta.get(NEXT_INODE_KEY))
            .expect("get counter")
            .expect("recover() must persist meta:next_inode");
        std::str::from_utf8(&counter).unwrap().parse().unwrap()
    };
    assert!(
        next_inode > DAMAGED_INODE,
        "recover() must seed the allocator strictly above the damaged record's KEY id \
         ({DAMAGED_INODE}) even though its VALUE never decoded — got next_inode={next_inode}",
    );

    // And a subsequent new-key PUT commits — the id it mints (== next_inode above, by
    // alloc_inode's CAS) is strictly greater than the damaged record's id.
    {
        let gateway = Backend::new(
            RedbMetadataStore::open(&db_path).expect("redb reopen"),
            FsChunkStore::open(&frags).expect("fs reopen"),
            MemCoordination::new(),
        );
        pollster::block_on(gateway.put_object("bucket/after-recover", b"new object")).expect(
            "a new-key PUT after recover() must commit, minting an inode id strictly above \
             the damaged record's key id",
        );
        assert_eq!(
            pollster::block_on(gateway.get_object("bucket/after-recover"))
                .expect("get the new object")
                .as_deref(),
            Some(&b"new object"[..]),
        );
    }
}

/// Criterion 2 — **total over size**. `recover()` must return `Ok(())` even when
/// [`MetadataStore::scan`] refuses as if the namespace were past `SCAN_CAP`, and it must
/// seed the same floor a healthy `scan`-backed run would — i.e. `high_water_marks` reads
/// its namespaces through the bounded-page seam ([`MetadataStore::scan_page`]), never
/// through `scan`.
///
/// Pre-fix RED: `high_water_marks` reads `inode:` (and, pre-patch, `pending:`/`orphan:`)
/// with `store.scan(...)` (pre-patch `crates/core/src/metadata.rs:2077,2094,2105`), so over
/// [`ScanCapExceededStore`] every one of those calls returns `Err(ScanCapExceeded)`,
/// `Gateway::recover()` propagates it, and this test's `expect` fails.
#[test]
fn recover_is_total_over_a_store_whose_scan_refuses_but_scan_page_works() {
    // Sanity: prove the double's own scan() really refuses, decoupled from the file-backed
    // scenario below — so the totality this test proves comes from avoiding `scan`
    // altogether, not from the double having forgotten to implement the refusal.
    let sanity = ScanCapExceededStore {
        inner: RedbMetadataStore::in_memory().expect("redb in-memory"),
    };
    assert!(
        pollster::block_on(MetadataStore::scan(&sanity, b"inode:")).is_err(),
        "the store double's scan() must refuse, or this test never exercises scan_page",
    );

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("meta.redb");
    let frags = dir.path().join("frags");

    // Reproduce the legacy-store shape `recover` exists for (mirrors
    // `s3_http_wire.rs:677-697`): a committed object under `inode:` but no persisted
    // `meta:next_inode` counter — so `recover` is load-bearing for the PUT below, not a
    // no-op the test would pass without exercising anything.
    {
        let gateway = Backend::new(
            RedbMetadataStore::open(&db_path).expect("redb"),
            FsChunkStore::open(&frags).expect("fs store"),
            MemCoordination::new(),
        );
        pollster::block_on(gateway.put_object("bucket/a", b"first")).expect("PUT A");
    }
    {
        let meta = RedbMetadataStore::open(&db_path).expect("redb reopen to strip counter");
        pollster::block_on(meta.commit(WriteBatch::new().delete(NEXT_INODE_KEY.to_vec())))
            .expect("strip meta:next_inode to model a legacy store");
    }

    // `scan` on THIS store always refuses; only `scan_page` (forwarded to the real backend)
    // can read it.
    let gateway = Gateway::new(
        ScanCapExceededStore {
            inner: RedbMetadataStore::open(&db_path).expect("redb reopen"),
        },
        FsChunkStore::open(&frags).expect("fs reopen"),
        MemCoordination::new(),
    );
    pollster::block_on(gateway.recover()).expect(
        "recover() must be Ok(()) even though scan() refuses on every call — criterion 2 \
         (totality over size): the walk must read its namespaces in bounded pages via \
         scan_page, never through scan",
    );
    pollster::block_on(gateway.put_object("bucket/b", b"second")).expect(
        "after recover() over the scan-refusing double, a new-key PUT must commit without \
         colliding with the legacy inode — the floor was seeded from scan_page",
    );
    assert_eq!(
        pollster::block_on(gateway.get_object("bucket/a"))
            .expect("get A")
            .as_deref(),
        Some(&b"first"[..]),
        "legacy object A survives, byte-identical",
    );
    assert_eq!(
        pollster::block_on(gateway.get_object("bucket/b"))
            .expect("get B")
            .as_deref(),
        Some(&b"second"[..]),
        "object B stored under a fresh, non-colliding inode",
    );
}
