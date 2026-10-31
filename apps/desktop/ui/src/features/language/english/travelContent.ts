export type BilingualLine = readonly [english: string, chinese: string];
export type DialogueLine = readonly [speaker: "agent" | "you", english: string, chinese: string];

export interface TravelScene {
  id: string;
  name: string;
  en: string;
  desc: string;
  listen: readonly BilingualLine[];
  say: readonly BilingualLine[];
  dialogue: readonly DialogueLine[];
  notes: readonly BilingualLine[];
}

export interface ReadingArticle {
  id: string;
  tag: string;
  level: "A2" | "B1";
  title: string;
  titleZh: string;
  paras: readonly BilingualLine[];
}

export type WordCategoryId = "airport" | "hotel" | "food" | "shop" | "general" | "work" | "emergency";

export interface WordTag {
  id: "all" | WordCategoryId;
  label: string;
}

export interface EnglishWord {
  en: string;
  pos: string;
  zh: string;
  ex: string;
  exZh: string;
  tag: WordCategoryId;
}

export type GrammarPair = readonly [incorrect: string, correction: string, explanation: string];

export interface GrammarQuestion {
  q: string;
  opts: readonly [string, string, string];
  a: 0 | 1 | 2;
}

export interface GrammarLesson {
  id: string;
  title: string;
  sub: string;
  rule: string;
  pairs: readonly GrammarPair[];
  quiz: readonly GrammarQuestion[];
}

