# Desktop application architecture

## Target and user-visible lifecycle

The first desktop target is Windows. Startup shows the main window and installs the system-tray icon. Closing the main window hides it and leaves the process resident in the tray. Selecting Open IPMsgSec restores, unminimizes, and focuses the existing window. Selecting Quit performs graceful session shutdown and exits the process. A second launch activates the existing instance.

Window visibility and application/session lifetime are distinct concepts. Closing the window is not a session shutdown operation.

## Component boundaries

React / TypeScript frontend
    |
    | Tauri Commands and Events (owned DTOs)
    v
Tauri desktop adapter (unverified)
    - commands.rs
    - tray.rs
    - state.rs
    - bounded event forwarding thread
    |
    v
ipmsgsec-app::ApplicationService / ApplicationHandle
    |
    +--> ipmsgsec-transport-udp (unverified)
    |
    +--> ipmsgsec-protocol-core (Verus proof boundary)

The Tauri adapter is the only frontend-facing boundary. The frontend never owns a UDP socket, ProtocolState, ReplyTarget, or ApplicationService internals.

## Session startup and explicit network configuration

The desktop process does not silently guess a broadcast destination or interface. Before starting a network session, the user supplies:

- An IPv4 bind socket address (the initial UI suggests 0.0.0.0:2425).
- One or more explicit IPv4 discovery targets, each including a port. Multiple targets are comma-separated.
- Local username and hostname.

The initial discovery targets field is empty by design. The UI may show examples such as 192.168.1.255:2425 or 255.255.255.255:2425, but it does not choose either automatically. The session worker sends BR_ENTRY to the configured targets at startup. The Discover command sends another BR_ENTRY.

The application does not persist these settings yet. Persistence and migration policy are a later milestone.

## Session ownership and event flow

DesktopState stores the ApplicationService and a clone of its ApplicationHandle. The service owns the existing bounded worker; the worker remains the sole owner of UDP transport, protocol session state, packet-number allocation, and observed peer endpoints.

A dedicated desktop event-forwarding thread drains SessionEvent from the service's bounded event queue and emits serializable DTOs on the Tauri session-event channel. The frontend can also request an owned peer snapshot. If the worker reports dropped events, the frontend refreshes the snapshot rather than assuming its event-derived view is complete.

The frontend uses these narrow commands:

- start_session: validate explicit IPv4 settings and start the service.
- discover: request another BR_ENTRY.
- get_snapshot: retrieve the current observed peer list.
- send_message: submit text to an observed peer endpoint.
- stop_session: gracefully stop the worker while leaving the desktop process alive.

A successful send means only that the local UDP socket accepted the datagram. It is not a remote delivery acknowledgement. Peer identities and source addresses are observations, not authenticated identities.

## Shutdown requirements

The main-window close request only hides the window. The explicit Stop session command or application Quit path performs session shutdown:

1. Stop the desktop event-forwarding thread.
2. Drop the UI-facing handle so no new commands can be submitted.
3. Request worker shutdown.
4. Attempt BR_EXIT to the configured destinations.
5. Release the UDP socket and join the worker.
6. For tray Quit/process exit, exit the Tauri process after shutdown.

The shutdown operation is idempotent. The Tauri process-level Exit hook also invokes it as a final cleanup path.

## Planned frontend layout

apps/desktop/
  src-tauri/
    src/
      main.rs
      lib.rs
      commands.rs
      state.rs
      tray.rs
    capabilities/
    Cargo.toml
    tauri.conf.json
  src/
    App.tsx
    main.tsx
    components/
    features/
    styles/
  package.json
  vite.config.ts

## Verification boundary

The protocol-core parser and state transition proof targets remain unchanged. The application worker, UDP adapter, Tauri commands, event-forwarding thread, frontend, tray integration, OS APIs, and packaging are unverified. CI compilation and tests are testing evidence, not formal proof. Windows runtime acceptance testing remains required for startup, discovery across hosts, close-to-tray behavior, tray restore, Stop session, graceful Quit, and second-instance activation.
