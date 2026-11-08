# MVP Definition

## Objective

Deliver the smallest useful IP Messenger-compatible LAN text messenger that also provides a manageable first target for Rust + Verus verification.

The MVP is a protocol experiment and security engineering project, not a claim of complete compatibility or production readiness.

## User-visible behavior

A user should be able to run two instances on a local network, discover the other instance, and exchange short plain-text messages.

## Included functionality

1. **Peer discovery:** send and receive only the discovery/entry packets needed for the selected compatibility subset; maintain a bounded peer table; define duplicate/departure behavior.
2. **Text messaging:** send and receive short plain-text messages with a bounded message length. Message content is data and is never executed.
3. **Minimal runtime:** a CLI is sufficient. UDP is the initial transport. Socket operations remain outside the verified protocol core.
4. **Verification:** parse arbitrary byte slices within a documented maximum packet size; return a validated typed packet or structured error; prove parser bounds/totality properties and state invariant preservation for the modeled subset; prove parse failure cannot partially mutate state; document uncovered code.

## Excluded functionality

- File transfer, file listing, attachments, and filesystem writes based on received names.
- Authentication, encryption, key management, and trust-on-first-use.
- Group messaging, rich text, images, audio, and arbitrary attachments.
- Persistent history/database, GUI, and mobile clients.
- Full protocol extensions and compatibility with every IP Messenger variant.
- Protection against unlimited packet floods or CPU exhaustion, beyond basic fixed limits.
- Formal verification of OS networking, compiler, runtime, or third-party dependencies.
- Complete timing-channel and whole-system noninterference proofs.

## Candidate acceptance criteria

- Two instances discover one another on a test LAN.
- Two instances exchange bounded plain-text messages using the documented wire subset.
- Malformed, truncated, oversized, unknown, and boundary-valued packets produce controlled errors or documented ignore behavior.
- Parsing is pure and does not mutate peer/message state.
- State transitions are deterministic for the same state and event.
- Peer table and message representation satisfy documented invariants.
- Verus verifies designated proof targets using the repository-pinned verifier.
- CI runs tests and proofs, and fails if verification fails or is skipped.
- Interoperability tests against at least one existing IP Messenger implementation are documented; otherwise the limitation is explicit.
- No claim exceeds the verified scope.

## Suggested milestones

### M0 — Repository/toolchain baseline
- Create Rust workspace.
- Pin a Verus revision and compatible Rust toolchain.
- Add a minimal proof target and CI verification job.
- Document reproducible setup commands.

### M1 — Pure parser
- Define bounded packet input and typed parse output.
- Research the exact MVP wire subset.
- Implement parser and negative tests.
- Prove no out-of-bounds access and that every bounded input returns a result.

### M2 — State machine
- Define peer/message state with explicit invariants.
- Implement discovery/message events as pure transitions.
- Prove invariant preservation for every transition.
- Prove parse errors do not change state by construction.

### M3 — UDP adapter and CLI
- Add socket I/O outside the verified core.
- Feed received datagrams into the bounded parser.
- Exercise two local instances and capture interoperability evidence.
- Document environmental assumptions and runtime limits.

### M4 — Compatibility expansion
- Add commands incrementally, one decision/protocol note at a time.
- Preserve proof coverage as parser and state machine grow.

## Non-goals for the first release

The MVP does not promise resistance to traffic flooding, confidentiality, sender authentication, reliable delivery, or complete compatibility. These require separate designs and threat-model updates.


## Wire-level starting point

The initial command subset and protocol observations are recorded in [protocol.md](protocol.md). The base protocol uses UDP for messages and defaults to port 2425. The first implementation must confirm exact encoding and UTF-8 negotiation behavior rather than assuming every existing peer uses UTF-8.
