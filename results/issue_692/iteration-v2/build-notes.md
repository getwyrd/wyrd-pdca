# Build notes — issue #692, multipart record family + validating decoders (654 split 2/3)

**Iteration 2.** Base `origin/main` @ `9dbcd72` (the post-wave-merge tip; the brief's verified
base `339da46` is an ancestor — `git merge-base --is-ancestor 339da46 HEAD` → yes). Every
`path:line` below is against the worktree `/home/eddie/wyrd/wyrd.pdca-wt-l0` **with the patch
applied** (i.e. the lines a reviewer sees in `patch.diff`).

The v1 attempt's shape was accepted (C1/C2/C3/T1/T2 PASS). This round keeps that shape and
**fixes the six recorded defects** the sign-off carried forward, plus the one gating typo. I
did not re-submit anything rejected unchanged; §1 is the disposition table.

---

## 1. Carry-forward disposition — every item, its fix, and its evidence

| # | Carried-forward finding | Fix | Evidence |
|---|---|---|---|
| 1 | **C4 gating**: `typos` — `entrys` at `multipart_records.rs:533` | test renamed `an_owned_entry_staged_placement_length_decodes_liberally_contextual_not_structural` (`crates/core/tests/multipart_records.rs:678`) | `xtask ci` green ×3 (§5) |
| 2 | **C5 mutant survived** `multipart.rs:1020` — `replace / with * in Budget::max_sessions` (the `SCAN_CAP/2` clamp) | clamp extracted to `scan_half()` (`multipart.rs:903-914`) and **pinned** by a profile whose unclamped quotient is 1,000,000 (`multipart_records.rs:377-387`) | mutants: 0 missed (§5); negation 08 (§4) |
| 3 | **C5 mutant survived** `multipart.rs:1844` — `delete ! in RetirePayload::validate` | the `Generation` arm now has **both** guards, and all four (chunks×segments) combinations are asserted (`multipart.rs:1936-1948`, `multipart_records.rs:757-786`) | mutants: 0 missed; negation 07 |
| 4 | **T5 judgment [impl]**: a generation obligation with neither chunks nor segments was **accepted** | rejected at decode — `"…names neither a flat chunk list nor a seg: generation: it owes nothing"` (`multipart.rs:1942-1947`) | negation 07 (§4) |
| 5 | **T4 batch review** `multipart.rs:931` + `:946` (BUG + CONVENTION, same class): `Budget::new` accepted out-of-range / relationally invalid profiles | five range rules from 0016's knob table enforced at construction **and therefore at decode** (`multipart.rs:1016-1057`) | negation 09; `budget_rejects_a_profile_outside_the_ranges_0016_settles` |
| 6 | **T4 batch review** `multipart.rs:1000` (BUG): saturating `U_ref` to `u64::MAX` still admitted one session at `W_ref = u64::MAX` | `U_ref` is now **exact in `u128`** (`u_ref_of`, `multipart.rs:912-938`) and `Budget::new` refuses `W_ref < U_ref` (`:1052-1057`) — the reviewer's exact tuple is refused | negation 10; `budget_refuses_the_profile_whose_true_footprint_overflows_its_own_budget` |

All three batch-review findings are **fixed**, not recorded-rejected — no
`review-rejected.md` is needed.

### Why the range rules are in scope (they look like "knob values", which are #655's)

The brief scopes the *shape* of the profile tuple here and its *values* to #655. What I added
is neither: it is the **valid ranges 0016 itself settles** (`0016:1463-1480`, "*Only a knob
whose entire range is safe is the implementer's freely*"), each a relation among the tuple's
own components or against a **format** constant:

* `MAX_INFLIGHT_PARTS ≤ MAX_PARTS_PER_SESSION` — `0016:1476`, iteration-13 finding 2;
* `MAX_STAGED_CHUNKS ≥ MAX_PART_CHUNKS` — `0016:1468`, the range's stated lower end;
* `MAX_PARTS_PER_SESSION ≤ MAX_PART_NUMBER` — the key grammar's own format bound
  (`multipart.rs:293-299`, child-1's); a knob *below* it is capacity, one *above* it names
  `part:` records no parser can read back;
* `MAX_INFLIGHT_PARTS × MAX_PART_CHUNKS ≤ SCAN_CAP/2` — `0016:1476`'s bounding invariant (the
  per-session owned `sidx:` range *is* this product, and the terminal-delete gate is a walk of
  it);
* `W_ref ≥ U_ref` — `0016:1479`, the stated range `[U_ref, deployment RAM budget]`.

No deployment number is written into the code: every bound is either another field of the same
tuple or a compile-time constant already in the tree. The two clamps 0016 states that this pure
tuple *cannot* see — `⌊(E_tx/2)/(bytes per slot key)⌋` and `B_ops` — are documented as #655's at
`multipart.rs:991-993` rather than silently dropped.

`SCAN_CAP` is a compile-time store-seam constant (`crates/traits/src/lib.rs:286`), not a
deployment knob, so checking against it does not violate 0016's "decode validates against
FORMAT maxima, never live knobs" rule (`0016:390-402`); that is stated at `multipart.rs:906-910`.
v1 already made the record's decodability depend on it (leg 1a compares the stored
`max_sessions` against the derived one, which includes the clamp), so the exposure class is
unchanged — it is now merely explicit.

