# Agent Instructions

These instructions apply to all work in this repository.

## Project intent

IPMsgSec is a Rust reimplementation of a subset of IP Messenger, designed around a small protocol core whose safety properties can be formally verified with Verus. The eventual target is practical interoperability with existing IP Messenger clients. The immediate target is [docs/mvp.md](docs/mvp.md).

Do not expand scope opportunistically. Keep each change small and preserve a clear boundary between verified logic and environmental effects.

## Before changing code

1. Read `README.md`, this file, and relevant documents under `docs/`.
2. Read relevant ADRs under `docs/adr/`. Do not silently contradict accepted decisions.
3. Inspect current code and tests. Do not assume files, APIs, toolchain versions, or protocol details that are not present.
4. For protocol behavior, consult protocol references and/or compatibility evidence. Record ambiguities instead of guessing.
5. If a task changes an architectural decision, add or amend an ADR in the same change.

## Verification-first rules

- Keep the protocol core deterministic and free of I/O.
- Prefer pure functions: bounded bytes -> parse result; state + typed event -> next state/result.
- State invariants explicitly and keep them close to the types and proof obligations they constrain.
- Treat all network input as attacker-controlled, including lengths, delimiters, encodings, command fields, and ordering.
- Validate bounds before slicing, allocating, indexing, or converting.
- Use checked arithmetic for input-derived lengths and offsets.
- Prefer fixed-capacity or explicitly bounded data structures in the verified core.
- Never use `unwrap()`, `expect()`, unchecked indexing, or panic-based control flow on attacker-controlled paths.
- No unsafe code in the verified core. Any proposed exception requires an ADR and a separate safety argument.
- Do not add shell execution, dynamic library loading, or execution of received content.
- Keep parsing separate from state mutation. Parse and validate completely before applying a state transition.
- Error handling must be explicit and must not leave partially updated state.
- Keep external effects (sockets, filesystem, clock, UI, process execution) outside the verified core.
- Avoid hidden global mutable state.

## Verus-specific rules

- Do not label code "verified" merely because it compiles as Rust or has tests.
- Run the repository-pinned Verus verifier for every changed proof target.
- Include exact command and verifier version/revision in verification documentation or CI output.
- For each proof, document preconditions, postconditions, invariants, trusted assumptions, and theorem scope.
- Avoid broad assume statements. Every assumption must be justified and documented.
- Do not use external_body to bypass a proof obligation in security-critical code without an explicit ADR and an issue tracking removal.
- Keep proof-only specifications distinguishable from executable code.
- Ensure CI includes proof targets; a skipped proof is not a passing proof.
- Where Verus does not model an external operation, isolate it behind a narrow interface and state the trust boundary.
- Proofs about pure functions do not establish properties of the complete process unless a composition/refinement argument is also supplied.

## Rust rules

- Follow the pinned toolchain; do not independently upgrade Rust or Verus.
- Keep dependencies minimal, especially in the verified core.
- Avoid unsafe code and FFI in the core.
- Use explicit types for protocol fields; do not pass raw command integers deep into the application.
- Use enums/newtypes for validated commands and identifiers.
- Use bounded lengths and explicit maximums. Packet-provided lengths must not drive unbounded allocation.
- Avoid lossy decoding unless the protocol explicitly requires it. Define and test text encoding behavior.
- Do not log full private message bodies or sensitive packet contents by default.

## Testing rules

- Add positive and negative tests for every parser branch.
- Include truncation at every byte boundary, empty input, maximum-size input, over-limit input, invalid delimiters, invalid numeric fields, unknown command values, and malformed text encoding as applicable.
- Test that parse failures do not mutate application state.
- Test repeated, reordered, duplicated, and stale protocol events.
- Use property-based tests and fuzzing where appropriate. These complement, but do not replace, formal proofs.
- Distinguish test evidence from proof evidence in documentation and commit messages.

## Documentation rules

- Update README/docs when behavior, scope, assumptions, proof status, build steps, or protocol compatibility changes.
- Use ADRs for material architectural decisions. Supersede accepted ADRs with new ADRs instead of rewriting history.
- Never claim support for an IP Messenger command or feature without an implementation and interoperability test.
- Never claim "memory safe", "formally verified", "noninterfering", or "secure" without naming exact scope and evidence.

## Change hygiene

- Keep changes focused and reviewable.
- Do not commit generated artifacts, local toolchain caches, secrets, private packet captures, or machine-specific configuration.
- Do not silently change ports, protocol constants, text encodings, or compatibility behavior.
- If a required fact cannot be verified, mark it unresolved and create a follow-up task rather than inventing a value.
