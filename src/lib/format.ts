export function formatDuration(totalSeconds: number): string {
  const safeSeconds = Math.max(0, Math.round(totalSeconds));
  const minutes = Math.floor(safeSeconds / 60);
  const seconds = safeSeconds % 60;

  return `${minutes.toString().padStart(2, "0")}:${seconds.toString().padStart(2, "0")}`;
}

export function formatRate(crystal: number, seconds: number): string {
  if (seconds <= 0) {
    return "0.0";
  }

  return ((crystal / seconds) * 3600).toFixed(1);
}

export function formatNumber(value: number, digits = 1): string {
  return value.toFixed(digits);
}

