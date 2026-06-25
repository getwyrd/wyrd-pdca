//! Tier-1 **disk-fault scenario** (M3.9, issue #195) — the real-block-layer complement
//! to the Tier-0 DST custodian campaign (`crates/dst/tests/custodian.rs`). Born at
//! Tier-1 per proposal 0005 §"DST and tests" → Tier-1 disk-fault injection
//! (`0005:405-408`) and the `xtask` touch-point `0005:437`.
//!
//! Where Tier-0 *models* bit rot / fragment loss in memory under madsim, this scenario
//! drives the **production** custodian repair path —
//! [`reconcile_step`] → [`scrub::reconcile`] / [`reconstruction::reconcile`] — over
//! **real** [`FsChunkStore`] D servers, one of which is rooted on a device-mapper
//! (`dm-flakey` / `dm-error`) faulted block device. It asserts the Tier-1 success
//! criterion (`0005:381-384`): a chunk whose fragment is lost to a real block-layer
//! fault is driven **back to full redundancy** and the object **reads without error
//! throughout the repair** (degraded reads succeed off the `k` survivors).
//!
//! The faulted victim is kept **inside the reconstruction fleet view** (it is NOT
//! pre-excluded): the real `EIO` the device returns must drive loss classification
//! through the production read in `reconstruction::assess` — the same branch the
//! root-free [`reconstruction_read_fault`] test pins at Check. Reconstruction reads
//! around the unreadable fragment and rebuilds it onto the healthy spare; the
//! production read-around is the fix this harness flushed out (ADR-0009).
//!
//! It is **gated** exactly like the Tier-2 container scenario
//! (`crates/chunkstore-grpc/tests/tier2_integration.rs`): `#[ignore]`d so the default
//! `cargo test` (and the container-free `cargo xtask ci`) only **compiles and
//! type-checks** it — proving the harness is real, API-bound Rust against the production
//! `FsChunkStore` / `reconcile_step` / `scrub` / `reconstruction` surface, not an
//! env-var shell string. Its body needs root + `dmsetup`, so it runs only under the
//! privileged off-Check job `cargo xtask disk-faults`, which stands up the faulted
//! device, exports the roots + the dm error-table below, and reads back the campaign
//! report this writes.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_coordination_mem::MemCoordination;
use wyrd_core::metadata::{self, EcScheme, InodeId, InodeRecord};
use wyrd_core::placement::Topology;
use wyrd_core::read::read_object;
use wyrd_core::repair;
use wyrd_core::write::write_new_object_placed;
use wyrd_custodian::{
    reconcile_step, Custodian, FencedZone, Reconciled, ReconstructionContext, ScrubContext,
};
use wyrd_traits::{
    ChunkId, ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore,
    PlacementChunkStore, Result,
};

use async_trait::async_trait;
use bytes::Bytes;

// Env contract with `xtask disk-faults` (`crate::disk_faults::run_scenario`).
const VICTIM_ROOT: &str = "WYRD_TIER1_VICTIM_ROOT";
const HEALTHY_ROOT: &str = "WYRD_TIER1_HEALTHY_ROOT";
const DM_NAME: &str = "WYRD_TIER1_DM_NAME";
const DM_ERROR_TABLE: &str = "WYRD_TIER1_DM_ERROR_TABLE";
const REPORT: &str = "WYRD_TIER1_REPORT";

// RS(2,1): 2 data + 1 parity = 3 fragments on servers 0,1,2 across domains A,B,C; server
// 3 (domain D) is the healthy spare a rebuild flips onto — the smallest genuinely
// erasure-coded scheme that survives one loss, so a read is always satisfiable from the
// k=2 survivors throughout the single-server fault (mirrors the Tier-0 campaign).
const K: u8 = 2;
const M: u8 = 1;
const N: usize = (K + M) as usize;
const ROOT: InodeId = 0;
const INODE: InodeId = 1;
const CHUNK: ChunkId = 0xC0FFEE;
/// Server 0 holds fragment index 0; its `FsChunkStore` is rooted on the faulted device.
const VICTIM: DServerId = 0;
/// The healthy spare a rebuilt fragment is re-placed onto (domain D).
const SPARE: DServerId = 3;

// ---- a placement-aware fleet of real `FsChunkStore` D servers ----

/// A [`PlacementChunkStore`] over several real [`FsChunkStore`]s, routing each fragment
/// to the D server its placement record names — the same shape the Tier-0 `Fleet` takes,
/// but backed by on-disk stores so the read path and the repair traverse real files (one
/// of them on the faulted device).
struct FsFleet {
    servers: Vec<(DServerId, Arc<FsChunkStore>)>,
}

impl FsFleet {
    fn store(&self, dserver: DServerId) -> Option<&Arc<FsChunkStore>> {
        self.servers
            .iter()
            .find(|(id, _)| *id == dserver)
            .map(|(_, s)| s)
    }
}

