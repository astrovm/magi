import { hasSpecialChars, hashText } from './commonFunctions';
import StringValue from './stringValue';

export default class Alias extends StringValue {
  constructor(alias: string) {
    super(alias);
  }

  replaceSpacesWith(char: string): void {
    this.value = this.value.replace(/\s/g, char);
  }

  includes(char: string): boolean {
    return this.value.includes(char);
  }

  hasSpecialChars(): boolean {
    return hasSpecialChars(this.value);
  }

  decode(): boolean {
    const originalAlias = this.value;

    try {
      this.value = decodeURIComponent(this.value);
      return true;
    } catch (error) {
      if (error instanceof URIError) {
        this.value = originalAlias;
        return false;
      }

      throw error;
    }
  }

  normalize(): string {
    this.decode();
    this.value = this.value.replace(/\s+/g, ' ').trim();
    return this.value;
  }

  async getHash(): Promise<string> {
    const lowerCaseAlias = this.value.toLowerCase();
    return hashText(lowerCaseAlias);
  }
}
