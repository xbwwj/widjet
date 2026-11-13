# Widjet

A lightweight widget runtime for Linux. Widgets are described by a JavaScript
script and rendered with the `CanvasRenderingContext2D` API on Wayland layer
surfaces.

## Run

```sh
cargo run -- user.js
```

The script path is a required positional argument:

```sh
target/debug/widjet path/to/widget.js
```

### Bullet-hell demo

`user2.js` is a small portrait Touhou-style game: a red enemy fires red circular
bullets, a white player triangle follows the pointer, collisions end the run,
and the bottom strip shows the score.

```sh
cargo run --release -- user2.js
```

Use `--release` for animation-heavy scripts; the CPU rasterizer is much slower
in a debug build.

### Widget example

`user.js` is a runnable example with two independent `WidgetCanvas` instances:
a large transparent label centered on the screen, and a small dark-blue widget
near the top edge with a moving white triangle. Click and context-menu events
are logged to the terminal.

Each `new WidgetCanvas()` creates its own Wayland layer surface and 2D canvas.
The script decides how many widgets exist.

## Scripting

```js
// One widget: a transparent, screen-centered label.
const label = new WidgetCanvas();
label.width = 1000;
label.height = 280;
const labelCtx = label.getContext("2d");
labelCtx.font = "900 120px sans-serif";
labelCtx.textAlign = "center";
labelCtx.textBaseline = "middle";
labelCtx.fillText("Hello, World!", label.width / 2, label.height / 2);

// Another widget: top-centered with a 40 px gap. Anchors and margins are
// forwarded to wlr-layer-shell.
const shape = new WidgetCanvas();
shape.anchor = WidgetAnchor.TOP;
shape.marginTop = 40;
shape.width = 420;
shape.height = 200;

const ctx = shape.getContext("2d");
function frame(timestamp) {
  ctx.fillStyle = "#10244a";
  ctx.fillRect(0, 0, shape.width, shape.height);
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

shape.onclick = (event) => console.log("click", event.x, event.y);
shape.addEventListener("contextmenu", (event) => console.log("contextmenu"));
```

### Timers and animation

The runtime provides browser-like scheduling:

```js
setTimeout(() => console.log("later"), 100);
const id = setInterval(() => console.log("tick"), 250);
clearInterval(id);

queueMicrotask(() => console.log("microtask"));
requestAnimationFrame((timestamp) => console.log(timestamp));
```

`setTimeout` / `setInterval` are host-side timers and enter the event loop as
`TaskSource::Timer` tasks. Promise reactions and `queueMicrotask` callbacks are
drained by a microtask checkpoint after every task. `requestAnimationFrame` is
driven by real `wl_surface.frame` callbacks when a surface is configured and has
a buffer; otherwise it falls back to a 60 Hz clock.

Type declarations are available in [`widjet.d.ts`](./widjet.d.ts).
