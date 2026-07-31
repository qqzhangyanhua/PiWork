export const DEFAULT_INSPECTOR_PERCENT = 42;
export const INSPECTOR_WIDTH_STORAGE_KEY = "piwork.inspectorWidthPercent";

export const clampInspectorPercent = (value: number) =>
  Math.min(60, Math.max(32, value));

export const readInspectorPercent = () => {
  if (typeof localStorage === "undefined") return DEFAULT_INSPECTOR_PERCENT;
  const persisted = Number(localStorage.getItem(INSPECTOR_WIDTH_STORAGE_KEY));
  return Number.isFinite(persisted) && persisted > 0
    ? clampInspectorPercent(persisted)
    : DEFAULT_INSPECTOR_PERCENT;
};

export const persistInspectorPercent = (value: number) => {
  const constrained = clampInspectorPercent(value);
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(INSPECTOR_WIDTH_STORAGE_KEY, String(constrained));
  }
  return constrained;
};
