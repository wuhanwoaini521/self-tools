/**
 * 真实地形观测区配置。
 *
 * 「真实地形」模式用 MapLibre + Mapterhorn DEM 展示某一条地貌知识条目对应的
 * 真实地理位置（四川盆地、青藏高原、喜马拉雅……），并给出「在这片真实地貌里看什么」。
 * 以知识条目 id 为键，与 landformKnowledgeData.ts 的 entry.id 一一对应。
 */

export interface RealTerrainRegion {
  /** 对应知识条目 id（landformKnowledgeData.entry.id） */
  entryId: string;
  /** 观测区地名 */
  place: string;
  /** 中心点 [经度, 纬度]（WGS84） */
  center: [number, number];
  /** 初始缩放级别 */
  zoom: number;
  /** 初始俯仰角（0 = 俯视，>0 显示 3D 地形） */
  pitch: number;
  /** 初始方位角 */
  bearing: number;
  /** 标题中的副题：这片真实地貌如何体现了条目特征 */
  focus: string;
  /** 观察指导：在地图上具体看什么地方、什么形态 */
  observation: string;
  /** 一个可对照的概念要点（与 3D 程序化模型呼应） */
  cue: string;
}

const region = (r: RealTerrainRegion) => r;

/**
 * 覆盖全部知识条目的真实地形观测区。坐标与缩放选取自各真实地物的概略范围，
 * 供用户在 Mapterhorn DEM 上观察真实形态。未收录的条目会回退到通用陆地视图。
 */
