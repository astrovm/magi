import { isAValidUrl } from './commonFunctions';

export default class Url {
  private url: string;

  constructor(url: string) {
    Url.validate(url);
    this.url = url;
  }

  private static validate(url: string): void {
    if (!isAValidUrl(url)) {
      throw new Error('Invalid URL provided');
    }
  }

  get(): string {
    return this.url;
  }

  isValid(): boolean {
    return isAValidUrl(this.url);
  }

  lengthIsGreaterThan(length: number): boolean {
    return this.url.length > length;
  }

  toString(): string {
    return this.url;
  }
}
