//! 口语陪练场景（Phase 1：纯文本任务型对话）。
//!
//! 场景是教学内容，不属于 UI。完成条件用目标关键词命中，不采信模型自评分数。

use serde::{Deserialize, Serialize};

/// CEFR 难度。未知代码拒绝入库，不默默降成 A1。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CefrLevel {
    A1,
    A2,
    B1,
    B2,
    C1,
    C2,
}

impl CefrLevel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A1 => "A1",
            Self::A2 => "A2",
            Self::B1 => "B1",
            Self::B2 => "B2",
            Self::C1 => "C1",
            Self::C2 => "C2",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "A1" => Some(Self::A1),
            "A2" => Some(Self::A2),
            "B1" => Some(Self::B1),
            "B2" => Some(Self::B2),
            "C1" => Some(Self::C1),
            "C2" => Some(Self::C2),
            _ => None,
        }
    }
}

/// 一条可观察的场景任务。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioGoal {
    pub id: String,
    pub description: String,
    pub description_zh: String,
    /// 小写短语；用户话里命中任一条即视为完成该目标。
    pub keywords: Vec<String>,
}

/// 场景目标词（教学展示用，不单独作为完成条件）。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetWord {
    pub word: String,
    pub gloss_zh: String,
}

/// 无 LLM 时的脚本回合。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptedReply {
    pub after_goal: Option<String>,
    pub text: String,
    pub assist_zh: Option<String>,
}

/// 内置场景定义。
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakingScenario {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub title_zh: String,
    pub description: String,
    pub description_zh: String,
    pub role_name: String,
    pub role_prompt: String,
    pub cefr: CefrLevel,
    pub goals: Vec<ScenarioGoal>,
    pub target_vocab: Vec<TargetWord>,
    pub opening_line: String,
    pub opening_line_zh: Option<String>,
    pub closing_line: String,
    pub closing_line_zh: Option<String>,
    pub scripted_replies: Vec<ScriptedReply>,
}

/// 用户话语是否命中某目标（大小写不敏感，按短语包含判断）。
#[must_use]
pub fn utterance_hits_goal(utterance: &str, goal: &ScenarioGoal) -> bool {
    let haystack = normalize_utterance(utterance);
    if haystack.is_empty() {
        return false;
    }
    goal.keywords.iter().any(|keyword| {
        let needle = normalize_utterance(keyword);
        !needle.is_empty() && haystack.contains(&needle)
    })
}

/// 根据全部用户回合更新目标完成集合（只增不减）。
#[must_use]
pub fn completed_goals(scenario: &SpeakingScenario, user_utterances: &[String]) -> Vec<String> {
    scenario
        .goals
        .iter()
        .filter(|goal| {
            user_utterances
                .iter()
                .any(|utterance| utterance_hits_goal(utterance, goal))
        })
        .map(|goal| goal.id.clone())
        .collect()
}

#[must_use]
pub fn all_goals_complete(scenario: &SpeakingScenario, completed: &[String]) -> bool {
    !scenario.goals.is_empty()
        && scenario
            .goals
            .iter()
            .all(|goal| completed.iter().any(|id| id == &goal.id))
}

#[must_use]
pub fn next_open_goal<'a>(
    scenario: &'a SpeakingScenario,
    completed: &[String],
) -> Option<&'a ScenarioGoal> {
    scenario
        .goals
        .iter()
        .find(|goal| !completed.iter().any(|id| id == &goal.id))
}

#[must_use]
pub fn scripted_reply_for(
    scenario: &SpeakingScenario,
    completed: &[String],
) -> (String, Option<String>) {
    if all_goals_complete(scenario, completed) {
        return (
            scenario.closing_line.clone(),
            scenario.closing_line_zh.clone(),
        );
    }
    let open = next_open_goal(scenario, completed);
    if let Some(goal) = open {
        if let Some(reply) = scenario
            .scripted_replies
            .iter()
            .find(|item| item.after_goal.as_deref() == Some(goal.id.as_str()))
        {
            return (reply.text.clone(), reply.assist_zh.clone());
        }
        if let Some(reply) = scenario
            .scripted_replies
            .iter()
            .find(|item| item.after_goal.is_none())
        {
            return (reply.text.clone(), reply.assist_zh.clone());
        }
        return (
            format!("Could you try: {}.", goal.description),
            Some(goal.description_zh.clone()),
        );
    }
    (
        scenario.opening_line.clone(),
        scenario.opening_line_zh.clone(),
    )
}

