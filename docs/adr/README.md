# Architecture Decision Records

ADRs record durable decisions about architecture, dependencies, security boundaries, and project scope.

## Status values

- **Proposed:** under discussion; not yet adopted.
- **Accepted:** current decision.
- **Superseded:** replaced by a later ADR.
- **Rejected:** considered and declined.

Accepted ADRs should generally not be edited to rewrite history. Add a new ADR that supersedes the old one.

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-rust-and-verus.md) | Rust and Verus as implementation/verification foundation | Accepted |
| [0002](0002-verified-core-boundary.md) | Isolate a pure verified protocol core | Accepted |
| [0003](0003-mvp-scope.md) | Restrict MVP to LAN peer discovery and plain-text messaging | Accepted |
| [0004](0004-no-unsafe-in-core.md) | Prohibit unsafe Rust in the verified core | Accepted |\n| [0005](0005-explicit-ipv4-discovery-destinations.md) | Require explicit IPv4 discovery destinations | Accepted |

When a decision changes, add a new numbered ADR and update this index.
