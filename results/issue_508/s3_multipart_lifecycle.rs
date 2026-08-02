//! **S3 multipart upload — the staged-fragment lifecycle against the maintenance plane, and
//! segmented publication** (issue #508, legs C and D).
//!
//! Its own test binary, not a leg appended to `s3_multipart_upload.rs`: this file drives the
//! custodian loops, and the custodian suite's own rule (issue #214) is that a test driving
//! those loops must not share one process's `tracing` callsite cache with an unrelated suite.
//!
//! The property under test is the half of the invariant a verb-level test cannot see. A patch
//! can satisfy every wire-level multipart assertion while the maintenance plane quietly
//! deletes a live upload's bytes, or while a torn-down upload's bytes are retained forever —
//! **both** directions of *every durable byte is either protected by a record that names it
//! or evidenced for reclamation*.
//!
//! Why the GC half alone would not discriminate: GC does **not** reclaim everything
//! unreferenced. It reclaims only on explicit evidence — an `orphan:` record past the
//! reader-safe window, or an expired `pending:` lease — and absent both it *conservatively
//! retains*. So "removing a fragment from the reference set" never reclaims it, and a fix that
//! filtered inside `gc::reconcile` instead of inside the **reference set** would pass a
//! GC-only oracle while an operator restore pass strands a live upload's parts — and the next
//! GC pass then deletes them, because the restore just wrote the evidence GC was waiting for.
//! That is why the restore half below is not optional.
//!
//! **This file references only base-visible symbols**, so the C4-verify red leg fails by
//! assertion rather than by build error.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use aws_credential_types::Credentials as SdkCredentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::Client;
use bytes::Bytes;
use wyrd_coordination_mem::MemCoordination;
use wyrd_custodian::{
    reconcile_after_restore, reconcile_step, reconciliation_status, set_lifecycle, Custodian,
    DServerLifecycle, ExpiredPendingPolicy, FencedZone, GcContext, ReconciliationStatus,
    ReconstructionContext, ScrubContext, Topology,
};
use wyrd_gateway_s3::sigv4::{
    format_amz_date, sign_with_payload_hash, Credentials as GatewayCredentials,
};
use wyrd_gateway_s3::{S3Config, S3Gateway};
use wyrd_server::Gateway;
use wyrd_traits::{
    ChunkStore, CommitOutcome, DServerId, FragmentId, Health, MetadataStore, Result, WriteBatch,
};

const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const REGION: &str = "us-east-1";
const BUCKET: &str = "wyrd-bucket";
const MIN_PART: usize = 5 * 1024 * 1024;
const GRACE: u64 = 1_000;

// ---------------------------------------------------------------------------------------
// Harness — the same in-memory store and D-server doubles the custodian suite uses, so the
// custodian loops run over the *same* trait stores the gateway wrote through.
// ---------------------------------------------------------------------------------------

#[derive(Clone, Default)]
struct MemMeta {
    data: Arc<Mutex<BTreeMap<Vec<u8>, Bytes>>>,
}

impl MemMeta {
    fn raw_scan(&self, prefix: &str) -> Vec<(String, Bytes)> {
        let guard = self.data.lock().expect("meta lock");
        guard
            .range(prefix.as_bytes().to_vec()..)
            .take_while(|(k, _)| k.starts_with(prefix.as_bytes()))
            .map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), v.clone()))
            .collect()
    }

    fn raw_get(&self, key: &str) -> Option<Bytes> {
        self.data
            .lock()
            .expect("meta lock")
            .get(key.as_bytes())
            .cloned()
    }
}

