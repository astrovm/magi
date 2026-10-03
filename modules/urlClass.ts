import { isAValidUrl, withScheme } from './commonFunctions';
import StringValue from './stringValue';

export default class Url extends StringValue {
  constructor(url: string) {
    Url.validate(url);
    super(url);
  }

  static fromInput(input: string): Url {
    return new Url(withScheme(input));
  }

  private static validate(url: string): void {
    if (!isAValidUrl(url)) {
      throw new Error('Invalid URL provided');
    }
  }

  getHost(): string {
    return new URL(this.value).host;
  }
}
