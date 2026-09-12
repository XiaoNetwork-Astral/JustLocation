import { exec } from 'kernelsu';

export type Exec = (command: string) => Promise<{
  errno: number;
  stdout: string;
  stderr: string;
}>;

export const moduleExec: Exec = async (command) => {
  if (!('ksu' in window)) throw new Error('请从 KernelSU 管理器打开模块面板');
  return exec(command);
};

export function encodeBase64(bytes: Uint8Array): string {
  return btoa(Array.from(bytes, (byte) => String.fromCharCode(byte)).join(''));
}
