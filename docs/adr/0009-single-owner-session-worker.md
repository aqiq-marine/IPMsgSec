# ADR 0009: Single-owner application session worker

- Status: Accepted
- Date: 2026-11-08

## Context

The shared application facade now separates UI-facing DTOs from the protocol
core and UDP adapter, but the CLI still owns the synchronous socket loop.
A GUI must not independently manage socket I/O or mutate protocol state, and
the service needs explicit command, event, queue, and shutdown semantics.

## Decision

1. Introduce one background worker thread per running session.
2. The worker exclusively owns `UdpTransport`, `ApplicationSession`,
   packet-number allocation, and the observed endpoint/capability map.
3. Expose a cloneable command handle with bounded command capacity. A full
   queue returns an explicit error; it never silently grows.
4. Expose a bounded event stream. The worker must not block indefinitely on
   a slow consumer. Events that cannot be enqueued are dropped and counted;
   a later `EventsDropped` event reports the loss. Consumers can request an
   owned snapshot to resynchronize peer state.
5. Snapshot queries are commands with one-shot replies, separate from
   asynchronous state-change events.
6. Graceful shutdown sends BR_EXIT, releases the socket, and joins the worker.
   Dropping the service also requests stop and joins.
7. Initially use `std::net::UdpSocket` with a short read timeout and standard
   channels. Do not introduce an async runtime without measured need.

## Consequences

- CLI and GUI use the same service API; neither directly operates the socket
  or protocol state.
- Event delivery is bounded and explicitly best-effort under overload. This
  is not a durable message queue or delivery acknowledgement.
- UDP send success means local socket submission only.
- The worker/application/transport remain unverified; protocol-core's Verus
  target is unchanged.
- Peer source addresses and capabilities remain unauthenticated network data.

## Implementation evidence

The implementation is in `crates/app/src/worker.rs`; see
[application-session-worker.md](../application-session-worker.md).
