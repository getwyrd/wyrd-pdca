//! **The multipart gateway seam** (issue #666, the first slice of #508).
//!
//! `MultipartGateway` is the companion trait that carries the six S3 multipart verbs across
//! the gateway seam in neutral container/object vocabulary (ADR-0046 decision 6, ADR-0010).
//! This slice adds the seam only: the composition root answers every verb "not implemented",
//! the S3 crate requires the trait of its gateway type, and the wire does not change. What is
//! asserted, each separately:
//!
//! 1. [`the_composition_root_answers_every_multipart_verb_not_implemented`] — each of the six
//!    methods, called on a real `wyrd_server::Gateway`, returns `Ok` carrying the seam's typed
//!    `MultipartRefusal::NotImplemented` arm: a value the caller matches on, not a panic and
//!    not an untyped boxed error.
//!    [`the_trait_defaults_answer_every_multipart_verb_not_implemented`] runs the same six
//!    checks against an empty `impl MultipartGateway`, so the trait's default bodies (what the
//!    S3 crate's object-only test doubles rely on) answer the same typed value.
//! 2. [`the_real_gateway_meets_the_s3_crates_gateway_bound`] — a helper bounded on
//!    `ObjectGateway + ContainerGateway + MultipartGateway` accepts the real `Gateway` and
//!    composes `S3Gateway::new` over it; the served router still answers every multipart form
//!    `501`.
//! 3. [`every_multipart_form_still_answers_501_without_entering_the_seam`] — over the
//!    in-process listener, a sentinel gateway counts every call into its seams. Each multipart
//!    form answers `501 NotImplemented` naming its subresource, and afterwards the multipart
//!    counter is **zero**. The status alone cannot tell "refused by routing" from "entered the
//!    seam, got `NotImplemented`, mapped it to `501`"; the counter can.
//!
//! RED on the base: the file names `MultipartGateway`, which does not exist there, so it does
//! not build. Leg 3's assertions would hold on the base too, on purpose: it pins "routing
//! untouched" (#667 owns routing), and its counter goes red if a multipart form ever reaches
//! the seam, whatever status the request then answers.

#![forbid(unsafe_code)]
// wall-clock exempt (test crate): SigV4 request dates against a live in-process server use
// real wall time; nothing here mixes clock sources within one asserted lifecycle (#619).
// File scope (not per-site) is deliberate here: a test crate never ships, so no production
// lifecycle can acquire a mixed clock from it.
#![allow(clippy::disallowed_methods)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures_util::Stream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_coordination_mem::MemCoordination;
use wyrd_gateway_core::{
    ContainerGateway, ContentHash, ListedObject, MultipartAbortAnswer, MultipartCompleteAnswer,
    MultipartCreateAnswer, MultipartGateway, MultipartListPartsAnswer, MultipartListUploadsAnswer,
    MultipartRefusal, MultipartUploadPartAnswer, ObjectGateway, ObjectMeta, ObjectRead,
};
use wyrd_gateway_s3::sigv4::{format_amz_date, sign, Credentials};
use wyrd_gateway_s3::{S3Config, S3Gateway};
use wyrd_metadata_redb::RedbMetadataStore;
use wyrd_server::Gateway;
use wyrd_traits::Result;

const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const REGION: &str = "us-east-1";

/// The six S3 multipart wire forms, as `(method, path, query, the subresource the 501 names)`.
/// The named subresource is the FIRST denylisted query key, which is what the S3 crate's guard
/// reports (`crates/gateway-s3/src/lib.rs`, `unsupported_subresource`).
const MULTIPART_FORMS: [(&str, &str, &str, &str); 6] = [
    // CreateMultipartUpload
    ("POST", "/b/k", "uploads", "uploads"),
    // UploadPart
    ("PUT", "/b/k", "partNumber=1&uploadId=u", "partNumber"),
    // CompleteMultipartUpload
    ("POST", "/b/k", "uploadId=u", "uploadId"),
    // AbortMultipartUpload
    ("DELETE", "/b/k", "uploadId=u", "uploadId"),
    // ListParts
    ("GET", "/b/k", "uploadId=u", "uploadId"),
    // ListMultipartUploads (bucket-scoped)
    ("GET", "/b", "uploads", "uploads"),
];

