import { hasSpecialChars, hashText } from './commonFunctions';

export default class Alias {
  private alias: string;

  constructor(alias: string) {
    this.alias = alias;
  }

  get(): string {
    return this.alias;
  }

  replaceSpacesWith(char: string): void {
    this.alias = this.alias.replace(/\s/g, char);
  }

  lengthIsGreaterThan(length: number): boolean {
    return this.alias.length > length;
  }

  includes(char: string): boolean {
    return this.alias.includes(char);
  }

  hasSpecialChars(): boolean {
    return hasSpecialChars(this.alias);
  }

  decode(): boolean {
    const originalAlias = this.alias;

    try {
      this.alias = decodeURIComponent(this.alias);
      return true;
    } catch (error) {
      if (error instanceof URIError) {
        this.alias = originalAlias;
        return false;
      }

      throw error;
    }
  }

  normalize(): string {
    this.decode();
    this.alias = this.alias.replace(/\s+/g, ' ').trim();
    return this.alias;
  }

  async getHash(): Promise<string> {
    const lowerCaseAlias = this.alias.toLowerCase();
    return hashText(lowerCaseAlias);
  }

  toString(): string {
    return this.alias;
  }
}
