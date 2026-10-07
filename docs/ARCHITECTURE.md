# Architecture

## Boundary

`osd-core` is platform-independent. X11, Unix sockets, CLI parsing, TOML, and filesystem concerns stay outside it.

## Data flow

client CLI/API -> Unix socket -> protocol -> server -> MessageBuffer/ScrollEngine -> Scene -> osd-x11 Renderer -> X11

## Queue policy

The queue is bounded and uses FIFO/drop-oldest semantics.

## Rendering

The renderer consumes a platform-neutral `Scene`. The initial X11 implementation uses an override-redirect Xlib window and Xft UTF-8 text rendering.

## Future extension points

- `Renderer` can gain a Wayland implementation.
- transport can gain TCP without changing the core.
- richer messages can be introduced only when required.
- plugins/EDSL/scripting are deliberately postponed.


## Runtime commands

`osd-protocol` distinguishes plain `Message` requests from tagged `Command` requests. The initial command set is:

- `SET_DIRECTION bottom-to-top|top-to-bottom`
- `SET_SPEED <pixels-per-second>`
- `SET_COLOR <color>`
- `CLEAR`
- `PAUSE`
- `RESUME`
- `SET_QUEUE_CAPACITY <size>`

`PAUSE` affects only animation advancement. Client messages continue to be accepted and queued while paused. `RESUME` preserves the current scroll position.

Startup-only configuration is intentionally separate from runtime state: socket path, X11 display/monitor selection, window position, and font are not protocol commands.
