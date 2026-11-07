declare class WidgetCanvas {
  getContext(contextType: "2d"): CanvasRenderingContext2D
  width: number;
  height: number;
  anchor: WidgetAnchorFlags;
}

type WidgetAnchorFlags = number;

declare var WidgetAnchor: {
  readonly TOP: 0x0001;
  readonly BOTTOM: 0x0002
  readonly LEFT: 0x0004;
  readonly RIGHT: 0x0008;
}
