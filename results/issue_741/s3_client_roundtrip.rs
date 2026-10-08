//! Issue #741: the `wyrd-validate` S3 client layer, driven end to end against a real Wyrd S3
//! gateway served in-process over loopback (proposal 0017 §2, `client.rs`).
//!
//! The gateway is composed directly — `Gateway::new(RedbMetadataStore, FsChunkStore,
//! MemCoordination)` behind `S3Gateway::serve` on a loopback listener, the pattern of
//! `crates/server/tests/s3_http_wire.rs:56-92` — with a **256 KiB** chunk size, so every object
//! here spans several chunks without the millions of chunks an 8-byte size would make of a
//! tens-of-MiB payload. The client under test is the production [`S3Client`]: the real
//! `aws-sdk-s3` stack, real SigV4, real HTTP/1.1 over TCP. Nothing is mocked.
//!
//! What is asserted:
//!
//! 1. **Round trip** — PUT → GET → DELETE, the GET byte-identical, and a GET after the DELETE
//!    reports the typed not-found error (404, `NoSuchKey`). The client is built from a resolved
//!    configuration, as the binary builds it.
//! 2. **Typed errors** — an error response surfaces as a value carrying its HTTP status, its S3
//!    `<Code>` and the `x-amz-request-id` the gateway stamped, asserted field by field. The
//!    request id is compared with the header a relay saw the gateway put on the wire, not
//!    merely checked for shape.
//! 3. **Streaming, in both directions**, each with an oracle that fails when the client
//!    aggregates the body. Both sit on a loopback **interposer**: a TCP relay between the client
//!    and the gateway that counts bytes per direction and can hold the response tail back. The
//!    oracle is *ordering at the wire*, not a size or a buffer count:
//!    * **PUT** — the payload comes from a generator that is never materialised. When the
//!      generator is asked for its FINAL piece, the relay must already have forwarded more than
//!      [`PUT_FORWARDED_FLOOR`] bytes of the request to the gateway. A client that collects the
//!      body before sending has forwarded nothing but, at most, a request head by then. The
//!      payload ([`STREAM_PAYLOAD`], 32 MiB) is far larger than every socket and SDK buffer
//!      between the generator and the relay put together, so a streaming client cannot hide a
//!      zero behind buffering.
//!    * **GET** — the relay forwards the first [`GET_PREFIX`] bytes of the response and then
//!      WITHHOLDS the rest. The client must hand the test a body piece while the tail is held. A
//!      client that collects the body before yielding has nothing to hand over, and the bounded
//!      wait fails with a message instead of hanging. Only then is the tail released, and the
//!      rest is read and compared with the generator, byte for byte.
//!
//! And the failure paths the client promises to name rather than swallow: responses the Wyrd
//! gateway does not produce but a validator must still report as what they are (an empty 404,
//! an XML error body with no `<Code>`, an HTML error page, an `<Error>` document cut off or
//! followed by junk inside a correctly sized body, a success the SDK cannot read, an error
//! body or object body cut short, a body whose framing disagrees with its declared length, a
//! PUT acknowledged or refused before its body was sent — whose source must then be let go,
//! not held or drained) come from a scripted loopback endpoint; the three deadlines are each
//! made to expire; and PUT body sources that end short, run long or fail are refused before
//! the gateway stores anything.
//!
//! Every helper task a fixture spawns is owned by it and aborted when it drops.
//!
//! RED before #741: `wyrd_validate::S3Client` does not exist, so this file does not compile.
//! The two streaming oracles are additionally shown red by mutation (buffer the PUT body;
//! collect the GET body) — recorded in the issue's build notes.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use futures_util::stream;
use futures_util::StreamExt as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio::task::{JoinHandle, JoinSet};
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_coordination_mem::MemCoordination;
use wyrd_gateway_s3::sigv4::Credentials as GatewayCredentials;
use wyrd_gateway_s3::{S3Config, S3Gateway};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_server::Gateway;
use wyrd_validate::error::REQUEST_ID_HEADER;
use wyrd_validate::{
    ClientOptions, Credentials, ErrorCode, Phase, ResolvedConfig, S3Client, S3Error, ServiceError,
};

const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const REGION: &str = "us-east-1";
const BUCKET: &str = "wyrd-validate";

/// The gateway's chunk size: multi-chunk objects at modest sizes, a manageable chunk count at
/// tens of MiB.
const CHUNK_SIZE: usize = 256 * 1024;
/// The streaming legs' payload.
const STREAM_PAYLOAD: u64 = 32 * 1024 * 1024;
/// The size of each piece the PUT generator yields.
const PIECE: usize = 64 * 1024;
/// The PUT oracle's floor: comfortably more than any request head, and comfortably less than
/// what a streaming client has put on the wire by the time a 32 MiB generator is on its last
/// piece.
const PUT_FORWARDED_FLOOR: u64 = 1024 * 1024;
/// How much of the GET response the relay forwards before it holds the tail.
const GET_PREFIX: u64 = 1024 * 1024;
/// The bound on any wait the GET leg makes while the tail is held.
const HOLD_WAIT: Duration = Duration::from_secs(15);
/// The deadline the timeout tests make expire.
const SHORT: Duration = Duration::from_millis(300);
/// The bound on any wait a failure-path test makes, so a client that hangs fails with a
/// message instead of hanging the suite.
const TEST_WAIT: Duration = Duration::from_secs(20);

// ---- task ownership ----

/// A spawned task its fixture owns: aborted when the guard drops.
struct Owned(JoinHandle<()>);

