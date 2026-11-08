# ADR 0007: Implement UTF-8 capability negotiation before the application layer

- Status: Accepted
- Date: 2026-11-08

## Context

The current text-messaging increment deliberately supports only non-empty ASCII
message bodies. It does not advertise `IPMSG_CAPUTF8OPT`, does not set
`IPMSG_UTF8OPT`, and does not retain peer capability flags. This is a safe
interoperability subset, but it prevents the application from sending and
displaying general Unicode text correctly.

The next architectural milestone had been planned as a shared application
layer used by both CLI and a future GUI. Building that layer first would make
the current encoding limitation part of the application API and event model.
Encoding and peer capability semantics affect those interfaces, so they
should be settled before the application layer is introduced.

## Decision

Implement UTF-8 capability negotiation as the next protocol milestone, before
the shared application layer and GUI work.

The negotiation milestone must:

1. Represent the relevant peer capability information in the protocol model
   and bounded peer state. Capability data from the network remains
   unauthenticated input.
2. Advertise local UTF-8 capability in the appropriate discovery packets,
   using the exact command-specific option-field semantics from the protocol
   specification.
3. Parse and retain a remote peer's UTF-8 capability advertisement.
4. Set the SENDMSG UTF-8 option only when the destination peer has advertised
   the corresponding capability. Do not infer capability from its name,
   address, or observed message bytes.
5. Decode incoming SENDMSG bodies according to the negotiated wire flag:
   UTF-8 when the message carries the UTF-8 option, and the specified legacy
   encoding (CP932) otherwise.
6. Define explicit behavior for malformed UTF-8, invalid or unrepresentable
   legacy text, unsupported option bits, and missing peer capability state.
   Do not silently reinterpret malformed data using a different encoding.
7. Preserve existing size limits and input bounds. Encoding conversion must
   not bypass the 4 KiB application body limit or the datagram limit.
8. Add protocol-core tests for capability parsing/state transitions and
   transport/application tests for encoding selection and decoding.
9. Update protocol documentation with the exact wire-level behavior and
   interoperability evidence. Do not claim compatibility with external
   clients until tested against them.

Before implementation, verify the precise placement and meaning of
`IPMSG_CAPUTF8OPT` and `IPMSG_UTF8OPT`, including any command-specific
constraints, against the protocol specification and representative client
behavior. This ADR chooses the implementation order and required behavior; it
does not replace that wire-format verification.

The existing ASCII-only behavior remains the compatibility baseline until
this milestone is implemented and tested. The current implementation must not
prematurely advertise UTF-8 support.

## Consequences

- The protocol model and peer state will gain bounded capability information.
- The message API can be designed around decoded text plus explicit encoding
  errors, rather than embedding an ASCII-only restriction into the future
  application layer.
- Legacy peers remain supported through CP932 fallback, subject to the
  protocol-defined negotiation rules and successful conversion.
- The verified protocol core may prove capability-state invariants and pure
  selection logic. Character conversion libraries, UDP I/O, and the complete
  application remain outside the proof boundary unless separately verified.
- This milestone delays the application layer by one PR, but reduces the risk
  of having to revise shared CLI/GUI interfaces later.

## Updated implementation order

1. **Implementation PR 6 — UTF-8 capability negotiation:** capability advertisement and
   tracking, negotiated SENDMSG flags, UTF-8/CP932 encoding and decoding, and
   bounded error handling.
2. **Implementation PR 7 — Shared application layer:** introduce an
   `ipmsgsec-app` crate/API and move communication orchestration out of the
   CLI so both CLI and GUI can use the same service.
3. **Implementation PR 8 — Session lifecycle and event distribution:** own protocol state
   and transport in a background worker/actor; define start/stop, command,
   event, and shutdown behavior.
4. **Implementation PR 9 — Tauri 2 GUI MVP:** peer list, chat view, message composer, and
   network settings, all using the shared application layer.
5. **Implementation PR 10 — Message delivery semantics:** acknowledgements, deduplication,
   retry policy, and explicit delivery status.
6. **Implementation PR 11 — Persistence:** settings and message history, with bounded
   storage behavior and an explicit retention policy.

The numbering is a planning sequence, not a claim that these milestones are
already implemented. Each milestone remains subject to its own design,
tests, review, and CI evidence.
