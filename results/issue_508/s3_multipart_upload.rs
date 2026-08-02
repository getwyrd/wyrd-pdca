//! **S3 multipart upload — verb semantics, routing safety, publication accounting, and the
//! chunk-size selection decision 7 forces on ordinary `PutObject`** (issue #508).
//!
//! Everything here runs over the wire against an in-process gateway: a real `aws-sdk-s3`
//! client and hand-signed raw requests for the forms an SDK cannot spell. Nothing is stubbed
//! — the gateway, the S3 wire layer, the commit protocol and the chunk store are all
//! production code; only the metadata *backend* is a test double, exactly as
//! `RedbMetadataStore::in_memory` is, and it is retained by the test so raw record prefixes
//! can be observed while a request is in flight.
//!
//! **This file references only base-visible symbols.** The C4-verify red leg reverts the
//! patch and compiles these tests against `origin/main`; a reference to anything the patch
//! *adds* would turn the red into a build error over a run that executed nothing. So every
//! record class is observed by **raw key prefix** through the store handle, and every ETag
//! oracle is computed here from the part bodies rather than imported.
//!
//! On the base every object-scoped multipart form is refused `501` by the subresource
//! denylist and `GET /b?uploads` likewise, so **every leg-A assertion of an exact status +
//! S3 code reds** (`501` is neither the required `400` nor the required `404`) — and one form
//! reds the other way: `PUT /b/k?part%4Eumber=1` answers **`200`, overwriting the object**,
//! because the denylist matches RAW keys while SigV4 canonicalisation decodes-then-re-encodes.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use aws_credential_types::Credentials as SdkCredentials;
use aws_sdk_s3::config::Region;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use aws_sdk_s3::Client;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use wyrd_chunkstore_fs::FsChunkStore;
use wyrd_coordination_mem::MemCoordination;
use wyrd_gateway_s3::sigv4::{
    format_amz_date, sign, sign_with_payload_hash, Credentials as GatewayCredentials,
};
use wyrd_gateway_s3::{S3Config, S3Gateway};
use wyrd_server::Gateway;
use wyrd_traits::{CommitOutcome, MetadataStore, Result, WriteBatch};

const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const REGION: &str = "us-east-1";
const BUCKET: &str = "wyrd-bucket";

/// S3's minimum size for a **non-final** part.
const MIN_PART: usize = 5 * 1024 * 1024;

// ---------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------

/// An in-memory metadata store the test **retains a handle to** after the gateway takes it.
///
/// `RedbMetadataStore` is not `Clone`, so `Gateway::new` consumes the only handle and the
/// raw-prefix observations below would be impossible. This is the same shape as the `MemMeta`
/// doubles the custodian and core suites already use, and it implements only the three
/// base-visible `MetadataStore` methods — so this file compiles unchanged against
/// `origin/main` and the red leg fails by assertion rather than by build error.
#[derive(Clone, Default)]
struct MemMeta {
    data: Arc<Mutex<BTreeMap<Vec<u8>, Bytes>>>,
    /// A key whose **next** read answers "absent" although the store holds a value — the
    /// one-shot interleaving injector described on [`MemMeta::hide_next_read`].
    hide_once: Arc<Mutex<Option<Vec<u8>>>>,
}

impl MemMeta {
    /// Arm a one-shot: the next `get(key)` answers `None` even though the store holds a value,
    /// and every read after it is ordinary.
    ///
    /// This reproduces **one specific interleaving**, deterministically and without any change
    /// to the code under test: a writer that reads a key as absent and then commits a batch
    /// preconditioned on that absence, while a concurrent writer's value lands in between. Left
    /// to real timing, that window is the few microseconds between one `get` and one `commit`,
    /// so a wall-clock race test would be a coin flip that passes for the wrong reason far more
    /// often than it catches the defect. The store still holds the real value and the commit
    /// still evaluates the real precondition — nothing about the production path is faked; only
    /// the *scheduling* is made repeatable.
    fn hide_next_read(&self, key: &str) {
        *self.hide_once.lock().expect("hide lock") = Some(key.as_bytes().to_vec());
    }
    /// Every `(key, value)` under a raw key prefix, ascending — the observation window onto
    /// the record classes the protocol writes.
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
        {
            let mut hide = self.hide_once.lock().expect("hide lock");
            if hide.as_deref() == Some(key) {
                *hide = None;
                return Ok(None);
            }
        }
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

/// Seed a `bucket:{name}` existence marker as **raw bytes before the store moves into
/// `Gateway::new`** — nothing on `main` creates bucket records (#511), so every
/// container-scoped leg would `404` otherwise.
fn seed_bucket(meta: &MemMeta, name: &str) {
    meta.data.lock().expect("meta lock").insert(
        format!("bucket:{name}").into_bytes(),
        Bytes::from(format!("{{\"name\":\"{name}\",\"created_millis\":0}}")),
    );
}

/// Start the gateway on an ephemeral loopback port, returning the address and the retained
/// store handle.
async fn start(chunk_size: usize) -> (SocketAddr, MemMeta, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let meta = MemMeta::default();
    seed_bucket(&meta, BUCKET);
    let gateway = Arc::new(
        Gateway::new(
            meta.clone(),
            FsChunkStore::open(dir.path()).expect("fs store"),
            MemCoordination::new(),
        )
        .with_chunk_size(chunk_size),
    );
    let config = S3Config::new(vec![GatewayCredentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    }]);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        S3Gateway::new(gateway, config)
            .serve(listener)
            .await
            .expect("serve");
    });
    (addr, meta, dir)
}

/// A real `aws-sdk-s3` client pointed at the loopback gateway.
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

/// SigV4 headers for a raw request with a query string and a known body.
fn signed_headers(
    method: &str,
    path: &str,
    query: &str,
    host: &str,
    body: &[u8],
) -> Vec<(String, String)> {
    let creds = GatewayCredentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    };
    // wall-clock exempt: SigV4 dates must be the REAL clock the gateway validates freshness
    // against — a test-controlled instant would be rejected as an expired signature, which is
    // the auth lifecycle's own source, not this test's (AGENTS.md § Review rubric, ADR-0009).
    #[allow(clippy::disallowed_methods)]
    let amz_date = format_amz_date(SystemTime::now());
    let signed = sign(
        method, path, query, host, &amz_date, body, &creds, REGION, "s3",
    );
    vec![
        ("authorization".to_string(), signed.authorization),
        ("x-amz-date".to_string(), signed.amz_date),
        ("x-amz-content-sha256".to_string(), signed.content_sha256),
    ]
}

/// Send a raw signed request and return `(status, body)`. This is how the ill-formed and
/// percent-encoded forms are driven — an SDK cannot spell `?part%4Eumber=1`.
async fn raw(
    addr: SocketAddr,
    method: &str,
    path: &str,
    query: &str,
    body: &[u8],
) -> (u16, String) {
    raw_with(addr, method, path, query, body, &[]).await
}

/// [`raw`] with `extra` request headers appended — for the forms whose *header* is the point
/// (an `x-amz-copy-source` that turns an UploadPart into an UploadPartCopy).
async fn raw_with(
    addr: SocketAddr,
    method: &str,
    path: &str,
    query: &str,
    body: &[u8],
    extra: &[(&str, &str)],
) -> (u16, String) {
    let host = addr.to_string();
    let mut headers = signed_headers(method, path, query, &host, body);
    headers.extend(
        extra
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string())),
    );
    let target = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };
    let mut request = format!("{method} {target} HTTP/1.1\r\nhost: {host}\r\n");
    for (name, value) in &headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str(&format!("content-length: {}\r\n", body.len()));
    request.push_str("connection: close\r\n\r\n");

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write head");
    // An over-cap body is refused AS IT IS READ, so the server may close the connection
    // before the whole body is written — a broken pipe here is the refusal working, and the
    // response is still readable.
    let _ = stream.write_all(body).await;
    let _ = stream.flush().await;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    let split = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("header terminator");
    let head = String::from_utf8_lossy(&buf[..split]).into_owned();
    let status: u16 = head
        .lines()
        .next()
        .expect("status line")
        .split_whitespace()
        .nth(1)
        .expect("status code")
        .parse()
        .expect("numeric status");
    let raw_body = &buf[split + 4..];
    let chunked = head.lines().any(|l| {
        l.to_ascii_lowercase().starts_with("transfer-encoding:")
            && l.to_ascii_lowercase().contains("chunked")
    });
    let text = if chunked {
        String::from_utf8_lossy(&dechunk(raw_body)).into_owned()
    } else {
        String::from_utf8_lossy(raw_body).into_owned()
    };
    (status, text)
}

