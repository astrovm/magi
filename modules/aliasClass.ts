import { hasSpecialChars, hashText, RESERVED_ALIASES } from './commonFunctions';
import StringValue from './stringValue';

export default class Alias extends StringValue {
  constructor(alias: string) {
    super(alias);
  }

  replaceSpacesWith(char: string): void {
    this.value = this.value.replace(/\s/g, char);
  }

  hasDot(): boolean {
    return this.value.includes('.');
  }

  hasSpecialChars(): boolean {
    return hasSpecialChars(this.value);
  }

  isReserved(): boolean {
    return RESERVED_ALIASES.includes(this.value.toLowerCase());
  }

  /**
   * Removes a trailing marker like the `+` in `/alias+` and reports if it was there.
   */
  stripSuffix(suffix: string): boolean {
    if (!this.value.endsWith(suffix)) {
      return false;
    }
    this.value = this.value.slice(0, -suffix.length);
    return true;
  }

  decode(): this {
    const originalAlias = this.value;

    try {
      this.value = decodeURIComponent(this.value);
    } catch (error) {
      if (error instanceof URIError) {
        this.value = originalAlias;
      } else {
        throw error;
      }
    }

    return this;
  }

  normalize(): this {
    this.decode();
    this.value = this.value.replace(/\s+/g, ' ').trim();
    return this;
  }

  prepareForLookup(slugChar: string): { hasDot: boolean } {
    this.normalize();
    const hasDot = this.hasDot();
    this.replaceSpacesWith(slugChar);
    return { hasDot };
  }

  async getHash(): Promise<string> {
    const lowerCaseAlias = this.value.toLowerCase();
    return hashText(lowerCaseAlias);
  }
}
