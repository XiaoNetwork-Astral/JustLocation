import { placesKey, type Place } from './backup';

export type ScopeDraft = { mode: 'all' | 'apps'; packages: string[] };

export function readScopeDraft(): ScopeDraft | null {
  try {
    const value = JSON.parse(localStorage.getItem('justlocation.scope') || 'null');
    if (
      (value?.mode === 'all' || value?.mode === 'apps') &&
      Array.isArray(value.packages) &&
      value.packages.every((p: unknown) => typeof p === 'string' && p.trim())
    )
      return value;
  } catch {
    /* Use the backend's saved scope. */
  }
  return null;
}
export function readPlaces(): Place[] {
  try {
    const items: unknown = JSON.parse(localStorage.getItem(placesKey) || '[]');
    if (!Array.isArray(items)) return [];
    return items.filter(
      (p): p is Place =>
        typeof p?.id === 'string' &&
        typeof p?.name === 'string' &&
        typeof p?.position?.latitude === 'number' &&
        typeof p?.position?.longitude === 'number',
    );
  } catch {
    return [];
  }
}
