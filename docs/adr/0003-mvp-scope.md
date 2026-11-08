# ADR 0003: Restrict MVP to LAN peer discovery and plain-text messaging

- Status: Accepted
- Date: 2026-11-08

## Context

The long-term goal is practical equivalence with IP Messenger, but implementing every command and extension at the outset would obscure the initial verification objective.

## Decision

The MVP includes only:
- Minimal UDP-based peer discovery.
- A bounded peer table.
- Short plain-text message send and receive.
- A CLI or similarly small interface.
- Formal verification of parser and state transitions for this subset.

The MVP excludes file transfer, authentication, encryption, attachments, persistence, GUI, and comprehensive resource-exhaustion protection.

## Consequences

- The initial protocol surface is small enough to model and test.
- The application is not yet a full IP Messenger replacement.
- File transfer and other features require separate threat-model review and incremental protocol expansion.
- Exact packet formats and command flags must be confirmed from protocol references and compatibility tests before implementation.

## Alternatives considered

- Full protocol implementation as the first milestone: rejected because it creates a large, difficult-to-verify surface.
- Parser-only demonstration with no networking: useful as an initial sub-milestone, but insufficient as the MVP user experience.
