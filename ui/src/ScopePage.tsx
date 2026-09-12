import { ArrowLeft } from 'lucide-react';
import { AppPicker } from './AppPicker';
import type { LoadApps } from './installedApps';

type Props = {
  limited: boolean;
  packages: string[];

  locked: boolean;
  error: string;
  loadApps: LoadApps;
  onChange(packages: string[]): void;

  onClearScope(): void;
  canClear: boolean;
  onClose(): void;
};

export function ScopePage({
  limited,
  packages,
  locked,
  error,
  loadApps,
  onChange,
  onClearScope,
  canClear,
  onClose,
}: Props) {
  return (
    <main className="scope-screen full-screen">
      <header className="scope-topbar screen-topbar">
        <button className="icon-button" aria-label="返回" onClick={onClose}>
          <ArrowLeft size={20} />
        </button>
        <h1>作用范围</h1>
        <button className="text-button" onClick={onClose}>
          完成
        </button>
      </header>
      <div className="scope-content">
        <p className="scope-note">
          {limited
            ? '只有勾选的应用会使用模拟位置，其他应用保持真实定位。'
            : '当前是所有应用都使用模拟位置，回到位置模拟页打开“作用范围”开关即可改为只对指定应用生效。'}
        </p>
        <p className="scope-hint">选择自动保存，开始模拟时生效</p>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        {limited ? (
          <AppPicker
            selected={packages}
            disabled={locked}
            loadApps={loadApps}
            onChange={onChange}
          />
        ) : (
          <p className="scope-all-note">所有应用都会使用模拟位置。之前勾选的应用已保留。</p>
        )}
        {canClear && limited && (
          <div className="scope-footer">
            <button type="button" className="text-button" disabled={locked} onClick={onClearScope}>
              改为全部应用
            </button>
            <span>清掉范围限制，所有应用都使用模拟位置；勾选本身会保留。</span>
          </div>
        )}
      </div>
    </main>
  );
}
