// Shared pieces of the CAD importers: 4x4 transforms, planar polygon
// triangulation, and the builder that turns world-space triangles into a
// RawScene. Importers only read and report; welding beyond exact duplicates,
// orientation and leak checks are the engine's job.
import earcut from 'earcut';
import type { RawGroup, RawPoint, RawScene } from './types';

export type Vec3 = [number, number, number];
/** Row-major 4x4. */
export type Mat4 = number[];

export const identity = (): Mat4 => [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];

export function mul4(a: Mat4, b: Mat4): Mat4 {
  const r = new Array<number>(16).fill(0);
  for (let i = 0; i < 4; i++)
    for (let j = 0; j < 4; j++) for (let k = 0; k < 4; k++) r[i * 4 + j] += a[i * 4 + k] * b[k * 4 + j];
  return r;
}

export function transform(m: Mat4, p: ArrayLike<number>): Vec3 {
  const w = m[12] * p[0] + m[13] * p[1] + m[14] * p[2] + m[15] || 1;
  return [
    (m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3]) / w,
    (m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7]) / w,
    (m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11]) / w,
  ];
}

/** True when the transform mirrors, which reverses triangle winding. */
export const mirrors = (m: Mat4) =>
  m[0] * (m[5] * m[10] - m[6] * m[9]) - m[1] * (m[4] * m[10] - m[6] * m[8]) + m[2] * (m[4] * m[9] - m[5] * m[8]) < 0;

export const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a: Vec3, b: Vec3): Vec3 => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

/**
 * Triangulates a planar polygon with holes by projecting onto the plane of
 * the normal's dominant axis. Returns 9 numbers per triangle, wound so the
 * triangles face along `normal`.
 */
export function triangulatePolygon(outer: Vec3[], holes: Vec3[][], normal: Vec3): number[] {
  const n = normal.map(Math.abs);
  const ax = n[0] > n[1] ? (n[0] > n[2] ? 0 : 2) : n[1] > n[2] ? 1 : 2;
  const [u, v] = ax === 0 ? [1, 2] : ax === 1 ? [2, 0] : [0, 1];
  const all = [...outer];
  const holeStarts: number[] = [];
  for (const h of holes) {
    holeStarts.push(all.length);
    all.push(...h);
  }
  const flat: number[] = [];
  for (const p of all) flat.push(p[u], p[v]);
  const idx = earcut(flat, holeStarts.length ? holeStarts : undefined, 2);
  const out: number[] = [];
  for (let i = 0; i < idx.length; i += 3) {
    const a = all[idx[i]];
    let b = all[idx[i + 1]];
    let c = all[idx[i + 2]];
    if (dot(cross(sub(b, a), sub(c, a)), normal) < 0) [b, c] = [c, b];
    out.push(...a, ...b, ...c);
  }
  return out;
}

/**
 * Whether points on this layer (or in this group) are sources or receivers, from its name:
 * "Sources", "Speakers", "Receivers", "Listeners", "Seats", "Mics".
 */
export function pointRole(layerPath: string): RawPoint['role'] | null {
  if (/source|speaker|talker|loudspeaker/i.test(layerPath)) return 'source';
  if (/receiver|listener|seat|mic(rophone)?s?\b|position/i.test(layerPath)) return 'receiver';
  return null;
}

/** Counts skipped objects per (object, reason) so a model with thousands of hidden faces stays readable. */
export class SkipList {
  private counts = new Map<string, { object: string; reason: string; n: number }>();

  add(object: string, reason: string, n = 1) {
    const k = `${object}\u0000${reason}`;
    const e = this.counts.get(k);
    if (e) e.n += n;
    else this.counts.set(k, { object, reason, n });
  }

  list(): RawScene['skipped'] {
    return [...this.counts.values()].map(({ object, reason, n }) => ({
      object,
      reason: n > 1 ? `${reason} (${n})` : reason,
    }));
  }
}

/** Growable float64 buffer for world-space triangle corners. */
class F64 {
  data = new Float64Array(1 << 12);
  length = 0;

  push3(x: number, y: number, z: number) {
    if (this.length + 3 > this.data.length) {
      const next = new Float64Array(this.data.length * 2);
      next.set(this.data);
      this.data = next;
    }
    this.data[this.length++] = x;
    this.data[this.length++] = y;
    this.data[this.length++] = z;
  }
}

const CURVED = 1;

/**
 * Collects world-space triangles (source units, Z up) per layer and builds a
 * RawScene: one group per layer path, vertices relative to the bounding-box
 * centre, exact duplicate vertices shared.
 */
export class SceneBuilder {
  private points: { id: string; role: RawPoint['role']; p: Vec3 }[] = [];
  private corners = new F64();
  private triGroup: number[] = [];
  private triFlags: number[] = [];
  private groups = new Map<string, { index: number; materials: Set<string> }>();

  pointCount(role: RawPoint['role']) {
    return this.points.filter((q) => q.role === role).length;
  }

  /** A source or receiver point, world space, source units. Repeated names get a number. */
  addPoint(name: string, role: RawPoint['role'], p: Vec3) {
    let id = name;
    for (let k = 2; this.points.some((q) => q.role === role && q.id === id); k++) id = `${name} ${k}`;
    this.points.push({ id, role, p });
  }

