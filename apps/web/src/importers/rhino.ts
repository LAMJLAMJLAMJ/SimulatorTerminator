// Rhino (.3dm) importer on rhino3dm.js (openNURBS in wasm). rhino3dm cannot
// mesh NURBS, so planar Brep faces are rebuilt from their trim loops and
// curved faces use the render mesh saved in the file. Coordinates stay in
// model units; groups are full layer paths.
import type { Brep, BrepTrim, File3dmObject, Layer, Mesh, RhinoModule } from 'rhino3dm';
import { SceneBuilder, SkipList, identity, mirrors, mul4, pointRole, sub, transform, triangulatePolygon, type Mat4, type Vec3 } from './geometry';
import type { GeometryImporter, ImportOptions, RawScene } from './types';

const UNIT_TO_METRES: Record<string, number> = {
  Microns: 1e-6,
  Millimeters: 0.001,
  Centimeters: 0.01,
  Decimeters: 0.1,
  Meters: 1,
  Kilometers: 1000,
  Inches: 0.0254,
  Feet: 0.3048,
  Yards: 0.9144,
  Miles: 1609.344,
  None: 1,
};

/** Samples along a curved trim edge; line and polyline edges use their own vertices. */
const EDGE_SAMPLES = 16;

export function importRhino(rhino: RhinoModule, bytes: ArrayBuffer, opts: ImportOptions = {}): RawScene {
  const skipHidden = opts.skipHidden ?? true;
  const doc = rhino.File3dm.fromByteArray(new Uint8Array(bytes));
  if (!doc) throw new Error('Not a readable .3dm file');
  const scene = new SceneBuilder();
  const skipped = new SkipList();
  const warnings: string[] = [];

  const unitSystem = doc.settings().modelUnitSystem;
  const unitName = Object.keys(rhino.UnitSystem).find((k) => (rhino.UnitSystem as Record<string, unknown>)[k] === unitSystem);
  const unitScale = unitName && UNIT_TO_METRES[unitName];
  if (!unitScale) warnings.push(`Unknown model units ${unitName ?? String(unitSystem)}; assuming metres`);

  const materials: string[] = [];
  for (let i = 0; i < doc.materials().count; i++) materials.push(doc.materials().get(i).name || `material#${i}`);
  const layers: Layer[] = [];
  for (let i = 0; i < doc.layers().count; i++) layers.push(doc.layers().get(i));
  const layerById = new Map(layers.map((l) => [l.id, l]));
  const layerVisible = (l: Layer) => {
    for (let x: Layer | undefined = l; x; x = layerById.get(x.parentLayerId)) if (!x.visible) return false;
    return true;
  };
  const layerMaterial = (l: Layer | undefined) => (l && l.renderMaterialIndex >= 0 ? materials[l.renderMaterialIndex] : null);

  // Object materials are informational: acoustic materials are assigned per layer.
  const MS = rhino.ObjectMaterialSource;
  const resolveMaterial = (obj: File3dmObject, layer: Layer | undefined, parent: string | null | undefined) => {
    const a = obj.attributes();
    if (a.materialSource === MS.MaterialFromObject && a.materialIndex >= 0) return materials[a.materialIndex];
    if (a.materialSource === MS.MaterialFromParent && parent !== undefined) return parent;
    return layerMaterial(layer);
  };

  const meshTris = (mesh: Mesh, M: Mat4) => {
    const out: number[] = [];
    const v = mesh.vertices();
    const f = mesh.faces();
    const P: Vec3[] = [];
    for (let i = 0; i < v.count; i++) P.push(transform(M, v.get(i) as number[]));
    const flip = mirrors(M);
    const push = (a: number, b: number, c: number) => out.push(...P[a], ...(flip ? P[c] : P[b]), ...(flip ? P[b] : P[c]));
    for (let i = 0; i < f.count; i++) {
      const [a, b, c, d] = f.get(i) as number[];
      push(a, b, c);
      if (d !== c) push(a, c, d);
    }
    return out;
  };

  // Points along one trim, in loop direction, without its end point.
  const trimPoints = (brep: Brep, trim: BrepTrim): Vec3[] => {
    const edge = brep.edges().get(trim.edgeIndex);
    const pts: Vec3[] = [];
    if (edge.isPolyline()) {
      const pl = edge.tryGetPolyline() as { count: number; get(i: number): number[] };
      for (let k = 0; k < pl.count; k++) pts.push(pl.get(k) as Vec3);
    } else {
      const [t0, t1] = edge.domain;
      for (let k = 0; k <= EDGE_SAMPLES; k++) pts.push(edge.pointAt(t0 + ((t1 - t0) * k) / EDGE_SAMPLES) as Vec3);
    }
    if (trim.isReversed) pts.reverse();
    pts.pop();
    return pts;
  };

  // Missing from the typings, present at runtime.
  const OUTER = (rhino as unknown as { BrepLoopType: { Outer: unknown } }).BrepLoopType.Outer;

  const addBrep = (brep: Brep, M: Mat4, object: string, layer: string, material: string | null) => {
    const faces = brep.faces();
    const flip = mirrors(M);
    let missing = 0;
    for (let fi = 0; fi < faces.count; fi++) {
      const face = faces.get(fi);
      if (!face.isPlanar()) {
        const mesh = face.getMesh(rhino.MeshType.Any);
        if (mesh) scene.add(layer, material, true, meshTris(mesh, M));
        else missing++;
        continue;
      }
      // Planar faces are rebuilt from their trim loops even when a render mesh
      // exists: render meshes of neighbouring faces are made independently and
      // their edges do not line up, which leaves cracks in the room shell.
      const loops: { outer: boolean; pts: Vec3[] }[] = [];
      for (let li = 0; li < face.loops.count; li++) {
        const loop = face.loops.get(li);
        const pts: Vec3[] = [];
        for (let ti = 0; ti < loop.trimCount; ti++) {
          const trim = loop.trims.get(ti);
          if (trim.edgeIndex >= 0) pts.push(...trimPoints(brep, trim));
        }
        loops.push({ outer: loop.loopType === OUTER, pts: pts.map((p) => transform(M, p)) });
      }
      const outer = loops.find((l) => l.outer) ?? loops[0];
      if (!outer) continue;
      const holes = loops.filter((l) => l !== outer).map((l) => l.pts);
      const [u0, u1] = face.domain(0);
      const [v0, v1] = face.domain(1);
      const n = face.normalAt((u0 + u1) / 2, (v0 + v1) / 2) as Vec3;
      const sign = face.orientationIsReversed ? -1 : 1;
      // World normal: transform two points rather than invert M.
      const c = face.pointAt((u0 + u1) / 2, (v0 + v1) / 2) as Vec3;
      const nw = sub(transform(M, [c[0] + sign * n[0], c[1] + sign * n[1], c[2] + sign * n[2]]), transform(M, c));
      // A mirroring transform maps the normal to the back side.
      if (flip) for (let k = 0; k < 3; k++) nw[k] = -nw[k];
      try {
        const tris = triangulatePolygon(outer.pts, holes, nw);
        if (flip)
          for (let i = 0; i < tris.length; i += 9)
            for (let k = 0; k < 3; k++) [tris[i + 3 + k], tris[i + 6 + k]] = [tris[i + 6 + k], tris[i + 3 + k]];
        scene.add(layer, material, false, tris);
      } catch (err) {
        warnings.push(`Could not triangulate a planar face of ${object}: ${(err as Error).message}`);
      }
    }
    if (missing) skipped.add(object, 'curved face saved without a render mesh', missing);
  };

  const OT = rhino.ObjectType;
  const ignored = new Map<string, number>();
  const visit = (obj: File3dmObject, M: Mat4, parent: { path: string; material: string | null } | null) => {
    const g = obj.geometry();
    const a = obj.attributes();
    const layer = layers[a.layerIndex] as Layer | undefined;
    const layerPath = layer?.fullPath ?? '';
    const typeName = g.constructor.name;
    const object = [parent?.path, a.name || `${layerPath.split('::').pop()} ${typeName}`].filter(Boolean).join(' / ');
    if (skipHidden && (!a.visible || (layer && !layerVisible(layer)))) {
      skipped.add(object, !a.visible ? 'hidden object' : `hidden layer "${layerPath}"`);
      return;
    }
    const material = resolveMaterial(obj, layer, parent?.material);
    switch (g.objectType) {
      case OT.Mesh:
        scene.add(layerPath, material, false, meshTris(g as Mesh, M));
        break;
      case OT.Brep:
        addBrep(g as Brep, M, object, layerPath, material);
        break;
      case OT.Extrusion: {
        const ext = g as unknown as { getMesh(t: unknown): Mesh | null; toBrep(split: boolean): Brep };
        const mesh = ext.getMesh(rhino.MeshType.Any);
        if (mesh) scene.add(layerPath, material, true, meshTris(mesh, M));
        else addBrep(ext.toBrep(true), M, object, layerPath, material);
        break;
      }
      case OT.Surface:
        addBrep(rhino.Brep.createFromSurface(g as never), M, object, layerPath, material);
        break;
      case OT.Point: {
        // Point objects on a sources or receivers layer place them; others are ignored.
        const role = pointRole(layerPath);
        if (!role) {
          ignored.set('Point', (ignored.get('Point') ?? 0) + 1);
          break;
        }
        const n = scene.pointCount(role) + 1;
        scene.addPoint(a.name || `${role === 'source' ? 'S' : 'R'}${n}`, role, transform(M, (g as unknown as { location: number[] }).location));
        break;
      }
      case OT.SubD:
        skipped.add(object, 'SubD: convert to a mesh in Rhino before export');
        break;
      case OT.InstanceReference: {
        const ref = g as unknown as { parentIdefId: string; xform: { toFloatArray(rowDominant: boolean): number[] } };
        const idef = doc.instanceDefinitions().findId(ref.parentIdefId);
        if (!idef) {
          warnings.push(`Block definition missing for ${object}`);
          break;
        }
        const childM = mul4(M, ref.xform.toFloatArray(true));
        for (const id of idef.getObjectIds() as string[]) {
          const child = doc.objects().findId(id);
          if (child) visit(child, childM, { path: `${object} [${idef.name}]`, material });
        }
        break;
      }
      default: {
        // Name by object type: a clipping plane's geometry is a plain Surface.
        const kind = Object.keys(OT).find((k) => (OT as Record<string, unknown>)[k] === g.objectType) ?? typeName;
        ignored.set(kind, (ignored.get(kind) ?? 0) + 1);
      }
    }
  };

  const objects = doc.objects();
  for (let i = 0; i < objects.count; i++) {
    const o = objects.get(i);
    if (o.attributes().isInstanceDefinitionObject) continue;
    visit(o, identity(), null);
  }
  if (ignored.size)
    warnings.push(`Ignored objects that are not surfaces: ${[...ignored].map(([t, n]) => `${n} ${t}`).join(', ')}`);
  (doc as unknown as { delete(): void }).delete();

  return scene.finish({ importer: 'rhino3dm', unitScaleToMetres: unitScale || 1 }, warnings, skipped);
}

/** `load` starts the rhino3dm wasm module; it is called once, on the first import. */
export function rhinoImporter(load: () => Promise<RhinoModule>): GeometryImporter {
  let rhino: Promise<RhinoModule> | null = null;
  return {
    id: 'rhino3dm',
    extensions: ['.3dm'],
    import: async (file, opts) => importRhino(await (rhino ??= load()), file, opts),
  };
}
