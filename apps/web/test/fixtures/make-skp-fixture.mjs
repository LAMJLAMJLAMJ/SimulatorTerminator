// Builds fixtures/hall.skp: a small shoebox hall that exercises the cases an
// acoustic importer must handle (tags, front/back materials, a hole, component
// instancing with inherited paint, a faceted column, a hanging reflector, a
// stray tiny face and a hidden instance). SketchUp units are inches.
import { create } from 'openskp';
import { writeFileSync } from 'node:fs';
const IN = 39.37007874; // inches per metre
const m = (x, y, z) => [x * IN, y * IN, z * IN];
const L = 20, W = 12, H = 8;

const b = create();
const plaster = b.addMaterial('Plaster', [235, 232, 225]);
const carpet = b.addMaterial('Carpet_Heavy', [120, 40, 40]);
const wood = b.addMaterial('Wood_Panel', [170, 120, 70]);
const glass = b.addMaterial('Glass', [180, 210, 230]);
const seat = b.addMaterial('Seat_Upholstered', [60, 60, 140]);
const acoustic = b.addMaterial('ACO_Absorber_50mm', [90, 140, 90]);

// Tags name the acoustic finish; they sit on groups and components, since
// raw SketchUp faces normally stay Untagged.
const tCarpet = b.addLayer('Floor carpet');
const tPlaster = b.addLayer('Plaster');
const tWood = b.addLayer('Wood panel');
const tAbsorber = b.addLayer('Absorber 50mm');
const tGlass = b.addLayer('Glass');
const tAud = b.addLayer('Audience');
const tStage = b.addLayer('Stage');
const tRefl = b.addLayer('Reflectors');
const tFloor = b.addLayer('Floor');
const tHidden = b.addLayer('x-Furniture', { hidden: true });

// Room shell as one group. Faces are wound so their FRONT points OUT of the
// room (what SketchUp's push/pull gives you), and the paint is on the BACK,
// i.e. the side you see from inside. The floor is the exception: wound inward
// and painted on the front. Both styles occur in real models.
const shellGroup = (name, layer, faces) => b.addGroup((d) => { for (const [pts, opts] of faces) d.addFace(pts, opts); }, { name, layer });
// floor: front faces up (into the room)
shellGroup('Floor', tCarpet, [[[m(0,0,0), m(L,0,0), m(L,W,0), m(0,W,0)], { material: carpet }]]);
// ceiling and side walls: front faces out of the room, painted on the back
shellGroup('Ceiling and side walls', tPlaster, [
  [[m(0,0,H), m(L,0,H), m(L,W,H), m(0,W,H)], { backMaterial: plaster }],
  [[m(L,0,0), m(L,0,H), m(0,0,H), m(0,0,0)], { backMaterial: plaster }],
  [[m(0,W,H), m(L,W,H), m(L,W,0), m(0,W,0)], { backMaterial: plaster }],
]);
shellGroup('Stage wall', tWood, [[[m(L,W,0), m(L,W,H), m(L,0,H), m(L,0,0)], { backMaterial: wood }]]);
// rear wall (x = 0) with a window opening, plus the glass filling it. These
// two are deliberately reversed (front faces into the room), a common slip
// that the importer must fix.
const hole = [m(0,4,2), m(0,4,5), m(0,8,5), m(0,8,2)];
shellGroup('Rear wall', tAbsorber, [[[m(0,0,0), m(0,W,0), m(0,W,H), m(0,0,H)], { backMaterial: acoustic, holes: [hole] }]]);
shellGroup('Window', tGlass, [[[m(0,4,2), m(0,8,2), m(0,8,5), m(0,4,5)], { backMaterial: glass }]]);