fn dechunk(mut raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(line_end) = raw.windows(2).position(|w| w == b"\r\n") {
        let size = usize::from_str_radix(String::from_utf8_lossy(&raw[..line_end]).trim(), 16)
            .unwrap_or(0);
        raw = &raw[line_end + 2..];
        if size == 0 || size > raw.len() {
            break;
        }
        out.extend_from_slice(&raw[..size]);
        raw = &raw[size + 2..];
    }
    out
}

/// The text between `<{tag}>` and `</{tag}>`.
fn tag<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = body.find(&open)? + open.len();
    let end = body[start..].find(&close)? + start;
    Some(&body[start..end])
}

/// The S3 error code an XML error body names.
fn err_code(body: &str) -> String {
    tag(body, "Code").unwrap_or("<no Code>").to_string()
}

/// Lowercase-hex SHA-256.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The **independently computed** oracle for the published multipart ETag:
/// `lowercase_hex( SHA-256( d_1 ‖ d_2 ‖ … ‖ d_N ) ) + "-" + N`, where `d_i` is the raw 32
/// binary digest bytes of the *i*-th part in ascending part-number order over exactly the
/// parts the client named.
///
/// Computed here from the part **bodies**, so the assertion is independent of the
/// implementation's choice — a "SHA-256-based pure function" admits many mutually
/// incompatible spellings (hex text instead of raw bytes, separators, part numbers mixed in),
/// and each would pass a self-referential oracle. Never MD5 (ADR-0047 closed the basis).
fn expected_multipart_etag(bodies_in_part_order: &[&[u8]]) -> String {
    let mut outer = Sha256::new();
    for body in bodies_in_part_order {
        outer.update(Sha256::digest(body));
    }
    format!("{}-{}", hex(&outer.finalize()), bodies_in_part_order.len())
}

/// A deterministic body of `len` bytes, distinguishable per part.
fn body(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| seed.wrapping_add((i % 251) as u8))
        .collect()
}

// ---------------------------------------------------------------------------------------
// Leg A — verb semantics, routing safety, and the verb × state table
// ---------------------------------------------------------------------------------------

/// **A(1).** `create` → `upload_part` × N (submitted **out of order**, with a non-final part
/// that is **not** a whole multiple of the chunk size) → `complete` succeeds, and a GET
/// returns the object **byte-identical** to the parts concatenated in part-number order.
///
/// The out-of-order submission and the 5 MiB + 7 B part are the discriminators: an assembler
/// that concatenated in *arrival* order, or that assumed chunk alignment at a part boundary,
/// produces a different object and fails the byte-identity assertion rather than an
/// "an error was returned" oracle it could never trip.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multipart_round_trip_is_byte_identical_and_etag_is_the_settled_composition() {
    // A chunk size that does NOT divide the part sizes, so part boundaries and chunk
    // boundaries deliberately disagree.
    let (addr, _meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "assembled/object.bin";

    let part1 = body(1, MIN_PART + 7); // non-final, NOT a whole multiple of the chunk size
    let part2 = body(2, MIN_PART);
    let part3 = body(3, 1024); // final part may be below the 5 MiB minimum

    let created = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create_multipart_upload");
    let upload_id = created.upload_id().expect("upload id").to_string();

    // Submitted OUT OF ORDER — 3, 1, 2 — so arrival order can never be mistaken for part
    // order.
    let mut etags: BTreeMap<i32, String> = BTreeMap::new();
    for (number, part) in [(3, &part3), (1, &part1), (2, &part2)] {
        let uploaded = s3
            .upload_part()
            .bucket(BUCKET)
            .key(key)
            .upload_id(&upload_id)
            .part_number(number)
            .body(ByteStream::from(part.clone()))
            .send()
            .await
            .unwrap_or_else(|e| panic!("upload_part {number}: {e:?}"));
        let etag = uploaded
            .e_tag()
            .expect("part etag")
            .trim_matches('"')
            .to_string();
        // A part's ETag is that part's lowercase-hex SHA-256 — computed here from the body.
        assert_eq!(
            etag,
            hex(&Sha256::digest(part)),
            "part {number}'s ETag must be its own lowercase-hex SHA-256 (ADR-0047; never MD5)"
        );
        etags.insert(number, etag);
    }

    // `list_parts` reflects the staged parts.
    let listed = s3
        .list_parts()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("list_parts");
    let numbers: Vec<i32> = listed
        .parts()
        .iter()
        .filter_map(|p| p.part_number())
        .collect();
    assert_eq!(
        numbers,
        vec![1, 2, 3],
        "list_parts is ascending by part number"
    );
    assert_eq!(
        listed.parts()[0].size(),
        Some(part1.len() as i64),
        "list_parts reports each part's true size"
    );

    let completed = CompletedMultipartUpload::builder()
        .set_parts(Some(
            etags
                .iter()
                .map(|(number, etag)| {
                    CompletedPart::builder()
                        .part_number(*number)
                        .e_tag(format!("\"{etag}\""))
                        .build()
                })
                .collect(),
        ))
        .build();
    let published = s3
        .complete_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await
        .expect("complete_multipart_upload");

    assert_eq!(
        published.e_tag().expect("published etag").trim_matches('"'),
        expected_multipart_etag(&[&part1, &part2, &part3]),
        "the published multipart ETag must be the settled composition — SHA-256 over the raw \
         32 binary digest bytes of each part in ascending part-number order, then `-N`"
    );

    // The object is byte-identical to the parts concatenated in PART-NUMBER order.
    let got = s3
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("get_object");
    let bytes = got.body.collect().await.expect("collect body").into_bytes();
    let mut expected = Vec::new();
    expected.extend_from_slice(&part1);
    expected.extend_from_slice(&part2);
    expected.extend_from_slice(&part3);
    assert_eq!(
        bytes.as_ref(),
        expected.as_slice(),
        "the published object must be the parts concatenated in PART-NUMBER order, byte for byte"
    );
}