impl Drop for Owned {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Reap finished tasks so a long-lived set does not grow.
fn reap(set: &mut JoinSet<()>) {
    while set.try_join_next().is_some() {}
}

// ---- fixture: a real Wyrd S3 gateway, in-process, over loopback ----

/// A Wyrd S3 gateway on an ephemeral loopback port. Declare it FIRST in a test, so every
/// client and relay (declared later, dropped earlier) has closed its sockets before the gateway
/// goes.
struct GatewayFixture {
    addr: SocketAddr,
    /// The serve task: the accept loop and its listener. Declared before `_dir`, so it is
    /// aborted before the chunk store's directory is removed. Each accepted connection is
    /// axum's own task inside `S3Gateway::serve`, which ends when its socket closes.
    _server: Owned,
    _dir: tempfile::TempDir,
}

async fn start_gateway() -> GatewayFixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let gateway = Arc::new(
        Gateway::new(
            RedbMetadataStore::in_memory().expect("redb"),
            FsChunkStore::open(dir.path()).expect("fs store"),
            MemCoordination::new(),
        )
        .with_chunk_size(CHUNK_SIZE),
    );
    let config = S3Config::new(vec![GatewayCredentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    }]);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = S3Gateway::new(gateway, config);
    let task = tokio::spawn(async move {
        server.serve(listener).await.expect("serve");
    });
    GatewayFixture {
        addr,
        _server: Owned(task),
        _dir: dir,
    }
}

/// An environment lookup holding one AWS credential pair.
fn aws_env(id: &str, secret: &str) -> impl Fn(&str) -> Option<OsString> {
    let (id, secret) = (id.to_string(), secret.to_string());
    move |name: &str| match name {
        "AWS_ACCESS_KEY_ID" => Some(OsString::from(&id)),
        "AWS_SECRET_ACCESS_KEY" => Some(OsString::from(&secret)),
        _ => None,
    }
}

/// Credentials resolved the way the binary resolves them (the AWS pair).
fn credentials(id: &str, secret: &str) -> Credentials {
    wyrd_validate::resolve(&aws_env(id, secret)).expect("credentials resolve")
}

/// The configuration the binary would resolve for `endpoint`.
fn config_for(endpoint: &str) -> ResolvedConfig {
    let argv: Vec<String> = [
        ("--endpoint", endpoint),
        ("--region", REGION),
        ("--bucket", BUCKET),
        ("--scenario", "smoke"),
        ("--duration", "1m"),
        ("--workers", "1"),
        ("--seed", "741"),
        ("--out", "out"),
        ("--run-id", "issue-741"),
        ("--driver-placement", "loopback"),
    ]
    .into_iter()
    .flat_map(|(flag, value)| [flag.to_string(), value.to_string()])
    .collect();
    wyrd_validate::resolve_config(&argv, &aws_env(ACCESS_KEY, SECRET_KEY))
        .expect("configuration resolves")
}

fn client_with(addr: SocketAddr, options: ClientOptions) -> S3Client {
    S3Client::new(
        &format!("http://{addr}"),
        REGION,
        &credentials(ACCESS_KEY, SECRET_KEY),
        options,
    )
}

fn client_for(addr: SocketAddr) -> S3Client {
    client_with(addr, ClientOptions::default())
}

/// Bound a failure-path call, so a client that hangs fails with `what` instead.
async fn within<T>(what: &str, call: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(TEST_WAIT, call)
        .await
        .unwrap_or_else(|_| panic!("{what}: no result within {TEST_WAIT:?}"))
}

// ---- payload: a deterministic generator, never materialised ----

/// Byte `i` of the payload.
fn byte_at(i: u64) -> u8 {
    (i.wrapping_mul(2_654_435_761) >> 13) as u8
}

/// The payload bytes `[start, start + len)`.
fn piece(start: u64, len: usize) -> Bytes {
    (0..len as u64).map(|i| byte_at(start + i)).collect()
}

/// Assert `chunk` is the payload starting at `offset`; return the offset after it.
fn check_against_payload(offset: u64, chunk: &[u8]) -> u64 {
    for (i, byte) in chunk.iter().enumerate() {
        let at = offset + i as u64;
        assert_eq!(
            *byte,
            byte_at(at),
            "GET body differs from what was PUT at byte {at}"
        );
    }
    offset + chunk.len() as u64
}

/// The pieces a PUT body source yields, in order.
type Pieces = Vec<Result<Bytes, std::io::Error>>;

/// A body source of the given pieces, as the caller of `put_object` hands it over.
fn source(
    pieces: Pieces,
) -> impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static {
    stream::iter(pieces)
}

/// `source`, watched: the receiver resolves once the source has been dropped (the sender it
/// holds goes with it), which is how a test sees that a PUT let go of its body.
fn watched<S>(
    source: S,
) -> (
    impl futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static,
    tokio::sync::oneshot::Receiver<()>,
)
where
    S: futures_util::Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static,
{
    let (held, released) = tokio::sync::oneshot::channel::<()>();
    let source = source.map(move |item| {
        let _held = &held;
        item
    });
    (source, released)
}

// ---- the interposer: a loopback TCP relay that counts, records heads, can hold the tail ----

/// What the relay has observed, shared with the test.
struct Relay {
    addr: SocketAddr,
    /// Request bytes forwarded client → gateway.
    to_gateway: Arc<AtomicU64>,
    /// Response bytes forwarded gateway → client.
    to_client: Arc<AtomicU64>,
    /// The head of the first response forwarded on each connection, recorded before the
    /// client is handed any of it.
    heads: Arc<Mutex<Vec<String>>>,
    /// `false` while the response tail past the hold point is withheld.
    release: watch::Sender<bool>,
    /// The accept loop. It holds every pump in a `JoinSet`, so aborting it aborts them all.
    _task: Owned,
}

impl Relay {
    /// The `name` header of the one response this relay forwarded: what the gateway put on the
    /// wire.
    fn response_header(&self, name: &str) -> Option<String> {
        let heads = self.heads.lock().expect("relay heads");
        assert_eq!(
            heads.len(),
            1,
            "the relay saw exactly one connection's response"
        );
        header_value(&heads[0], name)
    }
}

