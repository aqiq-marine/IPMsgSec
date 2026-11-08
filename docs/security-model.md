# Security Model

## Purpose

Define what IPMsgSec is intended to defend against and what its initial proof claims mean. This document is a scope statement, not evidence that the implementation already satisfies these properties.

## Assets

- Process memory and control flow.
- Integrity of peer and message state.
- Local user data and message content.
- Host information exposed through protocol responses.
- Filesystem and operating-system capabilities (deferred from MVP).

## Attacker model

For the MVP, assume an attacker can:
- Send arbitrary UDP datagrams to the application from the local network.
- Choose arbitrary bytes, lengths, delimiters, command values, text encodings, and ordering.
- Send truncated, oversized, duplicated, stale, reordered, and semantically inconsistent packets.
- Send packets without following the expected discovery/message sequence.
- Cause the application to receive any finite sequence of packets, subject to the explicit per-packet input bound.

Do not assume a packet's source address or claimed sender name/identifier is authentic. The MVP has no authentication mechanism.

The initial model does not promise availability under an unbounded flood. It does require fixed per-packet limits so an individual datagram cannot request arbitrary allocation.

## Security goals

### G1 — Memory safety of the verified core

For inputs satisfying the core API's documented bounds, execution of the verified core has no out-of-bounds access, invalid reference, use-after-free, double-free, or other memory-safety violation within the Verus/Rust model.

Safe Rust prevents many memory-unsafe operations by construction. Verus proofs cover additional functional obligations and any code for which Verus provides explicit reasoning.

### G2 — Parser totality and controlled failure

For every byte slice within the declared maximum length, parsing returns either a valid typed packet or a structured error. Malformed input must not cause a panic, undefined behavior, or partial mutation of application state.

This is bounded-input totality, not a guarantee that processing unlimited traffic is resource-bounded.

### G3 — State invariant preservation

If the initial state satisfies invariant I, each accepted packet transition produces a state satisfying I. Rejected packets leave the state unchanged.

```text
I(s) && parse(input) == Ok(packet)
    ==> I(transition(s, packet))

parse(input) == Err(error)
    ==> state_after == state_before
```

### G4 — No interpretation of message text as executable input

Message bodies are data. The application must not pass received content to a shell, command interpreter, dynamic loader, or equivalent execution interface.

This is an architectural property supported by API review and tests. A stronger proof requires modeling effects and proving the relevant call graph cannot produce a command-execution event. Rust type safety alone does not establish it.

### G5 — No unintended information disclosure (future goal)

Define a policy identifying public and secret state, then prove that states equal on public components produce equivalent observable network outputs. This is not an MVP guarantee.

Noninterference must specify the observer and all relevant observations. Network payload equality alone does not cover timing, error behavior, packet sizes, or termination.

## Explicitly excluded goals

- Availability against unlimited packet floods or CPU-exhaustion attacks.
- Sender authentication, integrity against spoofing, confidentiality, and replay protection.
- Security of the OS, network stack, compiler, runtime, or third-party dependencies.
- Security of unverified UI/CLI output and terminal emulators beyond safe escaping practices.
- Filesystem safety and file-transfer path confinement (file transfer is out of MVP).
- Whole-process control-flow integrity or arbitrary-code-execution impossibility as a standalone theorem.

## Important distinctions

- **Memory safety is not semantic correctness.** A memory-safe program can still execute an unintended command or update the wrong peer.
- **Type safety is not protocol validation.** A valid Rust String can still violate protocol rules or policy.
- **Memory safety is not confidentiality.** In-bounds reads can still disclose data through outputs.
- **Parser proof is not whole-program proof.** The transport adapter and external environment remain outside the proof boundary unless separately verified.
- **Tests and fuzzing are not proofs.** They are valuable complementary evidence.

## Trusted computing base and assumptions

The proof report must enumerate:
- Verus verifier version/revision and configuration.
- Rust compiler/toolchain version.
- Any assume statements, external_body annotations, trusted specifications, or axioms.
- Unsafe code and FFI (the verified core should contain none).
- Third-party APIs whose behavior is assumed.
- OS/socket behavior and runtime integration assumptions.

Any change to these assumptions requires re-evaluating the security claims.