/// **A(2b).** A multipart form this floor does not implement, or one that names two
/// operations at once, is **refused** — never served by an adjacent verb that mishandles it.
///
/// Three forms, each of which was answered *successfully* by an adjacent code path:
///
///  * `UploadPartCopy` (`x-amz-copy-source` on an UploadPart form) staged a **zero-byte part**
///    and answered `200` with the SHA-256 of nothing, so a Complete naming that part published
///    the destination object EMPTY — an out-of-scope verb turned into silent data loss;
///  * `GET /bucket?uploads&uploadId=…` names one specific upload *and* the whole listing, and
///    was answered with the listing;
///  * a `<Part>` naming its `<PartNumber>` or `<ETag>` twice, or an `<ETag>` with unbalanced
///    quotes, was accepted last-value-wins — an ambiguous document deciding which staged bytes
///    become the object and which are retired.
///
/// Each refusal is asserted with its exact status **and** S3 code, and the session is shown to
/// be untouched afterwards: a refused request stages nothing and publishes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn out_of_scope_and_ambiguous_multipart_forms_are_refused_not_mishandled() {
    let (addr, _meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "refused.bin";
    let path = format!("/{BUCKET}/{key}");
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

    // --- UploadPartCopy is refused, and stages nothing --------------------------------------
    let (status, response) = raw_with(
        addr,
        "PUT",
        &path,
        &format!("partNumber=1&uploadId={upload_id}"),
        b"",
        &[("x-amz-copy-source", "/other-bucket/other-key")],
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (501, "NotImplemented"),
        "UploadPartCopy is out of scope, and out of scope means REFUSED — serving it as a \
         0-byte UploadPart publishes an empty object where a copy was expected: {response}"
    );
    let listed = s3
        .list_parts()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("list_parts");
    assert!(
        listed.parts().is_empty(),
        "a refused UploadPartCopy must stage NO part; it staged {:?}",
        listed.parts()
    );

    // --- `?uploads` combines with no other marker -------------------------------------------
    let (status, response) = raw(
        addr,
        "GET",
        &format!("/{BUCKET}"),
        &format!("uploads&uploadId={upload_id}"),
        b"",
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (400, "InvalidArgument"),
        "`?uploads&uploadId=…` names two operations at once and must be refused, not answered \
         with a listing: {response}"
    );

    // --- an ambiguous or ill-formed Complete document publishes nothing ---------------------
    let part = body(7, MIN_PART);
    let etag = s3
        .upload_part()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(part.clone()))
        .send()
        .await
        .expect("upload_part")
        .e_tag()
        .expect("etag")
        .trim_matches('"')
        .to_string();
    for (document, what) in [
        (
            format!(
                "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber>\
                 <PartNumber>2</PartNumber><ETag>{etag}</ETag></Part></CompleteMultipartUpload>"
            ),
            "a repeated <PartNumber> names two different parts",
        ),
        (
            format!(
                "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>{etag}</ETag>\
                 <ETag>{etag}</ETag></Part></CompleteMultipartUpload>"
            ),
            "a repeated <ETag> names two different bodies",
        ),
        (
            format!(
                "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber>\
                 <ETag>&quot;{etag}</ETag></Part></CompleteMultipartUpload>"
            ),
            "an entity tag with unbalanced quotes is not an entity tag",
        ),
        (
            format!(
                "<CompleteMultipartUpload><Part><PartNumber>+1</PartNumber>\
                 <ETag>{etag}</ETag></Part></CompleteMultipartUpload>"
            ),
            "a signed part number is not the 1*DIGIT grammar",
        ),
    ] {
        let (status, response) = raw(
            addr,
            "POST",
            &path,
            &format!("uploadId={upload_id}"),
            document.as_bytes(),
        )
        .await;
        assert_eq!(
            (status, err_code(&response).as_str()),
            (400, "MalformedXML"),
            "{what}: the document must be refused whole, never resolved last-value-wins \
             ({response})"
        );
    }
    // The session survived every refusal, and the well-formed assembly still publishes.
    let published = s3
        .complete_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(
            aws_sdk_s3::types::CompletedMultipartUpload::builder()
                .parts(
                    aws_sdk_s3::types::CompletedPart::builder()
                        .part_number(1)
                        .e_tag(&etag)
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .expect("a refused Complete must leave the session usable");
    assert_eq!(
        published.e_tag().map(|t| t.trim_matches('"').to_string()),
        Some(expected_multipart_etag(&[&part])),
        "the published ETag is the settled composition over the ONE named part"
    );
}

/// **A(2c).** `ListMultipartUploads` pages: `max-uploads` bounds the page, truncation is
/// **computed**, and the `(key-marker, upload-id-marker)` pair resumes exactly after the row
/// the previous page ended on.
///
/// A listing that ignored them answered a one-row request with the whole admitted population
/// and reported `IsTruncated=false` however many rows it held — the count-based assertion that
/// passes while the property fails.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_upload_listing_is_paged_and_its_truncation_is_computed() {
    let (addr, _meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let mut ids = Vec::new();
    for key in ["page-a.bin", "page-b.bin", "page-c.bin"] {
        ids.push(
            s3.create_multipart_upload()
                .bucket(BUCKET)
                .key(key)
                .send()
                .await
                .expect("create")
                .upload_id()
                .expect("id")
                .to_string(),
        );
    }

    let (status, first) = raw(
        addr,
        "GET",
        &format!("/{BUCKET}"),
        "uploads&max-uploads=1",
        b"",
    )
    .await;
    assert_eq!(status, 200, "the listing must answer 200: {first}");
    assert_eq!(
        first.matches("<Upload>").count(),
        1,
        "`max-uploads=1` must return ONE row, not the whole population: {first}"
    );
    assert_eq!(
        tag(&first, "IsTruncated"),
        Some("true"),
        "truncation must be computed from the rows that remain, never hardcoded: {first}"
    );
    assert_eq!(
        tag(&first, "Key"),
        Some("page-a.bin"),
        "the first page is the first row in `(key, upload-id)` order: {first}"
    );
    let next_key = tag(&first, "NextKeyMarker").expect("a truncated page names its resume key");
    let next_id =
        tag(&first, "NextUploadIdMarker").expect("a truncated page names its resume upload id");

    let (status, second) = raw(
        addr,
        "GET",
        &format!("/{BUCKET}"),
        &format!("uploads&max-uploads=1&key-marker={next_key}&upload-id-marker={next_id}"),
        b"",
    )
    .await;
    assert_eq!(status, 200, "the continuation must answer 200: {second}");
    assert_eq!(
        tag(&second, "Key"),
        Some("page-b.bin"),
        "the continuation starts strictly AFTER the previous page's last row: {second}"
    );

    let (status, whole) = raw(addr, "GET", &format!("/{BUCKET}"), "uploads", b"").await;
    assert_eq!(status, 200, "an unpaged listing must answer 200: {whole}");
    assert_eq!(
        whole.matches("<Upload>").count(),
        3,
        "with no `max-uploads` every open session is listed: {whole}"
    );
    assert_eq!(
        tag(&whole, "IsTruncated"),
        Some("false"),
        "a complete listing is not truncated: {whole}"
    );
    assert_eq!(ids.len(), 3, "three sessions were opened");
}

/// **A(2).** Routing safety: each ill-formed multipart form answers **400 `InvalidArgument`**
/// — never a `2xx` that overwrites or deletes the object — and a multipart form carrying a
/// still-denylisted subresource answers **501 `NotImplemented`** naming it.
///
/// The percent-encoded `?part%4Eumber=1` is the sharpest single red: on `origin/main` the
/// denylist matches **raw** keys while SigV4 canonicalisation decodes-then-re-encodes, so the
/// form reaches the plain PUT arm and answers **200, overwriting the object**.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ill_formed_multipart_forms_are_refused_and_never_touch_the_object() {
    let (addr, _meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "guarded.bin";
    let path = format!("/{BUCKET}/{key}");
    let original = b"the original object bytes".to_vec();

    s3.put_object()
        .bucket(BUCKET)
        .key(key)
        .body(ByteStream::from(original.clone()))
        .send()
        .await
        .expect("seed the object");

    let evil = b"OVERWRITTEN".to_vec();
    let id = "0".repeat(32);
    for (method, query, body_bytes, what) in [
        (
            "PUT",
            "partNumber=1".to_string(),
            evil.clone(),
            "partNumber with no uploadId",
        ),
        (
            "PUT",
            format!("partNumber=notanumber&uploadId={id}"),
            evil.clone(),
            "non-numeric partNumber",
        ),
        (
            "PUT",
            format!("uploadId={id}"),
            evil.clone(),
            "uploadId with no partNumber",
        ),
        ("PUT", "uploads".to_string(), evil.clone(), "PUT ?uploads"),
        (
            "DELETE",
            "uploadId=U".to_string(),
            Vec::new(),
            "malformed uploadId on DELETE",
        ),
        // THE percent-encoding fence: `part%4Eumber` decodes to `partNumber`.
        (
            "PUT",
            "part%4Eumber=1".to_string(),
            evil.clone(),
            "percent-encoded partNumber",
        ),
    ] {
        let (status, response) = raw(addr, method, &path, &query, &body_bytes).await;
        assert_eq!(
            (status, err_code(&response).as_str()),
            (400, "InvalidArgument"),
            "`{method} {path}?{query}` ({what}) must be refused 400 InvalidArgument, never \
             served by a plain object verb"
        );
    }

    // The object is untouched by every one of them — the property a 2xx would have destroyed.
    let got = s3
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("the object still exists");
    let bytes = got.body.collect().await.expect("collect").into_bytes();
    assert_eq!(
        bytes.as_ref(),
        original.as_slice(),
        "no refused multipart form may have overwritten or deleted the object"
    );

    // A multipart marker is NOT a skeleton key: another denylisted subresource in the same
    // query still refuses 501, naming it — in its percent-encoded spelling too.
    let (status, response) = raw(
        addr,
        "PUT",
        &path,
        &format!("partNumber=1&uploadId={id}&t%61gging=1"),
        &evil,
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (501, "NotImplemented"),
        "a multipart form carrying a denylisted subresource must refuse 501"
    );
    assert!(
        response.contains("tagging"),
        "the 501 must name the offending subresource; got {response}"
    );

    let (status, response) = raw(addr, "GET", &format!("/{BUCKET}"), "uploads&%61cl", b"").await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (501, "NotImplemented"),
        "`GET /bucket?uploads&%61cl` must refuse 501 rather than list"
    );
    assert!(
        response.contains("acl"),
        "the 501 must name `acl`; got {response}"
    );
}