// One seat block, used 12 times; unpainted faces inherit the instance paint.
const seatBlock = b.addComponentDefinition('Seat block', (d) => {
  const [x, y, z] = [1.6, 0.9, 0.8];
  const box = [
    [m(0,0,0), m(0,y,0), m(x,y,0), m(x,0,0)],
    [m(0,0,z), m(x,0,z), m(x,y,z), m(0,y,z)],
    [m(0,0,0), m(x,0,0), m(x,0,z), m(0,0,z)],
    [m(0,y,0), m(0,y,z), m(x,y,z), m(x,y,0)],
    [m(0,0,0), m(0,0,z), m(0,y,z), m(0,y,0)],
    [m(x,0,0), m(x,y,0), m(x,y,z), m(x,0,z)],
  ];
  for (const f of box) d.addFace(f);
});

// Stage riser as a box, painted on the outside (front faces out).
b.addGroup((d) => {
  const [x0, x1, y0, y1, z] = [15, 20, 1, 11, 0.6];
  d.addFace([m(x0,y0,z), m(x1,y0,z), m(x1,y1,z), m(x0,y1,z)], { material: wood });
  d.addFace([m(x0,y0,0), m(x0,y0,z), m(x0,y1,z), m(x0,y1,0)], { material: wood });
  d.addFace([m(x0,y0,0), m(x1,y0,0), m(x1,y0,z), m(x0,y0,z)], { material: wood });
  d.addFace([m(x0,y1,0), m(x0,y1,z), m(x1,y1,z), m(x1,y1,0)], { material: wood });
}, { name: 'Stage', layer: tStage });

// A faceted round column, 24 segments with smoothed edges (a "curved" surface).
const column = b.addComponentDefinition('Column', (d) => {
  const r = 0.3, n = 24;
  for (let i = 0; i < n; i++) {
    const a0 = (2 * Math.PI * i) / n, a1 = (2 * Math.PI * (i + 1)) / n;
    const p = (a, z) => m(r * Math.cos(a), r * Math.sin(a), z);
    d.addFace([p(a0, 0), p(a1, 0), p(a1, H), p(a0, H)], { material: plaster, softEdges: true, smoothEdges: true });
  }
});

// A single-sided hanging reflector panel, tilted 15 degrees.
const reflector = b.addComponentDefinition('Reflector', (d) => {
  const t = (15 * Math.PI) / 180, w = 3, dd = 2;
  d.addFace([m(0,0,0), m(dd*Math.cos(t),0,dd*Math.sin(t)), m(dd*Math.cos(t),w,dd*Math.sin(t)), m(0,w,0)], { material: wood, backMaterial: wood });
});

// Stray geometry: a 5 mm triangle left on the floor.
const stray = b.addComponentDefinition('Stray', (d) => {
  d.addFace([m(5,5,0.001), m(5.005,5,0.001), m(5,5.005,0.001)]);
});

// A sofa on a hidden tag, plus a hidden instance; both should be skipped.
const sofa = b.addComponentDefinition('Sofa', (d) => {
  d.addFace([m(0,0,0), m(2,0,0), m(2,1,0), m(0,1,0)], { material: seat });
});

for (let r = 0; r < 4; r++) for (let c = 0; c < 3; c++)
  b.addInstance(seatBlock, { name: `Seats ${r + 1}${'ABC'[c]}`, translation: m(3 + r * 2.2, 1.5 + c * 3.2, 0), material: seat, layer: tAud });
b.addInstance(column, { name: 'Column 1', translation: m(12, 1.2, 0), layer: tPlaster });
b.addInstance(column, { name: 'Column 2', translation: m(12, 10.8, 0), layer: tPlaster });
b.addInstance(reflector, { name: 'Reflector', translation: m(13, 4.5, 6), layer: tRefl });
b.addInstance(stray, { name: '', layer: tFloor });
b.addInstance(sofa, { name: 'Sofa', translation: m(1, 1, 0), layer: tHidden });
b.addInstance(sofa, { name: 'Sofa hidden', translation: m(1, 9, 0), hidden: true });

writeFileSync(process.argv[2] ?? 'test/fixtures/hall.skp', b.toBytes());
console.log('wrote', process.argv[2] ?? 'test/fixtures/hall.skp');