export const SCENES = [
  {
    id: "basics", name: "万能 20 句", en: "Emergency kit", desc: "听不懂、说不出时先靠这些撑住场面。",
    listen: [],
    say: [
      ["Sorry, I don't understand.", "抱歉，我没听懂。"],
      ["Can you say that again, please?", "能再说一遍吗？"],
      ["Could you speak more slowly, please?", "能说慢一点吗？"],
      ["Could you write it down?", "能写下来吗？"],
      ["Excuse me.", "打扰一下。"],
      ["I'm from China.", "我来自中国。"],
      ["This is my first time here.", "我第一次来这儿。"],
      ["I need help.", "我需要帮助。"],
      ["Can you help me, please?", "能帮我一下吗？"],
      ["Where is the toilet?", "洗手间在哪儿？"],
      ["How much is it?", "多少钱？"],
      ["Do you take cards?", "可以刷卡吗？"],
      ["I'd like this one.", "我要这个。"],
      ["One moment, please.", "请稍等。"],
      ["Is it far?", "远吗？"],
      ["What time does it open?", "几点开门？"],
      ["I have a reservation.", "我有预订。"],
      ["No, thank you.", "不用了，谢谢。"]
    ],
    dialogue: [],
    notes: [["I'd like...", "比 I want 礼貌得多。想要什么，先说 I'd like。"]]
  },
  {
    id: "checkin", name: "机场值机", en: "Airport check-in", desc: "换登机牌这一关，问题就那四五个。",
    listen: [
      ["Where are you flying to?", "您今天飞哪儿？"],
      ["May I see your passport?", "能看一下护照吗？"],
      ["Any bags to check?", "有要托运的行李吗？"],
      ["Window or aisle?", "靠窗还是靠过道？"]
    ],
    say: [
      ["I'm flying to Tokyo.", "我去东京。"],
      ["Here's my passport.", "这是我的护照。"],
      ["I have one bag to check.", "我有一件行李要托运。"],
      ["Just this one.", "就这一件。"],
      ["A window seat, please.", "靠窗，谢谢。"],
      ["I'd like an aisle seat.", "我想要靠过道的。"],
      ["Is this okay as carry-on?", "这个能随身带上飞机吗？"],
      ["What time is boarding?", "几点登机？"],
      ["Which gate is it?", "登机口是几号？"]
    ],
    dialogue: [
      ["agent", "Good morning. Where are you flying to?", "早上好，您今天飞哪儿？"],
      ["you", "I'm flying to Tokyo.", "我去东京。"],
      ["agent", "May I see your passport?", "能看一下护照吗？"],
      ["you", "Here you are.", "给。"],
      ["agent", "Any bags to check?", "有要托运的行李吗？"],
      ["you", "Just this one.", "就这一件。"],
      ["agent", "Window or aisle?", "靠窗还是靠过道？"],
      ["you", "A window seat, please.", "靠窗，谢谢。"],
      ["agent", "Here's your boarding pass. Gate 12, boarding at 9:40.", "这是登机牌，12 号口，9:40 登机。"],
      ["you", "Thank you. Which gate is it?", "谢谢，登机口是几号？"]
    ],
    notes: [["Here you are.", "递东西时说，等于「给你」。比 Here's my passport 更随口，两个都能用。"]]
  },
  {
    id: "transfer", name: "机场转机", en: "Connecting flights", desc: "中转最怕走错航站楼、行李没直挂。",
    listen: [
      ["What's your final destination?", "您的最终目的地是？"],
      ["You have a two-hour layover.", "您中转停留两小时。"],
      ["Your connection is in Terminal 2.", "转机在 2 号航站楼。"],
      ["The gate has changed.", "登机口改了。"],
      ["Boarding will begin at 6:40.", "6:40 开始登机。"]
    ],
    say: [
      ["I'm transferring to Tokyo.", "我要转机去东京。"],
      ["Where do I go for my connecting flight?", "转机往哪儿走？"],
      ["Which terminal is my connection in?", "转机在哪个航站楼？"],
      ["Is my bag checked through to Tokyo?", "我的行李直挂到东京吗？"],
      ["Do I need to pick up my bag?", "我要取行李吗？"],
      ["How much time do I have?", "我还有多少时间？"],
      ["Am I too late for boarding?", "我登机还来得及吗？"],
      ["I missed my connecting flight.", "我错过转机航班了。"]
    ],
    dialogue: [
      ["you", "Excuse me, I'm transferring to Tokyo.", "打扰一下，我转机去东京。"],
      ["agent", "May I see your boarding pass?", "能看一下登机牌吗？"],
      ["you", "Here you are. Which terminal is my connection in?", "给。转机在哪个航站楼？"],
      ["agent", "Terminal 2. The gate has changed to 21.", "2 号航站楼，登机口改到 21。"],
      ["you", "How much time do I have?", "我还有多少时间？"],
      ["agent", "About an hour and a half.", "大概一个半小时。"],
      ["you", "Is my bag checked through to Tokyo?", "我行李直挂到东京吗？"],
      ["agent", "Yes, it's checked through.", "是的，直挂。"]
    ],
    notes: [
      ["transfer / connecting flight", "转机这件事叫 layover 或 transit，下一程航班叫 connecting flight。别把转机说成 change plane。"],
      ["checked through", "行李直挂。转机先问这一句，才知道要不要去行李转盘。"]
    ]
  },
  {
    id: "immigration", name: "入境问答", en: "Immigration", desc: "只问四件事：来干嘛、待多久、住哪儿、什么时候走。",
    listen: [
      ["What's the purpose of your visit?", "您来做什么？"],
      ["How long will you stay?", "待多久？"],
      ["Where will you be staying?", "住哪儿？"],
      ["Do you have a return ticket?", "有回程机票吗？"],
      ["Anything to declare?", "有要申报的吗？"]
    ],
    say: [
      ["I'm here for tourism.", "我来旅游。"],
      ["I'll stay for ten days.", "我待十天。"],
      ["I'm staying at this hotel.", "我住这家酒店。"],
      ["Here's my hotel booking.", "这是我的酒店订单。"],
      ["Here's my return ticket.", "这是我的回程机票。"],
      ["No, nothing to declare.", "没有要申报的。"],
      ["It's my first time here.", "我第一次来。"]
    ],
    dialogue: [
      ["agent", "What's the purpose of your visit?", "您来做什么？"],
      ["you", "Tourism.", "旅游。"],
      ["agent", "How long will you stay?", "待多久？"],
      ["you", "Ten days.", "十天。"],
      ["agent", "Where will you be staying?", "住哪儿？"],
      ["you", "At this hotel. Here's my booking.", "这家酒店，这是订单。"],
      ["agent", "Do you have a return ticket?", "有回程机票吗？"],
      ["you", "Yes, here it is.", "有，在这儿。"]
    ],
    notes: [["for tourism", "别说 for travel。入境官问目的时，标准答法是 tourism 或 sightseeing。"]]
  },
  {
    id: "taxi", name: "打车", en: "Taking a taxi", desc: "地址、打表、多少钱、停这儿。",
    listen: [
      ["Where to?", "去哪儿？"],
      ["Do you have an address?", "有地址吗？"],
      ["It's about twenty minutes.", "大概二十分钟。"],
      ["Traffic is heavy.", "堵车。"],
      ["Here or the next corner?", "停这儿还是下个路口？"]
    ],
    say: [
      ["Could you take me to this hotel?", "能送我去这家酒店吗？"],
      ["To this address, please.", "去这个地址，谢谢。"],
      ["How much will it cost?", "大概多少钱？"],
      ["How long does it take?", "要多久？"],
      ["Please turn on the meter.", "请打表。"],
      ["Could you wait here for five minutes?", "能在这儿等五分钟吗？"],
      ["Stop here, please.", "就停这儿。"],
      ["Do you take cards?", "能刷卡吗？"],
      ["Could I have a receipt?", "能给我发票吗？"]
    ],
    dialogue: [
      ["agent", "Where to?", "去哪儿？"],
      ["you", "Could you take me to this hotel? Here's the address.", "能送我去这家酒店吗？这是地址。"],
      ["agent", "No problem. About twenty minutes.", "没问题，大概二十分钟。"],
      ["you", "How much will it cost?", "大概多少钱？"],
      ["agent", "Around fifteen dollars.", "大概十五块。"],
      ["you", "Please turn on the meter.", "请打表。"],
      ["agent", "Sure.", "好。"],
      ["you", "Stop here, please. Do you take cards?", "就停这儿。能刷卡吗？"]
    ],
    notes: [
      ["take me to...", "打车固定说法，比 go to 地道。地址直接说：This address, please."],
      ["turn on the meter", "上车先说这句，有些地方不说就会被绕路要价。"]
    ]
  },
  {
    id: "hotel", name: "酒店入住", en: "Hotel check-in", desc: "入住、早餐、退房、多要一条毛巾。",
    listen: [
      ["How can I help you?", "需要什么帮助？"],
      ["Could I see your passport?", "能看下护照吗？"],
      ["Breakfast is from 7 to 10.", "早餐 7 点到 10 点。"],
      ["Check-out is at noon.", "中午退房。"]
    ],
    say: [
      ["I have a reservation. My name is Wang.", "我有预订，姓王。"],
      ["I'd like to check in.", "我要办入住。"],
      ["Is breakfast included?", "含早餐吗？"],
      ["What time is check-out?", "几点退房？"],
      ["Could I have a late check-out?", "能晚点退房吗？"],
      ["What's the Wi-Fi password?", "Wi-Fi 密码是多少？"],
      ["Could I have an extra towel?", "能多给一条毛巾吗？"],
      ["The air conditioner isn't working.", "空调坏了。"],
      ["Could you keep my luggage after check-out?", "退房后能帮我存行李吗？"]
    ],
    dialogue: [
      ["agent", "Good evening. How can I help you?", "晚上好，需要什么帮助？"],
      ["you", "I have a reservation. My name is Wang.", "我有预订，姓王。"],
      ["agent", "Could I see your passport, please?", "能看下护照吗？"],
      ["you", "Here you are.", "给。"],
      ["agent", "You're in room 305. Breakfast is from 7 to 10.", "305 房，早餐 7 点到 10 点。"],
      ["you", "Thank you. Is breakfast included?", "谢谢，含早餐吗？"],
      ["agent", "Yes, it is.", "含。"],
      ["you", "What time is check-out?", "几点退房？"]
    ],
    notes: [["is breakfast included", "用被动。别说 does breakfast include，那个要接宾语才通。"]]
  },
  {
    id: "food", name: "点餐", en: "Ordering food", desc: "两句话点完，一句喊买单。",
    listen: [
      ["Are you ready to order?", "可以点餐了吗？"],
      ["Anything to drink?", "喝点什么？"],
      ["Anything else?", "还要别的吗？"],
      ["For here or to go?", "堂食还是带走？"]
    ],
    say: [
      ["A table for two, please.", "两位。"],
      ["Could I see the menu?", "能看下菜单吗？"],
      ["What do you recommend?", "你推荐什么？"],
      ["I'll have this one.", "我要这个。"],
      ["I'll have the same.", "我要一样的。"],
      ["Not spicy, please.", "不要辣。"],
      ["No ice, please.", "不要冰。"],
      ["Could we have the bill, please?", "买单。"],
      ["Can I pay by card?", "能刷卡吗？"],
      ["Can I get this to go?", "这个能打包吗？"]
    ],
    dialogue: [
      ["agent", "Are you ready to order?", "可以点餐了吗？"],
      ["you", "Yes. I'll have this one.", "可以，我要这个。"],
      ["agent", "Anything to drink?", "喝点什么？"],
      ["you", "A glass of water, no ice, please.", "一杯水，不要冰。"],
      ["agent", "Anything else?", "还要别的吗？"],
      ["you", "That's all, thank you.", "就这些，谢谢。"],
      ["you", "Could we have the bill, please?", "买单。"],
      ["you", "Can I pay by card?", "能刷卡吗？"]
    ],
    notes: [["I'll have...", "点菜专用，比 I want 自然。买单英国说 the bill，美国说 the check，都对。"]]
  },
  {
    id: "directions", name: "问路", en: "Asking directions", desc: "开口 Excuse me，听懂 turn left 就够。",
    listen: [
      ["Go straight.", "一直走。"],
      ["Turn left at the bank.", "在银行那儿左转。"],
      ["It's about ten minutes on foot.", "走路大概十分钟。"],
      ["It's two blocks away.", "两个街区。"],
      ["You can't miss it.", "你不会错过的。"]
    ],
    say: [
      ["Excuse me, how do I get to the station?", "请问车站怎么走？"],
      ["Is it far from here?", "离这儿远吗？"],
      ["Can I walk there?", "能走过去吗？"],
      ["Which line should I take?", "坐哪条线？"],
      ["Where do I change?", "在哪儿换乘？"],
      ["Could you show me on the map?", "能在地图上指给我看吗？"],
      ["I'm lost.", "我迷路了。"],
      ["Thank you very much.", "非常感谢。"]
    ],
    dialogue: [
      ["you", "Excuse me, how do I get to the museum?", "请问博物馆怎么走？"],
      ["agent", "Go straight, then turn left at the bank.", "一直走，到银行那儿左转。"],
      ["you", "Is it far?", "远吗？"],
      ["agent", "About ten minutes on foot.", "走路十分钟。"],
      ["you", "Can I walk there?", "能走过去吗？"],
      ["agent", "Yes, it's close.", "能，很近。"],
      ["you", "Thank you very much.", "非常感谢。"]
    ],
    notes: [["how do I get to...", "固定说法。别说 how can I go to，母语者不会那样问。"]]
  },
  {
    id: "shopping", name: "购物退换", en: "Shopping & returns", desc: "试穿、换号、退货，三句到手。",
    listen: [
      ["Can I help you?", "需要帮忙吗？"],
      ["It's forty dollars.", "四十块。"],
      ["What size?", "什么码？"],
      ["You can try it on over there.", "可以在那边试。"]
    ],
    say: [
      ["How much is this?", "这个多少钱？"],
      ["Do you have this in a larger size?", "有大一号的吗？"],
      ["Can I try it on?", "能试穿吗？"],
      ["Do you take cards?", "能刷卡吗？"],
      ["I'd like to return this.", "我想退这个。"],
      ["It doesn't fit.", "不合身。"],
      ["Could I get a refund?", "能退款吗？"],
      ["Can I exchange it?", "能换吗？"],
      ["Could I have the receipt?", "能给我小票吗？"]
    ],
    dialogue: [
      ["you", "Excuse me, how much is this?", "请问这个多少钱？"],
      ["agent", "It's forty dollars.", "四十块。"],
      ["you", "Do you have this in a larger size?", "有大一号的吗？"],
      ["agent", "Let me check. What size?", "我看看，什么码？"],
      ["you", "Medium, please.", "中码。"],
      ["agent", "Here you are. You can try it on over there.", "给，可以在那边试。"],
      ["you", "Thank you.", "谢谢。"]
    ],
    notes: [["try it on", "代词 it 必须放中间，不能说 try on it。退货先亮小票。"]]
  },
  {
    id: "refund", name: "机场退税", en: "Tax refund", desc: "先海关盖章，再去退税柜台，顺序别搞反。",
    listen: [
      ["Do you have the receipts?", "发票有吗？"],
      ["Where did you buy this?", "这是在哪买的？"],
      ["Please sign here.", "请在这儿签名。"],
      ["The refund goes back to your card.", "退款退到你卡里。"]
    ],
    say: [
      ["I'd like to claim the tax refund.", "我要退税。"],
      ["Here are my receipts and the form.", "这是我的发票和表格。"],
      ["Where is the tax refund counter?", "退税柜台在哪儿？"],
      ["Do I need to show the goods?", "要出示商品吗？"],
      ["Could you stamp this, please?", "能盖个章吗？"],
      ["Could I get the refund in cash?", "能退现金吗？"],
      ["How long does the refund take?", "退款要多久到账？"],
      ["Is there a fee?", "有手续费吗？"]
    ],
    dialogue: [
      ["agent", "Do you have the receipts and the form?", "发票和表格都有吗？"],
      ["you", "Yes. I'd like to claim the tax refund.", "有。我要退税。"],
      ["agent", "Please show me the goods.", "请出示商品。"],
      ["you", "Here they are. Could you stamp this, please?", "在这儿。能盖个章吗？"],
      ["agent", "Done. The counter is on your right.", "盖好了，柜台在您右手边。"],
      ["you", "Could I get the refund in cash?", "能退现金吗？"],
      ["agent", "Cash or card, both fine.", "现金或退卡都行。"]
    ],
    notes: [
      ["claim the tax refund", "退税的标准说法，也可以说 VAT refund。英国叫 VAT，澳洲加拿大叫 GST。"],
      ["先盖章再托运", "海关要看到东西才盖章。行李托运走了，章盖不了，钱也就没了。"]
    ]
  },
  {
    id: "doctor", name: "看医生买药", en: "Doctor & pharmacy", desc: "哪儿疼、多久了、要什么药。",
    listen: [
      ["What seems to be the problem?", "哪里不舒服？"],
      ["How long have you had it?", "多久了？"],
      ["Any allergies?", "有没有过敏？"],
      ["Take this twice a day after meals.", "这个一天两次，饭后吃。"]
    ],
    say: [
      ["I don't feel well.", "我不舒服。"],
      ["I have a headache and a fever.", "我头疼，还发烧。"],
      ["I have a stomachache.", "我肚子疼。"],
      ["My throat hurts.", "我嗓子疼。"],
      ["It hurts here.", "这儿疼。"],
      ["I'm allergic to penicillin.", "我对青霉素过敏。"],
      ["Is it serious?", "严重吗？"],
      ["Do I need a prescription?", "要处方吗？"],
      ["Could you write down the medicine name?", "能把药名写下来吗？"]
    ],
    dialogue: [
      ["agent", "What seems to be the problem?", "哪里不舒服？"],
      ["you", "I have a headache and a fever.", "我头疼，还发烧。"],
      ["agent", "How long have you had it?", "多久了？"],
      ["you", "Since yesterday.", "从昨天开始。"],
      ["agent", "Any allergies?", "有过敏吗？"],
      ["you", "I'm allergic to penicillin.", "我对青霉素过敏。"],
      ["agent", "Take this twice a day after meals.", "这个一天两次，饭后吃。"],
      ["you", "Could you write down the medicine name?", "能把药名写下来吗？"]
    ],
    notes: [
      ["have a fever", "用 have，不用 be。别说 I am fever。"],
      ["-ache 结尾", "headache 头疼、stomachache 肚子疼、toothache 牙疼。说不出病名就指着位置说 It hurts here。"]
    ]
  },
  {
    id: "emergency", name: "紧急情况", en: "Emergencies", desc: "最坏的情况，最短的句子。",
    listen: [
      ["Where did you last see it?", "你最后在哪儿看到的？"],
      ["You should go to the police station.", "你该去警察局。"],
      ["Is anyone hurt?", "有人受伤吗？"]
    ],
    say: [
      ["I need a doctor.", "我要看医生。"],
      ["Call an ambulance, please.", "请叫救护车。"],
      ["I don't feel well.", "我不舒服。"],
      ["Is there a pharmacy nearby?", "附近有药店吗？"],
      ["I lost my passport.", "我护照丢了。"],
      ["My bag was stolen.", "我的包被偷了。"],
      ["Where is the nearest police station?", "最近的警察局在哪儿？"],
      ["Help!", "救命！"]
    ],
    dialogue: [
      ["you", "Excuse me, I need help. I lost my passport.", "打扰一下，我需要帮助，护照丢了。"],
      ["agent", "Where did you last see it?", "你最后在哪儿看到的？"],
      ["you", "In the taxi, I think.", "我想是在出租车里。"],
      ["agent", "You should go to the police station. It's two blocks away.", "你该去警察局，两个街区。"],
      ["you", "Where is it exactly?", "具体在哪儿？"],
      ["agent", "Go straight and turn right at the light.", "直走，红绿灯右转。"]
    ],
    notes: [["I lost my passport", "已经丢了用 lost。紧急时只说关键词也通：Passport. Lost. Help."]]
  }
] as const satisfies readonly TravelScene[];