/// **A(3).** The verb × state table, every cell reachable without the reaper, asserted with
/// an **exact status + S3 code**.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verb_by_state_answers_are_exact() {
    let (addr, meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "state-table.bin";
    let path = format!("/{BUCKET}/{key}");

    // --- after Abort: UploadPart / Complete / ListParts → 404 NoSuchUpload ---------------
    let created = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("create");
    let upload_id = created.upload_id().expect("upload id").to_string();
    let part = body(9, MIN_PART);
    let etag = s3
        .upload_part()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(part.clone()))
        .send()
        .await
        .expect("upload_part")
        .e_tag()
        .expect("etag")
        .trim_matches('"')
        .to_string();

    // Hold a SECOND part mid-stream, so the session's owned `sidx:` range is non-empty and
    // the terminal delete is blocked on exactly the gate the protocol specifies — which is
    // what makes the `Aborting` cell below observable rather than a race against the drain.
    let host = addr.to_string();
    let creds = GatewayCredentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    };
    let held_query = format!("partNumber=2&uploadId={upload_id}");
    // wall-clock exempt: SigV4 dates must be the REAL clock the gateway validates freshness
    // against — a test-controlled instant would be rejected as an expired signature, which is
    // the auth lifecycle's own source, not this test's (AGENTS.md § Review rubric, ADR-0009).
    #[allow(clippy::disallowed_methods)]
    let amz_date = format_amz_date(SystemTime::now());
    let held_signed = sign_with_payload_hash(
        "PUT",
        &path,
        &held_query,
        &host,
        &amz_date,
        "UNSIGNED-PAYLOAD",
        &creds,
        REGION,
        "s3",
    );
    let held_first = body(31, 128 * 1024);
    let mut held = TcpStream::connect(addr).await.expect("connect");
    let mut head = format!("PUT {path}?{held_query} HTTP/1.1\r\nhost: {host}\r\n");
    head.push_str(&format!("authorization: {}\r\n", held_signed.authorization));
    head.push_str(&format!("x-amz-date: {}\r\n", held_signed.amz_date));
    head.push_str("x-amz-content-sha256: UNSIGNED-PAYLOAD\r\n");
    head.push_str(&format!("content-length: {}\r\n", held_first.len() * 2));
    head.push_str("connection: close\r\n\r\n");
    held.write_all(head.as_bytes()).await.expect("held head");
    held.write_all(&held_first).await.expect("held first half");
    held.flush().await.expect("held flush");

    // **Wait for the held part to register its first owned entry before aborting.** Writing the
    // head and half the body does not mean a `sidx:` entry exists yet: the first staging batch
    // commits asynchronously. Aborting first raced the drain the Abort spawns — with the range
    // still empty, teardown deletes the `mpu:` record immediately and the *second* Abort below
    // answers `404 NoSuchUpload`, so the assertion tested a scheduling accident rather than the
    // `Aborting` cell. Polling for the entry makes the teardown gate genuinely closed, which is
    // what the cell is about (the sibling lifecycle test uses the same pattern).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while meta.raw_scan(&format!("sidx:{upload_id}:")).is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the held part never registered an owned `sidx:` entry, so the teardown gate this \
             cell depends on was never closed"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let (status, _) = raw(addr, "DELETE", &path, &format!("uploadId={upload_id}"), b"").await;
    assert_eq!(status, 204, "Abort of an Open session answers 204 at once");
    // A second Abort answers the cell the session is **actually** in, and both cells are
    // normative: `Aborting` is idempotent (`204`), and once the teardown has finished there is
    // no session left to abort (`404 NoSuchUpload`, real S3's answer for an unknown id).
    //
    // Which one a test sees is not something it can pin by waiting: Abort answers from its
    // fence commit and spawns the teardown drain *before* the response is written (0016 F9),
    // and in-process that drain can finish inside the round trip. So the assertion is on the
    // agreement between the answer and the store, read after the answer — which is a stronger
    // oracle than either cell alone and has no window: a `204` is the idempotent cell, while a
    // `404` is legitimate ONLY once the session record is gone. Nothing recreates one, so a
    // record still present after a `404` means a client was told its abort failed while the
    // session it names is right there — and any other status is neither cell.
    let (status, response) =
        raw(addr, "DELETE", &path, &format!("uploadId={upload_id}"), b"").await;
    let session_gone = meta.raw_scan(&format!("mpu:{upload_id}")).is_empty();
    match status {
        204 => {}
        404 => {
            assert_eq!(
                err_code(&response),
                "NoSuchUpload",
                "the terminal cell is NoSuchUpload: {response}"
            );
            assert!(
                session_gone,
                "a second Abort answered 404 while the session record is still present — the \
                 `Aborting` cell is idempotent (204), so this is a client told its abort failed \
                 about a session that is right there"
            );
        }
        other => panic!(
            "a second Abort is 204 while `Aborting` or 404 NoSuchUpload once torn down, never \
             {other}: {response}"
        ),
    }
    drop(held);

    let (status, response) = raw(
        addr,
        "PUT",
        &path,
        &format!("partNumber=2&uploadId={upload_id}"),
        &body(4, 16),
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (404, "NoSuchUpload"),
        "UploadPart after Abort must be 404 NoSuchUpload"
    );
    let (status, response) = raw(addr, "GET", &path, &format!("uploadId={upload_id}"), b"").await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (404, "NoSuchUpload"),
        "ListParts after Abort must be 404 NoSuchUpload"
    );
    let complete_body = format!(
        "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;{etag}&quot;\
         </ETag></Part></CompleteMultipartUpload>"
    );
    let (status, response) = raw(
        addr,
        "POST",
        &path,
        &format!("uploadId={upload_id}"),
        complete_body.as_bytes(),
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (404, "NoSuchUpload"),
        "Complete after Abort must be 404 NoSuchUpload"
    );

    // --- the tombstone: an identical retry is 200 + the recorded ETag, a DIFFERENT
    //     assembly reusing the id is 404 NoSuchUpload (never a silent wrong answer) -------
    let key2 = "tombstone.bin";
    let path2 = format!("/{BUCKET}/{key2}");
    let created = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key(key2)
        .send()
        .await
        .expect("create");
    let id2 = created.upload_id().expect("upload id").to_string();
    let a = body(11, MIN_PART);
    let b = body(12, 32);
    let mut part_etags = Vec::new();
    for (number, bytes) in [(1u32, &a), (2u32, &b)] {
        part_etags.push(
            s3.upload_part()
                .bucket(BUCKET)
                .key(key2)
                .upload_id(&id2)
                .part_number(number as i32)
                .body(ByteStream::from(bytes.clone()))
                .send()
                .await
                .expect("upload_part")
                .e_tag()
                .expect("etag")
                .trim_matches('"')
                .to_string(),
        );
    }
    let both = format!(
        "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;{}&quot;</ETag>\
         </Part><Part><PartNumber>2</PartNumber><ETag>&quot;{}&quot;</ETag></Part>\
         </CompleteMultipartUpload>",
        part_etags[0], part_etags[1]
    );
    let (status, first) = raw(
        addr,
        "POST",
        &path2,
        &format!("uploadId={id2}"),
        both.as_bytes(),
    )
    .await;
    assert_eq!(
        status, 200,
        "Complete of an Open session publishes: {first}"
    );
    let published = tag(&first, "ETag")
        .expect("published etag")
        .replace("&quot;", "");
    assert_eq!(
        published,
        expected_multipart_etag(&[&a, &b]),
        "the published ETag is the settled composition"
    );

    let (status, retried) = raw(
        addr,
        "POST",
        &path2,
        &format!("uploadId={id2}"),
        both.as_bytes(),
    )
    .await;
    assert_eq!(
        status, 200,
        "an IDENTICAL Complete retry inside the tombstone window answers 200: {retried}"
    );
    assert_eq!(
        tag(&retried, "ETag").expect("etag").replace("&quot;", ""),
        published,
        "the retry answers the RECORDED ETag, not a recomputed one"
    );

    // A different part list against the same, consumed id: 404, never "your assembly
    // succeeded" over an object that holds the earlier one.
    let only_first = format!(
        "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;{}&quot;</ETag>\
         </Part></CompleteMultipartUpload>",
        part_etags[0]
    );
    let (status, response) = raw(
        addr,
        "POST",
        &path2,
        &format!("uploadId={id2}"),
        only_first.as_bytes(),
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (404, "NoSuchUpload"),
        "a Complete reusing the upload id with a DIFFERENT part list must be 404 NoSuchUpload \
         (the complete_fingerprint rule) — never a silent wrong answer"
    );

    // --- ListMultipartUploads lists Open sessions and NOT terminal ones -----------------
    let open = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key("still-open.bin")
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    let (status, listing) = raw(addr, "GET", &format!("/{BUCKET}"), "uploads", b"").await;
    assert_eq!(status, 200, "GET /bucket?uploads lists: {listing}");
    assert!(
        listing.contains(&open),
        "an Open session must be listed; got {listing}"
    );
    assert!(
        !listing.contains(&id2),
        "a Completed session must NOT be listed; got {listing}"
    );
    assert!(
        !listing.contains(&upload_id),
        "an Aborting session must NOT be listed; got {listing}"
    );
}

