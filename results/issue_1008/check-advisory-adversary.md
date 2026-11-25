# Adversarial review: #1008 (flat publication, lost flips, bucket guard)

**Bottom line:** I could not break the flip's behaviour. All six of the brief's required reds go red when I re-apply them to the production code, and so does every extra mutation I tried except one (a branch nothing can reach today). One real finding: a coupling gap with #511's bucket deletion fence. The rest are notes.

## How I attacked it

I copied the patched target into scratch and ran `cargo test -p wyrd-core --test multipart_complete`: 59 passed. Then I re-applied each of the brief's demonstrated reds to `crates/core/src/multipart/complete.rs`, one at a time, and re-ran the tests:

| Red (brief) | Mutation | Leg that went red |
|---|---|---|
| `retire:records:` names every staged part | `published` built from `fence.frozen.keys()` (`complete.rs:1364`) | C (`multipart_complete.rs:3515`) |
| Version frozen at fence time | no target re-read after a lost flip (`complete.rs:1286-1289`) | F, F-exhausted, the read-fault leg |
| Generation obligation for an empty prior | drop the empty-flat early return (`complete.rs:1487`) | F2 |
| Bucket pin dropped | drop `.require(bucket_key, marker)` (`complete.rs:1431`) | H2 and A |
| Definite flip error answered `OperationAborted` with no release | `complete.rs:1255` | J |
| Unknown flip outcome treated as `Conflict` | `Err(unknown)` falls through to the lost-flip path (`complete.rs:1244-1248`) | G0-flip |

Extra mutations, all caught: checking the marker only for presence instead of exact bytes (H2's "recreated" case), no `require_absent` on obligations, no unnamed-parts obligation, no generation obligation (F and F3), ignoring a session that moved after a lost flip, no release on `R_PUBLISH` exhaustion, no release on H2, accepting a `part:` record that disagrees with its summary, `>=` instead of `>` in the flat-size check, and turning the flat-size check off.

The tests run the production path end to end: a real redb store, Create (#894), `upload_part` (#1027), the ordinary `write.rs` overwrite as the interloper, #653's committer, and `read_object`. None of it is a parallel copy of production logic.

## Findings

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/complete.rs:1429-1431` hand-writes the bucket pin (`.require(bucket_key, fence.bucket_marker)`). It does not reuse `require_bucket` at `crates/core/src/multipart/create.rs:698-705`, which says it keeps the ADR-0046 §4 precondition "in this one place" and carries `// deferred: #511 … #511 adds the fence's require_absent here`. The flip has no `#511` marker of its own. 0016:663 asks the flip for "the bucket-existence pair of ADR-0046 §4" (marker **and** deletion fence), and 0016's failure-mode table says Complete "MUST NOT strand an object in a deleted bucket". Concrete failing case once #511 adds the fence to `require_bucket`: DeleteBucket installs `bucket-deleting:<name>`; its emptiness scan finds no dirent (the multipart object isn't published yet); this flip, which pins only the marker, commits the dirent; DeleteBucket then removes the fence and the marker. The result is an object stranded in a deleted bucket. Create picks up the fix automatically and the flip doesn't. The lost-flip check at `complete.rs:1273-1281` also reads only the marker. It is unreachable today (nothing writes a deletion fence, and no DeleteBucket exists in `crates/`). The cheap fix is to make `require_bucket` `pub(super)` and call it from `FlipPlan::flip`, or at least to add a `// deferred: #511` marker at `complete.rs:1431` so #511 finds this caller.

## Notes (no action needed to accept)

- `complete.rs:146-147` says the publication stamp "is one reading, and it stamps both records the flip publishes: that field and the root's ADR-0047 `modified`." That is not quite true when the prior is stamped at or after `completed_at_millis`. `overwrite_modified` (`metadata.rs:2429`) then sets the root to `prior + 1`, while `Completion.completed_at_millis` and the answered `Publication` keep the raw stamp. Example: prior stamped 3_000, Complete at 2_500 gives root `modified` 3_001 and session 2_500. Lines 111-113 of the same doc state the ordering correctly. No test covers this case (F's interloper stamps 1_200, below `COMPLETED_AT` 2_500). The fix is to reword the doc. Both values come from one clock, so the one-clock rule holds.
- The committer-refusal branch at `complete.rs:1251-1252` (`downcast_ref::<PublicationError>` → `Stop::Publication`) has no test. I changed it to answer `Stop::EpochExhausted` and all 59 tests still passed. The "at the last version" case (`multipart_complete.rs:3908-3946`) asserts `Stop::Publication(VersionOverflow)`, but that value comes from the pre-check at `complete.rs:1398`, not from the committer. With current inputs the committer can only refuse on value size, which is #1011's deferred ceiling, so this is low value.
- Gate verdicts in `check-gates.json`:
  - **C5 "1 missed"** (`no_segments -> Default::default()`, `complete.rs:1309`) is a mutant that changes nothing: `WriteBatch::new()` is `Self::default()` (`crates/traits/src/lib.rs:1523-1525`). It is not a test gap.
  - **C4-diff-cov "fail"** means coverage was never measured, not that it was low. The patch can't apply to `origin/main` because the base holds #1007. My mutation runs above stand in for it on the flip's branches.
  - **C4-verify "PASS (green-only)"** is what the brief predicted for a base that already holds #1007. I reproduced the per-fix reds myself (table above), so the green-only verdict is backed by evidence, not just assumed.

## What I tried that did not break it

- Collisions on obligation tokens. Each fence epoch mints its own `s:<id>:<E+1>` tokens. Every other writer that could install one has to move the session off the fence bytes, so its batch and the flip exclude each other.
- The CAS on the prior failing forever because of re-encoding. `InodeRecord` uses `skip_serializing_if` on its optional fields (`metadata.rs:1708-1712`), so decode→encode is byte-identical.
- An interloper creating or deleting the key between the fence and the flip. The re-read switches between `Fresh` and `Supersede`, and the version is recomputed each time.
- A session moved by a rival, the reaper or a teardown during the flip. The flip's exact-bytes pin fails and the request is answered from the new state with no release (tested).
- Flip batch size. It stays within 0016's "≤ 4·V + O(1)". A value over the ceiling is #1011's (deferred).
- The flat-size check summing only named parts. Validation guarantees every named part is in the frozen set, and `FlipPlan::read` requires each record's chunk count to match its summary.
- A production caller sneaking in. `complete_upload` and `PublishRequest` appear only in `multipart.rs`, `complete.rs` and the test file.

<!-- pdca:leaf-complete -->
