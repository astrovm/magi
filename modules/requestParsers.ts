import { isString } from './typeGuards';

const readStringField = (value: unknown): string | null => {
  if (isString(value)) {
    return value;
  }

  return null;
};

export { readStringField };