---

## 2. What else changed, and what I deliberately did not change

**Compaction of v1 material** (to partly absorb the carry-forward's new lines — see §6):

* `SessionRecord`'s decode had a `required` table and a `forbidden` table (two matches, ~44
  lines). They express **one** rule — a state-scoped field is present *iff* its state defines
  it — so they are now one table (`multipart.rs:1391-1424`). Faithful: `Completing` defines
  `fenced_at_millis`/`segments_written`/`publish_target`, `Completed` defines `completion`,
  `Open`/`Aborting` define none, which is exactly the old pair of tables. This rule had **no
  test at all** in v1 (only leg 1c's `publish_target` identity), so I added one:
  `a_session_carries_exactly_the_fields_its_state_defines` (`multipart_records.rs:479-521`).
* `SessionState::ALL` (`multipart.rs:1246-1254` in v1) was dead — no consumer in the tree.
  Removed.
* `OwnedEntry::from_pending`'s two unreachable torn arms merged into one `_` arm
  (`multipart.rs:1703-1714`). This *introduced* a new surviving mutant (`delete match arm
  (None, None)`, caught by the first mutants re-run) because the arms are now
  distinguishable only by message — so the legacy-value-under-a-`sidx:`-key leg now asserts
  the specific reason (`multipart_records.rs:608-612`), and the mutant dies.
* `decode_retire_obligation`'s two wrong-scope arms merged (`multipart.rs:2037-2044`).
* The test's five repeated 9-line `matches!(RecordError::Structural{…})` blocks became one
  `structural(result, record) -> String` helper (`multipart_records.rs:114-126`), which also
  **strengthens** each leg: it now pins the reason text, not just the variant.

**Not changed, deliberately:**

* No `docs/design/` edit. The rubric's *Docs currency* rule ("a change that … adds a persisted
  field updates the living architecture doc in the same PR") arguably reaches
  `PendingEntry.owner`/`staged`; the brief puts `docs/design/` explicitly out of scope and
  fixes the file count at exactly 11, and 0016 §1 already specifies both fields normatively
  (`0016:442-491`). The fields are inert until #657 writes the first `sidx:` record. **Flagged
  here for the human** rather than resolved unilaterally — if the reviewer disagrees, the fix
  is one doc hunk, not a rebuild.
* No `Cargo.toml`/`Cargo.lock` change; `wyrd-traits` was already a direct dependency of
  `wyrd-core` (`crates/core/Cargo.toml:16`), so the test's `wyrd_traits::SCAN_CAP` reference
  needs no dev-dependency.
* The occupancy boundary (leg 3) stays as the brief settled it: `count > max_sessions`
  **decodes** (`multipart.rs:1161-1171`, `multipart_records.rs:809-822`).

---

## 3. The three refutation questions (forced, answered with evidence)

**(a) Genuine red? YES — ten times over.** Reverting *production* while keeping the test does
not even compile (the test calls APIs the patch adds), which is the brief's pre-declared
UNVERIFIABLE-RED posture. So I ran the stronger, per-check experiment the brief binds me to:
ten separate runs, each deleting exactly one production check and running the project's own
gate runner. **All ten went red** (§4). Every one was restored byte-for-byte afterwards — the
regenerated `git diff` is byte-identical to the shipped `patch.diff` (verified twice with
`diff -q`).

**(b) Production path? YES.** The test drives the production symbols directly:
`wyrd_core::multipart::{decode_admission_record, decode_session, decode_slot, decode_part,
decode_part_summary, decode_owned_entry, decode_retire_obligation, encode_record, Budget, …}`
and `wyrd_core::metadata::{decode, encode}` for the `PendingEntry` legs. There is no stand-in,
no re-implementation, no mock: leg 2 asserts on the bytes `metadata::encode` actually produces
(`multipart_records.rs:795-802`), which is the same function every `require(key,
encode(prior))` CAS in `metadata.rs:1368-1391` calls. `cargo mutants --in-diff` independently
confirms the tests reach production: 83 of 83 viable mutants of the changed production lines
are **caught** — a test suite that did not drive production could not kill them.

**(c) Fixture includes the fault? YES.** Every relational leg is a **hand-authored torn
value**, not a curated-clean one: leg 1a builds the honest `mpuctl` JSON from the profile's own
getters and then sets `max_sessions = derived + 1`; leg 1b pairs a `sidx:` key for upload
`a1…` with a payload owned by `b2…`; leg 1c gives a `Completing` session a `publish_target`
naming a different parent/name; leg 1d files a generation payload under a session token, a
session payload under a generation token, and a generation payload under the *wrong*
generation's token; leg 1e feeds `{lease_expiry_millis, owner}` and
`{lease_expiry_millis, staged}` through **both** readings. Each leg also asserts the honest
sibling **decodes**, so a decoder that rejected everything would fail too.

---

## 4. Negation demonstrations (the brief's binding red evidence)

Method — one run per check, through the project's own runner: delete the check from
production, run `./engine/xtask.sh ci` (which fails fast at its `cargo test --workspace` leg),
capture, restore. Driver: `$PDCA_SCRATCH/pdca-builder-692-negations/negate.py`. All ten RED.

```
01-leg-1a-admission-max-sessions-vs-profile: RED (exit 1)
02-leg-1b-owned-entry-owner-vs-key:          RED (exit 1)
03-leg-1c-publish-target-vs-session-dirent:  RED (exit 1)
04-leg-1d-retire-payload-vs-key-token:       RED (exit 1)
05-leg-1e-pending-entry-torn-shape:          RED (exit 1)
06-leg-2-skip-serializing-if-identity:       RED (exit 1)
07-t5-generation-obligation-owes-nothing:    RED (exit 1)
08-c5-max-sessions-scan-cap-clamp:           RED (exit 1)
09-review-931-946-budget-range-checks:       RED (exit 1)
10-review-1000-w-ref-below-u-ref:            RED (exit 1)
```

**01 — leg 1a** (drop `wire.max_sessions != derived`):

```
---- a_torn_admission_record_disagreeing_with_its_own_profile_is_rejected stdout ----
panicked at crates/core/tests/multipart_records.rs:413:18:
expected a structural `mpuctl` rejection, got Ok(AdmissionRecord { count: 1, max_sessions: 290,
  profile: Budget { w_ref: 1000000, max_part_chunks: 8, max_parts_per_session: 400,
  max_inflight_parts: 32, max_staged_chunks: 4000 } })
test result: FAILED. 17 passed; 1 failed
```

**02 — leg 1b** (drop `entry.owner != key_owner`):

```
---- a_torn_owned_entry_disagreeing_with_its_own_key_is_rejected stdout ----
panicked at crates/core/tests/multipart_records.rs:434:18:
expected a structural `sidx:` rejection, got Ok(OwnedEntry {
  owner: UploadId("b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"), lease_expiry_millis: 500,
  staged: StagedPlacement { scheme: None, placement: [7] } })
```

**03 — leg 1c** (drop the `publish_target` dirent identity):

```
---- a_torn_session_disagreeing_publish_target_with_its_own_object_is_rejected stdout ----
panicked at crates/core/tests/multipart_records.rs:472:22:
expected a structural `mpu:` rejection, got Ok(SessionRecord { … object: "o", …
  publish_target: Some(PublishTarget { parent: 1, name: "not-o", fence_epoch: 1 }), … })
```

**04 — leg 1d** (drop the whole token-identity `match`):

```
---- a_retire_payload_disagreeing_with_its_own_token_identity_is_rejected stdout ----
panicked at crates/core/tests/multipart_records.rs:570:9:
expected a structural `retire:` rejection, got Ok((Session { upload_id: UploadId("c3c3…"),
  epoch: 5, part: None }, Generation { inode: 7, version: 3, chunks: [ChunkRef { … }],
  segments: None }))
```

**05 — leg 1e** (drop the torn-shape check in `metadata.rs`):

```
---- a_pending_entry_with_exactly_one_of_owner_or_staged_is_torn_under_both_readings stdout ----
panicked at crates/core/tests/multipart_records.rs:600:9:
{"lease_expiry_millis":500,"owner":"a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1"} decoded as a PendingEntry
```

**06 — leg 2** (drop `skip_serializing_if` from `PendingEntry::owner` only):

```
---- a_legacy_pending_value_round_trips_byte_identically stdout ----
panicked at crates/core/tests/multipart_records.rs:801:5:
assertion `left == right` failed: a legacy pending: entry does not re-encode to its stored bytes
  left:  {"lease_expiry_millis":1234,"owner":null}   (as bytes)
 right:  {"lease_expiry_millis":1234}                (as bytes)
```

That is precisely the permanent `Conflict` ADR-0047:38-50 warns about, reproduced.

**07 — T5 / the generation-owes-nothing check**:

```
---- a_generation_obligation_names_its_bytes_in_exactly_one_form stdout ----
panicked at crates/core/tests/multipart_records.rs:782:19:
expected a structural `retire:` rejection, got Ok((Generation { inode: 7, version: 3 },
  Generation { inode: 7, version: 3, chunks: [], segments: None }))
```

**08 — C5 / the `SCAN_CAP/2` clamp** (`max_sessions` returns the bare quotient):

```
---- max_sessions_is_clamped_to_half_the_scan_cap stdout ----
panicked at crates/core/tests/multipart_records.rs:386:5:
assertion `left == right` failed
  left: 1000000
 right: 524288
```

(The first attempt at this negation left a `let`-and-return and tripped `clippy::let_and_return`
before the test leg; re-run with the whole body replaced, output above.)

**09 — the batch review's `:931`/`:946`** (drop the four relational/format range checks):

```
---- budget_rejects_a_profile_outside_the_ranges_0016_settles stdout ----
panicked at crates/core/tests/multipart_records.rs:343:22:
expected a structural `mpuctl profile` rejection, got Ok(Budget { w_ref: 1000000,
  max_part_chunks: 8, max_parts_per_session: 32, max_inflight_parts: 33, max_staged_chunks: 4000 })
```

**10 — the batch review's `:1000`** (drop `W_ref < U_ref`):

```
---- budget_rejects_a_profile_outside_the_ranges_0016_settles stdout ----
panicked at crates/core/tests/multipart_records.rs:343:22:
expected a structural `mpuctl profile` rejection, got Ok(Budget { w_ref: 1511,
  max_part_chunks: 8, max_parts_per_session: 400, max_inflight_parts: 32, max_staged_chunks: 1000 })
```

The per-run logs lived in `$PDCA_SCRATCH/pdca-builder-692-negations/logs/` and were removed
with the rest of my scratch at the end of the beat (scratch discipline); the excerpts above are
verbatim from them. To reproduce any one: delete the named check from `crates/core/src/{multipart,metadata}.rs`,
run `./engine/xtask.sh ci` from the PDCA root with `PDCA_WORKTREE` pointing at the patched
worktree, and restore the file.

---

## 5. Gate evidence (all through the project's own runners)

| Run | Command | Result |
|---|---|---|
| ci-1 | `./engine/xtask.sh ci` | **PASS** — `xtask ci: all checks passed` (17/17 in `multipart_records`) |
| ci-2 | `./engine/xtask.sh ci` (after compaction) | **PASS** |
| ci-final | `./engine/xtask.sh ci` | **FAIL — unrelated flake**, see below |
| ci-final2 | `./engine/xtask.sh ci` (same tree, re-run) | **PASS** |
| mutants-1 | `scripts/mutants-in-diff` | 114 mutants, **1 missed** (`from_pending`'s merged arm — fixed) |
| mutants-2 | `scripts/mutants-in-diff` (final tree) | 114 mutants: **0 missed**, 83 caught, 31 unviable |
| ci-3 | `./engine/xtask.sh ci` (after the doc-reference polish, = shipped tree) | **PASS** |
| mutants-3 | `scripts/mutants-in-diff` (shipped `patch.diff`) | 114 mutants: **0 missed**, 83 caught, 31 unviable |

**The ci-final flake, for the record** (so nobody attributes it to this patch): `cargo test
--workspace` failed once in `wyrd-gateway-s3`,
`tests::the_id_handed_to_the_client_selects_a_server_side_row_even_when_refused`
(`crates/gateway-s3/src/lib.rs:4027`). That test asserts on a **process-global tracing
capture**, and the row it printed belongs to a different plane (`"target":
"wyrd.gateway.s3.auth"`) with a timestamp ~2 h before the run — i.e. it read another test's
rows, the classic shared-subscriber interleaving. `crates/gateway-s3/` is **not** one of this
patch's 11 files, and the same tree passed the identical command three other times. This is
exactly the case `pdca.toml`'s `confirm_gating_fail = true` exists for.

**C4-verify** (`./engine/scripts/run-verify.sh`, exit **77** = UNVERIFIABLE, pre-declared):

```
run-verify.sh: GREEN — cargo test -p wyrd-core --test multipart_records (fix applied)
running 18 tests
test result: ok. 18 passed; 0 failed; …
run-verify.sh: RED — cargo test -p wyrd-core --test multipart_records (production reverted, test kept)
error[E0432]: unresolved imports `wyrd_core::multipart::decode_admission_record`, … (21 symbols)
run-verify.sh: UNVERIFIABLE — the RED leg's cargo run failed (status 101) WITHOUT running a test
```

That leg is worth more than its exit code suggests: it applies `patch.diff` to a **pristine
`origin/main` worktree** (`../wyrd-verify`), so it independently proves the patch applies to
the real PR base and that the named test is green *there* — not merely in my edited tree.

---

## 6. Budget accounting (honest, and over)

Counted as the v1 reviewer counted (added lines that are neither blank nor comment-only):

```sh
git diff | grep -E "^\+" | grep -v "^+++" | sed 's/^+//' \
  | awk '{s=$0; gsub(/^[ \t]+/,"",s); if (s=="" || s ~ /^\/\//) next; c++} END {print c}'
```

| | code-bearing added |
|---|---|
| v1 (T1 PASS, "1,243 against the 1,250 budget") | 1,244 |
| **this iteration** | **1,389** across exactly **11** files |
| brief's budget | 1,250 |

**+139 over, and I am flagging it rather than hiding it.** Where it went: the six
carry-forward fixes are ~90 code lines of production (five range rules with operator-facing
messages, exact `u128` `U_ref`, the generation guard) and ~115 of test (the range table with a
paired boundary for every rule — needed, or an off-by-one mutant survives — plus the overflow,
clamp, state-field and legacy-under-`sidx:` legs). I clawed back ~60 lines by compacting v1
material (§2) rather than by weakening a message or dropping a leg.

What I considered and rejected, with the cost shown:

* **Shorten the operator-facing rejection messages.** 68 of the added code lines are
  string-continuation lines (`36` in `multipart.rs`, `32` in the test — countable with
  `awk '/^[ \t]*"/'`). Collapsing them would recover ~40 lines and put ~15 error messages over
  the 100-column house style, and four tests assert on message *text* (`"derived, never
  chosen"`, `"must agree with the key"`, `"its state defines"`, `"owes nothing"`) precisely so a
  mutant cannot swap one rejection for another. Not worth 40 lines.
* **Drop the `*Wire` struct + manual `Deserialize` pattern in favour of serde's
  `#[serde(remote = "Self")]`.** This is the big one: it would delete ~100 lines of duplicated
  field lists across six records (`SessionRecordWire` alone is 22 fields + a 14-field copy).
  Rejected because the container attribute is read by **both** derives, so `Serialize` would
  also become a remote (non-trait) impl and every `encode_record` call would stop compiling;
  the fix is a hand-written `Serialize` forwarder per record, which gives back ~30 of the 100
  and adds an idiom `metadata.rs`'s own `InodeRecordWire`/`PendingEntryWire` precedent does not
  use. A 70-line saving is not worth a bespoke serde idiom in the module a whole milestone will
  extend.
* **Move validation out of `impl Deserialize` into the `decode_*` functions.** Would save the
  six wire structs outright (~90 lines) and is wrong: `serde_json::from_slice::<SessionRecord>`
  anywhere else would then bypass every relational check, and leg 1e explicitly requires the
  rejection to be a property of `PendingEntry`'s **own** decode, under both readings.

---

## 7. Loose ends the human should weigh at sign-off

1. **Budget overage** (§6): +139 code-bearing lines over the brief's 1,250, all of it
   carry-forward work, partly absorbed by compaction. Sign-off's call.
2. **Docs currency vs. the brief's scope** (§2): a persisted field is added and no living
   architecture doc is touched, because the brief forbids it and the proposal already
   specifies the fields.
3. **C4-verify is UNVERIFIABLE by design** — the brief pre-declares it (born-at-tier posture
   (a)): with production reverted the test cannot compile, so the gate can give no RED verdict.
   The ten negations in §4 are the substitute the brief binds, and all ten are red.
4. **One flaky unrelated test** observed once (§5) — `wyrd-gateway-s3`'s global-tracing capture
   test. Not this patch's file, and green on three other runs of the same tree. Worth its own
   issue upstream; out of scope here.