#[async_trait]
impl ChunkStore for FsFleet {
    async fn put_fragment(&self, id: FragmentId, fragment: Bytes) -> Result<()> {
        // Fan-out placement: fragment index `i` lands on D server `i` (placement [0,1,2]).
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
impl PlacementChunkStore for FsFleet {
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

// ---- in-memory metadata store (the loops are proven over the trait seam) ----

#[derive(Default)]
struct MemMeta {
    kv: std::sync::Mutex<std::collections::HashMap<Vec<u8>, Bytes>>,
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

    async fn commit(&self, batch: wyrd_traits::WriteBatch) -> Result<CommitOutcome> {
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

// ---- helpers ----

async fn read_inode(meta: &MemMeta) -> InodeRecord {
    let bytes = meta
        .get(&metadata::inode_key(INODE))
        .await
        .unwrap()
        .expect("inode present");
    metadata::decode(&bytes).unwrap()
}

/// Reload the dm device named by `DM_NAME` to the error/flakey table `xtask` exported —
/// the live block-layer fault injection. The device was created with a healthy *linear*
/// table so the fragments could be written; this flips it to a faulting target so the
/// victim D server's reads now error at the block layer. `suspend` → `load` → `resume`
/// is the dmsetup atomic table swap.
fn inject_disk_fault() {
    let name = std::env::var(DM_NAME).expect("WYRD_TIER1_DM_NAME set by xtask");
    let table = std::env::var(DM_ERROR_TABLE).expect("WYRD_TIER1_DM_ERROR_TABLE set by xtask");
    run_dmsetup(&["suspend", &name]);
    run_dmsetup(&["load", &name, "--table", &table]);
    run_dmsetup(&["resume", &name]);
    // Drop the page cache so subsequent reads actually hit the now-faulted device rather
    // than being served from cached pages (best-effort; the privileged job runs as root).
    let _ = std::fs::write("/proc/sys/vm/drop_caches", b"3");
}

fn run_dmsetup(args: &[&str]) {
    let status = Command::new("dmsetup")
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn dmsetup {args:?}: {e}"));
    assert!(status.success(), "dmsetup {args:?} failed with {status}");
}

/// Open a fresh `FsChunkStore` rooted at `base/<subdir>`, so each D server owns a
/// distinct directory (the victim's under the faulted mount).
fn open_store(base: &Path, subdir: &str) -> Arc<FsChunkStore> {
    let root = base.join(subdir);
    Arc::new(FsChunkStore::open(root).expect("open FsChunkStore"))
}

fn write_report(total: u64, full: u64, read_errors: u64) {
    if let Ok(path) = std::env::var(REPORT) {
        let body = format!(
            "chunks_total={total}\nchunks_full_redundancy={full}\nread_errors_during_repair={read_errors}\n"
        );
        std::fs::write(PathBuf::from(path), body).expect("write campaign report");
    }
}

/// Read the object, counting it as a read error if the read fails (the "no read errors
/// during repair" assertion is built on this count).
async fn read_or_count_error(
    meta: &MemMeta,
    fleet: &FsFleet,
    read_errors: &mut u64,
) -> Option<Vec<u8>> {
    match read_object(meta, fleet, INODE).await {
        Ok(bytes) => bytes,
        Err(_) => {
            *read_errors += 1;
            None
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "Tier-1: needs root + device-mapper — run via cargo xtask disk-faults"]
async fn faulted_chunk_repairs_to_full_redundancy_without_read_errors() {
    let (Ok(victim_root), Ok(healthy_root)) =
        (std::env::var(VICTIM_ROOT), std::env::var(HEALTHY_ROOT))
    else {
        eprintln!(
            "tier1_disk_faults: {VICTIM_ROOT}/{HEALTHY_ROOT} unset — skipping. \
             Run `cargo xtask disk-faults` (root + device-mapper) to drive it."
        );
        return;
    };
    let victim_base = PathBuf::from(victim_root);
    let healthy_base = PathBuf::from(healthy_root);

    // D server 0 (the victim) is rooted on the faulted mount; the rest — including the
    // spare a rebuild flips onto — are on the healthy scratch.
    let servers: Vec<(DServerId, Arc<FsChunkStore>)> = vec![
        (0, open_store(&victim_base, "d0")),
        (1, open_store(&healthy_base, "d1")),
        (2, open_store(&healthy_base, "d2")),
        (SPARE, open_store(&healthy_base, "d3")),
    ];
    let fleet = FsFleet {
        servers: servers.clone(),
    };
    let meta = MemMeta::default();

    // WRITE an RS(2,1) object across distinct domains (servers 0,1,2) over the real
    // on-disk stores while the victim's device is still healthy (linear).
    let data = b"reconstruct this erasure-coded chunk over a real faulted disk".to_vec();
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
        EcScheme::ReedSolomon { k: K, m: M },
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

    let mut read_errors = 0u64;

    // INJECT the real block-layer fault: the victim's device starts erroring all I/O.
    inject_disk_fault();

    // A health/loss finding enqueues the chunk on the shared repair queue.
    repair::enqueue_repair(&meta, CHUNK, "health")
        .await
        .unwrap();

    // READ during repair: the victim's fragment now errors at the block layer, but the
    // read path reads around it off the k=2 survivors — it must NOT error.
    assert_eq!(
        read_or_count_error(&meta, &fleet, &mut read_errors).await,
        Some(data.clone()),
        "object reads correctly while a real disk fault makes the victim fragment unreadable"
    );

    let coord = MemCoordination::new();
    let leader = Custodian::elect(&coord, "zone-tier1").await.unwrap();
    let mut zone = FencedZone::new();
    zone.install(leader.leadership());

    // SCRUB over the HEALTHY survivors + spare (NOT the victim): a dead `dm-error`
    // device fails its `list_fragments` walk too, and scrub treats a non-integrity I/O
    // fault as transient (propagate, by design — `scrub.rs`), so the dead disk is a
    // health finding (enqueued above), not a scrub finding. The survivors' referenced
    // fragments verify clean.
    let scrub_fleet: Vec<(DServerId, &dyn ChunkStore)> = servers
        .iter()
        .filter(|(id, _)| *id != VICTIM)
        .map(|(id, s)| (*id, s.as_ref() as &dyn ChunkStore))
        .collect();
    let scrub_ctx = ScrubContext {
        meta: &meta,
        fleet: &scrub_fleet,
    };
    reconcile_step(&zone, &leader, None, Some(&scrub_ctx), None, None, 100)
        .await
        .expect("scrub pass over the healthy survivors");

    // RECONSTRUCT with the victim KEPT IN the fleet view: the real `EIO` from the faulted
    // device drives loss classification through the production read in `assess`. The
    // victim's domain (A) is heavily utilized so the rebuilt fragment moves to the free,
    // healthy domain D (the spare) — driving the chunk back to full redundancy OFF the
    // fault, not side-stepping it by pre-excluding the victim.
    let mut recon_topo = Topology::default();
    recon_topo
        .register(0, "A")
        .register(1, "B")
        .register(2, "C")
        .register(3, "D")
        .set_utilization(VICTIM, 100);
    let recon_fleet: Vec<(DServerId, &dyn ChunkStore)> = servers
        .iter()
        .map(|(id, s)| (*id, s.as_ref() as &dyn ChunkStore))
        .collect();
    let recon_ctx = ReconstructionContext {
        meta: &meta,
        fleet: &recon_fleet,
        topology: &recon_topo,
    };
    let repaired = reconcile_step(&zone, &leader, None, None, Some(&recon_ctx), None, 200)
        .await
        .expect("reconstruction reads around the faulted fragment and rebuilds it");
    assert_eq!(
        repaired,
        Reconciled::Changed,
        "the faulted chunk was reconstructed"
    );

    // The obligation is drained, exactly one version-conditional commit landed, and the
    // chunk no longer references the faulted victim server.
    assert!(
        repair::queued_repairs(&meta).await.unwrap().is_empty(),
        "the repair obligation is drained by the reconstruction commit"
    );
    let record = read_inode(&meta).await;
    assert_eq!(record.version, 2, "exactly one version-conditional commit");
    assert!(
        !record.chunk_map[0].placement.contains(&VICTIM),
        "the faulted server no longer holds a referenced fragment"
    );

    // FULL REDUNDANCY: every placed fragment is present, verifies its checksum, and the N
    // fragments occupy N distinct failure domains — read from the healthy stores.
    let placement = &record.chunk_map[0].placement;
    assert_eq!(placement.len(), N, "n fragments placed");
    let mut domains = std::collections::HashSet::new();
    let mut full_redundancy = true;
    for (index, &server) in placement.iter().enumerate() {
        let store = servers.iter().find(|(id, _)| *id == server).map(|(_, s)| s);
        let intact = match store {
            Some(store) => match store
                .get_fragment(FragmentId {
                    chunk: CHUNK,
                    index: index as u16,
                })
                .await
            {
                Ok(Some(bytes)) => repair::fragment_intact(&bytes, CHUNK),
                _ => false,
            },
            None => false,
        };
        full_redundancy &= intact;
        domains.insert(["A", "B", "C", "D"][server as usize]);
    }
    assert!(
        full_redundancy,
        "every placed fragment verifies after repair"
    );
    assert_eq!(
        domains.len(),
        N,
        "n fragments on n distinct failure domains"
    );

    // The object still reads byte-identical after repair, off the now-full-redundancy set.
    assert_eq!(
        read_or_count_error(&meta, &fleet, &mut read_errors).await,
        Some(data),
        "object reads byte-identical after repair (full redundancy, atomic flip)"
    );

    assert_eq!(
        read_errors, 0,
        "no object read errored during the repair window"
    );

    // Report the campaign outcome for `xtask disk-faults`'s verdict.
    write_report(1, u64::from(full_redundancy), read_errors);
}
