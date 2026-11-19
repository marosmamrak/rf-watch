export function mhz(v: number): string {
  return `${v.toFixed(1)} MHz`;
}

export function dbm(v: number): string {
  return `${v.toFixed(0)} dBm`;
}

/** "4 s", "2 min 05 s" */
export function duration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} s`;
  return `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, '0')} s`;
}

export function percent(v: number): string {
  return `${Math.round(v * 100)} %`;
}
