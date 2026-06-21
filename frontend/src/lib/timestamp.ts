// Parsing and formatting of caption timestamps on the client.

/**
 * Parse a human timestamp into whole seconds.
 *
 * Accepts:
 *   - plain seconds: "83", "83.4"
 *   - mm:ss:        "1:23"
 *   - hh:mm:ss:     "1:02:03"
 *
 * Returns null for anything malformed (negative, non-numeric, > 24h, out of
 * range minute/second fields).
 */
export function parseTimestamp(input: string): number | null {
  const s = input.trim();
  if (s === "") return null;

  if (!s.includes(":")) {
    const n = Number(s);
    if (!Number.isFinite(n) || n < 0) return null;
    return clamp(Math.floor(n));
  }

  const parts = s.split(":");
  if (parts.length < 2 || parts.length > 3) return null;

  const nums = parts.map((p) => Number(p));
  if (nums.some((n) => !Number.isFinite(n) || n < 0)) return null;

  let h = 0;
  let m: number;
  let sec: number;
  if (parts.length === 3) {
    [h, m, sec] = nums;
  } else {
    [m, sec] = nums;
  }
  // Minute/second fields must be < 60 in colon notation.
  if (m >= 60 || sec >= 60) return null;

  const total = h * 3600 + m * 60 + Math.floor(sec);
  return clamp(total);
}

function clamp(n: number): number | null {
  if (n > 86400) return null;
  return n;
}

/** Format whole seconds as h:mm:ss (or m:ss when under an hour). */
export function formatTimestamp(totalSec: number): string {
  const s = Math.max(0, Math.floor(totalSec));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => n.toString().padStart(2, "0");
  if (h > 0) return `${h}:${pad(m)}:${pad(sec)}`;
  return `${m}:${pad(sec)}`;
}
