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

These command values are confirmed in upstream src/ipmsg.h. The implementation accepts `IPMSG_CAPUTF8OPT` only on BR_ENTRY and ANSENTRY discovery packets, and accepts `IPMSG_UTF8OPT` only on SENDMSG. Other SENDMSG flags, including SENDCHECK, remain unsupported. BR_EXIT does not carry the UTF-8 capability flag. Unsupported option combinations must never be reinterpreted as another command.

## Discovery flow

1. On startup, broadcast IPMSG_BR_ENTRY with `IPMSG_CAPUTF8OPT` to advertise UTF-8 message capability.
2. Existing peers add the entrant and reply directly with IPMSG_ANSENTRY carrying their own `IPMSG_CAPUTF8OPT` capability advertisement.
3. The new instance records each peer's advertised UTF-8 capability in its bounded peer state.
4. On shutdown, broadcast IPMSG_BR_EXIT without `IPMSG_CAPUTF8OPT`.

UDP loss is possible. The peer table is an observed peer cache, not authoritative membership. The peer table keeps one entry per identity. Repeated joins preserve the first-seen timestamp and refresh the advertised capability; unknown departures are no-ops. Peer expiration and stale departure/rejoin ordering remain unspecified.

## Text messages

IPMSG_SENDMSG carries the message body in additional data. Base message send/receive uses UDP. Receipt acknowledgements and read notifications are not part of the first MVP increment.

Upstream src/ipmsg.h defines:
- IPMSG_CAPUTF8OPT = 0x01000000: UTF-8 capability advertisement.
- IPMSG_UTF8OPT = 0x00800000: UTF-8 option for a packet/message.
- IPMSG_SENDCHECKOPT = 0x00000100: request a receive check.

The CLI chooses the message encoding per destination:
- If the destination has advertised `IPMSG_CAPUTF8OPT`, send the body as UTF-8 and set `IPMSG_UTF8OPT`.
- Otherwise, encode the body as CP932 and leave `IPMSG_UTF8OPT` clear.
- The receiver chooses UTF-8 or CP932 strictly from the incoming SENDMSG option flag. It does not guess or retry with another encoding if decoding fails.
- Both encoders enforce the 4 KiB encoded-body limit, reject NUL and terminal control characters (TAB/LF are allowed), and preserve the 8 KiB datagram bound.
- Malformed UTF-8 or CP932 is reported as a decoding error; the CLI displays the raw bytes as hexadecimal rather than silently inserting replacement characters.
- A message is sent only after the explicit destination has been observed in a discovery packet, so the sender has a capability advertisement. Unknown capability is not treated as UTF-8 support.

The upstream protocol draft specifies that `IPMSG_CAPUTF8OPT` is advertised on BR_ENTRY, ANSENTRY, and BR_ABSENCE. This implementation supports the first two commands; BR_ABSENCE is not in the current command subset. The draft also states that BR-family packets must not set `IPMSG_UTF8OPT`; for non-ASCII identity/group metadata it defines a separate UTF-8 text extension after the legacy group extension. The current CLI continues to restrict username and hostname fields to ASCII and does not implement that metadata extension.

The first UTF-8 increment does not request `IPMSG_SENDCHECKOPT`, send `IPMSG_RECVMSG` acknowledgements, implement retries, or interpret NUL-separated attachment extensions. The protocol-core models capability and message-encoding decisions; the encoding_rs conversion, UDP adapter, and CLI remain unverified boundaries.

## Parser requirements

The initial runtime parser is in `crates/protocol-core/src/parser.rs`. It currently:
- Splits only the first five ':' delimiters; the additional-data tail remains opaque and may contain colons.
- Parses version, packet number, and command as ASCII decimal numbers, with explicit overflow detection.
- Accepts protocol version 1 only.
- Requires non-empty username and hostname fields.
- Splits command into low 8-bit command ID and upper 24-bit option flags.
- Preserves unknown command IDs as `CommandKind::Unsupported` rather than mapping them to a supported command.
- Returns borrowed byte slices; it does not perform lossy UTF-8 conversion or allocate packet-controlled buffers.
- The Verus contract for delimiter search states that the returned delimiter is the first delimiter in the suffix.
- The Verus contract for decimal parsing states that successful parsing implies all bytes are ASCII decimal digits and the returned value equals the mathematical base-10 interpretation of those bytes.
- The parser's success contract states the version, identity-field, size, option-bit, and command-ID invariants.

The parser is written inside the Verus `verus!` macro and is a member of the runtime protocol-core crate. CI has successfully verified the Cargo-Verus target while compiling the same runtime crate; see [verification evidence](verification.md).

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
- Non-ASCII identity/group metadata and the BR-family UTF-8 metadata extension.
- Whether empty additional data is accepted for each command.
- Packet-number generation and collision policy.
- Peer expiration and duplicate/reordered event semantics.
- First interoperability target (client and version).

Resolve each item with a protocol note, implementation test, or ADR before broadening compatibility claims.
