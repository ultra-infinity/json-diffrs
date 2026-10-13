// A dropdown with a search box: every option at first, then only those whose
// label or id contains every word typed (case-insensitive). Enter picks the
// first match.

import { useEffect, useMemo, useRef, useState } from "react";

export interface Option {
  id: string;
  label: string;
}

export function matches(option: Option, query: string) {
  const haystack = `${option.label} ${option.id}`.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((term) => haystack.includes(term));
}

/** Matches with the query as their whole id first, then those starting with
 *  it, so "openai" beats "azure.openai"; otherwise the original order. */
export function rank(options: Option[], query: string) {
  const typed = query.trim().toLowerCase();
  if (!typed) return options;
  const score = (option: Option) => {
    const id = option.id.toLowerCase();
    if (id === typed) return 0;
    if (id.startsWith(typed) || option.label.toLowerCase().startsWith(typed)) return 1;
    return 2;
  };
  // Array.prototype.sort is stable, so ties keep their order.
  return [...options].sort((a, b) => score(a) - score(b));
}

export function Combobox({
  options,
  value,
  placeholder,
  searchPlaceholder,
  disabled,
  onPick,
  footer,
}: {
  options: Option[];
  value: string;
  placeholder: string;
  searchPlaceholder: string;
  disabled?: boolean;
  onPick: (id: string) => void;
  /** Extra entry under the list, e.g. "Other…"; receives the typed query. */
  footer?: { label: string; onPick: (query: string) => void };
}) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const root = useRef<HTMLDivElement>(null);
  const shown = useMemo(() => rank(options.filter((option) => matches(option, query)), query), [options, query]);
  const selected = options.find((option) => option.id === value);

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", close, true);
    return () => window.removeEventListener("pointerdown", close, true);
  }, [open]);

  const pick = (id: string) => {
    onPick(id);
    setQuery("");
    setOpen(false);
  };

  return (
    <div className="combobox" ref={root}>
      <button
        className="combobox-button"
        disabled={disabled}
        onClick={() => setOpen(!open)}
        title={selected?.id}
      >
        <span className={selected ? "" : "weak"}>{selected?.label ?? placeholder}</span>
        <span className="codicon codicon-chevron-down" />
      </button>
      {open && (
        <div className="combobox-popup">
          <input
            autoFocus
            placeholder={searchPlaceholder}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && shown[0]) pick(shown[0].id);
              if (event.key === "Escape") setOpen(false);
            }}
          />
          <div className="combobox-list">
            {shown.length === 0 && <div className="weak padded">No match</div>}
            {shown.map((option) => (
              <button
                key={option.id}
                className={`combobox-option${option.id === value ? " selected" : ""}`}
                title={option.id}
                onClick={() => pick(option.id)}
              >
                {option.label}
                {option.label !== option.id && <span className="weak"> ({option.id})</span>}
              </button>
            ))}
          </div>
          {footer && (
            <button
              className="combobox-option footer"
              onClick={() => {
                footer.onPick(query.trim());
                setQuery("");
                setOpen(false);
              }}
            >
              {footer.label}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
