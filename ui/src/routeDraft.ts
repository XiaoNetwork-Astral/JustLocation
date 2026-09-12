import { parsePosition } from './coordinates';
import type { Position, RoutePlan } from './protocol';

export const draftKey = 'justlocation.route';

export type PointInput = { latitude: string; longitude: string; altitude: string };
export type RouteDraft = {
  points: PointInput[];
  speed: string;
  repeatCount: string;
  repeatDelay: string;
};
export const blankPoint = (): PointInput => ({ latitude: '', longitude: '', altitude: '0' });
export const inputPoint = (p: Position): PointInput => ({
  latitude: String(p.latitude),
  longitude: String(p.longitude),
  altitude: String(p.altitude),
});
export const planDraft = (plan: RoutePlan): RouteDraft => ({
  points: plan.points.map(inputPoint),
  speed: String(plan.speed * 3.6),
  repeatCount: String(plan.repeat_count ?? 1),
  repeatDelay: String(plan.repeat_delay ?? 0),
});

/* Return null when no valid saved route draft is available. */
export function readSavedDraft(): RouteDraft | null {
  try {
    const draft = JSON.parse(localStorage.getItem(draftKey) || 'null');
    if (
      typeof draft?.speed === 'string' &&
      Array.isArray(draft.points) &&
      draft.points.length >= 2 &&
      draft.points.length <= 128 &&
      draft.points.every(
        (p: PointInput) =>
          p &&
          ['latitude', 'longitude', 'altitude'].every(
            (key) => typeof p[key as keyof PointInput] === 'string',
          ),
      )
    )
      return {
        ...draft,
        repeatCount: typeof draft.repeatCount === 'string' ? draft.repeatCount : '1',
        repeatDelay: typeof draft.repeatDelay === 'string' ? draft.repeatDelay : '0',
      };
  } catch {
    /* Browser storage may be unavailable. */
  }
  return null;
}

export function readDraft(plan?: RoutePlan): RouteDraft {
  if (plan) return planDraft(plan);
  return (
    readSavedDraft() ?? {
      points: [blankPoint(), blankPoint()],
      speed: '5.4',
      repeatCount: '1',
      repeatDelay: '0',
    }
  );
}

export function parseDraft(draft: RouteDraft): RoutePlan {
  if (draft.points.length < 2 || draft.points.length > 128)
    throw new Error('路线须包含 2 至 128 个点');
  const points = draft.points.map((p, index) => {
    try {
      return parsePosition(p.latitude, p.longitude, p.altitude);
    } catch (e) {
      throw new Error(`点 ${index + 1}：${e instanceof Error ? e.message : String(e)}`);
    }
  });
  const speed = Number(draft.speed) / 3.6;
  if (!Number.isFinite(speed) || speed <= 0 || speed > 1000)
    throw new Error('速度须大于 0，且不超过 3600 km/h');
  const repeat_count = Number(draft.repeatCount);
  const repeat_delay = repeat_count === 1 ? 0 : Number(draft.repeatDelay);
  if (!Number.isInteger(repeat_count) || repeat_count < 1 || repeat_count > 10000)
    throw new Error('播放次数须为 1 至 10000 的整数');
  if (
    repeat_count > 1 &&
    (!draft.repeatDelay.trim() ||
      !Number.isFinite(repeat_delay) ||
      repeat_delay < 0 ||
      repeat_delay > 86400)
  )
    throw new Error('间隔须为 0 至 86400 秒');
  if (
    points.some(
      (p, i) =>
        i > 0 && p.latitude === points[i - 1].latitude && p.longitude === points[i - 1].longitude,
    )
  )
    throw new Error('相邻路线点不能相同');
  return { points, speed, ...(repeat_count > 1 ? { repeat_count, repeat_delay } : {}) };
}
