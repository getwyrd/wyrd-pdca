# Design proposal — issue 633 / mpu-operator-exit

> Plan artifact (design-proposal form; enhancement). Do reads ONLY this file.
> The `- **Label:** value` lines are parsed by the driver — keep their shape.
>
> **The design is settled upstream:** proposal **0016 — the multipart commit protocol**,
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md` **as it stands on `origin/main`
> @ `22d71b4`** (content tip `c35d39d`, merged by PR #627, tracking issue #626). Read the file in
> the checkout, never a commit — `97e2392` is only the initial draft. This slice is its **FU-6**
> (`0016:2873`), and its normative points are decision 6's foreign-clock skip
> (`0016:1889-1907`, `:1984-1998`), decision 3's `Completing → Aborting` edge
> (`0016:572`, batch row `0016:665`) and the implementation order's point 3
> (`0016:255-262`). **Do MUST read those passages before writing code.**
>
> **This is a hard blocker on #625, not a deferral.** The reaper's clock guard deliberately
> *skips* a session whose clock it does not recognise, so for that session these verbs are the
> **only** exit in the whole design. Shipping the guard without them makes that state
> **absorbing**: its records, its staged bytes and its admission slot are held forever, and
> `MAX_SESSIONS` of them is a permanent `503 SlowDown` no in-system actor can clear.
>
> Citations re-verified against `origin/main` @ `22d71b4` on 2026-07-25. **This bundle builds on
> #636 *and* #625, not on bare `main`** — see `Depends on` / `Ordering note`.
>
> **RE-POINTED 2026-07-26.** #508 was re-planned as five slices (634 → 635 → 636 → 637 → 508)
> after its seventh attempt was rejected on reviewability. The session records and the fenced
> edges these verbs drive now land in **#636**; #508 is reduced to the S3 wire surface, which
> these management verbs do not touch. Below, "#508" has been replaced by the slice that actually
> lands each thing — except where it means the **release order** (this bundle and #625 still land
> with or before #508), which is unchanged.

- **Slug:** mpu-operator-exit
- **Kind:** enhancement (design proposal)
- **Goal:** give a session the reaper declines to judge an actual exit — **two operator verbs and
  one alarm**: (1) an operator-authorized **abort** that fences any `Open` or `Completing`
  session to `Aborting`, installing, for a `Completing` session and **in the same batch**,
  `retire:records:{seg:<g>:<E>}` for the segments that attempt already wrote; (2) an
  operator-authorized **terminal expiry** for a `Completed` **foreign-clocked** session whose
  tombstone window cannot be judged (abort does not apply to a terminal session); and (3) the
  **alarm** that names a skipped foreign-clocked session and summons the operator to use them.
- **Success criterion:** two new test files (see `Test file`), all legs asserted on durable store
  state or on a captured operator signal.
  **(A) Operator abort of an `Open` session.** Against a seeded `Open@E` session with staged
  parts, the abort verb answers success and leaves the session `Aborting@E+1` with
  `retire:bytes:{session}` installed (the Open door's payload, `0016:664`). A subsequent reaper pass (the one #625
  landed) then tears it down to zero — records gone, `orphan:` marks written, `mpuctl.count`
  decremented — so the verb hands off to the ordinary clock-free path rather than growing a
  second teardown.
  **(B) Operator abort of a `Completing` session ALSO retires that attempt's segments.** Against
  a seeded `Completing@E` session that has already written `seg:<g>:<E>:*` records, the verb
  leaves it `Aborting@E+1` **and** `retire:records:{seg:<g>:<E>}` present, and a **later** epoch's
  `seg:` range seeded alongside is **not** named by the obligation; a following reaper pass drains
  the named range empty.
  **One-batch atomicity is REQUIRED but is deliberately NOT in the binding criterion** — for
  either door. Both fences MUST commit their state change and their obligation(s) in a single
  batch (`0016:664-665`; for the `Completing` door, no interleaving may leave a session whose
  segments have no reclaimer — the X57 hole), but an end-state scan cannot tell one batch from
  two, and the C4-verify RED leg runs only the added test files. Atomicity is therefore evidenced
  **outside C4**, and Do owes both: (i) seeded DST cases appended to an existing
  `crates/dst/tests/custodian.rs` that interrupt between the two commits for **each** door, and
  (ii) the batch construction stated in `build-notes.md` for the reviewer to read. A green C4 is
  not a proof of it, and this brief does not pretend otherwise.
  **(C) Abort refuses where 0016 says it must — and the refusal is DISCRIMINATING.** A
  `Completed` session (tombstone) and an absent session are **not** abortable; an
  already-`Aborting` session is idempotent (success, no second fence, no double obligation, epoch
  unchanged). Careful: on the base an unknown command *also* leaves the store untouched and exits
  non-zero, so "store unchanged + non-zero" is **not** an oracle for these subcases — it is
  already true pre-fix. Each refusal subcase must therefore assert what only a *handled* refusal
  produces: a **distinct exit status** (the handled-error path returns `ExitCode::FAILURE` = 1,
  `crates/server/src/cli.rs:429-433`, whereas an unknown command exits **2**, `:419-423`) **and**
  a **stderr message naming the session state and, for the terminal case, the expiry verb**. Both
  are observable only because the tests run the real binary as a subprocess (see `Repro
  instruction`); an in-process `cli::run` returns an `ExitCode` the test cannot compare and writes
  its message straight to the process's stderr.
  **(D) Terminal expiry of a `Completed` foreign-clocked session.** Against a seeded `Completed`
  session whose `clock_source` the reaper does not own — which #625's reaper leaves untouched
  forever, because its tombstone window is measured from a stamp under a clock it does not own —
  the expiry verb records the operator's authorization, and the **next reaper pass** (driven in
  the test by the operator one-shot #625 landed, invoked as argv exactly like the verbs) performs
  the ordinary terminal delete: `mpu:` record gone, surviving `slot:` records gone, `mpuctl.count`
  decremented (0016's terminal predicate, `0016:2209-2215`). Assert **both** halves: the
  authorization is durable, and the pass that consumes it is the reaper's, not a second teardown
  path in the CLI. Assert too that an **unauthorized** foreign-clocked `Completed` session in the
  same store is still untouched by that pass.
  **(E) The alarm fires, and it names the session.** A reaper pass that meets a foreign-clocked
  session emits an operator-visible alarm carrying **the upload id and the `clock_source` it did
  not recognise** — never a silent skip (`0016:1989-1991`, normative). One alarm per skipped
  session per pass; a session whose clock the reaper **does** own produces none. **Store choice is
  a compile constraint, not a preference:** `wyrd-custodian` depends on `wyrd-traits` and
  `wyrd-core` only — it has **no** redb dependency, in `[dependencies]` or `[dev-dependencies]`
  (`crates/custodian/Cargo.toml`) — so this test MUST use the **in-file in-memory `MetadataStore`
  implementation the custodian suite already writes for exactly this purpose**
  (`crates/custodian/tests/gc.rs:46-70`, `gc_telemetry.rs:35-60`), never `RedbMetadataStore`.
  Adding a dev-dependency instead would put the fix in `Cargo.toml`, which the RED leg reverts
  while keeping the test — the compile-error false-PASS this brief exists to avoid. Capture the
  emission the way `gc_telemetry.rs` does (its own test binary, a capture layer, and/or
  `gather_prometheus` read-back).
- **Falsifiability:** RED is producible **in-process on this bundle's own base** — the folded
  `origin/pdca-integration/main` carrying #634, #635, #636 and #625 — with no deploy stack, no
  fleet and no container. On that base the records, the fenced transitions, the retirement ledger and the
  reaper all exist, but **no verb can move a foreign-clocked or fenced session and nothing names
  it to an operator**: legs A–D fail because the seeded session is exactly as seeded after the
  command (the command itself does not exist — `cli::run` answers `unknown command` and exits 2,
  `crates/server/src/cli.rs:419-423`), and leg E fails because the skip emits no alarm.
  **Base resolution is a gate-evaluability precondition, not a detail:** this is a **wave-2**
  bundle, so the driver stamps its stack base and exports `$PDCA_VERIFY_BASE =
  origin/pdca-integration/main` (`src/pdca_harness/flow.py:459`, `gates.py:352-360`), which
  `run-verify.sh` honours ahead of the brief's base (`engine/scripts/run-verify.sh:186-192`).
  If that export is missing, C4-verify resets to `origin/main`, neither prerequisite exists, the
  added tests **fail to compile**, and the RED leg's failure branch — which has **no zero-test
  guard on the non-zero path** (the `TESTS_RAN == 0` check at `:416-427` sits inside the
  cargo-*succeeded* branch; a compile failure skips that block and falls through to the
  unconditional PASS at `:433`, see `:415-434`) — prints "PASS — red without the fix" over a build
  that ran nothing. Do MUST record, from the RED leg, **how many tests actually
  ran and failed**; a zero-test run is a non-result, not a pass.
  **Corollary the tests must obey:** an added test file may reference **only symbols that exist
  on this bundle's base** (`main` + #634 + #635 + #636 + #625) — nothing this slice adds. The verbs are
  therefore driven as **argv strings** through `wyrd_server::cli::run` (a `pub fn` on the base,
  `crates/server/src/cli.rs:399`), and the alarm leg drives the reaper through the entry #625
  landed (`reconcile_step` plus its reaper context, `crates/custodian/src/reconciliation.rs`) —
  **this slice must not change either signature**, or one of the two legs stops compiling in one
  of the two trees. #625's brief requires it to land a **stable, publicly constructible** reaper
  context for exactly this reason; if what actually landed cannot be constructed from an
  integration test, Do MUST report that as a §6 blocker — **not** invent a test-only entry point
  (the repo forbids a parallel entry to the fenced control point,
  `crates/custodian/src/gc.rs:124-128`).
- **Invariant to restore:** **no state of an upload session is absorbing** — for every state,
  including one the reaper declines to judge, some actor can drive it out, and the resources it
  holds (records, staged bytes, its admission slot) are released in bounded time. Where the
  system's own clock cannot supply the judgment, the **operator** supplies it; the teardown that
  follows stays clock-free. **Source:** 0016 invariant (3) and its exit table (`0016:567-579`),
  the normative implementation-order point 3 (`0016:255-262`), and the accepted-costs row for a
  skipped foreign-clocked session ("at worst `MAX_SESSIONS` slots, each behind an alarm naming
  the session and its `clock_source`"). Provenance (`docs/principles.md`): this brief falls in the **§6 storage-lifecycle /
  reclamation** category, so per principle 4.2 it states catalogue invariant **C-1** — *a
  permanent or data-losing failure mode is never an acceptable cost* — with its citation
  (maintainer's standing rule, 2026-07-25; corroborated by 0016's refutation standard
  `:2802-2813` and `crates/custodian/src/gc.rs:22-25`). This is a **structural / lifecycle** change, so
  §1.2 applies: the target is the smallest change that **restores the invariant**, not the
  smallest diff.
  *Plan-exit gate (category-gated, both checks pass):* Scope names verbs, edges and the
  authorization record — no probe/guard/helper mechanism; and the invariant cannot be satisfied
  by guarding a single module, because it quantifies over every session state and the actor set
  that can exit it.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Depends on:** 625, 636
- **Conflicts with:** 637
- **Ordering note:** **RE-POINTED 2026-07-26 — #508 was re-planned as five slices, and this
  bundle's prerequisites moved from `508, 625` to `625, 636`.** #508's seventh attempt was
  rejected at sign-off on reviewability and it is now split into 634 → 635 → 636 → 637 → 508;
  #508 is reduced to the S3 wire surface, which these management verbs do not touch. Both
  remaining edges are genuine build-on dependencies: the session records, the fenced
  `Completing → Aborting` edge, the `retire:` ledger and the `seg:` records come from **#636**
  (`seg:` originating in #635, transitive); the reaper whose skip path this alarm rides, and whose
  terminal delete consumes this slice's expiry authorization, comes from **#625**. The wave fold
  gives this bundle both accepted diffs without waiting for a human merge.
  **The release direction is unchanged and normative (`0016:255-262`): FU-6 ships WITH #625 — the
  verb *and* the alarm, not the verb on the first operator report — and both land with or before
  #508.** #625 must not close without this bundle landing alongside it (the maintainer's comment
  on #625 says so explicitly).
  **`Conflicts with: 637`** is new and is a file conflict, not a dependency: #637 rewrites
  `crates/custodian/src/{gc,restore,reconciliation}.rs` — the same files this slice reaches into
  for the reaper's skip branch and terminal predicate — so without the edge the two would share
  a wave and be built blind on one base.
- **Surfaces:** data
- **Difficulty:** high
- **External dependencies:** none
  — the CLI legs run the real binary against a redb store in a tempfile temp dir and the alarm leg
  runs in-process over an in-memory store; the verbs and the reaper are record-only, so no D-server
  fleet, no topology and no container is needed to make the criterion go red→green.
- **Test file:** `crates/server/tests/multipart_operator_verbs.rs` (legs A–D, the CLI verbs) and `crates/custodian/tests/multipart_foreign_clock_alarm.rs` (leg E, the alarm)
  — both on this line deliberately, since the driver parses test paths off the label's own line.
  Both are
  **NEW** `crates/<c>/tests/<t>.rs` files, which this project's C4 gate requires: `run-verify.sh`
  discriminates on an **added** test file (`_is_test_file`, `engine/scripts/run-verify.sh:93`)
  and degrades to a green-only, proves-nothing branch otherwise (`:392-402`). They are separate
  binaries **deliberately**: the alarm leg asserts a `tracing`/durability-plane emission, and
  `tracing` caches callsite interest in process-global state — the custodian suite already
  splits its telemetry leg into its own binary for exactly this reason
  (`crates/custodian/tests/gc_telemetry.rs:6-13`, issue #214). Neither file may be
  `#![cfg(...)]`-gated: the gate reads crate-level cfgs off the added sources and applies the
  resulting `RUSTFLAGS` to the whole invocation (`:347-366`). **Any DST regression this slice
  owes goes into an EXISTING `crates/dst/tests/*.rs` file** — a modified file, which the gate does
  not add to its invocation, and which `cargo xtask ci` still runs.
