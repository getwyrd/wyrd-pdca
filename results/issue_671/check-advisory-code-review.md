No introduced correctness bugs found in the reviewed diff. One optional reuse cleanup:

- NEEDS-HUMAN [impl] — Low priority: `crates/server/tests/s3_put_object_ceiling.rs:623` adds a padded Base64 encoder already provided by `wyrd_gateway_s3::checksum::base64_encode` at `crates/gateway-s3/src/checksum.rs:150`. Use that public helper at the trailer-construction call instead of maintaining another implementation. The existing helper is unchanged by this diff, so reuse preserves compilation on the reverted base; this test exercises missing-length rejection, not Base64 correctness.

<!-- pdca:leaf-complete -->