type Backend = Gateway<RedbMetadataStore, FsChunkStore, MemCoordination>;

/// The real composition-root gateway, over the in-memory redb store as the peer suites build it
/// (`s3_list_objects.rs`, `request_capacity_planes.rs`).
fn real_gateway(dir: &std::path::Path) -> Backend {
    Gateway::new(
        RedbMetadataStore::in_memory().expect("redb"),
        FsChunkStore::open(dir).expect("fs store"),
        MemCoordination::new(),
    )
}

// ---- leg 1: the composition root answers every verb, typed ---------------------------------

#[tokio::test]
async fn the_composition_root_answers_every_multipart_verb_not_implemented() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert_every_verb_not_implemented(&real_gateway(dir.path())).await;
}

/// A gateway with no multipart concept: the empty `impl` the S3 crate's object-only test
/// doubles rely on, so only the trait's default bodies answer for it.
struct NoMultipart;

impl MultipartGateway for NoMultipart {}

#[tokio::test]
async fn the_trait_defaults_answer_every_multipart_verb_not_implemented() {
    assert_every_verb_not_implemented(&NoMultipart).await;
}

/// Call all six verbs on `gateway` and assert each returns `Ok` carrying the typed
/// `Refused(NotImplemented)` value. Each call is fully qualified, so it provably reaches `G`'s
/// `MultipartGateway` impl, never a same-named inherent method.
async fn assert_every_verb_not_implemented<G: MultipartGateway>(gateway: &G) {
    let create = MultipartGateway::create_multipart_upload(
        gateway,
        "b",
        "k",
        Some("text/plain".to_string()),
    )
    .await
    .expect("create answers a value, not Err");
    assert_eq!(
        create,
        MultipartCreateAnswer::Refused(MultipartRefusal::NotImplemented),
        "create_multipart_upload",
    );

    let body = futures_util::stream::iter(vec![Ok::<Bytes, wyrd_traits::BoxError>(
        Bytes::from_static(b"part one"),
    )]);
    let upload =
        MultipartGateway::upload_part(gateway, "b", "k", "u", 1, body, ContentHash::Unverified)
            .await
            .expect("upload_part answers a value, not Err");
    assert_eq!(
        upload,
        MultipartUploadPartAnswer::Refused(MultipartRefusal::NotImplemented),
        "upload_part",
    );

    let complete = MultipartGateway::complete_multipart_upload(
        gateway,
        "b",
        "k",
        "u",
        vec![(1, "etag-of-part-one".to_string())],
    )
    .await
    .expect("complete answers a value, not Err");
    assert_eq!(
        complete,
        MultipartCompleteAnswer::Refused(MultipartRefusal::NotImplemented),
        "complete_multipart_upload",
    );

    let abort = MultipartGateway::abort_multipart_upload(gateway, "b", "k", "u")
        .await
        .expect("abort answers a value, not Err");
    assert_eq!(
        abort,
        MultipartAbortAnswer::Refused(MultipartRefusal::NotImplemented),
        "abort_multipart_upload",
    );

    let parts = MultipartGateway::list_parts(gateway, "b", "k", "u")
        .await
        .expect("list_parts answers a value, not Err");
    assert_eq!(
        parts,
        MultipartListPartsAnswer::Refused(MultipartRefusal::NotImplemented),
        "list_parts",
    );

    let uploads = MultipartGateway::list_multipart_uploads(gateway, "b")
        .await
        .expect("list_multipart_uploads answers a value, not Err");
    assert_eq!(
        uploads,
        MultipartListUploadsAnswer::Refused(MultipartRefusal::NotImplemented),
        "list_multipart_uploads",
    );
}

// ---- leg 2: the real Gateway satisfies the S3 crate's bound --------------------------------

/// Serve `gateway` over an ephemeral loopback port. Bounded on the full triple the S3 crate's
/// `S3Gateway::new` requires, so a gateway missing any one of the three seams does not compile
/// here.
async fn start_s3<G>(gateway: Arc<G>) -> SocketAddr
where
    G: ObjectGateway + ContainerGateway + MultipartGateway,
{
    let config = S3Config::new(vec![Credentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    }]);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = S3Gateway::new(gateway, config);
    tokio::spawn(async move {
        server.serve(listener).await.expect("serve");
    });
    addr
}

