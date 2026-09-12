// Independent test receiver: decode transmitted LNAV words, propagate broadcast orbits and
// solve ECEF position/clock by least squares. It never imports the production orbit model.
const C = 299792458,
  MU = 3.986005e14,
  OMEGA = 7.2921151467e-5;
const parityMasks = [0xbb1f3480, 0x5d8f9a40, 0xaec7cd00, 0x5763e680, 0x6bb1f340, 0x8b7a89c0];
const parity = (value) => {
  let result = 0;
  for (let i = 0; i < 32; i++) result ^= (value >>> i) & 1;
  return result;
};
export function decode(hex) {
  const bytes = Buffer.from(hex, 'hex');
  if (bytes.length !== 40) throw Error('LNAV length');
  const words = [];
  let previous = 0;
  for (let n = 0; n < 10; n++) {
    const transmitted = bytes.readUInt32BE(n * 4);
    if (transmitted >>> 30) throw Error('nonzero word padding');
    const source = transmitted ^ (previous & 1 ? 0x3fffffc0 : 0);
    const augmented = source | ((previous & 3) << 30);
    const check = parityMasks.reduce((bits, mask) => (bits << 1) | parity(augmented & mask), 0);
    if (check !== (transmitted & 63)) throw Error(`parity word ${n + 1}`);
    if ((n === 1 || n === 9) && transmitted & 3) throw Error('word terminator');
    words.push(source >>> 6);
    previous = transmitted;
  }
  const bits = (start, length) => {
    let value = 0;
    for (let n = start; n < start + length; n++) {
      if (n % 30 >= 24) throw Error('decoder field overlaps parity');
      value = value * 2 + ((words[Math.floor(n / 30)] >>> (23 - (n % 30))) & 1);
    }
    return value;
  };
  const signed = (start, size) => {
    const v = bits(start, size);
    return v >= 2 ** (size - 1) ? v - 2 ** size : v;
  };
  const split = (start, sign = true) => {
    const v = bits(start, 8) * 2 ** 24 + bits(start + 14, 24);
    return sign && v >= 2 ** 31 ? v - 2 ** 32 : v;
  };
  if (bits(0, 8) !== 0x8b) throw Error('preamble');
  const subframe = bits(49, 3),
    tow = bits(30, 17) * 6;
  if (subframe < 1 || subframe > 5 || tow >= 604800) throw Error('header');
  return { bits, signed, split, subframe, tow };
}
export function ephemeris(frames) {
  const [a, b, c] = [1, 2, 3].map((n) => frames.get(n));
  if (!a || !b || !c) return null;
  const iodc = a.bits(82, 2) * 256 + a.bits(210, 8);
  if ((iodc & 255) !== b.bits(60, 8) || b.bits(60, 8) !== c.bits(270, 8)) return null;
  const angle = 2 ** -31 * Math.PI;
  return {
    week: a.bits(60, 10),
    iodc,
    health: a.bits(76, 6),
    toc: a.bits(218, 16) * 16,
    af0: a.signed(270, 22) * 2 ** -31,
    af1: a.signed(248, 16) * 2 ** -43,
    af2: a.signed(240, 8) * 2 ** -55,
    tgd: a.signed(196, 8) * 2 ** -31,
    toe: b.bits(270, 16) * 16,
    m0: b.split(106) * angle,
    e: b.split(166, false) * 2 ** -33,
    sqrtA: b.split(226, false) * 2 ** -19,
    deltaN: b.signed(90, 16) * 2 ** -43 * Math.PI,
    crs: b.signed(68, 16) * 2 ** -5,
    cuc: b.signed(150, 16) * 2 ** -29,
    cus: b.signed(210, 16) * 2 ** -29,
    node: c.split(76) * angle,
    i: c.split(136) * angle,
    argument: c.split(196) * angle,
    nodeRate: c.signed(240, 24) * 2 ** -43 * Math.PI,
    iRate: c.signed(278, 14) * 2 ** -43 * Math.PI,
    cic: c.signed(60, 16) * 2 ** -29,
    cis: c.signed(120, 16) * 2 ** -29,
    crc: c.signed(180, 16) * 2 ** -5,
  };
}
function orbit(e, t) {
  let dt = t - e.toe;
  if (dt > 302400) dt -= 604800;
  if (dt < -302400) dt += 604800;
  const a = e.sqrtA ** 2,
    m = e.m0 + (Math.sqrt(MU / a ** 3) + e.deltaN) * dt;
  let eccentric = m;
  for (let k = 0; k < 12; k++) eccentric = m + e.e * Math.sin(eccentric);
  const phi =
    Math.atan2(Math.sqrt(1 - e.e ** 2) * Math.sin(eccentric), Math.cos(eccentric) - e.e) +
    e.argument;
  const u = phi + e.cus * Math.sin(2 * phi) + e.cuc * Math.cos(2 * phi);
  const r =
    a * (1 - e.e * Math.cos(eccentric)) + e.crs * Math.sin(2 * phi) + e.crc * Math.cos(2 * phi);
  const inc = e.i + e.iRate * dt + e.cis * Math.sin(2 * phi) + e.cic * Math.cos(2 * phi);
  const node = e.node + (e.nodeRate - OMEGA) * dt - OMEGA * e.toe;
  const x = r * Math.cos(u),
    y = r * Math.sin(u);
  return [
    x * Math.cos(node) - y * Math.cos(inc) * Math.sin(node),
    x * Math.sin(node) + y * Math.cos(inc) * Math.cos(node),
    y * Math.sin(inc),
  ];
}
export function ecef(lat, lon, alt) {
  const p = (lat * Math.PI) / 180,
    l = (lon * Math.PI) / 180,
    e2 = 6.6943799901413165e-3;
  const n = 6378137 / Math.sqrt(1 - e2 * Math.sin(p) ** 2);
  return [
    (n + alt) * Math.cos(p) * Math.cos(l),
    (n + alt) * Math.cos(p) * Math.sin(l),
    (n * (1 - e2) + alt) * Math.sin(p),
  ];
}
const deltaTime = (a, b) => {
  let t = a - b;
  if (t < -302400) t += 604800;
  if (t > 302400) t -= 604800;
  return t;
};
export function rangeAt(e, rx, position) {
  let flight = 0.075,
    range = 0;
  for (let n = 0; n < 6; n++) {
    const p = orbit(e, rx - flight),
      a = OMEGA * flight;
    range = Math.hypot(
      Math.cos(a) * p[0] + Math.sin(a) * p[1] - position[0],
      -Math.sin(a) * p[0] + Math.cos(a) * p[1] - position[1],
      p[2] - position[2],
    );
    flight = range / C;
  }
  return range;
}
function linear(matrix, rhs) {
  const a = matrix.map((row, i) => [...row, rhs[i]]);
  for (let c = 0; c < 4; c++) {
    let pivot = c;
    for (let r = c + 1; r < 4; r++) if (Math.abs(a[r][c]) > Math.abs(a[pivot][c])) pivot = r;
    [a[c], a[pivot]] = [a[pivot], a[c]];
    if (Math.abs(a[c][c]) < 1e-12) throw Error('singular satellite geometry');
    const d = a[c][c];
    for (let j = c; j <= 4; j++) a[c][j] /= d;
    for (let r = 0; r < 4; r++)
      if (r !== c) {
        const scale = a[r][c];
        for (let j = c; j <= 4; j++) a[r][j] -= scale * a[c][j];
      }
  }
  return a.map((row) => row[4]);
}
export function solve(measurements) {
  if (measurements.length < 4) throw Error('need four decoded satellites');
  const position = [0, 0, 0, 0];
  for (let iteration = 0; iteration < 12; iteration++) {
    const normal = Array.from({ length: 4 }, () => [0, 0, 0, 0]),
      rhs = [0, 0, 0, 0];
    for (const { e, rx, tx } of measurements) {
      const observed = deltaTime(rx, tx) * C;
      if (observed < 1e7 || observed > 4e7) throw Error('invalid pseudorange');
      const p = orbit(e, tx),
        a = (OMEGA * observed) / C;
      const satellite = [
        Math.cos(a) * p[0] + Math.sin(a) * p[1],
        -Math.sin(a) * p[0] + Math.cos(a) * p[1],
        p[2],
      ];
      const d = position.slice(0, 3).map((x, i) => x - satellite[i]),
        r = Math.hypot(...d);
      const h = [...d.map((x) => x / r), 1],
        residual = observed - r - position[3];
      for (let i = 0; i < 4; i++) {
        rhs[i] += h[i] * residual;
        for (let j = 0; j < 4; j++) normal[i][j] += h[i] * h[j];
      }
    }
    const dx = linear(normal, rhs);
    dx.forEach((v, i) => (position[i] += v));
    if (Math.hypot(...dx) < 0.0001) return position;
  }
  throw Error('position did not converge');
}
