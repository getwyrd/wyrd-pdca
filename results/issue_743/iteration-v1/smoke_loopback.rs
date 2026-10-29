//! Issue #743: the `smoke` scenario end to end, against a real Wyrd S3 gateway served
//! in-process on loopback (the composition `crates/validate/tests/s3_client_roundtrip.rs:318-350`
//! uses: redb + `MemCoordination` + a local `FsChunkStore`), with the gateway's
//! `S3Config::server_version` set, so it answers `Server: wyrd/<version>` (#736). No container,
//! no network beyond `127.0.0.1`, no mock: the production `run` drives the production client
//! into the real `S3Gateway`.
//!
//! Between the validator and the gateway sits a **relay**: a loopback TCP forwarder that records
//! every request's head (and a bulk delete's body), counts connections, can strip the `Server`
//! header from the answer, and can plant an object, server-side, just before the first request
//! that writes. It is the oracle the validator cannot see. What a run left behind is read
//! straight from the gateway (`Gateway::get_object`), not through the validator's client.
//!
//! * **(D)** A `0.0.0+git.<sha>` gateway: PASS. Every row of the shipped profile passed, every
//!   unsupported operation reached the wire and was answered as declared (`CopyObject`
//!   included), through `run` and through the real binary.
//! * **(E)** A test-local profile that declares `PutObject` unsupported: FAIL, naming
//!   `unexpected-support` for `PutObject`, with the key it wrote left in place.
//! * **(F)** The version source: the header used and named; `--server-version` winning over a
//!   header no profile covers; no header, or `wyrd/unknown`, without the flag: INCONCLUSIVE
//!   naming `--server-version`, with no request that writes sent.
//! * **(G)** Only its own keys: every write under the run prefix (or the run's scratch bucket);
//!   on PASS nothing it wrote is left, and an object planted under the prefix mid-run (never
//!   listed by a sweep) survives, byte for byte, so cleanup is by ownership, not by listing;
//!   an object planted before the run makes it refuse with the usage status, writing nothing.
//! * **(H)** Scale: `--duration 2s` repeats the sweep for at least 2 s and says how many sweeps;
//!   `--workers 2` is INCONCLUSIVE before any connection; `--duration 7x` is refused by name.
//!
//! RED before #743: the binary resolves its configuration and exits INCONCLUSIVE, and
//! `run_with_profiles`, `matrix` and `smoke` do not exist, so the file does not compile.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::task::JoinSet;
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_coordination_mem::MemCoordination;
use wyrd_gateway_s3::sigv4::Credentials as GatewayCredentials;
use wyrd_gateway_s3::{S3Config, S3Gateway};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_server::Gateway;
use wyrd_traits::{MetadataStore, WriteBatch};
use wyrd_validate::matrix::{Declared, Operation, Profile, Refusal, State, PRE_0_1};
use wyrd_validate::s3::{ListRequest, ListVersion};
use wyrd_validate::smoke::scratch_bucket;
use wyrd_validate::{
    resolve_config, run_with_profiles, S3Client, EXIT_FAIL, EXIT_INCONCLUSIVE, EXIT_OK, EXIT_USAGE,
    PROFILES,
};

const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const REGION: &str = "us-east-1";
const BUCKET: &str = "validate";
const GATEWAY_CHUNK: usize = 256 * 1024;

/// An untagged build's identity, the shape `normalize_describe` gives it
/// (`crates/server/src/version/derivation.rs:41`).
const BUILD: &str = "0.0.0+git.1a2b3c4";

/// How much of a bulk delete's body the relay keeps.
const BODY_CAPTURE: usize = 64 * 1024;

type Backend = Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>;

// --- the in-process gateway ----------------------------------------------------------------

/// A Wyrd S3 gateway on an ephemeral loopback port, and the backend it serves, for reading what
/// a run left behind without going through the S3 surface. Field order is drop order: the
/// serving task is aborted before its chunk store's directory is removed.
struct GatewayFixture {
    _tasks: JoinSet<()>,
    addr: SocketAddr,
    backend: Arc<Backend>,
    _dir: tempfile::TempDir,
}

