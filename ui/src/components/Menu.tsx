// The menu bar's dropdowns and the right-click menu.

import { useEffect, useRef, useState, type RefObject } from "react";

export interface MenuItem {
  label: string;
  action: () => unknown;
  shortcut?: string;
  /** Shown with a check mark when true; omitted for plain actions. */
  checked?: boolean;
  disabled?: boolean;
}

export type MenuEntry = MenuItem | "separator";

/** Closes on a click outside `element` or on Escape. */
function useDismiss(element: RefObject<HTMLElement | null>, onClose: () => void, active = true) {
  useEffect(() => {
    if (!active) return;
    const pointer = (event: PointerEvent) => {
      if (!element.current?.contains(event.target as Node)) onClose();
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("pointerdown", pointer, true);
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("pointerdown", pointer, true);
      window.removeEventListener("keydown", key, true);
    };
  }, [element, onClose, active]);
}

function Items({ entries, onDone }: { entries: MenuEntry[]; onDone: () => void }) {
  return (
    <>
      {entries.map((entry, index) =>
        entry === "separator" ? (
          <div key={index} className="menu-separator" />
        ) : (
          <button
            key={index}
            className="menu-item"
            disabled={entry.disabled}
            onClick={() => {
              onDone();
              entry.action();
            }}
          >
            <span className="menu-check">{entry.checked ? "✓" : ""}</span>
            <span className="menu-label">{entry.label}</span>
            <span className="menu-shortcut">{entry.shortcut ?? ""}</span>
          </button>
        ),
      )}
    </>
  );
}

export function MenuBar({ menus }: { menus: { title: string; entries: MenuEntry[] }[] }) {
  const [openIndex, setOpenIndex] = useState<number | null>(null);
  const bar = useRef<HTMLDivElement>(null);
  useDismiss(bar, () => setOpenIndex(null), openIndex !== null);
  return (
    <div className="menubar" ref={bar}>
      {menus.map((menu, index) => (
        <div key={menu.title} className="menu">
          <button
            className={`menu-title${openIndex === index ? " open" : ""}`}
            onClick={() => setOpenIndex(openIndex === index ? null : index)}
            // Moving across the bar while a menu is open switches menus.
            onPointerEnter={() => openIndex !== null && setOpenIndex(index)}
          >
            {menu.title}
          </button>
          {openIndex === index && (
            <div className="menu-popup">
              <Items entries={menu.entries} onDone={() => setOpenIndex(null)} />
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuEntry[];
  onClose: () => void;
}) {
  const popup = useRef<HTMLDivElement>(null);
  useDismiss(popup, onClose);
  return (
    <div className="menu-popup context" ref={popup} style={{ left: x, top: y }}>
      <Items entries={items} onDone={onClose} />
    </div>
  );
}
