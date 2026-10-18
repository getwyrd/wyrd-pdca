- NEEDS-HUMAN [impl] — `.github/workflows/release.yml:122` (also pinned at `xtask/tests/dist_two_binary_layout.rs:380`): the release version comparison can falsely pass because `$wire` and `$shipped` are unquoted. Duplicate `Server` headers yield newline-separated values, so `[` receives extra operands and errors; inside an `if` condition, `set -e` does not stop execution, and the mismatch branch is skipped. Running the extracted condition under `sh -eu` with wire values `1.2.3` and `2.0.0` separated by a newline, against shipped version `1.2.3`, reproduced an `unexpected operator` diagnostic followed by exit 0. A whitespace-bearing mismatched value also passes. Quote both operands, escaping the quotes for the outer Docker command, update the pinned fixture, and verify malformed/duplicate values fail.

No additional correctness, reuse, simplification, or efficiency findings in this diff.

<!-- pdca:leaf-complete -->
