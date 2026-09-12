import type { Command, State } from './protocol';
import { encodeBase64, type Exec } from './platform';

export type Client = (command: Command) => Promise<State>;

export function createClient(exec: Exec): Client {
  return async (command) => {
    const bytes = new TextEncoder().encode(JSON.stringify({ version: 1, ...command }));
    const encoded = encodeBase64(bytes);
    const result = await exec(
      `/data/adb/modules/justlocation/bin/justlocationd request ${encoded}`,
    );
    if (result.errno !== 0)
      throw new Error(result.stderr.trim() || '后台未响应，请检查模块是否启用');
    const response = JSON.parse(result.stdout);
    if (
      response.version !== 1 ||
      typeof response.ok !== 'boolean' ||
      typeof response.state?.requested_active !== 'boolean' ||
      !('config' in response.state)
    ) {
      throw new Error('后台响应格式不兼容，请更新模块和面板');
    }
    if (!response.ok) throw new Error(response.error || '操作失败');
    return response.state;
  };
}
