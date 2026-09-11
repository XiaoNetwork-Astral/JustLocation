import { useEffect, useRef, useState } from 'react';
import { AppWindow, RadioTower, Joystick, Power, SlidersHorizontal, List } from 'lucide-react';

type Props = {
  independent: boolean; scopeLocked: boolean; toggleScope(): void; openScope(): void;
  speed: string; setSpeed(value: string): void; busy: boolean;
  canOpen: boolean; canClose: boolean; controlJoystick(open: boolean): void; note: string;
  openCells(): void;
  cellsEnabled: boolean; cellNote: string; toggleCells(): void;
};

export function FeatureMenus(props: Props) {
  const [menu, setMenu] = useState<'scope' | 'cell' | 'joystick' | null>(null);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!menu) return;
    const outside = (event: PointerEvent) => { if (!root.current?.contains(event.target as Node)) setMenu(null); };
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { setMenu(null); root.current?.querySelector<HTMLButtonElement>('[aria-expanded="true"]')?.focus(); }
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', outside); document.removeEventListener('keydown', escape); };
  }, [menu]);
  function show(next: typeof menu) { setMenu(menu === next ? null : next); }
  return <div className="feature-menus" ref={root}>
    <button className={`icon-button ${props.independent ? 'selected' : ''}`} aria-label="独立模拟菜单" title="独立模拟" aria-expanded={menu === 'scope'} aria-controls="feature-popover" onClick={() => show('scope')}><AppWindow size={21} /></button>
    <button className={`icon-button ${props.cellsEnabled ? 'selected' : ''}`} aria-label="基站菜单" title="基站" aria-expanded={menu === 'cell'} aria-controls="feature-popover" onClick={() => show('cell')}><RadioTower size={21} /></button>
    <button className="icon-button" aria-label="摇杆菜单" title="摇杆" aria-expanded={menu === 'joystick'} aria-controls="feature-popover" onClick={() => show('joystick')}><Joystick size={21} /></button>
    {menu && <div className="feature-popover" id="feature-popover" role="dialog" aria-label={menu === 'scope' ? '独立模拟' : menu === 'cell' ? '基站' : '摇杆'}>
      {menu === 'scope' && <>
        <button disabled={props.scopeLocked} onClick={props.toggleScope}><Power size={18} />{props.independent ? '禁用独立模拟' : '启用独立模拟'}</button>
        <button onClick={() => { setMenu(null); props.openScope(); }}><SlidersHorizontal size={18} />作用范围</button>
      </>}
      {menu === 'cell' && <>
        <button disabled={props.busy} onClick={() => { setMenu(null); props.toggleCells(); }}><Power size={18} />{props.cellsEnabled ? '停用基站模拟' : '启用基站模拟'}</button>
        <button onClick={() => { setMenu(null); props.openCells(); }}><List size={18} />基站与运营商</button>
        <p role="status">{props.cellNote}</p>
      </>}
      {menu === 'joystick' && <>
        <button disabled={!props.canOpen} onClick={() => props.controlJoystick(true)}><Joystick size={18} />打开摇杆</button>
        <button disabled={!props.canClose} onClick={() => props.controlJoystick(false)}><Power size={18} />关闭摇杆</button>
        <label className="field">最高速度（km/h）<input inputMode="decimal" value={props.speed} disabled={props.busy} onChange={event => props.setSpeed(event.target.value)} /></label>
        <p role="status">{props.note}</p>
      </>}
    </div>}
  </div>;
}
