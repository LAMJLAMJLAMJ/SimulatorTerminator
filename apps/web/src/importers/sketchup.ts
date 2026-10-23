// SketchUp (.skp) importer on OpenSKP, which reads legacy and 2021+ files in
// the browser. Coordinates stay in inches; groups are SketchUp tags.
import { parseSkp, type Definition } from 'openskp';
import { SceneBuilder, SkipList, identity, mirrors, mul4, transform, triangulatePolygon, type Mat4, type Vec3 } from './geometry';
import type { GeometryImporter, ImportOptions, RawScene } from './types';

const INCH = 0.0254;

// Instance.matrix is [r00 r01 r02 r10 r11 r12 r20 r21 r22 tx ty tz w]:
// a row-major rotation and scale block plus a translation.
const instanceMatrix = (a: number[]): Mat4 => [
  a[0], a[1], a[2], a[9],
  a[3], a[4], a[5], a[10],
  a[6], a[7], a[8], a[11],
  0, 0, 0, a[12] || 1,
];

interface Context {
  layer: string;
  material: string | null;
  hidden: boolean;
  path: string[];
}

export function importSkp(buffer: ArrayBuffer, opts: ImportOptions = {}): RawScene {
  const skipHidden = opts.skipHidden ?? true;
  const model = parseSkp(buffer);
  const scene = new SceneBuilder();
  const skipped = new SkipList();
  const warnings: string[] = [];
  const matName = (id: number | null) => (id == null ? null : (model.materialsById.get(id)?.name ?? `material#${id}`));
  const hiddenTags = new Set(model.layers.filter((l) => l.hidden).map((l) => l.name));

  const walk = (def: Definition, M: Mat4, ctx: Context) => {
    const vById = new Map(def.vertices.map((v) => [v.id, [v.x, v.y, v.z] as Vec3]));
    const eById = new Map(def.edges.map((e) => [e.id, e]));
    const object = ctx.path.join(' / ') || '(model)';
    const layer = ctx.layer || 'Untagged';
    const flip = mirrors(M);
    for (const face of def.faces) {
      if (skipHidden && (ctx.hidden || face.hidden || hiddenTags.has(layer))) {
        skipped.add(object, ctx.hidden ? 'hidden instance' : face.hidden ? 'hidden face' : `hidden tag "${layer}"`);
        continue;
      }
      const loops = face.loops.map((loop) =>
        loop.map((ce) => {
          const e = eById.get(ce.edgeId)!;
          return vById.get(ce.orientation >= 0 ? e.v1Id : e.v2Id)!;
        }),
      );
      // Soft or smooth edges mark faces that SketchUp shows as one curved surface.
      const curved = face.loops.some((loop) =>
        loop.some((ce) => {
          const e = eById.get(ce.edgeId);
          return !!e && (e.soft || e.smooth);
        }),
      );
      let tris: number[];
      try {
        tris = triangulatePolygon(loops[0], loops.slice(1), face.normal);
      } catch (err) {
        warnings.push(`Could not triangulate a face in ${object}: ${(err as Error).message}`);
        continue;
      }
      const world: number[] = [];
      for (let i = 0; i < tris.length; i += 9) {
        const a = transform(M, tris.slice(i, i + 3));
        const b = transform(M, tris.slice(i + 3, i + 6));
        const c = transform(M, tris.slice(i + 6, i + 9));
        world.push(...a, ...(flip ? c : b), ...(flip ? b : c));
      }
      // A face left on the default material takes the paint of the group or component around it.
      scene.add(layer, matName(face.materialId) ?? ctx.material, curved, world);
    }
    for (const inst of def.instances) {
      const child = model.definitions.get(inst.refIdx);
      if (!child) {
        warnings.push(`Missing definition ${inst.refIdx} for "${inst.name}"`);
        continue;
      }
      walk(child, mul4(M, instanceMatrix(inst.matrix)), {
        layer: inst.layer || ctx.layer,
        material: matName(inst.materialId) ?? ctx.material,
        hidden: ctx.hidden || inst.hidden,
        path: [...ctx.path, inst.name || child.name || 'unnamed'],
      });
    }
  };
  walk(model.root, identity(), { layer: '', material: null, hidden: false, path: [] });

  return scene.finish({ importer: 'openskp', unitScaleToMetres: INCH }, warnings, skipped);
}

export const sketchupImporter: GeometryImporter = {
  id: 'openskp',
  extensions: ['.skp'],
  import: async (file, opts) => importSkp(file, opts),
};
