//! **Every S3 response identifies the build that produced it** (issue #736), proven against
//! the built `wyrd` binary running as an `s3` role.
//!
//! Nothing on the wire said what a client was talking to: no `Server` header and no build
//! identifier anywhere in the response path, so a client could not tell last month's
//! deployment from today's, a captured HTTP exchange did not identify the build that produced
//! it, and a version-keyed capability matrix (proposal 0017 §3) had nothing to key on but an
//! operator-supplied parameter that can silently be wrong.
//!
//! Four things are asserted, each against a real `wyrd s3` child process:
//!
//! 1. **both planes carry it** — a signed request that succeeds and an unsigned request
//!    refused with 403 come back carrying a `Server` header whose value starts `wyrd/`. The
//!    error leg is the load-bearing half: an error response is the one most likely to end up
//!    in a bug report, and it is the leg a per-handler implementation would miss;
//! 2. **the value is the BAKED build identity, not a default** — the remainder is non-empty,
//!    is not the caller-agnostic `unknown` a bare `S3Config::new` carries, and is not the
//!    bare `0.0.0` workspace placeholder (an equality test, not a prefix one: the legitimate
//!    value on an untagged checkout is `0.0.0+git.<sha>`);
//! 3. **the startup log records the same string** — the child's `role started` JSON event
//!    carries a `version` field byte-equal to the header's remainder, so the running build is
//!    readable from the logs as well as the wire;
//! 4. **it came from git, not from a constant** — where a repository is visible and HEAD is
//!    not exactly a tag, the advertised remainder CONTAINS the short commit sha.
//!
//! **Why the BINARY and not an in-process fixture.** `S3Config::new` defaults the identity to
//! `unknown` so a library caller still emits a well-formed header; a fixture that composes
//! `S3Config` in-process (the `tests/s3_http_wire.rs:78-92` shape) would therefore go green on
//! `wyrd/unknown` with the build script, the version derivation and the composition-root
//! assignment all missing — the header shipped inert. Only the composition root
//! (`cli::serve_s3`) sets the field from the build script's constant, and it is reachable only
//! by running the real binary (the `tests/cli_roundtrip.rs:11-18` idiom, except that this
//! child is a long-running server: it is spawned, read from, and killed).
//!
//! Nothing here names a symbol this slice introduces — no new config field, no new constant,
//! no version module — so the file compiles unchanged against the pre-fix tree, where it goes
//! RED on the absent header (leg 1) and the absent log field (leg 3) rather than failing to
//! build.

#![forbid(unsafe_code)]
// wall-clock exempt (test crate): the SigV4 request date is stamped from real wall time so
// the signature lands inside the gateway's freshness window; no clocked lifecycle is
// asserted on here (#619). File scope, as in `tests/s3_http_wire.rs:28-34`: a test crate
// never ships, so no production lifecycle can acquire a mixed clock from it.
#![allow(clippy::disallowed_methods)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use wyrd_gateway_s3::sigv4::{format_amz_date, sign, Credentials};

/// The binary under test — the same idiom `tests/cli_roundtrip.rs:11` uses.
const WYRD: &str = env!("CARGO_BIN_EXE_wyrd");
const ACCESS_KEY: &str = "AKIAIOSFODNN7EXAMPLE";
const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
const REGION: &str = "us-east-1";
/// The line `cmd_s3` prints once the listener is bound (`crates/server/src/cli.rs:2183-2186`),
/// which reports `listener.local_addr()` — so the ephemeral port is PARSED rather than
/// guessed and two of these tests can run concurrently.
const LISTEN_LINE: &str = "wyrd s3: serving S3-compatible HTTP on ";
/// Ceiling on every wait: a role that never starts must fail the test, not hang it. The role
/// blocks forever by design, so an unbounded read here would turn a red into a stall.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
/// Ceiling on a single request/response exchange over loopback.
const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// A `wyrd s3` child process. **Killed from `Drop`**, so the child dies on the
/// assertion-failure path as well as the success one — the role serves until it is stopped.
struct Role {
    child: Child,
    /// The child's stderr, line by line, drained by a reader thread: both the listening
    /// address (a plain `eprintln!`) and the `role started` event (JSON, via the global
    /// subscriber, which writes to stderr — `crates/server/src/logging.rs:301-328`) arrive
    /// here.
    lines: Receiver<String>,
}

impl Drop for Role {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Role {
    /// Read stderr lines until `f` accepts one, bounded by [`STARTUP_TIMEOUT`].
    fn wait_for<T>(&self, what: &str, mut f: impl FnMut(&str) -> Option<T>) -> T {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    if let Some(found) = f(&line) {
                        return found;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    panic!("timed out after {STARTUP_TIMEOUT:?} waiting for {what}")
                }
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("the s3 role exited before {what}")
                }
            }
        }
    }
}

