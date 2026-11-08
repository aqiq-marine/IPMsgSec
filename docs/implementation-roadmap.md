# Implementation Roadmap

## Goal

Build an IP Messenger-compatible LAN text messenger whose protocol core is small, bounded, deterministic, and incrementally verified with Verus. Keep operating-system and network effects outside the verified core.

The project must distinguish properties that are **designed**, **tested**, **proved**, **assumed**, and **out of scope**. A proof claim must identify its source revision, target, toolchain, and assumptions. Verification of a core function does not imply verification of the transport adapter or whole application.

## Architecture

```text
UDP / OS boundary (unverified)
        |
        v
Bounded datagram
        |
        v
Parser (verified)
        |
        v
Protocol model and event classification (incrementally verified)
        |
        v
Pure state transition + abstract protocol actions (verified)
        |
        v
UDP sender / application layer / CLI or GUI (unverified)
```

- The core accepts bounded byte slices and explicit timestamps; it does not read clocks, access sockets, spawn processes, or perform I/O.
- Parsing and protocol classification borrow packet data where practical. Do not allocate from attacker-controlled lengths.
- State transitions return new state rather than mutating the caller's state.
- The transport is responsible for receiving and sending datagrams, interface selection, broadcast behavior, and translating I/O failures.
- CLI and GUI must share one application layer; neither UI should independently orchestrate protocol state and UDP transport.
- Never claim whole-application verification solely from proofs of the core.

## Current baseline

The runtime parser is in `crates/protocol-core/src/parser.rs`. It parses the first five colon-delimited fields, preserves the opaque tail, checks decimal overflow, and retains unknown command IDs as `Unsupported`.

The peer table is in `crates/protocol-core/src/state.rs`. It is bounded to 64 peers and 64 bytes per identity component. Identity matching is byte-exact. Duplicate joins and unknown departures are no-ops. A duplicate join preserves the first observation timestamp; expiration and timestamp refresh are not implemented.

The parser and peer-state modules are selected by the Cargo-Verus verification target. Existing CI also runs Rust tests, formatting, and Clippy. See [verification strategy](verification.md) for the recorded evidence and its exact scope.

## Completed milestones

PR 1 — Protocol model, PR 2 — Discovery transition and abstract actions, PR 3 — UDP transport adapter, PR 4 — IPv4 CLI discovery, and PR 5 — bounded plain-text messaging are implemented and merged. Their design details and acceptance criteria remain in the repository history and the earlier sections of this roadmap.

PR 5 currently supports a conservative ASCII-only SENDMSG subset. It does not advertise UTF-8 capability or negotiate message encoding. See [ADR 0006](adr/0006-conservative-ascii-messaging.md).

## Next implementation sequence

The order below is intentional: complete wire-encoding negotiation before introducing a shared application API, so that CLI and GUI are built on a stable text-message model. See [ADR 0007](adr/0007-utf8-negotiation-before-app-layer.md).

### PR 6 — UTF-8 capability negotiation

- Verify the exact command-specific wire semantics of `IPMSG_CAPUTF8OPT` and `IPMSG_UTF8OPT` against the protocol specification and representative clients.
- Add bounded peer capability state and parse remote capability advertisements.
- Advertise local UTF-8 support in the appropriate discovery packets.
- Select the SENDMSG UTF-8 flag only for peers that advertised support.
- Decode UTF-8-marked messages as UTF-8 and unflagged legacy messages as CP932, following the verified protocol rules.
- Specify deterministic behavior for malformed UTF-8, CP932 conversion failures, unsupported flags, and unknown peer capability.
- Preserve the 4 KiB body limit and datagram bound across encoding conversion.
- Add tests for capability state transitions, outgoing encoding/flags, incoming decoding, malformed data, and legacy-peer fallback.
- Update the protocol reference and interoperability test matrix with actual evidence.

Acceptance: capability state is bounded; unsupported or malformed data is handled explicitly; no peer is treated as UTF-8-capable without an observed advertisement; existing ASCII behavior remains valid; Rust tests, Clippy, formatting, and Verus CI pass. Only pure protocol decisions actually covered by Verus may be described as proved.

### PR 7 — Shared application layer

- Add an `ipmsgsec-app` crate that owns application-level orchestration behind a UI-independent API.
- Move communication orchestration out of the CLI and make the CLI a client of this API.
- Define owned, serializable application DTOs for peers, messages, errors, and status. Do not expose borrowed parser slices or transport-specific socket handles to a GUI.
- Represent encoding and decoding failures explicitly; do not erase the distinction between wire bytes and decoded text.
- Keep protocol-core and transport-udp as lower-level dependencies. The application layer must not duplicate protocol parsing.

Acceptance: CLI behavior is preserved through the shared API; application tests cover message/peer operations; protocol-core's proof boundary is unchanged.

### PR 8 — Session lifecycle and event distribution

- Give one background worker/actor sole ownership of the mutable protocol session and UDP transport.
- Define commands such as start, stop, discover, and send-message, plus events such as peer joined/updated/departed, message received, and transport error.
- Define bounded queues, shutdown behavior, and error propagation.
- Initially wrap the existing synchronous socket implementation with a worker thread and channels unless implementation evidence justifies adopting an async runtime.
- Ensure snapshots can be queried separately from event notifications.

Acceptance: lifecycle and event ordering are tested; shutdown does not leave a worker or socket running; queue limits and overload behavior are explicit.

### PR 9 — Tauri 2 GUI MVP

- Add a Tauri 2 desktop shell with a TypeScript/React frontend.
- Invoke the shared application API through a narrow Rust command boundary.
- Use events for state-change notifications and commands for request/response operations or snapshots.
- Implement peer list, chat view, message composer, and network settings.
- Keep frontend DTOs owned and serializable; never let the frontend directly manipulate protocol-core state or UDP sockets.
- Clearly distinguish observed peer identity from authenticated identity. No authentication is currently provided.

Acceptance: GUI and CLI use the same application layer; GUI tests cover rendering/state mapping where practical; the protocol-core proof target remains unchanged.

### PR 10 — Message delivery semantics

- Specify and implement acknowledgements, duplicate suppression, retry limits/backoff, and delivery status.
- Do not present successful UDP submission as proof of remote delivery.
- Keep retries and message queues bounded.

### PR 11 — Persistence

- Add persistence for settings and message history only after defining schema, migration, retention, and deletion behavior.
- Treat stored message content and peer metadata as untrusted data when loaded.
- Keep filesystem/database operations outside protocol-core.

## Security and verification rules

- Treat every received datagram and every field within it as attacker-controlled.
- Keep parser inputs bounded and avoid lossy conversion before validation.
- Never reinterpret an unsupported command as a supported one.
- Never interpret message bodies as shell commands, paths, format strings, or executable input.
- Bound peer counts, identity lengths, action counts, and future message queues.
- Prove state invariants for every transition variant, including rejected transitions.
- Keep tests and formal proofs as separate evidence categories.
- Pin the verifier and Rust toolchain in CI; ensure CI fails if a proof target is omitted or fails.
- Review whether the deployed executable actually calls the exact functions covered by the proofs.

## Out of scope for the initial MVP

Encryption/authentication, file transfer, read receipts, peer freshness/expiration, and whole-application formal verification. These may be added later as separately specified milestones.
