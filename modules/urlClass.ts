import { isAValidUrl } from './commonFunctions';

export default class Url {
  private url: string;

  constructor(url: string) {
    this.url = url;
  }

  get(): string {
    return this.url;
  }

  isValid(): boolean {
    return isAValidUrl(this.url);
  }

  lenghtIsGreaterThan(lenght: number): boolean {
    return this.url.length > lenght;
  }
}
