//! M3.9 (issue #195) — the **Check-running** core of the Tier-1 disk-fault campaign:
//! reconstruction must drive a chunk back to full redundancy when one of its placed
//! fragments is lost to a **block-layer read fault** (a dead sector / dead disk
//! surfacing `EIO`), exactly as the real `dm-error` / `dm-flakey` privileged scenario
//! (`tier1_disk_faults.rs`) exercises it against a device-mapper-faulted device.
//!
//! This is the in-process, root-free complement that the unprivileged `cargo xtask ci`
//! gate runs: it injects the **same loss** the real disk fault produces — a faulted D
//! server whose `get_fragment` returns `Err` (an unreadable fragment) — over the
//! in-memory trait stores, and drives the **production** repair path
//! ([`reconcile_step`] → [`reconstruction::reconcile`]) over it.
//!
//! The fault is kept **inside the reconstruction fleet view** (the victim is NOT
//! pre-excluded): the unreadable fragment must drive loss classification through the
//! production read in `reconstruction::assess`, the same branch the real block-layer
//! harness exists to flush. That read used to be `store.get_fragment(frag).await?` —
//! the `?` propagated the `EIO` and **aborted the whole reconciliation**, so one
//! faulted disk stalled repair for every queued chunk. The fix (mirroring the read
//! path) reads around an unreadable fragment, treating it as missing and rebuilding it
//! (`reconstruction.rs::assess`; ADR-0009 — a real-world discovery is a seeded
//! regression).
//!
//! Red→green contract (run-verify, the C4-verify gate): with the production read-around
//! reverted this test is **RED** — `reconcile_step` returns the propagated `EIO` and the
//! `.expect()` below panics. With the fix it is **GREEN** — the faulted fragment is
//! rebuilt onto a healthy spare and the chunk is back to full redundancy on `n` distinct
//! domains. The fault is load-bearing: a victim that returned valid bytes would leave the
//! chunk already whole (`Assessment::Drain`, no `Reconciled::Changed`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use async_trait::async_trait;
use bytes::Bytes;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{self, EcScheme, InodeId, InodeRecord};
use wyrd_core::placement::Topology;
use wyrd_core::read::read_object;
use wyrd_core::repair;
use wyrd_core::write::write_new_object_placed;
use wyrd_custodian::{reconcile_step, Custodian, FencedZone, Reconciled, ReconstructionContext};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore,
    PlacementChunkStore, Result, WriteBatch,
};

const ROOT: InodeId = 0;
const INODE: InodeId = 1;
const CHUNK: ChunkId = 0xC0FFEE;
/// Server 1 (domain B) holds fragment index 1; its store is the one we fault.
const VICTIM: DServerId = 1;

fn frag(index: u16) -> FragmentId {
    FragmentId {
        chunk: CHUNK,
        index,
    }
}

// ---- in-memory metadata store (the loop is proven over the trait seam) ----

#[derive(Default)]
struct MemMeta {
    kv: Mutex<HashMap<Vec<u8>, Bytes>>,
}

#[async_trait]
impl MetadataStore for MemMeta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.kv.lock().unwrap().get(key).cloned())
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        Ok(self
            .kv
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        let mut kv = self.kv.lock().unwrap();
        for pre in &batch.preconditions {
            if kv.get(&pre.key).cloned() != pre.expected {
                return Ok(CommitOutcome::Conflict);
            }
        }
        for (k, v) in batch.puts {
            kv.insert(k, v);
        }
        for k in batch.deletes {
            kv.remove(&k);
        }
        Ok(CommitOutcome::Committed)
    }
}

/// One D server's fragment bytes, with a **block-layer fault switch**. While healthy it
/// behaves like a normal store; once [`fault`](FaultyDServer::fault) is flipped, every
/// `get_fragment`/`put_fragment`/`list_fragments` returns an `EIO` `Err` — modelling a
/// `dm-error` device that fails ALL I/O, the exact loss the privileged Tier-1 scenario
/// injects with `dmsetup`. The fault is flipped **after** the object is written, so the
/// data lands healthy and then the disk goes bad (the realistic "disk dies after the
/// write" model the real harness reproduces).
#[derive(Default)]
struct FaultyDServer {
    frags: Mutex<HashMap<FragmentId, Bytes>>,
    faulted: AtomicBool,
}

impl FaultyDServer {
    fn fault(&self) {
        self.faulted.store(true, Ordering::SeqCst);
    }

    fn eio() -> wyrd_traits::BoxError {
        // EIO (5) is what a `dm-error` mapped device returns for every I/O — a real
        // block-layer read fault, distinct from `NotFound` (which `FsChunkStore` maps to
        // `Ok(None)`). This is the error reconstruction used to propagate-and-abort on.
        Box::new(std::io::Error::from_raw_os_error(5))
    }
}

