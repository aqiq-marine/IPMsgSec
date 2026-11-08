# ADR 0008: Introduce a shared UI-independent application facade

- Status: Accepted
- Date: 2026-11-08

## Context

The CLI previously owned protocol state, classified inbound events, applied
peer transitions, selected message encodings, and constructed display data.
A future GUI would otherwise need to reproduce that orchestration or call
low-level protocol and transport APIs directly.

The UTF-8 capability milestone has now established the wire-level message
encoding model. The CLI and GUI can share a stable application boundary
without exposing borrowed parser data or encoding assumptions to each UI.

## Decision

Introduce an `ipmsgsec-app` crate as the only application-facing API for UI
clients.

1. The application crate composes protocol-core and transport-udp but does not
   duplicate packet parsing or protocol-state transition logic.
2. UI-facing peer, message, error, and status values are owned DTOs and are
   serializable. Peer identity and raw message body bytes remain byte-preserving.
3. Message DTOs carry the wire encoding and decoded text separately, including
   an explicit decode-error state.
4. The application facade selects UTF-8 only when a destination's observed
   capability indicates support; otherwise it selects CP932.
5. The CLI becomes a client of this facade and must not independently apply
   protocol state transitions.
6. The current application session is synchronous and single-owner. A later
   milestone will introduce worker lifecycle, command/event channels, and
   shutdown/overload semantics.

## Consequences

- CLI and GUI can share peer snapshots, message events, status, and encoding
  selection.
- The application crate remains outside the Verus proof boundary. The
  protocol-core proof target is unchanged.
- UDP source addresses and peer identities remain unauthenticated. DTO
  serialization does not imply trust or identity verification.
- The current session API is not yet a concurrent service. The next milestone
  must define the worker/actor lifecycle before a GUI starts background
  networking.

## Implementation evidence

The initial implementation is in `crates/app`, with the public boundary
documented in [application-layer.md](../application-layer.md). The CLI uses
the application facade for inbound event processing, peer snapshots, and
outbound encoding.
