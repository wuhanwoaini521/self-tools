export type LandformModelType =
  | "mountains"
  | "plateau"
  | "basin"
  | "plain"
  | "river"
  | "island-arc"
  | "volcanic"
  | "glacial"
  | "karst"
  | "desert"
  | "coastal"
  | "structural"
  | "mass-movement";

export type LandformViewMode = "terrain" | "contour" | "section";

export interface LandformHotspot {
  id: string;
  label: string;
  description: string;
  position: [number, number, number];
}

export interface LandformViewerConfig {
  modelType: LandformModelType;
  cameraPosition: [number, number, number];
  hotspots: LandformHotspot[];
  /** 为不支持 WebGL 的环境预留静态示意图资源。 */
  fallbackImage?: string;
}

export const LANDFORM_VIEWERS: Record<LandformModelType, LandformViewerConfig> =
  {
    mountains: {
      modelType: "mountains",
      cameraPosition: [7, 5.2, 8],
      hotspots: [
        {
          id: "peak",
          label: "山峰",
          description: "山体局部最高的位置。",
          position: [-1.7, 3.1, -0.3],
        },
        {
          id: "ridge",
          label: "山脊",
          description: "沿高处连续延伸的地形脊线。",
          position: [0.1, 2.2, -0.2],
        },
        {
          id: "valley",
          label: "山谷",
          description: "相邻山地之间受侵蚀切割形成的低地。",
          position: [1.7, 0.55, 0.4],
        },
        {
          id: "slope",
          label: "坡面",
          description: "从山脊向山谷倾斜延伸的表面。",
          position: [-2.6, 1.1, 1.25],
        },
      ],
    },
    plateau: {
      modelType: "plateau",
      cameraPosition: [7.2, 4.8, 8.2],
      hotspots: [
        {
          id: "surface",
          label: "高原面",
          description: "整体抬升后相对平缓、范围广阔的表面。",
          position: [-0.7, 2.15, 0],
        },
        {
          id: "edge",
          label: "高原边缘",
          description: "高原向周边低地急剧过渡的位置。",
          position: [3.25, 1.2, -0.45],
        },
        {
          id: "gorge",
          label: "河谷",
          description: "河流向下切割高原形成的狭长低地。",
          position: [0.4, 1.35, -1.8],
        },
      ],
    },
    basin: {
      modelType: "basin",
      cameraPosition: [7.4, 5.4, 7.4],
      hotspots: [
        {
          id: "rim",
          label: "盆地边缘",
          description: "环绕盆地、海拔相对较高的边缘地带。",
          position: [-3.05, 2.2, 0.8],
        },
        {
          id: "mountains",
          label: "周缘山地",
          description: "构成盆地边界的连续高地。",
          position: [2.25, 2.8, -1.7],
        },
        {
          id: "floor",
          label: "盆底",
          description: "四周高而中部低、易汇集沉积物的地带。",
          position: [0, 0.38, 0.2],
        },
      ],
    },
    plain: {
      modelType: "plain",
      cameraPosition: [7.4, 5, 8.5],
      hotspots: [
        {
          id: "plain",
          label: "平原",
          description: "起伏小、坡度缓的广阔低地。",
          position: [-2.1, 0.35, 1],
        },
        {
          id: "channel",
          label: "河道",
          description: "河水集中流动的通道。",
          position: [0.2, 0.44, -0.65],
        },
        {
          id: "floodplain",
          label: "河漫滩",
          description: "洪水期容易被淹没、由细颗粒沉积形成的平坦地。",
          position: [1.45, 0.31, 0.82],
        },
        {
          id: "alluvium",
          label: "冲积区",
          description: "河流搬运物在流速变缓处堆积形成的区域。",
          position: [2.6, 0.36, -1.62],
        },
      ],
    },
    river: {
      modelType: "river",
      cameraPosition: [7.3, 5.4, 8],
      hotspots: [
        {
          id: "valley",
          label: "河谷",
          description: "河流沿地势低处侵蚀形成的带状低地。",
          position: [-2.5, 1.28, 1.15],
        },
        {
          id: "channel",
          label: "河道",
          description: "河水持续流动并塑造地貌的路径。",
          position: [0.15, 0.43, 0],
        },
        {
          id: "floodplain",
          label: "河漫滩",
          description: "河道两侧反复淹没、堆积的低平地。",
          position: [1.2, 0.68, 1.4],
        },
        {
          id: "fan",
          label: "冲积扇",
          description: "河流离开山口后因流速下降形成的扇状沉积体。",
          position: [3.05, 0.56, -1.2],
        },
      ],
    },
    "island-arc": {
      modelType: "island-arc",
      cameraPosition: [7.8, 5, 8.5],
      hotspots: [
        {
          id: "trench",
          label: "海沟",
          description: "俯冲板块弯曲下沉形成的深海凹槽。",
          position: [-2.85, 0.1, 0.65],
        },
        {
          id: "subduction",
          label: "俯冲带",
          description: "一块板块向另一块板块之下运动的边界。",
          position: [-0.8, 0.15, -0.15],
        },
        {
          id: "volcano",
          label: "火山锥",
          description: "岩浆喷发、堆积形成的锥状高地。",
          position: [1.35, 2.35, -0.5],
        },
        {
          id: "arc",
          label: "岛弧",
          description: "沿俯冲带一侧呈弧形分布的火山岛链。",
          position: [2.75, 1.15, 1],
        },
      ],
    },
    volcanic: {
      modelType: "volcanic",
      cameraPosition: [7.3, 5.1, 8.1],
      hotspots: [
        {
          id: "cone",
          label: "火山锥",
          description: "喷出物在火山口周围堆积形成的锥状地形。",
          position: [0, 2.8, 0],
        },
        {
          id: "crater",
          label: "火山口",
          description: "火山顶部或侧翼的喷发出口与凹陷。",
          position: [0.12, 2.93, -0.15],
        },
        {
          id: "lava",
          label: "熔岩流",
          description: "岩浆喷出地表后沿地势低处流动并冷却形成的地带。",
          position: [2.1, 0.6, 1.1],
        },
      ],
    },
    glacial: {
      modelType: "glacial",
      cameraPosition: [7.5, 5.5, 8.3],
      hotspots: [
        {
          id: "ice",
          label: "冰川",
          description: "在重力作用下缓慢流动的巨大冰体。",
          position: [0, 1.15, -0.3],
        },
        {
          id: "u-valley",
          label: "U 形谷",
          description: "冰川将谷底加宽、谷壁削陡形成的宽阔谷地。",
          position: [0, 0.47, 1.8],
        },
        {
          id: "moraine",
          label: "冰碛",
          description: "冰川搬运并在冰缘或消融后留下的碎屑堆积。",
          position: [1.9, 0.72, 2.5],
        },
      ],
    },
    karst: {
      modelType: "karst",
      cameraPosition: [7.1, 4.8, 8],
      hotspots: [
        {
          id: "tower",
          label: "峰林",
          description: "可溶岩长期溶蚀后保留下来的孤立陡峭石峰。",
          position: [-1.4, 2.35, -0.2],
        },
        {
          id: "sinkhole",
          label: "洼地 / 落水洞",
          description: "地表水下渗与溶蚀、塌陷共同形成的封闭低地。",
          position: [0.55, 0.2, 0.25],
        },
        {
          id: "cave",
          label: "溶洞系统",
          description: "地下水溶蚀并沿裂隙扩大的地下空腔。",
          position: [2.25, 0.95, 1.15],
        },
      ],
    },
    desert: {
      modelType: "desert",
      cameraPosition: [7.5, 4.6, 8.5],
      hotspots: [
        {
          id: "dune",
          label: "沙丘",
          description: "风搬运砂粒并在风速降低处堆积形成的丘状体。",
          position: [-1.4, 1.12, -0.2],
        },
        {
          id: "windward",
          label: "迎风坡",
          description: "受风吹拂、坡度较缓的一侧。",
          position: [-2.35, 0.62, 0.25],
        },
        {
          id: "slipface",
          label: "背风坡",
          description: "砂粒滑落堆积、坡度较陡的一侧。",
          position: [-0.52, 0.75, 0.45],
        },
      ],
    },
    coastal: {
      modelType: "coastal",
      cameraPosition: [7.7, 4.8, 8.5],
      hotspots: [
        {
          id: "shore",
          label: "岸线",
          description: "陆地与海水相互作用、位置随沉积和侵蚀变化的边界。",
          position: [0.05, 0.28, -0.2],
        },
        {
          id: "beach",
          label: "海滩",
          description: "波浪与沿岸流搬运、堆积砂砾形成的缓坡地带。",
          position: [-1.8, 0.35, 0.85],
        },
        {
          id: "cliff",
          label: "海蚀崖",
          description: "波浪侵蚀陡岸底部后，崖壁后退形成的陡坡。",
          position: [2.65, 1.3, -0.65],
        },
      ],
    },
    structural: {
      modelType: "structural",
      cameraPosition: [7.5, 5.3, 8.2],
      hotspots: [
        {
          id: "fault",
          label: "断层",
          description: "岩层或地块沿破裂面发生相对位移的构造。",
          position: [0, 1.05, 0],
        },
        {
          id: "scarp",
          label: "断层崖",
          description: "断层位移造成的明显陡坡或崖壁。",
          position: [1.1, 1.7, -0.35],
        },
        {
          id: "rift",
          label: "断陷谷",
          description: "两侧断裂下沉或拉张形成的狭长低地。",
          position: [-1.35, 0.45, 0.7],
        },
      ],
    },
    "mass-movement": {
      modelType: "mass-movement",
      cameraPosition: [7.5, 5.3, 8.1],
      hotspots: [
        {
          id: "head",
          label: "滑坡后缘",
          description: "物质开始脱离原坡面的上部区域。",
          position: [-1.7, 2.3, -0.45],
        },
        {
          id: "slide",
          label: "滑动体",
          description: "沿滑面向下移动的土体或岩体。",
          position: [-0.25, 1.35, 0.1],
        },
        {
          id: "toe",
          label: "堆积前缘",
          description: "滑动或崩落物质在坡脚停止并堆积的部位。",
          position: [1.9, 0.46, 0.65],
        },
      ],
    },
  };
