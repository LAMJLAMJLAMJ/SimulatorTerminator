import { expect, test } from 'vitest';
import { starterMaterials, suggestMaterial } from '../src/materials/starter';

test('starter library fits the octave band set', () => {
  for (const m of starterMaterials) {
    expect(m.absorption).toHaveLength(8);
    expect(m.scattering).toHaveLength(8);
    expect([...m.absorption, ...m.scattering].every((x) => x >= 0 && x <= 1)).toBe(true);
  }
  expect(new Set(starterMaterials.map((m) => m.name)).size).toBe(starterMaterials.length);
  // The studio's library less the per-person row.
  expect(starterMaterials).toHaveLength(63);
  expect(starterMaterials.some((m) => /sabins/.test(m.name))).toBe(false);
});

test('suggestions come from layer names', () => {
  const s = (layer: string) => suggestMaterial(layer, starterMaterials);
  expect(s('Room::Floor carpet')).toBe('Carpet, heavy, on foam pad');
  expect(s('Room::Ceiling')).toBe('Plaster ceiling');
  expect(s('Audience')).toBe('Audience, occupied upholstered seats (per m²)');
  expect(s('Model Materials Treatment::Treatment')).toBe('Mineral wool, 50 mm, on wall');
  // Every suggestion names a material that is in the library.
  for (const layer of ['carpet', 'curtain', 'gypsum', 'ceiling', 'plaster', 'brick', 'wood', 'glass', 'treatment', 'seat', 'floor'])
    expect(s(layer), layer).not.toBe(null);
  // A wall behind the audience is not seating.
  expect(s('Audience wall')).toBe(null);
  expect(s('Untagged')).toBe(null);
  // Only names that are in the library.
  expect(suggestMaterial('Glass', [])).toBe(null);
});
