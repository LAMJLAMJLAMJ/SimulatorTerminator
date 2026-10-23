// Reads a material library from a spreadsheet saved as CSV: one material per row,
// absorption (and optionally scattering) per frequency band in columns. Column headers
// carry the frequency ("125", "1k", "a 500 Hz"); scattering columns start with "s" or
// say "scatter". Third-octave columns are averaged into octaves. Rows above the header,
// like a title or legend, are ignored.
import type { Material } from '../engine/protocol/Material';

/** The project's octave bands, Hz. */
export const OCTAVES = [63, 125, 250, 500, 1000, 2000, 4000, 8000];
const THIRDS = [
  50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500, 630, 800, 1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000,
  6300, 8000, 10000,
];
/** Used when a file has no scattering columns. */
const DEFAULT_SCATTERING = 0.1;

export interface LibraryImport {
  materials: Material[];
  warnings: string[];
}

interface Column {
  index: number;
  hz: number;
  scattering: boolean;
}

export function parseMaterialCsv(text: string, fileName = 'CSV'): LibraryImport {
  text = text.replace(/^\uFEFF/, '');
  const head = text.split(/\r?\n/, 20).join('\n');
  const delimiter = [';', '\t', ','].reduce((best, d) => (count(head, d) > count(head, best) ? d : best), ',');
  const rows = splitRows(text, delimiter);

  // Exported workbooks often have a title or legend above the table: the header is
  // the first row that names at least three bands.
  const columnsOf = (header: string[]): Column[] =>
    header.flatMap((h, index) => {
      const hz = frequency(h);
      return hz ? [{ index, hz, scattering: /^s(?![a-z])|scatter/i.test(h.trim()) }] : [];
    });
  const headerRow = rows.slice(0, 50).findIndex((r) => columnsOf(r).filter((c) => !c.scattering).length >= 3);
  if (headerRow < 0) {
    throw new Error(`${fileName} has no frequency columns; headers should name the band, like 125 or 1k`);
  }
  const header = rows[headerRow].map((h) => h.trim());
  const nameCol = Math.max(0, header.findIndex((h) => /^(name|material|description|product)$/i.test(h)));
  const sourceCol = header.findIndex((h) => /source|reference|^ref/i.test(h));
  const idCol = header.findIndex((h) => /^(id|no\.?|nr\.?|number|#)$/i.test(h));
  const columns = columnsOf(header).filter((c) => c.index !== nameCol && c.index !== sourceCol);

  // A decimal comma is only possible when commas are not the delimiter.
  const number = (s: string) => Number((delimiter === ',' ? s : s.replace(',', '.')).trim());
  const body = rows.slice(headerRow + 1);
  // A percentage file has most values above 1; a few coefficients above 1 are normal
  // for ISO 354 measurements of thick or hung absorbers.
  const all = body
    .flatMap((r) => columns.filter((c) => !c.scattering).map((c) => r[c.index] ?? ''))
    .filter((s) => s.trim() !== '')
    .map(number)
    .filter(Number.isFinite)
    .sort((a, b) => a - b);
  const percent = all.length > 0 && all[Math.floor(all.length / 2)] > 1.5;

  const warnings: string[] = [];
  if (percent) warnings.push('Most values are above 1.5: read the whole file as percentages.');
  const hasScattering = columns.some((c) => c.scattering);
  if (!hasScattering) warnings.push(`No scattering columns: every material gets ${DEFAULT_SCATTERING}.`);
  // Problems that can hit many rows of a large library are listed once, with examples.
  const groups = new Map<string, string[]>();
  const note = (problem: string, row: string) => groups.set(problem, [...(groups.get(problem) ?? []), row]);

  const byName = new Map<string, Material>();
  body.forEach((r, i) => {
    // The row number in the spreadsheet the file was saved from.
    const line = `row ${headerRow + i + 2}`;
    const name = (r[nameCol] ?? '').replace(/\s+/g, ' ').trim();
    if (!name) return;
    const values = (scattering: boolean) => {
      const at = new Map<number, number>();
      for (const c of columns.filter((c) => c.scattering === scattering)) {
        const v = number(r[c.index] ?? '');
        if (Number.isFinite(v) && (r[c.index] ?? '').trim() !== '') at.set(c.hz, percent ? v / 100 : v);
      }
      return at.size ? toOctaves(at) : undefined;
    };
    const absorption = values(false);
    if (!absorption?.values) {
      note(`skipped, ${absorption?.problem ?? 'no absorption values'}`, `${line}, ${name}`);
      return;
    }
    let scattering = hasScattering ? values(true) : undefined;
    if (hasScattering && !scattering?.values) {
      note(`incomplete or no scattering, ${DEFAULT_SCATTERING} used`, `${line}, ${name}`);
      scattering = undefined;
    }
    const scatter = scattering?.values ?? OCTAVES.map(() => DEFAULT_SCATTERING);
    const clamped = [...absorption.values, ...scatter].some((v) => v > 1 || v < 0);
    if (clamped) note('values outside 0 to 1 clipped', `${line}, ${name}`);
    const clip = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 1000) / 1000;
    // Large libraries repeat names for different products or mountings: keep each one.
    let unique = name;
    if (byName.has(unique)) {
      const id = idCol >= 0 ? (r[idCol] ?? '').trim() : '';
      unique = id && !byName.has(`${name} (${id})`) ? `${name} (${id})` : name;
      for (let k = 2; byName.has(unique); k++) unique = `${name} (${k})`;
      note('same name as an earlier row, renamed', `${line}, ${unique}`);
    }
    const notes = [...new Set([...absorption.notes, ...(scattering?.notes ?? [])])];
    const source = [sourceCol >= 0 ? (r[sourceCol] ?? '').trim() : '', `imported from ${fileName}`, ...notes]
      .filter(Boolean)
      .join('; ');
    byName.set(unique, {
      name: unique,
      absorption: absorption.values.map(clip),
      scattering: scatter.map(clip),
      source,
    });
  });
  for (const [problem, where] of groups) {
    const examples = where.slice(0, 3).join('; ') + (where.length > 3 ? '; …' : '');
    warnings.push(`${where.length} ${where.length === 1 ? 'row' : 'rows'} ${problem} (${examples}).`);
  }
  return { materials: [...byName.values()], warnings };
}

/** Octave values from whatever bands a row has: octave columns as they are, third octaves averaged. */
function toOctaves(at: Map<number, number>): { values?: number[]; notes: string[]; problem?: string } {
  const notes: string[] = [];
  const out: (number | undefined)[] = OCTAVES.map((hz) => {
    if (at.has(hz) && ![hz / 1.26, hz * 1.26].some((t) => at.has(nearestThird(t)))) return at.get(hz);
    const thirds = [-1, 0, 1].map((k) => at.get(THIRDS[THIRDS.indexOf(hz) + k]));
    const given = thirds.filter((v) => v !== undefined);
    if (given.length === 3 || at.has(hz)) return given.length === 3 ? (given[0] + given[1] + given[2]) / 3 : at.get(hz);
    // Measurements that start or stop inside an octave, at 160 Hz say.
    if (given.length) notes.push(`${label(hz)} from ${given.length} of its 3 third octaves`);
    return given.length ? given.reduce((a, b) => a + b) / given.length : undefined;
  });
  if (out[0] === undefined && out[1] !== undefined) {
    out[0] = out[1];
    notes.push('63 Hz copied from 125 Hz');
  }
  if (out[7] === undefined && out[6] !== undefined) {
    out[7] = out[6];
    notes.push('8 kHz copied from 4 kHz');
  }
  const missing = OCTAVES.filter((_, i) => out[i] === undefined);
  if (missing.length) return { notes, problem: `no value at ${missing.map(label).join(', ')}` };
  return { values: out as number[], notes };
}

function nearestThird(hz: number) {
  return THIRDS.reduce((a, b) => (Math.abs(b - hz) < Math.abs(a - hz) ? b : a));
}

/** The band a header names, snapped to the nearest nominal third-octave centre; 0 if none. */
function frequency(header: string): number {
  if (/mm|cm|kg|thick|depth|density|nrc|saa|weight/i.test(header)) return 0;
  const m = /(\d+(?:[.,]\d+)?)\s*(k)?/i.exec(header);
  if (!m) return 0;
  const hz = Number(m[1].replace(',', '.')) * (m[2] ? 1000 : 1);
  const near = nearestThird(hz);
  return Math.abs(Math.log2(near / hz)) < 0.08 ? near : 0;
}

const label = (hz: number) => (hz >= 1000 ? `${hz / 1000} kHz` : `${hz} Hz`);

function count(s: string, ch: string) {
  return s.split(ch).length - 1;
}

/** Splits CSV text into rows of cells, honouring double quotes, which may hold delimiters and line breaks. */
function splitRows(text: string, delimiter: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cur = '';
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (quoted) {
      if (ch === '"' && text[i + 1] === '"') {
        cur += '"';
        i++;
      } else if (ch === '"') quoted = false;
      else cur += ch;
    } else if (ch === '"') quoted = true;
    else if (ch === delimiter) {
      row.push(cur);
      cur = '';
    } else if (ch === '\n' || ch === '\r') {
      if (ch === '\r' && text[i + 1] === '\n') i++;
      row.push(cur);
      rows.push(row);
      row = [];
      cur = '';
    } else cur += ch;
  }
  if (cur !== '' || row.length) rows.push([...row, cur]);
  return rows;
}

/** The library after an import: same names are replaced, new ones added at the end. */
export function mergeLibrary(current: Material[], imported: Material[]): Material[] {
  const incoming = new Map(imported.map((m) => [m.name, m]));
  const kept = current.map((m) => incoming.get(m.name) ?? m);
  return [...kept, ...imported.filter((m) => !current.some((c) => c.name === m.name))];
}
