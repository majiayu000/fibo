import { expect, test } from 'bun:test';
import { fit } from '../src/ui/fit';

test('a picture is shrunk along its longest side and never grown', () => {
  expect(fit(4000, 3000, 1568)).toEqual({ width: 1568 });
  expect(fit(3000, 4000, 1568)).toEqual({ height: 1568 });
  expect(fit(1568, 900, 1568)).toBeNull();
  expect(fit(300, 200, 1568)).toBeNull();
});