/// As `start_gateway_with_chunk` (`s3_client_roundtrip.rs:323-350`), advertising
/// `Server: wyrd/<server_version>`; `None` leaves `S3Config::new`'s `unknown`. The test bucket
/// exists: a listing answers `NoSuchBucket` for a bucket with no record (ADR-0046 decision 4,
/// `crates/server/src/lib.rs:746-758`), and CreateBucket is #511's, so the record is seeded on
/// the store before the gateway takes it, as `crates/server/tests/s3_list_objects.rs:82-89`
/// seeds it.
async fn start_gateway(server_version: Option<&str>) -> GatewayFixture {
    let dir = tempfile::tempdir().expect("temp dir");
    let meta = RedbMetadataStore::in_memory().expect("redb");
    meta.commit(WriteBatch::new().put(
        format!("bucket:{BUCKET}").into_bytes(),
        format!("{{\"name\":\"{BUCKET}\",\"created_millis\":0}}").into_bytes(),
    ))
    .await
    .expect("commit the bucket record");
    let backend = Arc::new(
        Gateway::new(
            meta,
            FsChunkStore::open(dir.path()).expect("fs store"),
            MemCoordination::new(),
        )
        .with_chunk_size(GATEWAY_CHUNK),
    );
    let mut config = S3Config::new(vec![GatewayCredentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    }]);
    if let Some(version) = server_version {
        config.server_version = version.to_string();
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = S3Gateway::new(Arc::clone(&backend), config);
    let mut tasks = JoinSet::new();
    tasks.spawn(async move {
        server.serve(listener).await.expect("serve");
    });
    GatewayFixture {
        _tasks: tasks,
        addr,
        backend,
        _dir: dir,
    }
}

/// The object stored under `key` in the test bucket, read from the backend itself.
fn stored(rt: &Runtime, gateway: &GatewayFixture, key: &str) -> Option<Vec<u8>> {
    rt.block_on(gateway.backend.get_object(&format!("{BUCKET}/{key}")))
        .expect("the backend reads")
}

/// `found` is exactly `planted`, byte for byte; the failure names what changed, not the bytes.
fn assert_unchanged(found: Option<Vec<u8>>, planted: &[u8], key: &str) {
    match found {
        None => panic!("`{key}`, which the run did not write, is gone"),
        Some(bytes) => assert!(
            bytes == planted,
            "`{key}`, which the run did not write, changed: {} bytes, {} planted",
            bytes.len(),
            planted.len()
        ),
    }
}

// --- the relay -----------------------------------------------------------------------------

/// One request as it crossed the relay.
#[derive(Debug, Clone)]
struct Request {
    method: String,
    /// The request target, percent-encoded as sent.
    target: String,
    /// The whole head, lowercase.
    head: String,
    /// The first [`BODY_CAPTURE`] bytes of a POST's body; empty for other methods.
    body: Vec<u8>,
}

impl Request {
    /// Whether the request could write: PUT, POST or DELETE.
    fn mutating(&self) -> bool {
        matches!(self.method.as_str(), "PUT" | "POST" | "DELETE")
    }

    /// The decoded path, without the query.
    fn path(&self) -> String {
        percent_decode(self.target.split('?').next().unwrap_or_default())
    }

    /// The query, as sent.
    fn query(&self) -> &str {
        self.target.split_once('?').map_or("", |(_, query)| query)
    }

    /// Whether the request names `bucket` and no object: `/<bucket>` or `/<bucket>/`.
    fn on_bucket(&self, bucket: &str) -> bool {
        let path = self.path();
        path.strip_suffix('/').unwrap_or(&path) == format!("/{bucket}")
    }

    /// Whether the query has the parameter `name`, with or without a value.
    fn has_param(&self, name: &str) -> bool {
        self.query()
            .split('&')
            .any(|p| p.split_once('=').map_or(p, |(k, _)| k) == name)
    }

    /// A v1 ListObjects of the test bucket: a bucket GET with a `prefix` and no `list-type`.
    fn is_v1_listing(&self) -> bool {
        self.method == "GET"
            && self.on_bucket(BUCKET)
            && self.has_param("prefix")
            && !self.has_param("list-type")
    }
}

#[derive(Default)]
struct Seen {
    connections: AtomicUsize,
    requests: Mutex<Vec<Request>>,
}

impl Seen {
    fn requests(&self) -> Vec<Request> {
        self.requests.lock().expect("relay log").clone()
    }

