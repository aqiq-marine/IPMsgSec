# Architecture

## Design principles

1. **Small trusted core:** keep code that must be reasoned about small and deterministic.
2. **Parse before acting:** untrusted bytes become typed values before they can cause state transitions.
3. **Effects at the edge:** sockets, clocks, filesystem, terminal, and process APIs stay outside the verified core.
4. **Explicit bounds:** every externally controlled length and collection has a documented maximum.
5. **Proof-oriented data model:** use enums and newtypes to make invalid states harder to represent, then prove remaining invariants.
6. **No implicit security claims:** Rust guarantees, Verus proofs, tests, and environmental assumptions are reported separately.

## Logical components

### Transport adapter (unverified boundary)

- Bind and receive UDP datagrams.
- Enforce the maximum datagram size at the boundary.
- Pass received bytes to the parser.
- Send protocol responses returned by the core.
- Handle socket errors and shutdown.

The adapter must not parse command fields independently or update protocol state directly. It is outside the initial Verus proof boundary.

### Protocol parser (verified core)

Conceptual interface:

```rust
fn parse_packet(input: &[u8]) -> Result<Packet, ParseError>
```

Requirements:
- No I/O, global mutable state, or side effects.
- No unchecked indexing or attacker-controlled allocation.
- Validate all offsets and lengths before access.
- Give unknown commands and malformed fields explicit outcomes.
- Do not mutate application state.
- Make maximum input size part of the API contract.

### Typed protocol model

Use explicit enums for supported commands and newtypes for validated identifiers. Raw command integers and raw text fields should not leak into transition logic unless needed for compatibility.

The parser should be the only constructor for wire-derived validated packet types, unless another constructor has an equally strong contract.

### State machine (verified core)

Conceptual interface:

```rust
fn transition(state: &State, packet: &Packet) -> TransitionResult
```

Prefer returning a new state or transition delta rather than partially mutating shared state. The state machine must be deterministic and independent of wall-clock time and network I/O.

Potential invariants:
- Peer identifiers are unique.
- Peer and message counts remain within declared limits.
- Every stored field respects its bound.
- Message content is data and is never interpreted as code.
- Events for unknown peers have a documented safe outcome.
- Duplicate/reordered events cannot violate representation invariants.

## Data flow

```text
UDP datagram
    |
    v
Transport boundary: enforce MAX_PACKET_SIZE
    |
    v
parse_packet(&[u8])
    |                         |
    | Ok(Packet)              | Err(ParseError)
    v                         v
transition(state, packet)    discard/report bounded error
    |
    v
new valid State / TransitionResult
    |
    v
transport sends explicitly constructed response
```

The error path must not update state. Logging should avoid reflecting arbitrary attacker-controlled data without encoding and length limits.

## Proposed crate/module layout

This is a target structure, not a claim that the files already exist.

```text
crates/
  protocol-core/
    src/
      lib.rs
      bounds.rs
      packet.rs
      parser.rs
      state.rs
      transition.rs
    verus/  # adapt to the pinned Verus workspace workflow
  transport-udp/
    src/
      lib.rs
  app/
    src/
      main.rs
```

The exact Verus workspace layout must follow the pinned toolchain's supported workflow. Do not force a conventional Cargo layout if it interferes with reproducible verification.

## Trust boundaries

| Component | Initial proof status | Treatment |
|---|---|---|
| Parser | Intended verified | Verus proof + tests |
| Typed packet model | Intended verified | Invariants and restricted construction |
| State transition | Intended verified | Verus proof + tests |
| UDP socket API | Assumed / unverified | Narrow adapter, OS/library contract |
| CLI and display | Unverified | Treat output as data; escape terminal control characters |
| Compiler and Verus toolchain | Trusted computing base | Pin versions; document |
| Third-party dependencies | Unverified unless separately analyzed | Minimize and isolate |

## Future extensions

File transfer should be a separate subsystem with a separate threat model. It introduces filesystem path handling, stream framing, partial reads/writes, and potentially large payloads. Do not add it to the MVP parser or state machine as an incidental feature.
