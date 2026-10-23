import { readFileSync } from 'node:fs';
import { describe, expect, test } from 'vitest';
import { mergeLibrary, parseMaterialCsv } from '../src/materials/csv';

const sample = readFileSync(new URL('fixtures/materials-sample.csv', import.meta.url), 'utf8');

describe('material library CSV', () => {
  test('reads six octave bands and extends them to 63 Hz and 8 kHz', () => {
    const { materials, warnings } = parseMaterialCsv(sample, 'sample.csv');
    expect(warnings).toEqual([]);
    expect(materials.map((m) => m.name)).toEqual(['Rockfon Sonar 20 mm, suspended 200 mm', 'Plaster on brick']);
    expect(materials[0].absorption).toEqual([0.45, 0.45, 0.8, 0.95, 0.95, 0.95, 0.9, 0.9]);
    expect(materials[0].scattering).toHaveLength(8);
    expect(materials[0].source).toBe(
      'Rockfon data sheet; imported from sample.csv; 63 Hz copied from 125 Hz; 8 kHz copied from 4 kHz',
    );
  });

  test('handles semicolons, decimal commas, percentages and no scattering', () => {
    const csv = 'Material;63 Hz;125 Hz;250 Hz;500 Hz;1 kHz;2 kHz;4 kHz;8 kHz;NRC\nCarpet;2;8;24;57;69;71;73;73;0,55\n';
    const { materials, warnings } = parseMaterialCsv(csv);
    expect(materials[0].absorption).toEqual([0.02, 0.08, 0.24, 0.57, 0.69, 0.71, 0.73, 0.73]);
    expect(materials[0].scattering).toEqual(new Array(8).fill(0.1));
    expect(warnings).toEqual([
      'Values above 1.5 found: read the whole file as percentages.',
      'No scattering columns: every material gets 0.1.',
    ]);
  });

  test('averages third-octave bands into octaves', () => {
    const thirds = [50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800, 1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000, 10000];
    const csv = `name,${thirds.join(',')}\nRamp,${thirds.map((_, i) => (i / 100).toFixed(2)).join(',')}\n`;
    const [m] = parseMaterialCsv(csv).materials;
    m.absorption.forEach((a, k) => expect(a).toBeCloseTo((3 * k + 1) / 100));
  });

  test('skips rows with missing bands and clips values above 1', () => {
    const csv = 'name,125,250,500,1000,2000,4000\nGap,0.1,,0.3,0.4,0.5,0.6\nISO 354,0.9,1.05,1.1,1,1,1\n';
    const { materials, warnings } = parseMaterialCsv(csv);
    expect(materials.map((m) => m.name)).toEqual(['ISO 354']);
    expect(materials[0].absorption.every((a) => a <= 1)).toBe(true);
    expect(warnings).toContain('Line 2, Gap: skipped, no value at 250 Hz.');
    expect(warnings).toContain('Line 3, ISO 354: values outside 0 to 1 were clipped.');
  });

  test('ignores columns that only look like frequencies', () => {
    const csv = 'name,thickness 200 mm,125,250,500,1000,2000,4000\nPanel,200,0.1,0.2,0.3,0.4,0.5,0.6\n';
    expect(parseMaterialCsv(csv).materials[0].absorption[1]).toBe(0.1);
  });

  test('rejects a file without bands', () => {
    expect(() => parseMaterialCsv('name,colour\nWall,white\n', 'x.csv')).toThrow(/no frequency columns/);
  });

  test('merging replaces same names and keeps the rest', () => {
    const m = (name: string, a: number) => ({ name, absorption: [a], scattering: [a], source: '' });
    expect(mergeLibrary([m('A', 0.1), m('B', 0.2)], [m('B', 0.5), m('C', 0.3)])).toEqual([m('A', 0.1), m('B', 0.5), m('C', 0.3)]);
  });
});