#[async_trait::async_trait]
impl MetadataStore for MemMeta {
    async fn get(&self, key: &[u8]) -> Result<Option<Bytes>> {
        Ok(self.data.lock().expect("meta lock").get(key).cloned())
    }

    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Bytes)>> {
        let guard = self.data.lock().expect("meta lock");
        Ok(guard
            .range(prefix.to_vec()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    async fn commit(&self, batch: WriteBatch) -> Result<CommitOutcome> {
        let mut guard = self.data.lock().expect("meta lock");
        for pc in &batch.preconditions {
            let current = guard.get(&pc.key);
            let holds = match &pc.expected {
                Some(expected) => current == Some(expected),
                None => current.is_none(),
            };
            if !holds {
                return Ok(CommitOutcome::Conflict);
            }
        }
        for (key, value) in &batch.puts {
            guard.insert(key.clone(), value.clone());
        }
        for key in &batch.deletes {
            guard.remove(key);
        }
        Ok(CommitOutcome::Committed)
    }
}

/// One D server, shared between the gateway (which writes fragments through it) and the
/// custodian fleet (which sweeps it) — so the maintenance plane is looking at exactly the
/// bytes the upload staged, not a curated copy.
#[derive(Clone, Default)]
struct MemDServer {
    fragments: Arc<Mutex<HashMap<FragmentId, Bytes>>>,
}

impl MemDServer {
    fn count(&self) -> usize {
        self.fragments.lock().expect("frag lock").len()
    }

    fn ids(&self) -> Vec<FragmentId> {
        let guard = self.fragments.lock().expect("frag lock");
        let mut ids: Vec<FragmentId> = guard.keys().copied().collect();
        ids.sort_by_key(|f| (f.chunk, f.index));
        ids
    }
}

#[async_trait::async_trait]
impl ChunkStore for MemDServer {
    async fn put_fragment(&self, id: FragmentId, bytes: Bytes) -> Result<()> {
        self.fragments.lock().expect("frag lock").insert(id, bytes);
        Ok(())
    }
    async fn get_fragment(&self, id: FragmentId) -> Result<Option<Bytes>> {
        Ok(self.fragments.lock().expect("frag lock").get(&id).cloned())
    }
    async fn list_fragments(&self) -> Result<Vec<FragmentId>> {
        Ok(self.ids())
    }
    async fn delete_fragment(&self, id: FragmentId) -> Result<()> {
        self.fragments.lock().expect("frag lock").remove(&id);
        Ok(())
    }
    async fn health(&self) -> Result<Health> {
        Ok(Health::Healthy)
    }
}

/// A placement-aware fleet façade: the gateway writes through one handle, and the custodian
/// sweeps the same servers by id.
///
/// Sized so `dserver` **is** the index (RS(6,3) places 9 fragments at the identity placement
/// `0..8`), so the server the reference set names is the server actually holding the bytes —
/// the property the real placement-aware stores have and a modulo-routed façade would break,
/// making every assertion below about a mismatch rather than about the protocol.
#[derive(Clone, Default)]
struct MemFleet {
    servers: Vec<MemDServer>,
}

impl MemFleet {
    fn new(n: usize) -> Self {
        Self {
            servers: (0..n).map(|_| MemDServer::default()).collect(),
        }
    }

    fn total_fragments(&self) -> usize {
        self.servers.iter().map(|s| s.count()).sum()
    }
}

#[async_trait::async_trait]
impl ChunkStore for MemFleet {
    async fn put_fragment(&self, id: FragmentId, bytes: Bytes) -> Result<()> {
        let index = (id.index as usize) % self.servers.len();
        self.servers[index].put_fragment(id, bytes).await
    }
    async fn get_fragment(&self, id: FragmentId) -> Result<Option<Bytes>> {
        let index = (id.index as usize) % self.servers.len();
        self.servers[index].get_fragment(id).await
    }
    async fn list_fragments(&self) -> Result<Vec<FragmentId>> {
        let mut out = Vec::new();
        for server in &self.servers {
            out.extend(server.list_fragments().await?);
        }
        Ok(out)
    }
    async fn delete_fragment(&self, id: FragmentId) -> Result<()> {
        let index = (id.index as usize) % self.servers.len();
        self.servers[index].delete_fragment(id).await
    }
    async fn health(&self) -> Result<Health> {
        Ok(Health::Healthy)
    }
}

#[async_trait::async_trait]
impl wyrd_traits::PlacementChunkStore for MemFleet {
    async fn put_fragment_at(
        &self,
        dserver: DServerId,
        id: FragmentId,
        bytes: Bytes,
    ) -> Result<()> {
        let index = (dserver as usize) % self.servers.len();
        self.servers[index].put_fragment(id, bytes).await
    }

    async fn get_fragment_at(&self, dserver: DServerId, id: FragmentId) -> Result<Option<Bytes>> {
        let index = (dserver as usize) % self.servers.len();
        self.servers[index].get_fragment(id).await
    }
}

fn seed_bucket(meta: &MemMeta, name: &str) {
    meta.data.lock().expect("meta lock").insert(
        format!("bucket:{name}").into_bytes(),
        Bytes::from(format!("{{\"name\":\"{name}\",\"created_millis\":0}}")),
    );
}

/// Start the gateway over a fleet the test keeps a handle to.
async fn start(chunk_size: usize, servers: usize) -> (SocketAddr, MemMeta, MemFleet) {
    let meta = MemMeta::default();
    seed_bucket(&meta, BUCKET);
    let fleet = MemFleet::new(servers);
    let gateway = Arc::new(
        Gateway::new(meta.clone(), fleet.clone(), MemCoordination::new())
            .with_chunk_size(chunk_size),
    );
    let config = S3Config::new(vec![GatewayCredentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    }]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        S3Gateway::new(gateway, config)
            .serve(listener)
            .await
            .expect("serve");
    });
    (addr, meta, fleet)
}

fn sdk_client(addr: SocketAddr) -> Client {
    let http_client = aws_smithy_http_client::Builder::new().build_http();
    let config = aws_sdk_s3::Config::builder()
        .behavior_version_latest()
        .region(Region::new(REGION))
        .endpoint_url(format!("http://{addr}"))
        .credentials_provider(SdkCredentials::new(
            ACCESS_KEY, SECRET_KEY, None, None, "static",
        ))
        .http_client(http_client)
        .force_path_style(true)
        .retry_config(aws_sdk_s3::config::retry::RetryConfig::disabled())
        .stalled_stream_protection(aws_sdk_s3::config::StalledStreamProtectionConfig::disabled())
        .build();
    Client::from_conf(config)
}

async fn elect(coord: &MemCoordination) -> (FencedZone, Custodian) {
    let leader = Custodian::elect(coord, "zone-multipart")
        .await
        .expect("elect");
    let mut zone = FencedZone::new();
    zone.install(leader.leadership());
    (zone, leader)
}