    fn log(&self) -> String {
        self.requests()
            .iter()
            .map(|r| format!("{} {}", r.method, r.target))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// An object planted server-side just before the first request that writes is forwarded.
struct Plant {
    backend: Arc<Backend>,
    key: String,
    bytes: Vec<u8>,
    done: AtomicBool,
}

#[derive(Default)]
struct RelayOptions {
    /// Drop every `Server` field from each response head.
    strip_server: bool,
    plant: Option<Plant>,
}

struct Relay {
    _tasks: JoinSet<()>,
    addr: SocketAddr,
    seen: Arc<Seen>,
}

async fn start_relay(upstream: SocketAddr, options: RelayOptions) -> Relay {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind relay");
    let addr = listener.local_addr().expect("relay addr");
    let seen = Arc::new(Seen::default());
    let options = Arc::new(options);
    let mut tasks = JoinSet::new();
    let accepting = Arc::clone(&seen);
    tasks.spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            accepting.connections.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(relay_one(
                client,
                upstream,
                Arc::clone(&accepting),
                Arc::clone(&options),
            ));
        }
    });
    Relay {
        _tasks: tasks,
        addr,
        seen,
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Read from `from` until a whole HTTP head is in hand; returns the bytes read (head and
/// whatever followed it) and where the head ends, or `None` if the stream ended first.
async fn read_head(from: &mut (impl AsyncRead + Unpin)) -> Option<(Vec<u8>, usize)> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(at) = find(&buf, b"\r\n\r\n") {
            return Some((buf, at + 4));
        }
        match from.read(&mut chunk).await {
            Ok(0) | Err(_) => return None,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    }
}

/// Copy `from` to `to` until `from` ends, calling `each` on every chunk; then shut `to` down.
async fn pump(
    mut from: impl AsyncRead + Unpin,
    mut to: impl AsyncWrite + Unpin,
    mut each: impl FnMut(&[u8]),
) {
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match from.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                each(&chunk[..n]);
                if to.write_all(&chunk[..n]).await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = to.shutdown().await;
}

async fn relay_one(
    mut client: TcpStream,
    upstream: SocketAddr,
    seen: Arc<Seen>,
    options: Arc<RelayOptions>,
) {
    let Some((first, head_end)) = read_head(&mut client).await else {
        return;
    };
    let head = String::from_utf8_lossy(&first[..head_end]).into_owned();
    let mut line = head.lines().next().unwrap_or_default().split(' ');
    let (method, target) = (
        line.next().unwrap_or_default().to_string(),
        line.next().unwrap_or_default().to_string(),
    );
    let capture_body = method == "POST";
    let request = Request {
        body: if capture_body {
            first[head_end..].to_vec()
        } else {
            Vec::new()
        },
        method,
        target,
        head: head.to_ascii_lowercase(),
    };
    if request.mutating() {
        if let Some(plant) = &options.plant {
            if !plant.done.swap(true, Ordering::SeqCst) {
                plant
                    .backend
                    .put_object(&format!("{BUCKET}/{}", plant.key), &plant.bytes)
                    .await
                    .expect("plant");
            }
        }
    }
    let index = {
        let mut requests = seen.requests.lock().expect("relay log");
        requests.push(request);
        requests.len() - 1
    };
    let Ok(mut gateway) = TcpStream::connect(upstream).await else {
        return;
    };
    if gateway.write_all(&first).await.is_err() {
        return;
    }
    let (client_read, mut client_write) = client.into_split();
    let (mut gateway_read, gateway_write) = gateway.into_split();
    let to_gateway = pump(client_read, gateway_write, |bytes| {
        if capture_body {
            let mut requests = seen.requests.lock().expect("relay log");
            let body = &mut requests[index].body;
            let room = BODY_CAPTURE.saturating_sub(body.len());
            body.extend_from_slice(&bytes[..bytes.len().min(room)]);
        }
    });
    let to_client = async {
        if options.strip_server {
            let Some((answer, head_end)) = read_head(&mut gateway_read).await else {
                let _ = client_write.shutdown().await;
                return;
            };
            let head = String::from_utf8_lossy(&answer[..head_end]);
            let kept: String = head
                .split_inclusive("\r\n")
                .filter(|line| !line.to_ascii_lowercase().starts_with("server:"))
                .collect();
            if client_write.write_all(kept.as_bytes()).await.is_err()
                || client_write.write_all(&answer[head_end..]).await.is_err()
            {
                return;
            }
        }
        pump(gateway_read, client_write, |_| {}).await;
    };
    tokio::join!(to_gateway, to_client);
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%')
            .then(|| bytes.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// --- running the validator -----------------------------------------------------------------

fn runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("the test runtime")
}

/// The validator's argv for `endpoint` and `run_id`: one worker, `--duration 1s`, each
/// overridden or extended by `extra`.
fn argv(endpoint: SocketAddr, run_id: &str, extra: &[(&str, &str)]) -> Vec<String> {
    let endpoint = format!("http://{endpoint}");
    let mut pairs: Vec<(String, String)> = [
        ("endpoint", endpoint.as_str()),
        ("region", REGION),
        ("bucket", BUCKET),
        ("scenario", "smoke"),
        ("duration", "1s"),
        ("workers", "1"),
        ("seed", "743"),
        ("out", "unused"),
        ("run-id", run_id),
        ("driver-placement", "loopback"),
    ]
    .iter()
    .map(|(f, v)| ((*f).to_string(), (*v).to_string()))
    .collect();
    for (flag, value) in extra {
        match pairs.iter_mut().find(|(f, _)| f == flag) {
            Some(pair) => pair.1 = (*value).to_string(),
            None => pairs.push(((*flag).to_string(), (*value).to_string())),
        }
    }
    pairs
        .into_iter()
        .flat_map(|(flag, value)| [format!("--{flag}"), value])
        .collect()
}

fn lookup(name: &str) -> Option<OsString> {
    match name {
        "AWS_ACCESS_KEY_ID" => Some(ACCESS_KEY.into()),
        "AWS_SECRET_ACCESS_KEY" => Some(SECRET_KEY.into()),
        _ => None,
    }
}

/// What one run of the validator did.
struct Ran {
    status: u8,
    stdout: String,
    stderr: String,
    wall: Duration,
}

impl Ran {
    fn says(&self) -> String {
        format!(
            "exit {}\n--- stdout\n{}--- stderr\n{}",
            self.status, self.stdout, self.stderr
        )
    }
}

/// The test's bound on one run of the validator: far past any `--duration` here, so a run that
/// hangs fails the test instead of holding it.
const RUN_BOUND: Duration = Duration::from_secs(120);

/// Run the library's `run_with_profiles` on a thread of its own, while the gateway and the
/// relay serve on the test runtime's workers; a run that outlasts [`RUN_BOUND`] fails the test.
fn smoke(endpoint: SocketAddr, run_id: &str, extra: &[(&str, &str)], profiles: &[Profile]) -> Ran {
    let args = argv(endpoint, run_id, extra);
    let profiles = profiles.to_vec();
    let (done, ran) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let started = Instant::now();
        let status = run_with_profiles(&args, &lookup, &mut out, &mut err, &profiles);
        let _ = done.send(Ran {
            status,
            stdout: String::from_utf8(out).expect("utf-8 stdout"),
            stderr: String::from_utf8(err).expect("utf-8 stderr"),
            wall: started.elapsed(),
        });
    });
    ran.recv_timeout(RUN_BOUND)
        .unwrap_or_else(|e| panic!("the run did not end within {RUN_BOUND:?}: {e}"))
}

