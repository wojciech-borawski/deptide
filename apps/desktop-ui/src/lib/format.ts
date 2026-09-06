import { DateTime, Duration } from "luxon";

export function formatDuration(durationMs: number): string {
  const duration = Duration.fromMillis(durationMs).shiftTo("hours", "minutes", "seconds");
  if (duration.hours >= 1) return duration.toFormat("h'h' mm'm' ss's'");
  if (duration.minutes >= 1) return duration.toFormat("m'm' ss's'");
  return `${Math.floor(duration.seconds)}.${Math.floor((durationMs % 1000) / 100)}s`;
}

export function formatClock(durationMs: number): string {
  const duration = Duration.fromMillis(durationMs);
  return duration.as("hours") >= 1 ? duration.toFormat("h:mm:ss") : duration.toFormat("mm:ss");
}

export function formatDateTime(iso: string): string {
  const moment = DateTime.fromISO(iso);
  if (!moment.isValid) return iso;
  return moment.toLocaleString({ year: "numeric", month: "short", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}
