# ADR 0011: Integrate the desktop client through the shared application worker

- Status: Accepted
- Date: 2026-11-08

## Context

The Tauri tray shell establishes process and window lifecycle, while the existing ipmsgsec-app crate already provides a bounded single-owner worker for UDP transport and protocol session state. The desktop frontend must not duplicate transport logic or own sockets. The application must remain active when the main window is hidden, and explicit process shutdown must release the worker cleanly.

The project also requires explicit IPv4 discovery destinations. The desktop client must not silently select a broadcast destination or interface before those protocol/network policy decisions are made.

## Decision

1. Instantiate the existing ApplicationService from the Tauri desktop adapter; do not reimplement UDP or protocol orchestration in Tauri.
2. Store the service and a cloneable ApplicationHandle in managed desktop state.
3. Keep the worker as the sole owner of UDP transport, protocol session state, packet-number allocation, and observed peer endpoints.
4. Forward worker SessionEvent values to the frontend through a dedicated adapter thread and the Tauri event channel. Convert socket addresses to strings and expose only serializable, owned DTOs.
5. Expose narrow commands for session startup, discovery, peer snapshots, message submission, and session stop.
6. Require the user to enter an explicit IPv4 bind address, one or more explicit IPv4 discovery targets, username, and hostname. Do not silently choose a broadcast destination. The initial target field is empty.
7. Start the network worker only after the user submits valid configuration. The process may be running in the tray without an active network session.
8. Closing the main window only hides it. The explicit Stop session command shuts down the worker but keeps the desktop process alive. Tray Quit and process Exit shut down the worker before exiting.
9. Preserve bounded command/event queues. If events are dropped, the frontend must resynchronize from a worker snapshot rather than assuming the event stream is complete.
10. Treat peer identity and source endpoints as unauthenticated observations. UDP submission success is not delivery confirmation.
11. Keep the Verus proof boundary unchanged: protocol-core only. The application worker and desktop integration remain unverified.

## Consequences

- CLI and GUI share one application implementation.
- A hidden window does not stop peer discovery or incoming message processing once a session is active.
- The initial desktop client requires explicit network configuration and does not persist it.
- The frontend receives DTOs and events only; it cannot access sockets or protocol state.
- Windows compile CI does not replace manual multi-host interoperability and tray lifecycle tests.