/// The real `wyrd-validate` binary, with a cleared environment holding only the AWS pair (and a
/// coverage run's profile path, as `cli_surface.rs` passes it); killed, failing the test, if it
/// outlasts [`RUN_BOUND`].
fn smoke_binary(endpoint: SocketAddr, run_id: &str) -> Ran {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wyrd-validate"));
    command
        .args(argv(endpoint, run_id, &[]))
        .env_clear()
        .env("AWS_ACCESS_KEY_ID", ACCESS_KEY)
        .env("AWS_SECRET_ACCESS_KEY", SECRET_KEY)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    let started = Instant::now();
    let mut child = command.spawn().expect("spawn wyrd-validate");
    // Read on threads of their own, so a full pipe never stalls the child.
    let read = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = pipe {
                pipe.read_to_string(&mut text).expect("utf-8 output");
            }
            text
        })
    };
    let stdout = read(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = read(child.stderr.take().map(|p| Box::new(p) as _));
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for wyrd-validate") {
            break status;
        }
        if started.elapsed() > RUN_BOUND {
            let _ = child.kill();
            panic!("wyrd-validate did not end within {RUN_BOUND:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ran {
        status: status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(u8::MAX),
        stdout: stdout.join().expect("stdout reader"),
        stderr: stderr.join().expect("stderr reader"),
        wall: started.elapsed(),
    }
}

/// The value of the summary line `  <name> = <value>`.
fn summary_value<'a>(stderr: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("  {name} = ");
    stderr
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
}

/// The summary's `<passed> passed, <not> did not` for `operation`.
fn counts(stderr: &str, operation: Operation) -> Option<(u64, u64)> {
    let prefix = format!("  {}: ", operation.name());
    let line = stderr.lines().find(|line| line.starts_with(&prefix))?;
    let (_, counts) = line.rsplit_once("; ")?;
    let mut words = counts.split(' ');
    let passed = words.next()?.parse().ok()?;
    let not_passed = words.nth(1)?.parse().ok()?;
    Some((passed, not_passed))
}

