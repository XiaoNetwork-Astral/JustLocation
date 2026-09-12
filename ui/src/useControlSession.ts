import { useEffect, useRef, useState } from 'react';
import type { Client } from './control';
import type { Command, Position, Scope, State } from './protocol';
import type { Joystick } from './joystick';
import { readScopeDraft, type ScopeDraft } from './panelStorage';

export function useControlSession(client: Client, joystick: Joystick, cellsOpen: boolean) {
  const [state, setState] = useState<State | null>(null);
  const [position, setPosition] = useState<Position | null>(null);
  const [name, setName] = useState('');
  const [scopeDraft, setScopeDraft] = useState<ScopeDraft>(
    () => readScopeDraft() || { mode: 'apps', packages: [] },
  );
  const scope: Scope =
    scopeDraft.mode === 'all' ? { mode: 'all' } : { mode: 'apps', packages: scopeDraft.packages };
  const [busy, setBusyState] = useState(false);
  const [error, setError] = useState('');
  const [joystickSpeed, setJoystickSpeed] = useState(
    () => localStorage.getItem('justlocation.joystick.speed') || '5.4',
  );

  const [joystickOpen, setJoystickOpen] = useState(false);

  const [joystickNote, setJoystickNote] = useState('拖到屏幕边缘可收起，点边缘把手展开。');
  const initialized = useRef(false);
  // Read current values without restarting the polling timer on each render.
  const stateRef = useRef<State | null>(null);
  const cellsOpenRef = useRef(false);
  const busyRef = useRef(false);
  stateRef.current = state;
  cellsOpenRef.current = cellsOpen;
  // A foreground operation invalidates any older status response.
  const revision = useRef(0);
  function setBusy(value: boolean) {
    busyRef.current = value;
    if (value) revision.current++;
    setBusyState(value);
  }
  const locked = busy || !!state?.requested_active;

  async function refresh() {
    if (busyRef.current) return;
    setBusy(true);
    const requestRevision = revision.current;
    try {
      const next = await client({ op: 'status' });
      if (revision.current !== requestRevision) return;
      setState(next);
      setError('');
      if (!initialized.current && next.config) {
        setPosition(next.config.position);
        if (next.requested_active || !readScopeDraft()) {
          const savedScope = next.config.scope;
          setScopeDraft((previous) => ({
            mode: savedScope.mode,
            packages: savedScope.mode === 'apps' ? savedScope.packages : previous.packages,
          }));
        }
      }
      initialized.current = true;
      // Read the actual service state when refreshing the panel.
      // A failed service lookup must not discard a successful backend refresh.
      if (next.requested_active) {
        let running: boolean | null = null;
        try {
          running = await joystick.read();
        } catch {
          running = null;
        }
        if (revision.current !== requestRevision) return;
        if (running !== null) {
          setJoystickOpen(running);
          if (running) setJoystickNote('摇杆正在运行。');
        }
      } else {
        setJoystickOpen(false);
      }
    } catch (e) {
      if (revision.current === requestRevision) {
        setState(null);
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      if (revision.current === requestRevision) setBusy(false);
    }
  }
  useEffect(() => {
    void refresh();
    return () => {
      revision.current++;
      busyRef.current = false;
    };
  }, [client]);
  // Poll idle sessions for channel readiness; slow down when the backend is unavailable.
  useEffect(() => {
    let cancelled = false;
    let timer = 0;
    const tick = async () => {
      if (!cancelled && !busyRef.current) {
        const requestRevision = revision.current;
        try {
          const next = await client({ op: 'status' });
          if (!cancelled && revision.current === requestRevision) {
            setState(next);
            // Only active simulations may replace the local position draft.
            if (next.requested_active && next.config) setPosition(next.config.position);
            setError('');
          }
        } catch (e) {
          if (!cancelled && revision.current === requestRevision)
            setError(e instanceof Error ? e.message : String(e));
        }
      }
      if (cancelled) return;
      const active = !!stateRef.current?.requested_active || cellsOpenRef.current;
      const reachable = !!stateRef.current;
      const wanted = active ? 2000 : reachable ? 5000 : 15000;
      timer = window.setTimeout(tick, busyRef.current ? 2000 : wanted);
    };
    timer = window.setTimeout(tick, 5000);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [client]);
  function editScope(next: ScopeDraft) {
    setScopeDraft(next);
    try {
      localStorage.setItem('justlocation.scope', JSON.stringify(next));
    } catch {
      setError('应用选择保存失败，关闭面板后可能丢失');
    }
  }
  async function routeCommand(command: Command) {
    if (busyRef.current || !state) return;
    setBusy(true);
    setError('');
    try {
      const next = await client(command);
      setState(next);
      if (next.config) {
        setPosition(next.config.position);
        setName('');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  async function environmentCommand(command: Command) {
    if (busyRef.current || !state) throw new Error('后台尚未就绪，请稍后重试');
    setBusy(true);
    try {
      setState(await client(command));
    } finally {
      setBusy(false);
    }
  }
  async function stopSimulation() {
    // Also close the overlay when stopping the simulation.
    let closed = true;
    try {
      await joystick.close();
    } catch {
      closed = !joystickOpen;
    }
    if (closed) setJoystickOpen(false);
    setJoystickNote(
      closed
        ? '位置模拟已停止，摇杆同时关闭。'
        : '位置模拟已停止，但摇杆没能关闭，请再点一次摇杆开关。',
    );
    if (!closed) setError('摇杆没有关闭成功，请重试。');
  }
  async function toggle() {
    if (!state || busyRef.current) return;
    setBusy(true);
    setError('');
    try {
      const next = await client(
        state.requested_active
          ? { op: 'stop' }
          : { op: 'start', config: { position: position!, scope } },
      );
      setState(next);
      if (state.requested_active) await stopSimulation();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }
  async function controlJoystick(open: boolean) {
    if (busyRef.current || !state) return;
    setBusy(true);
    setError('');
    let started = false;
    try {
      if (open) {
        const kmh = Number(joystickSpeed);
        if (!Number.isFinite(kmh) || kmh <= 0 || kmh > 3600)
          throw new Error('速度须大于 0、不超过 3600 km/h');
        await joystick.check();
        if (!state.requested_active) {
          const next = await client({ op: 'start', config: { position: position!, scope } });
          setState(next);
          started = true;
        }
        await joystick.open(kmh);
        localStorage.setItem('justlocation.joystick.speed', joystickSpeed);
        setJoystickOpen(true);
        setJoystickNote('摇杆已打开，拖动即可移动位置。');
      } else {
        await joystick.close();
        setJoystickOpen(false);
        setJoystickNote('摇杆已关闭，位置模拟会保持当前状态。');
      }
    } catch (e) {
      let message = e instanceof Error ? e.message : String(e);
      if (started) {
        try {
          setState(await client({ op: 'stop' }));
        } catch {
          message += '；停止模拟失败，请手动停止';
        }
      }
      setError(message);
    } finally {
      setBusy(false);
    }
  }
  async function select(p: Position, label: string) {
    if (busyRef.current) return false;
    if (state?.requested_active) {
      setBusy(true);
      try {
        setState(await client({ op: 'update', position: p }));
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        return false;
      } finally {
        setBusy(false);
      }
    }
    setPosition(p);
    setName(label);
    setError('');
    return true;
  }

  return {
    state,
    position,
    name,
    setName,
    scopeDraft,
    scope,
    busy,
    error,
    setError,
    locked,
    joystickSpeed,
    setJoystickSpeed,
    joystickOpen,
    joystickNote,
    refresh,
    editScope,
    routeCommand,
    environmentCommand,
    toggle,
    controlJoystick,
    select,
  };
}