/// Run one GC pass over the fleet at `now_millis`.
async fn run_gc(meta: &MemMeta, fleet: &MemFleet, now_millis: u64) {
    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord).await;
    let stores: Vec<(DServerId, &dyn ChunkStore)> = fleet
        .servers
        .iter()
        .enumerate()
        .map(|(i, s)| (i as DServerId, s as &dyn ChunkStore))
        .collect();
    let ctx = GcContext {
        meta,
        fleet: &stores,
        grace_window_millis: GRACE,
        // The deployed default. Reclamation of a multipart session's residue must be by
        // REFERENCE, not by expiry — the deployed GC defers expiry precisely because
        // "expired" cannot be trusted across producers' clocks.
        expired_pending: ExpiredPendingPolicy::Defer,
    };
    reconcile_step(&zone, &custodian, Some(&ctx), None, None, None, now_millis)
        .await
        .expect("reconcile_step");
}

/// Run one restore-reconciliation pass and return its report.
async fn run_restore(
    meta: &MemMeta,
    fleet: &MemFleet,
    now_millis: u64,
) -> wyrd_custodian::RestoreReport {
    let stores: Vec<(DServerId, &dyn ChunkStore)> = fleet
        .servers
        .iter()
        .enumerate()
        .map(|(i, s)| (i as DServerId, s as &dyn ChunkStore))
        .collect();
    let ctx = GcContext {
        meta,
        fleet: &stores,
        grace_window_millis: GRACE,
        expired_pending: ExpiredPendingPolicy::Defer,
    };
    reconcile_after_restore(&ctx, now_millis)
        .await
        .expect("reconcile_after_restore")
}

/// Run one **scrub** pass over the fleet — the maintenance consumer that verifies bytes and
/// enqueues a repair for anything that fails its checksum.
async fn run_scrub(meta: &MemMeta, fleet: &MemFleet) {
    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord).await;
    let stores: Vec<(DServerId, &dyn ChunkStore)> = fleet
        .servers
        .iter()
        .enumerate()
        .map(|(i, s)| (i as DServerId, s as &dyn ChunkStore))
        .collect();
    let ctx = ScrubContext {
        meta,
        fleet: &stores,
    };
    reconcile_step(&zone, &custodian, None, Some(&ctx), None, None, 0)
        .await
        .expect("reconcile_step (scrub)");
}

/// Run one **reconstruction** pass over the fleet, re-placing rebuilt fragments against a
/// topology whose domains are the fleet's own servers **minus `lost`** — the servers this
/// scenario models as gone, so a rebuilt shard must genuinely move rather than be rewritten
/// in place.
async fn run_reconstruction(meta: &MemMeta, fleet: &MemFleet, lost: &[usize], now_millis: u64) {
    let coord = MemCoordination::new();
    let (zone, custodian) = elect(&coord).await;
    let stores: Vec<(DServerId, &dyn ChunkStore)> = fleet
        .servers
        .iter()
        .enumerate()
        .map(|(i, s)| (i as DServerId, s as &dyn ChunkStore))
        .collect();
    let mut topology = Topology::default();
    for index in 0..fleet.servers.len() {
        if lost.contains(&index) {
            continue;
        }
        topology.register(index as DServerId, format!("d{index}"));
    }
    let ctx = ReconstructionContext {
        meta,
        fleet: &stores,
        topology: &topology,
        unreachable: &[],
    };
    reconcile_step(&zone, &custodian, None, None, Some(&ctx), None, now_millis)
        .await
        .expect("reconcile_step (reconstruction)");
}

/// The deployment wall clock, in epoch millis — the SAME lifecycle the gateway stamps its
/// `orphan:` marks from (proposal 0016's clock table: one clock per correctness lifecycle).
/// A maintenance pass driven from a logical zero would read every real mark as freshly
/// written and reclaim nothing, which would make the reclamation assertions vacuous.
fn wall_now() -> u64 {
    // wall-clock exempt: the gateway stamps `orphan:` marks from the DEPLOYMENT wall clock
    // (proposal 0016's clock table), so a maintenance pass judging those marks must read the
    // same source — mixing a logical clock into this one lifecycle is exactly the #557/#565
    // defect class, and here it would silently make every reclamation assertion vacuous.
    #[allow(clippy::disallowed_methods)]
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn body(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| seed.wrapping_add((i % 251) as u8))
        .collect()
}

/// Open a raw socket that streams half a part and then holds — so the session has genuinely
/// in-flight owned staging entries when the maintenance plane looks at it.
async fn hold_part_midstream(
    addr: SocketAddr,
    key: &str,
    upload_id: &str,
    part_number: u32,
    half: &[u8],
) -> tokio::net::TcpStream {
    use tokio::io::AsyncWriteExt;
    let host = addr.to_string();
    let path = format!("/{BUCKET}/{key}");
    let query = format!("partNumber={part_number}&uploadId={upload_id}");
    let creds = GatewayCredentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    };
    // wall-clock exempt: SigV4 dates must be the REAL clock the gateway validates freshness
    // against — a test-controlled instant would be rejected as an expired signature, which is
    // the auth lifecycle's own source, not this test's (AGENTS.md § Review rubric, ADR-0009).
    #[allow(clippy::disallowed_methods)]
    let amz_date = format_amz_date(SystemTime::now());
    let signed = sign_with_payload_hash(
        "PUT",
        &path,
        &query,
        &host,
        &amz_date,
        "UNSIGNED-PAYLOAD",
        &creds,
        REGION,
        "s3",
    );
    let mut head = format!("PUT {path}?{query} HTTP/1.1\r\nhost: {host}\r\n");
    head.push_str(&format!("authorization: {}\r\n", signed.authorization));
    head.push_str(&format!("x-amz-date: {}\r\n", signed.amz_date));
    head.push_str("x-amz-content-sha256: UNSIGNED-PAYLOAD\r\n");
    head.push_str(&format!("content-length: {}\r\n", half.len() * 2));
    head.push_str("connection: close\r\n\r\n");
    let mut socket = tokio::net::TcpStream::connect(addr).await.expect("connect");
    socket.write_all(head.as_bytes()).await.expect("head");
    socket.write_all(half).await.expect("half");
    socket.flush().await.expect("flush");
    socket
}