#[tokio::test]
async fn the_real_gateway_meets_the_s3_crates_gateway_bound() {
    let dir = tempfile::tempdir().expect("temp dir");
    let addr = start_s3(Arc::new(real_gateway(dir.path()))).await;
    for (method, path, query, subresource) in MULTIPART_FORMS {
        assert_refused_501(addr, method, path, query, subresource).await;
    }
}

// ---- leg 3: the wire is unchanged, and the seam is not entered -----------------------------

/// A gateway that records every call into any of its seams. The counters are fields, not
/// statics (ADR-0035). Every method answers what a gateway without the capability would —
/// the multipart verbs `NotImplemented`, the object verbs an error, the listing "no such
/// container" — so a request that DID enter a seam could still come back with a plausible
/// status; only the counters tell.
#[derive(Default)]
struct Sentinel {
    /// Calls into any `MultipartGateway` method.
    multipart_calls: AtomicUsize,
    /// Calls into any `ObjectGateway` or `ContainerGateway` method.
    other_calls: AtomicUsize,
}

impl Sentinel {
    fn multipart(&self) {
        self.multipart_calls.fetch_add(1, Ordering::SeqCst);
    }

    fn other(&self) -> wyrd_traits::BoxError {
        self.other_calls.fetch_add(1, Ordering::SeqCst);
        "the sentinel's object seam is never reached by a multipart form".into()
    }
}

impl MultipartGateway for Sentinel {
    async fn create_multipart_upload(
        &self,
        _container: &str,
        _key: &str,
        _content_type: Option<String>,
    ) -> Result<MultipartCreateAnswer> {
        self.multipart();
        Ok(MultipartCreateAnswer::Refused(
            MultipartRefusal::NotImplemented,
        ))
    }

    async fn upload_part<S>(
        &self,
        _container: &str,
        _key: &str,
        _upload_id: &str,
        _part_number: u32,
        _body: S,
        _expected: ContentHash,
    ) -> Result<MultipartUploadPartAnswer>
    where
        S: Stream<Item = Result<Bytes>> + Send + Unpin + 'static,
    {
        self.multipart();
        Ok(MultipartUploadPartAnswer::Refused(
            MultipartRefusal::NotImplemented,
        ))
    }

    async fn complete_multipart_upload(
        &self,
        _container: &str,
        _key: &str,
        _upload_id: &str,
        _named_parts: Vec<(u32, String)>,
    ) -> Result<MultipartCompleteAnswer> {
        self.multipart();
        Ok(MultipartCompleteAnswer::Refused(
            MultipartRefusal::NotImplemented,
        ))
    }

    async fn abort_multipart_upload(
        &self,
        _container: &str,
        _key: &str,
        _upload_id: &str,
    ) -> Result<MultipartAbortAnswer> {
        self.multipart();
        Ok(MultipartAbortAnswer::Refused(
            MultipartRefusal::NotImplemented,
        ))
    }

    async fn list_parts(
        &self,
        _container: &str,
        _key: &str,
        _upload_id: &str,
    ) -> Result<MultipartListPartsAnswer> {
        self.multipart();
        Ok(MultipartListPartsAnswer::Refused(
            MultipartRefusal::NotImplemented,
        ))
    }

    async fn list_multipart_uploads(&self, _container: &str) -> Result<MultipartListUploadsAnswer> {
        self.multipart();
        Ok(MultipartListUploadsAnswer::Refused(
            MultipartRefusal::NotImplemented,
        ))
    }
}

impl ContainerGateway for Sentinel {
    async fn list_container(&self, _container: &str) -> Result<Option<Vec<ListedObject>>> {
        let _ = self.other();
        Ok(None)
    }
}

impl ObjectGateway for Sentinel {
    async fn put_object_streaming<S>(
        &self,
        _key: &str,
        _source: S,
        _expected: ContentHash,
        _content_type: Option<String>,
    ) -> Result<String>
    where
        S: Stream<Item = Result<Bytes>> + Send + Unpin + 'static,
    {
        Err(self.other())
    }

    async fn get_object_streaming(self: Arc<Self>, _key: &str) -> Result<Option<ObjectRead>> {
        Err(self.other())
    }

    async fn head_object(&self, _key: &str) -> Result<Option<ObjectMeta>> {
        Err(self.other())
    }