/// **A(4).** Error forms, each asserted as an exact status **and** code.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn multipart_error_forms_are_exact() {
    let (addr, _meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "errors.bin";
    let path = format!("/{BUCKET}/{key}");

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

    // A part number outside 1..=10000 → 400 InvalidArgument.
    for number in ["0", "10001"] {
        let (status, response) = raw(
            addr,
            "PUT",
            &path,
            &format!("partNumber={number}&uploadId={upload_id}"),
            &body(1, 16),
        )
        .await;
        assert_eq!(
            (status, err_code(&response).as_str()),
            (400, "InvalidArgument"),
            "part number {number} is outside 1..=10000"
        );
    }

    let small = body(5, 1024); // a NON-final part below 5 MiB
    let tail = body(6, 64);
    let mut etags = Vec::new();
    for (number, bytes) in [(1u32, &small), (2u32, &tail)] {
        etags.push(
            s3.upload_part()
                .bucket(BUCKET)
                .key(key)
                .upload_id(&upload_id)
                .part_number(number as i32)
                .body(ByteStream::from(bytes.clone()))
                .send()
                .await
                .expect("upload_part")
                .e_tag()
                .expect("etag")
                .trim_matches('"')
                .to_string(),
        );
    }

    // A named part that does not exist → 400 InvalidPart, and nothing is published.
    let ghost = format!(
        "<CompleteMultipartUpload><Part><PartNumber>7</PartNumber><ETag>&quot;{}&quot;</ETag>\
         </Part></CompleteMultipartUpload>",
        etags[0]
    );
    let (status, response) = raw(
        addr,
        "POST",
        &path,
        &format!("uploadId={upload_id}"),
        ghost.as_bytes(),
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (400, "InvalidPart"),
        "a named part that was never staged is InvalidPart"
    );
    assert!(
        s3.get_object()
            .bucket(BUCKET)
            .key(key)
            .send()
            .await
            .is_err(),
        "a rejected Complete must publish NOTHING"
    );

    // Descending part numbers → 400 InvalidPartOrder.
    let descending = format!(
        "<CompleteMultipartUpload><Part><PartNumber>2</PartNumber><ETag>&quot;{}&quot;</ETag>\
         </Part><Part><PartNumber>1</PartNumber><ETag>&quot;{}&quot;</ETag></Part>\
         </CompleteMultipartUpload>",
        etags[1], etags[0]
    );
    let (status, response) = raw(
        addr,
        "POST",
        &path,
        &format!("uploadId={upload_id}"),
        descending.as_bytes(),
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (400, "InvalidPartOrder"),
        "a descending named-part list is InvalidPartOrder"
    );

    // A NON-final part below 5 MiB → 400 EntityTooSmall.
    let too_small = format!(
        "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;{}&quot;</ETag>\
         </Part><Part><PartNumber>2</PartNumber><ETag>&quot;{}&quot;</ETag></Part>\
         </CompleteMultipartUpload>",
        etags[0], etags[1]
    );
    let (status, response) = raw(
        addr,
        "POST",
        &path,
        &format!("uploadId={upload_id}"),
        too_small.as_bytes(),
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (400, "EntityTooSmall"),
        "a non-final part below 5 MiB is EntityTooSmall"
    );

    // A rejected Complete leaves the session USABLE: the Abort below must succeed, not 409.
    let (status, _) = raw(addr, "DELETE", &path, &format!("uploadId={upload_id}"), b"").await;
    assert_eq!(
        status, 204,
        "a rejected Complete must release the fence, so Abort still succeeds — a client typo \
         may not wedge an upload"
    );

    // A Complete body that is not well-formed XML, and one over the size cap → MalformedXML.
    let id2 = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key("malformed.bin")
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    let path2 = format!("/{BUCKET}/malformed.bin");
    for (bytes, what) in [
        (
            b"<CompleteMultipartUpload><Part>".to_vec(),
            "truncated document",
        ),
        (vec![b'x'; 8 * 1024 * 1024], "over the body cap"),
    ] {
        let (status, response) =
            raw(addr, "POST", &path2, &format!("uploadId={id2}"), &bytes).await;
        assert_eq!(
            (status, err_code(&response).as_str()),
            (400, "MalformedXML"),
            "a Complete body that is {what} must be MalformedXML"
        );
    }

    // `GET /bucket?uploads` on an ABSENT bucket → 404 NoSuchBucket.
    let (status, response) = raw(addr, "GET", "/no-such-bucket", "uploads", b"").await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (404, "NoSuchBucket"),
        "ListMultipartUploads on an absent bucket is NoSuchBucket"
    );

    // A well-formed but UNKNOWN upload id → 404 NoSuchUpload (not 400: the form is legal).
    let (status, response) = raw(
        addr,
        "GET",
        &path,
        &format!("uploadId={}", "a".repeat(32)),
        b"",
    )
    .await;
    assert_eq!(
        (status, err_code(&response).as_str()),
        (404, "NoSuchUpload"),
        "a well-formed but unknown upload id is NoSuchUpload — the form is legal, the upload \
         is not there"
    );
}

