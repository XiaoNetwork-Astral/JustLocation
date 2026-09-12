import { encodeBase64, moduleExec, type Exec } from './platform';
import type { CellCommand, CellResponse } from './protocol';

export type CellClient = (command: CellCommand) => Promise<CellResponse>;

export function createCellClient(exec: Exec): CellClient {
  return async (command) => {
    const bytes = new TextEncoder().encode(JSON.stringify({ version: 1, ...command }));
    if (bytes.length >= 65536) throw new Error('基站文件太大，请分成较小的区域');
    const encoded = encodeBase64(bytes);
    const result = await exec(`/data/adb/modules/justlocation/bin/justlocationd cells ${encoded}`);
    if (result.errno) throw new Error('基站查询服务未响应，请检查模块版本');
    const response = JSON.parse(result.stdout);
    if (response.version !== 1 || typeof response.ok !== 'boolean')
      throw new Error('基站服务版本不兼容');
    if (!response.ok) throw new Error(response.error || '基站操作失败');
    if ((command.op === 'settings' || command.op === 'configure') && !response.settings)
      throw new Error('供应商设置无法读取');
    if (
      command.op === 'query' &&
      (!Array.isArray(response.dataset?.region?.cells) || !response.dataset.attribution)
    )
      throw new Error('基站数据格式不兼容');
    return response;
  };
}
export const cellClient = createCellClient(moduleExec);

export function safeCellLink(value: string | null) {
  try {
    const url = new URL(value || '');
    return ['http:', 'https:'].includes(url.protocol) ? url.href : undefined;
  } catch {
    return undefined;
  }
}
