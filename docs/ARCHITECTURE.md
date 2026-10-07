# Architecture

## Boundary

`osd-core` is platform-independent. X11, Unix sockets, CLI parsing, TOML, and filesystem concerns stay outside it.

## Data flow

client CLI/API -> Unix socket -> protocol -> server -> MessageBuffer -> ScrollEngine -> Scene -> Renderer -> X11/Xft

## Queue policy

The queue is bounded and uses FIFO/drop-oldest semantics.

## Rendering

The renderer consumes a platform-neutral `Scene`. The initial X11 implementation uses an override-redirect Xlib window and Xft UTF-8 text rendering.

## Future extension points

- `Renderer` can gain a Wayland implementation.
- transport can gain TCP without changing the core.
- richer messages can be introduced only when required.
- plugins/EDSL/scripting are deliberately postponed.
