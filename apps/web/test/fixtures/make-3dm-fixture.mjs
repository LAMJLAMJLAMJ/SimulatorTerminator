// Builds fixtures/hall.3dm: the same kind of small hall as hall.skp, modelled
// the way Rhino users tend to: millimetre units, nested layers carrying the
// materials, separate planar surfaces with mixed orientation, a block placed
// 12 times, a NURBS column with no render mesh, one with a render mesh, a
// mesh balcony front, a tiny sliver, a hidden layer and some annotation.
import rhino3dm from 'rhino3dm';
import { writeFileSync } from 'node:fs';
const rh = await rhino3dm();
const mm = (x, y, z) => [x * 1000, y * 1000, z * 1000];
const L = 20, W = 12, H = 8;

const doc = new rh.File3dm();
doc.settings().modelUnitSystem = rh.UnitSystem.Millimeters;

const addMat = (name, rgb) => {
  const mat = new rh.Material(); mat.name = name;
  mat.diffuseColor = { r: rgb[0], g: rgb[1], b: rgb[2], a: 255 };
  doc.materials().add(mat);
  return doc.materials().count - 1;
};
const carpet = addMat('Carpet_Heavy', [120, 40, 40]);
const plaster = addMat('Plaster', [235, 232, 225]);
const wood = addMat('Wood_Panel', [170, 120, 70]);
const seat = addMat('Seat_Upholstered', [60, 60, 140]);
const glass = addMat('Glass', [180, 210, 230]);

const layers = {};
const addLayer = (name, parent, matIndex, visible = true) => {
  const l = new rh.Layer(); l.name = name; l.visible = visible;
  if (parent !== undefined) l.parentLayerId = doc.layers().findIndex(layers[parent]).id;
  if (matIndex !== undefined) l.renderMaterialIndex = matIndex;
  doc.layers().add(l);
  layers[parent ? `${parent}::${name}` : name] = doc.layers().count - 1;
};
addLayer('Room');
addLayer('Floor', 'Room', carpet);
addLayer('Walls', 'Room', plaster);
addLayer('Wood panel', 'Room', wood);
addLayer('Ceiling', 'Room', plaster);
addLayer('Audience', undefined, seat);
addLayer('Balcony');
addLayer('Reflectors', undefined, wood);
addLayer('Hidden stuff', undefined, undefined, false);
addLayer('Annotations');

const attrs = (layer, opts = {}) => {
  const a = new rh.ObjectAttributes();
  a.layerIndex = layers[layer];
  if (opts.name) a.name = opts.name;
  if (opts.material !== undefined) { a.materialIndex = opts.material; a.materialSource = rh.ObjectMaterialSource.MaterialFromObject; }
  else a.materialSource = rh.ObjectMaterialSource.MaterialFromLayer;
  return a;
};
const planar = (pts) => {
  const poly = new rh.Polyline(pts.length + 1);
  for (const p of pts) poly.add(p[0], p[1], p[2]);
  poly.add(pts[0][0], pts[0][1], pts[0][2]);
  const crv = poly.toNurbsCurve();
  const [a, b, c] = pts;
  const u = [b[0]-a[0], b[1]-a[1], b[2]-a[2]], v = [c[0]-a[0], c[1]-a[1], c[2]-a[2]];
  const n = [u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0]];
  const nz = (q) => { const l = Math.hypot(...q); return q.map((x) => x / l); };
  const yv = [n[1]*u[2]-n[2]*u[1], n[2]*u[0]-n[0]*u[2], n[0]*u[1]-n[1]*u[0]];
  const pl = { origin: a, xAxis: nz(u), yAxis: nz(yv), zAxis: nz(n) };
  return rh.Brep.createTrimmedPlane(pl, crv);
};

// Room surfaces: floor and stage wall face into the room, the rest face out.
doc.objects().addBrep(planar([mm(0,0,0), mm(L,0,0), mm(L,W,0), mm(0,W,0)]), attrs('Room::Floor', { name: 'Floor' }));
doc.objects().addBrep(planar([mm(0,0,H), mm(L,0,H), mm(L,W,H), mm(0,W,H)]), attrs('Room::Ceiling', { name: 'Ceiling' }));
doc.objects().addBrep(planar([mm(0,0,0), mm(0,0,H), mm(L,0,H), mm(L,0,0)]), attrs('Room::Walls', { name: 'Wall south' }));
doc.objects().addBrep(planar([mm(0,W,0), mm(L,W,0), mm(L,W,H), mm(0,W,H)]), attrs('Room::Walls', { name: 'Wall north' }));
doc.objects().addBrep(planar([mm(L,0,0), mm(L,W,0), mm(L,W,H), mm(L,0,H)]), attrs('Room::Wood panel', { name: 'Stage wall' }));
doc.objects().addBrep(planar([mm(0,0,0), mm(0,W,0), mm(0,W,H), mm(0,0,H)]), attrs('Room::Walls', { name: 'Rear wall' }));

