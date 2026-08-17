---
name: Minto Pyramid
description: >-
  Answer-first reports for the harness's prose leaves — the governing thought,
  then the key line, then detail grouped under the argument it supports.
  Instance #235; retires when upstream eduralph/pdca-harness#535 ships this
  template-side.
---

# Report shape — the Minto Pyramid

When you write prose a human (or a downstream leaf) must digest — a verdict, a
recommendation, a proposal, a rationale — you MUST structure it as a pyramid,
top down:

1. **Governing thought first.** Your first sentence states the conclusion: the
   verdict, the answer, the recommendation, the size band. No preamble, no
   throat-clearing, no suspense. A reader who stops after one sentence leaves
   with the outcome. (Answer-first also degrades gracefully: if the report is
   clipped in a terminal, a template section, or a tracker comment, the
   conclusion survives the cut.)

2. **Key line next.** The 2–4 arguments that jointly carry the conclusion —
   mutually exclusive (no argument restates another), collectively exhaustive
   (nothing the conclusion rests on is missing), ordered by importance.

3. **Detail last.** Evidence grouped *under the argument it supports*: the
   file:line, the command output, the measurement, the counter-case you probed.
   Never a chronological narrative of your process — "first I looked at X, then
   I ran Y" is the shape to avoid. The reader follows your argument, not your
   afternoon.

The same ordering applies fractally: inside a section, inside a long bullet,
inside a rationale field — state the point, then support it.

## Boundaries — what this style never overrides

- **Machine-parsed output wins, always.** Where your role mandates a format — a
  JSON file, a decision token, `- **Label:** value` lines, a template's sections
  or delimiters — produce exactly that format, unchanged. Apply the pyramid
  *inside* the free-text parts (a rationale string, a summary paragraph), never
  to the mandated structure itself.
- **Content is untouched.** This governs ordering and grouping only: the same
  facts, the same caveats, the same role boundaries — conclusion first.
