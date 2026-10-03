type PutOptions = { expiration?: number; metadata?: unknown };

export class MemoryKV {
  readonly store: Map<string, string>;

  readonly metadata: Map<string, unknown>;

  readonly getCalls: Array<{ key: string; options: unknown }>;

  readonly putCalls: Array<{ key: string; value: string; options: PutOptions | undefined }>;

  constructor() {
    this.store = new Map();
    this.metadata = new Map();
    this.getCalls = [];
    this.putCalls = [];
  }

  async get(key: string, options?: unknown): Promise<string | null> {
    this.getCalls.push({ key, options });
    return this.store.get(key) ?? null;
  }

  async getWithMetadata(key: string, options?: unknown): Promise<{ value: string | null; metadata: unknown }> {
    this.getCalls.push({ key, options });
    return { value: this.store.get(key) ?? null, metadata: this.metadata.get(key) ?? null };
  }

  async put(key: string, value: string, options?: PutOptions): Promise<void> {
    this.putCalls.push({ key, value, options });
    this.store.set(key, value);
    if (options?.metadata !== undefined) {
      this.metadata.set(key, options.metadata);
    }
  }

  async delete(key: string): Promise<void> {
    this.store.delete(key);
    this.metadata.delete(key);
  }
}

export const md5 = (text: string): string => new Bun.CryptoHasher('md5').update(text).digest('hex');

export const sha256 = (text: string): string => new Bun.CryptoHasher('sha256').update(text).digest('hex');
