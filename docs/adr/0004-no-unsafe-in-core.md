# ADR 0004: Prohibit unsafe Rust in the verified core

- Status: Accepted
- Date: 2026-11-08

## Context

The protocol core handles attacker-controlled bytes and is the primary target for memory-safety reasoning. Unsafe code would add proof obligations unnecessary for the initial feature set.

## Decision

No unsafe blocks, FFI, raw-pointer manipulation, or unchecked standard-library APIs are permitted in the verified protocol core.

Unsafe code may be considered at an external boundary only if a concrete requirement arises. Such a change requires:
- A new ADR.
- A narrowly scoped module.
- A documented safety invariant and justification.
- Tests and appropriate tooling.
- Explicit treatment as part of the trusted computing base unless formally verified.

## Consequences

- The core relies on safe Rust abstractions.
- Some low-level optimizations may be unavailable in the core.
- This rule does not automatically make the entire application safe; dependencies and external boundaries remain review targets.

## Alternatives considered

- Permit unsafe code wherever performance improves: rejected for the MVP because performance is not the priority and added trust is undesirable.
