# ADR 0005: Explicit IPv4 discovery destinations

- Status: Accepted
- Date: 2026-11-08

## Context

IP Messenger discovery requires broadcast announcements, but the correct
broadcast destination and egress interface depend on the host's network
configuration. Automatically guessing a subnet broadcast or selecting an
interface would make behavior platform-dependent and could send packets on
an unintended network.

The UDP transport already requires an explicit bind address and destination.
The first CLI milestone needs a reproducible way to exercise both LAN
broadcast and multiple local instances.

## Decision

- The CLI requires an explicit IPv4 bind address and at least one explicit
  IPv4 `--announce-to` destination.
- Announcement destinations may be unicast or broadcast and may be repeated.
- The CLI enables the socket's IPv4 broadcast option but does not enumerate
  interfaces, calculate subnet broadcasts, or choose routes.
- The operator selects a specific local interface by binding its address.
  Binding a wildcard address remains an explicit user choice.
- The initial CLI identity encoding is restricted to non-empty ASCII without
  the ':' delimiter. Non-ASCII encoding and UTF-8 capability negotiation
  remain unresolved.
- The initial external interoperability target is the official Windows
  IP Messenger 5.9.1 client. Selecting a target is not evidence that
  interoperability has been tested.

## Consequences

- The same binary can be used for a LAN broadcast test or a deterministic
  multi-process loopback test with distinct local ports.
- Configuration is more verbose but avoids hidden network policy.
- The CLI and transport remain unverified. The protocol-core proof does not
  establish socket, routing, or network behavior.
- A future interface-aware discovery policy must be separately specified and
  tested, preferably in a new ADR.

## Alternatives considered

- Automatically send to 255.255.255.255 and bind 0.0.0.0: rejected because
  it hides interface and routing choices.
- Enumerate interfaces and calculate directed broadcast addresses: deferred
  because it requires platform-specific behavior and a separate test matrix.