/// **A(5).** `list_parts` pagination is **genuinely computed**: `IsTruncated` is true exactly
/// when a further page remains, and the continuation returns the rest exactly once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn list_parts_pagination_is_genuinely_computed() {
    let (addr, _meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "paged.bin";
    let path = format!("/{BUCKET}/{key}");

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
    for number in 1..=5u32 {
        s3.upload_part()
            .bucket(BUCKET)
            .key(key)
            .upload_id(&upload_id)
            .part_number(number as i32)
            .body(ByteStream::from(body(number as u8, 128)))
            .send()
            .await
            .expect("upload_part");
    }

    let (status, page1) = raw(
        addr,
        "GET",
        &path,
        &format!("uploadId={upload_id}&max-parts=2"),
        b"",
    )
    .await;
    assert_eq!(status, 200, "list_parts page 1: {page1}");
    assert_eq!(
        tag(&page1, "IsTruncated"),
        Some("true"),
        "with 5 parts and max-parts=2 the first page MUST report IsTruncated=true"
    );
    assert_eq!(
        tag(&page1, "NextPartNumberMarker"),
        Some("2"),
        "the continuation marker is the last part number returned"
    );
    assert_eq!(
        page1.matches("<Part>").count(),
        2,
        "the page holds exactly max-parts entries"
    );

    let (status, page3) = raw(
        addr,
        "GET",
        &path,
        &format!("uploadId={upload_id}&max-parts=2&part-number-marker=4"),
        b"",
    )
    .await;
    assert_eq!(status, 200, "list_parts page 3: {page3}");
    assert_eq!(
        tag(&page3, "IsTruncated"),
        Some("false"),
        "the LAST page must report IsTruncated=false — a hard-coded value fails one of these two"
    );
    assert_eq!(
        page3.matches("<Part>").count(),
        1,
        "the last page holds the single remaining part"
    );
}

// ---------------------------------------------------------------------------------------
// Leg B — publication, accounting and the fence
// ---------------------------------------------------------------------------------------

/// **B(i).** The staging class is the **disjoint owned one, observed while the part is IN
/// FLIGHT** — not merely absent afterwards.
///
/// A post-hoc scan alone proves nothing here: an implementation that staged under `pending:`
/// and deleted it during the commit would pass it, while re-entering the global `pending:`
/// scans and the cross-clock expiry semantics for the whole life of the upload — exactly what
/// the disjoint class exists to prevent.
///
/// This is also the binding form of "Complete does not sit on the 30 s lease TTL": if no
/// lease-bearing entry survives a part commit, lease expiry cannot affect Complete by
/// construction. Deliberately **not** a >30 s sleep.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn staged_chunks_live_under_the_disjoint_owned_class_while_in_flight() {
    let (addr, meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "inflight.bin";

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

    // A raw socket the test releases on command: the head declares the full
    // `content-length` and only HALF the body is written, so the checkpoint below is taken
    // with the part genuinely mid-stream rather than after the fact.
    let host = addr.to_string();
    let path = format!("/{BUCKET}/{key}");
    let query = format!("partNumber=1&uploadId={upload_id}");
    let first_half = body(7, 128 * 1024);
    let second_half = body(8, 128 * 1024);
    let total = first_half.len() + second_half.len();
    let creds = GatewayCredentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    };
    // wall-clock exempt: SigV4 dates must be the REAL clock the gateway validates freshness
    // against — a test-controlled instant would be rejected as an expired signature, which is
    // the auth lifecycle's own source, not this test's (AGENTS.md § Review rubric, ADR-0009).
    #[allow(clippy::disallowed_methods)]
    let amz_date = format_amz_date(SystemTime::now());
    // `UNSIGNED-PAYLOAD` so the request can be signed without the body being known — the
    // whole point is that the body is still arriving when the checkpoint is taken.
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
    head.push_str(&format!("content-length: {total}\r\n"));
    head.push_str("connection: close\r\n\r\n");

    let mut socket = TcpStream::connect(addr).await.expect("connect");
    socket.write_all(head.as_bytes()).await.expect("head");
    socket.write_all(&first_half).await.expect("first half");
    socket.flush().await.expect("flush");

    // Wait for at least one chunk of the in-flight part to be registered.
    let mut owned = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        owned = meta.raw_scan(&format!("sidx:{upload_id}:"));
        if !owned.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // THE CHECKPOINT, taken with the part in flight.
    assert!(
        !owned.is_empty(),
        "while a part is streaming, its chunks MUST be registered under the disjoint owned \
         class `sidx:<upload-id>:` — the class no global `pending:` scan enumerates"
    );
    assert!(
        meta.raw_scan("pending:").is_empty(),
        "an assembled write must register NOTHING under `pending:`: owned entries there would \
         re-enter the global pending scans and the cross-clock expiry semantics for the whole \
         life of the upload"
    );
    assert!(
        !meta.raw_scan(&format!("slot:{upload_id}:")).is_empty(),
        "an in-flight part holds an in-flight slot — the key space that IS the admission cap"
    );

    // Release the rest of the body and let the part commit.
    socket.write_all(&second_half).await.expect("second half");
    socket.flush().await.expect("flush tail");
    let mut response = Vec::new();
    socket.read_to_end(&mut response).await.expect("response");
    let head = String::from_utf8_lossy(&response).into_owned();
    assert!(
        head.starts_with("HTTP/1.1 200"),
        "the released part must commit: {head}"
    );

    // After the commit no lease-bearing entry survives for that part.
    assert!(
        meta.raw_scan(&format!("sidx:{upload_id}:")).is_empty(),
        "the part commit must hand every chunk from its owned staging entry to the `part:` \
         record in ONE batch, leaving no `sidx:` entry behind"
    );
    assert!(
        meta.raw_scan(&format!("slot:{upload_id}:")).is_empty(),
        "the part commit releases its slot with a keyed CAS in the same batch"
    );
    assert!(
        !meta.raw_scan(&format!("part:{upload_id}:")).is_empty(),
        "…and the `part:` record that now protects those bytes exists"
    );
    assert!(
        meta.raw_scan("pending:").is_empty(),
        "no lease-bearing entry survives a part commit, so lease expiry cannot affect Complete \
         by construction"
    );
}

/// **B(iv).** Admission is exact, and the decrement is **off the request path**.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn admission_bootstraps_counts_and_decrements_off_the_request_path() {
    let (addr, meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);

    assert!(
        meta.raw_get("mpuctl").is_none(),
        "a fresh store carries no admission record — it is bootstrapped by the FIRST Create, \
         with no migration step"
    );
    let first = s3
        .create_multipart_upload()
        .bucket(BUCKET)
        .key("a.bin")
        .send()
        .await
        .expect("create")
        .upload_id()
        .expect("id")
        .to_string();
    let control = meta
        .raw_get("mpuctl")
        .expect("the first Create bootstraps `mpuctl`");
    let control = String::from_utf8_lossy(&control).into_owned();
    assert!(
        control.contains("\"count\":1"),
        "the bootstrap writes count 1: {control}"
    );
    assert!(
        control.contains("max_sessions") && control.contains("profile"),
        "`mpuctl` stores the governing limit AND the budget profile that establishes it, in \
         ONE value — equal quotients can hide unequal footprints: {control}"
    );

    s3.create_multipart_upload()
        .bucket(BUCKET)
        .key("b.bin")
        .send()
        .await
        .expect("create");
    let control = String::from_utf8_lossy(&meta.raw_get("mpuctl").expect("mpuctl")).into_owned();
    assert!(
        control.contains("\"count\":2"),
        "two open sessions read count == 2: {control}"
    );

    // The Abort RESPONSE returns from the fence commit alone: the session reads `Aborting`
    // immediately and `count` is still 2 — teardown is NOT on the request path. Asserting
    // `count == 1` at response time would reject the conforming implementation and push
    // teardown back onto the HTTP path.
    let (status, _) = raw(
        addr,
        "DELETE",
        &format!("/{BUCKET}/a.bin"),
        &format!("uploadId={first}"),
        b"",
    )
    .await;
    assert_eq!(status, 204, "Abort answers from the fence commit");

    // `count` returns to 1 only after the bounded drain — polled to a deadline, never slept.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut control = String::new();
    while std::time::Instant::now() < deadline {
        control = String::from_utf8_lossy(&meta.raw_get("mpuctl").expect("mpuctl")).into_owned();
        if control.contains("\"count\":1") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        control.contains("\"count\":1"),
        "after the bounded drain the admission count returns to 1: {control}"
    );
    assert!(
        meta.raw_get(&format!("mpu:{first}")).is_none(),
        "the torn-down session's record is deleted LAST, by the terminal delete that carries \
         the exactly-once decrement"
    );
}

