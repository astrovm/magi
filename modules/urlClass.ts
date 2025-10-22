import { isAValidUrl } from './commonFunctions';
import StringValue from './stringValue';

export default class Url extends StringValue {
  constructor(url: string) {
    Url.validate(url);
    super(url);
  }

  private static validate(url: string): void {
    if (!isAValidUrl(url)) {
      throw new Error('Invalid URL provided');
    }
  }

}
