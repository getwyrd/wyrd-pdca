## Summary
**User impact:** a multipart upload whose part numbers are spread far apart could
get stuck. Every attempt to complete it failed the same way, and the only way
out was to abort it and lose the uploaded parts. Retrying never helped.

This PR makes Complete check the size up front and answer with a clear "too
large" refusal, before it changes anything, instead of failing on every retry.

**Stacked PR.** It builds on #1050, #1059, #1063 and #1064, which are still
open. Review it after those; the diff below only applies on top of them.

## What to look at
- The size check in Complete's validation step, and the small type that builds
  the two part-set records once so the same bytes are checked and then written.
- The new test file, `crates/core/tests/multipart_complete_value_ceiling.rs`.
  It runs Complete over an in-memory store wrapped to refuse any value larger
  than the real store limit, the same way the existing restore-fence test does.
- To try it: `cargo test -p wyrd-core --test multipart_complete_value_ceiling`.
  With the `crates/core/src` part of the change reverted, 4 of its 9 tests fail
  with the stopped error described above.

## Root cause
When Complete publishes an object, its single commit writes the published parts'
numbers and the left-out parts' numbers each as one stored value. The part
numbers are range-encoded, so sparse numbers (1, 3, 5, …) cost bytes per part.
Past about 7,936 such parts a set goes over `MAX_VALUE_BYTES` (100,000), and the
store refuses that commit for good, so Complete failed the same way every time.

## Fix
- `RetireObligation::value()` (`crates/core/src/multipart.rs:4382`) returns the
  exact bytes stored for a retirement record. The publishing commit puts these
  bytes, and the check measures them.
- A new private `PartRetirement` (`crates/core/src/multipart/complete.rs:1150-1201`)
  builds the published-parts and left-out-parts records. Its `past_ceiling()`
  (`:1192`) returns `Refusal::SessionTooLarge { bytes, limit: MAX_VALUE_BYTES }`
  for the first one over the limit. A value exactly at the limit fits.
- `validate` (`complete.rs:1116-1148`) builds it and refuses on it. `validate`
  already runs twice: before the fence (`:961`, the refusal returns with nothing
  written) and against the set frozen behind it (`:1011`, the refusal goes
  through the existing release path, which returns the session to `Open` with
  its attempt count restored).
- The fence carries the measured records (`HeldFence`, `:419`), and the flip
  installs those instead of building its own (`:1836-1837`).
- Docs: the `SessionTooLarge` doc (`multipart.rs:4941-4964`) now says Complete
  answers it and what `bytes` means there. The `PartNumberSet` doc no longer
  claims the worst case fits in one value (it is 128,901 bytes for 10,000
  alternating parts). The `complete.rs` module doc gains a "value ceiling"
  section. `docs/design/architecture/05-building-block-view.md:252,254` is
  updated to match. Stale "#1011 still to land" pointers are removed.

No new `Refusal` variant, no change to `UploadPart` or the part-number range, and
no format change. Nothing calls Complete in production yet; the S3 verb comes
with #670. For that mapping: this refusal depends on which parts the request
names, not on the session as a whole. A different list can publish the same
session (for example, naming a part that joins two ranges), so #670 should not
tell the client that abort is the only way out.

## Verification
Line numbers are on this branch (the stack above with this patch applied);
`MAX_VALUE_BYTES` is at `crates/core/src/metadata.rs:577`.

- **Claim:** a set that fits publishes. Within part numbers 1 to 10,000 nothing
  is refused.
  - **Checked:** I1 (`multipart_complete_value_ceiling.rs:482`): parts
    1..=10,000, odd ones named. It publishes, and the stored records are 58,901
    and 58,909 bytes. 58,909 is the widest possible set in that range.
- **Claim:** an oversize request is refused with zero commits, and the session
  stays usable.
  - **Checked:** I2 (`:503`): 10,000 odd parts, all named. It is refused with
    `SessionTooLarge { bytes: 128901 }`, no batch is sent (`:513`), the session
    bytes are unchanged, and a later abort fences it.
  - **Checked:** I4-already-past (`:676`): the left-out set is the oversize one.
    It is refused with zero commits (`:691`).
- **Claim:** the limit applies to the stored value, not the bare set.
  - **Checked:** I3 inside (`:530`, set 99,981 B, stored 99,991 B) publishes.
    I3 at (`:562`, stored exactly 100,000 B) publishes. I3 past (`:580`, set
    99,995 B, stored 100,005 B) is refused with zero commits (`:591`).
- **Claim:** a part that arrives between the first read and the fence is caught
  against the frozen set.
  - **Checked:** I4 (`:630`): the wrapper adds part 15,871 just before the fence
    commits. It is refused with `SessionTooLarge { bytes: 100005 }` after exactly
    2 batches (fence, then release to `Open@2`, attempts 0), and no record or
    object is written. I4 control (`:611`) publishes without the extra part.
- **Test:** `crates/core/tests/multipart_complete_value_ceiling.rs` (new, 9
  tests). Pre-fix: 5 pass and 4 fail (I2, I3 past, I4, I4-already-past), each
  with the stopped error from the refused publish. Post-fix: 9 pass.
- **The test catches wrong fixes:** each of these mutants turns at least one
  test red. Using `>=` instead of `>` fails I3-at. Measuring the bare set fails 4
  tests. Checking only before the fence fails I4. Checking only behind the fence
  fails 3 tests. Checking only the published set before the fence fails
  I4-already-past. `cargo mutants` on the diff: 16 tried, 9 caught, 7 unviable,
  0 surviving.
- **Coverage:** the new test reaches 54 of 55 changed code lines. The one it
  misses (`complete.rs:1311`, the segment-batch release, now writing the same
  bytes through `value()`) is covered by the existing `multipart_complete`
  suite, which passes unchanged (85 tests).
- **Whole gate:** `cargo xtask ci` passes. `cargo fmt --check`,
  `cargo clippy -D warnings`, `typos` and the docs lint are clean.
- **Prior art:** #856 found the oversize part sets and fixed only the restore
  fence, leaving Complete's publishing commit. #908 added `SessionTooLarge` for
  Create, and this PR reuses that variant. No open or closed PR covers this.

Fixes #1011
