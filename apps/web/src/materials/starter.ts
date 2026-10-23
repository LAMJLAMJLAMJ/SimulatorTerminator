// A starter library so a model can be mapped end to end before a real material
// database is chosen. Octave bands 63 Hz to 8 kHz.
import type { Material } from '../engine/protocol/Material';

const SOURCE =
  'Typical textbook absorption at 125 Hz to 4 kHz, with 63 Hz and 8 kHz copied from the neighbouring band. Scattering is a placeholder. Replace with measured data.';

// Absorption at 125, 250, 500, 1k, 2k, 4k Hz.
const TEXTBOOK: [string, number[]][] = [
  ['Carpet, heavy', [0.08, 0.24, 0.57, 0.69, 0.71, 0.73]],
  ['Plaster, smooth', [0.013, 0.015, 0.02, 0.03, 0.04, 0.05]],
  ['Wood panel on battens', [0.42, 0.21, 0.1, 0.08, 0.06, 0.06]],
  ['Glass, 6 mm', [0.18, 0.06, 0.04, 0.03, 0.02, 0.02]],
  ['Mineral wool, 50 mm', [0.15, 0.6, 0.88, 0.94, 0.92, 0.9]],
  ['Seating, upholstered, occupied', [0.6, 0.74, 0.88, 0.96, 0.93, 0.85]],
  ['Concrete, painted', [0.01, 0.01, 0.02, 0.02, 0.02, 0.02]],
  ['Floor, hard', [0.02, 0.03, 0.03, 0.03, 0.03, 0.02]],
];

export const starterMaterials: Material[] = TEXTBOOK.map(([name, a]) => ({
  name,
  absorption: [a[0], ...a, a[5]],
  scattering: new Array(8).fill(0.1),
  source: SOURCE,
}));

// Suggestions from layer names, first match wins. They only pre-fill layers that have no
// material yet; the engineer confirms the mapping once per project.
const RULES: [RegExp, string][] = [
  [/carpet/i, 'Carpet, heavy'],
  [/plaster|gypsum|ceiling/i, 'Plaster, smooth'],
  [/wood|timber|reflector/i, 'Wood panel on battens'],
  [/glass|glaz/i, 'Glass, 6 mm'],
  [/absorb|treatment|mineral/i, 'Mineral wool, 50 mm'],
  [/^(?!.*wall).*(audience|seat)/i, 'Seating, upholstered, occupied'],
  [/concrete/i, 'Concrete, painted'],
  [/floor/i, 'Floor, hard'],
];

export function suggestMaterial(layerPath: string, library: Material[]): string | null {
  const hit = RULES.find(([re, name]) => re.test(layerPath) && library.some((m) => m.name === name));
  return hit ? hit[1] : null;
}
