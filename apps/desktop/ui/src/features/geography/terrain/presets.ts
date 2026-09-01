import type { LandformModelType } from "../landform3dData";
import type { PaletteTint } from "./palette";

export interface TerrainPreset {
  type: LandformModelType;
  seed: number;
  extent: number;
  /** grid cells per side — desktop (finer) vs compact screens */
  resolutionDesktop: number;
  resolutionMobile: number;
  /** vertical thickness of the museum-slice base */
  thickness: number;
  /** sea level; null = no water */
  waterLevel: number | null;
  contourInterval: number;
  contourBase: number;
  camera: {
    position: [number, number, number];
    target: [number, number, number];
  };
  section: { a: [number, number]; b: [number, number] };
  /** label-id -> world (x, z) anchor */
  labelAnchors: Record<string, [number, number]>;
  /** subtle per-type hue tint */
  tint?: PaletteTint;
}

const P: Record<LandformModelType, TerrainPreset> = {
  mountains: {
    type: "mountains",
    seed: 11,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.95,
    waterLevel: null,
    contourInterval: 0.34,
    contourBase: 0,
    camera: { position: [6.8, 5.6, 8.6], target: [0, 1.05, 0] },
    section: { a: [-3.8, -3.1], b: [3.8, 2.2] },
    labelAnchors: {
      peak: [-0.3, -0.5],
      ridge: [0.4, -1.1],
      valley: [1.6, 0.7],
      slope: [-2.6, 1.2],
    },
  },
  plateau: {
    type: "plateau",
    seed: 23,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.9,
    waterLevel: null,
    contourInterval: 0.3,
    contourBase: 0,
    camera: { position: [7.6, 5.8, 8.6], target: [0, 1.15, 0] },
    section: { a: [-3.8, 0], b: [3.8, 0] },
    labelAnchors: {
      surface: [-0.8, -0.4],
      edge: [2.9, -0.6],
      gorge: [0.6, -1.9],
    },
  },
  basin: {
    type: "basin",
    seed: 37,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.9,
    waterLevel: null,
    contourInterval: 0.3,
    contourBase: 0,
    camera: { position: [7.8, 6.4, 7.8], target: [0, 0.95, 0] },
    section: { a: [-3.8, -0.4], b: [3.8, 0.4] },
    labelAnchors: {
      rim: [-3.1, 0.8],
      mountains: [2.4, -1.7],
      floor: [0, 0.3],
    },
  },
  plain: {
    type: "plain",
    seed: 41,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.8,
    waterLevel: 0.12,
    contourInterval: 0.07,
    contourBase: 0,
    camera: { position: [6.8, 4.5, 8.4], target: [0, 0.38, 0] },
    section: { a: [-3.8, 2.1], b: [3.8, -1.0] },
    labelAnchors: {
      plain: [-2.2, 1.2],
      channel: [0.2, -0.65],
      floodplain: [1.5, 0.85],
      alluvium: [2.7, -1.7],
    },
  },
  river: {
    type: "river",
    seed: 53,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.85,
    waterLevel: 0.26,
    contourInterval: 0.26,
    contourBase: 0,
    camera: { position: [7.4, 5.4, 8.2], target: [0, 0.7, 0] },
    section: { a: [-3.8, 0], b: [3.8, 0] },
    labelAnchors: {
      valley: [-2.6, 1.2],
      channel: [0.15, 0],
      floodplain: [1.2, 1.4],
      fan: [3.1, -1.1],
    },
  },
  "island-arc": {
    type: "island-arc",
    seed: 61,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.85,
    waterLevel: 0,
    contourInterval: 0.5,
    contourBase: 0,
    camera: { position: [8.2, 6.0, 8.2], target: [-0.3, -0.4, -0.2] },
    section: { a: [-3.8, -0.4], b: [3.8, -0.4] },
    labelAnchors: {
      trench: [-2.9, 0.7],
      subduction: [-1.1, -0.2],
      volcano: [1.35, 0.5],
      arc: [2.85, 1.1],
    },
  },
  volcanic: {
    type: "volcanic",
    seed: 71,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.95,
    waterLevel: null,
    contourInterval: 0.38,
    contourBase: 0,
    camera: { position: [7.6, 6.2, 7.8], target: [0, 1.0, 0] },
    section: { a: [-3.6, 0], b: [3.6, 0] },
    labelAnchors: {
      cone: [0, 2.4],
      crater: [0.12, 2.7],
      lava: [2.1, 0.6],
    },
    tint: [1.01, 0.97, 0.93],
  },
  glacial: {
    type: "glacial",
    seed: 83,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.9,
    waterLevel: null,
    contourInterval: 0.3,
    contourBase: 0,
    camera: { position: [7.4, 5.6, 8.2], target: [0, 0.9, 0] },
    section: { a: [0, -3.0], b: [0, 3.0] },
    labelAnchors: {
      ice: [0, 0.3],
      "u-valley": [0, 1.8],
      moraine: [2.2, 0.3],
    },
    tint: [0.97, 0.99, 1.0],
  },
  karst: {
    type: "karst",
    seed: 97,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.85,
    waterLevel: null,
    contourInterval: 0.34,
    contourBase: 0,
    camera: { position: [7.6, 5.6, 7.8], target: [0, 1.0, 0] },
    section: { a: [-3.8, -1.2], b: [3.8, 1.0] },
    labelAnchors: {
      tower: [-1.5, -0.3],
      sinkhole: [0.55, 0.35],
      cave: [2.3, 1.2],
    },
  },
  desert: {
    type: "desert",
    seed: 103,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.8,
    waterLevel: null,
    contourInterval: 0.14,
    contourBase: 0,
    camera: { position: [7.6, 4.8, 8.4], target: [0, 0.45, 0] },
    section: { a: [-3.8, -1.6], b: [3.8, 1.6] },
    labelAnchors: {
      dune: [-1.4, 0.3],
      windward: [-2.4, 0.5],
      slipface: [-0.5, 0.9],
    },
    tint: [1.02, 1.0, 0.95],
  },
  coastal: {
    type: "coastal",
    seed: 113,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.85,
    waterLevel: 0,
    contourInterval: 0.32,
    contourBase: 0,
    camera: { position: [8.2, 5.4, 8.4], target: [0, 0.3, 0] },
    section: { a: [-3.8, 0], b: [3.8, 0] },
    labelAnchors: {
      shore: [0.15, -0.3],
      beach: [-1.9, 0.9],
      cliff: [2.7, -0.7],
    },
  },
  structural: {
    type: "structural",
    seed: 127,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.9,
    waterLevel: null,
    contourInterval: 0.34,
    contourBase: 0,
    camera: { position: [7.8, 5.8, 8.2], target: [0, 1.0, 0] },
    section: { a: [-3.6, -0.6], b: [3.6, 0.6] },
    labelAnchors: {
      fault: [-0.4, -0.2],
      scarp: [0.6, -0.5],
      rift: [-1.3, 0.7],
    },
  },
  "mass-movement": {
    type: "mass-movement",
    seed: 131,
    extent: 4,
    resolutionDesktop: 160,
    resolutionMobile: 104,
    thickness: 0.9,
    waterLevel: null,
    contourInterval: 0.3,
    contourBase: 0,
    camera: { position: [7.8, 5.8, 8.2], target: [0.3, 1.0, 0] },
    section: { a: [-3.6, 0], b: [3.6, 0] },
    labelAnchors: {
      head: [-1.6, -0.4],
      slide: [-0.3, 0.1],
      toe: [1.9, 0.7],
    },
  },
};

export function getPreset(type: LandformModelType): TerrainPreset {
  return P[type] ?? P.mountains;
}
