# ADR 0010: Tauri desktop shell and tray-resident lifecycle

- Status: Accepted
- Date: 2026-11-08

## Context

IPMsgSec needs a desktop frontend while keeping the shared application worker as the sole owner of UDP transport and protocol session state. The desktop application should behave like a conventional IP Messenger client: it opens a main window on startup, remains resident in the Windows notification area when the user closes the window, and exits only through an explicit quit action. The first implementation target is Windows. The architecture should avoid unnecessary Windows-only logic in the shared application layer so other desktop platforms can be considered later.

## Decision

1. Use Tauri 2 with a React and TypeScript frontend.
2. Prioritize Windows for initial implementation and acceptance testing.
3. Show the main window on startup.
4. Intercept the main window's close request and hide the window instead of exiting the process. Hiding the window must not stop the application session.
5. Provide a system-tray icon with an explicit Open IPMsgSec action and a separate Quit action. Opening restores, unminimizes, and focuses the existing main window.
6. Quit is an application-level shutdown action. Once the application worker is integrated, quit must request graceful session shutdown (BR_EXIT, socket release, worker join) before exiting the Tauri process.
7. Enforce single-instance behavior. A second launch should focus the existing main window rather than create another process/session.
8. Keep Tauri commands and event forwarding in a thin desktop adapter. The frontend must not access UDP sockets, protocol-core state, or ApplicationService internals.
9. The existing Verus proof boundary remains unchanged. Tauri, desktop glue, frontend, and OS integration are unverified.
10. Keep desktop-specific dependencies outside the root Rust workspace for now; add dedicated desktop build checks rather than making the existing protocol-core CI depend on platform GUI libraries.

## Initial delivery boundary

The first desktop-shell increment establishes startup, tray presence, window hide/restore, single-instance behavior, and explicit process exit. It does not yet start ApplicationService or implement peer/chat UI. Those are subsequent increments, so the tray shell does not claim active network messaging until the worker integration is implemented.

## Acceptance criteria

- Windows startup displays the main window.
- A tray icon and menu are created.
- Closing the main window hides it without exiting the process.
- The tray Open action restores and focuses the existing window.
- The tray Quit action exits the process.
- A second launch focuses the existing window.
- The frontend contains no direct transport or protocol-core access.
- Existing Rust tests, Clippy, formatting, and Verus checks remain unchanged.
- The desktop shell is labelled unverified; no whole-application proof is implied.

## Consequences

- The process lifetime is independent of main-window visibility.
- Future session integration must be owned by Tauri managed state and stopped only by the explicit application shutdown path, not by window close.
- Windows is the initial validation platform. Linux/macOS tray behavior and packaging remain follow-up compatibility work.