#[async_trait]
impl ChunkStore for FaultyDServer {
    async fn put_fragment(&self, id: FragmentId, fragment: Bytes) -> Result<()> {
        if self.faulted.load(Ordering::SeqCst) {
            return Err(Self::eio());
        }
        self.frags.lock().unwrap().insert(id, fragment);
        Ok(())
    }

    async fn get_fragment(&self, id: FragmentId) -> Result<Option<Bytes>> {
        if self.faulted.load(Ordering::SeqCst) {
            return Err(Self::eio());
        }
        Ok(self.frags.lock().unwrap().get(&id).cloned())
    }

    async fn list_fragments(&self) -> Result<Vec<FragmentId>> {
        if self.faulted.load(Ordering::SeqCst) {
            return Err(Self::eio());
        }
        Ok(self.frags.lock().unwrap().keys().copied().collect())
    }

    async fn delete_fragment(&self, id: FragmentId) -> Result<()> {
        if self.faulted.load(Ordering::SeqCst) {
            return Err(Self::eio());
        }
        self.frags.lock().unwrap().remove(&id);
        Ok(())
    }

    async fn health(&self) -> Result<Health> {
        Ok(Health::Healthy)
    }
}

/// A placement-aware fleet over several [`FaultyDServer`]s — routes each `_at` call to
/// the D server the placement record names, the same shape the production read path and
/// the reconstruction loop resolve fragments over.
struct Fleet<'a> {
    servers: Vec<(DServerId, &'a FaultyDServer)>,
}

impl<'a> Fleet<'a> {
    fn store(&self, dserver: DServerId) -> Option<&'a FaultyDServer> {
        self.servers
            .iter()
            .find(|(id, _)| *id == dserver)
            .map(|(_, s)| *s)
    }
}

#[async_trait]
impl ChunkStore for Fleet<'_> {
    async fn put_fragment(&self, id: FragmentId, fragment: Bytes) -> Result<()> {
        if let Some(store) = self.store(DServerId::from(id.index)) {
            store.put_fragment(id, fragment).await?;
        }
        Ok(())
    }

    async fn get_fragment(&self, id: FragmentId) -> Result<Option<Bytes>> {
        for (_, store) in &self.servers {
            if let Some(bytes) = store.get_fragment(id).await? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }

    async fn list_fragments(&self) -> Result<Vec<FragmentId>> {
        let mut all = Vec::new();
        for (_, store) in &self.servers {
            all.extend(store.list_fragments().await?);
        }
        Ok(all)
    }

    async fn delete_fragment(&self, id: FragmentId) -> Result<()> {
        for (_, store) in &self.servers {
            store.delete_fragment(id).await?;
        }
        Ok(())
    }

    async fn health(&self) -> Result<Health> {
        Ok(Health::Healthy)
    }
}

#[async_trait]
impl PlacementChunkStore for Fleet<'_> {
    async fn get_fragment_at(&self, dserver: DServerId, id: FragmentId) -> Result<Option<Bytes>> {
        match self.store(dserver) {
            Some(store) => store.get_fragment(id).await,
            None => Ok(None),
        }
    }

    async fn put_fragment_at(
        &self,
        dserver: DServerId,
        id: FragmentId,
        fragment: Bytes,
    ) -> Result<()> {
        if let Some(store) = self.store(dserver) {
            store.put_fragment(id, fragment).await?;
        }
        Ok(())
    }
}

async fn read_inode(meta: &MemMeta) -> InodeRecord {
    let bytes = meta
        .get(&metadata::inode_key(INODE))
        .await
        .unwrap()
        .expect("inode present");
    metadata::decode(&bytes).unwrap()
}