/// Poll until a raw prefix is (non-)empty, or a bounded deadline elapses.
async fn poll_until(meta: &MemMeta, prefix: &str, want_nonempty: bool) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if meta.raw_scan(prefix).is_empty() != want_nonempty {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    meta.raw_scan(prefix).is_empty() != want_nonempty
}

// ---------------------------------------------------------------------------------------
// Leg C — the staged-fragment lifecycle vs. the maintenance plane
// ---------------------------------------------------------------------------------------

/// **C(i).** A live upload is never harmed by maintenance: a GC pass reclaims **none** of its
/// fragments **and** a restore pass marks **none** stranded.
///
/// The restore half is the discriminating one. GC's conservative arm keeps an unevidenced
/// fragment either way, so a GC-only oracle cannot tell a correct fix from no fix at all. The
/// restore pass, by contrast, **writes** the `orphan:` evidence for anything it judges
/// unreferenced — so a fix that filtered inside `gc::reconcile` instead of inside the
/// reference set passes the GC half, strands the live upload's parts here, and the very next
/// GC pass deletes them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_live_upload_is_harmed_by_neither_gc_nor_a_restore_pass() {
    let (addr, meta, fleet) = start(64 * 1024, 9).await;
    let s3 = sdk_client(addr);
    let key = "live-upload.bin";

    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    s3.upload_part()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(body(3, MIN_PART)))
        .send()
        .await
        .expect("upload_part");

    // …and a SECOND part still streaming, so the fixture includes both sources of the staged
    // class: a committed `part:` record and an in-flight owned `sidx:` entry.
    let held_half = body(4, 128 * 1024);
    let _held = hold_part_midstream(addr, key, &upload_id, 2, &held_half).await;
    assert!(
        poll_until(&meta, &format!("sidx:{upload_id}:"), true).await,
        "the fixture must genuinely include an IN-FLIGHT owned staging entry — a curated \
         fixture that excluded it would prove nothing about the case that matters"
    );

    let staged_fragments = fleet.total_fragments();
    assert!(staged_fragments > 0, "the upload staged real fragments");

    // A GC pass well past the grace window reclaims NONE of them.
    run_gc(&meta, &fleet, wall_now() + 10 * GRACE).await;
    assert_eq!(
        fleet.total_fragments(),
        staged_fragments,
        "a GC pass must reclaim none of a live upload's fragments — they are protected by \
         records that name them (a committed `part:` record, an in-flight `sidx:` entry), not \
         by a timer"
    );

    // A restore pass marks NONE of them stranded — and says WHY.
    let report = run_restore(&meta, &fleet, wall_now() + 10 * GRACE).await;
    assert_eq!(
        report.stranded_marked, 0,
        "a restore pass must mark NOTHING stranded while an upload is live: every mark it \
         writes is the evidence GC is waiting for, so one mark here is a live upload's bytes \
         deleted on the next pass"
    );
    // …and the zero is NOT vacuous: nothing here is committed-referenced, so every fragment
    // the pass walked was one it would have marked but for the staged class. (Asserted
    // through a base-visible observable on purpose — naming a field this patch adds would
    // turn the red leg into a build error over a run that executed nothing.)
    assert!(
        meta.raw_scan("inode:")
            .iter()
            .all(|(_, v)| !String::from_utf8_lossy(v).contains("Committed")),
        "the fixture has NO committed object, so a committed-only reference set would have \
         marked every one of these fragments — which is what makes `stranded_marked == 0` \
         discriminating rather than trivially true"
    );
    assert!(
        meta.raw_scan("orphan:").is_empty(),
        "no `orphan:` evidence may exist for a live upload's bytes"
    );
}