/// Every key under `prefix`, through the validator's own listing (its reading of a listing is
/// itself checked against the real gateway by every PASS below).
fn keys_under(rt: &Runtime, endpoint: SocketAddr, prefix: &str) -> Vec<String> {
    let config =
        resolve_config(&argv(endpoint, "reader", &[]), &lookup).expect("the configuration");
    let client = S3Client::new(&config);
    let listing = rt
        .block_on(client.list_objects(&ListRequest {
            version: ListVersion::V2,
            prefix: prefix.to_string(),
            max_keys: 100,
        }))
        .expect("a listing");
    assert!(!listing.value.is_truncated);
    listing.value.objects.into_iter().map(|o| o.key).collect()
}

/// Every key a request in `requests` wrote to, or could have: the decoded object paths of every
/// PUT, POST and DELETE on an object, and the keys a bulk delete named.
fn written_keys(requests: &[Request]) -> Vec<String> {
    let mut keys = Vec::new();
    for request in requests.iter().filter(|r| r.mutating()) {
        let path = request.path();
        if let Some(key) = path.strip_prefix(&format!("/{BUCKET}/")) {
            keys.push(key.to_string());
        }
    }
    keys
}

/// The `<Key>` values of a bulk delete body.
fn delete_body_keys(body: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(body);
    text.split("<Key>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</Key>").map(|(key, _)| key.to_string()))
        .collect()
}

/// Every request that could write stayed inside what the run owns: an object under
/// `<run_id>/`, a bulk delete of keys under it, or the run's scratch bucket.
fn assert_only_own_writes(requests: &[Request], run_id: &str, context: &str) {
    let own_prefix = format!("/{BUCKET}/{run_id}/");
    let scratch = scratch_bucket(run_id);
    for request in requests.iter().filter(|r| r.mutating()) {
        let path = request.path();
        let bulk_delete =
            request.method == "POST" && request.on_bucket(BUCKET) && request.has_param("delete");
        if bulk_delete {
            let keys = delete_body_keys(&request.body);
            assert!(!keys.is_empty(), "{context}: a bulk delete naming no key");
            for key in keys {
                assert!(
                    key.starts_with(&format!("{run_id}/")),
                    "{context}: a bulk delete named `{key}`, outside `{run_id}/`"
                );
            }
        } else {
            assert!(
                path.starts_with(&own_prefix) || request.on_bucket(&scratch),
                "{context}: {} {} writes outside `{run_id}/` and the scratch bucket",
                request.method,
                request.target
            );
        }
    }
}

// --- (D) PASS against loopback -------------------------------------------------------------

#[test]
fn d_smoke_passes_against_a_loopback_gateway_every_row_checked_against_the_real_answer() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let relay = rt.block_on(start_relay(gateway.addr, RelayOptions::default()));
    let ran = smoke(relay.addr, "smoke-d", &[], &PROFILES);
    let log = relay.seen.log();
    assert_eq!(ran.status, EXIT_OK, "{}\n--- wire\n{log}", ran.says());
    assert!(
        ran.stderr.contains("wyrd-validate: smoke PASS"),
        "{}",
        ran.says()
    );
    // The version came off the wire, and selected the shipped profile.
    assert!(
        ran.stdout.contains(&format!(
            "  server-version = {BUILD} (from Server header)\n"
        )),
        "{}",
        ran.says()
    );
    assert!(ran
        .stdout
        .contains("  profile = pre-0.1 (versions below 0.1.0)\n"));
    // Every row of the profile passed against the real answers, and nothing failed.
    for row in PRE_0_1.rows {
        let (passed, not_passed) = counts(&ran.stderr, row.operation)
            .unwrap_or_else(|| panic!("no summary line for {}: {}", row.operation, ran.says()));
        assert!(
            passed >= 1,
            "{} never passed: {}",
            row.operation,
            ran.says()
        );
        assert_eq!(not_passed, 0, "{}: {}", row.operation, ran.says());
    }
    // And every unsupported operation reached the wire in its own shape.
    let requests = relay.seen.requests();
    let sent = |what: &str, matches: &dyn Fn(&Request) -> bool| {
        assert!(
            requests.iter().any(matches),
            "no {what} on the wire:\n{log}"
        );
    };
    sent("CopyObject", &|r| {
        r.method == "PUT" && r.head.contains("\r\nx-amz-copy-source: ")
    });
    sent("CreateMultipartUpload", &|r| {
        r.method == "POST" && r.has_param("uploads")
    });
    sent("UploadPart", &|r| {
        r.method == "PUT" && r.has_param("partNumber") && r.has_param("uploadId")
    });
    sent("CompleteMultipartUpload", &|r| {
        r.method == "POST" && r.has_param("uploadId")
    });
    sent("AbortMultipartUpload", &|r| {
        r.method == "DELETE" && r.has_param("uploadId")
    });
    sent("ListParts", &|r| {
        r.method == "GET" && !r.on_bucket(BUCKET) && r.has_param("uploadId")
    });
    sent("ListMultipartUploads", &|r| {
        r.method == "GET" && r.on_bucket(BUCKET) && r.has_param("uploads")
    });
    sent("GetBucketLocation", &|r| {
        r.method == "GET" && r.on_bucket(BUCKET) && r.has_param("location")
    });
    let scratch = scratch_bucket("smoke-d");
    sent("CreateBucket", &|r| {
        r.method == "PUT" && r.on_bucket(&scratch)
    });
    sent("DeleteBucket", &|r| {
        r.method == "DELETE" && r.on_bucket(&scratch)
    });
    sent("ListBuckets", &|r| r.method == "GET" && r.path() == "/");
    sent("HeadBucket", &|r| r.method == "HEAD" && r.on_bucket(BUCKET));
    // The ranged and the conditional GET went out as such, and so did the v1 listing.
    sent("a ranged GET", &|r| {
        r.method == "GET" && r.head.contains("\r\nrange: bytes=")
    });
    sent("a conditional GET", &|r| {
        r.method == "GET" && r.head.contains("\r\nif-none-match: \"")
    });
    sent("a v1 listing", &Request::is_v1_listing);
}

