import { isAValidUrl } from './commonFunctions';

export default class Url {
  private url: string;

  constructor(url: string) {
    this.url = url;
  }

  isAValidUrl() {
    return isAValidUrl(this.url);
  }

  lenghtIsGreaterThan(lenght: number) {
    return this.url.length > lenght;
  }
}
