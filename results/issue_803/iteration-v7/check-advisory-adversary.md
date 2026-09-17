# Adversarial review — issue #803 (staged protection class for GC and restore)

Verdict: I could not break the production fix. I found one test gap the builder can close.

## What I re-ran (in a scratch copy of `$PDCA_TARGET`)

- **Green:** `cargo test -p wyrd-custodian --test staged_protection` → 23 passed.
- **Red evidence:** `gate-logs/C4-verify.log:10-44` shows 21 tests failing on base, each at an assertion or `expect_err` line in `crates/custodian/tests/staged_protection.rs`. D and F are guards and pass on base, as the brief says. The gate's "23 ran red" is the gate's own wording, which the iteration-5 sign-off already cleared.
- **Leg G (DST, the madsim deterministic simulation tests) has real teeth.** No gate shows G red, so I built `wyrd-dst` under `--cfg madsim` with `MADSIM_TEST_NUM=50` against two hand-made mutants of `crates/custodian/src/gc.rs`:
  - GC reads the committed `inode:` set before the staged class (swapping `gc.rs:258` and `:273`): both `gc_staged_handoffs_*` properties fail. The trace is `Read("inode:"), Landed(PartCommit), Read("mpu:"), Landed(Flip), Landed(Drain), … Read("part:…")`, and the chunk is reclaimed.
  - The `part:` range is read before the `sidx:` range (swapping the two `walk_staged_range` calls at `gc.rs:828-835`): both properties fail. The part commit lands between the two reads.

## Refutation attempts that failed (no finding)

- **Read order across all three handoffs** (`gc.rs:258`/`:273`, `restore.rs:340`/`:350`/`:360`). I traced the part commit, the flip and the drain, landing together or one at a time within one pass. Every schedule leaves the chunk in at least one reading. For restore, the second `inode:` read (the `appeared` set, `restore.rs:366`) adds a second safety margin.
- **Paging** (`gc.rs:809-861`). `checked_page` (`gc.rs:1174-1205`) refuses an empty page that still has a cursor, and a page that does not move forward. So a page cannot silently cut the walk short. With a scan cap of 2, the listing and per-upload paging legs fail on base.
- **Other keys under `mpu:`.** `MPUCTL_KEY` is disjoint (`crates/core/src/multipart.rs:1126-1132`), and nothing else in the tree writes under `mpu:`. So a healthy store cannot be marked `Blocked` by a key that is not a session.
- **Degenerate schemes** such as RS k=0 are refused at decode (`crates/core/src/multipart.rs:2359-2370` called at `:2556`, and `:3574-3581`), so `place()` never sees zero fragments.
- **A GC fault stopping scrub in a combined step** (`reconciliation.rs:136-149`). Production runs GC in its own `reconcile_pass` (`crates/server/src/custodian.rs:609-624`), so a staged-read fault cannot suppress scrub or reconstruction there.
- **Response size of a 512-record page of large `part:` values.** The TiKV `scan_page` fetches in round-trip-sized pieces (`crates/metadata-tikv/src/lib.rs:1313-1320`). No metadata backend in the tree has a per-response limit this would cross.

## Findings

- NEEDS-HUMAN [impl] — **No positive test pins how a healthy staged placement expands, and two concrete mutants of `StagedSet::place` (`crates/custodian/src/gc.rs:754-773`) pass all 23 tests.** I ran both:
  - **M1:** `index` replaced by `0` in the `FragmentId` at `gc.rs:760-763`. Every staged fragment is then recorded as index 0. For a real Reed-Solomon staged chunk (the shape the gateway writes), fragments 1..k+m-1 are unprotected, and GC deletes them once they carry a mark past grace. That is the data loss this slice exists to stop. Result: 23 passed.
  - **M2:** `if false && …` at `gc.rs:756`. Every healthy staged chunk is then held whole instead of placed. GC and restore would log a false `untrusted-staged-record` operator signal for every healthy upload on every pass. A stray copy on a server outside the placement would never be reclaimed while the record lives. Result: 23 passed.

  Cause: every positive fixture uses a one-fragment `EcScheme::None` placement (`staged_protection.rs:610`, `:625`, `:653`, `:656`, `:894`, `:909`, `:1051`, `:1063`, `:1113`, and the DST's `crates/dst/tests/custodian.rs:2721-2725`). The only RS(2,1) fixtures (`:1617`, `:1630`, `:1643`) have the wrong length, so they take the `held` path. C5-mutants (`gate-logs/C5-mutants.log:13`: 9 caught, 27 unviable, 0 missed) generates neither mutant.

  Fix, without naming any new symbol (the brief allows this): in legs A and B, add one healthy upload whose `part:` and `sidx:` chunks are `ReedSolomon { k: 2, m: 1 }` with a full, non-identity placement such as `[2, 0, 1]`. Put every fragment on its placed server, marked past grace. Assert all three survive GC and none is marked by restore; that catches M1. Also put one extra copy of fragment 0 on server 3, which the placement does not name, and assert GC reclaims it and restore marks it; that catches M2.
