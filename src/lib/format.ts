const units = ["B", "KB", "MB", "GB", "TB"];

/** Formats a byte count the way Windows Explorer does (1024-based, one decimal). */
export function formatBytes(bytes: number, locale = navigator.language): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  const digits = value >= 100 || unit === 0 ? 0 : 1;
  return `${value.toLocaleString(locale, { maximumFractionDigits: digits })} ${units[unit]}`;
}

export const GB = 1024 ** 3;
export const MB = 1024 ** 2;