fn normalize_utterance(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 六个首发场景。id 稳定，重装不改。
#[must_use]
pub fn builtin_scenarios() -> Vec<SpeakingScenario> {
    vec![
        cafe_scenario(),
        airport_scenario(),
        hotel_scenario(),
        smalltalk_scenario(),
        meeting_scenario(),
        directions_scenario(),
    ]
}

#[must_use]
pub fn scenario_by_id(id: &str) -> Option<SpeakingScenario> {
    builtin_scenarios()
        .into_iter()
        .find(|item| item.id == id || item.slug == id)
}

fn cafe_scenario() -> SpeakingScenario {
    SpeakingScenario {
        id: "speaking:cafe".into(),
        slug: "cafe".into(),
        title: "Ordering at a cafe".into(),
        title_zh: "咖啡店点餐".into(),
        description: "Greet the barista, order a drink, and pay.".into(),
        description_zh: "打招呼、点一杯饮料、完成付款。".into(),
        role_name: "Barista Maya".into(),
        role_prompt: "You are Maya, a friendly barista in a small cafe. Speak natural English at CEFR A2. Guide the customer to greet you, order a drink, and pay. Keep turns short (1-3 sentences). Do not give scores. If the learner uses Chinese, reply in English and briefly model the English.".into(),
        cefr: CefrLevel::A2,
        goals: vec![
            goal("greet", "Greet the barista", "向店员问好", &["hello", "hi", "good morning", "good afternoon"]),
            goal(
                "order",
                "Order a drink",
                "点一杯饮料",
                &[
                    "latte",
                    "coffee",
                    "tea",
                    "cappuccino",
                    "i d like",
                    "i would like",
                    "can i have",
                    "i ll have",
                    "i will have",
                ],
            ),
            goal("pay", "Pay for the order", "完成付款", &["card", "cash", "pay", "bill", "check", "debit", "credit"]),
        ],
        target_vocab: words(&[
            ("latte", "拿铁"),
            ("for here", "在店里用"),
            ("to go", "外带"),
            ("card", "银行卡"),
        ]),
        opening_line: "Hi! Welcome in. What can I get started for you?".into(),
        opening_line_zh: Some("你好，欢迎光临。想喝点什么？".into()),
        closing_line: "Perfect — here's your drink. Have a great day!".into(),
        closing_line_zh: Some("好的，这是您的饮料。祝你有美好的一天！".into()),
        scripted_replies: vec![
            ScriptedReply {
                after_goal: Some("greet".into()),
                text: "Nice to see you. Would you like a latte, tea, or something else?".into(),
                assist_zh: Some("见到你真好。想要拿铁、茶，还是别的？".into()),
            },
            ScriptedReply {
                after_goal: Some("order".into()),
                text: "Got it. For here or to go? And how would you like to pay?".into(),
                assist_zh: Some("好的。在店里喝还是外带？怎么付款？".into()),
            },
            ScriptedReply {
                after_goal: Some("pay".into()),
                text: "Card is fine. I'll bring that right out.".into(),
                assist_zh: Some("刷卡可以。马上给您做好。".into()),
            },
        ],
    }
}

fn airport_scenario() -> SpeakingScenario {
    SpeakingScenario {
        id: "speaking:airport".into(),
        slug: "airport".into(),
        title: "Airport check-in".into(),
        title_zh: "机场值机".into(),
        description: "Check in, confirm your bag, and ask about the gate.".into(),
        description_zh: "办理值机、确认行李，并询问登机口。".into(),
        role_name: "Agent Chen".into(),
        role_prompt: "You are Agent Chen at an airport check-in desk. CEFR A2 English. Help the traveler check in, mention bags or seat, and tell them the gate. Short turns. No numeric pronunciation scores.".into(),
        cefr: CefrLevel::A2,
        goals: vec![
            goal("greet", "Greet the agent", "向工作人员问好", &["hello", "hi", "good morning"]),
            goal(
                "checkin",
                "Check in for the flight",
                "办理值机",
                &["check in", "checking in", "reservation", "booking", "passport", "ticket", "flight"],
            ),
            goal(
                "gate",
                "Ask about bags, seat, or gate",
                "询问行李、座位或登机口",
                &["gate", "bag", "luggage", "window", "aisle", "seat", "boarding"],
            ),
        ],
        target_vocab: words(&[
            ("check in", "值机"),
            ("boarding pass", "登机牌"),
            ("window seat", "靠窗座位"),
            ("gate", "登机口"),
        ]),
        opening_line: "Good morning. Passport and destination, please.".into(),
        opening_line_zh: Some("早上好。请出示护照并告诉我目的地。".into()),
        closing_line: "You're all set. Boarding starts in about forty minutes. Safe travels!".into(),
        closing_line_zh: Some("都办好了。大约四十分钟后开始登机。一路顺风！".into()),
        scripted_replies: vec![
            ScriptedReply {
                after_goal: Some("greet".into()),
                text: "Welcome. Are you checking in for the flight to London?".into(),
                assist_zh: Some("欢迎。您是办理去伦敦的航班吗？".into()),
            },
            ScriptedReply {
                after_goal: Some("checkin".into()),
                text: "I see your reservation. Window or aisle? And do you have a bag to check?".into(),
                assist_zh: Some("找到您的预订了。靠窗还是靠过道？有托运行李吗？".into()),
            },
            ScriptedReply {
                after_goal: Some("gate".into()),
                text: "Window seat is fine. Your gate is B12.".into(),
                assist_zh: Some("靠窗可以。登机口是 B12。".into()),
            },
        ],
    }
}

fn hotel_scenario() -> SpeakingScenario {
    SpeakingScenario {
        id: "speaking:hotel".into(),
        slug: "hotel".into(),
        title: "Hotel check-in".into(),
        title_zh: "酒店入住".into(),
        description: "Confirm a reservation and ask about the room.".into(),
        description_zh: "确认预订并询问房间相关信息。".into(),
        role_name: "Receptionist Ana".into(),
        role_prompt: "You are Ana at a hotel front desk. CEFR A2. Help the guest check in: reservation, nights/room, then wifi or breakfast. Short natural English. No fake scores.".into(),
        cefr: CefrLevel::A2,
        goals: vec![
            goal("reservation", "Mention a reservation", "说明有预订", &["reservation", "booked", "booking", "reserved"]),
            goal("stay", "Talk about nights or room type", "说明晚数或房型", &["night", "nights", "room", "king", "twin", "double"]),
            goal(
                "amenity",
                "Ask about wifi, breakfast, or towels",
                "询问无线网、早餐或毛巾",
                &["wifi", "wi fi", "breakfast", "towel", "password"],
            ),
        ],
        target_vocab: words(&[
            ("reservation", "预订"),
            ("check in", "入住"),
            ("wifi", "无线网"),
            ("breakfast", "早餐"),
        ]),
        opening_line: "Welcome to Harbor Inn. How can I help you?".into(),
        opening_line_zh: Some("欢迎来到 Harbor Inn。有什么可以帮您？".into()),
        closing_line: "Here's your key card. Breakfast is from seven to ten. Enjoy your stay!".into(),
        closing_line_zh: Some("这是房卡。早餐七点到十点。祝您入住愉快！".into()),
        scripted_replies: vec![
            ScriptedReply {
                after_goal: Some("reservation".into()),
                text: "I found your name. How many nights, and did you want a king room?".into(),
                assist_zh: Some("找到您的名字了。住几晚？需要大床房吗？".into()),
            },
            ScriptedReply {
                after_goal: Some("stay".into()),
                text: "That's booked. Would you like the wifi password or breakfast hours?".into(),
                assist_zh: Some("已经订好。需要无线网密码还是早餐时间？".into()),
            },
            ScriptedReply {
                after_goal: Some("amenity".into()),
                text: "Wifi is harbor-guest, password on the key sleeve. Breakfast starts at seven.".into(),
                assist_zh: Some("无线网名 harbor-guest，密码在房卡套上。早餐七点开始。".into()),
            },
        ],
    }
}

fn smalltalk_scenario() -> SpeakingScenario {
    SpeakingScenario {
        id: "speaking:smalltalk".into(),
        slug: "smalltalk".into(),
        title: "Everyday small talk".into(),
        title_zh: "日常闲聊".into(),
        description: "Introduce yourself, ask a question, and share one personal detail.".into(),
        description_zh: "自我介绍、提问，并分享一条个人信息。".into(),
        role_name: "Neighbor Sam".into(),
        role_prompt: "You are Sam, a friendly neighbor making small talk. CEFR A2. Get the learner to introduce themselves, ask you something, and mention work, study, or a hobby. Warm, short turns. No scores.".into(),
        cefr: CefrLevel::A2,
        goals: vec![
            goal("introduce", "Introduce yourself", "自我介绍", &["i m", "i am", "my name", "i m from", "i am from"]),
            goal("ask", "Ask the other person a question", "向对方提问", &["how are you", "what about you", "and you", "where are you from", "what do you"]),
            goal(
                "share",
                "Mention work, study, or a hobby",
                "提到工作、学习或爱好",
                &["work", "student", "study", "job", "like", "hobby", "teacher", "engineer"],
            ),
        ],
        target_vocab: words(&[
            ("nice to meet you", "很高兴认识你"),
            ("what about you", "你呢"),
            ("hobby", "爱好"),
        ]),
        opening_line: "Hey! I don't think we've met. I'm Sam.".into(),
        opening_line_zh: Some("嗨！好像没见过。我是 Sam。".into()),
        closing_line: "This was nice. Let's catch up again soon!".into(),
        closing_line_zh: Some("聊得很愉快。回头再聊！".into()),
        scripted_replies: vec![
            ScriptedReply {
                after_goal: Some("introduce".into()),
                text: "Nice to meet you. What do you do, or what do you like doing on weekends?".into(),
                assist_zh: Some("很高兴认识你。你做什么工作？周末喜欢干什么？".into()),
            },
            ScriptedReply {
                after_goal: Some("ask".into()),
                text: "I'm good, thanks. I like walking by the river. What about you?".into(),
                assist_zh: Some("我很好，谢谢。我喜欢去河边散步。你呢？".into()),
            },
            ScriptedReply {
                after_goal: Some("share".into()),
                text: "That's cool. I'd love to hear more next time.".into(),
                assist_zh: Some("真不错。下次再多聊聊。".into()),
            },
        ],
    }
}

fn meeting_scenario() -> SpeakingScenario {
    SpeakingScenario {
        id: "speaking:meeting".into(),
        slug: "meeting".into(),
        title: "A short work meeting".into(),
        title_zh: "工作会议".into(),
        description: "Greet the team, give a status update, and agree a next step.".into(),
        description_zh: "打招呼、同步进展，并确认下一步。".into(),
        role_name: "Manager Priya".into(),
        role_prompt: "You are Priya running a short standup. CEFR B1. Ask for a status update and a next step. Professional but kind. Short turns. No numeric scores.".into(),
        cefr: CefrLevel::B1,
        goals: vec![
            goal("greet", "Greet the team", "向团队问好", &["hello", "hi", "good morning", "thanks for"]),
            goal(
                "status",
                "Give a status update",
                "同步工作进展",
                &["update", "progress", "done", "finished", "working on", "blocker", "issue"],
            ),
            goal(
                "next",
                "Agree a next step or deadline",
                "确认下一步或截止日期",
                &["next", "deadline", "i will", "i ll", "send", "tomorrow", "by friday", "follow up"],
            ),
        ],
        target_vocab: words(&[
            ("update", "进展"),
            ("blocker", "阻塞点"),
            ("deadline", "截止日期"),
            ("follow up", "跟进"),
        ]),
        opening_line: "Morning, everyone. Let's keep this to five minutes — any updates?".into(),
        opening_line_zh: Some("大家早。我们五分钟结束——有进展要同步吗？".into()),
        closing_line: "Great. I'll send notes after this. Thanks, team.".into(),
        closing_line_zh: Some("很好。会后我发纪要。谢谢大家。".into()),
        scripted_replies: vec![
            ScriptedReply {
                after_goal: Some("greet".into()),
                text: "Thanks. What's the progress on your task, and any blockers?".into(),
                assist_zh: Some("谢谢。你的任务进展如何？有阻塞吗？".into()),
            },
            ScriptedReply {
                after_goal: Some("status".into()),
                text: "Understood. What will you do next, and by when?".into(),
                assist_zh: Some("明白了。下一步做什么？什么时候完成？".into()),
            },
            ScriptedReply {
                after_goal: Some("next".into()),
                text: "That deadline works. I'll follow up tomorrow.".into(),
                assist_zh: Some("这个截止日期可以。我明天跟进。".into()),
            },
        ],
    }
}

fn directions_scenario() -> SpeakingScenario {
    SpeakingScenario {
        id: "speaking:directions".into(),
        slug: "directions".into(),
        title: "Asking for directions".into(),
        title_zh: "旅行问路".into(),
        description: "Get someone's attention, ask for a place, and confirm the route.".into(),
        description_zh: "引起注意、询问地点，并确认路线。".into(),
        role_name: "Local Alex".into(),
        role_prompt: "You are Alex, a local helping a traveler. CEFR A2. Wait for excuse me / a place / a confirmation of left-right-straight. Give simple directions. No scores.".into(),
        cefr: CefrLevel::A2,
        goals: vec![
            goal("attention", "Get attention politely", "礼貌地引起注意", &["excuse me", "sorry", "hello"]),
            goal(
                "place",
                "Ask where a place is",
                "询问某地在哪里",
                &["where", "how do i get", "station", "museum", "toilet", "hotel", "bus"],
            ),
            goal(
                "confirm",
                "Confirm left, right, or straight",
                "确认左转、右转或直行",
                &["left", "right", "straight", "thank"],
            ),
        ],
        target_vocab: words(&[
            ("excuse me", "打扰一下"),
            ("turn left", "左转"),
            ("go straight", "直行"),
            ("on the right", "在右边"),
        ]),
        opening_line: "You look a bit lost — need a hand?".into(),
        opening_line_zh: Some("你看起来有点迷路——需要帮忙吗？".into()),
        closing_line: "Yes: left, then straight. You can't miss it. Take care!".into(),
        closing_line_zh: Some("对：先左转再直行。很好找。慢走！".into()),
        scripted_replies: vec![
            ScriptedReply {
                after_goal: Some("attention".into()),
                text: "No problem. Where do you want to go?".into(),
                assist_zh: Some("没事。你想去哪里？".into()),
            },
            ScriptedReply {
                after_goal: Some("place".into()),
                text: "The station is that way. Turn left at the lights, then go straight.".into(),
                assist_zh: Some("车站在那边。红绿灯左转，然后直行。".into()),
            },
            ScriptedReply {
                after_goal: Some("confirm".into()),
                text: "That's right — left, then straight. About five minutes.".into(),
                assist_zh: Some("没错——左转再直行。大概五分钟。".into()),
            },
        ],
    }
}

fn goal(id: &str, description: &str, description_zh: &str, keywords: &[&str]) -> ScenarioGoal {
    ScenarioGoal {
        id: id.into(),
        description: description.into(),
        description_zh: description_zh.into(),
        keywords: keywords.iter().map(|item| (*item).to_string()).collect(),
    }
}

fn words(pairs: &[(&str, &str)]) -> Vec<TargetWord> {
    pairs
        .iter()
        .map(|(word, gloss)| TargetWord {
            word: (*word).into(),
            gloss_zh: (*gloss).into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cafe_order_completes_when_user_asks_for_a_latte() {
        let cafe = cafe_scenario();
        let order = cafe.goals.iter().find(|goal| goal.id == "order").unwrap();
        assert!(utterance_hits_goal("I'd like a latte, please.", order));
        assert!(!utterance_hits_goal("hello there", order));
    }

    #[test]
    fn all_cafe_goals_complete_only_after_three_hits() {
        let cafe = cafe_scenario();
        let first = completed_goals(&cafe, &["Hi!".into()]);
        assert_eq!(first, vec!["greet".to_string()]);
        assert!(!all_goals_complete(&cafe, &first));
        let done = completed_goals(
            &cafe,
            &[
                "Hi!".into(),
                "Can I have a coffee?".into(),
                "I'll pay by card.".into(),
            ],
        );
        assert!(all_goals_complete(&cafe, &done));
    }

    #[test]
    fn unknown_cefr_is_rejected() {
        assert_eq!(CefrLevel::parse("a2"), Some(CefrLevel::A2));
        assert_eq!(CefrLevel::parse("easy"), None);
    }

    #[test]
    fn six_builtin_scenarios_have_stable_ids() {
        let ids: Vec<_> = builtin_scenarios()
            .into_iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(
            ids,
            vec![
                "speaking:cafe",
                "speaking:airport",
                "speaking:hotel",
                "speaking:smalltalk",
                "speaking:meeting",
                "speaking:directions",
            ]
        );
    }
}
