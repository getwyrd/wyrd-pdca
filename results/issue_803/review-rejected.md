# Review decisions — #803 (662.1)
# Format: <file:line> | <CLASS> | <MATCH> | <reason>

crates/custodian/src/gc.rs:810 | BUG | checks its budget profile | Deferred — tracked in getwyrd/wyrd#806 (filed 2026-09-16). The mpuctl budget-profile preflight (0016:348, X99 0016:2628) is out of this slice's scope by the iteration-5 sign-off, which asked for the deferral and not the check. The site carries the marker `// deferred: #806` at crates/custodian/src/gc.rs:810, the first line of `staged_fragments`.
