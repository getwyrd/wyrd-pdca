## Review rubric & protocol

This section binds both sides of a review: authors self-review against it
before requesting review, and reviewers judge against it — the repo's written
conventions are the arbiter, not reviewer taste. Every rule here earned its
place from a real review finding; when a class recurs, it graduates to a
deterministic gate and drops out of review scope.

### Hard conventions (MUST)

- **One clock per correctness lifecycle** (ADR-0009): all clock reads that
  decide one lifecycle's correctness — stamping a lease, checking its expiry,
  arming its GC — share a single time source; never mix a logical/manual
  clock with the wall clock inside one lifecycle (the #557/#565 defect
  class), and the expired-pending sweep defaults to deferring records whose
  lease may have been stamped under a different clock epoch rather than
  collecting them. Direct `SystemTime::now()` is conforming where madsim
  virtualises it (DST, ADR-0009; the ADR-0047 publication timestamp is the
  worked example); code that needs *test-controlled* time takes the testkit
  `Clock` seam (ADR-0024, Proposed). A new clock read states which source
  owns its lifecycle.
- **Narrow trait seams and dependency direction** (ADR-0010, ADR-0016): as
  stated under Architecture invariants; protocol gateways use only the traits
  their seam grants (ADR-0046).
- **Metadata validation boundaries** (ADR-0045, Proposed — current working
  practice): structural invariants are validated at decode and surface as
  errors, never as values; *contextual* checks (e.g. placement length) are
  liberal on read and strict in maintenance paths.
- **No DST-reachable shared mutable global state** (ADR-0035, Proposed —
  mechanically enforced today by `cargo xtask ci`'s statics gate).
- **Every new crate root carries `#![forbid(unsafe_code)]`** (`metadata-fdb`
  holds the sole, FFI-motivated `deny` exception).
- **Docs currency**: a change that adds or alters a port, an API operation, an
  RPC, a CLI flag, or a persisted field updates the living architecture doc in
  the same PR (see Design documents above). This is a merge requirement, not a
  follow-up.

### Recurring defect classes (MUST check when the diff touches the surface)

- *Protocol input*: torn, truncated, or oversize input is indeterminate or an
  error — never silently accepted. Enforce declared `Content-Length`, the
  chunked terminal CRLF, and cumulative (not per-line) section budgets sized
  for the worst-case **encoded** representation of the input.
- *Grammar strictness*: hand-rolled parsers for RFC formats (HTTP dates,
  `Range`, entity tags) validate every token — weekday names, digit widths, no
  `+`/`-` signs via `from_str`, case-insensitive units, clock-relative
  two-digit years (RFC 9110 §5.6.7). Prefer extending a shared parser over
  writing a new one.
- *Serialization identity*: optional/legacy fields are omitted when absent,
  never emitted as defaults — decode→encode must be byte-identical wherever a
  compare-and-swap or content hash depends on it (add the round-trip test).
  Absent timestamps/ETags are omitted from responses, never fabricated
  (epoch-1970) or emitted unvalidated against their grammar.
- *Absent or unsupported entries*: produce an explicit error or enqueue a
  repair obligation — never silent success, silent skip, or a count-based
  assertion that can pass while the property fails.
- *Transactions*: roll back (best-effort) before any early return over a live
  transaction; an aggregate error must let `CommitUnknownResult` outrank
  `Conflict` — never report a dropped write as a clean conflict.
- *Await discipline*: every await on external work is bounded (timeout,
  fail-closed); spawned helper tasks are aborted on drop; shutdown never joins
  a potentially infinite stream.
- *Probes and readiness*: readiness reflects the backend (fail-closed at
  start, `NOT_SERVING` before drain); liveness stays backend-independent;
  probe endpoints are reachable from the deployment topology and carry
  concurrency limits.
- *Test fidelity*: DST/sim models mirror the production adapter's error and
  seam semantics; conformance contracts run on every backend; a new
  destructive or concurrent path lands with seeded Tier-0 DST coverage.
- *Workflow edits*: re-check path filters, feature matrices, and diff-filter
  letters (`A`/`R`) for blind spots; guards scan every reachable directory.

### Reviewer protocol

- **DCO**: the `dco` status check is the sole authority. Do not report
  `Signed-off-by` findings from your own commit inspection — the SHAs a
  review context exposes are often GitHub's synthesized merge-preview
  commits, and every observed finding of this class was a false positive.
- **Deferrals are settled**: a finding answered with "Deferred — tracked in
  #N" (or an in-code `// deferred: #N` marker) is resolved for review
  purposes; do not re-raise it in later rounds. Raise the tracking issue
  instead if the deferral itself seems wrong.
- **Out of scope**: a real finding outside the PR's stated scope gets a
  decline-with-issue-reference, not an in-PR fix.
- **Definition of done**: deterministic gates green plus **one** deep,
  multi-pass review whose findings are each fixed or rejected with a recorded
  reason. Do not iterate review rounds chasing silence, and refresh stacked
  branches onto their base before reviewing dependents so stale content is
  not re-reported.