/// **C(iii)(a).** Drain / desired-state reports the draining server `Pending`, never
/// `Satisfied`, while it still holds an **in-flight owned** fragment — the F6 wipe trace, the
/// one row whose violation is a *wipe*.
///
/// "Does not act" is not a passing answer for this consumer: doing nothing satisfies it
/// trivially. The assertion is the **positive** one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_drain_over_a_live_uploads_bytes_reports_pending_never_satisfied() {
    let (addr, meta, fleet) = start(64 * 1024, 1).await;
    let s3 = sdk_client(addr);
    let key = "draining.bin";

    // With one server the whole placement lands on D-server 0, so the status question is
    // unambiguous.
    assert_eq!(
        reconciliation_status(&meta, 0).await.expect("status"),
        ReconciliationStatus::NotRequested,
        "no drain has been requested yet"
    );

    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    let held_half = body(5, 128 * 1024);
    let _held = hold_part_midstream(addr, key, &upload_id, 1, &held_half).await;
    assert!(
        poll_until(&meta, &format!("sidx:{upload_id}:"), true).await,
        "the part must be genuinely in flight — no committed `part:` record yet"
    );
    assert!(
        meta.raw_scan(&format!("part:{upload_id}:")).is_empty(),
        "the sharpened case: the ONLY thing naming these bytes is the in-flight owned entry, \
         so a consumer that counted committed part records alone would answer Satisfied"
    );

    set_lifecycle(&meta, 0, DServerLifecycle::Draining)
        .await
        .expect("record the drain request");
    assert_eq!(
        reconciliation_status(&meta, 0).await.expect("status"),
        ReconciliationStatus::Pending,
        "a drain must report Pending while a still-streaming part's fragments sit on the \
         server: Satisfied here lets the operator wipe the disk, the part then commits, and a \
         Complete publishes a map naming wiped bytes"
    );
    assert!(
        fleet.total_fragments() > 0,
        "…and those fragments are genuinely on the server (the fixture includes the bytes the \
         status is about)"
    );

    // **And the drain fence's answer to a client is retryable, not "your upload is gone."**
    // Every staging batch carries `require_absent(desired:dserver:<S>)` for the servers its
    // placement names, so with a drain recorded on the only server every further `UploadPart`
    // is refused. What the client is TOLD decides whether the transfer survives the drain:
    // `404 NoSuchUpload` is fatal to aws-cli (it abandons the transfer, and the session it
    // abandons is still perfectly open), while `503 SlowDown` is the retryable backpressure
    // every SDK already backs off on. The session must also still be listed as open.
    let refused = s3
        .upload_part()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .part_number(2)
        .body(ByteStream::from(body(6, MIN_PART)))
        .send()
        .await
        .expect_err("a drained placement cannot stage a part");
    let status = refused
        .raw_response()
        .map(|response| response.status().as_u16());
    assert_eq!(
        status,
        Some(503),
        "a recorded drain is a deployment condition the same request passes once it clears — \
         it must answer 503 SlowDown, never 404 NoSuchUpload for a session that is still open"
    );
    let still_open = s3
        .list_parts()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await;
    assert!(
        still_open.is_ok(),
        "the refusal must leave the session usable: {still_open:?}"
    );
}

/// **C(ii).** Teardown reclaims, and only what it should.
///
/// Two arms, stated separately because they differ: after an **Abort** and its drain, a GC
/// pass past the grace window returns the fragment count to its pre-upload baseline; after a
/// **Complete**, the published object's fragments SURVIVE and only the unnamed ones go.
/// Asserting the pre-upload baseline after a Complete would assert data loss.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn teardown_reclaims_after_abort_and_preserves_the_published_object_after_complete() {
    let (addr, meta, fleet) = start(64 * 1024, 9).await;
    let s3 = sdk_client(addr);

    // --- ABORT arm: back to the pre-upload baseline --------------------------------------
    let baseline = fleet.total_fragments();
    let key = "aborted.bin";
    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    for number in 1..=2u32 {
        s3.upload_part()
            .bucket(BUCKET)
            .key(key)
            .upload_id(&upload_id)
            .part_number(number as i32)
            .body(ByteStream::from(body(number as u8, MIN_PART)))
            .send()
            .await
            .expect("upload_part");
    }
    assert!(
        fleet.total_fragments() > baseline,
        "the upload staged real fragments"
    );

    s3.abort_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("abort");

    // Poll the bounded drain to a deadline — never a fixed sleep.
    assert!(
        poll_until(&meta, &format!("part:{upload_id}:"), false).await,
        "the drain must dispose of the aborted session's part records"
    );
    assert!(
        poll_until(&meta, &format!("mpu:{upload_id}"), false).await,
        "…and, last of all, the session record itself"
    );
    for prefix in ["psum:", "sidx:", "slot:"] {
        assert!(
            meta.raw_scan(&format!("{prefix}{upload_id}")).is_empty(),
            "no `{prefix}` record may survive an aborted session's teardown"
        );
    }
    assert!(
        !meta.raw_scan("orphan:").is_empty(),
        "teardown must WRITE the reclamation evidence: GC reclaims only on explicit evidence \
         and otherwise retains forever, so removing a reference without an `orphan:` mark is a \
         silent, permanent leak — not a loud one"
    );

    run_gc(&meta, &fleet, wall_now() + 10 * GRACE).await;
    assert_eq!(
        fleet.total_fragments(),
        baseline,
        "after an Abort and its drain, a GC pass past the grace window returns the fragment \
         count to its PRE-UPLOAD baseline"
    );

    // --- COMPLETE arm: the published object's fragments SURVIVE ---------------------------
    let key = "published.bin";
    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    let p1 = body(7, MIN_PART);
    let p2 = body(8, 4096);
    let mut parts = Vec::new();
    for (number, bytes) in [(1i32, &p1), (2i32, &p2)] {
        let etag = s3
            .upload_part()
            .bucket(BUCKET)
            .key(key)
            .upload_id(&upload_id)
            .part_number(number)
            .body(ByteStream::from(bytes.clone()))
            .send()
            .await
            .expect("upload_part")
            .e_tag()
            .expect("etag")
            .to_string();
        parts.push(
            CompletedPart::builder()
                .part_number(number)
                .e_tag(etag)
                .build(),
        );
    }
    s3.complete_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build(),
        )
        .send()
        .await
        .expect("complete");

    assert!(
        poll_until(&meta, &format!("part:{upload_id}:"), false).await,
        "the published parts' records are disposed of by their retirement obligation"
    );
    let published = fleet.total_fragments();
    run_gc(&meta, &fleet, wall_now() + 100 * GRACE).await;
    assert_eq!(
        fleet.total_fragments(),
        published,
        "after a COMPLETE the published object's fragments must SURVIVE every GC pass — \
         asserting the pre-upload baseline here would be asserting data loss"
    );
    let got = s3
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("get");
    let bytes = got.body.collect().await.expect("collect").into_bytes();
    let mut expected = p1.clone();
    expected.extend_from_slice(&p2);
    assert_eq!(
        bytes.as_ref(),
        expected.as_slice(),
        "…and the object still reads back byte-identical after the maintenance pass"
    );
}

