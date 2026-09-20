# Adversarial review — issue 664 (staged-drain-and-restore-fence)

Evidence re-run in a scratch clone of `$PDCA_TARGET` (`cargo 1.96.0`, in-memory doubles only):
the patch's 12 tests pass post-fix, and the frozen `gate-logs/C4-verify.log` shows 11 of them
failing **by assertion** on the reverted base against the real `reconciliation_status` /
`reconcile_after_restore` / `reconcile_step` entry points — no mock stands in for the production
path, no tautology, no compile failure. The red→green itself survives attack. What follows is
where the *fix* does not.

## Refutations that landed

- **NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:916`: a second pass over an
  **unrepaired** store certifies the restore fence complete.** `fence_one_session` returns early
  for `SessionState::Aborting {}`, and a session fenced *with residue* in pass 1 is `Aborting` in
  pass 2 — so `sessions_fenced_with_residue` comes back empty, `needs_human()` is false, and
  `complete_fence_generation` (`restore.rs:750`) stamps the record. Re-ran the H(ii) fixture twice
  with nothing changed between the passes: pass 1 → `sessions_fenced_with_residue:
  ["mpu:2b2b…"], mpufence = {"generation":1,"complete":false}`; pass 2 → report empty,
  `mpufence = {"generation":2,"complete":true}`, while the `seg:` record naming chunk `0x0A06` is
  still in the store and the only obligation still reads `parts: Set([(1,1)])`. This directly
  contradicts the fix's own doc at `restore.rs:743-744` ("the record stays at this generation, not
  complete, so the image reads as unfenced until the named records are repaired") and the brief's
  invariant that an image is declared fenced only when every record a resurrected session wrote has
  a named deleter. Under #508's gate, a gateway would read `complete` and serve multipart verbs.
  (The T4 batch review reached the same line from three independent passes; this is the executed
  proof of it.)

- **NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1079` + the instruction the operator is
  given at `crates/server/src/cli.rs:1317` / `docs/design/architecture/m4-first-deployment-blueprint.md:640`:
  doing exactly what the runbook says makes the outcome *worse*, silently.** The `Completing`
  teardown freezes an explicit `PartScope::Set` from the part records present at the first fence,
  and the fence is never revisited. Probe: fence a `Completing` session whose `seg:` record names
  chunks `{held, orphaned}` while only part 1 (`held`) survives → residue reported. Then do what
  the NEEDS-HUMAN line says — seed the missing `part:` record for `orphaned` and re-run. Result:
  `retire:bytes:s:<id>:<E>` still reads `parts: Set([(1,1)])` (part 2 is *not* added — the session
  is `Aborting`, so `restore.rs:916` skips it), yet `mpufence` flips to
  `{"generation":2,"complete":true}`. The restored part's bytes now have no deleter *and* the store
  certifies itself fenced. The `Open` teardown is immune only because it uses `PartScope::All`.

- **NEEDS-HUMAN [impl] — `crates/dst/tests/custodian.rs:2380-2394` bakes the false generalization
  in.** The DST property re-runs the pass only after a *racing well-behaved fencer* already
  installed a complete obligation, then asserts "a re-run over the settled store" certifies —
  concluding in its header comment (`:2200`) that "the withholding is a withholding rather than a
  pass that can never certify". It never re-runs over a store where the blocker was **not** settled,
  which is the case above. A reviewer reading this property would reasonably believe re-run
  behaviour is covered; it is not. The cheapest fix to the *evidence* is a second arm that re-runs
  over an unrepaired residue store.

- **NEEDS-HUMAN [impl] — `crates/custodian/src/restore.rs:1067` and `:1085`: the residue predicate
  and the cursor/range disjunction are asserted in prose and by nothing else.** `C5-mutants` lost 9
  mutants, all in `completing_plan`: `||`→`&&` at `:1067` and `:1085`, `>`→`==` at `:1085`, and
  `+=`→`-=`/`*=` on the two unreadable counters at `:1035`, `:1039`, `:1062` (a `-=` on a `usize` at
  0 would panic in debug, so those lines are executed by **no** test — `C4-diff-cov.log` lists the
  same lines as MISS). Concrete cases nothing covers: (a) a `Completing` session with
  `segments_written: 0` and `seg:` records present — the `&&` mutant drops the `retire:records`
  obligation and the segment records lose their deleter, which is the X57 defect this slice exists
  to close (I confirmed production gets it right, so only the test is missing); (b) a `Completing`
  session whose `part:` key parses but whose value will not decode, with every `seg:` chunk covered
  — the `unreadable_parts` arm of the residue sentence, promised at `restore.rs:229-230` and in the
  CLI's FENCE RESIDUE paragraph, is never exercised.

- **NEEDS-HUMAN [human] — `crates/custodian/src/restore.rs:750`: the multipart fence certification is
  gated on findings that have nothing to do with multipart.** `complete_fence_generation` withholds
  on the whole of `needs_human()`, which includes `dangling` and `misplaced`. Probe: one cleanly
  fenced `Open` session plus one committed object whose fragment the restore did not bring back →
  the session is fenced correctly in pass 1, and every pass thereafter reports `dangling: [1794]`
  and leaves `mpufence` at `complete:false` forever. Since the runbook now says "do NOT re-enable
  the S3 gateways until a run reports the fence complete"
  (`m4-first-deployment-blueprint.md:632`, `:722-730`), any restore that actually lost data — the
  case this pass exists for — keeps multipart disabled until the operator deletes each lost object.
  The brief asked for the narrower rule (leg I(iii): complete "only if leg H found nothing", i.e.
  the *fence* findings). Whether the stricter coupling is the intended product behaviour is a
  scope/fitness call, not something Do should silently pick.

## Claim in the bundle I think is overstated

- `check-gates.json` C4-verify row reads "red without the fix, green with it (12 test(s) ran red)".
  `gate-logs/C4-verify.log:109` reads `1 passed; 11 failed`. Leg C is a guard and is green on the
  base by design (brief, leg C), so the count of legs that *earned* a red is 11, not 12. The gate
  still passes on its own terms; only the number quoted in the row is wrong.

## Attacked and could not refute

- **The red is real and on the production path.** Every base failure in `C4-verify.log:33-94` is an
  assertion against the shipped functions (`Satisfied` vs `Pending`, a `Debug` rendering with no
  `staged_skipped`/`sessions_fenced`, a probe that never fired). Nothing is mocked away.
- **Paging the `mpu:` listing** (`restore.rs:858-867`, `STAGED_PAGE = 512`). Seeded 600 `Open`
  sessions: `sessions_fenced = 600`, 0 still `Open`. The cursor hand-off is correct even though no
  test in the patch reaches the continuation arm (`restore.rs:864`, a diff-cov MISS).
- **The obligation writer** (`multipart.rs:329`): `encode_retire_obligation` re-decodes against the
  key it mints, so an obligation the #659 drain would refuse cannot become durable. Tried the
  `Completing` path with zero surviving parts and with a lying cursor; every installed value
  decoded back through `decode_retire_obligation`.
- **`MPUFENCE_KEY = b"mpufence"`** does not collide with `scan(b"mpu:")` (4th byte `f` ≠ `:`), so no
  session listing — including the fence's own — can pick it up.
- **Atomicity legs F/G**: the injected-fault double genuinely refuses the whole batch, and neither
  the session record nor either obligation survives it; the assertions are not vacuous.
