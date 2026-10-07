# OSD

A long-lived Rust X11 OSD server/client designed as an `osd_cat`-like scrolling overlay.

## Current architecture

- `osd-core`: platform-independent messages, bounded FIFO/drop-oldest queue, scrolling model, layout scene, renderer trait.
- `osd-protocol`: small line-oriented UTF-8 protocol for the initial Unix socket transport.
- `osd-client`: reusable Rust client API plus `osd-client` CLI.
- `osd-x11`: reusable X11/XShape/Xinerama renderer and native X11 integration.
- `osd-server`: configuration, Unix socket server, runtime command handling, and OSD lifecycle.

## Configuration precedence

Built-in defaults -> TOML file -> CLI overrides.

## Initial behavior

Messages form one continuous list. New messages are appended; when capacity is reached, the oldest message is discarded. Default scrolling direction is bottom-to-top.

## Dependencies

The X11 backend owns the native X11 dependencies. `osd-server` does not require an X11 `build.rs`.

## Important implementation status

This is the first vertical slice. The core/protocol/client boundaries are intentionally stable, while monitor discovery, true transparent ARGB rendering, richer typography, and a more sophisticated event loop remain backend work for the next increment.

The development environment used to generate this source did not have Cargo installed, so this workspace has **not been locally compiled here**. Run `cargo check --workspace` on the target Guix/Linux machine before treating it as build-verified.

## Runtime commands

The client can send messages or runtime commands. `PAUSE` freezes animation but continues accepting messages; `RESUME` continues from the paused position. Queue capacity changes take effect immediately and shrinking the queue drops the oldest messages first.

Examples:

```sh
osd-client "Hello OSD"
osd-client cmd direction top-to-bottom
osd-client cmd speed 100
osd-client cmd color green
osd-client cmd clear
osd-client cmd pause
osd-client cmd resume
osd-client cmd queue-capacity 50
```

Runtime-mutable settings are direction, speed, foreground color, queue capacity, and animation state (pause/resume). Startup-only settings include socket path, X11 display/monitor selection, position, and font.
