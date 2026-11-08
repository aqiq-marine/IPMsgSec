# CLI Discovery

## Scope and trust boundary

`ipmsgsec-cli` is an unverified application/OS boundary. It uses
`ipmsgsec-transport-udp` for socket operations and delegates packet parsing,
classification, and peer-state transitions to `ipmsgsec-protocol-core`.
The CLI itself, packet-number generation, clock use, OS routing, and network
delivery are not covered by Verus proofs.

The CLI now supports IPv4 peer discovery and a narrow plain-text messaging
subset. It does not implement authentication, encryption, persistence, peer
expiration, delivery acknowledgements, retries, or file attachments.

## Explicit network configuration

Both the bind address and one or more announcement destinations are required.
No interface, subnet, or broadcast address is guessed by the program.

- `--bind` selects the local IPv4 address and UDP port.
- Repeat `--announce-to` for each unicast or broadcast destination.
- The socket's IPv4 broadcast option is enabled. The operating system still
  determines routing and the egress interface; bind to a specific local
  interface address when that distinction matters.
- Use port 2425 when communicating with conventional IP Messenger clients.
  Nonstandard ports are useful for isolated same-host tests.

The CLI currently accepts non-empty ASCII usernames and hostnames only. This
is a deliberately narrow interoperability subset: ASCII is represented
identically by common legacy code pages and UTF-8. Non-ASCII identity encoding
is unresolved and must not be silently assumed to be UTF-8 or CP932.

## Run

Build:

```sh
cargo run -p ipmsgsec-cli -- --bind 0.0.0.0:2425 \
  --announce-to 255.255.255.255:2425 \
  --username alice --hostname workstation
```

The default run lasts 30 seconds. On normal timeout completion, the CLI sends
`IPMSG_BR_EXIT` to the same configured destinations. Ctrl+C terminates the
process immediately; a BR_EXIT is not guaranteed in that case.

Example with two local instances (use two terminals):

Terminal A:
```sh
cargo run -p ipmsgsec-cli -- --bind 127.0.0.1:24251 \
  --announce-to 127.0.0.1:24252 --username alice --hostname local-a
```

Terminal B:
```sh
cargo run -p ipmsgsec-cli -- --bind 127.0.0.1:24252 \
  --announce-to 127.0.0.1:24251 --username bob --hostname local-b
```

For three instances, give each instance a distinct local UDP port and list
the other two endpoints as repeated `--announce-to` arguments. Each instance
sends BR_ENTRY at startup, answers BR_ENTRY directly with ANSENTRY, updates
its bounded peer table, and sends BR_EXIT on normal timeout completion.

The automated `crates/cli/tests/loopback_discovery.rs` integration tests spawn
real CLI subprocesses on loopback. One test exercises two instances, direct
ANSENTRY replies, and normal-shutdown BR_EXIT processing; another starts three
instances and checks that each observes the other two. These tests exercise
the local host's UDP behavior, not subnet broadcast or external-client
interoperability.

## Text messaging

To send a message to an explicit IPv4 UDP endpoint, add both `--send-to`
and `--message` (repeat `--send-to` to send the same message to several
destinations):

```sh
cargo run -p ipmsgsec-cli -- --bind 0.0.0.0:2425 \
  --announce-to 255.255.255.255:2425 \
  --send-to 192.0.2.20:2425 --message "Hello from IPMsgSec" \
  --username alice --hostname workstation
```

The CLI sends one unflagged `IPMSG_SENDMSG` packet to each explicit target
when it starts, then continues listening for the configured duration.
The receiver displays the sender identity, source address, and byte-escaped
message body. It does not automatically trust a message sender as a peer,
and it does not send `IPMSG_RECVMSG` acknowledgements.

Outbound bodies are restricted to non-empty ASCII, at most 4096 bytes.
Printable ASCII, TAB, and LF are accepted; NUL and other control bytes are
rejected. The receiver also rejects empty bodies, bodies over 4096 bytes,
NUL-separated extension payloads, and SENDMSG packets with option flags.
This avoids guessing between CP932 and UTF-8 because ASCII has the same
representation in both. Non-ASCII messages, UTF-8 capability
advertisement/negotiation, CP932 conversion, acknowledgements, retries, and
attachment extensions remain future work. Incoming bytes are not decoded as
UTF-8: they are escaped for terminal-safe display, so non-ASCII text may not
be human-readable yet.

The message target is an explicit socket address, not a peer-table selection.
The current peer table stores identities but not authenticated network
endpoints; source IP addresses are unauthenticated UDP metadata.

## Packet handling

- Startup sends `1:<packet-number>:<username>:<hostname>:1:` (BR_ENTRY).
- A received BR_ENTRY is classified by protocol-core, applied through
  `apply_discovery_event`, and answered with ANSENTRY to the exact source
  address paired with that datagram.
- A received ANSENTRY adds the peer without replying.
- A received BR_EXIT removes the exact byte-identity match.
- Malformed, oversized, unsupported, and non-discovery packets are ignored
  with a concise diagnostic; packet bodies are not logged.
- The packet number seed comes from the system clock and increments with
  checked arithmetic. It is best-effort, not a uniqueness or anti-replay
  guarantee.

The peer table is an observed cache. UDP loss, reordering, spoofed source
addresses, and stale peers are not solved by this CLI. There is no periodic
re-announcement or expiry policy in this milestone.

## Interoperability target and test record

The first external target is the official Windows IP Messenger client,
version 5.9.1, using UDP port 2425 and the BR_ENTRY / ANSENTRY / BR_EXIT
discovery subset. The protocol reference is the upstream draft 14 specification.

The repository CI can test compilation and loopback transport behavior, but
cannot establish interoperability with that external Windows application.
Before making an interoperability claim, run and record this manual matrix on
a shared IPv4 LAN:

1. Start the Windows 5.9.1 client, then start this CLI; confirm the CLI sees
   the Windows peer via ANSENTRY.
2. Start this CLI, then start the Windows client; confirm Windows lists the
   CLI identity after BR_ENTRY.
3. Stop the CLI by normal timeout; confirm Windows removes it after BR_EXIT.
4. Repeat with Windows firewall enabled and document the required inbound UDP
   rule if any.
5. Capture UDP packets and compare command numbers, packet numbers, identity
   bytes, source/destination addresses, and ports with the expected wire form.

No external-client run or packet capture is included in this change. Until the
matrix is completed, compatibility with Windows IP Messenger 5.9.1 is a
selected test target, not a tested result.
