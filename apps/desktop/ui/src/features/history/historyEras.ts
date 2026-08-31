import type { Icon } from "@phosphor-icons/react";
import { Buildings, Crown, MapPin, Scroll } from "@phosphor-icons/react";

/**
 * 历史时代数据：页面主要根据 currentEraIndex 渲染，
 * 不为每一个朝代写独立 JSX；滚轮 / 右侧详情 / 底部总时间轴共享同一份数组。
 */

export type EraFactIcon = "capital" | "era" | "exchange" | "system";

export interface EraFact {
  icon: EraFactIcon;
  label: string;
  value: string;
  caption: string;
}

export interface EraHighlight {
  eyebrow: string;
  title: string;
  description: string;
}

export interface HistoryEra {
  id: string;
  name: string;
  shortName: string;
  startYear: number;
  endYear: number;
  yearLabel: string;
  description: string;
  tags: string[];
  capital?: string;
  facts: EraFact[];
  highlights: EraHighlight[];
}

export const ERA_ICONS: Record<EraFactIcon, Icon> = {
  capital: MapPin,
  era: Crown,
  exchange: Scroll,
  system: Buildings,
};

function compactYear(value: number): string {
  return value < 0 ? `前${Math.abs(value)}` : `${value}`;
}

function range(start: number, end: number): string {
  return `${compactYear(start)}—${compactYear(end)}`;
}

interface EraSeed {
  id: string;
  name: string;
  start: number;
  end: number;
  description: string;
  tags: string[];
  capital?: string;
  facts: [EraFactIcon, string, string, string][];
  highlights: [string, string, string][];
}

function build(seed: EraSeed): HistoryEra {
  return {
    id: seed.id,
    name: seed.name,
    shortName: seed.name,
    startYear: seed.start,
    endYear: seed.end,
    yearLabel: range(seed.start, seed.end),
    description: seed.description,
    tags: seed.tags,
    capital: seed.capital,
    facts: seed.facts.map(([icon, label, value, caption]) => ({ icon, label, value, caption })),
    highlights: seed.highlights.map(([eyebrow, title, description]) => ({ eyebrow, title, description })),
  };
}

