# OSD

A long-lived Rust X11 OSD server/client designed as an `osd_cat`-like scrolling overlay.

## Current architecture

- `osd-core`: platform-independent messages, bounded FIFO/drop-oldest queue, scrolling model, layout scene, renderer trait.
- `osd-protocol`: small line-oriented UTF-8 protocol for the initial Unix socket transport.
- `osd-client`: reusable Rust client API plus `osd-client` CLI.
- `osd-server`: configuration, Unix socket server, and initial X11/Xft renderer.

## Configuration precedence

Built-in defaults -> TOML file -> CLI overrides.

## Initial behavior

Messages form one continuous list. New messages are appended; when capacity is reached, the oldest message is discarded. Default scrolling direction is bottom-to-top.

## Dependencies

The X11 backend uses the `x11` crate with Xlib/Xft/XRandR bindings. `x11` 2.21.0 exposes Xft UTF-8 rendering and XRandR bindings. See the upstream API documentation before extending the backend.

## Important implementation status

This is the first vertical slice. The core/protocol/client boundaries are intentionally stable, while monitor discovery, true transparent ARGB rendering, richer typography, and a more sophisticated event loop remain backend work for the next increment.

The development environment used to generate this source did not have Cargo installed, so this workspace has **not been locally compiled here**. Run `cargo check --workspace` on the target Guix/Linux machine before treating it as build-verified.