// Columns: NURBS cylinders. Column 1 has no render mesh (as written by
// rhino3dm or saved with "Save small"); column 2 carries one, as Rhino saves.
const cyl = (x, y) => new rh.Cylinder(new rh.Circle(mm(x, y, 0), 300), H * 1000);
const col1 = rh.Brep.createFromCylinder(cyl(12, 1.2), false, false);
doc.objects().addBrep(col1, attrs('Room::Walls', { name: 'Column 1' }));
const col2 = rh.Brep.createFromCylinder(cyl(12, 10.8), false, false);
const rm = new rh.Mesh(); const n = 24;
for (let i = 0; i < n; i++) {
  const a = (2 * Math.PI * i) / n;
  rm.vertices().add(12000 + 300 * Math.cos(a), 10800 + 300 * Math.sin(a), 0);
  rm.vertices().add(12000 + 300 * Math.cos(a), 10800 + 300 * Math.sin(a), H * 1000);
}
for (let i = 0; i < n; i++) { const j = (i + 1) % n; rm.faces().addQuadFace(2*i, 2*j, 2*j+1, 2*i+1); }
rm.normals().computeNormals();
col2.faces().get(0).setMesh(rm, rh.MeshType.Render); // binding order differs from rhino3dm.d.ts
doc.objects().addBrep(col2, attrs('Room::Walls', { name: 'Column 2' }));

// Seat block: a box Brep inside a block definition, placed 12 times.
const box = rh.Brep.createFromBoundingBox(new rh.BoundingBox([0, 0, 0], [1600, 900, 800]));
const boxAttr = new rh.ObjectAttributes(); boxAttr.layerIndex = layers['Audience'];
doc.instanceDefinitions().add('Seat block', '', '', '', [0, 0, 0], [box], [boxAttr]);
const idef = doc.instanceDefinitions().get(doc.instanceDefinitions().count - 1);
for (let r = 0; r < 4; r++) for (let c = 0; c < 3; c++) {
  const ref = new rh.InstanceReference(idef.id, rh.Transform.translationXYZ(3000 + r * 2200, 1500 + c * 3200, 0));
  doc.objects().addInstanceObject(ref, attrs('Audience', { name: `Seats ${r + 1}${'ABC'[c]}` }));
}

// Balcony front as a mesh object with an object-assigned material.
const bal = new rh.Mesh();
for (const p of [mm(2,0,4), mm(2,W,4), mm(2,W,5), mm(2,0,5)]) bal.vertices().add(...p);
bal.faces().addQuadFace(0, 1, 2, 3);
doc.objects().addMesh(bal, attrs('Balcony', { name: 'Balcony front' }));

// Hanging reflector, tilted 15 degrees.
const t = (15 * Math.PI) / 180;
doc.objects().addBrep(planar([mm(13,4.5,6), mm(13+2*Math.cos(t),4.5,6+2*Math.sin(t)), mm(13+2*Math.cos(t),7.5,6+2*Math.sin(t)), mm(13,7.5,6)]), attrs('Reflectors', { name: 'Reflector' }));

// A 3 mm sliver left over from modelling.
const sl = new rh.Mesh();
for (const p of [[5000,5000,1],[5003,5000,1],[5000,5000.5,1]]) sl.vertices().add(...p);
sl.faces().addTriFace(0, 1, 2);
doc.objects().addMesh(sl, attrs('Room::Floor'));

// Things to skip: a box on a hidden layer, a text dot and a curve.
doc.objects().addBrep(rh.Brep.createFromBoundingBox(new rh.BoundingBox([1000, 1000, 0], [3000, 2000, 900])), attrs('Hidden stuff', { name: 'Sofa' }));
doc.objects().addTextDot('Stage', mm(17, 6, 1), attrs('Annotations'));
doc.objects().addLine(mm(0, 0, 0), mm(L, W, 0), attrs('Annotations'));

const out = process.argv[2] ?? 'test/fixtures/hall.3dm';
writeFileSync(out, doc.toByteArray());
console.log('wrote', out, doc.objects().count, 'objects');
