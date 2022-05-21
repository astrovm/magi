import { hasSpecialChars, hashText } from './commonFunctions';

export default class Alias {
  private alias: string;

  constructor(alias: string) {
    this.alias = alias;
  }

  get(): string {
    return this.alias;
  }

  replaceSpacesWith(char: string) {
    this.alias = this.alias.replace(/\s/g, char);
  }

  lenghtIsGreaterThan(lenght: number): boolean {
    return this.alias.length > lenght;
  }

  includes(char: string): boolean {
    return this.alias.includes(char);
  }

  hasSpecialChars(): boolean {
    return hasSpecialChars(this.alias);
  }

  async getHash(): Promise<string> {
    const lowerCaseAlias = this.alias.toLowerCase();
    return hashText(lowerCaseAlias);
  }

  decode() {
    this.alias = decodeURIComponent(this.alias);
  }
}
