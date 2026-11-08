# Verification Strategy

## Objective

Use Rust and Verus to establish machine-checked properties of a small protocol core. Build explicit contracts and proof obligations incrementally instead of attempting a whole-application proof at once.

## Evidence levels

- **Designed:** intended property, not yet implemented.
- **Tested:** supported by tests; not a formal proof.
- **Proved:** Verus successfully discharged the stated theorem for the exact source revision and configuration.
- **Assumed:** relied upon but not proved in this repository.
- **Out of scope:** deliberately not covered.

Do not use "verified" without identifying target module, theorem, tool version, source revision, and assumptions.

## Phase 1 — parser

Model the parser as a pure function over a bounded byte slice:

```rust
fn parse_packet(input: &[u8]) -> Result<Packet, ParseError>
```

Proof obligations:
1. Every input slice satisfying the maximum-length precondition returns a result.
2. Every slice access is within bounds.
3. Arithmetic used for offsets and lengths cannot overflow or wrap into an invalid access.
4. Successful parsing produces a value satisfying the Packet invariant.
5. Failed parsing has no side effects.
6. The parser does not allocate based on an unvalidated packet-supplied length.

Start with simple cursor-based parsing and explicit remaining-length checks. Avoid complex generic parsers until the proof baseline is stable.

## Phase 2 — state transitions

Define a state invariant I(s), for example:
- Peer count is at most MAX_PEERS.
- Message count is at most MAX_MESSAGES.
- Peer identifiers are unique.
- Every peer field is within its declared bound.
- Every message body is within MAX_MESSAGE_BYTES.
- Only supported event variants enter the transition function.

Prove:
```text
I(s) ==> I(transition(s, event))
```

for each event variant. If transitions can be rejected, prove rejection preserves the original state. Prefer immutable input + returned output or a verified transition delta.

## Phase 3 — composition

Show that the transport adapter supplies only byte slices satisfying parser preconditions and that the core's returned response is the only protocol output emitted by the adapter.

This may initially be a code review and integration-test boundary rather than a full formal refinement proof. State that limitation honestly. A whole-application claim requires an argument connecting the deployed executable to the verified core.

## Information-flow follow-up

For a later confidentiality milestone:
1. Classify state fields as public, internal, or secret.
2. Define attacker-observable outputs (payloads, lengths, error categories, termination, and optionally timing).
3. Define a low-equivalence relation between states.
4. Prove equivalent public states yield equivalent observations after the same attacker inputs.
5. Include all output and error paths; do not only inspect the happy path.

```text
low_equivalent(s1, s2)
    ==> observe(run(s1, attacker_inputs))
        == observe(run(s2, attacker_inputs))
```

The relation must account for intentionally public information, such as a hostname if the protocol explicitly advertises it.

## Toolchain and reproducibility

- Pin a specific Verus revision and compatible Rust toolchain when implementation begins.
- Pin dependencies and commit the lockfile for executable crates.
- Record the verifier command used by CI.
- Ensure CI fails if a proof target is omitted, skipped, or fails.
- Keep a minimal smoke-proof target to detect toolchain drift.
- Do not state commands or version compatibility as facts until confirmed against the selected Verus revision.

## Complementary validation

- Unit tests for parser branches and state transitions.
- Property-based testing for invariants and round trips where appropriate.
- Coverage-guided fuzzing of the parser.
- Interoperability tests against existing clients.
- Static analysis and dependency review.

These techniques find defects but do not replace proofs.

## Proof review checklist

- What exact source code is covered?
- What are the input preconditions?
- Are attacker-controlled values unconstrained except for documented bounds?
- Does the theorem cover all branches and enum variants?
- Are assumptions bypassing the property?
- Is external behavior modeled faithfully?
- Is the verified function actually called by the application?
- Can an unverified adapter alter/reinterpret data after the proof boundary?
- Does the claim overstate what the theorem establishes?
