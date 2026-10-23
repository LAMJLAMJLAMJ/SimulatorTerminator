import { expect, test } from 'vitest';
import { starterMaterials, suggestMaterial } from '../src/materials/starter';

test('starter library fits the octave band set', () => {
  for (const m of starterMaterials) {
    expect(m.absorption).toHaveLength(8);
    expect(m.scattering).toHaveLength(8);
    expect([...m.absorption, ...m.scattering].every((x) => x >= 0 && x <= 1)).toBe(true);
  }
  expect(new Set(starterMaterials.map((m) => m.name)).size).toBe(starterMaterials.length);
});

test('suggestions come from layer names', () => {
  const s = (layer: string) => suggestMaterial(layer, starterMaterials);
  expect(s('Room::Floor carpet')).toBe('Carpet, heavy');
  expect(s('Room::Ceiling')).toBe('Plaster, smooth');
  expect(s('Audience')).toBe('Seating, upholstered, occupied');
  // A wall behind the audience is not seating.
  expect(s('Audience wall')).toBe(null);
  expect(s('Untagged')).toBe(null);
  // Only names that are in the library.
  expect(suggestMaterial('Glass', [])).toBe(null);
});