#[test]
fn d_the_real_binary_runs_smoke_to_a_pass() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let ran = smoke_binary(gateway.addr, "smoke-d-binary");
    assert_eq!(ran.status, EXIT_OK, "{}", ran.says());
    assert!(
        ran.stderr.contains("wyrd-validate: smoke PASS"),
        "{}",
        ran.says()
    );
    assert!(
        ran.stdout.contains("(from Server header)"),
        "{}",
        ran.says()
    );
}

// --- (E) a wrongly succeeding unsupported operation fails the run ----------------------------

#[test]
fn e_a_profile_declaring_put_object_unsupported_fails_the_run_as_unexpected_support() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let relay = rt.block_on(start_relay(gateway.addr, RelayOptions::default()));
    let mut rows = PRE_0_1.rows.to_vec();
    for row in &mut rows {
        if row.operation == Operation::PutObject {
            row.state = State::Unsupported(Refusal {
                issue: 1,
                status: 501,
                code: Declared::Code("NotImplemented"),
            });
            row.citation = "test-local: PutObject declared unsupported";
        }
    }
    let profile = Profile {
        name: "put-unsupported",
        versions: PRE_0_1.versions.clone(),
        rows: Vec::leak(rows),
    };
    let ran = smoke(relay.addr, "smoke-e", &[], &[profile]);
    assert_eq!(ran.status, EXIT_FAIL, "{}", ran.says());
    assert!(
        ran.stderr.contains("wyrd-validate: smoke FAIL"),
        "{}",
        ran.says()
    );
    assert!(
        ran.stderr
            .lines()
            .any(|line| line.contains("unexpected-support")
                && line.contains("PutObject answered 200")),
        "{}",
        ran.says()
    );
    // The run stopped at that answer, and left what it wrote in place: on FAIL nothing is
    // deleted.
    let requests = relay.seen.requests();
    let puts: Vec<&Request> = requests.iter().filter(|r| r.method == "PUT").collect();
    assert_eq!(puts.len(), 1, "{}", relay.seen.log());
    assert!(!requests
        .iter()
        .any(|r| r.method == "DELETE" || r.method == "POST"));
    let key = written_keys(&requests).pop().expect("the PUT's key");
    assert!(key.starts_with("smoke-e/"), "{key}");
    assert!(stored(&rt, &gateway, &key).is_some(), "{key} was removed");
    assert!(
        ran.stderr.contains(&format!(
            "left in place (written, or possibly written, by this run): {key}"
        )),
        "{}",
        ran.says()
    );
}

// --- (F) the version source ----------------------------------------------------------------

