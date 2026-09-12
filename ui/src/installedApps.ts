import { listPackages, getPackagesInfo } from 'kernelsu';

export interface InstalledApp {
  packageName: string;
  appLabel: string;
  isSystem: boolean;
}
export type LoadApps = () => Promise<InstalledApp[]>;
export const loadInstalledApps: LoadApps = async () => {
  if (!('ksu' in window)) throw new Error('请从 KernelSU 管理器打开面板');
  const packages = [...new Set(listPackages('all'))];
  const info = new Map(getPackagesInfo(packages).map((app) => [app.packageName, app]));
  return packages.map((packageName) => ({
    packageName,
    appLabel: info.get(packageName)?.appLabel || packageName,
    isSystem: info.get(packageName)?.isSystem === true,
  }));
};
