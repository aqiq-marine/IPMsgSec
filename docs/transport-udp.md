# UDP Transport Adapter

## Status

This crate is an unverified operating-system boundary. Verus proofs for
protocol-core do not cover socket behavior, source-address authenticity,
packet delivery, scheduling, or this adapter's Rust implementation.

## Receive path

1. The caller binds an explicit SocketAddr; the crate does not select an
   interface or bind a wildcard address on the caller's behalf.
2. Receive uses a buffer of MAX_PACKET_SIZE + 1 bytes.
3. A returned length greater than MAX_PACKET_SIZE is rejected before a
   BoundedDatagram is constructed or the parser is called.
4. The source SocketAddr is retained with the datagram.
5. ReceivedDatagram::classify delegates to parse_packet and
   classify_packet; it does not implement a second parser.

The extra byte is a sentinel. With a datagram larger than the receive buffer,
the standard UDP receive API may truncate silently, but the returned length
cannot be less than the buffer capacity. Since the buffer is one byte larger
than the protocol limit, the adapter can still reject the datagram. The
loopback test covers this behavior on the CI Linux runner; other targets need
their own validation.

## Reply path

The received datagram carries both its source address and a ReplyTarget
token. The token is opaque to protocol-core and is paired with the source
address only in the unverified adapter result. send_action requires the
action's token to match the token on that exact received datagram, then sends
the encoded answer-entry packet to the retained source address.

The caller supplies the packet number and local identity. This avoids
silently choosing a packet-number generation policy or a legacy text encoding.
Identity bytes are emitted verbatim; ':' is rejected because it is the header
delimiter. No UTF-8 validity is assumed.

## Explicitly unresolved

- Limited broadcast versus directed broadcast.
- Interface enumeration, binding, and multi-homed host behavior.
- Packet-number generation and collision policy.
- Identity encoding and interoperability with legacy peers.
- Whether empty additional data is accepted by every peer implementation.
- Authentication of source addresses (UDP source addresses are not identities).

## Error categories

- ReceiveError: socket I/O, oversized datagram, or reply-token exhaustion.
- InboundError::Parse: malformed packet or unsupported version.
- InboundError::Protocol: inconsistent/unsupported protocol event.
- SendError: action/source mismatch, response encoding, or socket send error.

No malformed packet is converted into a peer-state transition by this crate.