// ---------------------------------------------------------------------------------------
// Leg D — segmentation (decision 7)
// ---------------------------------------------------------------------------------------

/// **D.** With a small chunk size, an upload whose assembled map exceeds the flat ceiling
/// publishes a **segmented** root — `seg:<group-nonce>:<epoch>:<i>` records present, the inode
/// naming the group — and a GET returns the object byte-identical; a second, smaller object
/// still publishes a **flat** map.
///
/// The arithmetic (recorded in `build-notes.md`): at a 64 KiB chunk size, six 5 MiB parts
/// assemble to ⌈5 MiB / 64 KiB⌉ × 6 = 80 × 6 = **480 chunks**, which crosses the flat ceiling
/// `MAX_MAP_CHUNKS = ⌊(100 KB / 2) / 302 B⌋ = 165` while each part's own 80 chunks stays well
/// under `MAX_PART_CHUNKS` (the same 165) and every non-final part stays at the 5 MiB minimum.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_oversized_assembled_map_publishes_a_segmented_root_and_reads_back_intact() {
    let (addr, meta, _fleet) = start(64 * 1024, 9).await;
    let s3 = sdk_client(addr);
    let key = "segmented.bin";

    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();

    let mut expected = Vec::new();
    let mut parts = Vec::new();
    for number in 1..=6i32 {
        let bytes = body(number as u8, MIN_PART);
        expected.extend_from_slice(&bytes);
        let etag = s3
            .upload_part()
            .bucket(BUCKET)
            .key(key)
            .upload_id(&upload_id)
            .part_number(number)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .expect("upload_part")
            .e_tag()
            .expect("etag")
            .to_string();
        parts.push(
            CompletedPart::builder()
                .part_number(number)
                .e_tag(etag)
                .build(),
        );
    }
    s3.complete_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build(),
        )
        .send()
        .await
        .expect("complete a segmented object");

    let segments = meta.raw_scan("seg:");
    assert!(
        !segments.is_empty(),
        "a map above the flat ceiling must be published as SEGMENT records — a one-batch \
         publish of 480 chunk-refs would exceed the transaction envelope permanently"
    );
    let root = meta
        .raw_scan("inode:")
        .into_iter()
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
        .find(|v| v.contains("segments"))
        .expect("the inode root names its segment group");
    assert!(
        root.contains("nonce"),
        "the root names the segment GROUP — an independent nonce, not the upload id, because \
         segment records outlive the session's tombstone: {root}"
    );
    let group_key = segments[0].0.clone();
    assert!(
        !group_key.contains(&upload_id),
        "segment keys must NOT be keyed by the upload id ({group_key}): an id reused after its \
         tombstone expired would overwrite a live object's segments"
    );

    // …and the object reads back byte-identical through the segmented resolve.
    let got = s3
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("get");
    let bytes = got.body.collect().await.expect("collect").into_bytes();
    assert_eq!(
        bytes.as_ref(),
        expected.as_slice(),
        "a segmented object must read back byte-identical"
    );

    // A second, smaller object still publishes a FLAT map — segmentation is not the default.
    let small_key = "flat.bin";
    let small_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(small_key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    let small = body(9, 128 * 1024);
    let etag = s3
        .upload_part()
        .bucket(BUCKET)
        .key(small_key)
        .upload_id(&small_id)
        .part_number(1)
        .body(ByteStream::from(small.clone()))
        .send()
        .await
        .expect("upload_part")
        .e_tag()
        .expect("etag")
        .to_string();
    s3.complete_multipart_upload()
        .bucket(BUCKET)
        .key(small_key)
        .upload_id(&small_id)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(vec![CompletedPart::builder()
                    .part_number(1)
                    .e_tag(etag)
                    .build()]))
                .build(),
        )
        .send()
        .await
        .expect("complete a flat object");
    let flat_roots = meta
        .raw_scan("inode:")
        .into_iter()
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
        .filter(|v| !v.contains("segments"))
        .count();
    assert!(
        flat_roots > 0,
        "an object below the flat ceiling must still publish a FLAT inline map"
    );
    let got = s3
        .get_object()
        .bucket(BUCKET)
        .key(small_key)
        .send()
        .await
        .expect("get");
    let bytes = got.body.collect().await.expect("collect").into_bytes();
    assert_eq!(
        bytes.as_ref(),
        small.as_slice(),
        "the flat object round-trips"
    );
}

/// A `seggrp:` marker that a session never adopted is removed by the terminal delete, and one
/// that WAS adopted outlives its session — the two arms are mutually exclusive and exhaustive,
/// and the wrong choice either leaks one key per Create forever or deletes the only reuse
/// guard a live object's segments have.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_unadopted_segment_group_marker_is_reclaimed_with_its_session() {
    let (addr, meta, _fleet) = start(64 * 1024, 9).await;
    let s3 = sdk_client(addr);
    let key = "never-segments.bin";

    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    assert!(
        !meta.raw_scan("seggrp:").is_empty(),
        "Create reserves the segment-group nonce under `require_absent`, so reuse is \
         structurally impossible rather than merely improbable"
    );
    assert!(
        meta.raw_get(&format!("mpu:{upload_id}")).is_some(),
        "the session exists"
    );

    s3.abort_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("abort");
    assert!(
        poll_until(&meta, &format!("mpu:{upload_id}"), false).await,
        "the session is torn down"
    );
    assert!(
        meta.raw_scan("seggrp:").is_empty(),
        "a group the session NEVER adopted is removed by the terminal session delete — most \
         uploads write no `seg:` record at all, so a 'delete when the last segment goes' rule \
         alone would leak one key per Create, permanently"
    );
}