- **Verification posture:** flippable red→green at Check for every leg — all five fail on this
  bundle's base by assertion, not by compile error, provided the base resolves as described in
  `Falsifiability`. Do MUST additionally record these **negation runs** in `build-notes.md`, each
  with the leg that catches it: (1) fence a `Completing` session **without** installing
  `retire:records:{seg:<g>:<E>}` ⇒ leg B leaves segments with no adopting inode and no obligation
  naming them (the X57 hole); (2) install that obligation in a **second** batch — **note honestly
  that leg B's end-state scan CANNOT catch this**: a two-batch implementation reaches the same
  final state, so "both records present afterwards" is not an atomicity oracle. The atomicity is
  pinned instead by a **seeded DST case appended to an existing `crates/dst/tests/custodian.rs`**
  that interrupts between the two commits and asserts no `Completing`-fenced-with-unretired-
  segments state is reachable; if Do cannot construct that interleaving, say so and record the
  same-batch property as a reviewer/code-read obligation rather than claiming a test proves it;
  (3) name the **whole** `seg:<g>:`
  range rather than the fenced epoch's ⇒ leg B's later-epoch assertion fires (a rolled-back
  attempt's obligation must never delete a later attempt's published segments, F18); (4) make the
  expiry verb perform the terminal delete **itself** ⇒ leg D's "the reaper's pass consumed it"
  assertion fails, and the verb would bypass the empty-`sidx:` and no-obligation preconditions the
  terminal delete rests on; (5) let abort apply to a `Completed` session ⇒ leg C; (6) emit the
  alarm without the `clock_source` / upload id ⇒ leg E — an alarm an operator cannot act on is
  not an exit.
