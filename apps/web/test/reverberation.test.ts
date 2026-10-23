import { describe, expect, test } from 'vitest';
import { summary } from '../src/acoustics/reverberation';
import type { ReverberationReport } from '../src/engine/protocol/ReverberationReport';

const report: ReverberationReport = {
  bands_hz: [500, 1000],
  air: { temperature_c: 20, humidity_pct: 50, pressure_kpa: 101.325 },
  volume_m3: 1200,
  volume_from: 'closed mesh',
  area_m2: 760,
  sabine_s: [1.4, 1.3],
  eyring_s: [1.3, 1.2],
  absorption_area_m2: [140, 150],
  air_absorption_m2: [2, 5],
  layers: [
    { key: 'Walls', material: 'Plaster', area_m2: 400, absorption_area_m2: [8, 12] },
    { key: 'Seats', material: 'Upholstered', area_m2: 200, absorption_area_m2: [120, 120] },
    { key: 'Glass', material: null, area_m2: 160, absorption_area_m2: [0, 0] },
  ],
  warnings: ['160 m² on 1 layer without a material counts as not absorbing: Glass'],
};

describe('reverberation summary', () => {
  test('lists the main absorbers at 1 kHz and the warnings', () => {
    expect(summary(report).split('\n')).toEqual([
      'Volume 1200 m³ (closed mesh), surface facing the room 760 m².',
      'Most absorption at 1 kHz:',
      '   80 %  Seats (Upholstered, 200 m²)',
      '    8 %  Walls (Plaster, 400 m²)',
      '160 m² on 1 layer without a material counts as not absorbing: Glass',
    ]);
  });
});