/// Spawn the built binary as an `s3` role on an ephemeral loopback port and return it with
/// the address it reports.
fn start_role(data_dir: &Path) -> (Role, SocketAddr) {
    let mut child = Command::new(WYRD)
        .args([
            "s3",
            "--s3-listen",
            "127.0.0.1:0",
            "--data-dir",
            data_dir.to_str().expect("utf-8 path"),
            "--access-key",
            ACCESS_KEY,
            "--secret-key",
            SECRET_KEY,
            "--region",
            REGION,
            // Machine-readable startup events, so leg 3 reads a field rather than a phrase.
            "--log-format",
            "json",
            // Explicit, so an ambient `RUST_LOG` in the runner's environment cannot filter
            // the `role started` event out from under leg 3 (`--log-level` overrides both
            // the default and `RUST_LOG`, `crates/server/src/logging.rs:184-206`).
            "--log-level",
            "info",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the built wyrd binary as an s3 role");

    let stderr = child.stderr.take().expect("the child's stderr is piped");
    let (tx, rx) = std::sync::mpsc::channel();
    // Drain stderr continuously: the role would otherwise block on a full pipe, and the
    // startup lines must be buffered before the requests below are made.
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines() {
            match line {
                Ok(line) => {
                    if tx.send(line).is_err() {
                        break; // the test finished; nothing is reading any more
                    }
                }
                Err(_) => break,
            }
        }
    });

    let role = Role { child, lines: rx };
    let addr = role.wait_for("the s3 role's listening address", |line| {
        line.strip_prefix(LISTEN_LINE)
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|addr| addr.parse::<SocketAddr>().ok())
    });
    (role, addr)
}

/// The SigV4 headers for a request, produced by the production signer exactly as
/// `tests/s3_http_wire.rs:94-109` does.
fn signed_headers(method: &str, path: &str, host: &str, body: &[u8]) -> Vec<(String, String)> {
    let creds = Credentials {
        access_key_id: ACCESS_KEY.to_string(),
        secret_access_key: SECRET_KEY.to_string(),
    };
    // Stamp a fresh timestamp so the request is inside the gateway's freshness window.
    let amz_date = format_amz_date(SystemTime::now());
    let signed = sign(
        method, path, "", host, &amz_date, body, &creds, REGION, "s3",
    );
    vec![
        ("authorization".to_string(), signed.authorization),
        ("x-amz-date".to_string(), signed.amz_date),
        ("x-amz-content-sha256".to_string(), signed.content_sha256),
    ]
}

/// Send one HTTP/1.1 request over a fresh connection and return `(status, header block)`.
///
/// `s3_http_wire.rs`'s `send` helper returns `(status, body)` only; this test needs the
/// response HEADERS, so it reads the head itself. The request shape and the signing are the
/// same. Both directions are timeout-bounded, so a wedged child fails the test rather than
/// stalling the suite.
fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> (u16, String) {
    let host = addr.to_string();
    let mut request = format!("{method} {path} HTTP/1.1\r\n");
    request.push_str(&format!("host: {host}\r\n"));
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str(&format!("content-length: {}\r\n", body.len()));
    request.push_str("connection: close\r\n\r\n");

    let mut stream = TcpStream::connect(addr).expect("connect to the role's listener");
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .expect("write timeout");
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .expect("read timeout");
    stream
        .write_all(request.as_bytes())
        .expect("write the request head");
    stream.write_all(body).expect("write the request body");
    stream.flush().expect("flush");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("read the response");
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("the response has a header terminator");
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
    (status, head)
}

/// Pull a header value out of a raw HTTP/1.1 head block, case-insensitively.
fn header_value(head: &str, name: &str) -> Option<String> {
    head.lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .starts_with(&format!("{}:", name.to_ascii_lowercase()))
        })
        .map(|line| line.split_once(':').expect("header").1.trim().to_string())
}

/// The `wyrd/` remainder of a response's `Server` header — the advertised build identity.
fn advertised_version(head: &str, plane: &str) -> String {
    let server = header_value(head, "server").unwrap_or_else(|| {
        panic!(
            "the {plane} response carries no `Server` header — nothing on the wire says what \
             build a client is talking to. Head:\n{head}"
        )
    });
    server
        .strip_prefix("wyrd/")
        .unwrap_or_else(|| {
            panic!("the {plane} response's `Server: {server}` must be `wyrd/<version>`")
        })
        .to_string()
}

