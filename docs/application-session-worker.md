# Application session worker

## Ownership and lifecycle

`ipmsgsec-app` exposes `ApplicationService` and a cloneable
`ApplicationHandle`. Starting a service binds the explicitly configured UDP
address and transfers the transport to one named worker thread. That worker is
the sole owner of:

- the `UdpTransport` socket,
- the mutable `ApplicationSession` / protocol state,
- packet-number allocation,
- the map from observed UDP source endpoints to peer capabilities.

Neither the CLI nor a future GUI owns or calls the socket directly.

## Commands and snapshots

The handle accepts bounded commands:

- `discover()`: send BR_ENTRY to every explicitly configured destination.
- `send_message(address, text)`: send to an endpoint currently observed through
  discovery, selecting UTF-8 or CP932 from its last advertised capability.
- `snapshot()`: synchronously query an owned snapshot of currently observed
  peers and their source endpoints.

A successful `send_message` means the local UDP socket accepted the datagram;
it is not a delivery acknowledgement. Unknown endpoints are rejected. A
command queue at capacity returns `CommandQueueFull` instead of growing
without bound.

## Events and overload policy

The service exposes a bounded event receiver. Events include session start and
stop, protocol events with the observed source endpoint, successful local
message submission, and transport errors.

The worker never blocks indefinitely because a UI stopped consuming events.
If the event queue is full, the new event is dropped and a counter is
accumulated. When capacity becomes available, an `EventsDropped { count }`
event reports the number lost. Consumers must treat event delivery as
best-effort and use `snapshot()` to resynchronize peer state. Applications
that require lossless message history need a separately specified persistence
or acknowledgement layer; this queue is not such a mechanism.

## Shutdown

`ApplicationService::shutdown` sends a stop command, the worker attempts
BR_EXIT to the configured destinations, drops its socket, and is joined.
Dropping the service performs the same best-effort stop-and-join path.
The socket read timeout bounds command/stop responsiveness while the worker
is waiting for a datagram. A worker panic is reported as
`WorkerPanicked`.

## Trust and verification boundaries

The worker, channels, application facade, and UDP adapter are not Verus
verified. The protocol-core proof boundary is unchanged. Source addresses and
peer capability advertisements are unauthenticated observations. No delivery,
authentication, anti-replay, or whole-application verification guarantee is
implied.
