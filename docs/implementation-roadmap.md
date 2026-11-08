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

Add the unverified `transport-udp` crate.

- Bind sockets only at an explicit caller-supplied address; do not silently select an interface.
- Receive into `MAX_PACKET_SIZE + 1` bytes. A full sentinel byte proves the datagram exceeded the 8 KiB application limit even when the standard socket API does not expose a truncation flag.
- Preserve the source `SocketAddr` and pair it with an opaque per-receive `ReplyTarget` token.
- Delegate parsing and classification to `protocol-core`; the adapter must not parse command fields independently.
- Translate `SendAnswerEntry` into a directed UDP response to the source associated with the same received datagram.
- Keep socket I/O errors, oversized datagrams, parser errors, protocol errors, and response-encoding errors distinct.
- Encode identity bytes verbatim without assuming UTF-8; reject `:` in local identity fields because it is the header delimiter.
- Require the caller to supply the answer packet number. Packet-number generation/collision policy remains unresolved.
- Keep broadcast destination and interface policy explicit; this PR does not claim general LAN broadcast discovery.

The adapter is unverified. The `MAX_PACKET_SIZE + 1` sentinel technique depends on the UDP receive contract returning at least the supplied buffer length when a larger datagram is truncated; the unit test exercises this behavior on the CI Linux target. No platform-independent whole-application proof is claimed.

### PR 4 — CLI discovery and interoperability

Implemented in `ipmsgsec-cli`:

- Require an explicit IPv4 bind address and one or more explicit announcement destinations; no interface or broadcast destination is guessed.
- Send BR_ENTRY on startup and BR_EXIT on normal duration expiry.
- Receive bounded UDP datagrams, delegate parsing/classification to protocol-core, apply the pure discovery transition, and send ANSENTRY to the exact source of BR_ENTRY.
- Display the observed peer table. Ignore malformed, unsupported, and non-discovery packets without logging packet bodies.
- Support two/three same-host instances using distinct loopback ports and repeated unicast `--announce-to` destinations.
- Restrict local identity to non-empty ASCII without ':' until legacy/UTF-8 identity encoding is specified.
- Use a system-clock-seeded, checked packet-number counter; this is best-effort and not an anti-replay or uniqueness guarantee.

The CLI is unverified. CI tests are not external interoperability evidence. The first external target is the official Windows IP Messenger 5.9.1 client; the manual test matrix is in `docs/cli-discovery.md`. No external-client test or packet capture is claimed until that matrix is completed.

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
