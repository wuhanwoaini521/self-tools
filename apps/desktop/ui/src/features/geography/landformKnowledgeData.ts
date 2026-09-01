import type { LandformModelType } from "./landform3dData";

export interface LandformKnowledgeEntry {
  id: string;
  category: string;
  title: string;
  subtitle: string;
  intro: string;
  formation: string;
  identify: string;
  example: string;
  exampleId: string;
  viewerType: LandformModelType;
  keywords: string[];
}

const item = (entry: LandformKnowledgeEntry) => entry;

/**
 * 面向学习的主要地貌目录：按主导营力组织，而非试图罗列无限的地方性命名。
 * 每项都可搜索，并关联一个可交互的程序化教学场景。
 */
export const LANDFORM_KNOWLEDGE_ENTRIES: LandformKnowledgeEntry[] = [
  item({ id: "mountains", category: "构造与地形骨架", title: "山地与山脉", subtitle: "起伏强烈的连续高地系统", intro: "山地相对高差大、坡度明显；多个山地沿一定方向延伸并相连，构成山脉。", formation: "地壳挤压、褶皱、断裂和抬升塑造山地，风化与侵蚀持续刻出山脊和山谷。", identify: "等高线密集、连续高地和深切河谷是常见线索。", example: "喜马拉雅山脉", exampleId: "himalayas", viewerType: "mountains", keywords: ["山峰", "山脊", "山谷", "褶皱山", "断块山"] }),
  item({ id: "hills", category: "构造与地形骨架", title: "丘陵", subtitle: "相对高差较小、起伏和缓的高地", intro: "丘陵比山地低缓，峰顶常圆润，坡面连续而起伏不大。", formation: "可由长期侵蚀削低山地形成，也可由沉积或缓慢构造抬升形成。", identify: "等高线呈闭合但间距较宽，整体高差通常小于邻近山地。", example: "江南丘陵", exampleId: "china", viewerType: "mountains", keywords: ["低山", "圆丘", "坡地", "侵蚀"] }),
  item({ id: "plateau", category: "构造与地形骨架", title: "高原", subtitle: "海拔高而范围广阔的台地", intro: "高原整体海拔较高，表面可相对平缓，也可被河流深切。", formation: "大范围抬升、火山熔岩堆积或长期侵蚀保留高平面都能形成高原。", identify: "判断重点是成片的高海拔与边缘陡降，而不是单座高山。", example: "青藏高原", exampleId: "tibetan-plateau", viewerType: "plateau", keywords: ["台地", "高平面", "抬升", "河谷切割"] }),
  item({ id: "basin", category: "构造与地形骨架", title: "盆地", subtitle: "四周较高、中部较低的地形容器", intro: "盆地周缘通常是山地或高地，中部低平，易汇集水汽、河流与沉积物。", formation: "地壳沉降、断陷或周缘抬升可形成盆地，后续河流沉积会填平其内部。", identify: "边缘等高线密集而中心稀疏，水系常向中部汇集。", example: "四川盆地", exampleId: "sichuan-basin", viewerType: "basin", keywords: ["断陷", "盆底", "周缘山地", "沉积"] }),
  item({ id: "rift-valley", category: "构造与地形骨架", title: "裂谷与断陷谷", subtitle: "拉张断裂形成的狭长低地", intro: "裂谷是地壳受拉伸后，两侧断裂、中间相对下沉形成的长条状低地。", formation: "伸展构造使正断层两侧发生位移，低处可被湖泊、河流和火山活动占据。", identify: "在地形和遥感图上表现为平行的陡坡边界与长条低地。", example: "东非大裂谷", exampleId: "china", viewerType: "structural", keywords: ["地堑", "正断层", "拉张", "断陷"] }),
  item({ id: "escarpment", category: "构造与地形骨架", title: "陡崖与断层崖", subtitle: "地块位移或差异侵蚀形成的陡坡", intro: "陡崖是地形高度突然变化形成的陡峭坡面，断层崖是其常见构造成因。", formation: "断层位移直接形成陡坡；不同岩层抗蚀性差异也能保留崖壁。", identify: "等高线极密，沿一条较直或弧形边界集中分布。", example: "太行山大断崖", exampleId: "china", viewerType: "structural", keywords: ["断层", "崖壁", "陡坡", "地垒"] }),
  item({ id: "mesa", category: "构造与地形骨架", title: "桌山、孤峰与方山", subtitle: "顶平、边陡的残余高地", intro: "桌山和方山顶部较平、边坡陡峭；更小的孤峰常是其进一步侵蚀后的残余。", formation: "水平岩层中较坚硬的盖层保护下部软岩，侵蚀逐渐切割出孤立高地。", identify: "从侧面看顶部近水平，四周边缘明显陡落。", example: "美国纪念碑谷", exampleId: "china", viewerType: "plateau", keywords: ["台地", "残丘", "方山", "差异侵蚀"] }),

  item({ id: "volcano", category: "火山地貌", title: "火山锥", subtitle: "喷发物围绕火山口堆积的锥状体", intro: "火山锥是最直观的火山地貌，常由熔岩、火山灰和碎屑围绕喷口堆积而成。", formation: "多次喷发把物质堆在喷口周围，喷发方式决定锥体陡缓和层理。", identify: "圆锥形高地、顶端火山口和放射状沟谷是主要线索。", example: "富士山", exampleId: "japan", viewerType: "volcanic", keywords: ["火山口", "熔岩", "火山灰", "喷发"] }),
  item({ id: "stratovolcano", category: "火山地貌", title: "层状火山", subtitle: "熔岩与火山碎屑交替叠置的陡峭火山", intro: "层状火山由熔岩层和火山碎屑层反复叠加，外形常高大而坡陡。", formation: "较黏稠的岩浆与爆炸性喷发交替出现，形成层理明显的火山锥。", identify: "典型外形是对称陡峭的圆锥，常伴随火山灰、熔岩流和火山碎屑流。", example: "富士山", exampleId: "japan", viewerType: "volcanic", keywords: ["复合火山", "火山灰", "黏稠岩浆", "层理"] }),
  item({ id: "shield-volcano", category: "火山地貌", title: "盾状火山", subtitle: "由低黏度熔岩铺展形成的宽缓火山", intro: "盾状火山低而宽，外形像平放的盾牌，坡度远缓于层状火山。", formation: "低黏度玄武质熔岩流动距离远、层层铺展，逐渐堆成宽阔火山体。", identify: "看宽广基底、缓坡和大量薄层熔岩流，而非尖锐火山锥。", example: "夏威夷冒纳罗亚火山", exampleId: "japan", viewerType: "volcanic", keywords: ["玄武岩", "低黏度", "熔岩台地", "夏威夷"] }),
  item({ id: "caldera", category: "火山地貌", title: "火山口与破火山口", subtitle: "喷发或塌陷形成的圆形大型凹陷", intro: "火山口是喷发出口附近的凹陷；破火山口尺度更大，常由喷发后岩浆房顶部塌陷形成。", formation: "剧烈喷发排空部分岩浆后，上覆岩层失去支撑并塌陷，留下环状洼地。", identify: "圆形或椭圆形环状边缘、内部湖泊或新的火山锥都是常见特征。", example: "长白山天池", exampleId: "china", viewerType: "volcanic", keywords: ["塌陷", "环状", "火山湖", "岩浆房"] }),
  item({ id: "lava-plateau", category: "火山地貌", title: "熔岩台地与熔岩原", subtitle: "裂隙喷发熔岩广泛覆盖形成的平坦地", intro: "熔岩台地是大量流动性熔岩覆盖大片地表后形成的高平地或缓坡地。", formation: "裂隙式喷发可在短时间内铺展大面积玄武岩熔岩，冷却后形成层状岩台。", identify: "地表相对平坦、可见层状玄武岩和被河流切割的台地边缘。", example: "德干高原", exampleId: "china", viewerType: "plateau", keywords: ["裂隙喷发", "玄武岩", "熔岩流", "台地"] }),
  item({ id: "island-arc", category: "火山地貌", title: "岛弧与火山岛链", subtitle: "俯冲带上方呈弧形分布的火山岛", intro: "岛弧是沿板块边界排列的弧形岛屿或火山岛链，常与海沟、地震带相伴。", formation: "海洋板块俯冲后诱发上覆板块岩浆活动，长期形成一列火山岛。", identify: "地图上看岛链弯曲分布，并在外海一侧寻找深海沟。", example: "日本列岛", exampleId: "japan", viewerType: "island-arc", keywords: ["海沟", "俯冲", "火山岛", "板块"] }),

  item({ id: "plain", category: "流水地貌", title: "平原与冲积平原", subtitle: "低平而沉积物深厚的广阔地带", intro: "平原起伏小、坡度缓；河流长期搬运的泥沙常在中下游形成冲积平原。", formation: "流速降低时砂、粉砂和黏土逐步沉积，河道摆动不断改造平原表面。", identify: "等高线稀疏、河网密集，农田与城市常连续分布。", example: "长江中下游平原", exampleId: "shanghai", viewerType: "plain", keywords: ["冲积", "低平", "河网", "沉积"] }),
  item({ id: "river", category: "流水地貌", title: "河流地貌", subtitle: "侵蚀、搬运与沉积的连续结果", intro: "河流地貌随坡降和流量变化：上游多侵蚀，中下游多侧蚀和沉积。", formation: "水流通过侵蚀、搬运、沉积把高地、盆地、平原和海洋连成一条地貌序列。", identify: "同时观察流向、坡度、支流、弯曲河道和入海口。", example: "长江", exampleId: "yangtze", viewerType: "river", keywords: ["侵蚀", "搬运", "沉积", "河道"] }),
  item({ id: "canyon", category: "流水地貌", title: "V 形谷与峡谷", subtitle: "河流强烈下切形成的狭深谷地", intro: "V 形谷横剖面底窄壁陡；峡谷通常更深、更窄，岩壁更陡峭。", formation: "坡降大、下切强的河流不断侵蚀谷底，边坡风化和崩塌使谷壁后退。", identify: "等高线在河谷两侧极密，并向上游方向弯成 V 字。", example: "雅鲁藏布大峡谷", exampleId: "himalayas", viewerType: "river", keywords: ["下切", "河谷", "峡谷", "V形谷"] }),
  item({ id: "meander", category: "流水地貌", title: "曲流与牛轭湖", subtitle: "平缓河段侧蚀与沉积塑造的弯曲河道", intro: "曲流是河道在平原上形成的连续弯曲；被裁弯取直后的旧河湾可形成牛轭湖。", formation: "弯道凹岸流速快而侧蚀，凸岸流速慢而沉积，弯曲持续扩大并可能截弯。", identify: "俯视能看到 S 形河道、沙洲和孤立的弯月形水体。", example: "荆江河段", exampleId: "yangtze", viewerType: "river", keywords: ["凹岸", "凸岸", "裁弯取直", "牛轭湖"] }),
  item({ id: "floodplain-terrace", category: "流水地貌", title: "河漫滩与河流阶地", subtitle: "洪泛沉积面与旧河谷底的阶梯", intro: "河漫滩是洪水期被淹没的低平地；河流下切后，旧河漫滩可保留为阶地。", formation: "周期性洪水沉积细颗粒；基准面下降或地壳抬升会促使河流下切。", identify: "河道两侧低平面为河漫滩，更高处平坦台阶常是河流阶地。", example: "黄河中游河流阶地", exampleId: "yangtze", viewerType: "plain", keywords: ["洪水", "阶地", "冲积层", "下切"] }),
  item({ id: "alluvial-fan", category: "流水地貌", title: "冲积扇", subtitle: "山口处呈扇形展开的沉积体", intro: "冲积扇常位于山谷出口，河流一离开陡峭山地便向开阔平地散开并沉积。", formation: "坡度和流速骤降使砾石、砂和泥沙按粒径分选堆积，形成扇状地面。", identify: "从高处看像以山口为顶点向外展开的扇形，常有辫状分流。", example: "天山北麓冲积扇", exampleId: "china", viewerType: "plain", keywords: ["山口", "扇形", "砾石", "分流"] }),
  item({ id: "delta", category: "流水地貌", title: "三角洲", subtitle: "河流入海或入湖处形成的沉积平原", intro: "三角洲是河流携带的大量沉积物在河口水流减慢处堆积形成的低平地。", formation: "河流、波浪、潮汐共同控制沉积物的分布与三角洲前缘形态。", identify: "多汊河道、湿地和向水体推进的沉积陆地是关键线索。", example: "长江三角洲", exampleId: "shanghai", viewerType: "plain", keywords: ["河口", "分汊", "湿地", "沉积"] }),
  item({ id: "waterfall", category: "流水地貌", title: "瀑布与跌水", subtitle: "河床高差突变形成的急落水流", intro: "瀑布是水流越过陡坎的急剧跌落，跌水则规模较小。", formation: "软硬岩层差异侵蚀、断层或冰川遗留悬谷都可造成河床突变。", identify: "观察上游较平缓河段、垂直或陡倾落差与下方深潭。", example: "黄果树瀑布", exampleId: "china", viewerType: "river", keywords: ["跌坎", "侵蚀", "深潭", "悬谷"] }),

  item({ id: "glacier", category: "冰川地貌", title: "冰川", subtitle: "在重力下缓慢流动的大型冰体", intro: "冰川是长期积雪压实形成、能够缓慢流动的冰体，是强大的侵蚀和搬运营力。", formation: "高寒区积雪积累超过消融，雪晶压实成冰后沿谷地或冰盖向低处流动。", identify: "注意积累区、冰舌、冰裂缝及其前缘堆积物。", example: "海螺沟冰川", exampleId: "himalayas", viewerType: "glacial", keywords: ["冰舌", "冰裂缝", "积累", "消融"] }),
  item({ id: "u-valley-fjord", category: "冰川地貌", title: "U 形谷与峡湾", subtitle: "冰川加宽、削深形成的宽谷", intro: "U 形谷谷底宽平、两侧陡峭；海水淹没其下游后可形成峡湾。", formation: "冰川沿谷流动时同时磨蚀谷底和谷壁，侵蚀范围宽于河流。", identify: "横剖面近 U 字，常有悬谷；峡湾则狭长且两岸陡峭。", example: "挪威松恩峡湾", exampleId: "himalayas", viewerType: "glacial", keywords: ["谷冰川", "峡湾", "悬谷", "U形"] }),
  item({ id: "cirque", category: "冰川地貌", title: "冰斗", subtitle: "山地上部碗状的冰川侵蚀洼地", intro: "冰斗是高山冰川源头常见的半圆形凹地，三面陡峭、一面开口。", formation: "积雪与冰川在山坡凹处反复冻融、拔蚀和磨蚀，逐渐挖出盆状洼地。", identify: "在山脊附近寻找碗状凹地、斗壁与可能的冰斗湖。", example: "阿尔卑斯山冰斗", exampleId: "himalayas", viewerType: "glacial", keywords: ["碗状", "高山", "冰斗湖", "侵蚀"] }),
  item({ id: "arete-horn", category: "冰川地貌", title: "刃脊与角峰", subtitle: "多方向冰川切割保留的尖锐山脊和山峰", intro: "刃脊是两侧冰斗或冰川谷之间的狭窄山脊；角峰是多个冰斗围蚀后留下的尖峰。", formation: "冰川从不同方向向山体后退侵蚀，保留最坚硬的脊线和峰顶。", identify: "山脊狭窄锐利，山峰常呈金字塔状。", example: "马特洪峰", exampleId: "himalayas", viewerType: "glacial", keywords: ["山脊", "角峰", "冰斗", "围蚀"] }),
  item({ id: "moraine", category: "冰川地貌", title: "冰碛与终碛垄", subtitle: "冰川搬运碎屑留下的堆积地形", intro: "冰碛是冰川携带的未分选碎屑，在冰缘、两侧或冰下堆积形成的地貌。", formation: "冰川消融或停滞时，所携带的岩屑在原地卸载，终碛常标记冰川最大推进位置。", identify: "看弧形或条带状砾石土堆，常与冰川谷和冰前湖相邻。", example: "阿拉斯加冰川终碛", exampleId: "himalayas", viewerType: "glacial", keywords: ["冰川沉积", "终碛", "侧碛", "砾石"] }),
  item({ id: "drumlin-esker", category: "冰川地貌", title: "鼓丘与蛇丘", subtitle: "冰下沉积形成的流线丘与蜿蜒砂砾脊", intro: "鼓丘是冰下塑形的椭圆丘；蛇丘是冰下融水隧道沉积留下的长而曲折砂砾脊。", formation: "冰川底部的压力、沉积与融水流动共同将松散物质塑造成定向形态。", identify: "鼓丘成群且长轴指向冰流方向；蛇丘像蜿蜒堤坝。", example: "芬兰蛇丘群", exampleId: "himalayas", viewerType: "glacial", keywords: ["冰下", "融水", "流线", "砂砾脊"] }),

  item({ id: "karst", category: "岩溶地貌", title: "岩溶地貌", subtitle: "可溶岩被水溶蚀形成的地表与地下系统", intro: "岩溶地貌形成于石灰岩、白云岩等可溶岩地区，地表与地下排水相互连通。", formation: "含二氧化碳的水沿裂隙溶蚀岩石，并伴随坍塌、沉积和地下水作用。", identify: "洼地、落水洞、溶洞和峰林常成组合出现。", example: "桂林喀斯特", exampleId: "china", viewerType: "karst", keywords: ["石灰岩", "溶蚀", "地下河", "喀斯特"] }),
  item({ id: "cave", category: "岩溶地貌", title: "溶洞与地下河", subtitle: "地下水沿裂隙溶蚀扩大的空腔和水道", intro: "溶洞是可溶岩内部的空腔，常由地下河和裂隙水系连接。", formation: "地下水沿层面、节理或断裂持续溶蚀，水位变化后可形成多层洞穴。", identify: "洞内常有钟乳石、石笋和地下河，但这些是沉积物而非洞穴本身。", example: "织金洞", exampleId: "china", viewerType: "karst", keywords: ["地下河", "钟乳石", "石笋", "裂隙"] }),
  item({ id: "sinkhole", category: "岩溶地貌", title: "天坑、洼地与落水洞", subtitle: "溶蚀或塌陷形成的封闭低地", intro: "岩溶洼地与天坑是地表下凹形态；落水洞则是地表水进入地下的入口。", formation: "地下空腔扩大、顶板塌陷或地表持续溶蚀都会形成封闭低地。", identify: "等高线闭合且高程向内降低，常无地表出水口。", example: "重庆小寨天坑", exampleId: "china", viewerType: "karst", keywords: ["塌陷", "封闭洼地", "漏斗", "地下排水"] }),
  item({ id: "tower-karst", category: "岩溶地貌", title: "峰林与峰丛", subtitle: "岩溶洼地间保留下来的密集石灰岩山峰", intro: "峰丛峰林是热湿岩溶区的标志性地貌：前者峰体相连，后者更孤立分散。", formation: "溶蚀与地表水下渗不断扩大洼地，使残余石灰岩峰体变得突出。", identify: "从远处看有成片锥状、塔状孤峰与其间低洼地。", example: "桂林阳朔峰林", exampleId: "china", viewerType: "karst", keywords: ["孤峰", "峰丛", "塔状", "洼地"] }),
  item({ id: "stone-forest", category: "岩溶地貌", title: "石芽与石林", subtitle: "裸露可溶岩表面被溶蚀雕刻的尖脊", intro: "石芽和石林由裸露石灰岩被雨水与地下水沿裂隙切割，形成密集尖脊和石柱。", formation: "溶蚀先扩大岩石裂缝，再将岩块分隔、雕刻成尖锐形态。", identify: "地表可见成片直立石柱、沟槽与尖棱。", example: "云南石林", exampleId: "china", viewerType: "karst", keywords: ["石灰岩", "裂隙", "石柱", "溶沟"] }),

  item({ id: "dunes", category: "风成与干旱地貌", title: "沙丘", subtitle: "风搬运砂粒堆积形成的丘状体", intro: "沙丘是干旱或海岸地区常见的风成沉积地貌，形态受风向、风力和砂源控制。", formation: "风把砂粒推移、跃移和悬移，遇到障碍或风速降低时堆积成丘。", identify: "可辨认迎风缓坡与背风陡坡，沙脊方向反映主导风。", example: "塔克拉玛干沙漠", exampleId: "china", viewerType: "desert", keywords: ["风成", "新月形沙丘", "迎风坡", "背风坡"] }),
  item({ id: "yardang", category: "风成与干旱地貌", title: "雅丹", subtitle: "风蚀形成的平行脊与槽", intro: "雅丹是干旱区松散或半固结沉积物被风蚀后形成的长条状垄脊和沟槽。", formation: "主导风沿较软岩层或沉积层不断磨蚀、吹蚀，保留较坚硬部分。", identify: "脊槽成组平行排列，长轴通常与主导风方向一致。", example: "罗布泊雅丹", exampleId: "china", viewerType: "desert", keywords: ["风蚀", "脊槽", "干旱", "主导风"] }),
  item({ id: "mushroom-rock", category: "风成与干旱地貌", title: "风蚀蘑菇与风蚀柱", subtitle: "近地面磨蚀较强形成的上宽下窄岩体", intro: "风蚀蘑菇的下部常较细，上部较宽，形似蘑菇。", formation: "含砂气流靠近地面时磨蚀最强，软硬岩差异进一步强化这种外形。", identify: "看基部凹蚀、上部较宽的孤立岩块，并与裸露干旱环境相联系。", example: "柴达木风蚀地貌", exampleId: "china", viewerType: "desert", keywords: ["磨蚀", "砂粒", "孤立岩", "风化"] }),
  item({ id: "loess", category: "风成与干旱地貌", title: "黄土高原与黄土地貌", subtitle: "风成粉砂堆积后被流水切割的高地", intro: "黄土是以粉砂为主的风成沉积物；黄土高原表现为深厚黄土层和密集沟壑。", formation: "冰期或干冷环境提供细颗粒，风把它们搬运并沉积；后期流水强烈切割。", identify: "地表沟谷密集、坡面陡，常见塬、梁、峁等形态。", example: "中国黄土高原", exampleId: "china", viewerType: "desert", keywords: ["风成粉砂", "塬", "梁", "峁", "侵蚀沟"] }),
  item({ id: "playa", category: "风成与干旱地貌", title: "干盐湖与盐沼", subtitle: "内流盆地蒸发浓缩形成的低平盐壳", intro: "干盐湖位于封闭盆地低处，雨季可积水，干季因强烈蒸发留下盐壳。", formation: "河流把溶解盐分带入无外流盆地，水分蒸发后盐类反复沉淀。", identify: "地表极平、颜色浅，常有多边形干裂纹和白色盐壳。", example: "察尔汗盐湖", exampleId: "china", viewerType: "basin", keywords: ["内流盆地", "蒸发", "盐壳", "干裂"] }),
  item({ id: "wadi", category: "风成与干旱地貌", title: "干谷与冲沟", subtitle: "干旱区暴雨径流快速切割的沟谷", intro: "干谷平时无水或水量很小，但暴雨时可形成猛烈洪流；冲沟是坡面侵蚀形成的小沟。", formation: "植被稀少、土壤松散时，短时强降雨集中径流会迅速下切并搬运碎屑。", identify: "看无常流水的宽浅沟床、冲洪积物和沟头后退。", example: "河西走廊干谷", exampleId: "china", viewerType: "river", keywords: ["暴雨", "径流", "侵蚀沟", "冲洪积"] }),

  item({ id: "coast", category: "海岸与海洋地貌", title: "海岸地貌", subtitle: "波浪、潮汐、沿岸流与海平面共同塑造的边缘", intro: "海岸地貌是陆海交界处的侵蚀和沉积形态总称，变化速度常高于内陆地貌。", formation: "波浪侵蚀、潮汐水流、沿岸漂沙与海平面升降共同决定海岸形态。", identify: "先判断是岩石侵蚀岸还是砂质沉积岸，再看岬湾、海滩和沙坝。", example: "浙江海岸", exampleId: "china", viewerType: "coastal", keywords: ["波浪", "潮汐", "沿岸流", "岸线"] }),
  item({ id: "beach", category: "海岸与海洋地貌", title: "海滩", subtitle: "波浪分选砂砾形成的缓坡堆积带", intro: "海滩是从近岸浅水到陆地的砂砾沉积带，粒径和坡度受波浪能量控制。", formation: "波浪反复搬运、淘洗和分选砂砾，在能量较适中的岸段形成海滩。", identify: "沿岸连续的砂砾带、潮线和缓倾斜滩面是基本特征。", example: "北戴河海滩", exampleId: "china", viewerType: "coastal", keywords: ["砂砾", "波浪", "潮线", "堆积"] }),
  item({ id: "sea-cliff", category: "海岸与海洋地貌", title: "海蚀崖、海蚀平台与海岬", subtitle: "波浪侵蚀岩岸形成的陡壁与突出地带", intro: "海蚀崖是波浪侵蚀造成的陡壁；崖前常有较平坦的海蚀平台，坚硬岩体可形成海岬。", formation: "波浪集中冲蚀崖脚并形成凹槽，崖壁失稳后退，留下平台。", identify: "岩石海岸上可见陡直崖壁、波切凹槽和向海突出的岬角。", example: "澳大利亚大洋路", exampleId: "china", viewerType: "coastal", keywords: ["侵蚀岸", "海蚀洞", "海蚀柱", "海岬"] }),
  item({ id: "spit-tombolo", category: "海岸与海洋地貌", title: "沙嘴与连岛沙洲", subtitle: "沿岸漂沙堆积形成的狭长沙体", intro: "沙嘴从岸边向海伸出；连岛沙洲则把岛屿与大陆或另一岛屿连接起来。", formation: "沿岸流搬运砂砾，在海湾、岬角背后或波浪能量减弱处持续堆积。", identify: "遥感图上呈狭长带状、常与原岸线斜交或弯曲。", example: "大连金石滩连岛沙洲", exampleId: "china", viewerType: "coastal", keywords: ["沿岸漂沙", "沙洲", "岬角", "沉积岸"] }),
  item({ id: "barrier-lagoon", category: "海岸与海洋地貌", title: "堡岛、沙坝与潟湖", subtitle: "近岸沙体与受遮蔽水域组成的沉积系统", intro: "堡岛或沙坝将外海与内侧浅水隔开，内侧较平静的水域称为潟湖。", formation: "波浪和沿岸流把沉积物堆积在近岸浅海，海平面变化也会改变其位置。", identify: "平行于海岸的狭长沙体与其内侧封闭或半封闭水域成对出现。", example: "美国外滩群岛", exampleId: "china", viewerType: "coastal", keywords: ["堡岛", "沙坝", "潟湖", "近岸"] }),
  item({ id: "estuary", category: "海岸与海洋地貌", title: "河口、海湾与溺谷", subtitle: "淡水与海水交汇、受潮汐影响的水域", intro: "河口是河流入海处；海湾和溺谷是海水进入低地或被陆地环抱形成的水域。", formation: "海平面上升淹没河谷、潮汐侵蚀与河流沉积共同塑造复杂的河口岸线。", identify: "可见潮汐水道、盐沼、泥滩和由外海向内陆延伸的水体。", example: "钱塘江河口", exampleId: "shanghai", viewerType: "coastal", keywords: ["潮汐", "盐沼", "泥滩", "淡咸水"] }),

  item({ id: "landslide", category: "重力与坡地地貌", title: "滑坡", subtitle: "土体或岩体沿滑面整体下移", intro: "滑坡是斜坡物质在重力作用下沿一个或多个滑面发生的相对完整位移。", formation: "降雨入渗、地震、河流掏蚀、工程切坡或软弱夹层都可能降低斜坡稳定性。", identify: "常见后缘陡坎、拉张裂缝、错落台阶和坡脚鼓丘。", example: "金沙江白格滑坡", exampleId: "himalayas", viewerType: "mass-movement", keywords: ["滑面", "后缘", "堆积前缘", "失稳"] }),
  item({ id: "rockfall", category: "重力与坡地地貌", title: "崩塌与落石", subtitle: "陡崖岩体快速脱离并坠落", intro: "崩塌是岩体或土体从陡坡突然脱离、翻滚或坠落的快速运动。", formation: "节理裂隙、冻融、地震、暴雨和崖脚侵蚀都能触发崩塌。", identify: "崖脚常堆有新鲜、棱角分明的块石，崖壁可见剥落面。", example: "三峡库区崩塌地貌", exampleId: "yangtze", viewerType: "mass-movement", keywords: ["落石", "崖壁", "冻融", "块石"] }),
  item({ id: "talus", category: "重力与坡地地貌", title: "崩积锥与岩屑坡", subtitle: "崩塌物在坡脚堆积形成的锥状或裙状地形", intro: "崩积锥由崖壁脱落的碎屑在坡脚堆积形成；岩屑坡则沿山脚呈连续裙状。", formation: "反复崩塌、冻融和重力搬运使碎屑在坡度降低处按安息角稳定堆积。", identify: "坡脚有棱角明显的碎石堆，坡面通常较均一且陡度接近安息角。", example: "阿尔卑斯山岩屑坡", exampleId: "himalayas", viewerType: "mass-movement", keywords: ["坡脚", "碎屑", "安息角", "崩积"] }),
];
