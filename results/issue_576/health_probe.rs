//! Issue #576 / proposal 0010 §"Scope boundary" item 7: the d-server role answers the
//! standard gRPC health-checking protocol (`grpc.health.v1.Health/Check`), with
//! readiness reflecting the backing `ChunkStore`'s own `health()` — fail-closed on
//! `Err(_)` — rather than a supervisor's only signal being process existence.
//!
//! Three success criteria, each its own test:
//! (a) `Check` reports SERVING while the store is healthy;
//! (b) `Check` reports NOT_SERVING within a bounded wait once the store goes
//!     `Health::Unhealthy` **or** once `health()` returns `Err` (fail-closed — both
//!     asserted);
//! (c) the health check still answers (not shed with `RESOURCE_EXHAUSTED`) while the
//!     data plane is saturated at its admission bound.
//!
//! Driven in-process over real loopback gRPC (the same shape as
//! `crates/chunkstore-grpc/tests/round_trip.rs`), against the real `DServer::serve`
//! composition — not a stand-in.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{mpsc, oneshot, Semaphore};
use tonic::server::NamedService;
use tonic_health::pb::health_check_response::ServingStatus as WireServingStatus;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::{HealthCheckRequest, HealthCheckResponse};
use wyrd_chunk_format::{encode, FragmentHeader};
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_chunkstore_grpc::{ChunkStoreServer, GrpcChunkStore};
use wyrd_coordination_mem::MemCoordination;
use wyrd_server::dserver::{AdmissionControl, DServer, DSERVER_GROUP};
use wyrd_traits::{ChunkId, ChunkStore, FragmentId, Health, Result};

fn fid(chunk: ChunkId, index: u16) -> FragmentId {
    FragmentId { chunk, index }
}

/// A valid v1 fragment whose header records `id`'s chunk and index.
fn fragment(id: FragmentId, payload: &[u8]) -> Bytes {
    let mut header = FragmentHeader::new_v1(id.chunk, payload.len() as u64);
    header.ec_fragment_index = id.index;
    Bytes::from(encode(&header, payload))
}

fn fs_store() -> (FsChunkStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = FsChunkStore::open(dir.path()).expect("open store");
    (store, dir)
}

/// What [`ControllableStore::health`] reports — set by the test at will, read by the
/// production readiness-refresh task inside `DServer::serve`.
#[derive(Clone, Copy, Debug)]
enum HealthMode {
    Healthy,
    Unhealthy,
    /// `health()` itself fails — the fail-closed case (Design, §"Mapping":
    /// "`Err(_)` from `health()` ⇒ NOT_SERVING").
    Erroring,
}

/// A `ChunkStore` whose `health()` the test controls at runtime, and whose
/// `get_fragment` can optionally **gate** (announce entry via `entered`, then park on
/// `gate`) so a data-plane request can be made to hold its admission slot for as long
/// as criterion (c) needs. `put`/`list`/`delete` always delegate straight through.
struct ControllableStore {
    inner: FsChunkStore,
    health: Arc<Mutex<HealthMode>>,
    entered: Option<mpsc::UnboundedSender<()>>,
    gate: Option<Arc<Semaphore>>,
}

#[async_trait]
impl ChunkStore for ControllableStore {
    async fn put_fragment(&self, id: FragmentId, fragment: Bytes) -> Result<()> {
        self.inner.put_fragment(id, fragment).await
    }

    async fn get_fragment(&self, id: FragmentId) -> Result<Option<Bytes>> {
        if let Some(entered) = &self.entered {
            let _ = entered.send(());
        }
        if let Some(gate) = &self.gate {
            let _permit = gate.acquire().await.expect("gate not closed");
        }
        self.inner.get_fragment(id).await
    }

    async fn list_fragments(&self) -> Result<Vec<FragmentId>> {
        self.inner.list_fragments().await
    }

    async fn delete_fragment(&self, id: FragmentId) -> Result<()> {
        self.inner.delete_fragment(id).await
    }

    async fn health(&self) -> Result<Health> {
        match *self.health.lock().unwrap() {
            HealthMode::Healthy => Ok(Health::Healthy),
            HealthMode::Unhealthy => Ok(Health::Unhealthy),
            HealthMode::Erroring => Err("store cannot report its own health".into()),
        }
    }
}

/// The `grpc.health.v1` service name the readiness status is keyed on — the
/// `ChunkStoreServer`'s own registered name (`DServer::serve` sets readiness there,
/// not on the empty-name overall/liveness service). `ChunkStoreServer<T>`'s
/// `NamedService::NAME` does not depend on `T`, so any instantiation gives the same
/// constant.
fn readiness_service_name() -> &'static str {
    <ChunkStoreServer<()> as NamedService>::NAME
}