export const ARTICLES = [
  {
    id: "airport", tag: "airport", level: "A2", title: "At the Airport", titleZh: "在机场",
    paras: [
      ["Last week I flew to Japan for the first time. I arrived at the airport two hours before my flight, because the security check can take a long time.", "上周我第一次飞日本。我提前两小时到机场，因为安检有时候要排很久。"],
      ["At the check-in counter, the agent asked me where I was flying to. I told her Tokyo, and she asked if I had any bags to check. I had only one suitcase, so I put it on the scale. She gave me a boarding pass and told me the gate number.", "在值机柜台，工作人员问我去哪儿。我说东京，她又问我有没有行李要托运。我只有一个箱子，就放上了秤。她给了我登机牌，告诉我登机口号码。"],
      ["Then I went through security. I had to take my laptop out of my bag and put it in a tray. The airport was huge, so I followed the signs to my gate.", "然后我过安检。我得把笔记本电脑从包里拿出来放进托盘。机场很大，我跟着指示牌找到了登机口。"],
      ["My flight was delayed by forty minutes. I bought a coffee and waited near the window. Finally, we boarded at half past nine.", "我的航班延误了四十分钟。我买了杯咖啡，在窗边等。最后我们九点半登机。"]
    ]
  },
  {
    id: "hotel", tag: "hotel", level: "A2", title: "Checking Into a Hotel", titleZh: "酒店入住",
    paras: [
      ["I arrived at the hotel at nine in the evening. The receptionist asked for my passport and my reservation number. I had booked a double room for three nights.", "我晚上九点到酒店。前台要看我的护照和预订号。我订了一间大床房，住三晚。"],
      ["She told me breakfast was included and served from seven to ten. Check-out was at noon. Then she gave me a key card and said the room was on the third floor.", "她说含早餐，早上七点到十点。退房时间是中午。然后她给了我房卡，说房间在三楼。"],
      ["The room was clean, but the air conditioner didn't work. I called the front desk, and someone came up in ten minutes to fix it.", "房间很干净，但空调坏了。我打电话给前台，十分钟后有人上来修好了。"],
      ["The next morning I asked if I could have a late check-out. They said yes, but only until two o'clock.", "第二天早上我问能不能晚点退房。他们说可以，但只能到两点。"]
    ]
  },
  {
    id: "dinner", tag: "food", level: "A2", title: "Ordering Dinner", titleZh: "点餐",
    paras: [
      ["We walked into a small restaurant near the station. A waiter showed us to a table and gave us the menu.", "我们走进车站附近一家小餐厅。服务员带我们到桌边，给了菜单。"],
      ["I didn't know what to order, so I asked the waiter what he recommended. He said the fish was very fresh, so I ordered that.", "我不知道点什么，就问服务员推荐什么。他说鱼很新鲜，我就点了鱼。"],
      ["My friend wanted something not spicy. The waiter said the chicken was mild, and she ordered it with rice.", "我朋友想要不辣的。服务员说鸡肉不太辣，她就点了鸡肉配米饭。"],
      ["When we finished, I asked for the bill. The waiter asked if we wanted to pay by card or in cash. We paid by card and left a small tip.", "吃完我要了账单。服务员问刷卡还是付现。我们刷了卡，留了一点小费。"]
    ]
  },
  {
    id: "taxi", tag: "general", level: "A2", title: "Taking a Taxi", titleZh: "打车",
    paras: [
      ["I came out of the airport and looked for a taxi. The driver asked me where I wanted to go. I showed him the address on my phone.", "我出了机场找出租车。司机问我去哪儿。我把手机上的地址给他看。"],
      ["He said it would take about thirty minutes, but the traffic was heavy. I asked him to turn on the meter. He laughed and said it was already on.", "他说大概三十分钟，但路上堵。我请他打表。他笑了，说表已经开了。"],
      ["The ride cost twenty dollars. I asked for a receipt, because I needed it for my company.", "车费二十美元。我要了小票，因为公司要报销。"],
      ["Tipping is common here, so I gave him a little extra and said thank you.", "这里习惯给小费，所以我多给了一点，说了声谢谢。"]
    ]
  },
  {
    id: "city", tag: "general", level: "A2", title: "A Day in the City", titleZh: "在城市里逛一天",
    paras: [
      ["On my second day, I wanted to visit the museum. I didn't know how to get there, so I asked a woman at the bus stop.", "第二天我想去博物馆。我不知道怎么走，就在公交站问了一位女士。"],
      ["She said it was too far to walk and told me to take the number five bus. She also said I should get off at the third stop and turn left.", "她说走路太远，让我坐 5 路公交。她还说我在第三站下车，然后左转。"],
      ["I got lost anyway. A man saw me looking at the map and helped me. He walked with me to the corner and pointed at the building.", "我还是迷路了。一个男的看到我在看地图，过来帮忙。他陪我走到路口，指了指那栋楼。"],
      ["The museum was free on Sundays. I spent three hours there and took a lot of photos.", "博物馆周日免费。我在里面待了三小时，拍了很多照片。"]
    ]
  },
  {
    id: "pharmacy", tag: "emergency", level: "A2", title: "At the Pharmacy", titleZh: "在药店",
    paras: [
      ["I woke up with a sore throat and a headache. I didn't want to see a doctor, so I went to a pharmacy near the hotel.", "我醒来嗓子疼、头疼。我不想看医生，就去了酒店附近的药店。"],
      ["The pharmacist asked me what was wrong and how long I had felt like this. I said it started the night before.", "药剂师问我哪里不舒服、从什么时候开始的。我说前一晚开始的。"],
      ["She asked if I was allergic to anything. I told her I was allergic to penicillin. She gave me some medicine for my throat and told me to take it twice a day after meals.", "她问我有没有过敏。我说我对青霉素过敏。她给了我治嗓子的药，让我一天两次饭后吃。"],
      ["The medicine cost twelve dollars. I felt much better the next morning.", "药花了十二美元。第二天早上我感觉好多了。"]
    ]
  },
  {
    id: "meeting", tag: "work", level: "B1", title: "A Monday Meeting", titleZh: "周一的会",
    paras: [
      ["On Monday morning we had a team meeting. My manager asked each of us to give a short update on our work.", "周一早上我们开了团队会。经理让每个人简短汇报一下工作进展。"],
      ["I said my part was almost finished, but I needed two more days, because the client had changed the requirements.", "我说我负责的部分快完成了，但还需要两天，因为客户改了需求。"],
      ["My manager asked me to send an email with the new deadline. I said I would send it before lunch.", "经理让我发邮件说明新的截止时间。我说午饭前发。"],
      ["At the end of the meeting, he asked if anyone had questions. Nobody did, so we finished early.", "会议最后他问有没有人有问题。没人问，所以提前结束了。"]
    ]
  },
  {
    id: "email", tag: "work", level: "B1", title: "A Short Email", titleZh: "一封短邮件",
    paras: [
      ["Subject: Report and next steps. Dear Anna, thank you for your message. I have read the report and I have a few questions.", "主题：报告和下一步。Anna 你好，谢谢你的消息。报告我看了，有几个问题。"],
      ["First, the budget looks too low for the new plan. Second, could you send me the numbers for last month?", "第一，新计划的预算看起来太低了。第二，能把上个月的数字发我吗？"],
      ["I will be in the office on Wednesday, so we can talk then. Please let me know what time works for you.", "我周三在办公室，到时候可以聊。请告诉我你什么时间方便。"],
      ["Best regards, Li Wei", "此致，李伟"]
    ]
  }
] as const satisfies readonly ReadingArticle[];

