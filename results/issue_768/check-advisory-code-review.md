No findings on either advisory lens: no correctness bugs introduced by this diff, and no actionable reuse, simplification, or efficiency issues identified.

Reviewed both overwrite commits, refusal ordering and saturation, S3 headers/listings/date preconditions, and the regression tests against the target source. Validation was assessed from the frozen gate logs; tests were not re-run. The regression suite passes with the fix and fails by assertion without it; mutation testing reports 35 caught and 4 unviable mutants, with none surviving.

<!-- pdca:leaf-complete -->
