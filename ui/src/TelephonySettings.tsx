import { useEffect, useState } from 'react';
import type { State, Subscription, TelephonyConfig } from './protocol';
import { SwitchRow } from './Controls';

const defaults: TelephonyConfig = {
  cells_enabled: false,
  sim_enabled: false,
  radius_m: 500,
  subscriptions: [],
};
function draftFor(state: State): TelephonyConfig {
  const config = state.telephony || defaults;
  return {
    ...config,
    subscriptions: (state.detected_subscriptions || []).map((card) => {
      const saved = config.subscriptions.find((s) => s.id === card.id && s.slot === card.slot);
      return saved
        ? { ...saved }
        : { ...card, carrier: card.carrier || `SIM ${card.slot + 1}`, enabled: true };
    }),
  };
}
const slots = (cards: { id: number; slot: number }[]) =>
  cards
    .map((c) => `${c.slot}:${c.id}`)
    .sort()
    .join(',');

export function TelephonySettings({
  state,
  busy,
  onSave,
}: {
  state: State;
  busy: boolean;
  onSave(config: TelephonyConfig): Promise<void>;
}) {
  const [draft, setDraft] = useState(() => draftFor(state));
  const [radius, setRadius] = useState(String(draft.radius_m));
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [note, setNote] = useState('');
  const serialized = JSON.stringify([state.telephony, state.detected_subscriptions]);
  useEffect(() => {
    if (!dirty) {
      const next = draftFor(state);
      setDraft(next);
      setRadius(String(next.radius_m));
    }
  }, [serialized]);
  function update(next: TelephonyConfig) {
    setDraft(next);
    setDirty(true);
    setNote('');
    setError('');
  }
  function card(index: number, fields: Partial<Subscription>) {
    update({
      ...draft,
      subscriptions: draft.subscriptions.map((sub, i) =>
        i === index ? { ...sub, ...fields } : sub,
      ),
    });
  }
  async function save() {
    setError('');
    setNote('');
    if (slots(draft.subscriptions) !== slots(state.detected_subscriptions || [])) {
      setError('SIM 卡已变化，请重新打开此页后设置');
      return;
    }
    const radius_m = Number(radius);
    if (!radius.trim() || !Number.isFinite(radius_m) || radius_m < 1 || radius_m > 200000) {
      setError('模拟半径须在 1～200000 米之间');
      return;
    }
    const enabled = draft.subscriptions.filter((sub) => sub.enabled);
    if ((draft.cells_enabled || draft.sim_enabled) && !enabled.length) {
      setError('请至少选择一张 SIM 卡');
      return;
    }
    if (
      enabled.some(
        (sub) =>
          !/^[0-9]{3}$/.test(sub.mcc) ||
          !/^[0-9]{2,3}$/.test(sub.mnc) ||
          !/^[a-z]{2}$/.test(sub.country) ||
          !sub.carrier.trim() ||
          new TextEncoder().encode(sub.carrier).length > 128 ||
          /[\u0000-\u001f\u007f-\u009f]/.test(sub.carrier) ||
          (sub.cdma_sid !== undefined &&
            (!Number.isInteger(sub.cdma_sid) || sub.cdma_sid < 0 || sub.cdma_sid > 32767)),
      )
    ) {
      setError('请检查所选 SIM 卡的运营商信息');
      return;
    }
    setSaving(true);
    try {
      await onSave({ ...draft, radius_m });
      setDirty(false);
      setNote(state.requested_active ? '模拟设置已更新' : '设置已保存，开始位置模拟后生效。');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }
  const unavailable = !state.phone_connected || state.detected_subscriptions == null;
  const output = state.telephony_output;
  return (
    <section className="telephony-settings" aria-label="基站模拟设置">
      <h3>模拟设置</h3>
      {!state.phone_connected ? (
        <p role="status">等待电话服务连接</p>
      ) : state.detected_subscriptions == null ? (
        <p role="status">正在读取 SIM 卡</p>
      ) : (
        !state.detected_subscriptions.length && <p role="status">没有检测到可用的 SIM 卡</p>
      )}
      <fieldset className="telephony-fields">
        <SwitchRow
          title="模拟基站"
          checked={draft.cells_enabled}
          disabled={busy || saving || unavailable}
          summary="按目标位置附近的基站返回读数"
          onChange={(next) => update({ ...draft, cells_enabled: next })}
        />
        <SwitchRow
          title="模拟 SIM 运营商"
          checked={draft.sim_enabled}
          disabled={busy || saving || unavailable}
          summary="让应用读到你设置的运营商信息"
          onChange={(next) => update({ ...draft, sim_enabled: next })}
        />
        {draft.subscriptions.map((sub, index) => (
          <div className="sim-card" key={`${sub.slot}:${sub.id}`}>
            <SwitchRow
              title={`SIM ${sub.slot + 1} · ${sub.carrier}`}
              checked={sub.enabled}
              disabled={busy || saving || unavailable}
              summary="参与基站与运营商模拟"
              onChange={(next) => card(index, { enabled: next })}
            />
            <details>
              <summary>修改运营商</summary>
              <div className="sim-fields">
                <label className="field">
                  运营商名称
                  <input
                    disabled={busy || saving || unavailable}
                    value={sub.carrier}
                    onChange={(e) => card(index, { carrier: e.target.value })}
                  />
                </label>
                <label className="field">
                  国家代码（MCC）
                  <input
                    disabled={busy || saving || unavailable}
                    inputMode="numeric"
                    maxLength={3}
                    value={sub.mcc}
                    onChange={(e) => card(index, { mcc: e.target.value })}
                  />
                </label>
                <label className="field">
                  网络代码（MNC）
                  <input
                    disabled={busy || saving || unavailable}
                    inputMode="numeric"
                    maxLength={3}
                    value={sub.mnc}
                    onChange={(e) => card(index, { mnc: e.target.value })}
                  />
                </label>
                <label className="field">
                  国家/地区
                  <input
                    disabled={busy || saving || unavailable}
                    maxLength={2}
                    placeholder="例如 cn"
                    value={sub.country}
                    onChange={(e) => card(index, { country: e.target.value.toLowerCase() })}
                  />
                </label>
                <label className="field">
                  CDMA 系统 ID（可选）
                  <input
                    disabled={busy || saving || unavailable}
                    inputMode="numeric"
                    value={sub.cdma_sid ?? ''}
                    onChange={(e) =>
                      card(index, {
                        cdma_sid: e.target.value.trim() ? Number(e.target.value) : undefined,
                      })
                    }
                  />
                </label>
              </div>
            </details>
          </div>
        ))}
        <label className="field">
          模拟半径（米）
          <input
            disabled={busy || saving || unavailable}
            inputMode="numeric"
            value={radius}
            onChange={(e) => {
              setRadius(e.target.value);
              setDirty(true);
              setNote('');
            }}
          />
        </label>
        <button
          className={`tonal-button${dirty ? '' : ' muted'}`}
          disabled={busy || saving || unavailable || !dirty}
          onClick={() => void save()}
        >
          保存模拟设置
        </button>
      </fieldset>
      {dirty && <p>设置有修改，保存后生效。</p>}
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      {note && <p role="status">{note}</p>}
      {state.requested_active && state.telephony?.cells_enabled && (
        <p role="status">
          {!state.cell_hook_ready
            ? '基站服务尚未就绪'
            : output?.availability === 'missing_region'
              ? '请查询并应用附近基站'
              : output?.availability === 'outside_region'
                ? '当前位置超出基站数据范围，请重新查询'
                : output?.groups.some((group) => group.cells.length > 0)
                  ? '基站模拟已生效'
                  : '范围内没有与所选运营商匹配的基站'}
        </p>
      )}
      <p>基站使用下方查询并应用的数据；SIM 运营商设置不会更换手机实际使用的号码或网络。</p>
    </section>
  );
}
