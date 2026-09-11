// @vitest-environment jsdom
import { afterEach, expect, it, vi } from 'vitest';
import { createBackup, importBackup, parseBackup } from './backup';

const position = { latitude: 31.2, longitude: 121.5, altitude: -3, accuracy: 8, speed: 2, bearing: 90 };
const places = [{ id: 'p', name: '家 🏠', position, pinned: true }];
const routes = [{ id: 'r', name: '散步', plan: { points: [position, { ...position, latitude: 31.3 }], speed: 1.5, repeat_count: 3, repeat_delay: 8 } }];
function seed() {
  localStorage.setItem('justlocation.places', JSON.stringify(places));
  localStorage.setItem('justlocation.routes', JSON.stringify(routes));
}
afterEach(() => { localStorage.clear(); vi.restoreAllMocks(); });

it('round trips Unicode, all position fields, pins and route playback settings', () => {
  seed();
  const backup = parseBackup(createBackup(localStorage));
  expect(backup.places).toEqual(places);
  expect(backup.routes).toEqual(routes);
  localStorage.clear();
  expect(importBackup(backup, localStorage)).toEqual({ places: 1, routes: 1 });
  expect(JSON.parse(localStorage.getItem('justlocation.places')!)).toEqual(places);
  expect(JSON.parse(localStorage.getItem('justlocation.routes')!)).toEqual(routes);
});

it('merges id collisions without replacing existing entries and skips repeated content', () => {
  seed();
  const backup = parseBackup(createBackup(localStorage));
  backup.places[0].name = '另一个地方';
  expect(importBackup(backup, localStorage)).toEqual({ places: 1, routes: 0 });
  expect(importBackup(backup, localStorage)).toEqual({ places: 0, routes: 0 });
  const saved = JSON.parse(localStorage.getItem('justlocation.places')!);
  expect(saved[0]).toEqual(places[0]);
  expect(saved[1].id).not.toBe('p');
});

it('rejects invalid files as a whole without touching the saved data', () => {
  seed();
  const good = createBackup(localStorage);
  for (const mutate of [
    (b: any) => b.version = 99,
    (b: any) => b.places[0].position.latitude = 91,
    (b: any) => b.places[0].position.accuracy = -1,
    (b: any) => b.routes[0].plan.repeat_count = 0,
    (b: any) => b.routes[0].plan.speed = '2',
    (b: any) => b.routes[0].plan.points[1] = b.routes[0].plan.points[0],
  ]) {
    const broken = JSON.parse(good); mutate(broken);
    expect(() => parseBackup(JSON.stringify(broken))).toThrow();
  }
  expect(createBackup(localStorage)).toBe(good);
  expect(() => parseBackup(' '.repeat(2 * 1024 * 1024 + 1))).toThrow(/2 MB/);
});

it('restores the first collection when the second storage write fails', () => {
  seed();
  const backup = parseBackup(createBackup(localStorage));
  backup.places[0].name = '新位置'; backup.routes[0].name = '新路线';
  const before = createBackup(localStorage);
  const original = Storage.prototype.setItem;
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(function (this: Storage, key, value) {
    if (key === 'justlocation.routes') throw new Error('quota');
    original.call(this, key, value);
  });
  expect(() => importBackup(backup, localStorage)).toThrow(/保存失败/);
  expect(createBackup(localStorage)).toBe(before);
});
