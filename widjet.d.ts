declare class WidgetCanvas {
  constructor();

  readonly widgetId: number;
  width: number;
  height: number;

  anchor: WidgetAnchorFlags;
  exclusiveZone: number;
  marginTop: number;
  marginRight: number;
  marginBottom: number;
  marginLeft: number;

  onclick: ((event: WidgetEvent) => void) | null;
  oncontextmenu: ((event: WidgetEvent) => void) | null;
  onresize: ((event: WidgetEvent) => void) | null;
  onpointerenter: ((event: WidgetEvent) => void) | null;
  onpointerleave: ((event: WidgetEvent) => void) | null;
  onpointermove: ((event: WidgetEvent) => void) | null;
  onpointerdown: ((event: WidgetEvent) => void) | null;
  onpointerup: ((event: WidgetEvent) => void) | null;

  /** Schedule a callback on this surface's compositor frame clock. */
  requestAnimationFrame(callback: (timestamp: number) => void): number;
  cancelAnimationFrame(handle: number): void;

  addEventListener(
    type: string,
    listener: (event: WidgetEvent) => void,
    options?: boolean | { once?: boolean },
  ): void;
  removeEventListener(
    type: string,
    listener: (event: WidgetEvent) => void,
  ): void;
  dispatchEvent(event: WidgetEvent): boolean;

  getContext(contextType: "2d"): CanvasRenderingContext2D | null;
  transferControlToOffscreen(): OffscreenCanvas;
  destroy(): void;
}

declare class WidgetEvent {
  constructor(type: string, init?: Partial<WidgetEventInit>);

  readonly type: string;
  readonly target: WidgetCanvas | null;
  readonly currentTarget: WidgetCanvas | null;
  readonly defaultPrevented: boolean;
  readonly timeStamp: number;

  /** Surface-local pointer coordinates. */
  readonly x: number;
  readonly y: number;
  readonly clientX: number;
  readonly clientY: number;

  /** DOM-style button index: 0 = left, 1 = middle, 2 = right. */
  readonly button: number;
  /** DOM-style pressed-button bit mask. */
  readonly buttons: number;

  /** Populated for resize events. */
  readonly width: number;
  readonly height: number;

  preventDefault(): void;
  stopPropagation(): void;
  stopImmediatePropagation(): void;
}

interface WidgetEventInit {
  x: number;
  y: number;
  button: number;
  buttons: number;
  width: number;
  height: number;
  timeStamp: number;
}

/** Timer-driven compatibility timeline; prefer canvas.requestAnimationFrame. */
declare function requestAnimationFrame(
  callback: (timestamp: number) => void,
): number;
declare function cancelAnimationFrame(handle: number): void;

declare function setTimeout(
  handler: (...args: unknown[]) => void,
  timeout?: number,
  ...args: unknown[]
): number;
declare function setInterval(
  handler: (...args: unknown[]) => void,
  timeout?: number,
  ...args: unknown[]
): number;
declare function clearTimeout(handle?: number): void;
declare function clearInterval(handle?: number): void;
declare function queueMicrotask(callback: () => void): void;

declare var performance: {
  now(): number;
};

declare class OffscreenCanvas {
  constructor(width: number, height: number);

  width: number;
  height: number;

  getContext(contextType: "2d"): CanvasRenderingContext2D | null;
  transferToImageBitmap(): ImageBitmap;
}

declare class CanvasRenderingContext2D {
  fillStyle: string;
  strokeStyle: string;
  globalAlpha: number;
  lineWidth: number;
  lineCap: "butt" | "round" | "square";
  lineJoin: "round" | "bevel" | "miter";
  miterLimit: number;
  lineDashOffset: number;
  font: string;
  textAlign: "start" | "end" | "left" | "right" | "center";
  textBaseline:
    | "top"
    | "hanging"
    | "middle"
    | "alphabetic"
    | "ideographic"
    | "bottom";

  setLineDash(segments: number[]): void;
  getLineDash(): number[];

  save(): void;
  restore(): void;
  reset(): void;

  beginPath(): void;
  closePath(): void;
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  quadraticCurveTo(cpx: number, cpy: number, x: number, y: number): void;
  bezierCurveTo(
    cp1x: number,
    cp1y: number,
    cp2x: number,
    cp2y: number,
    x: number,
    y: number,
  ): void;
  arcTo(x1: number, y1: number, x2: number, y2: number, radius: number): void;
  rect(x: number, y: number, width: number, height: number): void;
  roundRect(
    x: number,
    y: number,
    width: number,
    height: number,
    radii?: number,
  ): void;
  arc(
    x: number,
    y: number,
    radius: number,
    startAngle: number,
    endAngle: number,
    counterclockwise?: boolean,
  ): void;
  ellipse(
    x: number,
    y: number,
    radiusX: number,
    radiusY: number,
    rotation: number,
    startAngle: number,
    endAngle: number,
    counterclockwise?: boolean,
  ): void;
  fill(fillRule?: "nonzero" | "evenodd"): void;
  stroke(): void;
  clip(fillRule?: "nonzero" | "evenodd"): void;

  fillRect(x: number, y: number, width: number, height: number): void;
  clearRect(x: number, y: number, width: number, height: number): void;
  strokeRect(x: number, y: number, width: number, height: number): void;

  translate(x: number, y: number): void;
  scale(x: number, y: number): void;
  rotate(angle: number): void;
  setTransform(a: number, b: number, c: number, d: number, e: number, f: number): void;
  resetTransform(): void;

  drawImage(
    image: CanvasImageSource,
    dx: number,
    dy: number,
    dw?: number,
    dh?: number,
  ): void;

  fillText(text: string, x: number, y: number, maxWidth?: number): void;
  strokeText(text: string, x: number, y: number, maxWidth?: number): void;
  measureText(text: string): TextMetrics;

  getImageData(sx: number, sy: number, sw: number, sh: number): ImageData;
  putImageData(imageData: ImageData, dx: number, dy: number): void;
}

type OffscreenCanvasRenderingContext2D = CanvasRenderingContext2D;

declare class TextMetrics {
  readonly width: number;
  readonly actualBoundingBoxLeft: number;
  readonly actualBoundingBoxRight: number;
  readonly fontBoundingBoxAscent: number;
  readonly fontBoundingBoxDescent: number;
  readonly actualBoundingBoxAscent: number;
  readonly actualBoundingBoxDescent: number;
  readonly emHeightAscent: number;
  readonly emHeightDescent: number;
  readonly hangingBaseline: number;
  readonly alphabeticBaseline: number;
  readonly ideographicBaseline: number;
}

declare class ImageBitmap {
  readonly width: number;
  readonly height: number;
  close(): void;
}

declare class ImageData {
  readonly width: number;
  readonly height: number;
  readonly data: Uint8ClampedArray;
}

type CanvasImageSource = ImageBitmap | WidgetCanvas | OffscreenCanvas;

type WidgetAnchorFlags = number;

declare var WidgetAnchor: {
  readonly TOP: 0x0001;
  readonly BOTTOM: 0x0002;
  readonly LEFT: 0x0004;
  readonly RIGHT: 0x0008;
};

declare var console: {
  log(...data: unknown[]): void;
  info(...data: unknown[]): void;
  debug(...data: unknown[]): void;
  warn(...data: unknown[]): void;
  error(...data: unknown[]): void;
};

