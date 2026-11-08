# Shared application layer

## Responsibility

`ipmsgsec-app` is the UI-independent application facade shared by the CLI
and future desktop frontends. It sits above the protocol core and UDP adapter:

```text
CLI / future Tauri frontend
          |
          v
   ipmsgsec-app
   - ApplicationSession
   - owned serializable DTOs
   - text encoding selection
          |
          +------> ipmsgsec-protocol-core
          |        verified parsing/classification/state transitions
          |
          +------> ipmsgsec-transport-udp
                   unverified socket and encoding boundary
```

The CLI no longer imports protocol-core directly. It supplies received
datagrams to `ApplicationSession`, consumes owned `AppEvent` values, queries
owned peer snapshots, and asks the application facade to encode an outbound
message for a peer's observed UTF-8 capability.

## API shape

- `ApplicationSession::handle_datagram` classifies an already bounded
  `ReceivedDatagram`, applies discovery transitions, and returns an
  application event plus an optional abstract response action.
- `ApplicationSession::peers` returns an owned peer snapshot.
- `ApplicationSession::status` returns a serializable status snapshot.
- `encode_message_for_peer` selects UTF-8 only when the caller supplies a
  previously observed positive capability; otherwise it selects CP932.
- `AppEvent`, `PeerInfo`, `ApplicationErrorDto`, and
  `ApplicationStatus` are owned serializable data transfer objects.

Message DTOs preserve both the original wire body bytes and the selected
encoding. Successfully decoded text is supplied separately. A decoding error
is represented explicitly; malformed bytes are not silently replaced or
reinterpreted. Peer identity components remain byte arrays rather than lossy
UTF-8 strings.

## Trust and verification boundaries

The application crate is not Verus-verified. It composes the verified
protocol-core with the unverified UDP adapter and character conversion.
The core's proof boundary is unchanged. The application facade does not
authenticate peer identities or source addresses; capability advertisements
and peer metadata remain unauthenticated network input.

`SessionUpdate` is an internal Rust integration type because its optional
`ProtocolAction` is consumed by the UDP adapter. UI-facing event and snapshot
DTOs do not expose `ReplyTarget`, `SocketAddr`, socket handles, borrowed
parser slices, or `ProtocolState`.

The synchronous `ApplicationSession` remains the deterministic per-datagram
state processor. `ApplicationService` now places it and the UDP transport
behind one worker thread, exposing bounded commands, a bounded best-effort
event stream, independent snapshots, and explicit shutdown. See
[Application session worker](application-session-worker.md).
