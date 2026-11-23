import { describe, expect, it } from 'vitest';
import { fitSize } from './canvas';

describe('canvas size', () => {
  it('fits like the server', () => {
    expect(fitSize(6240, 4160, 2048, 2048)).toEqual([2048, 1365]);
    expect(fitSize(4160, 6240, 1024, 1024)).toEqual([682, 1024]);
    expect(fitSize(600, 400, 4096, 4096)).toEqual([600, 400]);
    // The core's own test case: 480x320 into 200x200.
    expect(fitSize(480, 320, 200, 200)).toEqual([200, 133]);
  });
});