export const REAL_TERRAIN: Record<string, RealTerrainRegion> = {
  // —— 构造与地形骨架 ——
  mountains: region({
    entryId: "mountains",
    place: "喜马拉雅山脉 · 中国 / 尼泊尔",
    center: [84, 28.2],
    zoom: 4.6,
    pitch: 60,
    bearing: -20,
    focus: "连续高地的真实体现：在地形渲染中看高差巨大的山脊与深处河谷。",
    observation:
      "先看东西走向的连续高地脊线，再沿南北方向对比：北侧青藏高原整体抬升，南侧恒河平原低平，高差在很短距离内骤变。用放大与旋转观察深切河谷和雪峰连绵。",
    cue: "概念模型里的「山脊—山谷—坡面」，在喜马拉雅用足量的高差真实呈现。",
  }),
  hills: region({
    entryId: "hills",
    place: "江南丘陵 · 中国东南",
    center: [116.8, 27.6],
    zoom: 6.8,
    pitch: 55,
    bearing: -30,
    focus: "相对高差较小的起伏高地：与山脉不同，丘陵更圆缓。",
    observation:
      "将俯仰角调到较高观察丘陵的圆缓峰顶与和缓坡面，再与邻近山地比较：丘陵等高线更宽、起伏更小，地表常见农田与河网。",
    cue: "概念里的「低山·圆丘·坡地」，在江南丘陵能看到平缓绵延的大片低丘。",
  }),
  plateau: region({
    entryId: "plateau",
    place: "青藏高原 · 中国西部",
    center: [88, 33],
    zoom: 4.3,
    pitch: 55,
    bearing: -12,
    focus: "海拔高而范围广阔的台地：边缘陡降、顶部相对平缓。",
    observation:
      "从整体低机位看高原这条巨大的高平面：表面大范围平缓起伏，而向周边（塔里木盆地、印度平原）骤然下降。放大看中部的河谷切割与湖泊。",
    cue: "概念模型里的「高原面—高原边缘—河谷」，在青藏高原以公里级尺度真实上演。",
  }),
  basin: region({
    entryId: "basin",
    place: "四川盆地 · 中国西南",
    center: [105.2, 30.5],
    zoom: 5.5,
    pitch: 58,
    bearing: -18,
    focus: "四周高、中部低的地形容器：周缘山地环绕，中部低平。",
    observation:
      "先找到环绕四周的山地边缘（北有米仓山、西有邛崃山、东有巫山），再观察中部低平原。成都平原位于盆地西部，河流与城镇沿低地分布。",
    cue: "概念里的「盆地边缘—周缘山地—盆底」，四川盆地四周高中央低一目了然。",
  }),
  "rift-valley": region({
    entryId: "rift-valley",
    place: "东非大裂谷 · 肯尼亚段",
    center: [36.2, -1.2],
    zoom: 6.2,
    pitch: 55,
    bearing: -15,
    focus: "拉张断裂形成的狭长低地：两侧断崖、中间沉陷。",
    observation:
      "沿裂谷走向观察两条近乎平行的陡坡边界，谷底相比两侧高地下沉，常出现湖泊（如奈瓦沙湖）。用较高俯仰角沿谷地拖动，能清楚看到地堑的低平谷底。",
    cue: "概念里的「断层·断陷谷」，在非洲大地留下纵贯数千公里的真实裂痕。",
  }),
  escarpment: region({
    entryId: "escarpment",
    place: "太行山大断崖 · 中国华北",
    center: [113.6, 36],
    zoom: 6.5,
    pitch: 55,
    bearing: 30,
    focus: "地块位移与差异侵蚀形成的陡坡：一面断崖、一面缓升。",
    observation:
      "观察华北平原向太行山过渡的这条陡峭崖线：东侧平原低平，西侧山地快速抬升。沿崖线拖动能看到山前陡坡与河谷出口的冲积地带。",
    cue: "概念里的「断层崖·陡坡」，在太行山前平原与高原之间形成醒目高差。",
  }),
  mesa: region({
    entryId: "mesa",
    place: "纪念碑谷 · 美国犹他 / 亚利桑那",
    center: [-110.05, 37.0],
    zoom: 8.5,
    pitch: 52,
    bearing: -40,
    focus: "顶平、边陡的残余高地：水平岩层被侵蚀留下的方山。",
    observation:
      "从侧面视角观察孤立台地的平顶与四周陡落边缘，能明显看出顶部比周围地面高出一截。它们是水平岩层被差异侵蚀后保留下来的残余高地。",
    cue: "概念里的「台地·残丘·方山」，在纪念碑谷以标志性方山群呈现。",
  }),

  // —— 火山地貌 ——
  volcano: region({
    entryId: "volcano",
    place: "富士山 · 日本本州",
    center: [138.72, 35.36],
    zoom: 8.8,
    pitch: 58,
    bearing: -20,
    focus: "喷发物围绕火山口堆积的锥状体：对称陡峭的圆锥形。",
    observation:
      "找到孤立的富士山：从低机位看它呈对称圆锥，顶部有凹陷。可放大观察其放射状沟谷与山体周围的火山碎屑堆积。",
    cue: "概念里的「火山锥·火山口」，富士山是教科书级的对称锥体。",
  }),
  stratovolcano: region({
    entryId: "stratovolcano",
    place: "富士山 · 日本本州",
    center: [138.72, 35.36],
    zoom: 8.8,
    pitch: 58,
    bearing: -20,
    focus: "熔岩与火山碎屑交替叠置的陡峭圆锥山。",
    observation:
      "富士山的对称陡峭外形正是层状火山的典型：多个喷发期把熔岩层与火山碎屑层反复叠置。低机位观察坡度的陡峭与山顶火山口的凹陷。",
    cue: "概念里的「层理·复合火山」，在富士山读出多次喷发的历史叠层。",
  }),
  "shield-volcano": region({
    entryId: "shield-volcano",
    place: "冒纳罗亚火山 · 美国夏威夷大岛",
    center: [-155.58, 19.48],
    zoom: 8.5,
    pitch: 50,
    bearing: -15,
    focus: "由低黏度熔岩铺展形成的宽缓火山：低而宽广。",
    observation:
      "对比夏威夷大岛上的火山：冒纳罗亚基底极宽、坡度平缓，不像富士山那样陡峭。压低视角看它向四周铺展的宽阔熔岩坡面。",
    cue: "概念里的「玄武岩·低黏度·熔岩台地」，在盾状火山读出缓坡的成因。",
  }),
  caldera: region({
    entryId: "caldera",
    place: "长白山天池 · 中国吉林",
    center: [128.06, 42.0],
    zoom: 9.2,
    pitch: 58,
    bearing: -10,
    focus: "喷发或塌陷形成的圆形大型凹陷：环状边缘内是火山湖。",
    observation:
      "找到天池的圆形湖盆：四周环状陡坡是破火山口塌陷后留下的边界，中央的低洼被湖水占据。这种圆形凹陷无法由普通河流侵蚀形成。",
    cue: "概念里的「塌陷·环状·火山湖」，长白山天池把破火山口钉在地图上。",
  }),
  "lava-plateau": region({
    entryId: "lava-plateau",
    place: "德干高原 · 印度西部",
    center: [75.5, 18.5],
    zoom: 5.2,
    pitch: 50,
    bearing: -20,
    focus: "裂隙喷发熔岩广泛覆盖形成的高平地与台地。",
    observation:
      "德干高原是印度板块上一片由玄武岩熔岩层层覆盖的高地：表面相对平坦，西缘被阶梯状悬崖（西高止山脉）切割，河流向东切割出台地。",
    cue: "概念里的「裂隙喷发·玄武岩·台地」，德干高原卷帙浩繁地盖满熔岩。",
  }),
  "island-arc": region({
    entryId: "island-arc",
    place: "日本列岛 · 西太平洋",
    center: [138, 36.2],
    zoom: 4.0,
    pitch: 52,
    bearing: -25,
    focus: "俯冲带上方呈弧形分布的火山岛链与海沟。",
    observation:
      "从高空看日本列岛沿太平洋一侧呈弧形排列，东北侧海沟与岛弧平行。放大观察海沟方向的深水凹槽与弧上成排的火山山脉。",
    cue: "概念里的「海沟·俯冲·火山岛」，日本列岛是太平洋俯冲带的真实弧线。",
  }),

  // —— 流水地貌 ——
  plain: region({
    entryId: "plain",
    place: "长江中下游平原 · 中国",
    center: [116, 30.5],
    zoom: 5.6,
    pitch: 45,
    bearing: -15,
    focus: "低平而沉积物深厚的广阔地带：等高线稀疏、河网密集。",
    observation:
      "这片平原海拔很低、起伏极小，河流与湖泊密布，农田与城镇连续分布。与周边山地对比能清楚看出「平」——植被、水系与聚落都在低地铺开。",
    cue: "概念里的「低平·河网·冲积」，长江中下游平原把平字写到极致。",
  }),
  river: region({
    entryId: "river",
    place: "长江 · 中国",
    center: [112, 30],
    zoom: 4.4,
    pitch: 40,
    bearing: -10,
    focus: "从高原到平原的整条侵蚀—搬运—沉积序列。",
    observation:
      "沿长江从西向东拖动：上游高原峡谷下切、中游江汉平原侧蚀、下游三角洲沉积。一条河把高、盆、平、海连成完整地貌序列。",
    cue: "概念里的「侵蚀·搬运·沉积·河道」，长江是贯穿全程的样板。",
  }),
  canyon: region({
    entryId: "canyon",
    place: "雅鲁藏布大峡谷 · 中国西藏",
    center: [95.1, 29.6],
    zoom: 7.6,
    pitch: 58,
    bearing: -25,
    focus: "河流强烈下切形成的狭深谷地：底窄壁陡。",
    observation:
      "放大南迦巴瓦峰附近，雅鲁藏布江在此突然下切形成深谷，两侧山体壁立。用低机位沿峡谷拖动，体会河水的下切力与谷壁的陡峭。",
    cue: "概念里的「下切·峡谷·V形谷」，大峡谷让一条河切开世界屋脊。",
  }),
  meander: region({
    entryId: "meander",
    place: "荆江河段 · 长江中游",
    center: [112.4, 30.25],
    zoom: 8.6,
    pitch: 48,
    bearing: -20,
    focus: "平缓河段侧蚀与沉积塑造的连续弯曲河道。",
    observation:
      "荆江河段以「九曲回肠」闻名：从高空看长江画出连续而夸张的 S 形弯曲。凹岸侵蚀、凸岸堆积，放大可见沙洲与旧河道遗迹（牛轭湖）。",
    cue: "概念里的「凹岸·凸岸·裁弯取直」，荆江是曲流最生动的课堂。",
  }),
  "floodplain-terrace": region({
    entryId: "floodplain-terrace",
    place: "黄河中游 · 中国",
    center: [110.3, 36.7],
    zoom: 6.8,
    pitch: 52,
    bearing: -20,
    focus: "河漫滩与旧河谷底的阶梯：低平坦面与更高台阶。",
    observation:
      "黄河中游河谷两侧常见分级的平坦台阶：紧邻河道的低漫滩与位置更高的阶地。放大河谷剖面，能隐约读出地壳抬升或基准面下降留下的多级阶地。",
    cue: "概念里的「洪水·阶地·下切」，黄河阶地记录了河流的反复下切。",
  }),
  "alluvial-fan": region({
    entryId: "alluvial-fan",
    place: "天山北麓 · 中国新疆",
    center: [87.6, 43.9],
    zoom: 8.2,
    pitch: 55,
    bearing: -25,
    focus: "山口处呈扇形展开的沉积体：从山口向外散开。",
    observation:
      "在乌鲁木齐以南的天山北麓，河流一出山口便向准噶尔盆地散开，洪积扇呈扇形铺展。放大山前地带能找到以山口为顶点向外展开的扇形沉积。",
    cue: "概念里的「山口·扇形·砾石」，天山北麓是山前冲积扇的连绵现场。",
  }),
  delta: region({
    entryId: "delta",
    place: "长江三角洲 · 中国",
    center: [121.2, 31.2],
    zoom: 7.8,
    pitch: 45,
    bearing: -10,
    focus: "河流入海处形成的沉积平原：多汊河道与湿地。",
    observation:
      "长江在此入海，泥沙堆出低平的三角洲平原：河汊、沙洲与滩涂（崇明岛）向海推进，市内河网密集。与西侧丘陵对比，三角洲明显低平。",
    cue: "概念里的「河口·分汊·沉积」，长江三角洲把泥沙堆成繁华平原。",
  }),
  waterfall: region({
    entryId: "waterfall",
    place: "黄果树瀑布 · 中国贵州",
    center: [105.66, 25.99],
    zoom: 9.0,
    pitch: 58,
    bearing: -15,
    focus: "河床高差突变形成的急落水流：硬岩陡坎的跌水。",
    observation:
      "黄果树位于喀斯特高原边缘，河流在骤降的岩坎上跌落。放大可见上游平缓河段之后突然的陡降与下方的深潭，是软硬岩差异侵蚀的典型。",
    cue: "概念里的「跌坎·侵蚀·深潭」，黄果树用一条白练标记了河床突变。",
  }),

  // —— 冰川地貌 ——
  glacier: region({
    entryId: "glacier",
    place: "海螺沟冰川 · 贡嘎山 · 中国",
    center: [102.0, 29.58],
    zoom: 8.8,
    pitch: 58,
    bearing: -20,
    focus: "积雪压实、缓慢流动的大型冰体：高山冰雪覆盖。",
    observation:
      "在贡嘎山东坡观察海螺沟冰川：高处是终年积雪的积累区，冰舌沿谷地向下延伸，前缘可见冰碛与冰舌前缘堆积。DEM 上冰雪区与裸岩边界清晰。",
    cue: "概念里的「积累·冰舌·消融」，贡嘎山的现代冰川正处在动态平衡中。",
  }),
  "u-valley-fjord": region({
    entryId: "u-valley-fjord",
    place: "松恩峡湾 · 挪威",
    center: [6.5, 61.1],
    zoom: 8.2,
    pitch: 55,
    bearing: -25,
    focus: "冰川加宽、削深形成的宽谷，被海水淹没后形成峡湾。",
    observation:
      "松恩峡湾是世界上最长的峡湾：冰川曾把河谷削成又宽又深的 U 形，海平面上升后海水灌入。看到狭长、两侧陡峭、延伸入陆地的海湾即是峡湾。",
    cue: "概念里的「U形·峡湾·悬谷」，斯堪的纳维亚的峡湾是冰蚀的经典。",
  }),
  cirque: region({
    entryId: "cirque",
    place: "少女峰地区 · 瑞士阿尔卑斯",
    center: [7.9, 46.55],
    zoom: 8.8,
    pitch: 58,
    bearing: -20,
    focus: "山地上部碗状的冰川侵蚀洼地：三面陡、一面开口。",
    observation:
      "在少女峰、艾格峰一带的高山脊线附近寻找半圆形凹地：三面被陡峭冰斗壁围住，一面朝低处开口。放大山肩位置常能看到一个个呆状的冰斗。",
    cue: "概念里的「碗状·斗壁·冰斗湖」，阿尔卑斯山脊沿线密布冰斗。",
  }),
  "arete-horn": region({
    entryId: "arete-horn",
    place: "马特洪峰 · 瑞士阿尔卑斯",
    center: [7.66, 45.98],
    zoom: 8.8,
    pitch: 55,
    bearing: -20,
    focus: "多方向冰川切割保留的尖锐山脊与金字塔形角峰。",
    observation:
      "马特洪峰就是一座教科书级角峰：多个方向的冰川从四面侵蚀，只剩下中间最坚硬的尖峰。从侧面看它呈陡峭的四棱锥，四周山脊窄锐。",
    cue: "概念里的「山脊·角峰·围蚀」，马特洪峰把角峰刻成标志性符号。",
  }),
  moraine: region({
    entryId: "moraine",
    place: "阿拉斯加冰川带 · 美国",
    center: [-148.7, 61.0],
    zoom: 7.8,
    pitch: 52,
    bearing: -20,
    focus: "冰川搬运碎屑留下的堆积地形：终碛与侧碛。",
    observation:
      "在阿拉斯加中南部的谷冰川前，常能看到冰舌前缘堆积的弧形终碛垄和沿谷壁的侧碛垄。放大冰川舌末端与冰前湖，弧状堆积体清晰可辨。",
    cue: "概念里的「冰川沉积·终碛」，阿拉斯加把堆碛画在每一道冰舌前。",
  }),
  "drumlin-esker": region({
    entryId: "drumlin-esker",
    place: "芬兰湖沼区 · 南博滕",
    center: [23.5, 62.5],
    zoom: 7.6,
    pitch: 48,
    bearing: -15,
    focus: "冰下沉积形成的流线丘与蜿蜒砂砾脊。",
    observation:
      "芬兰曾是冰盖覆盖区：从高空看，镜面般的湖泊之间点缀着长椭圆形的流线丘（鼓丘）和蜿蜒的砂砾脊（蛇丘），长轴指向冰流方向。",
    cue: "概念里的「冰下·融水·流线·砂砾脊」，芬兰的鼓丘群保留着冰流的指纹。",
  }),

  // —— 岩溶地貌 ——
  karst: region({
    entryId: "karst",
    place: "桂林喀斯特 · 中国广西",
    center: [110.3, 24.8],
    zoom: 8.8,
    pitch: 52,
    bearing: -20,
    focus: "可溶岩被水溶蚀形成的地表与地下系统：峰林与洼地。",
    observation:
      "漓江两岸成片锥状、塔状孤峰与峰丛拔地而起，其间是低洼的岩溶平原与农田。这是石灰岩被水溶蚀后留下的最直观的地表形态。",
    cue: "概念里的「溶蚀·落水洞·峰林」，桂林把喀斯特的峰林美学推向极致。",
  }),
  cave: region({
    entryId: "cave",
    place: "织金洞 · 中国贵州",
    center: [105.9, 26.66],
    zoom: 8.8,
    pitch: 52,
    bearing: -15,
    focus: "地下水沿裂隙溶蚀扩大的空腔与水道（地下）。",
    observation:
      "织金洞位于黔西可溶岩高原内部，地表看只是普通的丘陵，真正的空腔在地下。这是「地下喀斯特」——入口处的地表水流从这里转入地下河系统。",
    cue: "概念里的「地下河·溶洞」，织金洞提醒你地表之下另有一个溶蚀世界。",
  }),
  sinkhole: region({
    entryId: "sinkhole",
    place: "奉节小寨天坑 · 中国重庆",
    center: [109.46, 30.9],
    zoom: 9.2,
    pitch: 55,
    bearing: -15,
    focus: "溶蚀或塌陷形成的封闭低地：地表凹陷、无出水口。",
    observation:
      "放大奉节一带的喀斯特高原，可找到小寨天坑这类巨大的近圆形封闭凹陷：四周壁立、坑底低于周围地面，地表水由落水洞转入地下。",
    cue: "概念里的「塌陷·封闭洼地」，小寨天坑是地球表面撕开的一道溶蚀巨口。",
  }),
  "tower-karst": region({
    entryId: "tower-karst",
    place: "桂林阳朔 · 中国广西",
    center: [110.5, 24.78],
    zoom: 8.8,
    pitch: 50,
    bearing: -20,
    focus: "溶蚀洼地间保留下来的密集石灰岩孤峰与峰丛。",
    observation:
      "阳朔一带是峰林峰丛的密集区：一个个孤立或相连的石灰岩塔峰从低洼平原上拔起。从高空看，它们像棋盘一样散布，孤峰之间是被溶蚀低的洼地。",
    cue: "概念里的「孤峰·峰丛·塔状」，阳朔的峰林群是岩溶的立体词典。",
  }),
  "stone-forest": region({
    entryId: "stone-forest",
    place: "云南石林 · 中国云南",
    center: [103.33, 24.82],
    zoom: 9.2,
    pitch: 50,
    bearing: -15,
    focus: "裸露可溶岩表面被溶蚀雕刻的密集尖脊与石柱。",
    observation:
      "DEM 上云南石林表现为一片多刺的石灰岩表面：大量直立石柱、尖脊与沟槽在很小范围内密集分布。这是岩溶在「地表」尺度上的极端雕刻。",
    cue: "概念里的「裂隙·石柱·溶沟」，云南石林把喀斯特雕成了石锋森林。",
  }),

  // —— 风成与干旱地貌 ——
  dunes: region({
    entryId: "dunes",
    place: "塔克拉玛干沙漠 · 中国新疆",
    center: [83.5, 38.5],
    zoom: 5.6,
    pitch: 48,
    bearing: -15,
    focus: "风搬运砂粒堆积形成的丘状体：沙丘与沙海。",
    observation:
      "塔克拉玛干是世界第二大流动沙漠：从高空看，沙丘密集排列、随主导风呈有规律的方向。放大能看到新月形沙丘的迎风缓坡与背风陡坡。",
    cue: "概念里的「风成·迎风坡·背风坡」，塔克拉玛干把沙丘铺成浩瀚沙海。",
  }),
  yardang: region({
    entryId: "yardang",
    place: "罗布泊雅丹 · 中国新疆",
    center: [90.3, 40.1],
    zoom: 8.0,
    pitch: 52,
    bearing: -15,
    focus: "风蚀形成的平行脊与槽：长轴对齐主导风。",
    observation:
      "罗布泊一带的雅丹（「龙城」）由风沿较软地层磨蚀而成：成排平行的垄脊与沟槽，长轴方向与主导风一致。放大可见脊槽相间的定向纹理。",
    cue: "概念里的「风蚀·脊槽·干燥」，雅丹把风的方向刻进地表。",
  }),
  "mushroom-rock": region({
    entryId: "mushroom-rock",
    place: "柴达木盆地 · 中国青海",
    center: [95.0, 37.0],
    zoom: 7.0,
    pitch: 50,
    bearing: -20,
    focus: "近地面磨蚀较强形成的上宽下窄岩体。",
    observation:
      "柴达木盆地的干旱戈壁上常能见到风蚀蘑菇与风蚀柱：下部被含砂气流磨蚀得较细、上部较宽。这类形态藏在盆地中的露头与孤立岩块处。",
    cue: "概念里的「磨蚀·孤立岩·风化」，柴达木的风把岩石磨成蘑菇形。",
  }),
  loess: region({
    entryId: "loess",
    place: "黄土高原 · 中国",
    center: [109.0, 36.5],
    zoom: 5.8,
    pitch: 52,
    bearing: -15,
    focus: "风成粉砂堆积后被流水强烈切割的高地：塬梁峁。",
    observation:
      "黄土高原表面被流水切割成密集沟壑：塬（宽阔平坦的高原面）、梁（长条垄地）、峁（圆形丘）在放大后清晰可辨。黄土是风成的沉积物。",
    cue: "概念里的「风成粉砂·塬·梁·峁」，黄土地貌是风水共同雕刻的作品。",
  }),
  playa: region({
    entryId: "playa",
    place: "察尔汗盐湖 · 中国青海",
    center: [94.9, 36.9],
    zoom: 8.2,
    pitch: 45,
    bearing: -10,
    focus: "内流盆地蒸发浓缩形成的低平盐壳。",
    observation:
      "察尔汗盐湖位于封闭的柴达木盆地低处：地表极平、颜色浅，雨季可有薄水、干季留下大片白色盐壳。与四周相对起伏的地形明显反差。",
    cue: "概念里的「内流盆地·蒸发·盐壳」，察尔汗把水浓缩成盐的平原。",
  }),
  wadi: region({
    entryId: "wadi",
    place: "河西走廊 · 中国甘肃",
    center: [99.5, 39.0],
    zoom: 7.0,
    pitch: 50,
    bearing: -15,
    focus: "干旱区暴雨径流快速切割的干谷与冲沟。",
    observation:
      "河西走廊的祁连山北麓有大量干谷与冲沟：平时无水，暴雨时却会形成猛烈的洪流并携带碎屑。放大山前可见放射状或线状的干沟床。",
    cue: "概念里的「暴雨·径流·侵蚀沟」，河西走廊的干谷是间歇性河道的教科书。",
  }),

  // —— 海岸与海洋地貌 ——
  coast: region({
    entryId: "coast",
    place: "浙江海岸 · 中国东南",
    center: [121.6, 29.0],
    zoom: 7.2,
    pitch: 45,
    bearing: -15,
    focus: "波浪、潮汐、沿岸流与海平面共同塑造的海陆边缘。",
    observation:
      "浙江是典型的基岩岬湾海岸：曲折的岸线由岬角与海湾交替构成，岛屿星罗棋布。放大可见礁石岸段与泥沙堆积的局部海滩，是侵蚀与沉积并存的岸。",
    cue: "概念里的「波浪·沿岸流·岸线」，浙江海岸把岬湾写得跌宕起伏。",
  }),
  beach: region({
    entryId: "beach",
    place: "北戴河海滩 · 中国河北",
    center: [119.5, 39.8],
    zoom: 8.8,
    pitch: 45,
    bearing: -10,
    focus: "波浪分选砂砾形成的缓坡堆积带：连续沙质岸。",
    observation:
      "北戴河是连续平缓的沙质海滩：波浪把砂砾搬运、淘洗并在近岸堆积，留下宽缓的滩面与清晰的潮线。与基岩海岸的陡岸形成对比。",
    cue: "概念里的「砂砾·波浪·潮线」，北戴河的海滩是沉积型海岸的代表。",
  }),
  "sea-cliff": region({
    entryId: "sea-cliff",
    place: "大洋路 · 澳大利亚维多利亚",
    center: [143.2, -38.7],
    zoom: 8.0,
    pitch: 50,
    bearing: -20,
    focus: "波浪侵蚀岩岸形成的陡壁、平台与海岬。",
    observation:
      "大洋路的十二使徒一带是著名的海蚀崖海岸：波浪冲蚀崖脚，崖壁后退，留下海蚀平台与孤立的海蚀柱。放大可见峭壁与向海突出的岬角。",
    cue: "概念里的「侵蚀岸·海蚀洞·海蚀柱」，大洋路把海蚀过程立体呈现。",
  }),
  "spit-tombolo": region({
    entryId: "spit-tombolo",
    place: "大连金石滩 · 中国辽宁",
    center: [121.5, 39.0],
    zoom: 8.5,
    pitch: 48,
    bearing: -15,
    focus: "沿岸漂沙堆积形成的狭长沙体：沙嘴与连岛沙洲。",
    observation:
      "在辽东半岛南岸观察沿岸漂移形成的沙嘴与连岛沙洲：狭长的沙体从岸边伸出或把岛与大陆连起来，放大可见其与波浪方向相关的走向。",
    cue: "概念里的「沿岸漂沙·沙洲·砂嘴」，金石滩的连岛沙洲是沉积滨岸的注脚。",
  }),
  "barrier-lagoon": region({
    entryId: "barrier-lagoon",
    place: "外滩群岛 · 美国北卡罗来纳",
    center: [-75.5, 35.2],
    zoom: 7.8,
    pitch: 45,
    bearing: -10,
    focus: "近岸沙体与受遮蔽水域组成的系统：堡岛与潟湖。",
    observation:
      "沿美国东海岸看外滩群岛：一排狭窄的堡岛（沙坝）把外海与内侧平静的潟湖隔开。放大能看到几乎平行于海岸的沙体与其后的浅水水域。",
    cue: "概念里的「堡岛·沙坝·潟湖」，外滩群岛把屏障岛链摆得天衣无缝。",
  }),
  estuary: region({
    entryId: "estuary",
    place: "钱塘江河口 · 中国浙江",
    center: [120.3, 30.2],
    zoom: 8.0,
    pitch: 45,
    bearing: -10,
    focus: "淡水与海水交汇、受潮汐影响的喇叭形河口。",
    observation:
      "钱塘江入海口呈喇叭形，受强劲潮汐影响形成著名涌潮：海水随潮汐进出入海口，河口岸线随海平面与沉积不断变化。放大可见滩涂与潮汐通道。",
    cue: "概念里的「潮汐·盐沼·泥滩」，钱塘江河口把潮汐动能写到极致。",
  }),

  // —— 重力与坡地地貌 ——
  landslide: region({
    entryId: "landslide",
    place: "金沙江白格滑坡 · 中国西藏 / 四川",
    center: [98.9, 31.1],
    zoom: 8.8,
    pitch: 55,
    bearing: -20,
    focus: "土体或岩体沿滑面整体下移：坡体失稳留下的后缘与堆积。",
    observation:
      "2018 年白格滑坡堵塞金沙江：滑坡后缘留下陡坎与错落台阶，滑舌堆积在江边，河道被局部阻塞改道。放大山体坡面可辨认后缘与新堆积体。",
    cue: "概念里的「滑面·后缘·堆积前缘」，白格滑坡把坡体失稳写进河谷。",
  }),
  rockfall: region({
    entryId: "rockfall",
    place: "三峡库区 · 中国重庆",
    center: [108.8, 30.9],
    zoom: 8.2,
    pitch: 52,
    bearing: -20,
    focus: "陡崖岩体快速脱离并坠落：崖壁剥落与坡脚块石。",
    observation:
      "三峡库区两岸多为陡峭岩壁，风化、冻融与库水位波动常诱发崩塌：崖壁可见新鲜剥落面，坡脚堆积棱角分明的块石。放大高边坡可辨认危岩带。",
    cue: "概念里的「落石·崖壁·块石」，三峡两岸把崩塌辩识摆在眼前。",
  }),
  talus: region({
    entryId: "talus",
    place: "阿尔卑斯山岩屑坡 · 欧洲",
    center: [7.8, 45.95],
    zoom: 8.0,
    pitch: 52,
    bearing: -20,
    focus: "崩塌物在坡脚堆积形成的锥状或裙状地形。",
    observation:
      "在阿尔卑斯的高山陡崖下常可见岩屑坡与崩积锥：从崖脚向山麓延伸出较均一的锥状或裙状砾石堆积，表面坡度接近岩屑的安息角。",
    cue: "概念里的「坡脚·碎屑·安息角」，阿尔卑斯的崩积锥堆在每道崖脚下。",
  }),
};

/** 未收录条目的通用回退：展示中国西南多山地形。 */
export const REAL_TERRAIN_FALLBACK: RealTerrainRegion = region({
  entryId: "_fallback",
  place: "中国西南多山地区 · 通用视图",
  center: [103.0, 28.0],
  zoom: 5.0,
  pitch: 55,
  bearing: -15,
  focus: "真实地形的通用观测视图。",
  observation:
    "从这张真实地形图出发，观察山地、高原、河谷之间的过渡与高差，再把看到的形态与「概念地形」的 3D 模型对照。",
  cue: "真实地形把概念模型里的形态还原到真实尺度。",
});

export function getRealTerrain(entryId: string): RealTerrainRegion {
  return REAL_TERRAIN[entryId] ?? REAL_TERRAIN_FALLBACK;
}
