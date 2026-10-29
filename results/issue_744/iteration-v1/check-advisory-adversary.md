# Adversarial review — #744 (derived payloads, size classes, tuning flags)

Scope: `patch.diff`, `brief.md`, `check-gates.json`, `gate-logs/`. Rebuilt and re-ran in a
scratch copy of `$PDCA_TARGET` (cargo 1.96.0): `payload_and_sizes.rs` 22/22 green and the
unchanged `cli_surface.rs` 21/21 green. C4-verify's UNVERIFIABLE (exit 77) was declared in
advance in the brief (net-new API), so I re-ran the brief's required reds by hand instead (below).

## Findings

- NEEDS-HUMAN [impl] — **The (C′) red shows up as a hang, not a failure.** `crates/validate/tests/payload_and_sizes.rs:281-287` (`no_piece_exceeds_the_documented_bound_whatever_the_size`) reads the last 64 KiB + 3 bytes of the `10 GiB + 1` and 1 TiB payloads through `Payload::range` with no time limit. I applied the brief's required "regenerate the prefix" mutant (hash blocks `0..offset/32` before every `fill_inside`, `crates/validate/src/payload.rs:171`). `the_last_bytes_of_a_huge_payload_are_produced_directly` (`:336`) fails in 5.0 s as designed, but this test spins. Run alone it was still going when I killed it at 90 s, and the full file at 150 s, so `cargo test -p wyrd-validate` never finishes. `cargo xtask ci` runs plain `cargo test` with no per-test timeout, so CI would report a job timeout, not a named failing test. The C5 log shows the same pattern: 12 TIMEOUT mutants in `fill_inside`/`Pieces::next`. Fix: put the huge-size piece checks inside `within(...)`, or move them into the timed test.

- NEEDS-HUMAN [human] — **(F) cannot catch a drifted chunk-size default, and the brief's reason for skipping it is wrong.** `payload_and_sizes.rs:825` is `assert_eq!(DEFAULT_CHUNK_SIZE, MIB)`, which compares the copied value with a literal and can never go red. I changed the server's `DEFAULT_CHUNK_SIZE` (`crates/server/src/lib.rs:59`) to `4 << 20` and re-ran: **22 passed**. The tool would then still put C at 1 MiB and T at 158 MiB by default, against a server whose single-PUT ceiling is 632 MiB. That is the "stale table" failure the brief's Motivation names. The brief says the constant is "private, so not checkable from a test", but its effect can be observed. `wyrd-server` is already a dev-dependency (`crates/validate/Cargo.toml:53`), and `Gateway::new` uses that default (`crates/server/src/lib.rs:163-168`). A single PUT declared one byte over the limit is refused `EntityTooLarge { declared, limit }` from its headers, before the body is read (`:284-296`, `:417`). `s3_client_roundtrip.rs:326` already starts that gateway in-process. Whether to add a behavioural check here is a scope call.

- NEEDS-HUMAN [human] — **Tunings no server can have are accepted, and their labels then claim boundaries the sizes don't straddle.** Units are refused, so an operator who means "158 MiB" may type `--segment-threshold-bytes 158`. I ran that on the built binary: it is accepted, echoed `= 158 (given)`, and the table puts "the chunk-map segmentation threshold (T)" at 157/158/159 B. Those sizes are smaller than one 1 MiB chunk and can never segment. The run would report coverage of a boundary it never crossed, which is the exact failure the brief's Motivation says this slice prevents. `--chunk-size 4294967296 --segment-threshold-bytes 1024` (T < C) is also accepted. The same gap applies to C: with any `--chunk-size` ≤ 64 KiB, the 64 KiB class keeps the `SubChunk` label, "inside one chunk" (`crates/validate/src/sizes.rs:263-264`), though it spans 16 or more chunks. The server refuses C < 1 MiB (`crates/server/src/lib.rs:104`) and its T is always a whole multiple of C (`crates/core/src/multipart.rs:4992-5002`). The brief says both "a tuning that cannot produce a valid table is refused" and "T is exactly that value"; whether T < C, a T that is not a multiple of C, or C ≤ 64 KiB counts as "cannot produce a valid table" needs a decision (`Tuning::resolve`, `sizes.rs:142-181`).