#[test]
fn f_given_server_version_wins_over_a_header_no_profile_covers() {
    let rt = runtime();
    // A tagged release: no shipped profile covers 0.1.0, so the header alone selects nothing.
    let gateway = rt.block_on(start_gateway(Some("0.1.0")));
    let alone = smoke(gateway.addr, "smoke-f-header", &[], &PROFILES);
    assert_eq!(alone.status, EXIT_INCONCLUSIVE, "{}", alone.says());
    assert!(alone
        .stdout
        .contains("  server-version = 0.1.0 (from Server header)\n"));
    assert!(
        alone.stderr.contains("`--server-version"),
        "{}",
        alone.says()
    );
    // Named on the command line, the given version is the one judged.
    let given = smoke(
        gateway.addr,
        "smoke-f-given",
        &[("server-version", BUILD)],
        &PROFILES,
    );
    assert_eq!(given.status, EXIT_OK, "{}", given.says());
    assert!(
        given.stdout.contains(&format!(
            "  server-version = {BUILD} (given; the Server header says `wyrd/0.1.0`)\n"
        )),
        "{}",
        given.says()
    );
    assert!(given
        .stdout
        .contains(&format!("  --server-version = {BUILD} (given)\n")));
}

/// INCONCLUSIVE naming `--server-version`, after only the first, read-only, request.
fn assert_no_version(ran: &Ran, relay: &Relay) {
    assert_eq!(ran.status, EXIT_INCONCLUSIVE, "{}", ran.says());
    assert!(ran.stderr.contains("`--server-version"), "{}", ran.says());
    assert!(ran.stderr.contains("INCONCLUSIVE"), "{}", ran.says());
    let requests = relay.seen.requests();
    assert_eq!(requests.len(), 1, "{}", relay.seen.log());
    assert!(
        !requests.iter().any(Request::mutating),
        "{}",
        relay.seen.log()
    );
}

#[test]
fn f_no_server_header_and_no_flag_is_inconclusive_before_any_write() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let relay = rt.block_on(start_relay(
        gateway.addr,
        RelayOptions {
            strip_server: true,
            ..RelayOptions::default()
        },
    ));
    let ran = smoke(relay.addr, "smoke-f-absent", &[], &PROFILES);
    assert_no_version(&ran, &relay);
    assert!(ran.stderr.contains("no Server header"), "{}", ran.says());
}

#[test]
fn f_a_wyrd_unknown_header_and_no_flag_is_inconclusive_before_any_write() {
    let rt = runtime();
    // `S3Config::new` advertises `wyrd/unknown` until told a version.
    let gateway = rt.block_on(start_gateway(None));
    let relay = rt.block_on(start_relay(gateway.addr, RelayOptions::default()));
    let ran = smoke(relay.addr, "smoke-f-unknown", &[], &PROFILES);
    assert_no_version(&ran, &relay);
    assert!(ran.stderr.contains("`wyrd/unknown`"), "{}", ran.says());
}

// --- (G) only its own keys -----------------------------------------------------------------

#[test]
fn g_an_occupied_run_prefix_is_refused_before_any_write() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let planted_key = "smoke-g-occupied/someone-elses";
    let planted: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    rt.block_on(
        gateway
            .backend
            .put_object(&format!("{BUCKET}/{planted_key}"), &planted),
    )
    .expect("plant");
    let relay = rt.block_on(start_relay(gateway.addr, RelayOptions::default()));
    let ran = smoke(relay.addr, "smoke-g-occupied", &[], &PROFILES);
    assert_eq!(ran.status, EXIT_USAGE, "{}", ran.says());
    assert!(ran.stderr.contains("`smoke-g-occupied/`"), "{}", ran.says());
    assert!(ran.stderr.contains("`--run-id`"), "{}", ran.says());
    let requests = relay.seen.requests();
    assert!(
        !requests.iter().any(Request::mutating),
        "{}",
        relay.seen.log()
    );
    assert_unchanged(stored(&rt, &gateway, planted_key), &planted, planted_key);
    assert_eq!(
        keys_under(&rt, gateway.addr, "smoke-g-occupied/"),
        vec![planted_key.to_string()]
    );
}

