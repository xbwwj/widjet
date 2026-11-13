// Widjet JavaScript runtime.
//
// This file is evaluated once, after the native canvas classes have been
// installed and before the user script runs. It provides:
//
// * `console` forwarding to the host process,
// * a small DOM-like `EventTarget` implementation for `WidgetCanvas`,
// * `requestAnimationFrame`, driven by the host frame clock.
(() => {
  "use strict";

  globalThis.WidgetAnchor = Object.freeze({
    TOP: 0x0001,
    BOTTOM: 0x0002,
    LEFT: 0x0004,
    RIGHT: 0x0008,
  });

  const NativeWidgetCanvas = WidgetCanvas;

  /** @type {Map<number, WidgetCanvas>} */
  const widgets = new Map();

  class WidgetEvent {
    constructor(type, init = {}) {
      this.type = type;
      this.target = null;
      this.currentTarget = null;
      this.defaultPrevented = false;
      this._immediatePropagationStopped = false;

      this.x = init.x ?? 0;
      this.y = init.y ?? 0;
      this.clientX = init.x ?? 0;
      this.clientY = init.y ?? 0;
      this.button = init.button ?? 0;
      this.buttons = init.buttons ?? 0;
      this.width = init.width ?? 0;
      this.height = init.height ?? 0;
      this.timeStamp = init.timeStamp ?? Date.now();
    }

    preventDefault() {
      this.defaultPrevented = true;
    }

    stopPropagation() {
      // Events do not bubble in Widjet yet, so this is only a compatibility
      // no-op. `stopImmediatePropagation` also stops remaining listeners.
    }

    stopImmediatePropagation() {
      this._immediatePropagationStopped = true;
    }
  }

  globalThis.WidgetEvent = WidgetEvent;

  /** @type {WeakMap<WidgetCanvas, Map<string, Set<object>>>} */
  const listeners = new WeakMap();

  function normalizeListener(callback) {
    if (typeof callback === "function") return callback;
    if (callback && typeof callback.handleEvent === "function") {
      return (event) => callback.handleEvent(event);
    }
    return null;
  }

  function addEventListener(type, callback, options) {
    const fn = normalizeListener(callback);
    if (!fn) return;

    let map = listeners.get(this);
    if (!map) {
      map = new Map();
      listeners.set(this, map);
    }

    let set = map.get(type);
    if (!set) {
      set = new Set();
      map.set(type, set);
    }

    const once = !!(options && (options === true || options.once));
    for (const entry of set) {
      if (entry.original === callback || entry.fn === fn) return;
    }
    set.add({ fn, original: callback, once });
  }

  function removeEventListener(type, callback) {
    const map = listeners.get(this);
    if (!map) return;

    const set = map.get(type);
    if (!set) return;

    for (const entry of Array.from(set)) {
      if (entry.original === callback) set.delete(entry);
    }
    if (set.size === 0) map.delete(type);
  }

  function dispatchEvent(event) {
    if (!event || typeof event.type !== "string") {
      throw new TypeError("dispatchEvent expects an event with a string type");
    }

    event.target = this;
    event.currentTarget = this;

    const handler = this["on" + event.type];
    if (typeof handler === "function") {
      try {
        handler.call(this, event);
      } catch (error) {
        console.error("error in on" + event.type + " handler:", error);
      }
    }

    const map = listeners.get(this);
    const set = map && map.get(event.type);
    if (!set) return !event.defaultPrevented;

    for (const entry of Array.from(set)) {
      if (entry.once) {
        set.delete(entry);
        if (set.size === 0) map.delete(event.type);
      }

      try {
        entry.fn.call(this, event);
      } catch (error) {
        console.error("error in " + event.type + " listener:", error);
      }

      if (event._immediatePropagationStopped) break;
    }

    return !event.defaultPrevented;
  }

  const prototype = NativeWidgetCanvas.prototype;
  prototype.addEventListener = addEventListener;
  prototype.removeEventListener = removeEventListener;
  prototype.dispatchEvent = dispatchEvent;

  // Wrap the native constructor so every widget can be found by the host when
  // it needs to dispatch a platform event.
  globalThis.WidgetCanvas = new Proxy(NativeWidgetCanvas, {
    construct(target, args) {
      const widget = Reflect.construct(target, args);
      widgets.set(widget.widgetId, widget);
      return widget;
    },
  });

  globalThis.__widjet_dispatch = (widgetId, type, params) => {
    const [x, y, button, buttons, width, height] = params;
    const widget = widgets.get(widgetId);
    if (!widget) return false;

    // Keep `widget.width`/`widget.height` in sync with the compositor without
    // triggering another resize request.
    if (type === "resize" && typeof widget.__setSize === "function") {
      widget.__setSize(width, height);
    }

    return widget.dispatchEvent(
      new WidgetEvent(type, {
        x,
        y,
        button,
        buttons,
        width,
        height,
        timeStamp: Date.now(),
      }),
    );
  };

  // -------------------------------------------------------------------------
  // Timers
  // -------------------------------------------------------------------------

  function normalizeTimerCallback(callback) {
    if (typeof callback === "function") return callback;
    if (typeof callback === "string") {
      return () => (0, eval)(callback);
    }
    throw new TypeError("timer callback must be a function or a string");
  }

  globalThis.setTimeout = (callback, delay = 0, ...args) => {
    const fn = normalizeTimerCallback(callback);
    return __widjet_set_timeout(
      () => fn(...args),
      Number(delay),
      false,
    );
  };

  globalThis.setInterval = (callback, delay = 0, ...args) => {
    const fn = normalizeTimerCallback(callback);
    return __widjet_set_timeout(
      () => fn(...args),
      Number(delay),
      true,
    );
  };

  function clearTimer(id) {
    if (id === undefined || id === null) return;
    __widjet_clear_timeout(Number(id));
  }

  globalThis.clearTimeout = clearTimer;
  globalThis.clearInterval = clearTimer;

  globalThis.queueMicrotask = (callback) => {
    if (typeof callback !== "function") {
      throw new TypeError("queueMicrotask expects a function");
    }
    Promise.resolve().then(callback);
  };

  globalThis.performance = {
    now: () => __widjet_now(),
  };

  // requestAnimationFrame is driven by the host. The host only blocks on a
  // timer while at least one callback is pending.
  let nextFrameId = 1;
  const frames = new Map();

  globalThis.requestAnimationFrame = (callback) => {
    if (typeof callback !== "function") {
      throw new TypeError("requestAnimationFrame expects a function");
    }
    const id = nextFrameId++;
    frames.set(id, callback);
    return id;
  };

  globalThis.cancelAnimationFrame = (id) => {
    frames.delete(id);
  };

  globalThis.__widjet_has_frames = () => frames.size > 0;

  globalThis.__widjet_frame = (timestamp) => {
    const pending = Array.from(frames.keys());
    for (const id of pending) {
      const callback = frames.get(id);
      frames.delete(id);
      if (!callback) continue;

      try {
        callback(timestamp);
      } catch (error) {
        console.error("error in requestAnimationFrame callback:", error);
      }
    }
    return frames.size;
  };

  function formatValue(value) {
    if (typeof value === "string") return value;
    if (value instanceof Error) return value.stack || String(value);
    if (typeof value === "object" && value !== null) {
      try {
        return JSON.stringify(value);
      } catch {
        return String(value);
      }
    }
    return String(value);
  }

  const log = (...args) => __widjet_log(args.map(formatValue).join(" "));
  globalThis.console = {
    log,
    info: log,
    debug: log,
    warn: log,
    error: log,
  };
})();