const SEEDS: EraSeed[] = [
  {
    id: "xia", name: "夏", start: -2070, end: -1600,
    description: "传统史学中的早期王朝，也是探索早期国家形成的重要起点。二里头遗址的宫城、道路与青铜礼器，为理解这一时期的政治与礼制提供了重要材料。",
    tags: ["早期国家", "二里头", "世袭制", "青铜萌芽"],
    capital: "阳城（待考）",
    facts: [["capital", "都城", "阳城", "文献说法不一"], ["era", "关键线索", "二里头遗址", "早期国家形态"], ["exchange", "区域互动", "城邑网络", "文化交往"], ["system", "制度", "世袭制", "王位传承"]],
    highlights: [["重要事件", "涂山之会", "传统记载中的早期盟会。"], ["代表人物", "大禹", "治水与早期王权的象征。"], ["文化成就", "青铜礼器", "早期礼制的重要载体。"]],
  },
  {
    id: "shang", name: "商", start: -1600, end: -1046,
    description: "以甲骨文和青铜器闻名的早期王朝。甲骨卜辞为研究商代祭祀、王权与社会提供一手材料，青铜礼器则反映政治与礼制结构。",
    tags: ["甲骨文", "青铜器", "殷墟", "祭祀"],
    capital: "殷（安阳）",
    facts: [["capital", "都城", "殷墟", "今河南安阳"], ["era", "关键线索", "甲骨文", "王室占卜记录"], ["exchange", "青铜网络", "礼制与技术", "区域传播"], ["system", "制度", "王权神授", "祭祀与占卜"]],
    highlights: [["重要事件", "盘庚迁殷", "商代中后期政治中心确立。"], ["代表人物", "武丁", "商代强盛时期的重要君主。"], ["文化成就", "甲骨文", "汉字的早期成熟形态。"]],
  },
  {
    id: "western-zhou", name: "西周", start: -1046, end: -771,
    description: "以宗法、分封与礼乐制度为主要特征的王朝。周人通过分封诸侯与礼乐秩序维系统治，王室东迁后进入东周时期。",
    tags: ["分封制", "宗法制", "礼乐", "井田"],
    capital: "镐京",
    facts: [["capital", "都城", "镐京", "今陕西西安"], ["era", "盛世", "成康之治", "早期稳定"], ["exchange", "诸侯网络", "分封朝贡", "联系各地"], ["system", "制度", "宗法礼乐", "维系等级秩序"]],
    highlights: [["重要事件", "牧野之战", "商周易代的标志性战役。"], ["代表人物", "周公", "制礼作乐，稳定政局。"], ["文化成就", "礼乐制度", "影响后世两千余年。"]],
  },
  {
    id: "spring-autumn", name: "春秋", start: -770, end: -476,
    description: "周王室权威下降、诸侯争霸加剧、旧秩序不断调整的时期。外交、战争与礼制均在变化，为战国时期的变法与统一准备了条件。",
    tags: ["争霸", "礼崩乐坏", "诸侯", "会盟"],
    capital: "洛邑（周）",
    facts: [["capital", "中心", "洛邑", "周王室所在"], ["era", "关键", "霸主迭起", "齐晋楚相继争霸"], ["exchange", "外交", "会盟聘问", "频繁互动"], ["system", "制度", "礼制松动", "等级秩序重构"]],
    highlights: [["重要事件", "城濮之战", "晋楚争霸的重要转折。"], ["代表人物", "孔子", "儒家学说的创立者。"], ["文化成就", "百家前奏", "诸子思想开始孕育。"]],
  },
  {
    id: "warring-states", name: "战国", start: -475, end: -221,
    description: "七雄竞争与制度创新并行，为统一帝国准备了条件。变法推动军政与经济能力重组，诸子百家形成重要思想传统。",
    tags: ["七雄", "变法", "百家争鸣", "兼并战争"],
    capital: "七雄都城",
    facts: [["capital", "格局", "七雄并立", "齐楚燕韩赵魏秦"], ["era", "关键", "商鞅变法", "秦的制度优势"], ["exchange", "外交", "合纵连横", "纵横捭阖"], ["system", "制度", "郡县萌芽", "中央集权起步"]],
    highlights: [["重要事件", "长平之战", "秦统一进程的关键战役。"], ["代表人物", "商鞅", "变法推动秦国崛起。"], ["文化成就", "百家争鸣", "诸子思想繁荣。"]],
  },
  {
    id: "qin", name: "秦", start: -221, end: -206,
    description: "中国首个完成大一统的中央集权王朝。秦以郡县制、统一文字度量衡等措施奠定制度基础，但因严刑峻法与沉重役使迅速覆亡。",
    tags: ["大一统", "郡县制", "书同文", "长城"],
    capital: "咸阳",
    facts: [["capital", "都城", "咸阳", "今陕西咸阳"], ["era", "关键", "统一六国", "公元前221年"], ["exchange", "标准", "车同轨", "统一交通标准"], ["system", "制度", "郡县制", "中央集权确立"]],
    highlights: [["重要事件", "焚书坑儒", "文化政策影响深远。"], ["代表人物", "秦始皇", "大一统制度的推动者。"], ["文化成就", "小篆度量衡", "标准化成就。"]],
  },
  {
    id: "western-han", name: "西汉", start: -202, end: 8,
    description: "汉初在秦制基础上调整，逐步形成稳定的大一统治理。文景时期休养生息，武帝时期开拓进取，与西域的联系影响深远。",
    tags: ["文景之治", "丝绸之路", "独尊儒术", "汉武帝"],
    capital: "长安",
    facts: [["capital", "都城", "长安", "今陕西西安"], ["era", "盛世", "文景之治", "休养生息"], ["exchange", "交流", "丝绸之路", "张骞通西域"], ["system", "制度", "察举制", "选官制度发展"]],
    highlights: [["重要事件", "张骞通西域", "开辟丝绸之路。"], ["代表人物", "汉武帝", "开疆拓土的一代君主。"], ["文化成就", "独尊儒术", "经学成为主流。"]],
  },
  {
    id: "eastern-han", name: "东汉", start: 25, end: 220,
    description: "东汉前期光武中兴，后期外戚、宦官与豪强影响中央政治，黄巾起义后各地军阀力量扩张，最终走向分裂。",
    tags: ["光武中兴", "宦官外戚", "黄巾起义", "造纸术"],
    capital: "洛阳",
    facts: [["capital", "都城", "洛阳", "今河南洛阳"], ["era", "盛世", "光武中兴", "秩序重建"], ["exchange", "交流", "佛教传入", "文化新元素"], ["system", "制度", "察举完善", "豪强势力上升"]],
    highlights: [["重要事件", "黄巾起义", "东汉统治动摇。"], ["代表人物", "蔡伦", "改进造纸术。"], ["文化成就", "造纸术", "推动知识传播。"]],
  },
  {
    id: "three-kingdoms", name: "三国", start: 220, end: 280,
    description: "魏、蜀、吴并立的时期。政权并存与战争塑造了区域格局，赤壁之战是理解三方形成的重要节点，不能简化为单一朝代。",
    tags: ["魏蜀吴", "赤壁之战", "诸葛亮", "分裂"],
    capital: "洛阳 / 成都 / 建业",
    facts: [["capital", "格局", "三足鼎立", "魏蜀吴并立"], ["era", "关键", "赤壁之战", "奠定三分格局"], ["exchange", "相持", "南北对峙", "区域力量重组"], ["system", "制度", "屯田制", "恢复农业生产"]],
    highlights: [["重要事件", "赤壁之战", "孙刘联军大败曹操。"], ["代表人物", "诸葛亮", "蜀汉丞相。"], ["文化成就", "建安文学", "三曹与七子。"]],
  },
  {
    id: "western-jin", name: "西晋", start: 266, end: 316,
    description: "短暂实现统一后，迅速因内乱与外部压力而瓦解。八王之乱严重削弱统治基础，北方政治与人口格局发生显著变化。",
    tags: ["短暂统一", "八王之乱", "门阀", "永嘉之乱"],
    capital: "洛阳",
    facts: [["capital", "都城", "洛阳", "今河南洛阳"], ["era", "关键", "太康盛世", "短暂繁荣"], ["exchange", "迁徙", "民族迁徙", "边疆压力加大"], ["system", "制度", "九品中正制", "门阀政治形成"]],
    highlights: [["重要事件", "八王之乱", "统治基础崩溃。"], ["代表人物", "司马炎", "完成统一。"], ["文化成就", "玄学盛行", "清谈风气。"]],
  },
  {
    id: "eastern-jin", name: "东晋", start: 317, end: 420,
    description: "以建康为中心的南方政权，与北方多个政权长期对峙。江南地区持续开发，淝水之战巩固了短期稳定。",
    tags: ["衣冠南渡", "建康", "淝水之战", "士族"],
    capital: "建康",
    facts: [["capital", "都城", "建康", "今江苏南京"], ["era", "关键", "淝水之战", "以少胜多"], ["exchange", "对峙", "南北并存", "政权相持"], ["system", "制度", "门阀政治", "王谢等士族"]],
    highlights: [["重要事件", "淝水之战", "东晋大胜前秦。"], ["代表人物", "谢安", "淝水之战的统帅。"], ["文化成就", "王羲之书法", "艺术高峰。"]],
  },
  {
    id: "northern-southern", name: "南北朝", start: 420, end: 589,
    description: "多个南北政权并存、民族与文化交流频繁的时期。政权更替频繁，但制度与文化发展并未中断，为隋唐统一积累条件。",
    tags: ["政权并立", "民族融合", "均田制", "佛教"],
    capital: "建康 / 洛阳等",
    facts: [["capital", "格局", "南北并立", "多政权交替"], ["era", "关键", "孝文帝改革", "民族融合"], ["exchange", "交融", "胡汉互动", "文化融合加深"], ["system", "制度", "均田制", "土地制度调整"]],
    highlights: [["重要事件", "孝文帝迁都洛阳", "推行汉化改革。"], ["代表人物", "北魏孝文帝", "推动改革。"], ["文化成就", "石窟艺术", "云冈与龙门。"]],
  },
  {
    id: "sui", name: "隋", start: 581, end: 618,
    description: "结束长期分裂并重新统一的短命王朝。大运河影响南北交通与经济联系，制度建设对唐朝具有承接意义。",
    tags: ["重新统一", "大运河", "科举制", "短命王朝"],
    capital: "大兴城（长安）",
    facts: [["capital", "都城", "大兴城", "今陕西西安"], ["era", "关键", "统一南北", "结束长期分裂"], ["exchange", "交通", "大运河", "沟通南北经济"], ["system", "制度", "科举制", "选官制度变革"]],
    highlights: [["重要事件", "开凿大运河", "贯通南北。"], ["代表人物", "隋文帝", "统一与制度建设。"], ["文化成就", "科举制", "影响千年。"]],
  },
  {
    id: "tang", name: "唐", start: 618, end: 907,
    description: "唐朝是中国历史上最为辉煌的朝代之一。政治开明，经济繁荣，文化昌盛，对外交往频繁，丝绸之路畅通，长安成为当时世界上最国际化的大都市。",
    tags: ["繁荣开放", "盛世气象", "兼容并包", "文化鼎盛"],
    capital: "长安",
    facts: [["capital", "都城", "长安", "今陕西西安"], ["era", "盛世", "贞观之治", "开元盛世"], ["exchange", "交流", "丝绸之路", "通达亚欧"], ["system", "制度", "科举制", "逐步完善"]],
    highlights: [["重要事件", "安史之乱", "755 年爆发，盛唐由盛转衰的转折。"], ["代表人物", "李白 · 杜甫", "诗仙与诗圣，唐诗最璀璨的篇章。"], ["文化成就", "唐诗繁荣", "诗歌、书法、绘画、音乐全面发展。"]],
  },
  {
    id: "five-dynasties", name: "五代十国", start: 907, end: 979,
    description: "北方多朝更替、南方多政权并存的过渡时期。不能用单一王朝线解释全国局势，区域经济与文化持续发展。",
    tags: ["政权更替", "十国并存", "过渡时期", "南方开发"],
    capital: "开封 / 洛阳等",
    facts: [["capital", "格局", "五代十国", "北方五朝南方十国"], ["era", "关键", "燕云十六州", "割让影响深远"], ["exchange", "互动", "南北往来", "区域经济持续"], ["system", "制度", "藩镇余绪", "政治动荡"]],
    highlights: [["重要事件", "石敬瑭割让燕云", "影响边防格局。"], ["代表人物", "冯道", "历仕数朝。"], ["文化成就", "词的发展", "文学新形态。"]],
  },
  {
    id: "northern-song", name: "北宋", start: 960, end: 1127,
    description: "与辽、西夏长期并存的中原王朝。政治、财政与边防问题彼此交织，商品经济与城市文化则高度繁荣。",
    tags: ["文治", "商品经济", "澶渊之盟", "王安石变法"],
    capital: "东京开封府",
    facts: [["capital", "都城", "开封", "今河南开封"], ["era", "关键", "王安石变法", "熙宁改革"], ["exchange", "外交", "澶渊之盟", "宋辽关系"], ["system", "制度", "文官政治", "重文抑武"]],
    highlights: [["重要事件", "王安石变法", "围绕财政军政改革。"], ["代表人物", "苏轼", "文学大家。"], ["文化成就", "宋词与市井文化", "繁荣发展。"]],
  },
  {
    id: "liao", name: "辽", start: 916, end: 1125,
    description: "契丹建立的政权，与北宋长期并存。辽的存在说明中国历史时间线并非单一王朝接续。",
    tags: ["契丹", "并存政权", "澶渊之盟", "南北面官"],
    capital: "上京临潢府",
    facts: [["capital", "都城", "上京临潢府", "今内蒙古"], ["era", "关键", "澶渊之盟", "与宋长期和战"], ["exchange", "贸易", "榷场往来", "宋辽互市"], ["system", "制度", "南北面官", "因俗而治"]],
    highlights: [["重要事件", "澶渊之盟", "宋辽百年和平。"], ["代表人物", "耶律阿保机", "建国称帝。"], ["文化成就", "契丹文字", "双轨文化。"]],
  },
  {
    id: "western-xia", name: "西夏", start: 1038, end: 1227,
    description: "党项建立的政权，先后与北宋、辽、金、南宋并存，是理解宋辽金夏并存格局的重要节点。",
    tags: ["党项", "并存政权", "河西走廊", "西夏文"],
    capital: "兴庆府",
    facts: [["capital", "都城", "兴庆府", "今宁夏银川"], ["era", "关键", "好水川之战", "大败宋军"], ["exchange", "通道", "河西走廊", "东西交通要道"], ["system", "制度", "番汉并行", "多元治理"]],
    highlights: [["重要事件", "三川口之战", "宋夏战争开端。"], ["代表人物", "元昊", "建立西夏。"], ["文化成就", "西夏文字", "独特文字系统。"]],
  },
  {
    id: "southern-song", name: "南宋", start: 1127, end: 1279,
    description: "以临安为都，与金、西夏等政权长期并存。江南经济文化进一步发展，与北方政权的战争与外交贯穿始终。",
    tags: ["临安", "江南经济", "岳飞", "理学"],
    capital: "临安",
    facts: [["capital", "都城", "临安", "今浙江杭州"], ["era", "关键", "绍兴和议", "宋金关系"], ["exchange", "贸易", "海上贸易", "市舶司繁荣"], ["system", "制度", "理学兴起", "思想体系化"]],
    highlights: [["重要事件", "岳飞抗金", "精忠报国。"], ["代表人物", "岳飞", "抗金名将。"], ["文化成就", "程朱理学", "影响深远。"]],
  },
  {
    id: "jin", name: "金", start: 1115, end: 1234,
    description: "女真建立的政权，先后与北宋、南宋、西夏并存。灭北宋后改变北方政治格局，其兴衰与蒙古崛起密切相关。",
    tags: ["女真", "并存政权", "猛安谋克", "灭北宋"],
    capital: "中都（北京）",
    facts: [["capital", "都城", "中都", "今北京"], ["era", "关键", "靖康之变", "灭北宋"], ["exchange", "和战", "宋金对峙", "长期相持"], ["system", "制度", "猛安谋克", "军民一体"]],
    highlights: [["重要事件", "靖康之变", "北宋灭亡。"], ["代表人物", "完颜阿骨打", "建立金朝。"], ["文化成就", "中都营建", "都城规划。"]],
  },
  {
    id: "yuan", name: "元", start: 1271, end: 1368,
    description: "蒙古建立的统一王朝，形成更广阔的欧亚交流网络。行省制度对后世地方治理影响深远，交通与商业网络更加扩展。",
    tags: ["大一统", "行省制", "欧亚交流", "元曲"],
    capital: "大都",
    facts: [["capital", "都城", "大都", "今北京"], ["era", "关键", "统一全国", "结束长期分裂"], ["exchange", "交流", "欧亚网络", "驿站与贸易"], ["system", "制度", "行省制", "地方治理创新"]],
    highlights: [["重要事件", "忽必烈统一", "建立元朝。"], ["代表人物", "忽必烈", "元朝建立者。"], ["文化成就", "元曲与杂剧", "文学繁荣。"]],
  },
  {
    id: "ming", name: "明", start: 1368, end: 1644,
    description: "以南京、北京为政治中心的统一王朝。文官体系与中央集权持续发展，边防、财政与海洋事务是理解晚明的重要线索。",
    tags: ["洪武", "永乐", "郑和下西洋", "内阁"],
    capital: "北京",
    facts: [["capital", "都城", "北京", "今北京"], ["era", "盛世", "永乐盛世", "郑和下西洋"], ["exchange", "交流", "海上丝路", "远航交流"], ["system", "制度", "内阁制", "中枢运作"]],
    highlights: [["重要事件", "郑和下西洋", "远航壮举。"], ["代表人物", "朱元璋", "开国皇帝。"], ["文化成就", "紫禁城与长城", "建筑成就。"]],
  },
  {
    id: "qing", name: "清", start: 1636, end: 1912,
    description: "由满洲贵族建立并完成全国统一的王朝。多民族国家治理与边疆事务具有重要意义，晚期面临内外压力与制度转型。",
    tags: ["康乾盛世", "多民族国家", "闭关锁国", "近代转型"],
    capital: "北京",
    facts: [["capital", "都城", "北京", "今北京"], ["era", "盛世", "康乾盛世", "疆域稳固"], ["exchange", "贸易", "朝贡与商行", "广州十三行"], ["system", "制度", "军机处", "皇权强化"]],
    highlights: [["重要事件", "鸦片战争", "近代转型的开端。"], ["代表人物", "康熙", "奠定统一格局。"], ["文化成就", "四库全书", "文献整理。"]],
  },
  {
    id: "modern", name: "近现代", start: 1912, end: 2026,
    description: "从民国建立到中华人民共和国时期，中国社会持续经历现代国家与社会转型。该时期跨度大，后续会按事件、人物和制度继续细分。",
    tags: ["现代转型", "民族复兴", "制度变革", "社会变迁"],
    capital: "北京",
    facts: [["capital", "中心", "北京", "首都"], ["era", "关键", "1949 年", "新中国成立"], ["exchange", "交流", "改革开放", "融入世界"], ["system", "制度", "现代国家", "治理体系"]],
    highlights: [["重要事件", "辛亥革命", "帝制终结。"], ["代表人物", "孙中山", "民主革命先行者。"], ["文化成就", "新文化运动", "思想启蒙。"]],
  },
];

export const historyEras: HistoryEra[] = SEEDS.map(build);

export const DEFAULT_ERA_INDEX = Math.max(
  0,
  historyEras.findIndex((era) => era.id === "tang"),
);

export function clampEraIndex(index: number): number {
  return Math.min(historyEras.length - 1, Math.max(0, index));
}
