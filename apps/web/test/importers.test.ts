import { readFileSync } from 'node:fs';
import rhino3dm from 'rhino3dm';
import { describe, expect, test } from 'vitest';
import { SceneBuilder, SkipList, triangulatePolygon, type Vec3 } from '../src/importers/geometry';
import { importRhino } from '../src/importers/rhino';
import { importSkp } from '../src/importers/sketchup';
import type { RawScene } from '../src/importers/types';

// The fixtures are a 20 x 12 x 8 m hall built by the scripts next to them.
const fixture = (name: string) => {
  const b = readFileSync(new URL(`fixtures/${name}`, import.meta.url));
  return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;
};

/** Bounding box in metres, world coordinates. */
function boundsMetres(s: RawScene) {
  const lo = [Infinity, Infinity, Infinity];
  const hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < s.vertices.length; i++) {
    const a = i % 3;
    const x = s.vertices[i] * s.unitScaleToMetres + s.origin[a];
    lo[a] = Math.min(lo[a], x);
    hi[a] = Math.max(hi[a], x);
  }
  return { lo, hi };
}

function expectConsistent(s: RawScene) {
  const nTri = s.triangles.length / 3;
  expect(nTri).toBeGreaterThan(0);
  expect(s.triGroup.length).toBe(nTri);
  expect(s.triFlags?.length).toBe(nTri);
  const nVert = s.vertices.length / 3;
  expect(Math.max(...s.triangles)).toBeLessThan(nVert);
  expect(Math.max(...s.triGroup)).toBeLessThan(s.groups.length);
  expect(new Set(s.groups.map((g) => g.key)).size).toBe(s.groups.length);
  // Every vertex is used, and no two are the same point.
  expect(new Set(s.triangles).size).toBe(nVert);
  const keys = new Set<string>();
  for (let i = 0; i < nVert; i++) keys.add(`${s.vertices[i * 3]},${s.vertices[i * 3 + 1]},${s.vertices[i * 3 + 2]}`);
  expect(keys.size).toBe(nVert);
}

const groupsOf = (s: RawScene) => s.groups.map((g) => g.key).sort();
const trianglesIn = (s: RawScene, key: string) => {
  const gi = s.groups.findIndex((g) => g.key === key);
  return s.triGroup.filter((g) => g === gi).length;
};

describe('SketchUp importer', () => {
  const s = importSkp(fixture('hall.skp'));

  test('keeps inches and centres the model', () => {
    expect(s.importer).toBe('openskp');
    expect(s.unitScaleToMetres).toBe(0.0254);
    expect(s.upAxis).toBe('z');
    const { lo, hi } = boundsMetres(s);
    expect(lo.map((x) => +x.toFixed(3))).toEqual([0, 0, 0]);
    expect(hi.map((x) => +x.toFixed(3))).toEqual([20, 12, 8]);
    expectConsistent(s);
  });

  test('groups by tag, with the paint as information only', () => {
    expect(groupsOf(s)).toEqual(
      ['Absorber 50mm', 'Audience', 'Floor', 'Floor carpet', 'Glass', 'Plaster', 'Reflectors', 'Stage', 'Wood panel'].sort(),
    );
    expect(s.groups.find((g) => g.key === 'Audience')?.material).toBe('Seat_Upholstered');
    // Twelve seat components of the same definition.
    expect(trianglesIn(s, 'Audience') % 12).toBe(0);
  });

  test('flags faces with soft edges as curved', () => {
    expect(s.triFlags!.some((f) => f & 1)).toBe(true);
    expect(s.triFlags!.some((f) => !(f & 1))).toBe(true);
  });

  test('skips hidden tags and instances, and can keep them', () => {
    expect(s.skipped).toEqual([
      { object: 'Sofa', reason: 'hidden tag "x-Furniture"' },
      { object: 'Sofa hidden', reason: 'hidden instance' },
    ]);
    const all = importSkp(fixture('hall.skp'), { skipHidden: false });
    expect(all.skipped).toEqual([]);
    expect(groupsOf(all)).toContain('x-Furniture');
    expect(all.triangles.length).toBeGreaterThan(s.triangles.length);
  });
});

describe('Rhino importer', async () => {
  const rhino = await rhino3dm();
  const s = importRhino(rhino, fixture('hall.3dm'));

  test('keeps millimetres and centres the model', () => {
    expect(s.importer).toBe('rhino3dm');
    expect(s.unitScaleToMetres).toBe(0.001);
    expect(s.origin).toEqual([10, 6, 4]);
    const { lo, hi } = boundsMetres(s);
    expect(lo.map((x) => +x.toFixed(3))).toEqual([0, 0, 0]);
    expect(hi.map((x) => +x.toFixed(3))).toEqual([20, 12, 8]);
    expectConsistent(s);
  });

  test('groups by full layer path', () => {
    expect(groupsOf(s)).toEqual(
      ['Audience', 'Balcony', 'Reflectors', 'Room::Ceiling', 'Room::Floor', 'Room::Walls', 'Room::Wood panel'].sort(),
    );
    expect(s.groups.find((g) => g.key === 'Room::Wood panel')?.label).toBe('Wood panel');
    // A block placed twelve times.
    expect(trianglesIn(s, 'Audience') % 12).toBe(0);
  });

  test('rebuilds planar faces and uses render meshes for curved ones', () => {
    expect(s.triFlags!.some((f) => f & 1)).toBe(true);
    expect(s.skipped).toContainEqual({ object: 'Column 1', reason: 'curved face saved without a render mesh' });
  });

  test('reports hidden layers and objects that are not surfaces', () => {
    expect(s.skipped).toContainEqual({ object: 'Sofa', reason: 'hidden layer "Hidden stuff"' });
    expect(s.warnings).toEqual(['Ignored objects that are not surfaces: 1 TextDot, 1 Curve']);
  });
});

describe('geometry', () => {
  test('triangulates a polygon with a hole facing the given normal', () => {
    const sq = (r: number): Vec3[] => [[-r, -r, 0], [r, -r, 0], [r, r, 0], [-r, r, 0]];
    const tris = triangulatePolygon(sq(2), [sq(1).reverse()], [0, 0, -1]);
    let area = 0;
    for (let i = 0; i < tris.length; i += 9) {
      const [ax, ay, , bx, by, , cx, cy] = tris.slice(i, i + 8);
      area += ((bx - ax) * (cy - ay) - (cx - ax) * (by - ay)) / 2;
    }
    // Facing -Z means clockwise seen from above: negative signed area.
    expect(area).toBeCloseTo(-12);
  });

  test('shares duplicate corners and drops collapsed triangles', () => {
    const b = new SceneBuilder();
    b.add('A', 'm', false, [0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 1, 1, 0, 0, 1, 0]);
    b.add('B', null, true, [5, 5, 5, 5, 5, 5, 6, 5, 5]);
    const warnings: string[] = [];
    const s = b.finish({ importer: 't', unitScaleToMetres: 1 }, warnings, new SkipList());
    expect(s.vertices.length / 3).toBe(4);
    expect(s.triangles.length / 3).toBe(2);
    expect(warnings).toEqual(['Dropped 1 triangle with repeated corners']);
    expect(s.groups.map((g) => g.material)).toEqual(['m', undefined]);
  });

  test('counts repeated skips', () => {
    const k = new SkipList();
    k.add('Chair', 'hidden face');
    k.add('Chair', 'hidden face');
    k.add('Desk', 'hidden face');
    expect(k.list()).toEqual([
      { object: 'Chair', reason: 'hidden face (2)' },
      { object: 'Desk', reason: 'hidden face' },
    ]);
  });
});
