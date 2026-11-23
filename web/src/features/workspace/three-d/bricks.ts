// The bricks kept (W7.8): a volume's bricks and slices, fetched by level, kept
// up to a budget of bytes, the least recently used let go first, so that the
// memory a volume takes is bounded however much of it is looked at.

/** What a brick takes: its samples' bytes. */
export interface Sized {
  samples: { byteLength: number };
}

export class Bricks<T extends Sized> {
  /** In the order used: the least recently used first. */
  private readonly kept = new Map<string, T>();
  private bytes = 0;

  constructor(readonly budget: number) {}

  /** Brick `key`, if it is kept: it is then the most recently used. */
  get(key: string): T | undefined {
    const brick = this.kept.get(key);
    if (brick === undefined) return undefined;
    this.kept.delete(key);
    this.kept.set(key, brick);
    return brick;
  }

  /** Brick `key` kept, the least recently used let go until the rest fits the budget; one larger than it isn't kept. */
  put(key: string, brick: T): void {
    const old = this.kept.get(key);
    if (old) {
      this.kept.delete(key);
      this.bytes -= old.samples.byteLength;
    }
    if (brick.samples.byteLength > this.budget) return;
    this.kept.set(key, brick);
    this.bytes += brick.samples.byteLength;
    for (const [oldest, one] of this.kept) {
      if (this.bytes <= this.budget) break;
      this.kept.delete(oldest);
      this.bytes -= one.samples.byteLength;
    }
  }

  /** Brick `key`, kept or fetched by `fetch`, then kept. */
  async of(key: string, fetch: () => Promise<T>): Promise<T> {
    const kept = this.get(key);
    if (kept) return kept;
    const brick = await fetch();
    this.put(key, brick);
    return brick;
  }

  /** The bytes kept, and the bricks. */
  get size(): { bytes: number; bricks: number } {
    return { bytes: this.bytes, bricks: this.kept.size };
  }
}
