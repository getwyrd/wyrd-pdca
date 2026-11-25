# Recorded rejections for #1013: one decision, "no seeded DST scenario lands with this change".
#
# Source of the decision: brief.md Scope, out-of-scope list ("seeded DST (#660 owns "two concurrent
# drainers"; add no DST scenario)"), repeated in the iteration-3, iteration-4 and iteration-5
# sign-offs ("Add no DST scenario (#660 owns it)", "unchanged from prior rounds").
#
# Why so many lines for one decision: the gate binds each line to the ONE finding nearest its
# recorded line, and three review passes anchor the same finding at different lines (iteration 5:
# sessions.rs:243, :586 and :841; only one of three decisions bound, so two re-blocked). One line
# per commit site of the session leg, the by-chunk retirement and the new test file lets every such
# anchor find a decision. Class TEST-GAP and the phrase "DST" only, in these four files only: a
# finding of another class, or without that word, is not cleared by any of them.
crates/custodian/src/retirement/sessions.rs:249 | TEST-GAP | DST | Out of scope by the brief ("add no DST scenario", brief.md Scope) and by three sign-offs ("Add no DST scenario (#660 owns it)"). Declined with issue reference #660; the in-code `// deferred: #660` marker at this line states that the session leg's seeded Tier-0 coverage is deferred there. Deterministic coverage ships instead in tests/multipart_session_terminal.rs: mark-before-delete order of every walk and retirement, the terminal delete under a forced rival and a forced ledger move, a walk commit overtaken by another drainer's terminal delete, and GC past the grace after each drain.
crates/custodian/src/retirement/sessions.rs:273 | TEST-GAP | DST | Same decision as sessions.rs:249 (the session listing loop): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:328 | TEST-GAP | DST | Same decision as sessions.rs:249 (the session visit): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:572 | TEST-GAP | DST | Same decision as sessions.rs:249 (the owned entry's delete): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:614 | TEST-GAP | DST | Same decision as sessions.rs:249 (the owned walk's page read): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:760 | TEST-GAP | DST | Same decision as sessions.rs:249 (the owned walk's entries): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:890 | TEST-GAP | DST | Same decision as sessions.rs:249 (the owned walk's commit): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:917 | TEST-GAP | DST | Same decision as sessions.rs:249 (the terminal delete's judgment): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:977 | TEST-GAP | DST | Same decision as sessions.rs:249 (the terminal delete's batch): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:1024 | TEST-GAP | DST | Same decision as sessions.rs:249 (the session commit): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/sessions.rs:1108 | TEST-GAP | DST | Same decision as sessions.rs:249 (the scan position's commit): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement/marks.rs:548 | TEST-GAP | DST | Same decision as sessions.rs:249 (a part's retirement by chunk id): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement.rs:435 | TEST-GAP | DST | Same decision as sessions.rs:249 (the whole-fleet listing): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/src/retirement.rs:531 | TEST-GAP | DST | Same decision as sessions.rs:249 (the session leg's dispatch): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/tests/multipart_session_terminal.rs:86 | TEST-GAP | DST | Same decision as sessions.rs:249, on the new test file: it is a Tokio file over real redb by the brief's own Success criterion ("over `RedbMetadataStore::in_memory()` … driving everything through `reconcile_step`"); no DST scenario lands with #1013, declined with issue reference #660.
crates/custodian/tests/multipart_session_terminal.rs:1113 | TEST-GAP | DST | Same decision as multipart_session_terminal.rs:86 (leg D): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/tests/multipart_session_terminal.rs:1522 | TEST-GAP | DST | Same decision as multipart_session_terminal.rs:86 (leg E): out of scope by the brief and three sign-offs, declined with issue reference #660.
crates/custodian/tests/multipart_session_terminal.rs:3373 | TEST-GAP | DST | Same decision as multipart_session_terminal.rs:86 (the walk's own records and the pass): out of scope by the brief and three sign-offs, declined with issue reference #660.
