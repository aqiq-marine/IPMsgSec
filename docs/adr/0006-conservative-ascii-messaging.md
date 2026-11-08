# ADR 0006: Conservative ASCII-only plain-text messages

- Status: Accepted
- Date: 2026-11-08

## Context

IP Messenger's SENDMSG body encoding depends on peer capability. The protocol
defines CP932 as the legacy default and permits UTF-8 when peers advertise
`IPMSG_CAPUTF8OPT` and mark a message with `IPMSG_UTF8OPT`. The current
discovery state stores identities but does not retain capability flags, so
the application cannot yet correctly negotiate message encoding.

The first messaging milestone should be useful without silently assuming
that a peer supports UTF-8. It must also avoid terminal-control injection
when displaying attacker-controlled network data.

## Decision

- The initial sender emits unflagged `IPMSG_SENDMSG` packets containing
  non-empty ASCII bodies only.
- Printable ASCII, TAB, and LF are allowed. NUL and other control bytes are
  rejected; attachment extensions are not supported.
- The application-level body limit is 4096 bytes, independent of the
  8192-byte datagram limit. The core rejects empty and oversized bodies and
  NUL-separated extension payloads on receive.
- No `IPMSG_CAPUTF8OPT` capability is advertised, no `IPMSG_UTF8OPT` flag
  is set, and no `IPMSG_SENDCHECKOPT` acknowledgement is requested. The
  initial receiver rejects all SENDMSG option flags rather than partially
  interpreting unimplemented extensions.
- Received message bytes remain opaque and are rendered with byte escaping.
  The CLI does not decode incoming bodies as UTF-8 or emit raw control bytes
  to the terminal.
- Message destinations are explicit IPv4 socket addresses. The current peer
  cache does not establish authenticated endpoint bindings.

## Consequences

- ASCII message bodies have identical byte representation in CP932 and UTF-8.
  This provides a narrow interoperable subset without guessing a peer's
  character encoding.
- Non-ASCII outbound text is rejected, and incoming non-ASCII text may not be
  human-readable until capability tracking and CP932/UTF-8 decoding are added.
- Delivery acknowledgements, retries, read receipts, and file attachments
  remain out of scope.
- The CLI and UDP adapter remain unverified; protocol-core proofs do not
  establish network delivery or message authenticity.

## Alternatives considered

- Always set `IPMSG_UTF8OPT`: rejected because the current peer state does
  not track the remote capability advertisement.
- Assume CP932 for all messages: deferred until a tested encoding/decoding
  implementation is available and behavior for non-Windows peers is specified.
- Accept arbitrary bytes and print them directly: rejected because terminal
  escape/control sequences could be injected by an untrusted sender.
