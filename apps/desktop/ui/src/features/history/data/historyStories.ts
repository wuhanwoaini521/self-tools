import type { HistoryStory } from "../types/history";

/**
 * 历史学习路线 / 故事。
 *
 * 第一版不依赖 AI：这里手写 2–3 条高质量、可验证的叙事主线。
 * `nodeId` 对应离线资料库中已存在的节点；存在则可点击进入对应 Detail，
 * 否则该步骤仅作为叙事节点展示（不伪造内容）。
 */
export const historyStories: HistoryStory[] = [
  {
    id: "tang-decline",
    title: "为什么唐朝由盛转衰？",
    description:
      "从建国、贞观、开元到安史之乱、藩镇割据与晚唐动荡，一条理解盛唐为何不可避免走向衰落的完整路线。",
    minutes: 8,
    periodId: "tang",
    nodes: [
      {
        id: "tang-founding",
        title: "唐朝建立",
        note: "李渊 618 年在长安称帝，承接隋代制度基础，开启一个兼具开放与集中特征的帝国。",
      },
      {
        id: "xuanwu-gate",
        title: "玄武门之变",
        note: "626 年李世民通过政变取得继承权，随后即位，成为贞观之治的起点。",
        nodeId: "xuanwu-gate",
      },
      {
        id: "zhen-guan",
        title: "贞观之治",
        note: "唐太宗前期政治清明、选贤任能，为帝国的稳定与扩张奠定基础。",
      },
      {
        id: "kai-yuan",
        title: "开元盛世",
        note: "唐玄宗前期国力鼎盛，长安、洛阳与江淮共同支撑帝国运转。",
      },
      {
        id: "an-lushan",
        title: "安史之乱",
        note: "755 年安禄山起兵，战乱延续多年，成为由盛转衰的关键节点。",
        nodeId: "an-lushan-rebellion",
      },
      {
        id: "fan-zhen",
        title: "藩镇割据",
        note: "平乱后河北等地藩镇长期保有较大自主性，中央—地方关系深刻重组。",
      },
      {
        id: "huang-chao",
        title: "黄巢起义",
        note: "9 世纪后期财政、党争与民变交织，黄巢起义冲击统治基础。",
      },
      {
        id: "tang-end",
        title: "唐亡",
        note: "907 年朱温废唐，五代十国开始，盛唐落幕。",
      },
    ],
  },
  {
    id: "qin-collapse",
    title: "为什么秦朝统一后迅速灭亡？",
    description:
      "秦以郡县制与统一标准完成大一统，却十几年便覆亡。这条路线从统一、制度到民变与灭亡，理解秦的成就与局限。",
    minutes: 6,
    periodId: "qin",
    nodes: [
      {
        id: "qin-unify",
        title: "统一六国",
        note: "公元前 221 年秦灭齐，建立中国首个大一统中央集权王朝。",
      },
      {
        id: "qin-system",
        title: "郡县与书同文",
        note: "以郡县制、统一文字度量衡取代旧的分封建制，奠定制度基础。",
      },
      {
        id: "qin-shihuang",
        title: "秦始皇",
        note: "统一的推动者，通过峻法与沉重役使维系帝国运转。",
        nodeId: "qin-shihuang",
      },
      {
        id: "qin-burning",
        title: "严刑与役使",
        note: "焚书坑儒、修长城建宫室，赋役沉重，社会矛盾迅速累积。",
      },
      {
        id: "qin-fall",
        title: "秦亡",
        note: "公元前 207 年刘邦入咸阳，强大的帝国在统一后十几年便覆亡。",
      },
    ],
  },
  {
    id: "han-stability",
    title: "汉朝为什么能够长期稳定？",
    description:
      "从汉初休养生息到文景、武帝与西域经略，这条路线梳理汉朝如何把统治基础打造成一个长期运行的体系。",
    minutes: 7,
    periodId: "western-han",
    nodes: [
      {
        id: "han-founding",
        title: "汉朝建立",
        note: "公元前 202 年刘邦称帝，在继承秦制的同时实行郡国并行。",
      },
      {
        id: "wen-jing",
        title: "文景之治",
        note: "休养生息、轻徭薄赋，使社会与财政逐步恢复稳定。",
      },
      {
        id: "wu-di",
        title: "汉武帝与集权",
        note: "推恩令削弱诸侯，独尊儒术，开拓西域，中央集权达到高峰。",
      },
      {
        id: "silk-road",
        title: "丝绸之路",
        note: "张骞通西域后，与西域联系深远影响中国与外部世界。",
      },
      {
        id: "han-economy",
        title: "长期的治理传统",
        note: "察举选官与宽严相济的治理，使汉朝成为后世长期王朝的重要参照。",
      },
    ],
  },
];
