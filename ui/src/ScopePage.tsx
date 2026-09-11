import { ArrowLeft } from 'lucide-react';
import { AppPicker, type LoadApps } from './AppPicker';

type Props = {
  /** 开关代表"是否只对指定应用生效"；这里是它的说明与勾选界面 */
  limited: boolean;
  packages: string[];
  /** 模拟进行中时不允许改范围 */
  locked: boolean;
  error: string;
  loadApps: LoadApps;
  onChange(packages: string[]): void;
  /** 清掉限制，回到"所有应用"；只在限定生效时显示 */
  onClearScope(): void;
  canClear: boolean;
  onClose(): void;
};

/**
 * 作用范围页（对应原版的「独立模拟」）。
 *
 * 原版把它做成抽屉里的一个独立页面（`cf.xml`），首页上没有它的开关，
 * 只有目标卡操作行里的一个入口。这里遵循同样的做法：首页不再放这组开关，
 * 页面只负责勾选应用，模式由进入时带的意图决定（"只对指定应用生效"）。
 */
export function ScopePage({ limited, packages, locked, error, loadApps, onChange, onClearScope, canClear, onClose }: Props) {
  return <main className="scope-screen full-screen">
    <header className="scope-topbar screen-topbar">
      <button className="icon-button" aria-label="返回" onClick={onClose}><ArrowLeft size={20} /></button>
      <h1>作用范围</h1><button className="text-button" onClick={onClose}>完成</button>
    </header>
    <div className="scope-content">
      {/* 模式由首页的入口决定，这里不再提供切换，只负责选应用。 */}
      <p className="scope-note">{limited
        ? '只有勾选的应用会使用模拟位置，其他应用保持真实定位。'
        : '当前是所有应用都使用模拟位置，回到位置模拟页打开“作用范围”开关即可改为只对指定应用生效。'}</p>
      <p className="scope-hint">选择自动保存，开始模拟时生效</p>
      {error && <p className="form-error" role="alert">{error}</p>}
      {limited ? <AppPicker selected={packages} disabled={locked} loadApps={loadApps} onChange={onChange} />
        : <p className="scope-all-note">所有应用都会使用模拟位置。之前勾选的应用已保留。</p>}
      {canClear && limited && <div className="scope-footer">
        <button type="button" className="text-button" disabled={locked} onClick={onClearScope}>改为全部应用</button>
        <span>清掉范围限制，所有应用都使用模拟位置；勾选本身会保留。</span>
      </div>}
    </div>
  </main>;
}
