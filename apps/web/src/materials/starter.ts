// The built-in library: the studio's own 64 materials (library.csv), loaded into
// every new project. Octave bands 63 Hz to 8 kHz.
import type { Material } from '../engine/protocol/Material';
import { parseMaterialCsv } from './csv';
import libraryCsv from './library.csv?raw';

/** Rows that are not absorption coefficients of a surface and stay out of the pickers. */
const NOT_COEFFICIENTS = /per person, sabins/i;

export const starterMaterials: Material[] = parseMaterialCsv(libraryCsv, 'the built-in library').materials.filter(
  (m) => !NOT_COEFFICIENTS.test(m.name),
);

// Suggestions from layer names, first match wins. They only pre-fill layers that have no
// material yet; the engineer confirms the mapping once per project.
const RULES: [RegExp, string][] = [
  [/carpet/i, 'Carpet, heavy, on foam pad'],
  [/curtain|drape/i, 'Curtains, heavy, deep folds'],
  [/gypsum|drywall/i, 'Gypsum board (12 mm on studs)'],
  [/ceiling/i, 'Plaster ceiling'],
  [/plaster/i, 'Plaster on masonry'],
  [/brick/i, 'Brick, unglazed'],
  [/wood|timber|reflector/i, 'Wood paneling on battens'],
  [/glass|glaz/i, 'Heavy plate glass'],
  [/absorb|treatment|mineral/i, 'Mineral wool, 50 mm, on wall'],
  [/^(?!.*wall).*(audience|seat)/i, 'Audience, occupied upholstered seats (per m²)'],
  [/concrete|floor/i, 'Concrete or terrazzo floor'],
];

export function suggestMaterial(layerPath: string, library: Material[]): string | null {
  const hit = RULES.find(([re, name]) => re.test(layerPath) && library.some((m) => m.name === name));
  return hit ? hit[1] : null;
}
