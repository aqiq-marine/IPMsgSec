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

The first parser implementation lives in `crates/protocol-core/src/parser.rs`. It is part of the runtime crate and is written inside the Verus `verus!` macro, so the same function is compiled by ordinary Cargo and selected for Verus verification. The crate opts into Cargo-Verus with `[package.metadata.verus]`.

Its current interface is:

```rust
fn parse_packet(input: &[u8]) -> Result<Packet, ParseError>
```

The parser splits exactly the first five ':' delimiters, retains the rest as opaque bytes, parses decimal fields with pre-multiplication overflow checks, and separates the low command byte from the upper option flags. Unknown command IDs remain an explicit `Unsupported` variant.

Two helper contracts now state useful local properties:
- `find_delimiter` returns the first colon at or after the cursor, or proves there is no colon in the remaining suffix.
- `parse_decimal` returning `Ok(value)` implies every input byte is an ASCII decimal digit and `value` equals the mathematical base-10 interpretation of the complete input sequence. A recursive ghost function defines that interpretation, and a push lemma connects one loop iteration to the next. The accumulator checks the u64 overflow condition before multiplication and addition.

The command classifier has a separate logical relation, `command_matches`, connecting each runtime variant to its wire ID. The parser's success postcondition includes non-empty identity fields, the supported version, bounded body length, correct option-bit extraction, and consistency between the command variant and low command byte.

The current test suite contains 16 parser tests and 3 boundary-type tests. Parser cases include successful parsing, colons in the body, option flags, unknown and maximum command values, truncation, missing delimiters, empty identity fields, malformed numeric fields, overflow, unsupported versions, empty additional data, oversized datagrams, maximum packet numbers, leading zeroes, and exhaustive one-/two-byte inputs. These tests are not formal proofs.

Proof obligations:
1. Every input slice satisfying the maximum-length precondition returns a result.
2. The delimiter helper returns the first delimiter, or establishes its absence.
3. Successful decimal parsing implies all bytes are decimal digits; checked accumulation cannot overflow.
4. Every slice access is within bounds.
5. Arithmetic used for offsets and lengths cannot overflow or wrap into an invalid access.
6. Successful parsing produces a value satisfying the Packet invariant and command-ID relation.
7. Failed parsing has no side effects.
8. The parser does not allocate based on an unvalidated packet-supplied length.

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

The first peer-table transition implementation lives in `crates/protocol-core/src/state.rs`. It models a bounded observed-peer cache with a maximum of 64 peers and 64 bytes per identity field. `PeerEvent::Joined` adds or refreshes a peer; `PeerEvent::Departed` removes an exact byte-identity match. A repeated join does not create a duplicate, and its observation timestamp is monotonic (older observations cannot move it backwards). Unknown departures are no-ops. Invalid identities and capacity overflow return errors without mutating the input state.

The state invariant is intended to establish:
- Peer count is at most `MAX_PEERS`.
- Every username and hostname is non-empty and at most `MAX_IDENTITY_BYTES`.
- No two entries have the same byte-exact username/hostname pair.

The transition takes the observation timestamp as an explicit input. It does not read a clock, perform I/O, or mutate the supplied state. Tests cover add, duplicate refresh, stale timestamps, departure, unknown departure, invalid identity, and capacity overflow. The exact Verus proof status is determined by the CI result for the current revision.

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


## Initial executable proof baseline

The repository contains verification/smoke.rs, a standalone checked-offset arithmetic target, and a Cargo-Verus-enabled protocol-core crate. The runtime parser is written inside the crate's verus! module and is selected by the CI command `cargo verus verify -p ipmsgsec-protocol-core`. This design avoids a second proof-only parser. The parser's actual proof status remains pending until that command succeeds on the current source revision.
