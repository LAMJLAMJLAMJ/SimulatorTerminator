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
  /** Layer path + resolved material, e.g. "Walls|Plaster". Stable across re-imports. */
  key: string;
  /** Rhino layer or SketchUp tag path. */
  layerPath: string;
  /** Resolved: object over layer (Rhino), face paint (SketchUp). */
  material: string;
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
  /** SketchUp only: group of the back-face paint. */
  triGroupBack?: Uint16Array;
  /** Bit 0 = from a curved or smoothed source face. */
  triFlags?: Uint8Array;
  warnings: string[];
  skipped: { object: string; reason: string }[];
}