/// Start a relay in front of `upstream`. With `hold_after = Some(n)`, each connection forwards
/// the first `n` response bytes and then withholds the rest until [`Relay::release`] is set.
async fn start_relay(upstream: SocketAddr, hold_after: Option<u64>) -> Relay {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind relay");
    let addr = listener.local_addr().expect("relay addr");
    let to_gateway = Arc::new(AtomicU64::new(0));
    let to_client = Arc::new(AtomicU64::new(0));
    let heads = Arc::new(Mutex::new(Vec::new()));
    let (release, released) = watch::channel(hold_after.is_none());
    let (up, down, seen) = (
        Arc::clone(&to_gateway),
        Arc::clone(&to_client),
        Arc::clone(&heads),
    );
    let task = tokio::spawn(async move {
        let mut pumps = JoinSet::new();
        while let Ok((client, _)) = listener.accept().await {
            reap(&mut pumps);
            let gateway = TcpStream::connect(upstream).await.expect("dial gateway");
            let (client_rx, client_tx) = client.into_split();
            let (gateway_rx, gateway_tx) = gateway.into_split();
            pumps.spawn(pump(client_rx, gateway_tx, Arc::clone(&up), None, None));
            pumps.spawn(pump(
                gateway_rx,
                client_tx,
                Arc::clone(&down),
                hold_after.map(|n| (n, released.clone())),
                Some(Arc::clone(&seen)),
            ));
        }
    });
    Relay {
        addr,
        to_gateway,
        to_client,
        heads,
        release,
        _task: Owned(task),
    }
}

/// A fresh relay in front of `gateway` and a client that reaches the gateway only through it,
/// so the relay sees exactly the one connection that client's single call opens.
async fn observed(gateway: SocketAddr) -> (Relay, S3Client) {
    let relay = start_relay(gateway, None).await;
    let client = S3Client::from_config(
        &config_for(&format!("http://{}", relay.addr)),
        ClientOptions::default(),
    );
    (relay, client)
}