    async fn delete_object(&self, _key: &str) -> Result<bool> {
        Err(self.other())
    }
}

#[tokio::test]
async fn every_multipart_form_still_answers_501_without_entering_the_seam() {
    let sentinel = Arc::new(Sentinel::default());
    let addr = start_s3(Arc::clone(&sentinel)).await;

    for (method, path, query, subresource) in MULTIPART_FORMS {
        assert_refused_501(addr, method, path, query, subresource).await;
    }

    // Each response above was read to the end, so its handler has returned: any seam call a
    // form made has already been counted.
    assert_eq!(
        sentinel.multipart_calls.load(Ordering::SeqCst),
        0,
        "a multipart form entered the MultipartGateway seam; routing is #667's, not this slice's",
    );
    assert_eq!(
        sentinel.other_calls.load(Ordering::SeqCst),
        0,
        "a multipart form fell through to an object or listing verb",
    );
}

// ---- the wire helper -----------------------------------------------------------------------

/// Send one signed multipart form and assert it is refused `501 NotImplemented`, the message
/// naming `subresource`.
async fn assert_refused_501(
    addr: SocketAddr,
    method: &str,
    path: &str,
    query: &str,
    subresource: &str,
) {
    let (status, body) = send_signed(addr, method, path, query).await;
    let form = format!("{method} {path}?{query}");
    assert_eq!(status, 501, "{form}: expected 501, body: {body}");
    assert!(
        body.contains("<Code>NotImplemented</Code>"),
        "{form}: expected the NotImplemented code, body: {body}",
    );
    assert!(
        body.contains(&format!("the `{subresource}` S3 subresource")),
        "{form}: expected the 501 to name `{subresource}`, body: {body}",
    );
}

/// Send a raw SigV4-signed `{method} {path}?{query}` with an empty body and return
/// `(status, body)`. The query is signed as it rides on the wire (`sign` canonicalizes it), so a
/// valueless key like `uploads` passes authentication and reaches routing. Bounded: a server
/// that never answers fails the test instead of hanging it.
async fn send_signed(addr: SocketAddr, method: &str, path: &str, query: &str) -> (u16, String) {
    let host = addr.to_string();
    let creds = Credentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    };
    let amz_date = format_amz_date(SystemTime::now());
    let signed = sign(
        method, path, query, &host, &amz_date, b"", &creds, REGION, "s3",
    );
    let mut request = format!("{method} {path}?{query} HTTP/1.1\r\n");
    request.push_str(&format!("host: {host}\r\n"));
    request.push_str(&format!("authorization: {}\r\n", signed.authorization));
    request.push_str(&format!("x-amz-date: {}\r\n", signed.amz_date));
    request.push_str(&format!(
        "x-amz-content-sha256: {}\r\n",
        signed.content_sha256
    ));
    request.push_str("content-length: 0\r\n");
    request.push_str("connection: close\r\n\r\n");

    let exchange = async {
        let mut stream = TcpStream::connect(addr).await.expect("connect");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write request");
        stream.flush().await.expect("flush");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.expect("read response");
        raw
    };
    let raw = tokio::time::timeout(Duration::from_secs(30), exchange)
        .await
        .unwrap_or_else(|_| panic!("{method} {path}?{query}: no response within 30s"));

    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("response has a header terminator");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let status: u16 = head
        .lines()
        .next()
        .expect("status line")
        .split_whitespace()
        .nth(1)
        .expect("status code")
        .parse()
        .expect("numeric status");
    let body_raw = &raw[split + 4..];
    let chunked = head.lines().any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    let body = if chunked {
        dechunk(body_raw)
    } else {
        body_raw.to_vec()
    };
    (status, String::from_utf8_lossy(&body).into_owned())
}

/// Decode an HTTP/1.1 chunked-transfer-encoded body.
fn dechunk(mut raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(line_end) = raw.windows(2).position(|w| w == b"\r\n") {
        let size_str = String::from_utf8_lossy(&raw[..line_end]);
        let size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
        raw = &raw[line_end + 2..];
        if size == 0 || size > raw.len() {
            break;
        }
        out.extend_from_slice(&raw[..size]);
        raw = &raw[(size + 2).min(raw.len())..];
    }
    out
}
