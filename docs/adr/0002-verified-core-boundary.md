# ADR 0002: Isolate a pure verified protocol core

- Status: Accepted
- Date: 2026-11-08

## Context

Network applications combine attacker-controlled input with sockets, clocks, filesystem access, logging, and application state. Proving the entire application at once creates a large and difficult proof boundary.

## Decision

Separate the system into:
1. An unverified transport/application edge that performs I/O.
2. A small protocol core that parses bounded byte slices, represents validated packets, and computes deterministic state transitions.

The core must not perform socket or filesystem I/O, read the clock, execute processes, or mutate global state. Parse failures must not partially mutate application state.

## Consequences

- Core functions can be verified with explicit contracts.
- The boundary makes the trusted computing base visible.
- Integration must ensure the running application routes received bytes through the verified parser and uses the verified transition result.
- Whole-application guarantees require additional composition/refinement evidence; they are not implied by core proofs.

## Alternatives considered

- Verify the whole network application at once: rejected initially due to scope and external API complexity.
- Parse directly in socket callbacks and mutate global state: rejected because parsing and effects would be entangled.
