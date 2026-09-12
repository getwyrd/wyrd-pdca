# Adversarial review — #772 (multipart owned staging entry)

**Bottom line:** I could not refute the core fix. Each of the brief's negations, re-run on the
patched tree, fails exactly one test, and the tests drive the production readers over real
stores. I found one scope hole on the **write** side (a human call), one real test gap (the GC
skip's audit signal can be deleted with every test still green), and two small nits.

## Findings

- NEEDS-HUMAN [human] — **The `pending:` writers still accept an owned or torn entry.**
  `put_pending` (`crates/core/src/metadata.rs:2039-2047`) and `renew_pending`
  (`metadata.rs:2111`, `put(key, encode(entry))`) encode whatever `PendingEntry` they are given
  under `pending:`, with no shape check. Reproduced on the patched tree over an in-memory redb
  store: `put_pending(&store, 5, &OwnedEntry::new(..).to_pending())` → `Committed`;
  `renew_pending(&store, &[6], 10, &owned)` over a live ordinary lease → `Committed`, and
  `pending:6` now holds the owned shape; the next ordinary `renew_pending` of chunk 6 then fails
  with "an owned pending entry is stored under `pending:`". A torn literal (`owner: Some(..),
  staged: None`) compiles because the fields are `pub` (`metadata.rs:1595-1601`) and gets
  written the same way. Every `pending:` reader refuses the result, both sweeps skip it forever,
  and under the deployed `ExpiredPendingPolicy::Defer` nothing even logs it. That is the "bytes
  nothing can read back" hazard S9 exists to close, reached through the `pending:` writer
  instead of a `sidx:` one. It is also T4-batch-review's `metadata.rs:1596` blocker. The fix is
  small (refuse `owner.is_some() || staged.is_some()` in both writers; both already return the
  boxed error, so no signature changes). But it is not one of the ten legs, and it grows the
  `metadata.rs` rebase surface the brief wants kept small for #776. Fix it here, or decline with
  a follow-up issue.

- NEEDS-HUMAN [impl] — **Nothing tests the GC skip's audit signal.** That signal is the only
  thing behind the claim "so the skip is never silent" (`crates/custodian/src/gc.rs:490`), and
  it is the exact point T4 raised at `gc.rs:491`. I replaced the body of
  `emit_unreadable_pending` (`gc.rs:594`) with `let _ = (entry, fault);`: the new leg at
  `crates/custodian/tests/gc.rs:896` still passes, and so does the whole `wyrd-custodian` suite
  (19 test binaries, 111 passed, 0 failed). C5 missed this because cargo-mutants'
  `replace emit_unreadable_pending with ()` leaves both parameters unused, which does not compile
  under `warnings = "deny"` (`Cargo.toml:227`), so it was counted among the 26 "unviable".
  Fix: in the `gc.rs:896` leg, capture the audit output with the existing pattern (`Capture` /
  `capturing_dispatch`, `crates/custodian/tests/segmented_map_consumers.rs:295-339`, including its
  `set_global_default` guard at `:335` against the #214 callsite-cache race). Then assert exactly
  one `unreadable-pending-entry` line naming `pending:226` (the misfiled chunk `0xE2`,
  `tests/gc.rs:914`).

- NEEDS-HUMAN [impl] — Minor: `OwnedEntry::from_pending`'s ordinary-shape arm
  (`crates/core/src/multipart.rs:3560-3563`) never runs in any test (C4-diff-cov lists
  3560-3563 as MISS). Its doc (`:3548`) promises
  `PendingEntryNamespaceMismatch { namespace: "sidx:", shape: "ordinary" }`, and this is the public
  validator S9 relies on. One assertion in the S9 test (`from_pending` on an
  `owner: None, staged: None` literal) covers it.

- NEEDS-HUMAN [impl] — Minor docs wording in the S10 sentence
  (`docs/design/architecture/05-building-block-view.md:202`). "(`staged`, a scheme the erasure
  coder supports and one D server per fragment)" puts a property decode checks (geometry) next
  to one it deliberately does not check (placement length: S6, and a `[5,6]` placement under
  `rs(2,1)` decodes). Read as written, it suggests decode enforces length, which is the over-claim
  the brief withdrew at v3. Also, "is a typed error at every reader" makes the promise S4 says
  not to make: `renew_pending` / `live_lease_guards` return the crate's boxed error. Suggested
  wording: "one D server per planned fragment (its length checked only by maintenance)" and
  "is refused at every reader".

## Refutation attempts that did not land

- **Red→green evidence.** `cargo test -p wyrd-core --test multipart_owned_staging` on the
  patched tree: 13/13 pass. The RED leg is a compile failure, as the brief pre-declared (C4-verify
  exit 77). In its place I ran the negations myself. Each fails exactly one test:
  - S1: drop `supported` in `checked_staged_scheme` (`multipart.rs:3446`) → only s1 fails.
  - S2: `if false` at `multipart.rs:3615` → only s2.
  - S3: pairing forced to `Ok` (`:3373`) → only s3.
  - S4, one reader at a time back to the generic decode: `renew_pending` (`metadata.rs:2106`) →
    only `s4_renew_pending…`; `live_lease_guards` (`:2142`) → only `s4_a_leased_commit…`; the
    sweep (`write.rs:652`) → only s5.
  - S5: `?`-abort in `write.rs` → only s5.
  - S6: add a length check to `StagedPlacement::new` → only s6.
  - S7: drop `skip_serializing_if` on `owner`, then separately on `staged` → only s7 each time.
  - GC side: generic decode at `gc.rs:498` → the `tests/gc.rs:896` leg fails (the fragment is
    reclaimed); `?`-abort → the same leg fails at `:936`.
- **S8's negation as the brief words it cannot fail S8.** Dropping a `skip_serializing_if` does
  nothing to an owned value, because both fields are present. I ran it and only S7 fails. The
  test relies on the canonical-bytes gate instead: removing `require_canonical`
  (`multipart.rs:3622`) fails s8 alone, so S8 does test something real. At sign-off, check that
  build-notes.md records this re-targeted negation and not the one the brief describes.
- **C5's one MISSED mutant is equivalent** (`metadata.rs:1650:30`, `||` → `&&`).
  `PendingEntry::try_from` at `:1649` has already refused every torn value, so by `:1650`
  `owner.is_some() == staged.is_some()`, and the two operators agree on every input that can
  reach that line. This is not a test gap. To clear the C5 red, simplify the condition to
  `entry.owner.is_some()`, or pin it in `.cargo/mutants.toml`.
- **T4's three `write.rs` blockers** (`write.rs:650`, `:652`, "silent skip") hit a function with
  no production caller. `sweep_expired_leases` is defined at `write.rs:644` and called only from
  seven test files; the production expiry path is the GC's. The brief also forbids adding a
  tracing seam to `write.rs` for this leg. These blockers can be rejected with that reason
  recorded. T4's `gc.rs:491` finding is wrong on the facts (the skip does emit a warning and a
  counter), but see the `[impl]` finding above: no test holds that in place.