/// **C(iii)(b).** **Scrub** over a **corrupt staged fragment** enqueues a repair — the
/// positive, not "does not act".
///
/// "Walks it" is not a passing answer: a scrub that never fetches a staged fragment never
/// verifies it, so a byte that rotted during a staging window measured in hours is published
/// into the object silently. The assertion is therefore the *repair obligation*, which only
/// exists if the fragment was actually fetched, actually verified, and actually failed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scrub_verifies_staged_fragments_and_enqueues_a_repair_for_a_corrupt_one() {
    let (addr, meta, fleet) = start(64 * 1024, 9).await;
    let s3 = sdk_client(addr);
    let key = "scrub-staged.bin";

    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    s3.upload_part()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(body(3, MIN_PART)))
        .send()
        .await
        .expect("upload_part");

    // The part is COMMITTED (a `part:` record names its chunks) and the session is still
    // `Open`: exactly the staged population decision 2 makes scrub-visible.
    assert!(
        !meta.raw_scan(&format!("part:{upload_id}:")).is_empty(),
        "the part must be committed for its chunks to carry a verifiable EC scheme"
    );
    assert!(
        meta.raw_scan("repair:").is_empty(),
        "no repair obligation before the corruption"
    );

    // A clean pass first: nothing is enqueued for intact staged bytes, so the assertion below
    // cannot pass merely because scrub enqueues indiscriminately.
    run_scrub(&meta, &fleet).await;
    assert!(
        meta.raw_scan("repair:").is_empty(),
        "scrub must not enqueue a repair for INTACT staged fragments"
    );

    // Corrupt one staged fragment in place — a bit-flip in the stored bytes, the shape a rotten
    // sector produces.
    let target = fleet.servers[0]
        .ids()
        .first()
        .copied()
        .expect("the staged part placed a fragment on server 0");
    {
        let mut guard = fleet.servers[0].fragments.lock().expect("frag lock");
        let bytes = guard.get(&target).expect("fragment").clone();
        let mut corrupted = bytes.to_vec();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xff;
        guard.insert(target, Bytes::from(corrupted));
    }

    run_scrub(&meta, &fleet).await;
    let queued = meta.raw_scan("repair:");
    assert!(
        !queued.is_empty(),
        "scrub must ENQUEUE A REPAIR for a corrupt STAGED fragment — a pass that only walks \
         `referenced.placed` never fetches it, so staged redundancy decays untended and the \
         damage is published into the object"
    );
    assert!(
        queued
            .iter()
            .any(|(key, _)| key.contains(&format!("{:032x}", target.chunk))
                || key.contains(&target.chunk.to_string())),
        "the obligation must name the corrupt chunk: {queued:?}"
    );
}

/// **C(iii)(c).** **Reconstruction** of a lost staged fragment **updates that part record's
/// placement** under the fenced repoint CAS.
///
/// A committed-only implementation does nothing here and would pass a "does not silently
/// re-place" oracle, so the assertion is the positive: the `part:` record's placement moves and
/// the rebuilt fragment exists at its new position.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconstruction_repoints_a_lost_staged_fragment_under_the_session_fence() {
    // RS(6,3) over **ten** servers: losing one fragment leaves eight survivors (well above `k`)
    // AND leaves one spare failure domain for the rebuilt shard to move to — so the repoint is
    // observable as a *changed* placement rather than a same-server rewrite.
    let (addr, meta, fleet) = start(64 * 1024, 10).await;
    let s3 = sdk_client(addr);
    let key = "reconstruct-staged.bin";

    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    s3.upload_part()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(body(4, MIN_PART)))
        .send()
        .await
        .expect("upload_part");

    // The record key pads the part number, exactly as `ListParts`'s ordering needs.
    let part_key = format!("part:{upload_id}:00001");
    let before = meta.raw_get(&part_key).expect("the part record");
    let before_text = String::from_utf8_lossy(&before).into_owned();

    // Lose one staged fragment outright — the shape a wiped disk produces. Fragment index 8 is
    // placed on server 8 (the identity placement), and server 9 is the spare.
    let lost = fleet.servers[8]
        .ids()
        .first()
        .copied()
        .expect("the staged part placed a fragment on server 8");
    fleet.servers[8]
        .fragments
        .lock()
        .expect("frag lock")
        .remove(&lost);

    // Scrub notices the absence and enqueues the repair; reconstruction resolves it.
    run_scrub(&meta, &fleet).await;
    assert!(
        !meta.raw_scan("repair:").is_empty(),
        "scrub must enqueue the repair for a MISSING staged fragment"
    );
    // Server 8 is modelled as GONE (the disk that lost the fragment), so the rebuilt shard must
    // be re-placed on the spare domain and the part record must be repointed to name it.
    run_reconstruction(&meta, &fleet, &[8], wall_now()).await;

    let after = meta.raw_get(&part_key).expect("the part record survives");
    let after_text = String::from_utf8_lossy(&after).into_owned();
    assert_ne!(
        before_text, after_text,
        "reconstruction must REPOINT the staged part record's placement under the fenced CAS — \
         a committed-only implementation drains the obligation and leaves the staged chunk \
         permanently under-replicated"
    );
    let holders: Vec<usize> = fleet
        .servers
        .iter()
        .enumerate()
        .filter(|(_, server)| server.ids().contains(&lost))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(
        holders.len(),
        1,
        "the rebuilt fragment must exist at exactly one position: {holders:?}"
    );
    assert_ne!(
        holders[0], 8,
        "the rebuilt shard must be re-placed on a DIFFERENT server than the one it was lost \
         from, and the part record must name that server"
    );
    assert!(
        meta.raw_scan("repair:").is_empty(),
        "a resolved repair drains its obligation"
    );
    // The session is still usable: the repoint rode the session fence rather than replacing it.
    let listed = s3
        .list_parts()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("list_parts after a staged repoint");
    assert_eq!(
        listed.parts().len(),
        1,
        "the part is still staged and listable"
    );
}