/// **B(ii)/(iii).** After Complete and its bounded drain: the session is a `Completed`
/// tombstone, the published parts' `part:`/`psum:` records are gone, **no `orphan:` record
/// exists for any published chunk**, and a Complete naming only a **subset** of the staged
/// parts orphan-marks exactly the unnamed ones' bytes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn publication_disposes_of_every_part_record_and_orphans_only_the_unnamed() {
    let (addr, meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "subset.bin";
    let path = format!("/{BUCKET}/{key}");

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

    // Stage parts 1, 2 and 3; Complete will name only 1 and 3 (S3 discards the rest), and
    // part 2 is uploaded TWICE — the two commonest client behaviours.
    let p1 = body(21, MIN_PART);
    let p2a = body(22, MIN_PART);
    let p2b = body(23, MIN_PART);
    let p3 = body(24, 512);
    let mut etags = BTreeMap::new();
    for (number, bytes) in [(1u32, &p1), (2u32, &p2a), (2u32, &p2b), (3u32, &p3)] {
        let etag = s3
            .upload_part()
            .bucket(BUCKET)
            .key(key)
            .upload_id(&upload_id)
            .part_number(number as i32)
            .body(ByteStream::from(bytes.clone()))
            .send()
            .await
            .expect("upload_part")
            .e_tag()
            .expect("etag")
            .trim_matches('"')
            .to_string();
        etags.insert(number, etag);
    }

    let named = format!(
        "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>&quot;{}&quot;</ETag>\
         </Part><Part><PartNumber>3</PartNumber><ETag>&quot;{}&quot;</ETag></Part>\
         </CompleteMultipartUpload>",
        etags[&1], etags[&3]
    );
    let (status, response) = raw(
        addr,
        "POST",
        &path,
        &format!("uploadId={upload_id}"),
        named.as_bytes(),
    )
    .await;
    assert_eq!(
        status, 200,
        "Complete naming a subset publishes: {response}"
    );
    assert_eq!(
        tag(&response, "ETag").expect("etag").replace("&quot;", ""),
        expected_multipart_etag(&[&p1, &p3]),
        "the ETag is composed over exactly the parts the client NAMED"
    );

    // The published object is parts 1 + 3.
    let got = s3
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("get");
    let bytes = got.body.collect().await.expect("collect").into_bytes();
    let mut expected = p1.clone();
    expected.extend_from_slice(&p3);
    assert_eq!(
        bytes.as_ref(),
        expected.as_slice(),
        "the object is the NAMED parts, in order — never every staged part"
    );

    // Poll the bounded drain to a deadline for the terminal condition.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if meta.raw_scan(&format!("part:{upload_id}:")).is_empty()
            && meta.raw_scan(&format!("psum:{upload_id}:")).is_empty()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        meta.raw_scan(&format!("part:{upload_id}:")).is_empty(),
        "after the drain no `part:` record survives — the published ones are disposed of by \
         `retire:records:` and the unnamed one by its own `retire:bytes:` obligation"
    );
    assert!(
        meta.raw_scan(&format!("psum:{upload_id}:")).is_empty(),
        "…and neither does any `psum:` summary"
    );

    let session = meta
        .raw_get(&format!("mpu:{upload_id}"))
        .expect("a Completed session survives as a tombstone (its deletion needs #625)");
    let session = String::from_utf8_lossy(&session).into_owned();
    assert!(
        session.contains("Completed"),
        "the session record is a `Completed` tombstone: {session}"
    );

    // **A published upload gives its staged-reference admission slot back.** The tombstone keeps
    // its `mpu:` record until #625's `W_tombstone` exit, but it owns no `part:`/`psum:`/`sidx:`
    // record any more, so it must stop being charged against the concurrency budget the
    // reconcile pass's memory sizes. An implementation that only released the slot with the
    // record would accept a few dozen LIFETIME uploads and then refuse every Create for ever.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut control = String::new();
    while std::time::Instant::now() < deadline {
        control = String::from_utf8_lossy(&meta.raw_get("mpuctl").expect("admission record"))
            .into_owned();
        if control.contains("\"staged\":0") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(
        control.contains("\"staged\":0"),
        "after the drain the published session no longer holds a staged-reference slot: {control}"
    );
    assert!(
        control.contains("\"count\":1"),
        "…while its tombstone still occupies an `mpu:` record, which is what the record-population \
         bound covers and what #625's `W_tombstone` exit releases: {control}"
    );

    // The published object's chunks carry NO `orphan:` record — `retire:records:` must never
    // orphan anything, because its bytes are live object content.
    let inode = meta
        .raw_scan("inode:")
        .into_iter()
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
        .find(|v| v.contains("Committed"))
        .expect("a committed inode");
    let orphans = meta.raw_scan("orphan:");
    for chunk_id in published_chunk_ids(&inode) {
        assert!(
            !orphans
                .iter()
                .any(|(k, _)| k.contains(&format!(":{chunk_id}:"))),
            "no `orphan:` record may exist for a PUBLISHED chunk ({chunk_id}); a
             `retire:records:` obligation that orphaned would be silent data loss"
        );
    }
    assert!(
        !orphans.is_empty(),
        "the unnamed staged part's bytes MUST be orphan-marked — evidenced for reclamation, \
         never merely unreferenced (an unevidenced fragment is retained forever, silently)"
    );
}

/// The chunk ids a committed inode's raw JSON names.
fn published_chunk_ids(inode_json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = inode_json;
    while let Some(at) = rest.find("\"id\":") {
        rest = &rest[at + 5..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if end > 0 {
            out.push(rest[..end].to_string());
        }
        rest = &rest[end..];
    }
    out
}

// ---------------------------------------------------------------------------------------
// Leg E — decision 7 also changes ordinary `PutObject`
// ---------------------------------------------------------------------------------------

/// **E(i)/(iii).** A single `PutObject` stays **flat** by chunk-size selection, and is
/// refused `400 EntityTooLarge` when it cannot fit the flat ceiling even at `chunk_size_max`.
///
/// The oracle is the **SIZING**, not a round-trip: a byte-identical round trip is already
/// green on the base for both shapes, so it would prove nothing here. What is false on the
/// base is that the published chunk count stops growing with the object.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn single_put_stays_flat_by_choosing_its_chunk_size() {
    // A deliberately tiny configured chunk size: at 512 B a 512 KiB object would need 1024
    // chunks — far past the flat ceiling — on the base, and the count is what this asserts.
    let (addr, meta, _dir) = start(512).await;
    let s3 = sdk_client(addr);
    let key = "large-single-put.bin";
    let payload = body(31, 512 * 1024);

    s3.put_object()
        .bucket(BUCKET)
        .key(key)
        .body(ByteStream::from(payload.clone()))
        .send()
        .await
        .expect("put_object");

    let inode = meta
        .raw_scan("inode:")
        .into_iter()
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
        .find(|v| v.contains("Committed"))
        .expect("a committed inode");
    let chunks = published_chunk_ids(&inode).len();
    // `MAX_MAP_CHUNKS` is ⌊(100 KB / 2) / 302 B⌋ = 165 — derived here from the same settled
    // arithmetic rather than imported, so this file stays base-compiling.
    const MAX_MAP_CHUNKS: usize = (100_000 / 2) / 302;
    assert!(
        chunks <= MAX_MAP_CHUNKS,
        "a single PUT must publish a FLAT map inside the flat ceiling ({MAX_MAP_CHUNKS} \
         chunks) by RAISING its chunk size — it published {chunks}, which grows with the \
         object (the base's behaviour)"
    );
    assert!(
        chunks > 1,
        "the object is still genuinely chunked ({chunks} chunks); a degenerate one-chunk map \
         would make the ceiling assertion vacuous"
    );

    // …and it still round-trips, so the sizing did not corrupt the object.
    let got = s3
        .get_object()
        .bucket(BUCKET)
        .key(key)
        .send()
        .await
        .expect("get");
    let bytes = got.body.collect().await.expect("collect").into_bytes();
    assert_eq!(bytes.as_ref(), payload.as_slice(), "the object round-trips");
}