export const WORD_TAGS = [
  { id: "all", label: "全部" },
  { id: "airport", label: "机场" },
  { id: "hotel", label: "酒店" },
  { id: "food", label: "餐厅" },
  { id: "shop", label: "购物" },
  { id: "general", label: "通用" },
  { id: "work", label: "工作" },
  { id: "emergency", label: "紧急" }
] as const satisfies readonly WordTag[];

type WordEntry = readonly [
  en: string,
  pos: string,
  zh: string,
  ex: string,
  exZh: string,
  tag: WordCategoryId
];

const WORD_ENTRIES: readonly WordEntry[] = [
  ["boarding pass", "n.", "登机牌", "Show your boarding pass at the gate.", "在登机口出示登机牌。", "airport"],
  ["gate", "n.", "登机口", "Which gate is it?", "是几号登机口？", "airport"],
  ["terminal", "n.", "航站楼", "Your flight leaves from Terminal 2.", "你的航班在 2 号航站楼。", "airport"],
  ["delay", "n./v.", "延误", "There's a two-hour delay.", "延误两小时。", "airport"],
  ["luggage", "n.", "行李（不可数）", "Could you keep my luggage here?", "能帮我存一下行李吗？", "airport"],
  ["carry-on", "n.", "随身行李", "Is this okay as carry-on?", "这个能随身带吗？", "airport"],
  ["customs", "n.", "海关", "We went through customs quickly.", "我们过海关很快。", "airport"],
  ["security check", "n.", "安检", "The security check is over there.", "安检在那边。", "airport"],
  ["layover", "n.", "中转停留", "I have a three-hour layover in Dubai.", "我在迪拜中转三小时。", "airport"],
  ["reservation", "n.", "预订", "I have a reservation under Wang.", "我用王的名字订了房。", "hotel"],
  ["check in", "v.", "办入住", "I'd like to check in.", "我想办入住。", "hotel"],
  ["check out", "v.", "退房", "What time do we check out?", "几点退房？", "hotel"],
  ["deposit", "n.", "押金", "Do I need to pay a deposit?", "需要交押金吗？", "hotel"],
  ["front desk", "n.", "前台", "Could you call the front desk?", "能帮我打给前台吗？", "hotel"],
  ["elevator", "n.", "电梯", "Where's the elevator?", "电梯在哪儿？", "hotel"],
  ["laundry", "n.", "洗衣", "Is there a laundry service?", "有洗衣服务吗？", "hotel"],
  ["menu", "n.", "菜单", "Could I see the menu?", "能看下菜单吗？", "food"],
  ["bill", "n.", "账单", "Could we have the bill, please?", "买单。", "food"],
  ["tip", "n./v.", "小费", "Is the tip included?", "含小费吗？", "food"],
  ["spicy", "adj.", "辣的", "Not spicy, please.", "不要辣。", "food"],
  ["takeaway", "n.", "外带", "Takeaway, please.", "打包带走。", "food"],
  ["recommend", "v.", "推荐", "What do you recommend?", "你推荐什么？", "food"],
  ["still water", "n.", "不带气的水", "A bottle of still water, please.", "一瓶不带气的水。", "food"],
  ["receipt", "n.", "收据", "Could I have the receipt?", "能给我小票吗？", "shop"],
  ["refund", "n./v.", "退款", "Could I get a refund?", "能退款吗？", "shop"],
  ["exchange", "v.", "换货", "Can I exchange it for a larger size?", "能换大一号吗？", "shop"],
  ["discount", "n.", "折扣", "Is there a discount for two?", "买两个有折扣吗？", "shop"],
  ["size", "n.", "尺码", "Do you have this in my size?", "有我的码吗？", "shop"],
  ["fitting room", "n.", "试衣间", "The fitting room is over there.", "试衣间在那边。", "shop"],
  ["book", "v.", "预订", "I'd like to book a table for two.", "我想订两人位。", "general"],
  ["cancel", "v.", "取消", "I need to cancel my booking.", "我要取消预订。", "general"],
  ["confirm", "v.", "确认", "Could you confirm my booking?", "能确认一下我的预订吗？", "general"],
  ["available", "adj.", "有空的／有货", "Do you have a room available?", "有空房吗？", "general"],
  ["nearby", "adj./adv.", "附近的", "Is there a pharmacy nearby?", "附近有药店吗？", "general"],
  ["expensive", "adj.", "贵的", "That's too expensive.", "太贵了。", "general"],
  ["cheap", "adj.", "便宜的", "Do you have anything cheaper?", "有便宜点的吗？", "general"],
  ["lost", "adj.", "丢失的", "I lost my passport.", "我护照丢了。", "general"],
  ["ambulance", "n.", "救护车", "Call an ambulance, please.", "请叫救护车。", "emergency"],
  ["pharmacy", "n.", "药店", "Where's the nearest pharmacy?", "最近的药店在哪儿？", "emergency"],
  ["prescription", "n.", "处方", "Do I need a prescription?", "要处方吗？", "emergency"],
  ["allergic", "adj.", "过敏的", "I'm allergic to penicillin.", "我对青霉素过敏。", "emergency"],
  ["stolen", "adj.", "被偷的", "My bag was stolen.", "我的包被偷了。", "emergency"],
  ["aisle", "n.", "过道", "An aisle seat, please.", "靠过道，谢谢。", "airport"],
  ["window seat", "n.", "靠窗座位", "I'd like a window seat.", "我想要靠窗的。", "airport"],
  ["departure", "n.", "出发／起飞", "Departure is at 9:40.", "9:40 起飞。", "airport"],
  ["arrival", "n.", "到达", "Arrival time is six p.m.", "到达时间是下午六点。", "airport"],
  ["baggage claim", "n.", "行李提取处", "Baggage claim is downstairs.", "取行李在楼下。", "airport"],
  ["passport control", "n.", "护照查验", "Passport control is on the left.", "护照查验在左边。", "airport"],
  ["transfer desk", "n.", "中转柜台", "Where's the transfer desk?", "中转柜台在哪儿？", "airport"],
  ["boarding", "n.", "登机", "Boarding starts in ten minutes.", "十分钟后登机。", "airport"],
  ["key card", "n.", "房卡", "The key card doesn't work.", "房卡刷不开。", "hotel"],
  ["twin room", "n.", "双床房", "I'd like a twin room.", "我要一间双床房。", "hotel"],
  ["double room", "n.", "大床房", "Is a double room available?", "有大床房吗？", "hotel"],
  ["invoice", "n.", "发票", "Could I have an invoice?", "能给我开张发票吗？", "hotel"],
  ["complaint", "n.", "投诉", "I'd like to make a complaint.", "我要投诉。", "hotel"],
  ["towel", "n.", "毛巾", "Could I have an extra towel?", "能多给一条毛巾吗？", "hotel"],
  ["appetizer", "n.", "前菜", "We'll start with two appetizers.", "先来两个前菜。", "food"],
  ["main course", "n.", "主菜", "What's the main course?", "主菜是什么？", "food"],
  ["dessert", "n.", "甜点", "Do you have any dessert?", "有甜点吗？", "food"],
  ["medium", "adj.", "五分熟", "Medium, please.", "五分熟，谢谢。", "food"],
  ["refill", "n./v.", "续杯", "Could I get a refill?", "能续一杯吗？", "food"],
  ["cutlery", "n.", "餐具", "Could I have some cutlery?", "能给我餐具吗？", "food"],
  ["cash", "n.", "现金", "Can I pay in cash?", "可以付现金吗？", "shop"],
  ["change", "n.", "零钱", "Here's your change.", "这是找您的零钱。", "shop"],
  ["try on", "v.", "试穿", "Can I try it on?", "能试穿吗？", "shop"],
  ["tax free", "adj.", "免税的", "Is this tax free?", "这个免税吗？", "shop"],
  ["price tag", "n.", "价签", "There's no price tag.", "没有价签。", "shop"],
  ["sell", "v.", "卖", "Do you sell umbrellas?", "你们卖伞吗？", "shop"],
  ["address", "n.", "地址", "Here's the address.", "这是地址。", "general"],
  ["direction", "n.", "方向", "Which direction is the station?", "车站在哪个方向？", "general"],
  ["map", "n.", "地图", "Could you show me on the map?", "能在地图上指给我看吗？", "general"],
  ["nearby", "adj.", "附近的", "Is there a bank nearby?", "附近有银行吗？", "general"],
  ["open", "adj./v.", "开着的／开门", "Are you open now?", "现在营业吗？", "general"],
  ["closed", "adj.", "关门的", "It's closed on Sundays.", "周日不营业。", "general"],
  ["meeting", "n.", "会议", "The meeting starts at ten.", "会议十点开始。", "work"],
  ["schedule", "n.", "日程", "What's your schedule tomorrow?", "你明天什么安排？", "work"],
  ["deadline", "n.", "截止时间", "The deadline is Friday.", "截止时间是周五。", "work"],
  ["report", "n.", "报告", "I'll send you the report today.", "我今天把报告发你。", "work"],
  ["client", "n.", "客户", "The client is waiting.", "客户在等。", "work"],
  ["colleague", "n.", "同事", "He's my colleague.", "他是我同事。", "work"],
  ["attachment", "n.", "附件", "Please see the attachment.", "请看附件。", "work"],
  ["update", "n.", "进展", "Could you give me an update?", "能跟我说下进展吗？", "work"],
  ["feedback", "n.", "反馈", "Any feedback is welcome.", "欢迎任何反馈。", "work"],
  ["budget", "n.", "预算", "It's over budget.", "超预算了。", "work"],
  ["medicine", "n.", "药", "Do you have any medicine for a cold?", "有治感冒的药吗？", "emergency"],
  ["hospital", "n.", "医院", "Where's the nearest hospital?", "最近的医院在哪儿？", "emergency"],
  ["hurt", "v./adj.", "疼／受伤", "My leg hurts.", "我腿疼。", "emergency"],
  ["police station", "n.", "警察局", "Where's the police station?", "警察局在哪儿？", "emergency"]
];

