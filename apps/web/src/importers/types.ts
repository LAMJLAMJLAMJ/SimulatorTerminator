// Contract for the model-import work. Importers run in TypeScript because the
// libraries that read .3dm, .skp and IFC in the browser are JavaScript or wasm.
// The engine converts to metres and Z up and runs the mesh checks.

export interface ImportOptions {
  /** Skip objects on hidden layers. */
  skipHidden?: boolean;
}

export interface GeometryImporter {
  id: 'rhino3dm' | 'openskp' | 'web-ifc' | 'obj' | (string & {});
  extensions: string[];
  import(file: ArrayBuffer, opts: ImportOptions): Promise<RawScene>;
}

export interface RawGroup {
  /** Layer or tag path. Stable across re-imports; acoustic materials are assigned per layer. */
  key: string;
  /** Rhino layer or SketchUp tag path. */
  layerPath: string;
  /** Informational only: the source object or face material. */
  material?: string;
  label: string;
}

export interface RawScene {
  importer: string;
  /** 0.001 for a model drawn in mm. */
  unitScaleToMetres: number;
  upAxis: 'z' | 'y';
  /** float64, metres; vertices are relative to it. */
  origin: [number, number, number];
  groups: RawGroup[];
  /** Relative to origin, in source units. */
  vertices: Float32Array;
  triangles: Uint32Array;
  triGroup: Uint16Array;
  /** Bit 0 = from a curved or smoothed source face. */
  triFlags?: Uint8Array;
  /** Point objects on source and receiver layers, relative to origin, in source units. */
  points: RawPoint[];
  warnings: string[];
  skipped: { object: string; reason: string }[];
}

export interface RawPoint {
  id: string;
  role: 'source' | 'receiver';
  position: [number, number, number];
}
