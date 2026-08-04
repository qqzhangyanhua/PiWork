import { Check, ChevronDown } from "lucide-react";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import type { ModelProvider } from "../../app/tauriClient";
import { ProviderLogo } from "./ProviderLogo";

export type BrandSelectOption = {
  value: string;
  label: string;
  provider: ModelProvider;
  badge?: string;
  description?: string;
};

export function BrandSelect({
  id,
  label,
  value,
  options,
  disabled = false,
  onChange,
}: {
  id: string;
  label: string;
  value: string;
  options: BrandSelectOption[];
  disabled?: boolean;
  onChange(value: string): void;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value));
  const [activeIndex, setActiveIndex] = useState(selectedIndex);
  const selected = options[selectedIndex];
  const listboxId = `${id}-listbox`;
  const labelId = `${id}-label`;

  useEffect(() => {
    if (!open) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [open]);

  useEffect(() => {
    if (!open) setActiveIndex(selectedIndex);
  }, [open, selectedIndex]);

  const close = () => {
    setOpen(false);
    queueMicrotask(() => triggerRef.current?.focus());
  };

  const choose = (index: number) => {
    const option = options[index];
    if (!option) return;
    onChange(option.value);
    close();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (disabled || options.length === 0) return;
    if (!open) {
      if (!["ArrowDown", "ArrowUp", "Enter", " "].includes(event.key)) return;
      event.preventDefault();
      setActiveIndex(selectedIndex);
      setOpen(true);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((current) => (current + 1) % options.length);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((current) => (current - 1 + options.length) % options.length);
      return;
    }
    if (event.key === "Home") {
      event.preventDefault();
      setActiveIndex(0);
      return;
    }
    if (event.key === "End") {
      event.preventDefault();
      setActiveIndex(options.length - 1);
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(activeIndex);
    }
  };

  return (
    <div className="brand-select" ref={rootRef}>
      <span className="brand-select__label" id={labelId}>{label}</span>
      <button
        aria-activedescendant={open ? `${id}-option-${activeIndex}` : undefined}
        aria-controls={listboxId}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-labelledby={labelId}
        className="brand-select__trigger"
        disabled={disabled}
        id={id}
        onClick={() => {
          setActiveIndex(selectedIndex);
          setOpen((current) => !current);
        }}
        onKeyDown={handleKeyDown}
        ref={triggerRef}
        role="combobox"
        type="button"
      >
        {selected ? <ProviderLogo provider={selected.provider} /> : <span className="brand-select__empty-mark" />}
        <span className="brand-select__value">{selected?.label ?? "—"}</span>
        {selected?.badge && <span className="brand-select__badge">{selected.badge}</span>}
        <ChevronDown aria-hidden="true" className="brand-select__chevron" size={16} />
      </button>
      {open && (
        <div aria-labelledby={labelId} className="brand-select__listbox" id={listboxId} role="listbox">
          {options.map((option, index) => (
            <button
              aria-selected={option.value === value}
              className="brand-select__option"
              data-active={index === activeIndex || undefined}
              id={`${id}-option-${index}`}
              key={option.value}
              onClick={() => choose(index)}
              onPointerMove={() => setActiveIndex(index)}
              role="option"
              type="button"
            >
              <ProviderLogo provider={option.provider} />
              <span className="brand-select__option-copy">
                <strong>{option.label}</strong>
                {option.description && <small>{option.description}</small>}
              </span>
              {option.badge && <span className="brand-select__badge">{option.badge}</span>}
              {option.value === value && <Check aria-hidden="true" size={15} />}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
