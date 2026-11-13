/// <reference path="../widjet.d.ts" />

// Two independent WidgetCanvas layer surfaces:
//   * a large transparent label centered on the screen,
//   * a small dark-blue widget near the top edge with a moving white triangle.

// ---------------------------------------------------------------------------
// 1. Large centered label
// ---------------------------------------------------------------------------

const labelCanvas = new WidgetCanvas();

const LABEL_WIDTH = 1000;
const LABEL_HEIGHT = 280;

labelCanvas.width = LABEL_WIDTH;
labelCanvas.height = LABEL_HEIGHT;

const labelCtx = /** @type {CanvasRenderingContext2D} */ (
  labelCanvas.getContext("2d")
);

function drawLabel() {
  const width = labelCanvas.width;
  const height = labelCanvas.height;

  labelCtx.clearRect(0, 0, width, height);
  labelCtx.fillStyle = "#ffffff";
  labelCtx.font = "900 240px sans-serif";
  labelCtx.textAlign = "center";
  labelCtx.textBaseline = "middle";
  labelCtx.fillText("Hello World", width / 2, height / 2);
}

// Redraw if the compositor assigns a different size.
labelCanvas.onresize = drawLabel;
drawLabel();

// ---------------------------------------------------------------------------
// 2. Small top widget: dark-blue background + moving white triangle
// ---------------------------------------------------------------------------

const shapeCanvas = new WidgetCanvas();

const BASE_WIDTH = 420;
const BASE_HEIGHT = 200;
const TRIANGLE_SIZE = 56;
const TRIANGLE_PADDING = 20;

// Anchor to the top edge so this widget sits near the top of the screen and
// never overlaps the centered label. With no left/right anchor the compositor
// centers it horizontally.
shapeCanvas.anchor = WidgetAnchor.TOP;
shapeCanvas.marginTop = 40;
shapeCanvas.width = BASE_WIDTH;
shapeCanvas.height = BASE_HEIGHT;

const shapeCtx = /** @type {CanvasRenderingContext2D} */ (
  shapeCanvas.getContext("2d")
);

let lastTimestamp = 0;
let lastResize = -Infinity;
let triangleX = TRIANGLE_PADDING;
let triangleDirection = 1;

/** @param {number} timestamp */
function applySubtleResize(timestamp) {
  // Resizing recreates the compositor buffers, so keep it infrequent.
  if (timestamp - lastResize < 500) return;
  lastResize = timestamp;

  const width = Math.round(BASE_WIDTH + Math.sin(timestamp / 900) * 8);
  const height = Math.round(BASE_HEIGHT + Math.cos(timestamp / 1100) * 6);
  if (width !== shapeCanvas.width) shapeCanvas.width = width;
  if (height !== shapeCanvas.height) shapeCanvas.height = height;
}

/** @param {number} timestamp */
function drawShape(timestamp) {
  const delta =
    lastTimestamp === 0 ? 16 : Math.min(timestamp - lastTimestamp, 50);
  lastTimestamp = timestamp;

  applySubtleResize(timestamp);

  const width = shapeCanvas.width;
  const height = shapeCanvas.height;

  // Dark-blue background.
  shapeCtx.fillStyle = "#10244a";
  shapeCtx.fillRect(0, 0, width, height);

  // White triangle moving horizontally.
  const left = TRIANGLE_PADDING;
  const right = Math.max(left, width - TRIANGLE_SIZE - TRIANGLE_PADDING);
  triangleX += triangleDirection * delta * 0.12;
  if (triangleX <= left) {
    triangleX = left;
    triangleDirection = 1;
  } else if (triangleX >= right) {
    triangleX = right;
    triangleDirection = -1;
  }

  const centerY = height / 2;
  shapeCtx.fillStyle = "#ffffff";
  shapeCtx.beginPath();
  shapeCtx.moveTo(triangleX, centerY + TRIANGLE_SIZE / 2);
  shapeCtx.lineTo(triangleX + TRIANGLE_SIZE, centerY + TRIANGLE_SIZE / 2);
  shapeCtx.lineTo(triangleX + TRIANGLE_SIZE / 2, centerY - TRIANGLE_SIZE / 2);
  shapeCtx.closePath();
  shapeCtx.fill();

  requestAnimationFrame(drawShape);
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/** @param {WidgetEvent} event */
function handleClick(event) {
  console.log(
    `onclick: x=${event.x.toFixed(1)} y=${event.y.toFixed(1)} button=${event.button}`,
  );
}

/** @param {WidgetEvent} event */
function handleContextMenu(event) {
  console.log(
    `contextmenu: x=${event.x.toFixed(1)} y=${event.y.toFixed(1)} button=${event.button}`,
  );
}

/** @param {WidgetEvent} event */
function handleClickListener(event) {
  console.log(
    `click listener: x=${event.x.toFixed(1)} y=${event.y.toFixed(1)}`,
  );
}

/** @param {WidgetEvent} event */
function handleShapeResize(event) {
  console.log(`resize: ${event.width}x${event.height}`);
}

shapeCanvas.onclick = handleClick;
shapeCanvas.addEventListener("contextmenu", handleContextMenu);
shapeCanvas.addEventListener("click", handleClickListener);
shapeCanvas.onresize = handleShapeResize;

requestAnimationFrame(drawShape);
