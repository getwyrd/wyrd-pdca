"""Apply one named mutation to the production files of the worktree given as argv[1].

Usage: mutate.py <worktree> <mutation-id>
Each mutation is a list of (file, old, new) replacements; each `old` must occur exactly once.
"""

import pathlib
import sys

M = "crates/custodian/src/retirement/marks.rs"
S = "crates/custodian/src/retirement/sessions.rs"
R = "crates/custodian/src/retirement.rs"

MUTATIONS = {
    # The two stalls the sign-off named, reintroduced.
    "R1-part-restarts-at-0": [
        (M, "self.by_chunk_part(&entry, &chunks, from).await",
            "self.by_chunk_part(&entry, &chunks, 0).await"),
    ],
    "R2-owned-walk-ignores-bookmark": [
        (S, "Some((entry, marked)) if entry == key.as_slice() && marked as usize <= named_len => {",
            "Some((entry, marked)) if false && entry == key.as_slice() && marked as usize <= named_len => {"),
    ],
    # The `Aborting`-only guard of the owned walk's retirement by chunk id.
    "R3-aborting-guard-bypassed": [
        (S, "let gate = if aborting {", "let gate = if aborting || true {"),
    ],
    # The session listing's page loop stops after the first page.
    "R4-page-loop-stops-after-page-1": [
        (S, "(Some(_), Some((last, _))) => after = Some(last),",
            "(Some(_), Some((last, _))) => {\n                after = Some(last);\n                if after.is_some() {\n                    break;\n                }\n            }"),
    ],
    # A walk step's commit does not spend a step.
    "R5-walk-step-spends-no-step": [
        (S, "    pass.steps_left -= 1;\n    match meta.commit(batch).await {\n        Ok(CommitOutcome::Committed) => {\n            pass.changed = true;\n            emit_walk_step",
            "    match meta.commit(batch).await {\n        Ok(CommitOutcome::Committed) => {\n            pass.changed = true;\n            emit_walk_step"),
    ],
    # The per-chunk listing cap: off by one, and gone.
    "R6a-listing-cap-off-by-one": [
        (M, "if fragments.len() > LISTED_PER_CHUNK {", "if fragments.len() >= LISTED_PER_CHUNK {"),
    ],
    "R6b-listing-cap-removed": [
        (M, "if fragments.len() > LISTED_PER_CHUNK {", "if false && fragments.len() > LISTED_PER_CHUNK {"),
        (S, "if others.len() > LISTED_PER_CHUNK {", "if false && others.len() > LISTED_PER_CHUNK {"),
    ],
    # The brief's six demonstrated reds.
    "M1-sidx-empty-gate-removed": [
        (S, "            Walked::Stop => return Ok(()),", "            Walked::Stop => {}"),
    ],
    "M2-completed-without-tombstone-input": [
        (S, "if !aborting && ctx.tombstone == Tombstone::Retain {",
            "if false && !aborting && ctx.tombstone == Tombstone::Retain {"),
    ],
    "M3a-decrement-not-under-session-precondition": [
        (S, "let mut batch = WriteBatch::new().require(key.to_vec(), prior.to_vec());",
            "let mut batch = WriteBatch::new();"),
    ],
    "M3b-decrement-in-its-own-batch": [
        (S, "        .require(MPUCTL_KEY.to_vec(), ledger)\n",
            ""),
        (S, "    batch = batch.put(MPUCTL_KEY.to_vec(), metadata::encode(&released));",
            "    let _ = ledger;\n    meta.commit(WriteBatch::new().put(MPUCTL_KEY.to_vec(), metadata::encode(&released))).await?;"),
    ],
    "M4-untrusted-deleted-without-marking-by-chunk-id": [
        (M, "let run = self.run(fragments, entry).await?;\n                        if run.covered < fragments.len() {",
            "let run = self.run(&[], entry).await?;\n                        if false && run.covered < fragments.len() {"),
        (S, "let run = step.run(&others, &key).await?;", "let run = step.run(&[], &key).await?;"),
    ],
    "M5-by-chunk-on-a-short-fleet": [
        (R, "if self.fleet.len() == self.operator_fleet_size {", "if true {"),
    ],
    "M6-e2-marks-only-what-the-listing-found": [
        (S, "                (named, Some(chunk))",
            "                (named.into_iter().take(0).collect(), Some(chunk))"),
    ],
    # Survivors of the last C5 run, now targeted.
    "R7-held-step-goes-on": [
        (S, "let any_held = !held.is_empty();", "let any_held = held.is_empty();"),
    ],
    "R8-conflict-not-judged-again": [
        (S, "        Ok(CommitOutcome::Conflict) => match meta.get(key).await? {\n            Some(now) if pass.steps_left > 0 => Ok(Ended::Again(now.to_vec())),",
            "        Ok(CommitOutcome::Conflict) => match meta.get(key).await? {\n            Some(now) if false && pass.steps_left > 0 => Ok(Ended::Again(now.to_vec())),"),
    ],
    "R9-part-meets-no-reads-left": [
        (M, "None if self.reads_left == 0 => {", "None if false && self.reads_left == 0 => {"),
    ],
    "R10-part-session-read-not-charged": [
        (M, "                self.reads_left -= 1;\n                let aborting = match self.meta.get(&mpu_key(upload_id)).await? {",
            "                let aborting = match self.meta.get(&mpu_key(upload_id)).await? {"),
    ],
    "R11-walk-reads-not-charged": [
        (S, "            self.reads_left -= 1;\n            let prior = match read_arm(self.meta, &mark_key, self.event).await? {",
            "            let prior = match read_arm(self.meta, &mark_key, self.event).await? {"),
    ],
    "R12-release-one-none": [
        ("crates/core/src/multipart.rs",
         "        Some(Self {\n            count: self.count.checked_sub(1)?,\n            ..*self\n        })",
         "        let _ = self;\n        None"),
    ],
    # The whole session leg neutralized: not dispatched.
    "M0-session-leg-not-dispatched": [
        (R, "let sessions = sessions::reconcile(ctx, &by_chunk).await?;",
            "let sessions = if false {\n        sessions::reconcile(ctx, &by_chunk).await?\n    } else {\n        Reconciled::Satisfied\n    };"),
    ],
}


def main() -> None:
    root = pathlib.Path(sys.argv[1])
    name = sys.argv[2]
    if name == "--list":
        print("\n".join(MUTATIONS))
        return
    for rel, old, new in MUTATIONS[name]:
        path = root / rel
        text = path.read_text()
        count = text.count(old)
        if count != 1:
            sys.exit(f"{name}: {rel}: expected one match, found {count}: {old!r}")
        path.write_text(text.replace(old, new))


if __name__ == "__main__":
    main()