/// A **published segmented** object is protected from the whole maintenance plane, and
/// deleting it retires its generation in ONE O(1) obligation.
///
/// This is the case a committed-map reader that ignores `seg:` records loses: once the
/// retirement drain has deleted the session's `part:` records, a segmented object's chunks are
/// in **neither** the committed nor the staged class, so a restore pass orphan-marks every one
/// of them and the next GC pass past the grace window deletes a live object's bytes. The
/// restore half is the discriminating one — GC's conservative arm retains an unevidenced
/// fragment either way.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_published_segmented_object_survives_maintenance_and_its_delete_retires_it() {
    let (addr, meta, fleet) = start(64 * 1024, 9).await;
    let s3 = sdk_client(addr);
    let key = "segmented-maintained.bin";

    let upload_id = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    let mut expected = Vec::new();
    let mut parts = Vec::new();
    for number in 1..=6i32 {
        let bytes = body(number as u8, MIN_PART);
        expected.extend_from_slice(&bytes);
        let etag = s3
            .upload_part()
            .bucket(BUCKET)
            .key(key)
            .upload_id(&upload_id)
            .part_number(number)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .expect("upload_part")
            .e_tag()
            .expect("etag")
            .to_string();
        parts.push(
            CompletedPart::builder()
                .part_number(number)
                .e_tag(etag)
                .build(),
        );
    }
    s3.complete_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build(),
        )
        .send()
        .await
        .expect("complete a segmented object");
    assert!(
        !meta.raw_scan("seg:").is_empty(),
        "the fixture must actually be segmented, or this proves nothing"
    );

    // Wait for the publication's `retire:records:{parts}` to drain: the part records are the
    // segmented object's LAST non-inode protection, so the discriminating window opens only
    // once they are gone.
    assert!(
        poll_until(&meta, &format!("part:{upload_id}:"), false).await,
        "the published parts' records must drain"
    );
    let fragments_before = fleet.total_fragments();

    // The restore half: nothing of a live, committed, SEGMENTED object may be marked stranded.
    let report = run_restore(&meta, &fleet, wall_now()).await;
    assert_eq!(
        report.stranded_marked, 0,
        "a restore pass must mark NOTHING stranded for a live segmented object — every one of \
         its fragments is referenced through its `seg:` records"
    );
    // …and a GC pass far past the grace window reclaims none of it.
    run_gc(&meta, &fleet, wall_now() + 10 * GRACE).await;
    assert_eq!(
        fleet.total_fragments(),
        fragments_before,
        "GC must reclaim none of a live segmented object's fragments"
    );
    let got = s3
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("get after maintenance");
    let bytes = got.body.collect().await.expect("collect").into_bytes();
    assert_eq!(
        bytes.as_ref(),
        expected.as_slice(),
        "the segmented object still reads back byte-identical after a full maintenance sweep"
    );

    // Deleting it installs ONE generation obligation — never an inline fan-out of ~480 chunks'
    // worth of orphan marks — and the drain then removes its `seg:` records and its bytes.
    s3.delete_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("delete the segmented object");
    // The delete answers from its own fenced commit and the generation obligation drains behind
    // it (bounded, off the request path), so the *terminal* condition is what a wire test can
    // observe without racing the drain. That the delete installs exactly ONE obligation and
    // **no** inline orphan fan-out is pinned deterministically where the batch itself is
    // visible: `crates/core/src/metadata.rs`'s
    // `deleting_a_segmented_generation_installs_one_obligation_and_no_inline_orphans`.
    //
    // Poll for the terminal condition rather than sleeping.
    assert!(
        poll_until(&meta, "orphan:", true).await,
        "the generation drain must WRITE the orphan evidence — GC conservatively retains an \
         unevidenced fragment forever, so a release without evidence is a silent leak"
    );
    assert!(
        poll_until(&meta, "seg:", false).await,
        "the retired generation's `seg:` records must be gone"
    );
    assert!(
        poll_until(&meta, "retire:bytes:g:", false).await,
        "and the obligation itself drains, rather than being retained for ever"
    );
    run_gc(&meta, &fleet, wall_now() + 10 * GRACE).await;
    assert_eq!(
        fleet.total_fragments(),
        0,
        "past the grace window every fragment of the deleted segmented object is reclaimed"
    );
}
