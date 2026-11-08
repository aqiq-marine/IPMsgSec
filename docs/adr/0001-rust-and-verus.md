# ADR 0001: Rust and Verus as the implementation/verification foundation

- Status: Accepted
- Date: 2026-11-08

## Context

The project aims to reimplement IP Messenger while proving selected properties about untrusted network input. The implementation should provide strong memory-safety defaults and support machine-checked functional proofs.

## Decision

- Implement the application in Rust.
- Use Verus to specify and prove properties of the designated protocol core.
- Pin the Rust toolchain and Verus revision to a mutually compatible pair.
- Treat Rust's type/ownership guarantees and Verus theorems as distinct evidence.

## Consequences

Positive:
- Safe Rust eliminates broad classes of memory errors in code that remains within safe Rust.
- Enums, newtypes, and ownership can make invalid representations harder to construct.
- Verus can express and prove additional invariants and functional properties.

Costs and limitations:
- Verus introduces toolchain and proof-maintenance burden.
- Not all Rust crates or runtime behavior are automatically covered by Verus.
- The verifier, compiler, OS, and external dependencies are in the trusted computing base unless separately verified.
- A Verus proof over a pure core does not alone prove the whole application.

## Alternatives considered

- C/C++ plus Frama-C/CBMC: useful for analyzing existing code, but less aligned with a clean-sheet implementation.
- Rust without Verus: strong baseline memory safety, but insufficient for explicit formal-proof goals.
- Lean/Coq specifications with a separate Rust implementation: powerful, but requires an additional refinement/correctness bridge.
