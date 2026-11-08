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
UDP sender / CLI (unverified)
```

- The core accepts bounded byte slices and explicit timestamps; it does not read clocks, access sockets, spawn processes, or perform I/O.
- Parsing and protocol classification borrow packet data where practical. Do not allocate from attacker-controlled lengths.
- State transitions return new state rather than mutating the caller's state.
- The transport is responsible for receiving and sending datagrams, interface selection, broadcast behavior, and translating I/O failures.
- Never claim whole-application verification solely from proofs of the core.

## Current baseline

The runtime parser is in `crates/protocol-core/src/parser.rs`. It parses the first five colon-delimited fields, preserves the opaque tail, checks decimal overflow, and retains unknown command IDs as `Unsupported`.

The peer table is in `crates/protocol-core/src/state.rs`. It is bounded to 64 peers and 64 bytes per identity component. Identity matching is byte-exact. Duplicate joins and unknown departures are no-ops. A duplicate join preserves the first observation timestamp; expiration and timestamp refresh are not implemented.

The parser and peer-state modules are selected by the Cargo-Verus verification target. Existing CI also runs Rust tests, formatting, and Clippy. See [verification strategy](verification.md) for the recorded evidence and its exact scope.

## Milestones

### PR 1 — Protocol model

Add a small model between parsed packets and state transitions.

- Introduce a borrowed `PeerIdentity<'a>` with username and hostname accessors.
- Validate both identity components as non-empty and no longer than `MAX_IDENTITY_BYTES`.
- Introduce `ProtocolEvent<'a>` variants for peer join, peer departure, and received text message.
- Introduce a structured `ProtocolError` for invalid identities and unsupported commands.
- Implement a pure `classify_packet(&Packet) -> Result<ProtocolEvent, ProtocolError>`.
- Preserve the message body as borrowed opaque bytes. Do not decode it as UTF-8 at this layer.
- Do not infer command-specific payload formats that are not yet specified. In particular, discovery-command additional data remains ignored by this classification layer until an explicit protocol decision is recorded.
- Add unit tests for all supported commands, unsupported commands, identity bounds, and body preservation.
- Keep the parser and peer-state interfaces stable.

Acceptance:
- The new module is exported by the crate.
- Classification is deterministic, has no I/O or state mutation, and does not allocate.
- The identity bounds match the peer-state bounds.
- The runtime tests, Clippy, formatting, and Verus CI all pass.
- Any property not actually discharged by Verus is labelled tested or designed, not proved.

### PR 2 — Discovery transition and abstract actions

Compose protocol events with the peer table without performing I/O.

- Distinguish `BroadcastEntry` from `AnswerEntry` in the protocol event model; both represent a peer join, but only the former requests a response.
- Add a `ProtocolState` that owns the bounded peer table.
- Represent the datagram source as an opaque `ReplyTarget(u64)` token supplied by the transport boundary, not as a platform socket or IP type.
- Add the abstract action `SendAnswerEntry { target }`.
- Return the next state, peer transition outcome, and a fixed-capacity ordered action list from a pure transition function.
- On every broadcast entry, add or retain the peer and request an answer-entry response. Repeated announcements remain state-idempotent but still receive a response, allowing recovery from a lost prior response.
- On answer-entry, add or retain the peer without responding.
- On exit, remove the exact byte-identity match; an unknown departure is a no-op.
- Reject message events at this discovery-only layer. The input state is immutable and no partial state/action result is returned on errors.
- Prove the state invariant, action bound, and correspondence between discovery event kinds and generated actions.

The initial action list has capacity one by construction. `ReplyTarget` is an adapter correlation token; the transport owns the mapping from that token to a real source address. Do not place socket handles, IP addresses with platform-dependent behavior, or clock reads inside the verified transition function.

### PR 3 — UDP transport adapter

Add a separate `transport-udp` crate or module.

- Bind sockets and receive datagrams outside the verified core.
- Enforce the application packet-size bound before parsing.
- Handle truncation explicitly; do not assume a normal `recv_from` buffer necessarily reports that a datagram was truncated.
- Preserve the datagram source address for directed replies.
- Translate abstract protocol actions into UDP sends.
- Separate socket errors from parse and protocol errors.
- Decide and document interface binding and limited-vs-directed broadcast behavior before claiming broad LAN discovery support.

The adapter is unverified unless a separate refinement argument and platform assumptions are supplied.

### PR 4 — CLI discovery and interoperability

- Provide a minimal CLI to start/stop discovery and display the observed peer list.
- Test with two and three local instances.
- Exercise duplicate entry, departure, malformed datagrams, packet loss/reordering assumptions, and shutdown.
- Select and document a concrete external IP Messenger interoperability target.
- Record packet captures or reproducible test traces for interoperability claims.

### PR 5 — Text messaging

- Compose `SendMessage` events with a bounded message state or delivery interface.
- Set a distinct maximum message-body size; the 8 KiB datagram bound is not automatically the application message limit.
- Specify UTF-8 capability advertisement and message encoding behavior, including legacy-peer behavior, before implementing negotiation.
- Preserve message bytes until an explicit decoding policy is applied.
- Do not add acknowledgements, read notifications, file transfer, or encryption unless separately scoped.

## Protocol decisions to resolve

Track these as explicit decisions or ADRs before broad interoperability claims:

1. Limited broadcast, directed broadcast, or both.
2. Interface selection and socket binding.
3. Identity-field encoding and byte-level validation.
4. UTF-8 negotiation and legacy encoding behavior.
5. Per-command additional-data grammar, including NUL-separated extensions.
6. Whether empty additional data is valid for each command.
7. Packet-number generation and collision behavior.
8. Peer expiration and stale/reordered departure/rejoin semantics.
9. First external interoperability target and version.

Until a decision is made, prefer preserving opaque bytes and avoiding speculative validation.

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

Encryption/authentication, file transfer, GUI, full legacy compatibility, read receipts, message acknowledgements, peer freshness/expiration, and whole-application formal verification. These may be added later as separately specified milestones.
