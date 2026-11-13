# widjet — ScriptThread runtime

A lightweight Wayland widget runtime written in Rust, using QuickJS (`rquickjs`)
for the JavaScript canvas API.

## Project layout

- `src/main.rs`: selects the Wayland backend and directly runs `ScriptThread`.
- `src/script_thread.rs`: owns QuickJS `Runtime`/`Context`, timers, task queues,
  microtask checkpoints, rAF scheduling, widget reconciliation and rendering.
- `src/platform.rs`: platform-neutral `PlatformEventSource` and `PlatformHost`
  traits.
- `src/wayland.rs`: Wayland backend implementing both platform traits.
- `src/canvas/`: canvas pixel storage and the shared widget registry.
- `src/js/`: QuickJS bindings and the JS runtime library.
- `examples/`: runnable widgets.
- `widjet.d.ts`: TypeScript declarations.

There is no `app.rs`, `App` composition layer, separate JS-agent wrapper,
`JsBridge`, or platform command queue. QuickJS is owned directly by
`ScriptThread`. `main` constructs `WaylandPlatform` and passes it straight to
`ScriptThread::new(platform).run(&source)`.

## One WidgetId registry

`WidgetStore` is the single authoritative `WidgetId -> WidgetState` registry.
It contains each widget's desired `WidgetSpec`, revision, materialization state,
compositor size, applied revision, per-surface rAF timing, frame-callback
watchdog state and removal tombstone. `WidgetCanvas` JS bindings access this
same model directly; they do not keep a second copy of the widget properties.

The Wayland backend separately has `HashMap<WidgetId, WlWindow>` because it
owns the actual Wayland proxy objects, shm pools, buffers and frame callbacks.
That is a backend resource table, not a second application-level widget model.
`CanvasStore` is keyed by `CanvasId`, not `WidgetId`, and stores pixel targets.
The JS runtime has one `Map<WidgetId, WidgetCanvas>` only to resolve native
input events to JS event targets; per-surface animation queues live in a
`WeakMap` keyed by the `WidgetCanvas` object. `TaskQueue` contains no
WidgetId-keyed map or set.

## Build and run

Requirements:

- Rust stable with Rust 2024 edition support
- A Wayland session
- A compositor supporting `wlr-layer-shell`

```sh
cargo check
cargo test
cargo run --release -- examples/hello.js
cargo run --release -- examples/game.js
```

## Event-loop sequence

1. `ScriptThread` evaluates the user script and drains the initial QuickJS job
   queue.
2. It reconciles materialized `WidgetStore` entries through `PlatformHost`.
3. Each turn schedules due timers, runs at most one task, then performs a
   microtask checkpoint and another widget-state reconciliation.
4. Dirty canvas snapshots are presented through `PlatformHost`.
5. Each surface's native frame callback releases only that surface's rAF queue;
   a 250 ms watchdog handles stalled compositor callbacks.
6. The script thread polls `PlatformEventSource` without entering JS from the
   platform callback. It uses a zero timeout while tasks remain, otherwise
   waits until a native event or the next timer/frame deadline.

## Adding another platform

Implement `PlatformEventSource` to wait for and normalize native events, and
`PlatformHost` to create/update/destroy surfaces, present snapshots and manage
frame callbacks. No QuickJS type or canvas drawing implementation needs to be
platform-specific. The included backend is Wayland-only; Windows and macOS
backends are not included in this project snapshot.
