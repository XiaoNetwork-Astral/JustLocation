import { encodeBase64, moduleExec, type Exec } from './platform';

export async function exportToDownloads(
  content: string,
  filename: string,
  exec: Exec,
): Promise<string> {
  if (!/^[a-zA-Z0-9_-]+\.(json|gpx)$/.test(filename)) throw new Error('无效的导出文件名');
  const directory = '/sdcard/Download/JustLocation';
  const path = `${directory}/${filename}`;
  const temporary = `${directory}/.${filename}.${crypto.randomUUID()}.tmp`;
  async function run(command: string) {
    const result = await exec(command);
    if (result.errno !== 0) throw new Error('导出失败，请检查手机的存储空间后重试');
  }
  try {
    await run(`mkdir -p '${directory}' && (umask 077; set -C; : > '${temporary}')`);
    const bytes = new TextEncoder().encode(content);
    for (let start = 0; start < bytes.length; start += 24000) {
      const chunk = encodeBase64(bytes.subarray(start, start + 24000));
      await run(`printf '%s' '${chunk}' | base64 -d >> '${temporary}'`);
    }
    await run(`test ! -e '${path}' && chmod 644 '${temporary}' && mv '${temporary}' '${path}'`);
    return path;
  } catch (error) {
    try {
      await exec(`rm -f '${temporary}'`);
    } catch {
      /* The next export uses a new temporary file. */
    }
    throw error;
  }
}
export async function saveFile(content: string, extension: 'json' | 'gpx'): Promise<string> {
  const filename = `justlocation-${new Date().toISOString().replace(/[:.]/g, '-')}-${crypto.randomUUID().slice(0, 8)}.${extension}`;
  if ('ksu' in window) {
    await exportToDownloads(content, filename, moduleExec);
    return `已保存到 Download/JustLocation/${filename}`;
  }
  const url = URL.createObjectURL(
    new Blob([content], {
      type: extension === 'json' ? 'application/json' : 'application/gpx+xml',
    }),
  );
  const link = document.createElement('a');
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 30000);
  return '已交给浏览器下载';
}
