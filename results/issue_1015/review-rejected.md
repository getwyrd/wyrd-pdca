# Recorded decisions for #1015. Six decisions, each made at a sign-off (none is the builder's).
# Row format: <file:line> | <CLASS> | <MATCH> | <reason>. MATCH is a phrase from the finding's
# rationale; a row clears only a finding of that class at that line whose rationale contains it.
# Line numbers are for the iteration-7 tree (base 3c600b6 + patch.diff).
#
# Decision 1 — "the slot reserve -> commit -> release race between the reaper's reads and its idle
# fence is a known gap of this slice". Source: the iteration-2 sign-off (2026-11-18, brief.md
# "Iteration 2 — carry-forward"), reaffirmed at the iteration-3, -4, -5 and -6 sign-offs ("the
# slot-cycle gap stays accepted/ignored"). The gap is stated in code at
# crates/custodian/src/reaper.rs:120 (module doc, "Known gap") and at pin_slots' doc
# (reaper.rs:1091), in docs/design/architecture/06-runtime-view.md:94, and reproduced by the
# ignored DST row crates/dst/tests/custodian.rs:8740 (its #[ignore] reason, :8735, names the
# follow-up). One row per site the finding has anchored at or is likely to anchor at. Class BUG and
# the phrase "reserve" only: a finding of another class, or one not about a reservation, is not
# cleared.
#
# Decision 2 — "a profile swapped at count zero cannot get a session fenced under the old
# profile". The iteration-3 batch review raised it twice at the fence commit (now reaper.rs:1199);
# the iteration-6 batch review raised it again there in other words ("a matching empty ledger …
# a valid profile swap"), which the "zero" rows did not match, so the same decision is recorded
# under those words too. Class BUG.
#
# Decision 3 — "the fence carries no precondition on mpuctl". Source: the iteration-5 sign-off
# (2026-11-20, brief.md "Iteration 5 — carry-forward": "Record as rejected with reasons the T4
# items reachable only through outside damage: no mpuctl precondition on the fence"), kept at the
# iteration-6 sign-off ("the recorded rejections in review-rejected.md stand"). The iteration-5
# batch review raised it three times at the fence commit (then reaper.rs:1123, now :1199). Class
# BUG; the phrases are the findings' own, plus the spellings a re-worded finding of the same
# defect would use ("pin the admission profile" is the iteration-6 batch review's).
#
# Decision 4 — "the combined step authorizes the retirement leg with the reaper's context and runs
# it with the reconstruction context". Source: the iteration-5 sign-off (same block: "and the
# reconciliation.rs:187 separate-context convention (one store and clock passed to both,
# custodian.rs:611-639)"), kept at the iteration-6 sign-off. The iteration-5 batch review classed
# it CONVENTION; the iteration-6 batch review raised the same thing twice as BUG, at
# reconciliation.rs:187 and :185, in the words "reconstruction store" and "its own metadata
# store", which the CONVENTION rows did not match. Same decision, so it is recorded under that
# class and those words as well.
#
# Decision 5 — "a clean-lap position has no freshness bound". Source: the iteration-5 sign-off
# (same block: "Deferred / not to fix: F2 (stale clean-lap, reaper.rs:714) needs >64 sessions; the
# only accepted profile caps at 46 — defer to #508 and say so in build-notes and a doc comment"),
# kept at the iteration-6 sign-off ("F2-of-iteration-5 (stale clean-lap) stays deferred to #508").
# This is a deferral, not a rejection of the finding's substance: the in-code marker is
# `// deferred: #508` at reaper.rs:790, and CLEAN_LAP_TAG's doc (reaper.rs:253-266) states the gap.
# Recorded here as well because this gate matches rows, not markers. Class BUG. A row clears the
# findings at one line only (the nearest), so the decision is recorded at each line it anchored at
# (now :791 and :789) and at the tag's definition (:267).
#
# Decision 6 — "the stray-obligation-beside-Open drain case is #659's follow-up, not this
# slice's". Source: the iteration-3 sign-off (2026-11-20, brief.md "Iteration 3 — carry-forward":
# "Out of scope (filed as #659 follow-ups, SUMMARY §10): the stray-obligation-beside-Open drain
# case and the post-check first-Create/drain race — do not change #659's drain"), kept at the
# iteration-4, -5 and -6 sign-offs ("#659 follow-ups stay out of scope"). The iteration-6 batch
# review raised it at reconciliation.rs:199, and no row existed, so it blocked. The rows below are
# the builder's transcription of that standing decision (iteration 7); delete them if the
# follow-up issue alone should carry it. Class BUG.
crates/custodian/src/reaper.rs:1106 | BUG | reserve | Accepted known gap at the iteration-2 sign-off (2026-11-18), kept at the iteration-3, iteration-4 and iteration-5 sign-offs: closing it needs a proposal 0016 change (a per-session witness every part commit rewrites, or the reaper occupying free indices), which the sign-off ruled out of this slice. Stated in the module doc (reaper.rs:120) and 06-runtime-view.md:94; regression row ignored with the follow-up named (dst/tests/custodian.rs:8740, :8735). Follow-up issue to be filed (SUMMARY §10); it must land before #668/#508 create sessions in production.
crates/custodian/src/reaper.rs:1095 | BUG | reserve | Same decision as reaper.rs:1106 (pin_slots): accepted known gap at the iteration-2 sign-off; follow-up to be filed.
crates/custodian/src/reaper.rs:1171 | BUG | reserve | Same decision as reaper.rs:1106 (the idle arm's pins): accepted known gap at the iteration-2 sign-off; follow-up to be filed.
crates/custodian/src/reaper.rs:1199 | BUG | reserve | Same decision as reaper.rs:1106 (the fence commit): accepted known gap at the iteration-2 sign-off; follow-up to be filed.
crates/dst/tests/custodian.rs:8740 | BUG | reserve | Same decision as reaper.rs:1106 (the ignored regression row): accepted known gap at the iteration-2 sign-off; follow-up to be filed.
crates/custodian/src/reaper.rs:1199 | BUG | zero | Not reachable: a matching ledger whose count is 0 at the pass's first read (reaper.rs:504) means no mpu: record existed then — count is every session in any state, moved exactly once by Create and once by the terminal delete (0016:348). Every session the walk then lists was created after that read, so its created_at_millis is a wall-clock stamp no older than the read (one ahead of the reaper's clock past δ_clock is held, never judged), and neither W_open (7 d) nor W_session (30 d) can fence it in that pass: no fence, sized by any profile, lands on it. The pass's last read (reaper.rs:529) then refuses the step on the swapped profile before the retirement leg runs (tests a_ledger_replaced_during_the_pass_refuses_the_step and a_first_create_during_the_pass_under_another_profile_refuses_the_step in reaper/tests.rs, the second through the production Create). Create itself never replaces a stored profile (crates/core/src/multipart/create.rs:428, ProfileSkew); a swap is an operator CAS gated on the live population draining (0016:2077).
crates/custodian/src/reaper.rs:529 | BUG | zero | Same decision as reaper.rs:1199 (zero): the last-read check this finding names refuses the swapped profile; no fence of the pass can reach a session created after a count-0 first read.
crates/custodian/src/reaper.rs:1199 | BUG | admission-ledger precondition | Rejected at the iteration-5 sign-off (2026-11-20): reachable only through damage from outside the protocol. No writer deletes mpuctl — Create and the terminal delete both put it (crates/core/src/multipart/create.rs:695, crates/custodian/src/retirement/sessions.rs:985; 0016:2028-2030, "it is never deleted") — and none replaces a stored profile under live sessions: Create refuses on ProfileSkew (create.rs:428), and an operator swap is a CAS admissible only at count 0 (0016:2077-2080). So for a fence to land after the ledger vanished or changed profile, something outside those writers must have rewritten the ledger while sessions were live. Pinning mpuctl in the fence would also cost ordinary operation: every Create and every terminal delete rewrites mpuctl (create.rs:684-695, sessions.rs:975-985), so each fence would lose to routine admission traffic. What the reaper does about such damage: its last read refuses the step so nothing is drained (still_admitted, reaper.rs:527), and since iteration 6 the signal says how many sessions the pass had fenced by then (emit_profile_refused, reaper.rs:1288).
crates/custodian/src/reaper.rs:1199 | BUG | pin the admission ledger | Same decision as reaper.rs:1199 (admission-ledger precondition): rejected at the iteration-5 sign-off, reachable only through outside damage.
crates/custodian/src/reaper.rs:1199 | BUG | admission ledger precondition | Same decision as reaper.rs:1199 (admission-ledger precondition), another spelling: rejected at the iteration-5 sign-off, reachable only through outside damage.
crates/custodian/src/reaper.rs:1199 | BUG | mpuctl precondition | Same decision as reaper.rs:1199 (admission-ledger precondition), another spelling: rejected at the iteration-5 sign-off, reachable only through outside damage.
crates/custodian/src/reaper.rs:1199 | BUG | precondition on `mpuctl` | Same decision as reaper.rs:1199 (admission-ledger precondition), another spelling: rejected at the iteration-5 sign-off, reachable only through outside damage.
crates/custodian/src/reaper.rs:1199 | BUG | pin `mpuctl` | Same decision as reaper.rs:1199 (admission-ledger precondition), another spelling: rejected at the iteration-5 sign-off, reachable only through outside damage.
crates/custodian/src/reconciliation.rs:187 | CONVENTION | reconstruction context | Rejected at the iteration-5 sign-off (2026-11-20): the two contexts are separate types by design (the reaper's is also used alone, by the operator one-shot, reconciliation.rs:196-209), and the one production caller that passes both builds them from one store and one clock — `meta` and `&clock` in both ReconstructionContext (crates/server/src/custodian.rs:611-629) and ReaperContext (:634-639), as ReaperContext::clock's doc requires (reaper.rs, "One source with the pass's other legs"). A caller that passed two stores or two clocks would be a new caller written against that doc; no code path does.
crates/custodian/src/reconciliation.rs:187 | CONVENTION | reaper context | Same decision as reconciliation.rs:187 (reconstruction context): rejected at the iteration-5 sign-off; the one caller passes one store and one clock to both.
crates/custodian/src/reaper.rs:791 | BUG | clean cursor | Deferred to #508 at the iteration-5 sign-off (2026-11-20): a clean-lap position is written only when the listing is longer than SESSIONS_PER_PASS (64, reaper.rs:220), and the only profile the profile check accepts admits MAX_SESSIONS = 46 sessions (crates/core/src/multipart.rs:6025), so today every listing is judged whole by one pass and the tag is never written. Bounding a lap's age belongs with the change that lets a deployment choose a larger profile (#508). Stated at CLEAN_LAP_TAG's doc (reaper.rs:253-266, "What it does not bound"); in-code marker `// deferred: #508` at reaper.rs:790.
crates/custodian/src/reaper.rs:791 | BUG | clean-lap | Same decision as reaper.rs:791 (clean cursor): deferred to #508 at the iteration-5 sign-off; unreachable under the one accepted profile (46 sessions against a 64-session pass).
crates/custodian/src/reaper.rs:791 | BUG | freshness | Same decision as reaper.rs:791 (clean cursor): deferred to #508 at the iteration-5 sign-off; unreachable under the one accepted profile (46 sessions against a 64-session pass).
crates/custodian/src/reaper.rs:789 | BUG | clean cursor | Same decision as reaper.rs:791 (clean cursor), at the line just above it, where one of the three iteration-5 findings anchored (then reaper.rs:712): deferred to #508 at the iteration-5 sign-off.
crates/custodian/src/reaper.rs:789 | BUG | clean-lap | Same decision as reaper.rs:791 (clean cursor), at the line just above it: deferred to #508 at the iteration-5 sign-off.
crates/custodian/src/reaper.rs:267 | BUG | clean cursor | Same decision as reaper.rs:791 (clean cursor), at the tag's definition: deferred to #508 at the iteration-5 sign-off.
crates/custodian/src/reaper.rs:267 | BUG | clean-lap | Same decision as reaper.rs:791 (clean cursor), at the tag's definition: deferred to #508 at the iteration-5 sign-off.
crates/custodian/src/reaper.rs:1199 | BUG | profile swap | Same decision as reaper.rs:1199 (zero), in the iteration-6 batch review's words: a ledger whose count is 0 at the pass's first read (reaper.rs:504) means no mpu: record existed then, so every session the walk lists was created after that read; its created_at_millis is no older than the read, and neither W_open (7 d) nor W_session (30 d) can fence it in that pass. No fence lands on a session admitted after a swap; the pass's last read (reaper.rs:529) then refuses the step.
crates/custodian/src/reaper.rs:1199 | BUG | empty ledger | Same decision as reaper.rs:1199 (zero), another spelling from the iteration-6 batch review: a session admitted after a count-0 first read is too young for either window, so no fence of that pass can reach it.
crates/custodian/src/reaper.rs:1199 | BUG | pin the admission profile | Same decision as reaper.rs:1199 (admission-ledger precondition), in the iteration-6 batch review's words: rejected at the iteration-5 sign-off and kept at the iteration-6 sign-off; reachable only through outside damage or through a swap at count 0, which the "zero" rows above answer.
crates/custodian/src/reaper.rs:1199 | BUG | admission profile | Same decision as reaper.rs:1199 (admission-ledger precondition), another spelling: rejected at the iteration-5 sign-off, kept at the iteration-6 sign-off.
crates/custodian/src/reconciliation.rs:187 | BUG | reconstruction store | Same decision as reconciliation.rs:187 (CONVENTION, reconstruction context), raised as BUG by the iteration-6 batch review: rejected at the iteration-5 sign-off (2026-11-20) and kept at the iteration-6 sign-off. The two contexts are separate types by design (the reaper's is also used alone, by the operator one-shot, reconciliation.rs:196-209), and the one production caller that passes both builds them from one store and one clock — `meta` and `&clock` in both ReconstructionContext (crates/server/src/custodian.rs:611-629) and ReaperContext (:634-639), as ReaperContext::clock's doc requires. No code path passes two stores.
crates/custodian/src/reconciliation.rs:187 | BUG | reaper.meta | Same decision as reconciliation.rs:187 (reconstruction store): rejected at the iteration-5 sign-off, kept at the iteration-6 sign-off; the one caller passes one store and one clock to both.
crates/custodian/src/reconciliation.rs:187 | BUG | metadata store | Same decision as reconciliation.rs:187 (reconstruction store): rejected at the iteration-5 sign-off, kept at the iteration-6 sign-off; the one caller passes one store and one clock to both.
crates/custodian/src/reconciliation.rs:187 | BUG | reconstruction context | Same decision as reconciliation.rs:187 (reconstruction store), the CONVENTION rows' phrase under the class the iteration-6 batch review used.
crates/custodian/src/reconciliation.rs:187 | BUG | reaper context | Same decision as reconciliation.rs:187 (reconstruction store), the CONVENTION rows' phrase under the class the iteration-6 batch review used.
crates/custodian/src/reconciliation.rs:185 | BUG | reconstruction store | Same decision as reconciliation.rs:187 (reconstruction store), at the line the second iteration-6 finding anchored at: rejected at the iteration-5 sign-off, kept at the iteration-6 sign-off.
crates/custodian/src/reconciliation.rs:185 | BUG | metadata store | Same decision as reconciliation.rs:187 (reconstruction store), at the line the second iteration-6 finding anchored at.
crates/custodian/src/reconciliation.rs:185 | BUG | both contexts | Same decision as reconciliation.rs:187 (reconstruction store), at the line the second iteration-6 finding anchored at, in its own words ("When both contexts are supplied").
crates/custodian/src/reconciliation.rs:185 | CONVENTION | reconstruction context | Same decision as reconciliation.rs:187 (CONVENTION, reconstruction context), at the line just above it.
crates/custodian/src/reconciliation.rs:199 | BUG | stray teardown obligation | Out of scope at the iteration-3 sign-off (2026-11-20), kept at the iteration-4, -5 and -6 sign-offs: "the stray-obligation-beside-Open drain case" is filed as a #659 follow-up (SUMMARY §10) and this slice must not change #659's drain. What the finding describes is the drain acting on a decodable {session, all} obligation that sits beside a session still Open — a state no writer leaves (every fence writes the obligation and moves the session in one batch, crates/core/src/multipart/abort.rs:83-99). A reaper fence of such a session loses on the obligation key, and the session is then held and named (obligation-key-taken, reaper.rs:1224); what the drain does with the obligation is the drain's rule, in crates/custodian/src/retirement.rs, which is #659's.
crates/custodian/src/reconciliation.rs:199 | BUG | stray obligation | Same decision as reconciliation.rs:199 (stray teardown obligation): out of scope at the iteration-3 sign-off, a #659 follow-up.
crates/custodian/src/reconciliation.rs:199 | BUG | obligation beside | Same decision as reconciliation.rs:199 (stray teardown obligation): out of scope at the iteration-3 sign-off, a #659 follow-up.
crates/custodian/src/reconciliation.rs:187 | BUG | stray teardown obligation | Same decision as reconciliation.rs:199 (stray teardown obligation), at the fleet branch's call of the same leg: out of scope at the iteration-3 sign-off, a #659 follow-up.