/// Bind, register, and serve one D server over `store` with the given admission
/// posture and health-refresh cadence; return its data endpoint, its health endpoint,
/// a shutdown trigger, and the serve task.
async fn serve_controllable(
    store: ControllableStore,
    admission: AdmissionControl,
    health_refresh_interval: Duration,
) -> (
    String,
    String,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<()>>,
) {
    let coord = Arc::new(MemCoordination::new());
    let server = DServer::bind(store, "127.0.0.1:0".parse().unwrap())
        .await
        .expect("bind")
        .with_admission_control(admission)
        .with_health_refresh_interval(health_refresh_interval);
    let endpoint = server.endpoint().to_string();
    let health_endpoint = server.health_endpoint().to_string();
    let lease = server
        .register(&*coord, DSERVER_GROUP, Duration::from_secs(3600))
        .await
        .expect("register");
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(
        server.serve(coord, lease, Duration::from_secs(3600), async move {
            let _ = rx.await;
        }),
    );
    (endpoint, health_endpoint, tx, handle)
}

/// Dial `endpoint`'s `grpc.health.v1.Health` service.
async fn health_client(endpoint: &str) -> HealthClient<tonic::transport::Channel> {
    let channel = tonic::transport::Endpoint::try_from(endpoint.to_string())
        .expect("valid endpoint")
        .connect()
        .await
        .expect("connect to the health endpoint");
    HealthClient::new(channel)
}

