//! M2.8 definition of done (issue #155): the **static-endpoints gateway client
//! mode** — `wyrd put`/`get --endpoints …` — composes a [`Gateway`] over a
//! `FanoutChunkStore<GrpcChunkStore>` built from a configured endpoint list, and
//! round-trips an object byte-identically across a real, networked cluster.
//!
//! This is the in-process loopback proof of that composition (the C4-verify
//! criterion in the brief; the containerized `docker compose up` + `wyrd
//! put/get` flow is the supplementary manual / nightly tier). Like
//! `chunkstore-grpc/tests/round_trip.rs` and the Tier-2 test, it stands up real
//! gRPC D servers over loopback — real tonic transport, real HTTP/2 framing —
//! rather than an in-memory fake, then drives the *same* `connect_gateway`
//! composition the CLI's `--endpoints` path uses, so the test exercises the
//! shipping client mode, not a stand-in.

use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_chunkstore_grpc::{ChunkStoreServer, ChunkStoreService};
use wyrd_server::cli::connect_gateway;

/// Stand up one D-server service over a fresh `FsChunkStore`, bound to an
/// ephemeral loopback port, and return its dialable endpoint. The listener is
/// bound (accepting into the OS backlog) before we hand back the endpoint, so a
/// client can dial with no startup race. The temp dir and serve task are kept
/// alive for the test's duration.
async fn spawn_dserver() -> (String, tempfile::TempDir, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = FsChunkStore::open(dir.path()).expect("open store");
    let service = ChunkStoreService::new(store);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");

    let server = tokio::spawn(async move {
        Server::builder()
            .add_service(ChunkStoreServer::new(service))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .expect("serve");
    });

    (format!("http://{addr}"), dir, server)
}

/// A deterministic payload that spans several chunks at the test's chunk size,
/// so each chunk fans its own fragments out across the cluster.
fn payload(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(7))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gateway_put_get_byte_identical_across_grpc_cluster() {
    // A four-server cluster of real, networked gRPC D servers over loopback.
    let mut endpoints = Vec::new();
    let mut dirs = Vec::new();
    let mut servers = Vec::new();
    for _ in 0..4 {
        let (endpoint, dir, server) = spawn_dserver().await;
        endpoints.push(endpoint);
        dirs.push(dir);
        servers.push(server);
    }

    // The exact composition the CLI's `--endpoints` path builds: a `Gateway`
    // over a `FanoutChunkStore<GrpcChunkStore>` of the configured endpoints, with
    // metadata held locally under a data dir.
    let data_dir = tempfile::tempdir().expect("data dir");
    let gateway = connect_gateway(data_dir.path().to_str().unwrap(), &endpoints)
        .await
        .expect("compose the static-endpoints gateway client mode")
        // A small chunk size so the object spans several chunks under the default
        // rs(6,3): every chunk's 9 fragments fan out across the four D servers.
        .with_chunk_size(8 * 1024);

    // S3 PUT → GET across the networked cluster, byte-identical (the criterion).
    let key = "cluster/object";
    let data = payload(40 * 1024 + 777);
    gateway
        .put_object(key, &data)
        .await
        .expect("PUT fans fragments out over gRPC to the cluster");
    let got = gateway
        .get_object(key)
        .await
        .expect("GET reconstructs the object from gRPC fragments");
    assert_eq!(
        got.as_deref(),
        Some(&data[..]),
        "object read back over the networked gRPC cluster must be byte-identical",
    );

    // A missing key reports not-found, not an error, through the same path.
    assert!(
        gateway
            .get_object("cluster/absent")
            .await
            .expect("a miss is Ok(None), not a transport error")
            .is_none(),
        "an unknown key returns None",
    );

    for server in servers {
        server.abort();
    }
}
