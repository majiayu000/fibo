// Dials a machine through every relay it is on at once (docs/decisions/2026-11-26-two-relays.md).
// A handshake crosses phone → relay → machine, so the first one to finish is the fastest
// path right now; the others are closed when they arrive. Only when all fail does it fail,
// with the first relay's reason, as a phone with one relay would.
export function firstThrough<T extends { close(): void }>(dials: (() => Promise<T>)[]): Promise<T> {
  if (dials.length === 1) return dials[0]();
  return new Promise((resolve, reject) => {
    let won = false;
    let left = dials.length;
    const errors: unknown[] = [];
    dials.forEach((dial, i) =>
      dial().then(
        (r) => {
          if (won) r.close();
          else {
            won = true;
            resolve(r);
          }
        },
        (e) => {
          errors[i] = e;
          if (--left === 0 && !won) reject(errors[0]);
        },
      ),
    );
  });
}
