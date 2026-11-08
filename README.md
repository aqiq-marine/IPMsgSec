# IPMsgSec

**IPMsgSec** is a security-oriented, formally verified reimplementation of the IP Messenger protocol in Rust, with [Verus](https://github.com/verus-lang/verus) used to prove selected safety properties.

The long-term goal is an application that is practically interoperable with conventional IP Messenger clients. The first milestone is deliberately smaller: build a minimal, testable protocol core and establish a trustworthy verification workflow before adding the full feature set.

> **Status: early design / MVP.** No security guarantee is claimed yet. A property is considered verified only when its proof is checked by the pinned verification toolchain and its assumptions are documented.

## Goals

- Interoperate with existing IP Messenger implementations where the implemented protocol subset permits it.
- Parse untrusted network input without memory unsafety, malformed-input panics, or unchecked resource growth within declared bounds.
- Keep protocol parsing and state transitions small, deterministic, and amenable to formal verification.
- Use Rust's type and ownership systems together with Verus proofs; do not treat either as a substitute for the other.
- Document the trusted computing base, proof assumptions, and unverified boundaries.

## MVP scope

The MVP is a small LAN text messenger, not the whole IP Messenger feature set.

### Included
- IPv4 UDP receive and send on the conventional IP Messenger port (exact configuration and interoperability behavior will be documented as implementation decisions).
- A bounded parser for the packet subset required by the MVP.
- Minimal peer discovery and a bounded peer table.
- Plain-text message send and receive (UTF-8 capability negotiation with CP932 fallback, 4 KiB encoded-body limit, explicit IPv4 targets).
- Explicit limits on datagram size, field lengths, and in-memory collections.
- Deterministic protocol state transitions, separated from socket I/O.
- Unit tests, adversarial parser tests, and Verus proofs for the pure protocol core.

### Out of scope for MVP
- File transfer and file announcements.
- Encryption, authentication, signatures, and identity assurance.
- Group chat, attachments, rich text, persistence, and GUI.
- Full compatibility with every IP Messenger extension or vendor-specific behavior.
- Resistance to unlimited traffic floods. Basic fixed limits are still required.
- Timing-side-channel noninterference and whole-process information-flow proofs.

## Security properties and proof claims

| Property | MVP target | Notes |
|---|---|---|
| Memory safety in safe Rust core | Rust type/ownership system | No unsafe code in the protocol core. |
| Parser totality for bounded input | Verus proof | Every input within the declared maximum returns a packet or structured error. |
| Bounds and arithmetic safety | Verus proof + checked operations | Validate indices and lengths before use. |
| State invariants | Verus proof | Every accepted event preserves peer/message invariants. |
| No shell/command execution from packets | Architectural restriction + review/tests | Network data is never passed to a shell or interpreted as executable input. |
| No unintended secret disclosure | Future work | A later information-flow policy must define public/secret data and observations. |
| OS / compiler / dependencies | Assumptions and trust boundary | Not proven by protocol-core proofs. |

Verus proofs over the protocol core do **not** automatically prove the correctness of UDP/TCP implementations, the operating system, the compiler, third-party crates, or the complete running application. Any unsafe block, FFI boundary, and network-library behavior must be separately justified.

A parser proof does not prove protocol correctness, and memory safety does not imply information-flow security. Do not describe the application as "formally verified" without stating the exact verified modules and assumptions.

## Architecture

```text
                 UDP socket / OS boundary
                          |
                          v
                 transport adapter
                          |
                  bounded byte slice
                          |
                          v
+------------------------------------------------+
|            verified protocol core              |
|                                                |
|  parser -> typed packet -> state transition    |
|                                                |
|  bounded data types + invariants + Verus specs |
+------------------------------------------------+
                          |
                          v
                 application / CLI
```

Keep socket I/O, clocks, filesystem operations, and UI outside the verified core. Convert external input into bounded values at the boundary. The core should accept borrowed byte slices and return typed results; it should not perform I/O or mutate global state.

## Repository layout

```text
.
├── README.md
├── AGENTS.md
├── docs/
│   ├── README.md
│   ├── architecture.md
│   ├── mvp.md
│   ├── security-model.md
│   ├── verification.md
│   └── adr/
│       ├── README.md
│       ├── 0001-rust-and-verus.md
│       ├── 0002-verified-core-boundary.md
│       ├── 0003-mvp-scope.md
│       └── 0004-no-unsafe-in-core.md
└── (implementation to be added)
```

## Development

The initial Rust workspace, bounded datagram type, and a first pure IP Messenger packet parser are present. The parser is written in the runtime crate's Verus `verus!` module and is selected for Cargo-Verus verification. The pinned toolchain and local commands are documented in [docs/toolchain.md](docs/toolchain.md). The parser now carries local Verus contracts for delimiter search, decimal-digit validation, command classification, and successful packet invariants. These contracts are not considered proved until a successful CI run on the exact commit.

Planned quality gates:
1. Formatting and linting.
2. Unit and integration tests.
3. Parser fuzzing as a bug-finding complement, not a proof.
4. Verus verification of designated proof targets.
5. CI fails closed if a proof target is skipped or verification does not run.

## Protocol references

- [IP Messenger official site](https://ipmsg.org/)
- [IP Messenger protocol specification](https://ipmsg.org/protocol.txt) — confirm wire-format details against the specification and representative clients before implementation.
- [Verus](https://github.com/verus-lang/verus)

Record the chosen packet subset, encodings, delimiters, command flags, and compatibility assumptions in a protocol document before implementing them. Do not infer the wire format from this README.

## Contributing

See [AGENTS.md](AGENTS.md) for repository-wide engineering rules and [docs/](docs/README.md) for design and verification documents. Architectural changes should be recorded as ADRs under `docs/adr/`.

## License

Not yet specified. Do not assume that this project is licensed for redistribution until a license is added.


## Initial implementation baseline

The workspace now includes bounded peer discovery and a text-message CLI with UTF-8 capability negotiation and CP932 fallback. The protocol-core parser and state transition modules are selected by CI's Verus target; the UDP adapter and CLI remain unverified. See:
- [Toolchain setup](docs/toolchain.md)
- [MVP wire protocol subset](docs/protocol.md)
- [Protocol core crate](crates/protocol-core/)
- [Verus smoke proof](verification/smoke.rs)
- [Runtime packet parser](crates/protocol-core/src/parser.rs)
- [CI workflow](.github/workflows/ci.yml)

The smoke proof checks bounded offset arithmetic. CI also verifies the runtime protocol-core target. No end-to-end security theorem is claimed; socket behavior, delivery, peer authenticity, and CLI behavior remain outside the proof boundary.


## CLI peer discovery

A minimal IPv4 discovery CLI is available in `crates/cli`. It requires
explicit bind and announcement destinations; see
[CLI discovery and interoperability](docs/cli-discovery.md) for build/run
examples, same-host multi-instance tests, trust boundaries, and the selected
external interoperability target. This does not imply that external-client
interoperability has already been tested.

## Text messaging

The CLI advertises UTF-8 capability during discovery and selects UTF-8 or CP932
per destination when sending `IPMSG_SENDMSG`. It decodes incoming messages
according to `IPMSG_UTF8OPT` and displays malformed bytes safely. The UDP
adapter and CLI remain unverified; acknowledgements, retries, and attachments
are not yet implemented. See [CLI discovery and messaging](docs/cli-discovery.md).
