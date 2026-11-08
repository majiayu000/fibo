// Kept apart from pictures.ts so it runs without the native image modules.

/** What a step makes of a picture `w` by `h`: the size to scale to, or none when it is already small. */
export function fit(w: number, h: number, side: number): { width: number } | { height: number } | null {
  if (Math.max(w, h) <= side) return null;
  return w >= h ? { width: side } : { height: side };
}
