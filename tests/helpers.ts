export class MemoryKV {
  readonly store: Map<string, string>;

  readonly getCalls: Array<{ key: string; options: unknown }>;

  constructor() {
    this.store = new Map();
    this.getCalls = [];
  }

  async get(key: string, options?: unknown): Promise<string | null> {
    this.getCalls.push({ key, options });
    return this.store.get(key) ?? null;
  }

  async put(key: string, value: string): Promise<void> {
    this.store.set(key, value);
  }
}

export const md5 = (text: string): string => new Bun.CryptoHasher('md5').update(text).digest('hex');