/// Copy `from` → `to`, counting every byte forwarded. With a hold, forward exactly `limit`
/// bytes, then wait for the release before forwarding the rest. With `heads`, record the first
/// head that crosses (up to its blank line) before forwarding the bytes that complete it.
async fn pump(
    mut from: OwnedReadHalf,
    mut to: OwnedWriteHalf,
    count: Arc<AtomicU64>,
    mut hold: Option<(u64, watch::Receiver<bool>)>,
    heads: Option<Arc<Mutex<Vec<String>>>>,
) {
    let mut buf = vec![0u8; 16 * 1024];
    let mut forwarded = 0u64;
    let mut head = heads.map(|heads| (heads, Vec::new()));
    loop {
        let mut want = buf.len();
        if let Some((limit, released)) = &mut hold {
            if forwarded >= *limit {
                if released.wait_for(|go| *go).await.is_err() {
                    return;
                }
                hold = None;
            } else {
                want = want.min((*limit - forwarded) as usize);
            }
        }
        let n = match from.read(&mut buf[..want]).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if let Some((heads, pending)) = &mut head {
            pending.extend_from_slice(&buf[..n]);
            if let Some(end) = find(pending, b"\r\n\r\n") {
                let text = String::from_utf8_lossy(&pending[..end]).into_owned();
                heads.lock().expect("relay heads").push(text);
                head = None;
            }
        }
        if to.write_all(&buf[..n]).await.is_err() {
            break;
        }
        forwarded += n as u64;
        count.fetch_add(n as u64, Ordering::SeqCst);
    }
    let _ = to.shutdown().await;
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The value of header `name` in an HTTP head (case-insensitive name).
fn header_value(head: &str, name: &str) -> Option<String> {
    head.split("\r\n").skip(1).find_map(|line| {
        let (field, value) = line.split_once(':')?;
        field
            .trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

// ---- a scripted endpoint: fixed raw responses the Wyrd gateway never sends ----

/// The request id the scripted endpoint stamps.
const SCRIPTED_ID: &str = "scripted-request-741";

/// What the scripted endpoint does after writing its response.
#[derive(Clone, Copy)]
enum Then {
    /// Close the connection (which, for a response with no declared length, ends its body).
    Close,
    /// Keep the connection open and send nothing more.
    Stall,
    /// Read and discard whatever more the client sends, until it closes the connection.
    Drain,
}

/// A loopback endpoint answering every request with one fixed raw HTTP response.
struct Scripted {
    addr: SocketAddr,
    /// The accept loop. It holds every connection task in a `JoinSet`, so aborting it closes
    /// them all.
    _task: Owned,
}

async fn scripted(response: String, then: Then) -> Scripted {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind scripted");
    let addr = listener.local_addr().expect("scripted addr");
    let response = Bytes::from(response);
    let task = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        while let Ok((mut socket, _)) = listener.accept().await {
            reap(&mut connections);
            let response = response.clone();
            connections.spawn(async move {
                // GET and DELETE carry no body, so the head is the whole request; reading it
                // all means closing the socket sends FIN, not RST. A PUT is answered on its
                // head alone, before its body is read (with `Then::Stall` or `Then::Drain`, so
                // the socket is not closed over unread bytes).
                let mut seen = Vec::new();
                let mut buf = [0u8; 4096];
                while find(&seen, b"\r\n\r\n").is_none() {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => seen.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = socket.write_all(&response).await;
                match then {
                    Then::Close => {
                        let _ = socket.shutdown().await;
                    }
                    Then::Stall => std::future::pending::<()>().await,
                    Then::Drain => {
                        let mut sink = vec![0u8; 64 * 1024];
                        while matches!(socket.read(&mut sink).await, Ok(n) if n > 0) {}
                    }
                }
            });
        }
    });
    Scripted {
        addr,
        _task: Owned(task),
    }
}

/// A raw response: status line, the scripted request id, `headers` (each `name: value`), the
/// blank line, then `body` as given (no length is added).
fn raw(status: &str, headers: &[&str], body: &str) -> String {
    let mut text = format!("HTTP/1.1 {status}\r\n{REQUEST_ID_HEADER}: {SCRIPTED_ID}\r\n");
    for header in headers {
        text.push_str(header);
        text.push_str("\r\n");
    }
    text.push_str("\r\n");
    text.push_str(body);
    text
}

/// A response whose body length is declared.
fn sized(status: &str, headers: &[&str], body: &str) -> String {
    let length = format!("Content-Length: {}", body.len());
    let mut all = headers.to_vec();
    all.push(&length);
    raw(status, &all, body)
}

/// Read a GET body to its end or its first error.
async fn read_to_end(body: &mut wyrd_validate::ObjectBody) -> Result<Vec<u8>, S3Error> {
    let mut all = Vec::new();
    while let Some(chunk) = body.next_chunk().await? {
        all.extend_from_slice(&chunk);
    }
    Ok(all)
}

/// The request id and detail of an `Unreadable`, or a panic naming what came instead.
fn unreadable(err: S3Error, status: u16) -> String {
    match err {
        S3Error::Unreadable {
            status: got,
            request_id,
            detail,
        } => {
            assert_eq!(got, status, "the status the server sent");
            assert_eq!(request_id.as_deref(), Some(SCRIPTED_ID), "request id");
            assert!(!detail.is_empty(), "the SDK's diagnostic is kept");
            detail
        }
        other => panic!("expected an unreadable response with status {status}, got {other:?}"),
    }
}

/// Assert `err` is a body failure carrying `request_id`.
fn assert_body_error(err: &S3Error, request_id: Option<&str>, what: &str) {
    match err {
        S3Error::Body {
            request_id: got, ..
        } => assert_eq!(got.as_deref(), request_id, "{what}: request id"),
        other => panic!("{what}: expected a body failure, got {other:?}"),
    }
}

// ---- criterion 1: the round trip ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn put_get_delete_round_trips_and_a_later_get_is_typed_not_found() {
    let gateway = start_gateway().await;
    let config = config_for(&format!("http://{}", gateway.addr));
    let bucket = config.args.bucket.clone();
    let key = "round-trip/object";
    // Three and a bit chunks.
    let len = (3 * CHUNK_SIZE + 4321) as u64;

    // Each call goes through its own relay, so its receipt can be held against the request id
    // the gateway actually put on the wire.
    let (relay, client) = observed(gateway.addr).await;
    let put = client
        .put_object(&bucket, key, len, source(vec![Ok(piece(0, len as usize))]))
        .await
        .expect("PUT through the client layer");
    let stamped = relay.response_header(REQUEST_ID_HEADER);
    assert!(stamped.is_some(), "the gateway stamps every response");
    assert_eq!(put.request_id, stamped, "the PUT receipt's request id");
    assert!(put.e_tag.is_some(), "the PUT receipt carries the ETag");

    let (relay, client) = observed(gateway.addr).await;
    let mut body = client.get_object(&bucket, key).await.expect("GET");
    assert_eq!(body.content_length(), len, "declared Content-Length");
    assert_eq!(
        body.receipt().request_id,
        relay.response_header(REQUEST_ID_HEADER),
        "the GET receipt's request id"
    );
    assert_eq!(
        body.receipt().e_tag,
        put.e_tag,
        "the GET returns the PUT's ETag"
    );
    let mut offset = 0;
    while let Some(chunk) = body.next_chunk().await.expect("read GET body") {
        offset = check_against_payload(offset, &chunk);
    }
    assert_eq!(offset, len, "the GET body is exactly the object PUT");

    let (relay, client) = observed(gateway.addr).await;
    let deleted = client.delete_object(&bucket, key).await.expect("DELETE");
    assert_eq!(
        deleted.request_id,
        relay.response_header(REQUEST_ID_HEADER),
        "the DELETE receipt's request id"
    );

    let (relay, client) = observed(gateway.addr).await;
    let gone = client
        .get_object(&bucket, key)
        .await
        .expect_err("a GET after the DELETE must fail");
    let S3Error::Service(err) = gone else {
        panic!("a GET of a deleted object must be a typed S3 error response, got {gone:?}");
    };
    let stamped = relay.response_header(REQUEST_ID_HEADER);
    assert!(stamped.is_some(), "the gateway stamps error responses too");
    assert_eq!(err.status, 404, "not-found status");
    assert_eq!(
        err.code,
        ErrorCode::Code("NoSuchKey".to_string()),
        "S3 <Code>"
    );
    assert!(err.message.is_some(), "the gateway sends a <Message>");
    assert_eq!(
        err.request_id, stamped,
        "the request id the gateway stamped"
    );
}

// ---- criterion 2: an error response is a typed value ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_error_response_is_a_typed_value_with_status_code_and_request_id() {
    let gateway = start_gateway().await;
    let relay = start_relay(gateway.addr, None).await;
    // Signed, but with a key the gateway never issued: the gateway refuses it (fail-closed).
    let intruder = S3Client::new(
        &format!("http://{}", relay.addr),
        REGION,
        &credentials("AKIAINTRUDER0000000", "a-secret-the-gateway-never-issued"),
        ClientOptions::default(),
    );

    let refused = intruder
        .put_object(
            BUCKET,
            "forbidden",
            5,
            source(vec![Ok(Bytes::from_static(b"never"))]),
        )
        .await
        .expect_err("an unknown access key must be refused");
    let S3Error::Service(err) = refused else {
        panic!("a refusal must be a typed S3 error response, got {refused:?}");
    };
    let stamped = relay.response_header(REQUEST_ID_HEADER);
    assert!(stamped.is_some(), "the gateway stamps error responses");
    assert_eq!(err.status, 403, "status");
    assert_eq!(
        err.code,
        ErrorCode::Code("InvalidAccessKeyId".to_string()),
        "S3 <Code>"
    );
    assert!(err.message.is_some(), "the gateway sends a <Message>");
    assert_eq!(
        err.request_id, stamped,
        "the request id the gateway stamped"
    );

    // Not reaching a server at all is a different fact, never the same "unknown" value.
    let closed = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let dead = closed.local_addr().expect("addr");
    drop(closed);
    let unreachable = client_for(dead)
        .delete_object(BUCKET, "anything")
        .await
        .expect_err("nothing listens there");
    assert!(
        matches!(unreachable, S3Error::Transport { .. }),
        "an unreachable endpoint is a transport failure, got {unreachable:?}"
    );
    let not_a_url = S3Client::new(
        "not a url",
        REGION,
        &credentials(ACCESS_KEY, SECRET_KEY),
        ClientOptions::default(),
    )
    .delete_object(BUCKET, "anything")
    .await
    .expect_err("an endpoint that is not a URL is never reached");
    assert!(
        matches!(not_a_url, S3Error::Transport { .. }),
        "an endpoint that is not a URL is never reached, got {not_a_url:?}"
    );
}

// ---- criterion 2, continued: responses the gateway never sends, still named for what they are ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_empty_error_body_is_no_body_not_a_code_the_sdk_made_up() {
    // The SDK names an empty 404 `NotFound` itself; the server sent no code.
    let endpoint = scripted(sized("404 Not Found", &[], ""), Then::Close).await;
    let expected = S3Error::Service(ServiceError {
        status: 404,
        code: ErrorCode::NoBody,
        message: None,
        request_id: Some(SCRIPTED_ID.to_string()),
    });

    let got = within("GET", client_for(endpoint.addr).get_object(BUCKET, "k"))
        .await
        .expect_err("a 404 is an error");
    assert_eq!(got, expected, "GET of an empty 404");
    let got = within(
        "DELETE",
        client_for(endpoint.addr).delete_object(BUCKET, "k"),
    )
    .await
    .expect_err("a 404 is an error");
    assert_eq!(got, expected, "DELETE of an empty 404");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_xml_error_body_without_a_code_is_told_apart_from_one_that_is_not_xml() {
    // Well-formed XML, no <Code>: an S3 error response with no code.
    let endpoint = scripted(
        sized(
            "400 Bad Request",
            &["Content-Type: application/xml"],
            "<Error><Message>no code in here</Message></Error>",
        ),
        Then::Close,
    )
    .await;
    let expected = S3Error::Service(ServiceError {
        status: 400,
        code: ErrorCode::NoCodeInBody,
        message: Some("no code in here".to_string()),
        request_id: Some(SCRIPTED_ID.to_string()),
    });
    let got = within("GET", client_for(endpoint.addr).get_object(BUCKET, "k"))
        .await
        .expect_err("a 400 is an error");
    assert_eq!(got, expected, "GET");
    let got = within(
        "DELETE",
        client_for(endpoint.addr).delete_object(BUCKET, "k"),
    )
    .await
    .expect_err("a 400 is an error");
    assert_eq!(got, expected, "DELETE");

    // Error bodies that are not an S3 `<Error>` document: an HTML page that is not XML at all
    // (what a proxy in front of the endpoint sends); a well-formed XHTML page; and well-formed
    // XML with another root — the wrapped error shape other AWS protocols use, whose `<Code>`
    // sits one level down, where the SDK's S3 reader would find no code.
    for body in [
        "<html><body><h1>502 Bad Gateway</h1><hr><center>proxy</center></body></html>",
        "<html><body><h1>502 Bad Gateway</h1></body></html>",
        "<ErrorResponse><Error><Code>SlowDown</Code></Error></ErrorResponse>",
    ] {
        let endpoint = scripted(sized("502 Bad Gateway", &[], body), Then::Close).await;
        let got = within("GET", client_for(endpoint.addr).get_object(BUCKET, "k"))
            .await
            .expect_err("a 502 is an error");
        unreadable(got, 502);
        let got = within(
            "DELETE",
            client_for(endpoint.addr).delete_object(BUCKET, "k"),
        )
        .await
        .expect_err("a 502 is an error");
        unreadable(got, 502);
    }

    // `<Error>` documents with a `<Code>` that is not reported as if it had been read: one that
    // is not well-formed (an entity XML does not define), and one that is well-formed but that
    // the SDK fails partway through (markup inside `<Code>`).
    for body in [
        "<Error><Code>SlowDown</Code><Message>&bogus;</Message></Error>",
        "<Error><Code><b>Slow</b>Down</Code><Message>m</Message></Error>",
    ] {
        let endpoint = scripted(
            sized(
                "503 Service Unavailable",
                &["Content-Type: application/xml"],
                body,
            ),
            Then::Close,
        )
        .await;
        let got = within(
            "DELETE",
            client_for(endpoint.addr).delete_object(BUCKET, "k"),
        )
        .await
        .expect_err("a 503 is an error");
        unreadable(got, 503);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_error_document_cut_off_or_followed_by_junk_is_unreadable() {
    // Each body arrives whole at the HTTP layer (its Content-Length matches), but the XML in
    // it is not a whole document. The SDK's reader stops at the root's last child it reads,
    // so on its own it would report the `<Code>` (and the half `<Message>`) as received.
    for (status, body) in [
        ("503 Service Unavailable", "<Error><Code>SlowDown</Code>"),
        (
            "503 Service Unavailable",
            "<Error><Code>SlowDown</Code><Message>half",
        ),
        (
            "404 Not Found",
            "<Error><Code>NoSuchKey</Code></Error><junk",
        ),
    ] {
        let code: u16 = status[..3].parse().expect("status code");
        let endpoint = scripted(
            sized(status, &["Content-Type: application/xml"], body),
            Then::Close,
        )
        .await;
        let got = within("GET", client_for(endpoint.addr).get_object(BUCKET, "k"))
            .await
            .expect_err("an error status is an error");
        unreadable(got, code);
        let got = within(
            "DELETE",
            client_for(endpoint.addr).delete_object(BUCKET, "k"),
        )
        .await
        .expect_err("an error status is an error");
        unreadable(got, code);
    }

    // What may legitimately surround a whole document is accepted: an XML declaration before
    // it, whitespace and a comment after it.
    let endpoint = scripted(
        sized(
            "503 Service Unavailable",
            &["Content-Type: application/xml"],
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <Error><Code>SlowDown</Code><Message>m</Message></Error>\n<!-- end -->\n",
        ),
        Then::Close,
    )
    .await;
    let got = within(
        "DELETE",
        client_for(endpoint.addr).delete_object(BUCKET, "k"),
    )
    .await
    .expect_err("a 503 is an error");
    assert_eq!(
        got,
        S3Error::Service(ServiceError {
            status: 503,
            code: ErrorCode::Code("SlowDown".to_string()),
            message: Some("m".to_string()),
            request_id: Some(SCRIPTED_ID.to_string()),
        })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_success_the_sdk_cannot_read_is_unreadable_not_an_error_response() {
    // A 200 whose `Last-Modified` fails the HTTP-date grammar: the SDK cannot read it, and
    // that is neither an S3 error response nor a body-less one.
    let endpoint = scripted(
        sized("200 OK", &["Last-Modified: not-a-date"], "hello"),
        Then::Close,
    )
    .await;
    let got = within("GET", client_for(endpoint.addr).get_object(BUCKET, "k"))
        .await
        .expect_err("an unreadable success is a failure");
    let detail = unreadable(got, 200);
    assert!(
        detail.contains("LastModified"),
        "the SDK's diagnostic names the header it could not read: {detail}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_error_body_cut_short_is_unreadable() {
    // Declares 100 bytes, sends 10, closes.
    let endpoint = scripted(
        raw(
            "500 Internal Server Error",
            &["Content-Length: 100"],
            "<Error><Co",
        ),
        Then::Close,
    )
    .await;
    let got = within(
        "DELETE",
        client_for(endpoint.addr).delete_object(BUCKET, "k"),
    )
    .await
    .expect_err("a 500 is an error");
    unreadable(got, 500);
}

// ---- GET body integrity: never a shorter or longer object ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_get_body_with_no_declared_length_is_refused() {
    // No Content-Length, not chunked: the body ends when the connection closes, so a torn
    // body would look exactly like a whole short one.
    let endpoint = scripted(raw("200 OK", &["Connection: close"], "hel"), Then::Close).await;
    let got = within("GET", client_for(endpoint.addr).get_object(BUCKET, "k"))
        .await
        .expect_err("an unframed body cannot be trusted");
    assert_body_error(&got, Some(SCRIPTED_ID), "GET with no Content-Length");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_get_body_cut_short_is_a_body_error() {
    // Declares 10 bytes, sends 3, closes.
    let endpoint = scripted(raw("200 OK", &["Content-Length: 10"], "hel"), Then::Close).await;
    let mut body = within("GET", client_for(endpoint.addr).get_object(BUCKET, "k"))
        .await
        .expect("the response head is fine");
    assert_eq!(body.content_length(), 10);
    let got = within("read", read_to_end(&mut body))
        .await
        .expect_err("a torn body is a failure");
    assert_body_error(&got, Some(SCRIPTED_ID), "GET body cut short");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_get_body_whose_framing_disagrees_with_its_declared_length_is_a_body_error() {
    // Chunked framing that ends cleanly, beside a Content-Length it does not match (RFC 9112
    // §6.1 forbids sending both). The HTTP layer follows the chunked framing, so the
    // disagreement is caught by counting against the declared length — and never by handing
    // the caller a byte past it first.
    for (declared, what) in [
        (10u64, "shorter than declared"),
        (2, "longer than declared"),
    ] {
        let endpoint = scripted(
            raw(
                "200 OK",
                &[
                    "Transfer-Encoding: chunked",
                    &format!("Content-Length: {declared}"),
                ],
                "3\r\nhel\r\n0\r\n\r\n",
            ),
            Then::Close,
        )
        .await;
        let mut body = within("GET", client_for(endpoint.addr).get_object(BUCKET, "k"))
            .await
            .expect("the response head declares a length");
        assert_eq!(body.content_length(), declared, "{what}: declared length");
        let mut handed = 0u64;
        let got = loop {
            match within("read", body.next_chunk()).await {
                Ok(Some(chunk)) => {
                    handed += chunk.len() as u64;
                    assert!(
                        handed <= declared,
                        "{what}: {handed} bytes handed over, past the declared {declared}"
                    );
                }
                Ok(None) => panic!("{what}: the body was read as whole"),
                Err(e) => break e,
            }
        };
        assert_body_error(&got, Some(SCRIPTED_ID), what);
    }
}

// ---- deadlines: each one expires as a typed timeout naming its phase ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_endpoint_that_never_answers_expires_the_operation_deadline() {
    let endpoint = scripted(String::new(), Then::Stall).await;
    let options = ClientOptions {
        operation_timeout: SHORT,
        ..ClientOptions::default()
    };
    let got = within(
        "GET",
        client_with(endpoint.addr, options).get_object(BUCKET, "k"),
    )
    .await
    .expect_err("no answer is a failure");
    assert_eq!(
        got,
        S3Error::Timeout {
            phase: Phase::Operation,
            limit: SHORT
        }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_body_that_stops_arriving_expires_the_body_idle_deadline() {
    // Declares 10 bytes, sends 3, then neither sends nor closes.
    let endpoint = scripted(raw("200 OK", &["Content-Length: 10"], "hel"), Then::Stall).await;
    let options = ClientOptions {
        body_idle_timeout: SHORT,
        ..ClientOptions::default()
    };
    let mut body = within(
        "GET",
        client_with(endpoint.addr, options).get_object(BUCKET, "k"),
    )
    .await
    .expect("the response head arrives");
    let got = within("read", read_to_end(&mut body))
        .await
        .expect_err("a stalled body is a failure");
    assert_eq!(
        got,
        S3Error::Timeout {
            phase: Phase::BodyIdle,
            limit: SHORT
        }
    );
}

/// Linux only: a listener whose accept queue is full drops further SYNs (`tcp_conn_request`),
/// so a connect to it neither succeeds nor is refused until a deadline fires. Other kernels
/// may refuse instead.
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_connection_that_cannot_open_expires_the_connect_deadline() {
    let socket = tokio::net::TcpSocket::new_v4().expect("socket");
    socket
        .bind("127.0.0.1:0".parse().expect("loopback"))
        .expect("bind");
    let addr = socket.local_addr().expect("addr");
    // Never accepted from: fill its queue until a connect hangs.
    let _listener = socket.listen(0).expect("listen");
    let mut fillers = Vec::new();
    loop {
        assert!(fillers.len() < 64, "the accept queue never filled");
        match tokio::time::timeout(SHORT, TcpStream::connect(addr)).await {
            Ok(Ok(stream)) => fillers.push(stream),
            Ok(Err(e)) => panic!("connecting to a listener with a full queue: {e}"),
            Err(_) => break,
        }
    }

    // Only the connect deadline is short: the operation deadline, which also covers the
    // connect, must not be the one that fires.
    let options = ClientOptions {
        connect_timeout: SHORT,
        ..ClientOptions::default()
    };
    let got = within(
        "DELETE",
        client_with(addr, options).delete_object(BUCKET, "k"),
    )
    .await
    .expect_err("a connection that never opens is a failure");
    assert_eq!(
        got,
        S3Error::Timeout {
            phase: Phase::Connect,
            limit: SHORT
        }
    );
}

// ---- requests that cannot be built are never sent ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_request_that_cannot_be_built_is_named_as_such() {
    let gateway = start_gateway().await;
    let got = within("DELETE", client_for(gateway.addr).delete_object(BUCKET, ""))
        .await
        .expect_err("S3 has no object with an empty key");
    assert!(
        matches!(got, S3Error::Request { .. }),
        "an empty key is a request that could not be built, got {got:?}"
    );

    let got = within(
        "PUT",
        client_for(gateway.addr).put_object(BUCKET, "k", u64::MAX, source(Vec::new())),
    )
    .await
    .expect_err("no S3 Content-Length is that large");
    assert!(
        matches!(got, S3Error::Request { .. }),
        "an unrepresentable length is a request that could not be built, got {got:?}"
    );
}

// ---- PUT body sources that do not match their declared length, or fail ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_put_source_that_is_short_long_or_failing_is_refused_and_nothing_is_stored() {
    let gateway = start_gateway().await;
    let client = client_for(gateway.addr);
    let five = || Ok(Bytes::from_static(b"hello"));
    let cases: Vec<(&str, u64, Pieces)> = vec![
        ("ends short", 10, vec![five()]),
        (
            "runs long, in the piece after the declared length",
            5,
            vec![five(), Ok(Bytes::from_static(b"!!!"))],
        ),
        ("runs long, within one piece", 3, vec![five()]),
        (
            "fails",
            10,
            vec![five(), Err(std::io::Error::other("the source broke"))],
        ),
    ];
    for (what, declared, pieces) in cases {
        let key = format!("refused/{what}");
        let got = within(
            what,
            client.put_object(BUCKET, &key, declared, source(pieces)),
        )
        .await
        .expect_err("a source that does not match its declared length is refused");
        assert_body_error(&got, None, what);

        let stored = within("GET", client.get_object(BUCKET, &key))
            .await
            .expect_err("a refused PUT stores nothing");
        let S3Error::Service(err) = stored else {
            panic!("{what}: expected the key to be absent, got {stored:?}");
        };
        assert_eq!(
            (err.status, err.code),
            (404, ErrorCode::Code("NoSuchKey".to_string())),
            "{what}: nothing was stored"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_put_acknowledged_before_its_body_was_sent_is_not_a_receipt() {
    // The endpoint reads only the request head, answers at once, and reads nothing more: an
    // acknowledgement of bytes it was never sent. The Wyrd gateway reads the whole body before
    // it answers, so only a scripted endpoint can show this.
    let early = scripted(sized("200 OK", &["ETag: \"early\""], ""), Then::Stall).await;
    let hello = || Ok::<_, std::io::Error>(Bytes::from_static(b"hello"));

    // The source's tail never comes. Once the PUT has answered, the connection still sending
    // the body must let go of it rather than wait on it forever.
    let (body, released) = watched(stream::iter([hello()]).chain(stream::pending()));
    let got = within(
        "PUT, tail pending",
        client_for(early.addr).put_object(BUCKET, "k", 10, body),
    )
    .await
    .expect_err("an acknowledgement before the body is not a receipt");
    assert_body_error(&got, Some(SCRIPTED_ID), "tail pending");
    let _ = within("the source released after the PUT returned", released).await;

    // The source fails, but only after the acknowledgement arrived.
    let (fail, failed) = tokio::sync::oneshot::channel::<()>();
    let got = within(
        "PUT, fails later",
        client_for(early.addr).put_object(
            BUCKET,
            "k",
            10,
            stream::iter([hello()]).chain(stream::once(async move {
                let _ = failed.await;
                Err(std::io::Error::other("the source broke"))
            })),
        ),
    )
    .await
    .expect_err("an acknowledgement before the body is not a receipt");
    let _ = fail.send(());
    assert_body_error(&got, Some(SCRIPTED_ID), "fails later");

    // A large generator still producing when the acknowledgement arrives, to an endpoint that
    // goes on reading: once the PUT has answered, the generator is stopped and let go, not
    // drained to its end in the background.
    let draining = scripted(sized("200 OK", &["ETag: \"early\""], ""), Then::Drain).await;
    let declared = 2 * STREAM_PAYLOAD;
    let produced = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&produced);
    let (generator, released) = watched(stream::iter(0..declared / PIECE as u64).map(move |i| {
        counter.fetch_add(PIECE as u64, Ordering::SeqCst);
        Ok::<_, std::io::Error>(piece(i * PIECE as u64, PIECE))
    }));
    let got = within(
        "PUT, generator mid-flight",
        client_for(draining.addr).put_object(BUCKET, "k", declared, generator),
    )
    .await
    .expect_err("an acknowledgement before the body is not a receipt");
    assert_body_error(&got, Some(SCRIPTED_ID), "generator mid-flight");
    let _ = within("the generator released after the PUT returned", released).await;
    let produced = produced.load(Ordering::SeqCst);
    assert!(
        produced < declared,
        "the generator was drained after the PUT had answered ({produced} of {declared} bytes)"
    );

    // A server that REFUSES the PUT before reading its body is reported as its refusal.
    let refusing = scripted(
        sized(
            "403 Forbidden",
            &["Content-Type: application/xml"],
            "<Error><Code>AccessDenied</Code><Message>no</Message></Error>",
        ),
        Then::Stall,
    )
    .await;
    let (body, released) = watched(stream::iter([hello()]).chain(stream::pending()));
    let got = within(
        "PUT, refused early",
        client_for(refusing.addr).put_object(BUCKET, "k", 10, body),
    )
    .await
    .expect_err("a refusal is an error");
    assert_eq!(
        got,
        S3Error::Service(ServiceError {
            status: 403,
            code: ErrorCode::Code("AccessDenied".to_string()),
            message: Some("no".to_string()),
            request_id: Some(SCRIPTED_ID.to_string()),
        })
    );
    let _ = within("the source released after the refusal", released).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_empty_object_round_trips() {
    // A source with nothing to produce still ends cleanly, so its PUT is a receipt.
    let gateway = start_gateway().await;
    let client = client_for(gateway.addr);
    let put = within(
        "PUT",
        client.put_object(BUCKET, "empty", 0, source(Vec::new())),
    )
    .await
    .expect("an empty object is an object");
    assert!(put.e_tag.is_some(), "the PUT receipt carries the ETag");
    let mut body = within("GET", client.get_object(BUCKET, "empty"))
        .await
        .expect("GET");
    assert_eq!(body.content_length(), 0, "declared Content-Length");
    assert_eq!(
        within("read", read_to_end(&mut body)).await.expect("read"),
        Vec::<u8>::new()
    );
}

// ---- criterion 3: streaming in both directions ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_put_body_is_on_the_wire_before_the_generator_finishes() {
    let gateway = start_gateway().await;
    let relay = start_relay(gateway.addr, None).await;
    let client = client_for(relay.addr);

    let pieces = STREAM_PAYLOAD / PIECE as u64;
    // What the relay had forwarded when the generator was asked for its final piece.
    let at_final_piece = Arc::new(AtomicU64::new(u64::MAX));
    let (forwarded, seen) = (Arc::clone(&relay.to_gateway), Arc::clone(&at_final_piece));
    let generator = stream::iter(0..pieces).map(move |i| {
        if i == pieces - 1 {
            seen.store(forwarded.load(Ordering::SeqCst), Ordering::SeqCst);
        }
        Ok::<_, std::io::Error>(piece(i * PIECE as u64, PIECE))
    });

    client
        .put_object(BUCKET, "stream/put", STREAM_PAYLOAD, generator)
        .await
        .expect("streaming PUT");

    let before_last = at_final_piece.load(Ordering::SeqCst);
    assert_ne!(
        before_last,
        u64::MAX,
        "the generator reached its final piece"
    );
    assert!(
        before_last > PUT_FORWARDED_FLOOR,
        "when the generator was asked for its final piece the relay had forwarded only \
         {before_last} request bytes to the gateway (floor {PUT_FORWARDED_FLOOR}): the client \
         held the {STREAM_PAYLOAD}-byte body instead of streaming it"
    );
    assert!(
        relay.to_gateway.load(Ordering::SeqCst) >= STREAM_PAYLOAD,
        "the whole body crossed the relay"
    );

    // And the stored object is the generated one.
    let mut body = client_for(gateway.addr)
        .get_object(BUCKET, "stream/put")
        .await
        .expect("GET");
    let mut offset = 0;
    while let Some(chunk) = body.next_chunk().await.expect("read GET body") {
        offset = check_against_payload(offset, &chunk);
    }
    assert_eq!(
        offset, STREAM_PAYLOAD,
        "the stored object is the generated one"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_get_body_yields_while_the_response_tail_is_withheld() {
    let gateway = start_gateway().await;
    // Store the object directly (no relay), then read it back through a holding relay.
    let pieces = STREAM_PAYLOAD / PIECE as u64;
    client_for(gateway.addr)
        .put_object(
            BUCKET,
            "stream/get",
            STREAM_PAYLOAD,
            stream::iter(0..pieces)
                .map(|i| Ok::<_, std::io::Error>(piece(i * PIECE as u64, PIECE))),
        )
        .await
        .expect("PUT the object to read back");

    let relay = start_relay(gateway.addr, Some(GET_PREFIX)).await;
    let client = client_for(relay.addr);

    let mut body = tokio::time::timeout(HOLD_WAIT, client.get_object(BUCKET, "stream/get"))
        .await
        .unwrap_or_else(|_| {
            panic!(
                "no GET response within {HOLD_WAIT:?} while the relay withheld the response \
                 past its first {GET_PREFIX} bytes: the client is collecting the body before \
                 returning it"
            )
        })
        .expect("GET");
    assert_eq!(body.content_length(), STREAM_PAYLOAD);

    let first = tokio::time::timeout(HOLD_WAIT, body.next_chunk())
        .await
        .unwrap_or_else(|_| {
            panic!(
                "no body piece within {HOLD_WAIT:?} while the relay withheld the response past \
                 its first {GET_PREFIX} bytes: the client is collecting the body before \
                 yielding it"
            )
        })
        .expect("read the first body piece")
        .expect("the body is not empty");
    let held = relay.to_client.load(Ordering::SeqCst);
    assert!(
        held <= GET_PREFIX,
        "the relay was still holding the tail ({held} bytes forwarded)"
    );
    assert!(
        (first.len() as u64) < STREAM_PAYLOAD,
        "the first piece is part of the body, not all of it"
    );
    let mut offset = check_against_payload(0, &first);

    relay.release.send(true).expect("release the tail");
    while let Some(chunk) = body.next_chunk().await.expect("read GET body") {
        offset = check_against_payload(offset, &chunk);
    }
    assert_eq!(
        offset, STREAM_PAYLOAD,
        "the GET body is exactly the object PUT"
    );
}
