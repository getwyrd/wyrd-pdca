# core: every session record carries its segment-group nonce (809.2)

> Child 2 of 5 of #809's split at its re-plan (2026-09-29); #809 is itself 664.2. Do reads ONLY
> this file. Keep the `- **Label:** value` lines. `path:line` citations are on `origin/main` @
> `243241e` (verified 2026-09-29). Background: 0016 =
> `docs/design/proposals/draft/0016-multipart-commit-protocol.md`; the nonce's lifecycle at
> `0016:508-518`; the Create row at `:656`, the terminal-delete row at `:673`; X57 at `:880`.

- **Slug:** completing-session-nonce
- **Kind:** enhancement
- **Defect:** no session record can name its segment group's nonce. The session record carries
  `parent`, `object`, `content_type`, `created_at_millis`, `clock_source`, `epoch`, `attempts`
  and `state` (`crates/core/src/multipart.rs:2078-2088`); `Completing`'s `publish_target`
  carries `parent`, `name` and the fence `epoch` (`:1954-1961`); `Open` and `Aborting` carry no
  fields (`:2019`, `:2033`). The nonce is deliberately independent of the upload id (`:3307-3311`),
  so nothing on the record can derive it. 0016 needs it on the record for the session's whole
  life:
  * **at Create**, which mints it and reserves it with `require_absent(seggrp:<nonce>)` plus a
    `seggrp:` marker in the same batch (`0016:508-518`, `:656`);
  * **while `Completing`**, so a writer that ends the attempt can install
    `retire:records:{seg:<nonce>:<E>}` in the same batch as its fence (`0016:665`,
    `:2193-2196`). That covers the restore fence (child-4 of this split), the reaper and operator
    abort (#656, #659) and Complete's own rollback (#658). Without it those `seg:` records have
    no deleter anywhere (X57, `0016:880`);
  * **at the terminal delete**, which removes the `seggrp:` marker when the nonce's `seg:` range
    is empty (`0016:673`), including for an upload aborted before Complete, whose record is
    `Open` or `Aborting`;
  * **across a rollback**: `Completing@E → Open@E+1` (`0016:538-552`) drops `publish_target`,
    and the next attempt must write under the same nonce at a new epoch.
  `metadata.rs:1016-1018` already describes the nonce as "minted with the publishing session".
  **Design settled at Plan (the human):** option (i), 2026-09-12: the nonce is **stored on the
  session record**, not derived. Rejected: deriving it from `(upload id, E)` as `0016:2333` says,
  because the code keeps the nonce independent of the upload id on purpose (`:3307`).
  **Placement A, 2026-09-29:** the nonce is a field of the session record itself, present in
  **every** state, and a `Completing` session's segment group is `(the session's nonce,
  publish_target.epoch)`. Rejected: placement B, the nonce inside `Completing`'s
  `publish_target` only (this brief's earlier shape). It left `Open` and `Aborting` records
  unable to name the marker they reserved, and lost the nonce on every rollback.
- **Success criterion:** the NEW file `crates/core/tests/multipart_segment_nonce.rs` passes. The
  wire spelling is fixed here so that this test and the fixtures of #841, #842, #843 and #810
  agree: every session record carries `"segment_nonce"`, a string of exactly 32 lowercase hex
  characters (`SegmentNonce`, `crates/core/src/metadata.rs:963-1002`), immediately after
  `"clock_source"` and before `"epoch"` (creation-time fields first, then the ones transitions
  change). `publish_target` is unchanged. For example:
  `{"parent":1,"object":"n","created_at_millis":1000,"clock_source":"wall","segment_nonce":"0123456789abcdef0123456789abcdef","epoch":3,"attempts":1,"state":{"kind":"Open"}}`
  and, for `Completing`, the same record with
  `"state":{"kind":"Completing","fenced_at_millis":1,"segments_written":2,"publish_target":{"parent":1,"name":"n","epoch":3}}`.
  Legs, each over all four states (`Open`, `Completing`, `Aborting`, `Completed`):
  (a) a record carrying the nonce decodes through `decode_session_record` (`multipart.rs:2250`),
  and re-encoding the decoded value with `wyrd_core::metadata::encode` (`metadata.rs:1934`)
  gives back the input bytes exactly;
  (b) the same record **without** `segment_nonce` is refused;
  (c) a nonce that is not 32 lowercase hex characters (uppercase, 31 characters, one containing
  `:`) is refused;
  (d) the record names its nonce once: a `segment_nonce` inside `publish_target` is refused, and
  so is the field in any position other than the one above (the canonical-bytes check,
  `multipart.rs:2121-2125`).
  In `crates/core/tests/multipart_session_records.rs` (green-only): the decoded record exposes
  its nonce in every state, so the terminal delete can mint `seggrp_key` (`metadata.rs:1520`)
  from it; and a `Completing` record exposes its attempt's `SegmentGroup` `(nonce,
  publish_target.epoch)`, so a writer can mint the `seg:` range (`seg_range_prefix`,
  `metadata.rs:1505`) from it. Neither re-parses the nonce. Any other state has no attempt
  group.
  **(L) `cargo xtask ci` green.** After this change every existing session fixture in the
  workspace carries the nonce; they are listed under Scope.
- **Falsifiability:** RED on `origin/main`, in-process, by assertion. The base
  `SessionRecordWire` is `deny_unknown_fields` (`multipart.rs:2077`), so (a) fails in every state
  with `unknown field segment_nonce`. The base accepts the nonce-less record, so (b) fails too.
  (c) and (d) are refused on the base for the same unknown-field reason, so they pass there; they
  earn their keep against a mutant that accepts a malformed nonce or a second copy, so pair each
  with (a)'s positive arm in the same test. The new file names only base-visible symbols
  (`decode_session_record`, `metadata::encode`, the existing `SessionRecord` and `SessionState`
  API). The accessors this slice adds are asserted only in the green-only file, because naming
  them in the new file would stop the red leg compiling (UNVERIFIABLE,
  `engine/scripts/run-verify.sh:533-541`). Record in `build-notes.md` how many tests ran red.
- **Invariant to restore:** a session's segment-group nonce is nameable from the session's own
  record in every state from Create to the terminal delete, so the batch that ends a
  `Completing` attempt can install the deleter of that attempt's `seg:` records, and the terminal
  delete can remove the `seggrp:` marker Create reserved. Source: 0016 `:508-518`, `:656`,
  `:665`, `:673`, X57 (`:880`), `:2193-2196`; `metadata.rs:1016-1018`; the human's option (i)
  (2026-09-12) with placement A (2026-09-29). SELF-TEST: a nonce carried only by `Completing`
  is gone from an `Aborting` record and after a rollback to `Open`, so the terminal delete cannot
  name the marker and a second attempt cannot reuse the group.
- **Repo + branch target:** getwyrd/wyrd @ main
- **Ordering note:** wave 1 of #809's split, in parallel with child-1 (no shared file). child-3
  and child-4 build on it; #843 and #810 inherit the spelling. In `crates/dst/tests/custodian.rs`
  this child edits only the two session helpers, `handoff_session` (`:2760`) and
  `replace_session` (`:4531`), one line each. #722 adds a new property to that file and does not
  edit either helper, and #682 was split, so no `Conflicts with` is declared; if #722's accepted
  patch turns out to touch either helper, add it then. Downstream, not work here: the Create
  writer (#508's `CreateMultipartUpload`) mints the nonce and reserves `seggrp:` in the same batch
  (`0016:656`); #658's Complete fence reads the nonce from the record rather than writing it;
  0016's `mpu:` row (`0016:350`) lists no nonce field, and this slice does not edit 0016, so the
  divergence is recorded in `05-building-block-view.md` (Scope).
- **Surfaces:** data
- **Difficulty:** medium
- **Scope:** the nonce as a field of the session record in every state: `SessionRecordWire`
  and `SessionRecord` (`multipart.rs:2078-2088`, `:2133`), the record's canonical-bytes decode
  (`:2121-2125`), and accessors for the nonce and for a `Completing` session's attempt group.
  The field decodes through `SegmentNonce`'s validating constructor: the type deliberately has no
  `Deserialize` (`metadata.rs:979-982`). Update the record's docs to match: the field count
  (`multipart.rs:2102`, "Seven of the eight fields"), and `PublishTarget`'s note that its epoch
  "makes the attempt's segment-group nonce deterministic" (`:1945`), which now reads as the
  attempt's epoch within the session's group. Every existing full session record in the
  workspace gains the field, each through its one builder:
  `crates/core/tests/multipart_session_records.rs` (`session_with`, `:88`, and the inline records
  at `:646`, `:711`), `crates/core/tests/multipart_state_machine.rs` (`completed_session_bytes`,
  `:402`), `crates/custodian/tests/staged_protection.rs` (`session`, `:577`),
  `crates/custodian/tests/staged_scrub.rs` (`session`, `:427`),
  `crates/custodian/tests/staged_repair.rs` (`session`, `:415`),
  `crates/custodian/tests/staged_drain_status.rs` (`session_open`, `:275`), and
  `crates/dst/tests/custodian.rs` (`handoff_session` `:2760` and `replace_session` `:4531`
  only). State-only fixtures (`{"kind":…}` values, `publish_target` values) do not change. Also
  the persisted-field sentence in `docs/design/architecture/05-building-block-view.md:202`
  (`AGENTS.md:154-157`, "Docs currency"), noting that 0016's `mpu:` row does not yet list the
  field. `metadata.rs` only for a constructor that builds a `SegmentGroup` from an
  already-validated `SegmentNonce`, if the accessor needs one. Size budget: under 45 KB of diff.
  / out of scope: any writer of the nonce (Create, #508; #658) and any reader of it (child-4);
  the `seggrp:` marker's writes and deletes; `restore.rs` and every custodian source file; the
  retire-obligation codec; any edit to 0016 or an ADR.
- **Repro instruction:** on `origin/main`, pass the example `Open` record above to
  `decode_session_record`. It fails with `unknown field segment_nonce`. Drop the field and it
  decodes. Do the same with the `Completing` example: a record in the one state that has a
  segment-write phase cannot name the group it writes.
- **External dependencies:** `typos`, `docs-renderer`
- **Test file:** `crates/core/tests/multipart_segment_nonce.rs`. This is a **NEW** file:
  C4-verify's red comes only from an added `*/tests/*.rs` (`run-verify.sh:141-144`). The
  accessor leg goes in the existing `multipart_session_records.rs`, which the red leg reverts,
  so it runs green-only.
- **Citations expected:** Do must cite `path:line` on the target branch for every change. Peer
  callsites Do MAY open and mirror:
  * `crates/core/src/multipart.rs:2077-2135` (the session wire shape, its canonical decode and
    `SessionRecord`), `:2204-2240` (the record's own cross-field checks), `:2250`
    (`decode_session_record`), `:1943-1961` (`PublishTarget`), `:2006-2039` (`SessionState`).
  * `crates/core/src/metadata.rs:983-1047` (`SegmentNonce`, `SegmentGroup::new`, the one
    validating path) and `:1048` (`SegmentGroup`'s own `Deserialize`, the peer for decoding a
    validated nonce off the wire); `:1505` (`seg_range_prefix`), `:1520` (`seggrp_key`).
  * Existing fixtures: `crates/core/tests/multipart_session_records.rs:81-120`, `:276`, `:320`.
- **Prior-art check (triage cycles):** by path (`multipart.rs`), 2026-09-29 on `243241e`. The
  last change is `4533549` (capacity knobs). No open PR touches the file, and no merged change
  adds a nonce to the session record. Rejected prior art: #809 iteration 1 added this nonce
  inside `PublishTarget` after `epoch` inside a 179 KB patch; its codec legs passed review, but
  that placement is placement B, rejected above. #637 iteration 1 fenced `Completing` sessions
  with no nonce at all; do not repeat that.
- **Disposition hint:** likely-fix

## STOP discipline

Draft only until Check sign-off. Pushing to a feature/draft branch and opening a draft PR MAY
happen during the cycle. The PR MUST NOT be marked ready before sign-off accepts.

Plan-review response (2026-09-29): the finding was right and is resolved by the human's call.
0016 creates the nonce at Create and needs it at the terminal delete (`0016:508-518`, `:656`,
`:673`), which a `Completing`-only field cannot serve. The human chose placement A on
2026-09-29: the nonce is a field of the session record in every state, and a `Completing`
attempt's group is `(nonce, publish_target.epoch)`. Option (i) (store, don't derive) stands. Legs
(a)–(c) now cover all four states; (d) now refuses a second copy instead of refusing the field
outside `Completing`. The fixture scope is the eight full-record builders, one line each. Writers
(Create, #508; #658) stay out of scope, and are named downstream.