- NEEDS-HUMAN [impl] — **One overflow refusal does not name its flag.** `--segment-threshold-bytes 18446744073709551615` fits in a `u64`, so it parses, then fails at `crates/validate/src/sizes.rs:230`. The binary prints `wyrd-validate: the segmentation threshold + 1 does not fit in 64 bits`, which never names `--segment-threshold-bytes`. Brief (E) says an overflowing value is "refused by name"; every other path I tried does name its flag (`--chunk-size` and `--max-root-value-bytes` at `u64::MAX` name both flags). No test executes the `Overflow` message: diff-cov lists `sizes.rs:112` as MISS. Fix: name the flag in the messages at `:224`/`:230` and add a CLI case next to `payload_and_sizes.rs:707`.

- NEEDS-HUMAN [impl] — **Mutation survivors show three public surfaces with no test (C5, 14 missed).** First, `Pieces::size_hint` (`crates/validate/src/payload.rs:245`): all 8 mutants survived. That includes `-`→`+`, which over-reports the lower bound (a broken `Iterator` contract) and makes the test's own `.collect()` of the 1 TiB tail range (`payload_and_sizes.rs:284-287`) reserve about 33.5 M `Vec` slots, about 800 MB, and still pass. Second, `Payload::generation()` (`payload.rs:142`) survives being replaced with 0 or 1. Third, `Boundary::label` survives being replaced with `"xyzzy"`, because `payload_and_sizes.rs:456` only checks that the label is non-empty. So the label text required by (D)'s "each class labelled with the boundary it straddles" is never pinned; only the enum variant is. All low severity, and each is a one-line assertion.

## Attempted to refute, could not

- **(A) known-answer hashes.** I rebuilt the construction in Python from the `payload.rs` module docs alone. All 6 whole-payload SHA-256 constants and both pinned 64-byte tails (`10 GiB + 1`, 1 TiB) match exactly, so the docs describe the code precisely enough to reimplement it.
- **The brief's required reds**, each applied in scratch and reverted:
  - generation dropped from the derivation → (B) fails (also A and C′);
  - size dropped → (B)'s size leg fails;
  - C hardcoded at 1 MiB → (D) fails;
  - root budget hardcoded at 50,000 → (D)'s root-budget case fails, along with the CLI and echo legs;
  - range start shifted by one byte → (C) fails;
  - a default echoed as `(given)` → (E) fails.

  The prefix-regenerating red is real but hangs; see the first finding.
- **The derived T matches the server's rule.** `⌊budget/315⌋ × C` matches `VALUE_CHUNK_CAPACITY`/`one_value_bytes` (`crates/core/src/multipart.rs:4940-5002`), and (F) ties both constants and `MAX_MAP_CHUNKS`.
- **Strict parsing holds.** Arabic-Indic digits, `+5`, whitespace, `0x10`, `000`, 2^64, a repeated flag and a trailing flag are all refused by name. Leading zeros (`0004096`) are accepted and echoed as `4096 (given)`: same value, not a defect.
- **Range bounds hold.** `fill`/`range` refuse an out-of-range or reversed range without writing to the buffer; `end == size` and empty ranges work.
- Note, not a finding (out of scope, belongs to #743's capability matrix): T−1/T/T+1 cross the segmentation boundary only for writes cut into whole chunks. A multipart upload whose part size is not a multiple of C goes over 158 chunks before T + 1. The `AtSegmentation` label describes the single-PUT/chunk-aligned case.
- Not raised: T4 review and DCO, per the rubric's reviewer protocol.

<!-- pdca:leaf-complete -->