#[test]
fn g_a_pass_writes_only_under_its_prefix_and_deletes_only_what_it_wrote() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let run_id = "smoke-g-own";
    // Planted after the occupancy check, under the run prefix but outside every sweep's: no
    // sweep lists it, so only a cleanup that deletes by listing the run prefix would touch it.
    let planted_key = format!("{run_id}/planted-by-someone-else");
    let planted: Vec<u8> = (0..=255u8).rev().cycle().take(3000).collect();
    let relay = rt.block_on(start_relay(
        gateway.addr,
        RelayOptions {
            plant: Some(Plant {
                backend: Arc::clone(&gateway.backend),
                key: planted_key.clone(),
                bytes: planted.clone(),
                done: AtomicBool::new(false),
            }),
            ..RelayOptions::default()
        },
    ));
    let ran = smoke(relay.addr, run_id, &[], &PROFILES);
    assert_eq!(
        ran.status,
        EXIT_OK,
        "{}\n--- wire\n{}",
        ran.says(),
        relay.seen.log()
    );
    let requests = relay.seen.requests();
    assert_only_own_writes(&requests, run_id, "PASS run");
    // Nothing the run wrote is left, read from the backend itself.
    let written = written_keys(&requests);
    assert!(written.len() >= 4, "{}", relay.seen.log());
    for key in &written {
        assert_eq!(
            stored(&rt, &gateway, key),
            None,
            "{key} is left after a PASS"
        );
    }
    assert!(
        ran.stderr.contains("left in place: nothing"),
        "{}",
        ran.says()
    );
    // The object it did not write survives, byte for byte, and is all that is left.
    assert_unchanged(stored(&rt, &gateway, &planted_key), &planted, &planted_key);
    assert_eq!(
        keys_under(&rt, gateway.addr, &format!("{run_id}/")),
        vec![planted_key]
    );
}

// --- (H) scale arguments -------------------------------------------------------------------

#[test]
fn h_smoke_repeats_its_sweep_until_the_duration_has_elapsed() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let relay = rt.block_on(start_relay(gateway.addr, RelayOptions::default()));
    let ran = smoke(relay.addr, "smoke-h", &[("duration", "2s")], &PROFILES);
    assert_eq!(ran.status, EXIT_OK, "{}", ran.says());
    let sweeps: u64 = summary_value(&ran.stderr, "sweeps")
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("no sweep count: {}", ran.says()));
    let elapsed: f64 = summary_value(&ran.stderr, "elapsed")
        .and_then(|value| value.strip_suffix(" s (--duration 2s)"))
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("no elapsed time: {}", ran.says()));
    assert!(elapsed >= 2.0, "elapsed {elapsed} s: {}", ran.says());
    assert!(
        ran.wall >= Duration::from_secs(2),
        "the run took {:?}",
        ran.wall
    );
    // Repeated, not one sweep and a wait: a loopback sweep takes far less than 2 s.
    assert!(sweeps >= 2, "{sweeps} sweeps: {}", ran.says());
    // The count is what reached the wire: one v1 listing per sweep.
    let v1_listings = relay
        .seen
        .requests()
        .iter()
        .filter(|r| r.is_v1_listing())
        .count() as u64;
    assert_eq!(v1_listings, sweeps, "{}", ran.says());
}

#[test]
fn h_workers_other_than_one_is_inconclusive_before_any_request() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let relay = rt.block_on(start_relay(gateway.addr, RelayOptions::default()));
    let ran = smoke(
        relay.addr,
        "smoke-h-workers",
        &[("workers", "2")],
        &PROFILES,
    );
    assert_eq!(ran.status, EXIT_INCONCLUSIVE, "{}", ran.says());
    assert!(ran.stderr.contains("`--workers 2`"), "{}", ran.says());
    // After the configuration echo.
    assert!(
        ran.stdout
            .contains("wyrd-validate: resolved configuration\n")
            && ran.stdout.contains("  --workers = 2\n"),
        "{}",
        ran.says()
    );
    assert_eq!(relay.seen.connections.load(Ordering::SeqCst), 0);
}

#[test]
fn h_a_duration_that_does_not_parse_is_refused_by_name() {
    let rt = runtime();
    let gateway = rt.block_on(start_gateway(Some(BUILD)));
    let relay = rt.block_on(start_relay(gateway.addr, RelayOptions::default()));
    for bad in ["7x", "2", "s", "-2s", "+2s", "2 s", "1h30m", "0s", "2S"] {
        let ran = smoke(
            relay.addr,
            "smoke-h-duration",
            &[("duration", bad)],
            &PROFILES,
        );
        assert_eq!(ran.status, EXIT_USAGE, "--duration {bad}: {}", ran.says());
        assert!(
            ran.stderr.contains("flag `--duration`") && ran.stderr.contains(&format!("`{bad}`")),
            "--duration {bad}: {}",
            ran.says()
        );
        assert!(ran.stdout.is_empty(), "{}", ran.says());
    }
    assert_eq!(relay.seen.connections.load(Ordering::SeqCst), 0);
}
