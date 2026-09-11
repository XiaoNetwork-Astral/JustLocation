import { useEffect } from 'react';
import { ArrowLeft } from 'lucide-react';
import { SwitchRow } from './Controls';
import type { GnssConfig, State } from './control';

type Props = {
  state: State | null;
  busy: boolean;
  onToggle(patch: Partial<GnssConfig>): void;
  onClose(): void;
};

/** 卫星开关的说明文字：把"开关开着但接口没接上"和"已生效"区分开。 */
const note = (enabled: boolean | undefined, ready: boolean | undefined, active: boolean, purpose: string) =>
  !enabled ? purpose : !active ? '已启用，开始位置模拟后生效' : ready ? '已连接系统接口' : '等待系统接口接入';

/**
 * 卫星面板。
 *
 * GNSS 状态与 NMEA 是本项目自己的通道，原版把这类东西放在设置里
 * （`res/xml/g.xml` 的「位置模拟」分类下有「模拟GPS信号 (ROOT)」等条目），
 * 而不是堆在首页。所以这里做成从目标卡进入的一个独立面板，首页只留
 * 目标卡 / 区块头 / 点位列表三段。
 */
export function SatellitePanel({ state, busy, onToggle, onClose }: Props) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return <main className="full-screen" role="dialog" aria-label="卫星">
    <header className="scope-topbar screen-topbar">
      <button className="icon-button" aria-label="返回" onClick={onClose}><ArrowLeft size={20} /></button>
      <h1>卫星</h1>
      <button className="text-button" onClick={onClose}>完成</button>
    </header>
    <div className="scope-content">
      <p className="scope-note">这两条通道只在位置模拟运行、且应用在作用范围内时接管系统的星座与报文回调。</p>
      <section className="card">
        <div className="feature-list">
          <SwitchRow title="GNSS 状态" checked={!!state?.gnss?.gnss_enabled} disabled={busy || !state}
            summary={note(state?.gnss?.gnss_enabled, state?.gnss_hook_ready, !!state?.requested_active, '投递预置的卫星状态与首次定位回调')}
            onChange={next => onToggle({ gnss_enabled: next })} />
          <SwitchRow title="NMEA 报文" checked={!!state?.gnss?.nmea_enabled} disabled={busy || !state}
            summary={note(state?.gnss?.nmea_enabled, state?.nmea_hook_ready, !!state?.requested_active, '命中范围时丢弃系统的 NMEA 回调，不合成报文')}
            onChange={next => onToggle({ nmea_enabled: next })} />
        </div>
      </section>
      <section className="card">
        <h2>接口状态</h2>
        <dl>
          <div><dt>GNSS 状态</dt><dd>{state?.gnss_hook_ready ? '已就绪' : state ? '尚未接入' : '后台未连接'}</dd></div>
          <div><dt>NMEA 报文</dt><dd>{state?.nmea_hook_ready ? '已就绪' : state ? '尚未接入' : '后台未连接'}</dd></div>
        </dl>
      </section>
    </div>
  </main>;
}