- **Production reach:** the live path is exercised at Check. The verbs run through the real CLI
  dispatch and commit real batches to a real store; the alarm is emitted from the reaper's own
  skip path through the fenced control point. Nothing here is honoured only by a test double.
  The one *interim* choice is the surface, and it is **settled, not open**: proposal **0008**'s
  management API + thin CLI is Milestone 8 and does not exist yet (`0008` names today's
  dev CLI as the only binary surface), so these verbs land on the existing `wyrd` CLI.
  **Interim-surface exception GRANTED — Eduard Ralph, 2026-07-25: "deviation granted as we can't
  wait for 0008."** Build against the dev CLI; do not wait for, or partially build, a management
  plane. See Open question 1 for the terms the grant carries.
- **Scope:** three deliverables, on the existing `wyrd` CLI (see `Production reach`):
  (1) **operator abort** — one verb taking the upload id and the store location, fencing
  `Open@E → Aborting@E+1` or `Completing@E → Aborting@E+1` on the operator's authority, in **one**
  batch preconditioned on the session record's exact bytes. **The two doors carry different
  payloads and must not be conflated:** the `Open` door is the ordinary abort/reap fence — 1 put
  plus `retire:bytes:{session}` (`0016:664`) — and the `Completing` door is the dedicated
  transition — 1 put plus `retire:bytes:{session, parts}` **and**
  `retire:records:{seg:<g>:<E>}` for that fence epoch's segments (`0016:665`, "one shape for all
  three doors" refers to the three *Completing* doors only); it refuses a `Completed` or
  absent session and is idempotent against an already-`Aborting` one. It is the **same shape** as
  the restore fence and the `W_session` over-age edge (`0016:665` — "one shape for all three
  doors"); reuse that code path rather than writing a third.
  (2) **terminal expiry** — a second verb that records the operator's authorization to expire a
  **`Completed` foreign-clocked** session whose tombstone window cannot be judged, as an
  **additive optional field on the `mpu:` session record**, written by a fenced CAS
  (`require(mpu == prior)`), read by the reaper's terminal predicate
  (`operator_expiry_authorized`, `0016:2209-2211`), and discarded with the record. **The
  interface is settled here, not left to Do**, because the test and the implementation must agree
  on it. **Exact argv (use these spellings verbatim in the code, the test, the runbook and the
  architecture doc):** `wyrd mpu abort --upload-id <ID> --data-dir <DIR>` and
  `wyrd mpu expire --upload-id <ID> --data-dir <DIR>`, both additionally accepting the role's
  standard `--metadata-backend <B>` and log flags, both flags-only (stray positionals refused as
  `cmd_custodian` does), **exactly one `--upload-id` per invocation — no wildcard, no bulk form**
  (a term of the granted interim-surface exception, Open question 1), and **no clock-source flag** — the local clock-source identity is resolved
  from the **same configuration helper #625's reaper uses**; reuse that symbol rather than
  re-deriving it, and if #625 exposed none, that is a §6 blocker to report, not a second source of
  truth to invent. The record field is a single additive optional value on the session record that
  is **absent** unless authorized (never `false`); the verb takes the **upload id** and the store location and
  **refuses** — with the discriminating exit status and message of criterion C — for (a) a session
  in any state other than `Completed`, and (b) a `Completed` session whose `clock_source` **equals
  the local one** (the reaper judges those itself; authorizing them would let an operator cut short
  a tombstone window S3 retry-idempotence depends on). Comparing `clock_source` therefore requires
  the verb to resolve the **same local clock-source identity the reaper uses**, from the same
  configuration seam — name it, do not re-derive it. The verb is **idempotent**: authorizing an
  already-authorized session succeeds and changes nothing. Add a criterion assertion for the
  locally-clocked refusal; without it, "authorize every tombstone" passes leg D. The field is
  `skip_serializing_if`-omitted when absent. **State the reason accurately:** the base's
  `renew_pending` and `live_lease_guards` precondition on the **raw stored bytes** they read
  (`require(key, current)`, `crates/core/src/metadata.rs:748-760`, `:786-795`), so the "a `null`
  would break every renewal" mechanism 0016 asserts at `:475-491` is not what that code does — do
  not repeat it. The requirement rests on the repo's standing rule (`AGENTS.md` *Serialization
  identity*: optional fields omitted when absent, never emitted as defaults; decode→encode
  byte-identical wherever a CAS or content hash depends on it) and on **#636's own session CAS
  paths**, which re-encode. Ship the decode→encode round-trip test for both shapes (field absent,
  field present). The verb does **not** delete anything itself: the
  terminal delete has preconditions (session-scoped obligations drained, `sidx:` range observed
  empty) that the reaper already implements and that must not be duplicated in a CLI.
  (3) **the alarm** — the reaper's foreign-clock skip emits an operator-visible signal naming the
  upload id and the unrecognised `clock_source`, on the existing durability/audit seam, once per
  skipped session per pass, with the operator's remedy (these two verbs) documented beside it.
  Plus: the **living architecture docs**
  (`docs/design/architecture/06-runtime-view.md`, `08-crosscutting-concepts.md`) updated for the
  new CLI verbs, the new session field and the alarm — a **merge requirement** in this repo
  (`AGENTS.md` "Docs currency"), not a follow-up; and the operator runbook line that tells whoever
  sees the alarm what to run.
  **Two edits inside #625's loop are IN scope, and are the only ones**: the alarm emission in its
  foreign-clock skip branch, and the **additive `operator_expiry_authorized` term in its terminal
  predicate** — consuming the authorization is necessarily a change to #625's behaviour
  (`0016:2209-2215` defines terminality that way), and a scope fence that forbade it would make
  criterion D unimplementable. Everything else in that loop is untouched.
  / out of scope: the **reaper loop's windows, its clock guard, its progress observable and its
  teardown mechanics** (#625, this bundle's prerequisite — beyond the two named edits above);
  #636's **record classes and state machine**, and #508's **S3 client-facing verbs** — this slice adds
  two *management* verbs and exactly one additive field to an existing record, and nothing else; **FU-2** urgent
  operator-forced staged evacuation; **FU-3** drain/admission telemetry and alerting (#630) —
  this slice ships *one* alarm, not the observability surface; **FU-4** surfacing the abandonment
  reason in the S3 error text; **any S3 wire-visible verb** — these are operator verbs, not
  client-facing ones; the **proposal-0008 management API / auth surface** (Milestone 8); any file
  under `docs/design/adr/` or `docs/design/specs/`, and any edit to 0016 itself; and **any change
  to #636's or #625's behaviour beyond the three this brief names** — the additive session field,
  the alarm emission in the reaper's foreign-clock skip branch, and the additive
  `operator_expiry_authorized` term in the reaper's terminal predicate. Anything else is a Check §6
  item to raise, not a silent edit to a layer below.
- **Repro instruction:** on this bundle's base (`origin/pdca-integration/main`, i.e. `main` +
  #634 + #635 + #636 + #625). *For the CLI legs (server crate, redb is a dependency there):*
  create a temp dir, open `RedbMetadataStore::open(dir.join("meta.redb"))`, seed the
  session records as **raw JSON bytes** (or through #636's public helpers where they are `pub`) —
  an `Open@E` session with parts, a `Completing@E` session with `seg:<g>:<E>:*` records **and** a
  decoy `seg:<g>:<E+2>:*` range, a `Completed` session with a foreign `clock_source`, and
  `mpuctl` — then **drop the store** (redb holds an exclusive file lock) before invoking the CLI.
  Legs A–D: run the **real binary as a subprocess** —
  `Command::new(env!("CARGO_BIN_EXE_wyrd"))` (cargo provides that env var to integration tests of
  the crate declaring `[[bin]] name = "wyrd"`) — with the verb's argv, then reopen the store and
  scan the prefixes. A subprocess is required, not stylistic: it is what makes the **exit status**
  and **stderr** of criterion C observable, and it sidesteps both the nested-runtime panic (the
  roles build their own runtime and `block_on` it) and redb's exclusive file lock. Wait on the
  child with a bounded timeout. On the base the command is unknown, exit **2**, stderr
  `unknown command`, and every record exactly as seeded. Leg E: build the reaper context #625
  landed over the custodian suite's **in-file in-memory** `MetadataStore` (never redb — that crate
  has no redb dependency) and call `reconcile_step` under a capturing subscriber, mirroring
  `crates/custodian/tests/gc_telemetry.rs`; on the base the pass skips the foreign session
  silently. Every wall-clock read the tests make needs the annotation the workspace lint demands
  (bare `SystemTime::now()` is denied by `clippy.toml`, wyrd#619 / `63d66b9` — state which clock
  owns the read).
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and should mirror:
  **the CLI dispatch and its conventions** — `cli::run`'s match arms and `usage()`
  (`crates/server/src/cli.rs:399-435`), the handled-error vs unknown-command exit paths
  (`:419-423` vs `:429-433`), `ParsedArgs` (`:2247-2284`) and its `VALUELESS_FLAGS` allowlist
  (`:2244`), and `cmd_custodian`'s **stray-positional refusal** and its "a value after a
  valueless flag does NOT disable it" rationale (`:913-930`) — an operator verb that silently
  mis-parses is the failure class that comment exists for.
  **the fence shape to reuse** — the `Completing → Aborting` restore-fence path #636 landed
  (0016's "one shape for all three doors", `0016:665`), and the exact-bytes precondition
  discipline of `WriteBatch::require` (`crates/traits/src/lib.rs:825-843`).
  **the reaper's skip path and terminal predicate** — the module #625 landed under
  `crates/custodian/`, dispatched from `reconcile_step`
  (`crates/custodian/src/reconciliation.rs:65-115`). Read it: this slice adds the alarm inside its
  existing skip branch and the authorization term to its existing terminal predicate — it does not
  add a second pass.
  **alarm/audit emission** — `emit_strand` and its siblings
  (`crates/custodian/src/restore.rs:433-470`, `target: "wyrd.custodian.restore.audit"`), and the
  `DurabilityTelemetry` seam (declared `crates/telemetry/src/lib.rs:79-84`) with its in-process
  read-back `gather_prometheus` (`:170-180`).
  Follow whichever of the two #625 used for its pass outcomes so the operator sees one plane.
  **serialization identity** — the repo's standing rule (`AGENTS.md` *Serialization identity*)
  and #636's own session CAS paths. Do **not** repeat 0016's claim at `:475-491` that
  `renew_pending` re-encodes its prior: it preconditions on the raw bytes it read
  (`crates/core/src/metadata.rs:748-760`, `live_lease_guards` likewise at `:786-795`).
  **test capture pattern** — `crates/custodian/tests/gc_telemetry.rs:1-33` (own binary, capture
  layer, `gather_prometheus` read-back).
- **Prior-art check (triage cycles):** searched by affected file path across merged history and
  closed/rejected work on 2026-07-25 at `22d71b4`. `crates/server/src/cli.rs` carries six
  subcommands (`put`, `get`, `d-server`, `custodian`, `s3`, `demo`, `:412-428`) and **no**
  multipart or session-management verb; its last touches are #616/#619/#551/#527, none related.
  `crates/custodian/` has no reaper and no foreign-clock path (that arrives with #625, this
  bundle's prerequisite). No management API exists — proposal 0008 (draft, Milestone 8) states
  the current surface is "a hand-rolled dev-only CLI". No open or closed PR implements an
  operator session-abort or expiry verb; this issue was filed from #626/PR #627 (a docs-only
  proposal). Result: **no prior art; net-new.**
- **Disposition hint:** likely-fix

## Motivation

The reaper declining to judge a session is the *safe* direction — reaping on a producer's stamp
when producers do not share the reconciler's clock is the #557 defect class, which this
repository has already paid for once. But declining to judge is only safe if some **other** actor
can judge. Today that actor would be nobody: a foreign-clocked session keeps its records, its
staged bytes and its admission slot forever, and a deployed producer mismatch or a legacy record
can create these in bulk. `MAX_SESSIONS` of them is a permanent `503 SlowDown` on
`CreateMultipartUpload` with no in-system exit at all. The operator is the judgment the clock
cannot supply — and an operator who is never told is not an exit either, which is why the alarm
is part of this slice rather than a follow-up.

## Design

### The two verbs are not the same verb

0016 is explicit that **abort does not apply to a terminal session** (iteration-9 finding 11): a
`Completed` session answers `404` to `AbortMultipartUpload`, and making the operator verb an
exception would give the operator a way to "abort" a session whose object is already published.
The terminal case therefore gets its **own** verb whose meaning is narrower and honest: *expire
this tombstone whose window I cannot judge*. It authorizes; the reaper still performs the delete,
under the preconditions it already enforces.

### Why the `Completing` door must retire its segments in the same batch

A `Completing@E` session's segment records `seg:<g>:<E>:*` are durable but **unadopted** — no
inode names them. If the operator abort fences the session without installing
`retire:records:{seg:<g>:<E>}` in the same batch, there is afterwards no rollback to write that
obligation (the session is past `Completing`) and no inode to adopt them: they accumulate with no
deleter. That is the X57 hole, and it is why 0016 gives all three `Completing → Aborting` doors —
the restore fence, the `W_session` ceiling and this operator abort — **one shape**
(`0016:665`). The obligation names **that fence epoch only**, so a rolled-back attempt's
obligation can never delete a later attempt's published segments (F18).

### The authorization is a record, not a flag in the CLI's head

The expiry verb runs in a short-lived process; the terminal delete runs in the reaper, possibly
much later, under preconditions (`sidx:` observed empty, no session-scoped `retire:` obligation,
exact-bytes CAS) that exist to make the `mpuctl.count` decrement exactly-once. So the operator's
decision must be **durable and readable by the reaper** — an additive optional field on the
session record, set by a fenced CAS, consumed by the reaper's terminal predicate, discarded with
the record. Its `skip_serializing_if` omission follows the repo's standing serialization-identity
rule and #636's own re-encoding CAS paths — not the mechanism 0016 attributes to `renew_pending`,
which preconditions on raw stored bytes.

### Where these verbs live, for now

Proposal 0008's management API + thin CLI is Milestone 8 and unbuilt; 0008 itself names today's
dev CLI as the only binary surface. These verbs therefore land as CLI subcommands on the existing
`wyrd` binary, following its conventions (flags only, stray positionals refused loudly, explicit
`--data-dir`/backend selection). When M8's management plane lands, they migrate to it — the
records and the semantics do not change, only the transport. **This placement is a deviation from
0008 and needs the maintainer's explicit exception: see Open question 1.**

## Alternatives considered

- **Let the reaper judge a foreign-clocked session anyway** (trust the stamp). Rejected upstream:
  it is the #557 defect class — a future-epoch stamp defers cleanup indefinitely and an older
  epoch reaps a live upload.
- **One verb for both cases.** Rejected: abort of a terminal session is a category error and
  would let an operator "abort" a published object's session; the two decisions have different
  preconditions and different blast radii.
- **The expiry verb performs the terminal delete itself.** Rejected: it would duplicate — and
  therefore eventually diverge from — the empty-`sidx:` gate, the obligation-drained gate and the
  exactly-once decrement, in a process with no leadership fence.
- **Ship the verbs on the first operator report** (the original FU framing). Rejected in 0016's
  own implementation order (point 3, iteration-8 finding 3): the state is absorbing from the
  moment the guard ships, so the verb and the alarm ship *with* the guard.
- **Alarm only, no verbs.** Rejected: an alarm with no remedy is not an exit.

## Impact & compatibility

- **Additive only on disk**: one optional session-record field, omitted when absent. A store
  written by the prerequisite slices reads unchanged, and a record this slice writes is readable
  by them (the field is ignored by anything that does not know it — but the round-trip identity
  test is mandatory).
- **Two new CLI subcommands** and one new alarm on the existing durability/audit plane — hence a
  docs-currency obligation in the same PR, and a runbook line telling the operator what to run
  when the alarm fires.
- **No S3 wire-visible change.** A session an operator aborts becomes `Aborting`, which the
  existing verb × state table already answers as `404 NoSuchUpload`.

## Open questions

1. **SETTLED — interim-surface exception GRANTED (Eduard Ralph, 2026-07-25).** FU-6 assigns
   these operations to the **proposal-0008 management surface**, and 0008 is emphatic that the
   management API is the source of truth, that the thin CLI carries **no** management logic and is
   distinct from today's bespoke dev parser, and that the plane authenticates with OIDC + mTLS and
   operator RBAC (`docs/design/proposals/draft/0008-management-and-administration.md:111-118`,
   `:144-151`, `:153-190`). This slice nonetheless puts two **destructive** verbs on the
   unauthenticated dev CLI, because 0008's plane is Milestone 8 and does not exist while FU-6 is a
   hard blocker on #625. The maintainer's ruling, verbatim: *"deviation granted as we can't wait
   for 0008."* The grant carries three terms, which are Scope obligations, not suggestions:
   **(i) authority is host access** — the verbs open **no** network surface, no listener and no
   new auth path; whoever can run the binary against the data dir is already trusted with it;
   **(ii) no bulk form** — each invocation names exactly one `--upload-id`, never a wildcard or
   "abort all" (a bulk form waits for the authenticated plane, Open question 3);
   **(iii) migration is owed to M8** — the architecture-doc entry this slice writes records that
   these verbs move onto the management API when it lands, semantics and records unchanged, only
   the transport. Do implements against the dev CLI and does **not** wait for, or partially build,
   a management plane. This closes the one blocker cross-vendor plan review left open
   (`results/plan-review-codex-633-r2.md` F7).
2. **Verb spelling — SETTLED in Scope** (`wyrd mpu abort` / `wyrd mpu expire`, both
   `--upload-id` + `--data-dir`), because the added test must hard-code it. The maintainer may
   rename at sign-off — it is a one-line change across code, test, runbook and doc, not a
   redesign — but Do does not choose it.
3. **Blast radius of a bulk mishap — SETTLED as term (ii) of the grant.** These verbs destroy an
   in-flight upload by design, so v1 takes exactly one explicit `--upload-id` per invocation:
   never a wildcard, never "abort all". A bulk form waits for the authenticated management plane.
   Say so at sign-off if you want the bulk form sooner — it widens the blast radius of the very
   surface the exception was granted for.
4. **Alarm plane.** If #625 emitted its pass outcomes on the metrics seam, put the alarm there
   too **and** as an audit event; do not split the operator's view across two planes.

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle (useful for CI feedback). The PR MUST NOT be marked ready before sign-off
accepts, and this slice MUST be merged as part of the same stack as #625 — never left behind it.
