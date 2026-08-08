/**
 * The app's right-click menu.
 *
 * A desktop app that leaves the webview's own menu in place is telling you
 * it's a web page: right-clicking anything offers Reload and Inspect Element.
 * This replaces it — one small card, opened by whichever component knows what
 * the click was on and what can be done with it.
 *
 * There's no menu bar and no global registry behind it. A component calls
 * `useContextMenu()`, renders the `menu` it hands back, and builds the item
 * list at the moment of the click, where it still has the row in scope.
 */
import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

/** Loose on purpose, so any icon from `lib/icons` drops straight in. */
type IconComponent = (props: {
  size?: number;
  style?: CSSProperties;
  "aria-hidden"?: boolean;
}) => ReactNode;

export interface MenuItem {
  label: ReactNode;
  onSelect: () => void;
  icon?: IconComponent;
  /** Stays listed and stays reachable by keyboard — just can't be chosen.
   *  Hiding it instead would make the menu's shape change per row, which is
   *  worse: you'd have to read it every time to find anything. */
  disabled?: boolean;
  /** Draw a hairline above this item, to break the list into groups. */
  divide?: boolean;
}

/** How close the card may come to the window edge before it's pushed back. */
const MARGIN = 8;

export function useContextMenu() {
  const [state, setState] = useState<{
    x: number;
    y: number;
    items: MenuItem[];
  } | null>(null);
  const opener = useRef<HTMLElement | null>(null);

  const close = useCallback(() => {
    setState(null);
    // Focus goes back where the click came from, so dismissing with Escape
    // doesn't drop you at the top of the document. Safe to do before the
    // unmount lands: focus moves to an element that isn't going anywhere.
    opener.current?.focus?.({ preventScroll: true });
    opener.current = null;
  }, []);

  const open = useCallback((e: MouseEvent, items: MenuItem[]) => {
    e.preventDefault();
    // Nested targets each own their own menu; the innermost one wins rather
    // than both firing and the outer one landing last.
    e.stopPropagation();
    if (!items.length) return;
    opener.current = e.currentTarget as HTMLElement;
    setState({ x: e.clientX, y: e.clientY, items });
  }, []);

  const menu = state ? (
    <ContextMenu x={state.x} y={state.y} items={state.items} onClose={close} />
  ) : null;

  return { open, close, menu };
}

function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  // Null until measured. The card renders at the raw click point so it can be
  // sized, and only becomes visible once it's been pulled back inside the
  // window — otherwise a menu opened near the bottom edge visibly jumps.
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const [active, setActive] = useState(() => items.findIndex((i) => !i.disabled));

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const box = el.getBoundingClientRect();
    setPos({
      x: Math.max(MARGIN, Math.min(x, window.innerWidth - box.width - MARGIN)),
      y: Math.max(MARGIN, Math.min(y, window.innerHeight - box.height - MARGIN)),
    });
    // `preventScroll`, because the card is fixed and needs no scrolling into
    // view — and because a scroll here would immediately dismiss it.
    el.focus({ preventScroll: true });
  }, [x, y]);

  useEffect(() => {
    // Capture phase, and pointerdown rather than click: a right-click's
    // pointerdown lands before its contextmenu, so opening a second menu
    // closes the first on the way in instead of leaving two on screen. It
    // also means the dismissal happens on press, like the platform's.
    const onPointerDown = (e: PointerEvent) => {
      if (ref.current?.contains(e.target as Node)) return;
      onClose();
    };
    // Scrolling or resizing under a fixed card would leave it pointing at
    // whatever has moved into its place, which is worse than losing it.
    const dismiss = () => onClose();
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("blur", dismiss);
    window.addEventListener("resize", dismiss);
    window.addEventListener("scroll", dismiss, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("blur", dismiss);
      window.removeEventListener("resize", dismiss);
      window.removeEventListener("scroll", dismiss, true);
    };
  }, [onClose]);

  const step = (dir: number) =>
    setActive((cur) => {
      let i = cur;
      for (let k = 0; k < items.length; k++) {
        i = (i + dir + items.length) % items.length;
        if (!items[i].disabled) return i;
      }
      return cur;
    });

  const choose = (item: MenuItem) => {
    if (item.disabled) return;
    onClose();
    item.onSelect();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        step(1);
        break;
      case "ArrowUp":
        e.preventDefault();
        step(-1);
        break;
      case "Home":
        e.preventDefault();
        setActive(items.findIndex((i) => !i.disabled));
        break;
      case "End":
        e.preventDefault();
        setActive(items.map((i) => !i.disabled).lastIndexOf(true));
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        if (items[active]) choose(items[active]);
        break;
      case "Escape":
      case "Tab":
        e.preventDefault();
        onClose();
        break;
    }
  };

  return createPortal(
    <div
      ref={ref}
      className="menu"
      role="menu"
      tabIndex={-1}
      data-placed={pos ? "true" : undefined}
      style={{ left: pos?.x ?? x, top: pos?.y ?? y }}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item, i) => {
        const Icon = item.icon;
        return (
          <Fragment key={i}>
            {item.divide && i > 0 && <div className="menu-sep" />}
            <button
              type="button"
              role="menuitem"
              className="menu-item"
              tabIndex={-1}
              // aria-disabled, not `disabled`: a disabled button drops out of
              // the arrow-key walk, and a menu whose items appear and vanish
              // from the keyboard order is harder to use than one where the
              // greyed-out entry simply refuses.
              aria-disabled={item.disabled || undefined}
              data-active={i === active ? "true" : undefined}
              // Move, not enter: the pointer sitting still under a menu that
              // opened beneath it shouldn't steal the highlight from the
              // keyboard.
              onMouseMove={() => !item.disabled && setActive(i)}
              onClick={() => choose(item)}
            >
              {Icon && <Icon size={14} style={{ flex: "none" }} aria-hidden />}
              <span style={{ flex: 1 }}>{item.label}</span>
            </button>
          </Fragment>
        );
      })}
    </div>,
    document.body,
  );
}

/**
 * Suppress the webview's own context menu, once, for the life of the process.
 *
 * Not everywhere: text you can select and fields you can type into keep it,
 * because the platform's copy/paste and spelling menu there is better than
 * anything worth rebuilding, and losing it would be a real regression rather
 * than a cosmetic one.
 */
export function blockNativeContextMenu(): void {
  document.addEventListener("contextmenu", (e) => {
    const el = e.target as Element | null;
    if (el?.closest?.("input, textarea, .selectable")) return;
    e.preventDefault();
  });
}
