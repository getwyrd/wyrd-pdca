# Correction — issue 406, post-iteration-6 (INV-1 read-arm leak)

**What changed (one logical fix):** the session read-your-writes checker
(`session_read_your_writes`) fabricated a definite absence from a determinate
non-404 failed GET. `version` is `None` for *every* non-200 read, so a determinate
403 / 409 / 412 / 416 GET fell into the branch labelled "determinate absent (404)"
and, with a standing `AtLeast(_)` obligation, returned `false` — a fabricated
own-write-lost from a read that observed nothing. This is the INV-1 fault-class the
file's own header says the surface-wide re-plan existed to close, reappearing in the
read arm. The read-arm test only used a 500 (guarded as indeterminate), so it stayed
green while the leak lived.

**Fix:** `crates/server/src/consistency_workload.rs`, GET arm — gate the definite
absence on `status == 404`, not on `version.is_none()`. Any other determinate non-200
read observed nothing → not counted a violation.

**Discriminating red added:** `crates/server/tests/consistency_workload.rs`, inside
`session_read_your_writes_guards_indeterminate_on_every_arm` — a `[PUT k v=1 @200 ;
GET k None @403]` history must be accepted.

## Evidence (red → green, on this box, base = feat/m4-production-metadata-backend)

- Fix applied → the RYW test passes; whole socket-free suite 8/8 green; full suite
  incl. the wire-driven concurrent test 9/9 green (loopback bind works here).
- Fix reverted, test kept → the new 403 assertion FAILS:
  `a determinate non-404 failed read (403) observed nothing — it must not fabricate
  a definite absence / own-write-lost`. So the test genuinely catches the leak.
- `cargo fmt -p wyrd-server --check` clean; `cargo clippy -p wyrd-server
  --all-targets` clean.
- `run-verify.sh` (C4-verify): **PASS — red without the fix, green with it.**
- `pdca gates 406` (whole-tree C4-ci + C4-verify): **overall pass** — C4-ci
  "xtask ci: all checks passed" (fmt/clippy/build/test/deny/conformance), C4-verify PASS.
  Ran on this box, not the review sandbox, so the full workspace incl. socket/Docker
  paths is green with the fix.

## §6 NEEDS-HUMAN — updated disposition

- **INV-1 read-arm leak (the substantive one)** — RESOLVED by this correction, with a
  named red→green. No longer a sign-off blocker.
- **C2 red pre-fix / C4 runtime / T3 runtime** — the full suite incl. the wire leg ran
  green in the verify worktree on this box (loopback permitted); the reviewer/adversary
  saw these only because their sandbox denies `TcpListener::bind`. Clearable.
- **T4 contribution (prior-art / duplicate-work)** — still a human check; not mechanical.
- **T5 judgment (writer-supplied version-climb)** — brief pre-scopes backend-observed
  versions to a later slice; human confirms that scope holds.
- **Validation — Elle/JVM off-Check leg** — out of `cargo xtask ci` by design (ADR-0016);
  human confirms the serialized EDN is what the off-Check job consumes.

## Adversary's second point — now TIGHTENED (test-only)

The PUT/DELETE "indeterminate → clear obligation" reds used histories with no prior
obligation, so the `is_indeterminate → Unknown` clear was never exercised and dropping
the guard left them green — the comment overclaimed. Fixed by adding the two
discriminating histories the adversary named, which pin the "clears a STANDING bound"
direction. Source module is UNCHANGED (byte-identical) — the source already cleared
correctly; only the tests and their comments changed.

- `put_arm_clears`: `[PUT k v=5 @200 ; PUT k v=2 @500 ; GET k v=1 @200]` must be accepted.
  Drop the PUT indeterminate-clear → AtLeast(5) stands → v=1 < 5 → REJECT (proven: the
  assertion fails on exactly this mutation).
- `delete_arm_clears`: `[PUT k v=1 @200 ; DELETE k @500 ; GET k None @404]` must be accepted.
  Drop the DELETE indeterminate-clear → AtLeast(1) stands → the 404 read trips own-write-lost
  → REJECT (proven: the assertion fails on exactly this mutation).
- The original `put_arm` / `delete_arm` are kept (they pin the "must not ESTABLISH a bound"
  direction) with corrected comments that no longer overclaim.

Both new reds verified red→green on this box: green with the correct source, and each
flips to red with its own assertion message when the corresponding clear is removed.
`cargo fmt`/`clippy -p wyrd-server` clean; socket-free suite 8/8 green.
