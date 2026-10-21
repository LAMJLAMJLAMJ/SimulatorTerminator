import type { RawScene } from './types';

/** A closed shoebox in millimetres, for smoke tests and the demo button. */
export function testRoom(x = 20000, y = 30000, z = 12000): RawScene {
  // prettier-ignore
  const vertices = new Float32Array([0,0,0, x,0,0, x,y,0, 0,y,0, 0,0,z, x,0,z, x,y,z, 0,y,z]);
  // prettier-ignore
  const triangles = new Uint32Array([0,2,1, 0,3,2, 4,5,6, 4,6,7, 0,1,5, 0,5,4, 2,3,7, 2,7,6, 0,4,7, 0,7,3, 1,2,6, 1,6,5]);
  return {
    importer: 'test-room',
    unitScaleToMetres: 0.001,
    upAxis: 'z',
    origin: [0, 0, 0],
    groups: [{ key: 'Room|Default', layerPath: 'Room', material: 'Default', label: 'Room' }],
    vertices,
    triangles,
    triGroup: new Uint16Array(12),
    warnings: [],
    skipped: [],
  };
}
