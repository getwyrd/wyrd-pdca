- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:1045`: A PUT with status 200, a strong ETag and `request_body_ended: true` returns `Pass` without inspecting its response body. Thus `Body::Failed` can produce a run-level PASS despite an incomplete response; nonempty response bytes also pass despite the client's empty-body contract. Check the body before returning success: transport failure should leave the row unsettled, and an unexpected body should be `protocol`. Add both fixtures while preserving the existing early-acknowledgement failure.

- NEEDS-HUMAN [impl] — `crates/validate/src/matrix.rs:1092`: A matching conditional GET with status 304 and `Body::Failed` falls into “answered 304 with a body,” producing fatal `protocol`. A timeout/reset does not establish that forbidden bytes arrived, so this manufactures FAIL where the observation only supports INCONCLUSIVE. Handle `Body::Failed` as unsettled and retain the fatal result for actual nonempty bytes. A shared empty-body checker could cover this and the PUT case consistently.

- NEEDS-HUMAN [impl] — `crates/validate/tests/capability_matrix.rs:653`: The bulk-delete fixtures do not distinguish correctly associated probes from probes of other keys. Frozen `gate-logs/C5-mutants.log` confirms that changing `k == key` to `k != key` at `crates/validate/src/matrix.rs:1225` survives. Add a three-key fixture with ordered HEAD results `a:404, b:404, c:200` and require fatal `oracle`: the surviving mutation checks b for a and a for both b and c, falsely passing without examining c's readable result.

No separate efficiency findings. Findings are advisory; no gate was rerun.

<!-- pdca:leaf-complete -->