  /** `tris` holds 9 numbers per triangle. */
  add(layerPath: string, material: string | null | undefined, curved: boolean, tris: ArrayLike<number>) {
    let g = this.groups.get(layerPath);
    if (!g) {
      g = { index: this.groups.size, materials: new Set() };
      this.groups.set(layerPath, g);
    }
    if (material) g.materials.add(material);
    for (let i = 0; i + 8 < tris.length; i += 9) {
      for (let k = 0; k < 9; k += 3) this.corners.push3(tris[i + k], tris[i + k + 1], tris[i + k + 2]);
      this.triGroup.push(g.index);
      this.triFlags.push(curved ? CURVED : 0);
    }
  }

  get triangleCount() {
    return this.triGroup.length;
  }

  finish(head: { importer: string; unitScaleToMetres: number }, warnings: string[], skipped: SkipList): RawScene {
    if (this.groups.size > 0xffff) throw new Error(`Too many layers (${this.groups.size}); the limit is 65535`);
    const src = this.corners.data;
    const nCorners = this.corners.length / 3;

    const lo = [Infinity, Infinity, Infinity];
    const hi = [-Infinity, -Infinity, -Infinity];
    for (let i = 0; i < nCorners * 3; i++) {
      const a = i % 3;
      if (src[i] < lo[a]) lo[a] = src[i];
      if (src[i] > hi[a]) hi[a] = src[i];
    }
    // Centre the model so float32 keeps sub-millimetre precision far from the CAD origin.
    const c = nCorners ? lo.map((l, a) => Math.round((l + hi[a]) / 2)) : [0, 0, 0];

    // Share exactly equal float32 corners through an open-addressing hash.
    const rel = new Float32Array(nCorners * 3);
    for (let i = 0; i < rel.length; i++) rel[i] = src[i] - c[i % 3] + 0; // + 0 turns -0 into 0 for hashing
    const bits = new Uint32Array(rel.buffer);
    let size = 1;
    while (size < nCorners * 2) size <<= 1;
    const table = new Int32Array(size).fill(-1);
    const remap = new Uint32Array(nCorners);
    const vertices: number[] = [];
    for (let i = 0; i < nCorners; i++) {
      const x = bits[i * 3], y = bits[i * 3 + 1], z = bits[i * 3 + 2];
      let h = (Math.imul(x, 0x9e3779b1) ^ Math.imul(y, 0x85ebca77) ^ Math.imul(z, 0xc2b2ae3d)) & (size - 1);
      for (;;) {
        const v = table[h];
        if (v < 0) {
          const id = vertices.length / 3;
          table[h] = id;
          remap[i] = id;
          vertices.push(rel[i * 3], rel[i * 3 + 1], rel[i * 3 + 2]);
          break;
        }
        if (vertices[v * 3] === rel[i * 3] && vertices[v * 3 + 1] === rel[i * 3 + 1] && vertices[v * 3 + 2] === rel[i * 3 + 2]) {
          remap[i] = v;
          break;
        }
        h = (h + 1) & (size - 1);
      }
    }

    // Drop triangles that collapse once duplicates are shared, and number the
    // vertices that are left in order of first use.
    const triangles: number[] = [];
    const triGroup: number[] = [];
    const triFlags: number[] = [];
    const used = new Int32Array(vertices.length / 3).fill(-1);
    const kept: number[] = [];
    let collapsed = 0;
    for (let t = 0; t < this.triGroup.length; t++) {
      const tri = [remap[t * 3], remap[t * 3 + 1], remap[t * 3 + 2]];
      if (tri[0] === tri[1] || tri[1] === tri[2] || tri[0] === tri[2]) {
        collapsed++;
        continue;
      }
      for (const v of tri) {
        if (used[v] < 0) {
          used[v] = kept.length / 3;
          kept.push(vertices[v * 3], vertices[v * 3 + 1], vertices[v * 3 + 2]);
        }
        triangles.push(used[v]);
      }
      triGroup.push(this.triGroup[t]);
      triFlags.push(this.triFlags[t]);
    }
    if (collapsed) warnings.push(`Dropped ${collapsed} triangle${collapsed > 1 ? 's' : ''} with repeated corners`);

    const groups: RawGroup[] = [...this.groups].map(([layerPath, g]) => ({
      key: layerPath,
      layerPath,
      material: g.materials.size === 1 ? [...g.materials][0] : undefined,
      label: layerPath.split('::').pop() || layerPath,
    }));

    const s = head.unitScaleToMetres;
    return {
      importer: head.importer,
      unitScaleToMetres: s,
      upAxis: 'z',
      origin: [c[0] * s, c[1] * s, c[2] * s],
      groups,
      vertices: new Float32Array(kept),
      triangles: new Uint32Array(triangles),
      triGroup: new Uint16Array(triGroup),
      triFlags: new Uint8Array(triFlags),
      points: this.points.map(({ id, role, p }) => ({ id, role, position: [p[0] - c[0], p[1] - c[1], p[2] - c[2]] })),
      warnings,
      skipped: skipped.list(),
    };
  }
}
