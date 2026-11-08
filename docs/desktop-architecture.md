# Desktop application architecture

## Target and user-visible lifecycle

The first desktop target is Windows. Startup shows the main window. Closing that window hides it and leaves the process resident in the system tray. Selecting Open IPMsgSec from the tray restores, unminimizes, and focuses the existing window. Selecting Quit exits the application. A second launch must activate the existing instance rather than creating another process/session.

Process start -> create tray icon/menu -> show main window.

Main window close -> prevent close -> hide window. The process and network session remain alive.

Tray Open -> show, unminimize, and focus the existing window.

Tray Quit -> graceful application shutdown -> process exit.

Window visibility and application/session lifetime are distinct concepts. Closing the window is not a session shutdown operation.

## Component boundaries

React / TypeScript frontend
    |
    | Tauri Commands and Events
    v
Tauri desktop adapter (unverified)
    - commands.rs
    - tray.rs
    - state.rs
    |
    v
ipmsgsec-app::ApplicationService / ApplicationHandle
    |
    +--> ipmsgsec-transport-udp (unverified)
    |
    +--> ipmsgsec-protocol-core (Verus proof boundary)

The first tray-shell increment does not yet instantiate ApplicationService. The following integration increment will store the service/handle in Tauri managed state, forward SessionEvent values to the frontend, and expose narrow request/response commands. The frontend must never own a socket or mutate protocol state.

## Planned frontend layout

apps/desktop/
  src-tauri/
    src/
      main.rs
      lib.rs
      tray.rs
      state.rs
      commands.rs
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

## Shutdown requirements

When ApplicationService is integrated, only the explicit Quit path and process-level shutdown path may stop the session. The sequence is:

1. Stop accepting new frontend operations.
2. Request worker shutdown.
3. Attempt BR_EXIT to configured destinations.
4. Release the UDP socket and join the worker.
5. Exit the Tauri process.

A window close request must not run this sequence.

## Verification boundary

Tauri, frontend code, tray integration, OS APIs, and the desktop adapter are unverified. Existing Verus proofs continue to apply only to the existing protocol-core proof targets. Frontend tests and desktop build checks are testing evidence, not formal proof.
