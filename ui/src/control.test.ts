import { describe, expect, it } from 'vitest';
import { createClient } from './control';

describe('control transport', () => {
  it('encodes JSON as one shell-safe base64 argument', async () => {
    let command = '';
    const client = createClient(async (value) => {
      command = value;
      return {
        errno: 0,
        stderr: '',
        stdout: JSON.stringify({
          version: 1,
          ok: true,
          error: null,
          state: { requested_active: false, config: null },
        }),
      };
    });
    await client({ op: 'status' });
    const encoded = command.split(' ').at(-1)!;
    expect(encoded).toMatch(/^[A-Za-z0-9+/]+=*$/);
    expect(JSON.parse(atob(encoded))).toEqual({ version: 1, op: 'status' });
  });
  it('propagates backend and command errors instead of showing success', async () => {
    await expect(
      createClient(async () => ({ errno: 1, stdout: '', stderr: 'service unavailable' }))({
        op: 'status',
      }),
    ).rejects.toThrow('service unavailable');
    await expect(
      createClient(async () => ({
        errno: 0,
        stderr: '',
        stdout: JSON.stringify({
          version: 1,
          ok: false,
          error: 'invalid position',
          state: { requested_active: false, config: null },
        }),
      }))({ op: 'status' }),
    ).rejects.toThrow('invalid position');
  });
  it('rejects unexpected protocol versions and malformed state', async () => {
    for (const value of [{ version: 2 }, { version: 1, ok: true, state: {} }]) {
      await expect(
        createClient(async () => ({ errno: 0, stderr: '', stdout: JSON.stringify(value) }))({
          op: 'status',
        }),
      ).rejects.toThrow();
    }
  });
});
