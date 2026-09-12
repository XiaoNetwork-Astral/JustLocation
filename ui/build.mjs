import { dirname, join } from 'node:path';
import { output, root, run, win } from '../tool/build.mjs';

export function npm(args) {
  const cli = join(dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js');
  const cache = ['--cache', join(output, 'cache', 'npm'), '--no-audit', '--no-fund'];
  if (win) run(process.execPath, [cli, ...args, ...cache], join(root, 'ui'));
  else run('npm', [...args, ...cache], join(root, 'ui'));
}

export function build() { npm(['run', 'build']); }
export function test() {
  npm(['run', 'check']);
  npm(['test']);
}
