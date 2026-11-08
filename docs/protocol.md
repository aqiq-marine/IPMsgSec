# MVP Protocol Subset

## Sources

Initial protocol research uses the upstream IP Messenger protocol draft 14 (modified 2017-06-12) and upstream Windows implementation header:

- https://github.com/shirouzu/ipmsg/blob/master/protocol.txt
- https://github.com/shirouzu/ipmsg/blob/master/src/ipmsg.h

The specification states that the default TCP/UDP port is 2425, messages use UDP, and file transfer uses TCP. The command format is textual:

~~~text
version:packet-number:user-name:host-name:command:additional-data
~~~

Example from the specification:

~~~text
1:100:shirouzu:jupiter:32:Hello
~~~

The command field is a 32-bit value represented in decimal text. Its low 8 bits identify the command; upper bits are option flags. The parser must not assume every command value is supported.

## MVP command subset

| Command | Value | Purpose |
|---|---:|---|
| IPMSG_BR_ENTRY | 0x00000001 (1) | Broadcast entry |
| IPMSG_BR_EXIT | 0x00000002 (2) | Broadcast departure |
| IPMSG_ANSENTRY | 0x00000003 (3) | Reply to entry |
| IPMSG_SENDMSG | 0x00000020 (32) | Send a text message |

These command values are confirmed in upstream src/ipmsg.h. MVP initially supports these command kinds without optional flags, except UTF-8 capability negotiation described below. Unsupported option combinations must be rejected or ignored according to an explicit policy; never reinterpret them as another command.

## Discovery flow

1. On startup, broadcast IPMSG_BR_ENTRY.
2. Existing peers add the entrant and reply directly with IPMSG_ANSENTRY.
3. The new instance adds peers from received IPMSG_ANSENTRY packets.
4. On shutdown, broadcast IPMSG_BR_EXIT.

UDP loss is possible. The peer table is an observed peer cache, not authoritative membership. Expiration and duplicate/stale announcement behavior remain to be specified.

## Text messages

IPMSG_SENDMSG carries the message body in additional data. Base message send/receive uses UDP. Receipt acknowledgements and read notifications are not part of the first MVP increment.

Upstream src/ipmsg.h defines:
- IPMSG_CAPUTF8OPT = 0x01000000: UTF-8 capability advertisement.
- IPMSG_UTF8OPT = 0x00800000: UTF-8 option for a packet/message.
- IPMSG_SENDCHECKOPT = 0x00000100: request a receive check.

The first implementation should prefer UTF-8-capable peers and explicitly negotiate/mark UTF-8 according to the protocol. Exact encoding rules for identity fields and compatibility with peers that do not advertise UTF-8 must be settled before claiming interoperability with those peers. Do not assume all legacy peers use UTF-8.

## Parser requirements

The initial runtime parser is in `crates/protocol-core/src/parser.rs`. It currently:
- Splits only the first five ':' delimiters; the additional-data tail remains opaque and may contain colons.
- Parses version, packet number, and command as ASCII decimal numbers, with explicit overflow detection.
- Accepts protocol version 1 only.
- Requires non-empty username and hostname fields.
- Splits command into low 8-bit command ID and upper 24-bit option flags.
- Preserves unknown command IDs as `CommandKind::Unsupported` rather than mapping them to a supported command.
- Returns borrowed byte slices; it does not perform lossy UTF-8 conversion or allocate packet-controlled buffers.

The parser is written inside the Verus `verus!` macro and is a member of the runtime protocol-core crate. This is intended to keep the executable parser and proof target identical; the claim is pending successful CI verification.

Additional requirements:
- Parse from a bounded byte slice; do not first convert the entire datagram using lossy UTF-8 conversion.
- Find delimiters using checked cursor arithmetic.
- Validate required header fields before constructing a typed packet.
- Reject malformed numeric fields, overflow, unsupported protocol versions, missing delimiters, and oversized datagrams with structured errors.
- Treat additional data as opaque bytes until command-specific encoding is validated.
- Account for NUL-separated extension data; do not assume the first NUL or colon always has the same meaning for every command.
- Never interpret message contents as shell commands, format strings, paths, or executable input.
- Preserve unknown command values as unsupported input or reject them; never reinterpret them as a supported command.

## Unresolved protocol decisions

- Whether entry/exit broadcasts target limited broadcast, directed broadcast, or both.
- Whether the app binds all interfaces or a selected interface.
- Exact identity-field encoding and validation.
- UTF-8 behavior with older/non-UTF-8 peers.
- Whether empty additional data is accepted for each command.
- Packet-number generation and collision policy.
- Peer expiration and duplicate/reordered event semantics.
- First interoperability target (client and version).

Resolve each item with a protocol note, implementation test, or ADR before broadening compatibility claims.
