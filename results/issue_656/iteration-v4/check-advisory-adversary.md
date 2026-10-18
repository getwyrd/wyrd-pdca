# Adversarial review — issue 656 (multipart Create / Abort), round 4

Bottom line: I could refute the fix in one place. That place is the same Abort branch the
gating T4 review already blocks on, and I reproduced it. Create, the shared fence builder, the
restore refactor and the evidence all held up when I attacked them.

## Refutation that landed

- NEEDS-HUMAN [impl] — `crates/core/src/multipart/store.rs:797-802`: on `Conflict`, if the
  re-read finds the session byte-identical, Abort returns `AbortFault::ObligationKeyTaken`
  ("damage") **without reading the obligation key**. The comment at `:798-799` claims "only the
  obligation key can have failed". That holds for redb and the sim-FDB model. It does not hold
  for every backend. The TiKV adapter maps a `get_for_update` lock failure to `Conflict` when
  "the concurrent winner already holds/moved the lock"
  (`crates/metadata-tikv/src/lib.rs:1388-1393`). If that lock holder never commits, the re-read
  finds nothing changed. The peer call site this slice was told to mirror already handles this
  case. Restore reads each obligation key and reports `SessionUnsettled::LostConflict`, "a
  conflict a re-read cannot explain" (`crates/custodian/src/restore.rs:324-325`, `:1065-1075`).
  The client Abort skips that read.
  **Concrete failing cases** (my scratch probe, a wrapper over `RedbMetadataStore` that answers
  `Ok(Conflict)` once without applying anything):
  (1) one spurious `Conflict` → Abort returns `ObligationKeyTaken { retire:bytes:s:<id>:0 }`
  while that key is **absent**;
  (2) round 1 → `CommitUnknownResult { may_still_commit: true }` with the batch held, round 2
  → spurious `Conflict` → Abort returns `ObligationKeyTaken`. When the held round-1 batch is
  then released it **commits**, and the session is `Aborting`.
  So the in-doubt outcome is thrown away for a "damage" error. That breaks the patch's own
  promise at `:711-714` ("never with an error that lets its caller take the fence for one that
  installed nothing") and the rubric's *Transactions* class (`CommitUnknownResult` must outrank
  a conflict-style answer). No existing test can catch it: `d_a_taken_obligation_key_is_a_typed_fault`
  (`crates/core/tests/multipart_create_abort.rs:1981`) only plants the key, so it passes whether
  or not the key is read. The DST model is strict CAS, so it never produces this case either.
  **Fix:** read the obligation key in this branch. Report `ObligationKeyTaken` only when the key
  is present. When it is absent, treat the conflict as an unexplained lost race: keep `doubt`
  and spend a round, or end with the held `CommitUnknownResult` if `may_still_commit`. Add both
  probes above as regressions.

## Attacks that did not land

- **Red→green evidence.** C4-verify is UNVERIFIABLE (exit 77, `gate-logs/C4-verify.log`). The
  brief declared that in advance: the new API does not exist on the base. build-notes is withheld,
  so I re-ran the three required reds myself in a scratch copy. (A′) with the contention budget
  set to `MAX_UPLOAD_ID_ATTEMPTS` fails both `a_prime_*` legs, and (A) free-running fails too.
  (E) with an `mpuctl` decrement added to the Abort batch fails 8 Abort legs. (F) with a re-mint
  on an unknown outcome fails 9 F/C legs. The unmodified code passes 34/34. The reds come from
  the real `create_session` / `abort_session` paths over a real redb store, not from a copy of
  them.
- **Gates C4-ci never reached.** C4-ci fails on the inherited cargo-deny advisory
  (RUSTSEC-2026-0346/0349, rustls 0.21.12 via tikv-client). Earlier sign-off already routed that
  waiver to Act. Because the gate stopped there, its later steps never ran: conformance, the
  ADR-0035 statics guard, and `cargo xtask dst` (madsim clippy plus the 50-seed sweep). I ran them
  by hand. `xtask statics` and `xtask conformance` pass. `cargo clippy -p wyrd-dst --all-targets`
  under `--cfg madsim` is clean. The full `cargo test -p wyrd-dst` with `MADSIM_TEST_NUM=50`
  exits 0: `multipart_admission` 6/6, and `custodian` 33/33, which covers the refactored restore
  fence.
- **Create** (`store.rs:351-616`). I tried each of these and found no failing case under the
  strict-CAS contract:
  - an unknown outcome landing between the recovery's reads;
  - a spurious `Conflict` while in doubt (`lost()` falls through to `Contended` and keeps the
    identity);
  - a bucket deleted while a batch is in doubt (ends `Unsettled`, and the held batch can no
    longer land);
  - a repeated id/nonce (`vacant()` at `:522-527` runs before the first batch);
  - a profile skew or a full ledger under doubt (`settle()` at `:444-466`);
  - backoff overflow at large profiles (`admission_backoff_millis` saturates).
- **Size headroom** (`multipart.rs:2368` `widest_encoding`). It covers every state, with every number
  the decoder allows at its widest, measured by the real encoder, so JSON escapes are counted.
  The decoder (`multipart.rs:2581-2638`) checks nothing that `SessionRecord::open` could violate.
- **Restore refactor** (`restore.rs:1051-1052`). `mpu_key(parse_mpu_key(key)) == key`, because
  the parse is strict (`multipart.rs:1236-1244`). The preconditions and puts come out in the same
  order as the inline code they replace. Conflict classification is unchanged.

<!-- pdca:leaf-complete -->
