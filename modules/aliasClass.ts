import { hasSpecialChars, hashText } from './commonFunctions';

export default class Alias {
  private alias: string;

  constructor(alias: string) {
    this.alias = alias;
  }

  get() {
    return this.alias;
  }

  replaceSpacesWith(char: string) {
    this.alias = this.alias.replace(/\s/g, char);
  }

  lenghtIsGreaterThan(lenght: number) {
    return this.alias.length > lenght;
  }

  includes(char: string) {
    return this.alias.includes(char);
  }

  hasSpecialChars() {
    return hasSpecialChars(this.alias);
  }

  async getHash() {
    const lowerCaseAlias = this.alias.toLowerCase();
    return hashText(lowerCaseAlias);
  }

  decode() {
    this.alias = decodeURIComponent(this.alias);
  }
}
