import { expect, it, vi } from 'vitest';
import { exportToDownloads } from './fileExport';

it('writes UTF-8 in bounded chunks and publishes only after all chunks succeed', async () => {
  const exec = vi.fn().mockResolvedValue({ errno: 0, stdout: '', stderr: '' });
  const content = '家🏠'.repeat(12000);
  const path = await exportToDownloads(content, 'backup.json', exec);
  expect(path).toBe('/sdcard/Download/JustLocation/backup.json');
  const commands = exec.mock.calls.map((call) => call[0] as string);
  const chunks = commands.filter((command) => command.includes('base64 -d'));
  expect(chunks.length).toBeGreaterThan(1);
  expect(commands.every((command) => command.length < 33000)).toBe(true);
  const decoded = chunks.flatMap((command) =>
    Array.from(atob(command.match(/printf '%s' '([^']+)'/)![1]), (char) => char.charCodeAt(0)),
  );
  expect(new TextDecoder().decode(new Uint8Array(decoded))).toBe(content);
  expect(commands.at(-1)).toContain('mv ');
});

it('does not publish an incomplete file and reports write failures', async () => {
  const exec = vi.fn().mockResolvedValue({ errno: 0, stdout: '', stderr: '' });
  exec
    .mockResolvedValueOnce({ errno: 0, stdout: '', stderr: '' })
    .mockResolvedValueOnce({ errno: 1, stdout: '', stderr: 'full' });
  await expect(exportToDownloads('example', 'backup.json', exec)).rejects.toThrow(/导出失败/);
  expect(exec.mock.calls.some((call) => call[0].includes('mv '))).toBe(false);
  expect(exec.mock.calls.at(-1)![0]).toContain('rm -f ');
});

it('rejects filenames that could escape the export directory or become shell syntax', async () => {
  const exec = vi.fn();
  for (const name of ['../a.json', "a';id;.json", 'file.txt']) {
    await expect(exportToDownloads('data', name, exec)).rejects.toThrow();
  }
  expect(exec).not.toHaveBeenCalled();
});
