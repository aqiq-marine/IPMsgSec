# Documentation

This directory contains the design, scope, security model, and verification strategy for IPMsgSec.

## Documents

- [MVP scope](mvp.md): features, exclusions, acceptance criteria, and milestones.
- [Architecture](architecture.md): module boundaries and data flow.
- [Security model](security-model.md): attacker capabilities, assets, guarantees, and assumptions.
- [Verification strategy](verification.md): proof targets, methodology, trusted computing base, and evidence levels.
- [Architecture Decision Records](adr/README.md): rationale for durable design decisions.

## Updating documentation

When changing protocol scope, safety claims, trust boundaries, or architecture, update the relevant document and add an ADR when the change is material. Keep statements about implemented behavior separate from plans.

## Open questions

Resolve these during implementation and record the evidence:
- Exact IP Messenger protocol revision and command subset for MVP.
- Exact packet encoding and delimiter handling for supported commands.
- Broadcast address and interface selection behavior.
- Handling of duplicate peer announcements, packet ordering, and stale peers.
- Compatible Rust edition/toolchain and Verus revision.
- Whether a later MVP increment adds TCP message bodies or remains UDP-only.
