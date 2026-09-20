# PR description

## Summary
**User impact:** multipart uploads (and the background cleanup that reclaims
storage after them) have no agreed-on set of size/count limits yet, and when
these limits get picked one at a time instead of as a checked set, the
result can be a session that gets stuck permanently — it can never be
cleaned up because the cleanup work itself is too big to fit in one
transaction. That already happened once in production (see the "storage
lifecycle" note in this repo's principles doc). This change is what
prevents it from happening again for this seam.

This PR adds the actual numbers — how many chunks a part/segment/map can
hold, how many parts a session can have in flight, batch size and operation
ceilings, retry budgets — each with its derivation written down, plus one
function that checks a whole set of these numbers against each other and
says which check failed if any don't hold. Tracked as issue #655.

Nothing is wired up to real uploads yet: no upload is rejected or admitted
differently by this change. It only defines the numbers and proves they're
internally consistent. The pieces that actually use them land in later,
separate changes.

## What to look at
- `crates/core/src/multipart.rs`, the new "section 14" block at the bottom
  of the file: each `pub const` is one knob, with a doc comment explaining
  where its value comes from and what it depends on.
- `knob_clamps_hold` in the same file: the one function that takes a set of
  these numbers and returns `Ok` or "which check failed."
- `crates/core/tests/multipart_knobs.rs`: the test that exercises all of the
  above — worth skimming even without Rust experience, since each test name
  describes one check in plain terms (e.g. "every capacity fits its key
  space").

## Root cause
The numbers this multipart protocol depends on (part/segment size limits,
in-flight session limits, batch and retry budgets) were previously going to
be picked independently by whichever code needed them first, with no single
place checking that they're consistent with each other. Proposal 0016 sets
the valid range and the invariant for each number but deliberately leaves
the exact value to be chosen here, in one place, with one check across all
of them.

## Fix
`crates/core/src/multipart.rs` gains a block of named constants (one per
knob), each with a doc comment stating the formula behind its value and the
source it comes from, plus a `KnobSet` type and `knob_clamps_hold(&KnobSet)
-> Result<(), KnobClamp>`, which checks every inequality the numbers must
satisfy together and names the specific one that breaks, so a future value
change fails loudly with a message someone can act on. Two derived values
(`MAX_SESSIONS`, `MAX_OWNED_FLEET`) are computed from the others rather than
hand-picked, matching the rule that a derived number must never be a
separately chosen constant.

## Verification
- **Claim:** the shipped value set satisfies every consistency check.
  **Checked:** `crates/core/src/multipart.rs:4866` (`knob_clamps_hold`)
  called on `KnobSet::DEPLOYED` — asserted in
  `crates/core/tests/multipart_knobs.rs:166` (leg 1).
  **Test:** `cargo test -p wyrd-core --test multipart_knobs` — this is a new
  test file, so "pre-fix" means the crate as it stands on `origin/main`
  (`605b33a`), which doesn't have these symbols at all and fails to compile;
  post-fix all 8 tests pass.
- **Claim:** `knob_clamps_hold` actually rejects a bad value set, for every
  individual check — it isn't a function that just always returns success.
  **Checked:** `crates/core/tests/multipart_knobs.rs:181` (leg 2) — one row
  per check, each violating exactly one inequality and asserted to fail
  under that check's specific name.
  **Test:** same file; as extra evidence, `build-notes.md` §6 in the
  original bundle records the function temporarily short-circuited to
  always return `Ok`, which turns 4 of these 8 tests red.
- **Claim:** the derived numbers match the formulas in the design proposal,
  not a re-typed or drifted copy of them.
  **Checked:** `crates/core/tests/multipart_knobs.rs:274,305,332` (leg 3) —
  each derived constant recomputed independently from the stated formula
  and compared against the shipped constant.
- **Claim:** every capacity number fits inside the key space it will be
  addressed by (so a value can never overflow the key format that has to
  represent it).
  **Checked:** `crates/core/tests/multipart_knobs.rs:355` (leg 4).

Tracked as issue #655; the repository's own tracker link pattern isn't
configured, so no clickable link is included above — see the commit
trailer below for the machine-readable reference.

Fixes #655