/// **E(ii).** A **lengthless** `aws-chunked` PUT — which cannot evaluate the sizing formula
/// at all, because `x-amz-decoded-content-length` is optional by design — publishes a chunk
/// count consistent with the **size-independent** selection.
///
/// The stock SDK always sends the decoded-length header, so this form is driven by hand.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lengthless_streaming_put_uses_the_size_independent_selection() {
    let (addr, meta, _dir) = start(512).await;
    let key = "lengthless.bin";
    let payload = body(41, 64 * 1024);

    // An UNSIGNED single-shot body with no declared length: `content-length` frames the
    // request, and the store is given no object length to size from — the same "no length to
    // evaluate the formula against" condition a lengthless `aws-chunked` stream presents.
    let host = addr.to_string();
    let path = format!("/{BUCKET}/{key}");
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
        "",
        &host,
        &amz_date,
        "UNSIGNED-PAYLOAD",
        &creds,
        REGION,
        "s3",
    );
    let mut request = format!("PUT {path} HTTP/1.1\r\nhost: {host}\r\n");
    request.push_str(&format!("authorization: {}\r\n", signed.authorization));
    request.push_str(&format!("x-amz-date: {}\r\n", signed.amz_date));
    request.push_str("x-amz-content-sha256: UNSIGNED-PAYLOAD\r\n");
    request.push_str("transfer-encoding: chunked\r\n");
    request.push_str("connection: close\r\n\r\n");
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(request.as_bytes()).await.expect("head");
    // One HTTP/1.1 chunk, then the terminal chunk — no `content-length` anywhere.
    stream
        .write_all(format!("{:x}\r\n", payload.len()).as_bytes())
        .await
        .expect("chunk size");
    stream.write_all(&payload).await.expect("chunk data");
    stream
        .write_all(b"\r\n0\r\n\r\n")
        .await
        .expect("terminator");
    stream.flush().await.expect("flush");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.expect("response");
    let head = String::from_utf8_lossy(&response).into_owned();
    assert!(
        head.starts_with("HTTP/1.1 200"),
        "a lengthless streaming PUT must still succeed: {head}"
    );

    let inode = meta
        .raw_scan("inode:")
        .into_iter()
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
        .find(|v| v.contains("Committed"))
        .expect("a committed inode");
    let chunks = published_chunk_ids(&inode).len();
    // The size-independent selection is ⌈5 GiB / MAX_MAP_CHUNKS⌉ ≈ 31 MiB, so a 64 KiB body
    // is ONE chunk — while the configured 512 B chunk size would make it 128. That is the
    // discriminator: on the base the count follows the configured size.
    assert_eq!(
        chunks, 1,
        "a lengthless stream must size its chunks from the SIZE-INDEPENDENT rule \
         (⌈5 GiB / MAX_MAP_CHUNKS⌉ ≈ 31 MiB), so a 64 KiB body is one chunk; it published \
         {chunks}, which is the configured 512 B chunk size (the base's behaviour)"
    );
}

/// **Negation-4 probe (concurrency).** Four parts uploaded CONCURRENTLY all commit.
///
/// A part commit only *reads* the session record (`require(mpu == Open@E)`); a
/// precondition does not write, so N concurrent commits sharing one session precondition
/// never conflict with each other. An implementation that WROTE the session record on each
/// part commit — a per-session counter, a "last part" stamp — would serialize them and lose
/// all but one to `Conflict`. Leg A is sequential and cannot catch that; this is the
/// headline `aws s3 cp` shape, so it is asserted deliberately.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_part_uploads_do_not_conflict_with_each_other() {
    let (addr, _meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "concurrent.bin";
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

    let mut tasks = Vec::new();
    for number in 1..=4i32 {
        let client = s3.clone();
        let key = key.to_string();
        let id = upload_id.clone();
        tasks.push(tokio::spawn(async move {
            client
                .upload_part()
                .bucket(BUCKET)
                .key(&key)
                .upload_id(&id)
                .part_number(number)
                .body(ByteStream::from(body(number as u8, MIN_PART)))
                .send()
                .await
                .map(|r| r.e_tag().unwrap_or_default().to_string())
        }));
    }
    let mut committed = 0;
    for task in tasks {
        let outcome = task.await.expect("join");
        assert!(
            outcome.is_ok(),
            "every concurrent part upload must commit — a part commit that WROTE the session \
             record would serialize them and lose all but one: {outcome:?}"
        );
        committed += 1;
    }
    assert_eq!(committed, 4, "all four concurrent parts committed");
}

/// **Negation-4b (the same part number, racing).** A part attempt that reads its `part:` key as
/// absent while a concurrent attempt's record lands before its commit answers **200**, last
/// writer wins — never `404 NoSuchUpload` for a session that is demonstrably open.
///
/// This is the shape an SDK produces whenever it times out a slow part and retries while the
/// first attempt is still streaming, and real S3 answers `200` to both. The part commit
/// preconditions on the `part:` key — `require_absent` when the attempt saw no prior — so the
/// loser gets a store `Conflict`, and spelling that "your upload does not exist" makes aws-cli
/// treat a transient, self-resolving race as fatal and abandon the whole transfer. The
/// sequential re-upload is covered elsewhere and would NOT catch it; the sibling probe above
/// drives four *distinct* part numbers and would not either.
///
/// The interleaving is produced by the store fixture (`MemMeta::hide_next_read`), not by racing
/// two clients and hoping: the window is the few microseconds between one `get` and one
/// `commit`, so a timing-based version passes for the wrong reason far more often than it
/// catches the defect. The precondition the commit evaluates is the real one against the real
/// stored record.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_same_part_number_retry_racing_the_original_is_not_a_lost_upload() {
    let (addr, meta, _dir) = start(64 * 1024).await;
    let s3 = sdk_client(addr);
    let key = "same-number-race.bin";
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

    // The "first attempt": an ordinary part 1, committed.
    let first = s3
        .upload_part()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(body(11, MIN_PART)))
        .send()
        .await
        .expect("the first attempt commits")
        .e_tag()
        .expect("etag")
        .trim_matches('"')
        .to_string();

    // The "retry": its pre-commit read of `part:<id>:00001` answers absent — exactly what it
    // would have seen had it started before the first attempt committed — so its batch carries
    // `require_absent(part:…)` while the store holds the first attempt's record.
    meta.hide_next_read(&format!("part:{upload_id}:00001"));
    let second = s3
        .upload_part()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(ByteStream::from(body(22, MIN_PART)))
        .send()
        .await;
    let second = match second {
        Ok(response) => response
            .e_tag()
            .expect("etag")
            .trim_matches('"')
            .to_string(),
        Err(err) => panic!(
            "a same-part-number retry that loses the `part:` precondition must answer 200 (last \
             writer wins), never `NoSuchUpload` for a session that is still open: {err:?}"
        ),
    };
    assert_ne!(
        first, second,
        "the two attempts carry different bodies, so different digests"
    );

    // Exactly ONE record survives for part 1, it is the LAST writer's, and Complete over it
    // publishes — so the winner is a whole part, not a half-committed one.
    let listed = s3
        .list_parts()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("list_parts");
    assert_eq!(
        listed.parts().len(),
        1,
        "one part number holds exactly one record however many attempts raced"
    );
    let winner = listed.parts()[0]
        .e_tag()
        .expect("etag")
        .trim_matches('"')
        .to_string();
    assert_eq!(
        winner, second,
        "last writer wins: the surviving record is the retry's, not the attempt it superseded"
    );
    s3.complete_multipart_upload()
        .bucket(BUCKET)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(
            aws_sdk_s3::types::CompletedMultipartUpload::builder()
                .parts(
                    aws_sdk_s3::types::CompletedPart::builder()
                        .part_number(1)
                        .e_tag(&winner)
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .expect("the session survives the race and publishes");
}
