// Reads a material library from a spreadsheet saved as CSV: one material per row,
// absorption (and optionally scattering) per frequency band in columns. Column headers
// carry the frequency ("125", "1k", "a 500 Hz"); scattering columns start with "s" or
// say "scatter". Third-octave columns are averaged into octaves.
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
  const lines = text.replace(/^﻿/, '').split(/\r?\n/).filter((l) => l.trim() !== '');
  if (lines.length < 2) throw new Error(`${fileName} needs a header row and at least one material`);
  const delimiter = [';', '\t', ','].reduce((best, d) => (count(lines[0], d) > count(lines[0], best) ? d : best), ',');
  const rows = lines.map((l) => splitRow(l, delimiter));
  const header = rows[0].map((h) => h.trim());

  const nameCol = Math.max(0, header.findIndex((h) => /^(name|material|description|product)$/i.test(h)));
  const sourceCol = header.findIndex((h) => /source|reference|^ref/i.test(h));
  const columns: Column[] = [];
  header.forEach((h, index) => {
    if (index === nameCol || index === sourceCol) return;
    const hz = frequency(h);
    if (hz) columns.push({ index, hz, scattering: /^s(?![a-z])|scatter/i.test(h) });
  });
  if (!columns.some((c) => !c.scattering)) {
    throw new Error(`${fileName} has no frequency columns; headers should name the band, like 125 or 1k`);
  }

  // A decimal comma is only possible when commas are not the delimiter.
  const number = (s: string) => Number((delimiter === ',' ? s : s.replace(',', '.')).trim());
  const body = rows.slice(1);
  const all = body.flatMap((r) => columns.map((c) => number(r[c.index] ?? '')).filter(Number.isFinite));
  const percent = Math.max(...all) > 1.5;

  const warnings: string[] = [];
  if (percent) warnings.push('Values above 1.5 found: read the whole file as percentages.');
  const hasScattering = columns.some((c) => c.scattering);
  if (!hasScattering) warnings.push(`No scattering columns: every material gets ${DEFAULT_SCATTERING}.`);

  const byName = new Map<string, Material>();
  body.forEach((r, i) => {
    const line = i + 2;
    const name = (r[nameCol] ?? '').trim();
    if (!name) return;
    const values = (scattering: boolean) => {
      const at = new Map<number, number>();
      for (const c of columns.filter((c) => c.scattering === scattering)) {
        const v = number(r[c.index] ?? '');
        if (Number.isFinite(v) && (r[c.index] ?? '').trim() !== '') at.set(c.hz, percent ? v / 100 : v);
      }
      return toOctaves(at);
    };
    const absorption = values(false);
    if (!absorption.values) {
      warnings.push(`Line ${line}, ${name}: skipped, ${absorption.problem}.`);
      return;
    }
    const scattering = hasScattering ? values(true) : { values: OCTAVES.map(() => DEFAULT_SCATTERING), notes: [] };
    if (!scattering.values) {
      warnings.push(`Line ${line}, ${name}: skipped, scattering ${scattering.problem}.`);
      return;
    }
    const clamped = [...absorption.values, ...scattering.values].some((v) => v > 1 || v < 0);
    if (clamped) warnings.push(`Line ${line}, ${name}: values outside 0 to 1 were clipped.`);
    const clip = (v: number) => Math.min(1, Math.max(0, v));
    if (byName.has(name)) warnings.push(`Line ${line}: a second "${name}" replaces the first.`);
    const notes = [...new Set([...absorption.notes, ...scattering.notes])];
    const source = [sourceCol >= 0 ? (r[sourceCol] ?? '').trim() : '', `imported from ${fileName}`, ...notes]
      .filter(Boolean)
      .join('; ');
    byName.set(name, {
      name,
      absorption: absorption.values.map(clip),
      scattering: scattering.values.map(clip),
      source,
    });
  });
  return { materials: [...byName.values()], warnings };
}

/** Octave values from whatever bands a row has: octave columns as they are, third octaves averaged. */
function toOctaves(at: Map<number, number>): { values?: number[]; notes: string[]; problem?: string } {
  const notes: string[] = [];
  const out: (number | undefined)[] = OCTAVES.map((hz) => {
    if (at.has(hz) && ![hz / 1.26, hz * 1.26].some((t) => at.has(nearestThird(t)))) return at.get(hz);
    const thirds = [-1, 0, 1].map((k) => at.get(THIRDS[THIRDS.indexOf(hz) + k]));
    if (thirds.every((v) => v !== undefined)) return (thirds[0]! + thirds[1]! + thirds[2]!) / 3;
    return at.get(hz);
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

/** Splits one CSV line, honouring double quotes. */
function splitRow(line: string, delimiter: string): string[] {
  const out: string[] = [];
  let cur = '';
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (quoted) {
      if (ch === '"' && line[i + 1] === '"') {
        cur += '"';
        i++;
      } else if (ch === '"') quoted = false;
      else cur += ch;
    } else if (ch === '"') quoted = true;
    else if (ch === delimiter) {
      out.push(cur);
      cur = '';
    } else cur += ch;
  }
  out.push(cur);
  return out;
}

/** The library after an import: same names are replaced, new ones added at the end. */
export function mergeLibrary(current: Material[], imported: Material[]): Material[] {
  const incoming = new Map(imported.map((m) => [m.name, m]));
  const kept = current.map((m) => incoming.get(m.name) ?? m);
  return [...kept, ...imported.filter((m) => !current.some((c) => c.name === m.name))];
}
