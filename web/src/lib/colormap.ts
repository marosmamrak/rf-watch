export const DBM_MIN = -110;
export const DBM_MAX = -45;

/** Maps dBm to 0..1 over the display range, clamped. */
export function unit(dbm: number, min = DBM_MIN, max = DBM_MAX): number {
  return Math.min(1, Math.max(0, (dbm - min) / (max - min)));
}

// black → deep blue → violet → red → orange → pale yellow
const STOPS: [number, [number, number, number]][] = [
  [0.0, [5, 7, 12]],
  [0.2, [18, 30, 90]],
  [0.4, [100, 30, 140]],
  [0.6, [210, 40, 60]],
  [0.8, [250, 150, 30]],
  [1.0, [255, 245, 200]],
];

/** Heat colour for t in 0..1. */
export function heat(t: number): [number, number, number] {
  const x = Math.min(1, Math.max(0, t));
  for (let i = 1; i < STOPS.length; i++) {
    const [t1, c1] = STOPS[i];
    if (x <= t1) {
      const [t0, c0] = STOPS[i - 1];
      const f = (x - t0) / (t1 - t0);
      return [0, 1, 2].map((k) => Math.round(c0[k] + (c1[k] - c0[k]) * f)) as [number, number, number];
    }
  }
  return STOPS[STOPS.length - 1][1];
}

/** 256-entry lookup table, RGB triplets. */
export const HEAT_LUT: Uint8ClampedArray = (() => {
  const lut = new Uint8ClampedArray(256 * 3);
  for (let i = 0; i < 256; i++) lut.set(heat(i / 255), i * 3);
  return lut;
})();