export const WORDS: readonly EnglishWord[] = WORD_ENTRIES.map(([en, pos, zh, ex, exZh, tag]) => ({
  en,
  pos,
  zh,
  ex,
  exZh,
  tag
}));

export const GRAMMAR_LESSONS = [
  {
    id: "be", title: "be 动词：句子不能没有动词", sub: "中文能省「是」，英语不行",
    rule: "中文说「我累了」不用动词，英语必须有 am/is/are。主语 + be + 形容词／地点。",
    pairs: [
      ["I very tired.", "I'm very tired.", "very 是副词，不能当动词用，前面要有 am。"],
      ["He not here.", "He's not here.", "否定要把 not 放在 be 后面。"],
      ["You are agree?", "Do you agree?", "agree 自己就是动词，不能再套一个 are。"]
    ],
    quiz: [
      { q: "「我不舒服。」", opts: ["I don't feel well.", "I am not comfortable feeling.", "I no well."], a: 0 },
      { q: "「他不在酒店。」", opts: ["He not at the hotel.", "He isn't at the hotel.", "He no in hotel."], a: 1 },
      { q: "「我来自中国。」", opts: ["I'm from China.", "I from China.", "I am a Chinese people."], a: 0 }
    ]
  },
  {
    id: "present", title: "习惯用原形，正在做用 -ing", sub: "现在时不是「现在正在」",
    rule: "习惯、事实用动词原形（第三人称加 s）；此刻正在发生用 be + 动词 ing。",
    pairs: [
      ["I go to Tokyo now.", "I'm going to Tokyo now.", "「现在正在去」要用进行时。"],
      ["I am work here.", "I work here.", "work 是动词，前面不能加 am。"],
      ["She is want coffee.", "She wants coffee.", "want 是状态动词，一般不用进行时；第三人称加 s。"]
    ],
    quiz: [
      { q: "「我正在等朋友。」", opts: ["I wait for a friend.", "I'm waiting for a friend.", "I am wait for a friend."], a: 1 },
      { q: "「我在这家公司上班。」", opts: ["I am work in this company.", "I working here.", "I work for this company."], a: 2 },
      { q: "「她每天七点起床。」", opts: ["She gets up at seven every day.", "She is get up at seven.", "She get up at seven."], a: 0 }
    ]
  },
  {
    id: "past", title: "讲发生过的事，动词要变形", sub: "lose → lost，go → went",
    rule: "过去的事用过去式：规则加 -ed，不规则要背。问句和否定用 did + 原形。",
    pairs: [
      ["I lose my passport yesterday.", "I lost my passport yesterday.", "已经丢了要用过去式 lost。"],
      ["Did you went there?", "Did you go there?", "did 后面一定跟原形。"],
      ["I didn't ate.", "I didn't eat.", "否定里用原形，不加 -ed。"]
    ],
    quiz: [
      { q: "「我昨天到的。」", opts: ["I arrive yesterday.", "I arrived yesterday.", "I did arrived yesterday."], a: 1 },
      { q: "「你吃了吗？」", opts: ["Did you eat?", "Did you ate?", "Do you ate?"], a: 0 },
      { q: "「我没看到那个牌子。」", opts: ["I didn't saw the sign.", "I not see the sign.", "I didn't see the sign."], a: 2 }
    ]
  },
  {
    id: "article", title: "a / the / 不加冠词", sub: "第一次提用 a，都知道的那个用 the",
    rule: "第一次说到、不特指：a（辅音前）/ an（元音前）。双方都知道是哪个：the。泛指复数或不可数：不加。",
    pairs: [
      ["I need a information.", "I need some information.", "information 不可数，不能加 a。"],
      ["I want to book the room.", "I want to book a room.", "还没定哪一间，用 a。"],
      ["I lost the passport.", "I lost my passport.", "第一次提到、说的是自己的，用 my。"]
    ],
    quiz: [
      { q: "「我想订一间房。」", opts: ["I'd like to book a room.", "I'd like to book room.", "I'd like to book the room."], a: 0 },
      { q: "「能给我一条毛巾吗？」", opts: ["Could I have towel?", "Could I have a towel?", "Could I have the towels?"], a: 1 },
      { q: "「我在等出租车。」", opts: ["I'm waiting for a taxi.", "I'm waiting for taxi.", "I'm waiting for the taxis."], a: 0 }
    ]
  },
  {
    id: "modal", title: "礼貌要借 can / could / would", sub: "Could I have... 比 Give me... 好一百倍",
    rule: "请求：Could I have...?／Could you...? 比 Can I 客气一点。想要什么用 I'd like，不用 I want。",
    pairs: [
      ["Give me the bill.", "Could I have the bill, please?", "命令式听起来冲，加 could 和 please 立刻变礼貌。"],
      ["You can help me?", "Could you help me?", "疑问句要倒装，而且求助用 Could you 更自然。"],
      ["I want a window seat.", "I'd like a window seat.", "I'd like = I would like，服务员听得更舒服。"]
    ],
    quiz: [
      { q: "想请对方说慢一点：", opts: ["Speak slowly!", "Could you speak more slowly, please?", "You speak slow."], a: 1 },
      { q: "在餐厅要一杯水：", opts: ["Give me water.", "I want water.", "Could I have a glass of water, please?"], a: 2 },
      { q: "想要靠窗座位：", opts: ["I'd like a window seat, please.", "I want window seat.", "Give me window."], a: 0 }
    ]
  },
  {
    id: "question", title: "问句的顺序别搞反", sub: "Where are you from? 不是 Where you are from?",
    rule: "一般疑问：Do/Does/Did + 主语 + 原形。特殊疑问：疑问词 + be/do + 主语。",
    pairs: [
      ["Where you are from?", "Where are you from?", "be 动词要提到主语前面。"],
      ["How much it costs?", "How much does it cost?", "问价格用 does it cost。"],
      ["You have a reservation?", "Do you have a reservation?", "陈述句语气加问号不是提问，要加 do。"]
    ],
    quiz: [
      { q: "问对方住哪儿：", opts: ["Where you staying?", "Where are you staying?", "Where do you staying?"], a: 1 },
      { q: "问几点开门：", opts: ["What time it opens?", "What time does it open?", "What time open?"], a: 1 },
      { q: "问有没有空房：", opts: ["You have rooms?", "Do you have any rooms available?", "Have rooms?"], a: 1 }
    ]
  },
  {
    id: "there", title: "there is / there are 和 in on at", sub: "某地有某物，用 there",
    rule: "「有」表示存在：there is + 单数，there are + 复数。地点：in 大地方，on 平面，at 点（at the gate, at the corner）。",
    pairs: [
      ["Have a bank near here?", "Is there a bank near here?", "表示「存在」不能直接用 have。"],
      ["There is many shops.", "There are many shops.", "复数用 there are。"],
      ["I'm in gate 12.", "I'm at gate 12.", "具体的点用 at。"]
    ],
    quiz: [
      { q: "「附近有药店吗？」", opts: ["Is there a pharmacy near here?", "Have a pharmacy near here?", "There is pharmacy?"], a: 0 },
      { q: "「我在机场。」", opts: ["I'm at the airport.", "I'm on airport.", "I'm in airport."], a: 0 },
      { q: "「这里有很多餐厅。」", opts: ["There is many restaurants.", "There are many restaurants here.", "Have many restaurants."], a: 1 }
    ]
  },
  {
    id: "count", title: "不可数名词：luggage / information / advice", sub: "别加 s，别加 a",
    rule: "luggage、information、advice、water、money 不可数：不加 s，不加 a/an，用 some / much / a piece of。",
    pairs: [
      ["I have two luggages.", "I have two bags.", "luggage 不可数，要数就用 bags。"],
      ["Can you give me an advice?", "Can you give me some advice?", "advice 不可数，用 some。"],
      ["How many money is it?", "How much is it?", "价格直接问 how much。"]
    ],
    quiz: [
      { q: "「我有很多行李。」", opts: ["I have many luggages.", "I have a lot of luggage.", "I have a luggage."], a: 1 },
      { q: "「能给我一些信息吗？」", opts: ["Could you give me an information?", "Could you give me some information?", "Could you give me informations?"], a: 1 },
      { q: "「多少钱？」", opts: ["How many money?", "How much is it?", "How many is it?"], a: 1 }
    ]
  },
  {
    id: "future", title: "说将来的三种说法", sub: "will、be going to、正在进行的 -ing",
    rule: "will + 原形：临时决定、承诺。be going to + 原形：已经打算好。现在进行时也能说安排好的将来。",
    pairs: [
      ["I will to call you.", "I'll call you.", "will 后面直接跟原形，不加 to。"],
      ["I go to Tokyo next week.", "I'm going to Tokyo next week.", "说计划好的事，要用 going to 或进行时。"],
      ["We are go to leave.", "We're going to leave.", "be going to 后面必须是原形。"]
    ],
    quiz: [
      { q: "「我明天飞东京。」", opts: ["I'm flying to Tokyo tomorrow.", "I will flying to Tokyo.", "I fly to Tokyo tomorrow."], a: 0 },
      { q: "「我给你打电话。」", opts: ["I will to call you.", "I'll call you.", "I calling you."], a: 1 },
      { q: "「我们打算早点走。」", opts: ["We will leaving early.", "We go to leave early.", "We're going to leave early."], a: 2 }
    ]
  },
  {
    id: "perfect", title: "现在完成时：已经做过", sub: "have/has + 过去分词",
    rule: "强调「到现在为止已经…」，常配 already、just、ever、never。有明确过去时间（yesterday、last week）就不能用，要用过去时。",
    pairs: [
      ["I have ate.", "I've eaten.", "have 后面用过去分词 eaten。"],
      ["I have lost my passport yesterday.", "I lost my passport yesterday.", "有 yesterday 就用过去时。"],
      ["Did you ever been to Japan?", "Have you ever been to Japan?", "问经历用 Have you ever..."]
    ],
    quiz: [
      { q: "「我已经吃过了。」", opts: ["I've already eaten.", "I have ate.", "I already eat."], a: 0 },
      { q: "「你去过东京吗？」", opts: ["Did you ever go to Tokyo?", "Have you ever been to Tokyo?", "Do you ever been Tokyo?"], a: 1 },
      { q: "「我的包丢了（现在还没找到）。」", opts: ["I lost my bag yesterday.", "I've lost my bag.", "I lose my bag."], a: 1 }
    ]
  },
  {
    id: "compare", title: "比较级和最高级", sub: "cheaper / the cheapest",
    rule: "短词加 -er/-est：cheap → cheaper → the cheapest。长词用 more/most：expensive → more expensive → the most expensive。不规则：good → better → best。",
    pairs: [
      ["This one is more cheap.", "This one is cheaper.", "单音节词直接加 -er，不用 more。"],
      ["He is more tall than me.", "He's taller than me.", "tall 也是单音节。"],
      ["It's the most cheap one.", "It's the cheapest one.", "最高级同理。"]
    ],
    quiz: [
      { q: "「这个便宜一点。」", opts: ["This one is cheaper.", "This one is more cheap.", "This one is cheap more."], a: 0 },
      { q: "「哪个最快？」", opts: ["Which is the most fast?", "Which is the fastest?", "Which is more fast?"], a: 1 },
      { q: "「这个比那个好。」", opts: ["This is more good than that.", "This is gooder than that.", "This is better than that."], a: 2 }
    ]
  },
  {
    id: "frequency", title: "always / usually / never 放哪儿", sub: "在动词前，在 be 后",
    rule: "频率副词放在实义动词前面，be 动词后面。",
    pairs: [
      ["I go always there.", "I always go there.", "放在动词 go 前面。"],
      ["He always is late.", "He's always late.", "be 动词后面。"],
      ["I never not go.", "I never go.", "never 本身就是否定，别再加 not。"]
    ],
    quiz: [
      { q: "「我通常七点起床。」", opts: ["I usually get up at seven.", "I get up usually at seven.", "Usually I get up at seven o'clock always."], a: 0 },
      { q: "「他从不准时。」", opts: ["He never is on time.", "He's never on time.", "He not never on time."], a: 1 },
      { q: "「我们总是先订位。」", opts: ["We book always a table.", "Always we book a table.", "We always book a table."], a: 2 }
    ]
  },
  {
    id: "phrasal", title: "动词后面那个小介词", sub: "wait for / arrive at / listen to",
    rule: "很多动词要跟固定介词，背的时候连着介词一起背，别按中文一对一直译。",
    pairs: [
      ["I'm waiting my friend.", "I'm waiting for my friend.", "wait 后面要 for。"],
      ["I arrived to the hotel.", "I arrived at the hotel.", "arrive at + 小地点，arrive in + 城市国家。"],
      ["Listen me.", "Listen to me.", "listen 后面要 to。"]
    ],
    quiz: [
      { q: "「我在等出租车。」", opts: ["I'm waiting for a taxi.", "I'm waiting a taxi.", "I'm waiting to a taxi."], a: 0 },
      { q: "「我昨天到上海。」", opts: ["I arrived to Shanghai yesterday.", "I arrived in Shanghai yesterday.", "I arrived at Shanghai yesterday."], a: 1 },
      { q: "「听我说。」", opts: ["Listen me.", "Listen at me.", "Listen to me."], a: 2 }
    ]
  },
  {
    id: "timeprep", title: "时间介词 in / on / at", sub: "at 时刻，on 某天，in 月份年份",
    rule: "at + 具体时刻（at 7 o'clock）；on + 某天（on Monday、on May 1st）；in + 月、年、季节、较长时段（in July、in 2026、in the morning）。",
    pairs: [
      ["I'll come in Monday.", "I'll come on Monday.", "具体某天用 on。"],
      ["The flight is in 7 o'clock.", "The flight is at 7 o'clock.", "具体时刻用 at。"],
      ["My birthday is on January.", "My birthday is in January.", "月份用 in。"]
    ],
    quiz: [
      { q: "「我周一到。」", opts: ["I'll arrive on Monday.", "I'll arrive in Monday.", "I'll arrive at Monday."], a: 0 },
      { q: "「飞机 7 点起飞。」", opts: ["The flight leaves in 7.", "The flight leaves at 7.", "The flight leaves on 7."], a: 1 },
      { q: "「我 2026 年去。」", opts: ["I'll go on 2026.", "I'll go at 2026.", "I'll go in 2026."], a: 2 }
    ]
  },
  {
    id: "passive", title: "被动语态：被偷了、被取消了", sub: "be + 过去分词",
    rule: "东西是「被」怎么样的，用 be + 过去分词。过去的事用 was/were + 过去分词。",
    pairs: [
      ["My bag stole.", "My bag was stolen.", "包是被偷的，用被动。"],
      ["The flight cancelled.", "The flight was cancelled.", "航班是被取消的。"],
      ["It was stole.", "It was stolen.", "was 后面用过去分词。"]
    ],
    quiz: [
      { q: "「我的包被偷了。」", opts: ["My bag was stolen.", "My bag stole.", "My bag is steal."], a: 0 },
      { q: "「航班取消了。」", opts: ["The flight cancelled.", "The flight was cancelled.", "The flight is cancel."], a: 1 },
      { q: "「房间每天都打扫。」", opts: ["The room cleans every day.", "The room is clean every day.", "The room is cleaned every day."], a: 2 }
    ]
  },
  {
    id: "chinglish", title: "最典型的中式英语", sub: "这几个错，十个中国人九个犯",
    rule: "别按中文一个字一个字翻。very 不修饰动词，how to 不能单独成句，开灯是 turn on。",
    pairs: [
      ["I very like it.", "I really like it.", "very 只能修饰形容词，修饰动词用 really。"],
      ["How to say this in English?", "How do you say this in English?", "How to 是短语，不是完整问句。"],
      ["Open the light.", "Turn on the light.", "开灯是 turn on，不是 open。"]
    ],
    quiz: [
      { q: "「我很喜欢这个。」", opts: ["I very like this.", "I really like this.", "I like very this."], a: 1 },
      { q: "「这个用英语怎么说？」", opts: ["How to say this in English?", "How do you say this in English?", "How say this in English?"], a: 1 },
      { q: "「能开一下灯吗？」", opts: ["Could you open the light?", "Could you turn on the light?", "Could you open light?"], a: 1 }
    ]
  }
] as const satisfies readonly GrammarLesson[];
