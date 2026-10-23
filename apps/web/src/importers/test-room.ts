import type { RawScene } from './types';

/** A closed shoebox in millimetres, normals facing in, for smoke tests and the demo button. */
export function testRoom(x = 20000, y = 30000, z = 12000): RawScene {
  // prettier-ignore
  const vertices = new Float32Array([0,0,0, x,0,0, x,y,0, 0,y,0, 0,0,z, x,0,z, x,y,z, 0,y,z]);
  // prettier-ignore
  const triangles = new Uint32Array([0,1,2, 0,2,3, 4,6,5, 4,7,6, 0,5,1, 0,4,5, 2,7,3, 2,6,7, 0,7,4, 0,3,7, 1,6,2, 1,5,6]);
  return {
    importer: 'test-room',
    unitScaleToMetres: 0.001,
    upAxis: 'z',
    origin: [0, 0, 0],
    groups: [{ key: 'Room', layerPath: 'Room', material: 'Default', label: 'Room' }],
    vertices,
    triangles,
    triGroup: new Uint16Array(12),
    warnings: [],
    skipped: [],
  };
}
