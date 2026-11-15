import type { Language } from "./i18n";
import type { ThemePreference } from "./types";

/** Last component of a Windows or POSIX path. */
export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** Folder part of a path, without the trailing separator. */
export function folderOf(path: string): string {
  const index = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return index > 0 ? path.slice(0, index) : "";
}

const UNITS = ["B", "KB", "MB", "GB"];

/** Size in bytes as a short readable string, e.g. "1.4 MB". */
export function formatBytes(bytes: number, language: Language): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  const number = new Intl.NumberFormat(language, {
    maximumFractionDigits: digits,
    minimumFractionDigits: digits,
  }).format(value);
  return `${number} ${UNITS[unit]}`;
}

/** Date and time a file was opened, from seconds since the epoch. */
export function formatOpenedAt(seconds: number, language: Language): string {
  return new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(
    new Date(seconds * 1000),
  );
}

export function resolveTheme(preference: ThemePreference, systemPrefersDark: boolean): "light" | "dark" {
  if (preference === "system") return systemPrefersDark ? "dark" : "light";
  return preference;
}
