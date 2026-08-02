# Adversarial review — issue 636 (multipart commit protocol)

Attempted to refute: the ETag composition (leg B), the subset publish + evidence ordering
(leg B), the separated retry budgets (leg F), the admission counter's exactness (leg E), the
slot-space cap and the publication-CAS/version rule (DST i/ii), the fence-then-walk teardown,
and the `PendingEntry` round-trip identity — **could not**. Those legs drive the production
verbs, assert durable store state, and I could not construct an input that breaks them. Four
attacks did land; all four were reproduced by building and running the suite in a scratch clone
(`cargo 1.96.0`; both new test binaries green as shipped before each negation).

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:3423-3425`: `next_session_part` throws
  its cursor away and asks for `usize::MAX` rows, so the session drain silently truncates the
  moment a page comes back short.** `cursor.as_deref().map(|_| &[][..])` discards the computed
  key and passes `Some(b"")` (which `page_start` resolves to "start of prefix"), the limit is
  `usize::MAX`, and the returned `next` is dropped — so every call re-reads the *whole*
  `part:<id>:` range from row 0 and treats one page as the complete range. The seam forbids
  exactly this: `crates/traits/src/lib.rs:1054-1056` makes `next` the sole authority for
  exhaustion, and `:1069-1073` says a `limit` above the store's effective cap is **clamped**,
  never an error. Concrete failing case: a store built with `with_scan_cap(1_000)` (a supported
  inherent knob on all three backends — `crates/metadata-redb/src/lib.rs:86`,
  `crates/metadata-tikv/src/lib.rs:1083`, `crates/metadata-fdb/src/lib.rs:1336`) hands back
  1,000 rows; the marking walk then reports "no unit at or after 1001", declares marking
  finished, and the deleting phase removes **all 4,001** `part:`/`psum:` records anyway.
  Reproduced by changing only that limit to `1_000` in a scratch copy:
  `g_a_partially_drained_teardown_converges_with_no_double_decrement` fails at
  `crates/core/tests/multipart_admission_and_drain.rs:771` with `left: 1000, right: 4001` —
  3,001 fragments' bytes left unreferenced **and** unevidenced, which is refutation outcome (a)
  and the "truncated derivation marked fully drained = silent permanent part loss" defect the
  brief says a rejected attempt already shipped. Nothing but the accident that
  `SCAN_CAP (2^20) >= MAX_PARTS_PER_SESSION (10_000)` keeps this latent at default settings.
  Two further consequences of the same line: the doc at `:3408-3410` claims the range is read
  "one row at a time", but each unit materializes every `part:` record **with its values** (up
  to 10,000 × ~50 KB at `MAX_PART_CHUNKS`), which on TiKV/FDB is one transaction per unit
  against a 5 s deadline — a large teardown that cannot complete, permanently; and the walk is
  O(N²) — replacing it with a real exclusive cursor (`part_key(id, from - 1)`) and `limit 1`
  keeps **all 18 tests green** and drops that one test from 38.7 s to 27.5 s, i.e. the suite as
  written cannot see the difference.

- **NEEDS-HUMAN [human] — `crates/core/src/metadata.rs:2934` and `:3050`: the live
  DELETE/overwrite path stops writing reclamation evidence, and nothing in this repository
  drains the obligation that replaces it.** `unlink` and `commit_chunk_map_superseding{,_leased}`
  now route through `retire_generation`, and `generation_retires_inline`
  (`crates/core/src/metadata.rs:486-500`) keeps the inline `orphan:` fan-out only up to 250
  marks — i.e. 27 chunks at the server's default RS(6,3) + 1 MiB chunk
  (`crates/server/src/lib.rs:48`,`:100`). Reproduced: a 30-chunk (270-fragment) object deleted
  through the production `metadata::unlink` — the call `crates/server/src/lib.rs:582` makes on
  every DELETE — commits, writes **0** `orphan:` marks, installs one `retire:bytes:g:42:1`
  obligation, and the fleet still holds all 270 fragments. `crates/custodian/src/gc.rs:150-198`
  reclaims only from `orphan:` marks or expired `pending:` leases and otherwise retains
  conservatively; it has no knowledge of `retire:` (that is #637), and `grep` finds **no
  non-test caller** of `drain_obligation`/`drain_step`/`teardown_session` anywhere in
  `crates/` or `xtask/` (the loop is #625). So from the instant this merges until #625 lands,
  every DeleteObject and every overwrite above ~28 MiB leaks its bytes permanently. That
  contradicts the brief's "**Client-visible: nothing.** No verb reaches this code until #508"
  and the code's own claim at `crates/core/src/metadata.rs:2929-2932` ("the only thing that
  moves is *when* the grace clock starts — at drain, never earlier than today"): with no
  drainer, the grace clock never starts. It is also the exact half of this slice's stated
  invariant that forbids "unprotecting without evidence". The human call — ship with a tracked
  merge-order obligation extended from #508 to *this* slice, keep the inline route until the
  reaper exists, or have some existing custodian pass drain `retire:` — is a scope/fitness
  decision, not something Do can settle by iterating.

- **NEEDS-HUMAN [impl] — `crates/core/tests/multipart_admission_and_drain.rs:735-753`: the test
  named `..._with_no_double_decrement` cannot detect a double decrement.** Its fixture creates
  exactly one session, so `before == 1`, and `terminal_delete`'s
  `admission.count.saturating_sub(..)` (`crates/core/src/multipart.rs:4136-4139`) clamps at
  zero: `before - 1 == 0` is satisfied by a decrement of 1 **or** 2. Reproduced by negating the
  production line to `saturating_sub(2)` — that test stays green (leg E,
  `crates/core/tests/multipart_protocol.rs:1168`, does catch it, so the property is covered, but
  not by the test that claims it). Two sessions in the fixture, or an assertion that the ledger
  moved by exactly one from a count > 1, restores the teeth the name promises.

- **NEEDS-HUMAN [impl] — `crates/core/src/multipart.rs:4232-4234` and `:4440-4441`: the leg-H2
  helper counts an undrained `retire:bytes:` obligation as a safe class, so the no-gap invariant
  passes over precisely the fragments of the finding above.** `FragmentClass::ObligatedForRetirement`
  is honest about 0016's overlap rule but is not a class any production consumer acts on, so
  `assert_no_classification_gap` (`crates/core/tests/multipart_protocol.rs:392`) reports
  "sound" for a store in which 270 fragments are unreferenced, unmarked and unreachable by GC.
  Two neighbouring arms widen it further: `:4363-4375` marks *every* inventory fragment sharing
  a chunk id as staged **regardless of D server**, and `obligated` at `:4440` is keyed by chunk
  alone — so a genuine gap on a sibling position is absorbed. The helper is run after every
  scenario in legs A–H, so its permissiveness sets the ceiling on what any of those legs can
  catch.

- **NEEDS-HUMAN [impl] — `crates/dst/tests/concurrency.rs:738` and `:867` assert a mark is
  "written when absent and SKIPPED when present, never re-stamped", which is not what
  `mark_fragment` does.** Production replaces the stamp on the present arm under
  `require(key == prior)` (`crates/core/src/multipart.rs:3582-3585`, documented at `:3556-3562`),
  and `crates/core/tests/multipart_protocol.rs:1993` asserts exactly that re-stamp. The DST
  `puts == 1` assertion holds only because the losing drainer's batch conflicts *whole*, not
  because anything skips — so the stated rationale is wrong, and if the present arm ever became
  reachable from `reclaim_owned` the DST leg would fail on correct behaviour. Same class, one
  file over: `crates/server/src/lib.rs:565-575` still tells the reader that `unlink` "writes an
  orphan grace record for each fragment in the *same atomic batch*", which is now false for
  every object above the inline threshold (AGENTS.md docs-currency rule).

**On the evidence itself (no bullet, pre-declared).** C4-verify's PASS is not evidence here and
the brief says so: the red is a build failure on a net-new module, and `run-verify.sh` scores
that as red without counting a single test. The compensating obligation — the F/G/E
mechanism-negation runs — lives in `build-notes.md`, which no gate reads and which this leaf is
not given, so I verified what I could myself: leg E does catch a doubled decrement (above),
and leg G's orphan-count assertion does catch a truncated marking walk — but only because the
in-test `MemMeta` has no scan cap, which is the first finding. `check-gates.json` already
carries C5 as `unverifiable` (7200 s timeout) and T4 as `fail` (38 blocking), so nothing here
rests on a green I did not re-run.
