# Adversarial review — #772 (multipart owned staging entry)

**Bottom line:** I could not break the fix. The red→green evidence holds up when rebuilt independently. The one gating failure (T4) describes behavior the base code already had, not something this patch introduced. Whether that blocker stands is a human call.

## Findings

- NEEDS-HUMAN [human] — **The T4 gating blocker looks like old behavior, not a regression.** T4 flags `crates/core/src/metadata.rs:1674` (`decode_pending_entry` accepts `"owner":null` and unknown fields, so `renew_pending` "can CAS successfully and silently rewrite/drop fields"). I ran `renew_pending(&[chunk], now=1000, lease 4500)` over three stored `pending:` values, first on the base and then on the patched tree:
  - `{"lease_expiry_millis":1500,"x":1}`: **both** return `Committed` and store `{"lease_expiry_millis":4500}`.
  - `{"lease_expiry_millis":1500,"owner":null}`: **both** return the same.
  - An owned value (both fields present): the **base** returns `Committed` and erases the ownership fields. The **patched** code returns `Err` and leaves the bytes untouched.

  The `"owner":null` example loses nothing, because null means absent. The base already dropped unknown fields: its derive ignored them, and `renew_pending` stores the caller's entry by design (`metadata.rs:2146`). The rubric's identity rule applies only "wherever a compare-and-swap or content hash depends on it". Both `pending:` compare-and-swaps pin the raw bytes they read (`metadata.rs:2146`, `:2181`), not a re-encoding, so none of them depends on decode→encode identity. The patch explains why the wire stays open (`metadata.rs:1586-1589`: stored records read by a fleet running mixed versions). Closing it would change the `pending:` format, which the brief never asked for. I'd suggest declining with an issue reference under the rubric's out-of-scope rule rather than rebuilding, but the decision belongs to the human.

- NEEDS-HUMAN [human] — **The namespace rule lives in the entry point, not in the codec.** The store-wide `metadata::decode::<PendingEntry>` still reads an owned value as a plain `PendingEntry`: `metadata.rs:1593` and `:1626` apply only the pairing check. The tests pin this on purpose (`crates/core/tests/multipart_owned_staging.rs:314`, `:431`). All four current readers now use the new entry point, and a negation covers each one (see below). But on the base, all four readers used `let e: PendingEntry = metadata::decode(&v)?`. The next `pending:` reader written that way will accept an owned entry, and neither the compiler nor the runtime will object. That is the "invariant becomes a convention" outcome the brief's invariant section warns about. `decode_owned_entry` reads through its own `OwnedEntryWire` (`crates/core/src/multipart.rs:3613`) and never through `PendingEntry`'s `Deserialize`. So the ordinary-only rule could sit inside `PendingEntry`'s own `try_from` without affecting `sidx:`. The brief left the mechanism to the builder, and 0016 plans one renewal loop for both shapes, so this is a design choice, not a defect. Low priority.

- Minor, no action needed: the S10 docs addition (`docs/design/architecture/05-building-block-view.md:202`) is 164 words. The ADR-0047 bullet the brief named as the length model (`:187-194`) is 92 words. The content is accurate. I checked the "off unless an operator arms it" wording against `crates/custodian/src/gc.rs:172-175`, and confirmed that `write::sweep_expired_leases` has no production caller.

## What I tried and could not refute

- **Re-ran the evidence** in a scratch copy: `cargo test -p wyrd-core --test multipart_owned_staging` passed 14/14 and `-p wyrd-custodian --test gc` passed 11/11. Re-running the built binaries 30 and 60 times produced no flaky failures. C4-verify's RED leg is the pre-declared compile failure (`gate-logs/C4-verify.log`), so I rebuilt the brief's eight isolating negations myself instead of relying on the builder's account. Each one fails **exactly one** test:
  - S1, scheme check off: `s1_…`
  - S2, owner/key check off: `s2_…`
  - S3, pairing forced to `Ok`: `s3_…`
  - S4, `renew_pending` on the generic decode: `s4_renew_pending_…`
  - S4, `live_lease_guards` on the generic decode: `s4_a_leased_commit_…`
  - S5, sweep `continue`→`break`, or `Ok` on skip: `s5_…`
  - S6, reject a length mismatch: `s6_…`
  - S7, drop `owner`'s `skip_serializing_if`: `s7_…`
  - S8, drop `require_canonical`: `s8_…`
- **Extra negations beyond the brief, all caught:**
  - GC on the generic decode, GC `continue`→`break`, the GC audit call silenced, the GC counter removed, and GC skipping the next *readable* entry after an unreadable one (the order-dependent bug round 2 found). Each fails the GC leg, which runs every seed in both scan orders.
  - The write sweep on the generic decode fails `s5_…`.
  - Dropping the writer-side check from `put_pending` or `renew_pending` fails `s3_…` and `s4_the_pending_writers_…`.
  - Dropping the pairing check from `PendingEntryWire::try_from` fails `s3_…`.
- **Production path:** the tests call the real `decode_pending_entry`, `renew_pending`, `create_leased`, `put_pending`, `sweep_expired_leases` (over redb) and `reconcile_step` under `ExpiredPendingPolicy::Reclaim`. Nothing is mirrored or re-implemented.
- **Missed readers:** every production read of a `pending:` value now goes through the new entry point (`metadata.rs:2141`, `:2177`, `write.rs:655`, `gc.rs:504`). `crates/custodian/src/restore.rs:731` reads keys only.
- **Bypass inputs on `pending:`**, all refused by `decode_pending_entry` in a probe:
  - `\u`-escaped field names: namespace mismatch.
  - Duplicate `owner` or `staged` with a trailing `null`: duplicate-field error.
  - `"staged":null` beside an owner: torn value.
  - An owned value with an unknown field nested in `staged`: malformed-value error.
  - Duplicate `lease_expiry_millis`: duplicate-field error.

  No owned-shaped input reads as an ordinary lease. On `sidx:`, the closed wire and the canonical-bytes check also refuse `null` spellings, reordered fields and extra whitespace.
- **GC fails safe:** a skipped chunk never enters `expired_pending` (`gc.rs:173`, `:204`) and has no orphan record, so it lands in the "no evidence, keep it" branch (`gc.rs:207-211`). Its `pending:` entry is never added to `swept_pending`, so it is not deleted.
