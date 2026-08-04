const UNITS: Array<[Intl.RelativeTimeFormatUnit, number]> = [
  ["day", 86_400_000],
  ["hour", 3_600_000],
  ["minute", 60_000],
];

export function formatRelativeTime(iso: string, language: string): string {
  const elapsed = new Date(iso).getTime() - Date.now();
  const [unit, duration] =
    UNITS.find(([, value]) => Math.abs(elapsed) >= value) ?? ["minute", 60_000];
  return new Intl.RelativeTimeFormat(language, { numeric: "auto" }).format(
    Math.round(elapsed / duration),
    unit,
  );
}