- **Fail-safe checks.**
  - All four production readers of `pending:` values now go through `decode_pending_entry`
    (`metadata.rs:2106`, `:2142`; `write.rs:652`; `gc.rs:498`). The only other production
    `pending:` scan, restore (`crates/custodian/src/restore.rs:731`), reads keys only, so a
    misfiled value still protects its fragments there.
  - A skipped chunk cannot be reclaimed some other way: GC deletes a fragment only when it has an
    orphan record or its chunk is in the expired set (`gc.rs:196-211`).
- **Odd inputs through both decoders.** Each was refused or decoded as intended:
  - `"owner":null` plus `staged` → `TornOwnedEntry` under both namespaces.
  - A `a`-escaped owner → `NoncanonicalRecordValue` under `sidx:`.
  - A duplicate `owner` field, or an uppercase owner → `MalformedRecordValue`.
  - An unknown top-level field → refused under `sidx:`.
  - `rs(255,255)` with an empty placement decodes under `sidx:`. That is correct: the coder
    supports that geometry, and length is the contextual check.
  - Two open-wire values decode under `pending:` but do not re-encode byte-identically:
    `"owner":null,"staged":null`, and an ordinary lease with an unknown field. The old
    `PendingEntry` derive was open in the same way, so this diff did not introduce it.
