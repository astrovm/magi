export default abstract class StringValue {
  protected value: string;

  constructor(value: string) {
    this.value = value;
  }

  get(): string {
    return this.value;
  }

  lengthIsGreaterThan(length: number): boolean {
    return this.value.length > length;
  }

  toString(): string {
    return this.value;
  }
}