/// Poll `Check` for `service` until it reports `expected`, bounded by `budget` —
/// the "within a bounded wait" criterion (b)/(a) name, without coupling the test to
/// the production refresh cadence's exact timing.
async fn wait_for_check(
    client: &mut HealthClient<tonic::transport::Channel>,
    service: &str,
    expected: WireServingStatus,
    budget: Duration,
) -> HealthCheckResponse {
    tokio::time::timeout(budget, async {
        loop {
            let resp = client
                .check(HealthCheckRequest {
                    service: service.to_string(),
                })
                .await
                .expect("Check RPC succeeds (the health service is registered)")
                .into_inner();
            if resp.status == expected as i32 {
                return resp;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("service {service:?} did not reach {expected:?} within {budget:?}"))
}

/// Success criterion (a): `Check` reports SERVING while the backing store's
/// `health()` is `Health::Healthy`.
///
/// Red pre-fix: `main` registers no `grpc.health.v1.Health` service (repro:
/// `crates/server/src/dserver.rs` `add_service` registers only `ChunkStoreServer`), so
/// a `Check` against it returns `UNIMPLEMENTED` — this assertion fails. Reverting the
/// production change also removes the `tonic-health` dependency and the
/// `with_health_refresh_interval` / `health_endpoint` methods this test calls, so the
/// red leg is a compile failure on the base toolchain, which the gate equally counts
/// as red (Falsifiability).
#[tokio::test]
async fn check_reports_serving_while_the_store_is_healthy() {
    let (store, _dir) = fs_store();
    let health = Arc::new(Mutex::new(HealthMode::Healthy));
    let controllable = ControllableStore {
        inner: store,
        health,
        entered: None,
        gate: None,
    };
    let (_endpoint, health_endpoint, shutdown, handle) = serve_controllable(
        controllable,
        AdmissionControl::default(),
        Duration::from_millis(20),
    )
    .await;

    let mut client = health_client(&health_endpoint).await;
    let resp = wait_for_check(
        &mut client,
        readiness_service_name(),
        WireServingStatus::Serving,
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(
        resp.status,
        WireServingStatus::Serving as i32,
        "a healthy store reads SERVING",
    );

    let _ = shutdown.send(());
    let _ = handle.await;
}

/// Success criterion (b): `Check` reports NOT_SERVING within a bounded wait once the
/// store reports `Health::Unhealthy` **or** once `health()` returns `Err` — both
/// asserted (fail-closed).
///
/// Red pre-fix: same as (a) — no health service is registered (behavioural red) or the
/// build does not compile (the dependency/method red the Falsifiability note accepts).
#[tokio::test]
async fn check_reports_not_serving_once_unhealthy_or_erroring() {
    let (store, _dir) = fs_store();
    let health = Arc::new(Mutex::new(HealthMode::Healthy));
    let controllable = ControllableStore {
        inner: store,
        health: Arc::clone(&health),
        entered: None,
        gate: None,
    };
    let (_endpoint, health_endpoint, shutdown, handle) = serve_controllable(
        controllable,
        AdmissionControl::default(),
        Duration::from_millis(20),
    )
    .await;
    let mut client = health_client(&health_endpoint).await;
    let name = readiness_service_name();

    // Baseline: converges to SERVING once the refresher's first read lands.
    wait_for_check(
        &mut client,
        name,
        WireServingStatus::Serving,
        Duration::from_secs(5),
    )
    .await;

    // Half (a): an `Health::Unhealthy` store flips readiness to NOT_SERVING.
    *health.lock().unwrap() = HealthMode::Unhealthy;
    wait_for_check(
        &mut client,
        name,
        WireServingStatus::NotServing,
        Duration::from_secs(5),
    )
    .await;

    // Recover, confirming the flip is not one-directional...
    *health.lock().unwrap() = HealthMode::Healthy;
    wait_for_check(
        &mut client,
        name,
        WireServingStatus::Serving,
        Duration::from_secs(5),
    )
    .await;

    // ...then half (b): `health()` itself erroring ALSO flips readiness to
    // NOT_SERVING — the fail-closed case (a store that cannot even report its health
    // must not read as ready).
    *health.lock().unwrap() = HealthMode::Erroring;
    wait_for_check(
        &mut client,
        name,
        WireServingStatus::NotServing,
        Duration::from_secs(5),
    )
    .await;

    let _ = shutdown.send(());
    let _ = handle.await;
}

/// Success criterion (c): the health check still answers — rather than being shed
/// with `RESOURCE_EXHAUSTED` — while the data plane is saturated at its admission
/// bound (`max_concurrent_requests` held by an in-flight data RPC).
///
/// Red pre-fix: on `main` there is no separate, unlayered path for a health probe at
/// all (no health service is registered); once the production change under test wires
/// one INSIDE the same admission-layered `Server::builder()` chain (the wrong
/// composition this criterion rules out), a held data-plane slot sheds the health
/// check exactly like a data request — RESOURCE_EXHAUSTED — failing this assertion.
/// Green requires the health service to be composed genuinely outside the admission
/// stack (Design, §"Overload policy").
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn health_check_answers_while_the_data_plane_is_saturated() {
    let (store, _dir) = fs_store();
    let (entered_tx, mut entered_rx) = mpsc::unbounded_channel();
    let gate = Arc::new(Semaphore::new(0)); // closed: the held request parks until released
    let health = Arc::new(Mutex::new(HealthMode::Healthy));
    let controllable = ControllableStore {
        inner: store,
        health,
        entered: Some(entered_tx),
        gate: Some(gate.clone()),
    };
    // Server-wide admission limit of 1 — the same shape as
    // `crates/server/tests/dserver.rs`'s `overload_across_connections_sheds_excess_with_a_retryable_status`,
    // adapted here to prove the HEALTH check is exempt from it, not that the data
    // plane sheds (that is already covered there).
    let admission = AdmissionControl {
        max_concurrent_requests: 1,
        ..AdmissionControl::default()
    };
    let (endpoint, health_endpoint, shutdown, handle) =
        serve_controllable(controllable, admission, Duration::from_millis(20)).await;

    let mut client = health_client(&health_endpoint).await;
    let name = readiness_service_name();
    // Converge to SERVING first, so the later assertion is unambiguous: a NOT_SERVING
    // read would also (trivially) not be RESOURCE_EXHAUSTED, which would not actually
    // prove the bypass.
    wait_for_check(
        &mut client,
        name,
        WireServingStatus::Serving,
        Duration::from_secs(5),
    )
    .await;

    // Saturate the one server-wide admission slot with a held data-plane request.
    let data_client = GrpcChunkStore::connect(endpoint)
        .await
        .expect("connect data client");
    let id = fid(0x5_1ED, 0);
    let frag = fragment(
        id,
        b"a fragment that never gets read while the slot is held",
    );
    data_client
        .put_fragment(id, frag)
        .await
        .expect("seed the fragment");
    let admitted = tokio::spawn(async move { data_client.get_fragment(id).await });
    entered_rx
        .recv()
        .await
        .expect("the data request is admitted and holds the one admission slot");

    // The health check must still answer — promptly, and with a real serving status,
    // not RESOURCE_EXHAUSTED — while that slot is held.
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        client.check(HealthCheckRequest {
            service: name.to_string(),
        }),
    )
    .await
    .expect(
        "the health check must be answered within the budget, not left to queue or be shed \
         behind the saturated data-plane admission bound",
    );
    let resp = outcome
        .expect("the health check succeeds (not shed with RESOURCE_EXHAUSTED)")
        .into_inner();
    assert_eq!(
        resp.status,
        WireServingStatus::Serving as i32,
        "the store is still healthy throughout — overload is not unreadiness (Design, \
         §\"Overload policy\")",
    );

    gate.add_permits(8);
    let _ = admitted.await;
    let _ = shutdown.send(());
    let _ = handle.await;
}