/// The BINDING Tier-1 success criterion (`0005:381-384`), proven at Check over the
/// production reconstruction path: a chunk whose fragment is lost to a **block-layer read
/// fault** (the victim D server returns `EIO`) — with the victim KEPT in the
/// reconstruction fleet view — is driven **back to full redundancy** with **no read
/// error** throughout, never aborting the reconciliation.
#[tokio::test]
async fn reconstructs_a_chunk_whose_fragment_is_lost_to_a_block_layer_read_fault() {
    let meta = MemMeta::default();
    let (d0, d1, d2, d3) = (
        FaultyDServer::default(),
        FaultyDServer::default(),
        FaultyDServer::default(),
        FaultyDServer::default(),
    );
    let fleet = Fleet {
        servers: vec![(0, &d0), (1, &d1), (2, &d2), (3, &d3)],
    };

    // WRITE an RS(2,1) object across distinct domains (servers 0,1,2 → A,B,C) while every
    // device is still healthy — the data lands before the disk goes bad.
    let data = b"reconstruct this erasure-coded chunk over a faulted block device".to_vec();
    let mut topo = Topology::default();
    topo.register(0, "A")
        .register(1, "B")
        .register(2, "C")
        .register(3, "D");
    let outcome = write_new_object_placed(
        &meta,
        &fleet,
        ROOT,
        "obj",
        INODE,
        &data,
        data.len(),
        EcScheme::ReedSolomon { k: 2, m: 1 },
        &topo,
        0,
        1_000,
        || CHUNK,
    )
    .await
    .expect("write the object");
    assert_eq!(outcome, CommitOutcome::Committed);
    assert_eq!(
        read_inode(&meta).await.chunk_map[0].placement,
        vec![0, 1, 2],
        "RS(2,1) placed across distinct domains A,B,C (servers 0,1,2)"
    );

    // INJECT the block-layer fault: the victim (server 1, domain B) now fails ALL I/O
    // with EIO — the same loss a `dm-error` mapped device produces. A health finding
    // enqueues the chunk on the shared repair queue.
    d1.fault();
    repair::enqueue_repair(&meta, CHUNK, "health")
        .await
        .unwrap();

    // READ during repair: the victim's fragment now errors at the block layer, but the
    // read path reads around it off the k=2 survivors — it must NOT error (degraded read).
    assert_eq!(
        read_object(&meta, &fleet, INODE).await.unwrap(),
        Some(data.clone()),
        "object reads correctly while a block-layer fault makes the victim fragment unreadable"
    );

    // RECONSTRUCT through the real fenced control point, with the victim KEPT IN the
    // fleet view (NOT pre-excluded): the unreadable fragment drives loss classification
    // through the production read in `assess`. Domain B (the victim) is heavily utilized
    // so the rebuilt fragment moves to the free, healthy domain D (server 3) — proving
    // the chunk is driven back to full redundancy on a HEALTHY device, off the fault.
    let mut recon_topo = Topology::default();
    recon_topo
        .register(0, "A")
        .register(1, "B")
        .register(2, "C")
        .register(3, "D")
        .set_utilization(VICTIM, 100);
    let recon_fleet: [(DServerId, &dyn ChunkStore); 4] = [(0, &d0), (1, &d1), (2, &d2), (3, &d3)];
    let ctx = ReconstructionContext {
        meta: &meta,
        fleet: &recon_fleet,
        topology: &recon_topo,
    };

    let coord = MemCoordination::new();
    let leader = Custodian::elect(&coord, "zone-tier1").await.unwrap();
    let mut zone = FencedZone::new();
    zone.install(leader.leadership());

    // Pre-fix this `.expect` PANICS: `assess` propagates the victim's EIO and
    // `reconcile_step` returns it. Post-fix the fault is read around and the rebuild
    // commits.
    let repaired = reconcile_step(&zone, &leader, None, None, Some(&ctx), None, 200)
        .await
        .expect("reconstruction must read around the faulted fragment, not abort on its EIO");
    assert_eq!(
        repaired,
        Reconciled::Changed,
        "the chunk lost to a block-layer read fault was reconstructed"
    );

    // The obligation is drained, exactly one version-conditional commit landed, and the
    // chunk no longer references the faulted victim server.
    assert!(
        repair::queued_repairs(&meta).await.unwrap().is_empty(),
        "the repair obligation is drained by the reconstruction commit"
    );
    let record = read_inode(&meta).await;
    assert_eq!(record.version, 2, "exactly one version-conditional commit");
    assert_eq!(
        record.chunk_map[0].placement,
        vec![0, 3, 2],
        "the rebuilt fragment moved off the faulted victim onto the healthy spare (domain D)"
    );
    assert!(
        !record.chunk_map[0].placement.contains(&VICTIM),
        "the faulted server no longer holds a referenced fragment"
    );

    // FULL REDUNDANCY: every placed fragment is present, verifies its checksum, and the
    // n fragments occupy n distinct failure domains (A, D, C) — read off the healthy set.
    for (index, server) in [(0u16, &d0), (1, &d3), (2, &d2)] {
        let bytes = server
            .get_fragment(frag(index))
            .await
            .unwrap()
            .expect("fragment present after repair");
        assert!(
            repair::fragment_intact(&bytes, CHUNK),
            "fragment {index} verifies its checksum and belongs to the chunk"
        );
    }
    let domains: std::collections::HashSet<_> = record.chunk_map[0]
        .placement
        .iter()
        .map(|id| recon_topo.domain_of(*id).unwrap().clone())
        .collect();
    assert_eq!(
        domains.len(),
        3,
        "n fragments on n distinct failure domains"
    );

    // The object still reads byte-identical after repair, off the now-full-redundancy set.
    assert_eq!(
        read_object(&meta, &fleet, INODE).await.unwrap(),
        Some(data),
        "object reads byte-identical after repair (full redundancy, atomic flip)"
    );
}