/// Run `git` in this crate's directory (the test binary's cwd is the package root, which is
/// inside the checkout), returning trimmed stdout on success.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Legs 1-3: both response planes advertise the BAKED build identity, and the startup log
/// records the same string.
///
/// Pre-fix this is RED twice over: no response carries a `Server` header at all (the gateway
/// sets none and axum/hyper add none), and the `role started` event records `role`, `listen`,
/// `region` and `dservers` and nothing else.
#[test]
fn every_s3_response_advertises_the_baked_build_version() {
    let work = tempfile::tempdir().expect("temp dir");
    let (role, addr) = start_role(work.path());
    let host = addr.to_string();
    let path = "/wyrd-bucket/version-probe";
    let object = b"an object stored while the response head is inspected".to_vec();

    // The success plane.
    let (status, head) = send(
        addr,
        "PUT",
        path,
        &signed_headers("PUT", path, &host, &object),
        &object,
    );
    assert_eq!(status, 200, "a signed PUT must be accepted. Head:\n{head}");
    let advertised = advertised_version(&head, "signed PUT");

    // The error plane — the leg a per-handler implementation would miss, and the response
    // most likely to end up in a bug report.
    let (status, head) = send(addr, "GET", path, &[], b"");
    assert_eq!(
        status, 403,
        "an unsigned GET must be refused. Head:\n{head}"
    );
    assert_eq!(
        advertised_version(&head, "unsigned 403"),
        advertised,
        "success and failure must name the SAME build"
    );

    // Leg 2: it is a real build identity, not a default and not the bare placeholder.
    assert!(
        !advertised.is_empty(),
        "`Server: wyrd/` with nothing after it identifies no build"
    );
    assert_ne!(
        advertised, "unknown",
        "the running binary advertises `wyrd/unknown` only if the composition root never set \
         the baked identity — the header would be shipped inert"
    );
    assert_ne!(
        advertised, "0.0.0",
        "the bare workspace placeholder is the same string for every build ever made; a \
         derived version on an untagged checkout is `0.0.0+git.<sha>`"
    );

    // Leg 3: the startup log names the same build, so it is readable from the logs as well
    // as the wire.
    let logged = role.wait_for("the `role started` event", |line| {
        let event: serde_json::Value = serde_json::from_str(line).ok()?;
        let fields = event.get("fields")?;
        if fields.get("message")?.as_str()? != "role started" {
            return None;
        }
        // `None` here is a MISSING version field, which must fail the test rather than
        // spin: report the whole event so the failure is diagnosable.
        Some(
            fields
                .get("version")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    panic!("the `role started` event carries no `version` field: {fields}")
                }),
        )
    });
    assert_eq!(
        logged, advertised,
        "the logged build and the advertised build must be byte-equal, or a log line and a \
         captured HTTP exchange name two different things"
    );
}

/// Leg 4: the advertised version came from **git**, not from a constant — the remainder
/// contains the commit the binary was built from.
///
/// Deliberately a containment check on the short sha rather than equality with a
/// re-normalized `git describe`: it binds the value to the real derivation while staying
/// immune to the `-dirty` suffix (a gate applies a patch, so the tree may or may not be dirty
/// when the build script runs) and to tag shape. It must not call the production normalizer —
/// that would make the oracle the code under test.
///
/// Skipped, with a printed reason, in the two states where a git-derived version carries no
/// sha to look for: no repository is visible (a source tarball, or the image build whose
/// `.dockerignore` excludes `.git/`), or HEAD is exactly a tag (a release version is a bare
/// `X.Y.Z` by design).
///
/// Deliberately NOT skipped on an ambient `WYRD_VERSION`, though a build environment CAN
/// supply one: cargo re-exports a build script's own `cargo:rustc-env` into the environment
/// of the test process it spawns, so this process always sees the variable once the build
/// bakes one — measured, not assumed — and an inherited override is indistinguishable from
/// cargo's own re-export from in here. Skipping on its presence would make this leg
/// vacuous in exactly the runs it exists for. The cost of asserting instead is that a
/// deliberate `WYRD_VERSION=1.2.3 cargo test …` fails this one leg; the message says so.
#[test]
fn the_advertised_version_carries_the_commit_it_was_built_from() {
    let Some(sha) = git(&["rev-parse", "--short", "HEAD"]) else {
        println!("skipped: no git repository is visible from this checkout");
        return;
    };
    if git(&["describe", "--tags", "--exact-match", "HEAD"]).is_some() {
        println!("skipped: HEAD is exactly a tag, so the release version carries no sha");
        return;
    }

    let work = tempfile::tempdir().expect("temp dir");
    let (_role, addr) = start_role(work.path());
    let (status, head) = send(addr, "GET", "/wyrd-bucket/absent", &[], b"");
    assert_eq!(
        status, 403,
        "an unsigned GET must be refused. Head:\n{head}"
    );

    let advertised = advertised_version(&head, "unsigned 403");
    println!("advertised build identity: wyrd/{advertised} (HEAD {sha})");
    assert!(
        advertised.contains(&sha),
        "the advertised `wyrd/{advertised}` must carry the commit it was built from ({sha}) — \
         a version that does not is a constant, not a build identity. (If this build was \
         handed an explicit WYRD_VERSION, that value is used verbatim and this leg cannot \
         hold; build without the override to run it.)"
    );
}
