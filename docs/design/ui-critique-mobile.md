# UI critique: mobile-first redesign brief

**Scope:** `apps/desktop/ui` — 10 routes on mobile 390×844 and desktop 1440×900.
**Date:** 2026-11-01, 07:47–08:10 (+08:00).
**Author:** independent design critique (read-only; no application code changed).
**Reference:** `output/reference/ember-dashboard.jpg` (2295×1827).

---

## 0. How this was measured (and what changed while measuring)

| Item | Value |
|---|---|
| Browser | Chrome via `apps/desktop/node_modules/playwright` (`channel: "chrome"`) |
| Mobile emulation | viewport 390×844, `isMobile: true`, `hasTouch: true`, DPR 2 |
| Desktop | viewport 1440×900, DPR 1 |
| Scroll tests | CDP `Input.synthesizeScrollGesture` (`gestureSourceType: "touch"`) + `Input.dispatchTouchEvent` sequences |
| Evidence | screenshots + JSON dumps in `/tmp/ui-critique/` (`shots/`, `metrics.json`, `scroll2.json`, `visible.json`) |
| Git state | started on `HEAD = 3b5f4d1`; during the audit `theme/modern-ui.css` + its `main.tsx` import were added and then committed as **`5b565b3 feat(ui): 现代扁平化基础层 + 移动端单一滚动链路`** |

### ⚠ The target moved during the audit

A new file `apps/desktop/ui/src/theme/modern-ui.css` was **created at 07:48:44**, rewritten at **07:54:12**, imported from `main.tsx`, and committed at ~08:10 as **`5b565b3`**. That file already does three of the things this brief would otherwise ask for:

- removes the aurora field and glass (`--aurora-field: none`, `background-image: none`, `::before/::after` disabled) — verified in the live DOM: `.app-shell` computes `background-image: none`, and **zero** elements in `.app-shell`/`.app-content` have a `backdrop-filter`;
- flattens `*-card` to `1px solid var(--line)` + `--radius-lg` + `--shadow-xs`;
- on ≤767px makes `.app-content > .page-pane` the scroll owner and turns `.page-scroll` into `overflow: visible; height: auto`.

So: **the "aurora + glass + glow" complaint is largely already fixed by that commit.** My earlier captures (07:47–07:49) show the old aurora look; everything cited below as a *current* measurement was re-verified at 07:54–08:08. The remaining problems are **structural, not decorative**: navigation, scroll chains, control sizing, type scale, contrast, nesting, states. Those are untouched by `modern-ui.css`.

**Not verified:** iOS/Safari behaviour (Chrome only — no `dvh`/rubber-band/safe-area hardware test), dark themes (`data-theme` other than `pencil`), the Tauri shell, back/forward history, reduced-motion visually, data states beyond the seeded workspace, and anything behind the AI panel.

---

## 1. Verdict

The app is not "dated" any more — it is **inconsistent and structurally unfinished on mobile**. Three different design systems coexist in one product:

1. **Home (`.hd-*`)** — a genuine dashboard system: 38px buttons, 10px radius, hairline cards, KPI tiles, list rows. This is the only page that resembles the reference.
2. **Geography / Review / Language** — pill tabs (`999px`), 44px controls, 18px cards, tinted surfaces, per-module hues.
3. **Collections / News / Study / System** — 27–36px controls, `6px` radius, its own accent (violet), own spacing.

Nothing enforces a shared contract, so every screen re-decides height, radius, colour and density. The user's "太老套 / 不一致 / 整体效果偏差劲" is accurate: the *chrome* is now modern, the *components* are not.

The mobile blocking defects are not cosmetic:

- the bottom navigation contains **17 destinations in a 1128px horizontal strip** — 11 of them are off-screen with no affordance;
- **News** renders 100 stories (14,124px of content) inside a **181px** scroll window, next to a permanently empty 194px reader pane; a swipe anywhere but that 181px strip scrolls **nothing**;
- **Collections** keeps its desktop list/detail split on mobile: a 140px clipped list window above a duplicated detail header;
- **39% of visible controls are smaller than 44×44px** (worst: Travel 24/27, Language 11/12, Review 7/8); History's tappable era bars are **3–15px tall**;
- `--muted` text sits at **3.59:1** contrast, and status text drops to **1.65:1**;
- one page uses **35 distinct font size/weight combinations** and **14 distinct corner radii**, including 49 text nodes under 12px.

---

## 2. Reference vs. app: what Ember does that this app does not

Measured from the reference image, compared with the live app.

| # | Reference (Ember) | App today | Gap |
|---|---|---|---|
| 1 | Near-white canvas (`#fafafa`/`#f7f7f8`), white surfaces; **no gradients, no glow, no blur anywhere** | now flat (after `modern-ui.css`), but `sunrise-glass.css` still ships **24 gradients** and `--accent-glow*`/`--accent-grad-*` tokens are still referenced | delete the decorative layer, not just paint over it |
| 2 | **Hairline** separation: `1px #ececec` borders, shadow ≈ none | cards now `1px + shadow-xs` ✅, but sidebar/rail/top-bar use 3 different line colours (`--line`, `--line-soft`, `--line-tab`) | one border token |
| 3 | Radius discipline: cards **14–16**, inner plates **10–12**, buttons **10**, pill **only** for status badges | **14 radii on one page** (3,4,5,6,8,9,10,12,14,16,18,20,`999px`,`50% 46%`); `999px` used 17×; Collections uses `6px`, Geography `999px`, Review `8px` | one radius scale |
| 4 | **Micro labels**: 14–15px, sentence case, muted, one icon 16–18px, above the value | labels 10–11.5px, sometimes uppercase Latin eyebrows (`NEWS`, `REVIEW`, `GEOGRAPHY EXPLORER · OFFLINE FIRST`, `HISTORY / SEMANTIC ATLAS`) mixed with Chinese titles | pick one label style |
| 5 | **KPI numbers**: near-black, ~34px, tight tracking; colour only on the delta | numbers in **module hues** (red 9, blue 0, brown 5, green 0 on Review) and 30px vs 24px vs 28px across pages | ink for numbers, colour for semantics only |
| 6 | **One accent**, used ≤3 times per screen | indigo `#5566e6` **plus** violet `#8b5cf6` (Collections "AI 综合串讲"), amber, rose, emerald, plus per-module identity colours | single accent + semantic states |
| 7 | **Dense 2-column** layout with 24px gutters and 4-up KPI row | mobile = 1 column with 2-up KPI tiles that leave orphan tiles (Language: 5 tiles → hole in row 3); desktop home ≈ matches | fix the mobile grid math |
| 8 | **One button system**: 40px tall, 10–12px radius, black filled primary / white secondary / ghost, all same height and font | 8+ treatments, heights **26/28/29/30/31/32/36/38/41/44/45px**, radii 6/8/10/18/999/50%, fonts 12/13/14/15/16px | build the system, delete the rest |
| 9 | **Inputs**: filled/white 40px fields, full width, placeholder inside | Travel city input is **195px wide and centred** (placeholder clips); Settings fields are **297×36**; news search has no border at all | one input spec, 44px on touch |
| 10 | **Comfortable touch spacing**: rows 56px, 12–16px padding, 8px gaps | 39% of controls < 44px; `.hd-link` buttons are **16–19px tall**; nav items 64×52 for 17 items | enforce 44px floor |
| 11 | **Empty states** are small and purposeful (a short hint in the flow) | Collections empty card is ~400px tall for a 2-line message + 200px dead space; News reader is a 50%-of-screen dashed void on desktop | compact empty state component |
| 12 | **Detail is a route/sheet**, list stays full width | News and Collections keep list + detail side by side on mobile | collapse to one pane |
| 13 | Nav: 6–7 items in a 240px rail, no horizontal scrolling | **17 items** in a 1128px scrolling strip | 5 + "More" |

---

## 3. Global defects, with measurements

### 3.1 Navigation & routing

| Defect | Evidence |
|---|---|
| **17 bottom-nav items in a 1128px strip** | `.app-bottom-nav` `scrollWidth 1128` / `clientWidth 390`; item `64×52`; only ~5.5 visible (Home, Review, History, Geography, Language, Study). Items 7–17 (`News`, `RSS`, `Travel`, `Markdown`, `Collections`, `Knowledge`, `Graph`, `Search`, `Server`, `System`, `Settings`) render at `x = 664 … 1060`, fully off-screen. Touch horizontal scroll works (`scrollLeft 0 → 201`) but there is **no scrollbar, no fade, no peek, no indicator**. List generated from `NAV_GROUPS` (17 routes) — the comment in `styles.css:16457` says this was deliberate ("全部列出 + 横向滚动"), and it is the single worst mobile decision in the app. |
| **Two "routes" do not exist** | `PAGE_IDS` (`App.tsx:147`) has `study-board`, **not** `study`; and has no `settings`. `#settings` and `#study` are absent, so `fromHash()` returns `null` and the page stays whatever it was: fresh load of `#settings` renders **Home** (5,416 chars), while the URL keeps saying `#settings`. Deep links, bookmarks and the shared routes in this brief are broken. |
| **A nav item that is not a route** | Item 17 "Settings" is a `<button>` that opens `.settings-backdrop`; hash stays `#system` and the active highlight stays on System. It looks identical to the 16 real routes. |
| **No `aria-current`** | Active nav item is visual only (`<nav aria-label="主导航">` exists, but no `aria-current="page"` on items). |
| **Desktop rail is a scroll trap** | 17 items in 5 groups exceed 842px; measured `app-nav[1150/842]` on the Study route — the rail itself becomes the vertical scroller, so hover/focus scroll moves the whole navigation. |
| **Top bar is three competing clusters** | Desktop 56px bar: brand left, floating "AI 助手 ⌘/" pill centre, search+3 icons right. On mobile the bar spends 4 × 44px + brand on 390px, i.e. ~50% of the bar for icons. Search button is `34×36` on desktop (below 44). |

### 3.2 Scroll & overflow — verified with real touch gestures

Root structure: `.app-shell { height:100vh; overflow:hidden }` → `.app-content { overflow:hidden }` → `.page-pane { height:100%; overflow:hidden }` → page. `modern-ui.css` re-enables `overflow-y:auto` on `.app-content > .page-pane` at ≤767px and neutralises `.page-scroll`. That fixes the *shell* nesting, **but every page-level nested scroller survives**:

| Route | Scroll owner(s) | Touch result | Root cause |
|---|---|---|---|
| home | `.page-pane` | ✅ moves 423px/gesture | ok |
| travel | `.page-pane` | ✅ 422px | ok (but `.travel-page` still defines its own heights) |
| language | `.page-pane.language-pane` | ✅ 421px | duplicate rule: `.page-pane.language-pane { overflow-y:auto }` also declares it |
| review / collections / system | `.page-pane` | ✅ | ok |
| **news** | `.news-list` (**181px**), `.news-reader` (194px) | ❌ swipe at (195,300) → `pane 0→0`, `list 0→0`. Only a swipe **inside the 181px strip** scrolls. | `.news-page` is `display:grid; height:100%; grid-template-rows: 236.7px 112.2px 375.1px`; `.news-body` is `grid-template-rows: 180.7px 194.4px` — the 900px media query collapses `grid-template-columns` to `1fr` **but never resets the rows**, so list and reader split the leftover height 50/50. List holds **100 stories / 14,124px** of content in 181px. |
| **geography** | `.geo-rail-track` (sw 924 / cw 324), `.geo-index-rail` (sw 412 / cw 352, `overflow-x:hidden`), `.geo-discovery-card` (max 35px) | ⚠ rail scrolls horizontally only; `geo-index-rail` **clips 60px permanently**; `geo-discovery-card` is a scroller that **does not move** (dead trap) | fixed-width rails inside `geography-explore.css`; `.geo-*{overflow:hidden}` on a 352px parent |
| **history** | `.history-era-scroll` (sw 1040 / cw 332), `.history-v2-tl-rail` (max 13px) | ⚠ 3.1× horizontal-only timeline; `history-v2-tl-rail` **does not scroll** (dead 13px); card is ~790px tall and ~70% empty | `era-timeline.css` desktop geometry kept on mobile + `overflow:hidden` wrapper |
| **collections** | `.page-pane.collections-pane` → internal list window **140px** (sh 224) | ⚠ list rows clipped mid-row; detail duplicates the list's content below | list/detail split not collapsed |
| **study-board** | none | ✅ page doesn't need to scroll | but the toolbar row alone overflows horizontally (last item clipped at x≈380) |
| **every route** | `.app-bottom-nav` | horizontal strip | see 3.1 |

**Overlays:** the AI launcher is `48×48`, `right: 14`, `bottom: 72` → occupies y 724–772 on the right edge, i.e. **on top of page content**, and no scroller reserves space for it. It covers the "掌握 64%" badge on Home, the "继续学习" button on Language, and the quiz card on Review. Scrollers have `padding-bottom: 0px`, so the last row butts against the 61px nav.

**Dead `<button>` targets (a11y + touch):** History renders 72 buttons; the era bars are `111×3`, `131×3`, `65×4`, `46×5 … 51×15` px — untappable, and they inflate the control count.

### 3.3 Typography

| Metric | Value |
|---|---|
| Distinct `size/weight` pairs on **one** page (Home, mobile) | **35** |
| Sizes actually used | 10, 10.5, 11, 11.5, 12, 12.5, 13, 14, 15, 16, 19, 20, 24, 30 px |
| Weights actually used | 400, 500, 600, 650, 680, 700, 800, 900 |
| Text nodes **< 12px** | Home **49**, Geography **75**, History **60** |
| Declared type tokens | only 6 (`--text-page-title`, `--text-card-title`, `--text-body`, `--text-secondary`, `--text-caption`, `--text-kpi`) |
| Body size | 13px for most paragraphs (`--text-body` says 14px, `.news-story-snippet` 13px, `.page-shell-desc` 13px) |

So the token layer exists (`styles.css:62-67`) but **29 of 35 used steps are undeclared**. Sentences like `10px/650`, `10px/800`, `11px/900`, `12.5px/700` have no design meaning.

**Language mixing** (4 conventions at once): English eyebrows (`NEWS`, `REVIEW`, `CONTINUE LEARNING`) + Chinese H1 (`今日新闻`) + English page nav labels (`Home`, `Review`, `Language`) + Chinese group labels (`概览 / 学习 / 发现 / 创作 / 知识`) + English page title (`Language`) + English section titles (`Good morning`, `Today's English`).

### 3.4 Colour & contrast (WCAG 2.1, measured on live DOM)

| Pair | Ratio | Verdict | Where |
|---|---|---|---|
| `#FFB020` on `#F1F3FC` | **1.65** | fail | "降级" status text |
| `#F87171` 11px on white | **2.77** | fail | "关键" tag |
| `--muted #7A7F93` on canvas `#F1F3FC` | **3.59** | fail AA | captions, list meta, `正在加载复习队列…`, empty-state hints, `review` eyebrow |
| `--muted #7A7F93` on white | **3.97** | fail AA | "收藏 0", "共收录 0 个跨模块知识条目" |
| white on `#7A7F93` badge | **3.97** | fail | "已停用" |
| `#16896D` (success) | **4.34** | fail (needs 4.5 at 13px) | trust note |
| `#5566E6` on white | 4.74 | pass | accent text |
| white on `#5566E6` | 4.74 | pass | primary buttons |

`--muted` is the workhorse secondary colour and it fails AA at every size it is used. Accent usage is also uncontrolled: eyebrows in accent, badges in accent, unread dot in accent, active nav in accent, KPI numerals in accent — the reference reserves its accent for the chart and one button.

### 3.5 Controls — the missing button system

Measured visible buttons per route (mobile), all different:

```
home         hd-btn 161x38  r10  12.5px  white+1px   | hd-btn-primary 161x38 r10 #5566e6
             hd-link 63x16  r10  11.5px  transparent          ← 16–19px tall
travel       chips 52x26 r10 | 324x36 r10 | 104x36 r10       ← three widths, two styles, one card
geography    100x44 r999 #5566e6 | 80x44 r999 ghost | 64x44 r12 #5566e6+border
             159x44 r999 #5566e6 | 176x124 r18 card-as-button
history      era bars 46x3 … 131x15 r3
news         52x32 r10 | 65x32 r10 | 30x36 r10 | story rows 365x138 r10
language     65x36 r10 | 320x41 r10 #5566e6 | 100x25 r10 | 86x41 r10
study-board  44x44 r10 | 61x44 r10 | swatches 28x28 r50%
review       107x41 r10 | filters 102x30 r8 (solid accent) / 124x30 r8 ghost (no container at all)
collections  90x29 r6 12px #5566e6 | 101x27 r6 #8B5CF6 (2nd accent) | 92x31 r6 outline
system       104x36 r10
```

- **Heights:** 25, 26, 27, 28, 29, 30, 31, 32, 36, 38, 41, 44, 45.
- **Radii:** 3, 6, 8, 10, 18, 999, 50%.
- **Primary colour:** `#5566e6` on Home/Geography/Language/Review/Collections, `#8b5cf6` on Collections' AI button, none at all on Travel and News (their "primary" is a white bordered button identical to secondary).
- **`--control-height: 36px`** is declared in `styles.css:28` and **no page follows it** (observed 25–45).
- **Touch floor:** 121 of 309 visible mobile controls are `< 44×44` (**39%**). Per route: Travel 24/27, Language 11/12, Review 7/8, Collections 3/3, System 1/1, Study 11/23, Home 13/22, History 35/73, Geography 11/35, News 5/105.
- **Inputs:** Travel city field `195px` wide, centred inside a 335px card → placeholder clipped (`输入想去的城市，如：杭…`), typing area tiny. Settings on mobile: `select/input` are **297×36**, close button **30×30**. News search input has **no visible border or background** on the page canvas, sitting next to a separate 30×36 refresh button of a different height.
- **State feedback:** Home buttons show `:hover`; touch devices see nothing. No `:active` treatment anywhere (project bans transforms, so no press feedback at all). `:focus-visible` is `2px solid var(--accent)` (good) but only reachable by keyboard.

### 3.6 Cards & nesting

- **Geography: 16 nested card elements.** `.geo-index-card` → segmented `精选地点 / 收藏` → `.geo-place-card` → inner `地点` chip + `32×32` pin button. Three visual containers deep.
- **Review:** outer white card → 4 tinted tiles (accent 6%) → each tile is its own radius/shadow.
- **Travel:** one card at `r16` contains chips at `r999`, inputs at `r10`, a `r10` full-width button and a `r10` centred button.
- Radius mix per route: home 7, travel 4, geography 10, history 10, news 5, language 3, review 6, collections 4, study-board 2 distinct values.
- Border colours in play simultaneously: `--line`, `--line-soft`, `--accent`, `#e3e6f4` (hardcoded in the bottom nav), `rgb(227 230 244 / .8)`.
- `modern-ui.css` uses `[class$="-card"], [class*="-card"]` — a broad selector that will fight `geography-explore.css`, `era-timeline.css`, `travel.css`, `home.css`, `knowledgeReviewCard.css` one specificity step at a time long-term. It paints over the problem instead of deleting it.

### 3.7 Empty / loading / error states

| Route | State | Problem |
|---|---|---|
| collections | empty | ~400px tall card, 24px title, folder icon, 2-line hint, then ~200px dead space before the nav |
| review | loading | `正在加载复习队列…` at **3.59:1**, no skeleton, still showing after 4.2s in the web preview |
| study-board | empty | 33 chars of visible text: `未命名学习板 / 0 笔` + a toolbar + a **blank grid**, no onboarding, no "how to start" |
| news (desktop) | no selection | dashed empty box occupies ~50% of the canvas |
| news | degraded source | full-bleed amber banner between hero and list, competing with the page title; `1.65:1` status text inside |
| system / server / graph | sparse | 22–392 chars of content, no illustration or next action |
| all | error | no reusable error/toast visual language found beyond `.news-degraded` and a toast class; not consistently reachable in a read-only audit |

### 3.8 Localisation & content

- Bottom nav labels are English; page bodies are Chinese; eyebrows are English uppercase. Pick one.
- Collections shows the same string twice on one screen (`验证创建→持久化→删除` in the list card and again as the detail subtitle) because list preview + detail are both rendered on mobile.
- `#测试` is used as a persisted collection tag — demo data leaking into the UI.

---

## 4. Per-route defect table

`M` = mobile 390×844, `D` = desktop 1440×900. Severity: **P0** blocks use, **P1** visibly broken, **P2** polish.

| Route | Sev | Mobile (M) | Desktop (D) |
|---|---|---|---|
| **home** | P1 | H1 24px/700 + 4 icon buttons = 56px bar + ~180px greeting before any data. 2×2 KPI grid: numerals 30px/600 in 2 colours, units 11px/700 — 3 levels inside one tile. `.hd-link` row actions are **16–19px tall** (7 of them). "接着上次继续" rows are individually bordered cards inside a card. AI FAB covers the "掌握 64%" badge. 49 text nodes < 12px. | Closest to the reference ✅. 4-up KPI row, 2-col body. Still: tinted numerals, `99+` red badge in the rail, 3 hardcoded radii in the shell, rail is a scroller. |
| **travel** | P1 | Hero = pill badge + H1 + **4-line paragraph** → ~420px before a control. City input **195px, centred**, placeholder clipped. Three controls at 3 widths (195/335/195) — `查看城市资料` (324×36, washed-out ghost) and `创建空白行程` (104×36) look like two different button systems in one card. Chips (52×26, r10) ≠ chips on Geography (r999). 24/27 controls < 44px. | Same card, more empty right margin; form column ~50% of the canvas. |
| **geography** | P1 | `精选地点 5 / 收藏 0` segmented control is **clipped at the right edge**; place cards run past the viewport (`geo-rail-track` 924px in a 324px rail, no peek/affordance). `geo-index-rail` clips 60px with `overflow-x:hidden` — unreachable. Dead 35px scroller. **16 nested cards.** Uppercase accent eyebrow. 75 text nodes < 12px. | Strongest desktop page (5-card rail + map + tabs) but: floating globe decoration with coordinates is noise; 3 radius systems; `Explore/知识` pills vs `搜索` r12; 24/37 controls < 44px. |
| **history** | P0 | Era timeline is a **1040px-wide horizontally scrolling canvas inside a 332px window** with only "← 横向滑动了解更多 →" as a cue; the card is ~790px tall and **~70% empty** (dashed lanes, no data at this width); dead 13px scroller; 72 controls of which 35 are < 44px and the tappable bars are **3–15px tall**; last row sits under the nav. | Timeline works at 1440 (density visible), but still horizontal-only and 66/73 controls < 44px. |
| **news** | P0 | **100 stories / 14,124px in a 181px window**; a swipe outside that strip scrolls nothing (verified). Reader pane = 194px permanently empty ("选择一条新闻开始阅读。"). Unread dot **overlaps the title** (`dot x=360..366` vs `title x=109..364`, `overlapsTitle: true`). Tabs 今日/稍后读/订阅源 are three identical white cards with **no selected state**. Search input borderless + a separate 30×36 icon button. Amber degraded banner with `1.65:1` text. | Same 2-pane split with a **50%-of-canvas dashed empty reader**; unread dot collides with wrapped titles; thumbnails are 72px screenshots-of-text. |
| **language** | P1 | Title "Language" (English) + a 44px `English` select floating beside it on a different baseline. 5 KPI tiles in a 2-col grid → **orphan tile** leaves a hole. Mixed language inside one card (`CONTINUE LEARNING` / `New Concept English 1` / `11 句 · 9 个生词`). 3 buttons + gear in one row at 4 different widths. Card is cut by the nav; FAB covers "继续学习". 11/12 controls < 44px. | Same; tiles stay 2-up and orphaned; right half of the canvas empty. |
| **study (study-board)** | P1 | Toolbar = 6 pill buttons in a **horizontally scrolling row with the last item clipped at the viewport edge**; no page scroll needed; empty grid canvas with **no empty state**; title row is bare text with no card/system; AI FAB overlaps the canvas. 11/23 controls < 44px. | Same toolbar; `app-nav` becomes the page scroller because the canvas is `height:100%`. |
| **review** | P1 | Outer card → 4 tinted tiles → **4 KPI numbers in 4 module hues** (red/blue/brown/green) — the most "dated" element on screen. Filter row: active = solid accent pill 30px, inactive = **plain text with no container** (30px, sub-44 targets). `SRS 间隔重复` outlined-accent next to white `刷新队列` — two hierarchies, one row. Nested card radii 16 → 10 → 16. Quiz card cut by the nav. Loading state `正在加载复习队列…` at 3.59:1. | Same; the quiz card is pinned in a column, the KPI block dominates. |
| **collections** | P0 | Desktop split kept: a **140px clipped list window** (rows cut mid-height) sitting directly under the top bar, then a **second title block** repeating the selected collection + the same description string, then a ~400px empty card and ~200px dead space. 3 buttons at **27/29/31px** with `r6` and 12px text — a different design system from Home. Second accent hue (`#8B5CF6`). Selected list row shows an accent focus ring rather than a selected style. | 2-pane split is correct here; the same 27–31px buttons and `r6` remain inconsistent with the rest of the app. |
| **settings (`#settings`)** | P0 | **Route does not exist** → renders Home. Reachable only via nav item 17 / gear → `.settings-backdrop` (no `role="dialog"`, no `aria-modal`) with `.settings-dialog` 335px wide `r18` (page uses `r16`), inner `.settings-body` scrolls 4,179px in 635px ✅. Five 66px theme cards with **no selected indicator** (active theme not marked). All selects/inputs **297×36**; close **30×30**. Theme names/descriptions still describe the retired "日出光场 / 玻璃表层". | Same dialog at 422×36 controls; full-screen backdrop; no focus trap verified. |

---

## 5. Redesign spec — one mobile-first system

Target: **Operate** mode. Flat, dense, hairline, one accent, generous touch targets. Keep the incumbent accent hue (`#5566e6`) and the 5 themes — the problem is not the palette, it is the lack of rules.

### 5.1 Design tokens

Replace the token block at `apps/desktop/ui/src/styles.css:21-80` (structure) and the `:root[data-theme="pencil"]` colour block. Keep every existing theme able to override **colours only**.

```css
:root {
  /* ---- canvas & surfaces (light reference theme) ---- */
  --bg:            #f7f7f8;   /* page canvas, near-white, never a gradient */
  --panel:         #ffffff;   /* cards, sheets, nav bars */
  --panel-raised:  #ffffff;
  --panel-sunken:  #fafafa;   /* inset groups (table headers, reader bg) */
  --panel-hover:   #f4f4f5;   /* row/button hover + pressed */
  --overlay:       rgb(17 20 24 / 32%);

  /* ---- lines ---- */
  --line:          #e9e9ec;   /* THE border. 1px everywhere. */
  --line-soft:     #f1f1f3;   /* dividers inside a card */
  --line-strong:   #d9d9de;   /* inputs on hover, active tab underline */

  /* ---- ink (all AA on --bg and --panel) ---- */
  --text:          #16171a;   /* headings, KPI numerals, row titles */
  --text-secondary:#3f4148;   /* body copy */
  --muted:         #5f6673;   /* 5.08:1 on #f1f3fc, 6.1:1 on #fff  */
  --faint:         #8a9099;   /* placeholders, disabled — never <12px */
  --text-on-accent:#ffffff;

  /* ---- accent: ONE hue, three roles ---- */
  --accent:        #5566e6;
  --accent-hover:  #4655d6;
  --accent-soft:   #eef0fe;   /* selected row / active nav tint 8% */
  --accent-ring:   rgb(85 102 230 / 25%);

  /* ---- semantic (text-safe) ---- */
  --success:       #0f7a4f;   /* 5.4:1 on white */
  --success-soft:  #e9f6ef;
  --warning:       #8a5a00;   /* 5.9:1 on white — NEVER #ffb020 for text */
  --warning-soft:  #fdf3e2;
  --danger:        #b42318;   /* 6.2:1 on white */
  --danger-soft:   #fef3f2;
  --info:          #175cd3;

  /* ---- radius: 4 values, nothing else ---- */
  --radius-sm: 8px;   /* chips, tags, small plates, swatches      */
  --radius-md: 10px;  /* buttons, inputs, icon buttons, rows     */
  --radius-lg: 12px;  /* cards on mobile                          */
  --radius-xl: 14px;  /* panels/cards on tablet+desktop           */
  --radius-pill: 999px; /* STATUS BADGES ONLY — never a button   */

  /* ---- space: 4px base, 8 steps ---- */
  --space-1: 4px;  --space-2: 8px;  --space-3: 12px; --space-4: 16px;
  --space-5: 20px; --space-6: 24px; --space-7: 32px; --space-8: 40px;

  /* ---- touch / control contract ---- */
  --control-h: 44px;        /* mobile. desktop media query → 36px */
  --control-h-sm: 36px;
  --control-h-lg: 52px;
  --icon-btn: 44px;         /* mobile. desktop → 36px */
  --row-h: 56px;
  --tap-min: 44px;

  /* ---- type: 7 steps, nothing else ---- */
  --font-sans: "Manrope", -apple-system, "Segoe UI", system-ui, sans-serif;
  --type-display: 600 28px/1.2  var(--font-sans);  /* page H1 on mobile */
  --type-h1:      600 22px/1.25 var(--font-sans);  /* desktop page title */
  --type-h2:      600 17px/1.4  var(--font-sans);  /* card/section title */
  --type-body:    400 15px/1.6  var(--font-sans);  /* mobile body — 15, not 13 */
  --type-body-d:  400 14px/1.6  var(--font-sans);  /* desktop body */
  --type-meta:    400 13px/1.5  var(--font-sans);  /* secondary, row subtitle */
  --type-label:   600 11px/1.2  var(--font-sans);  /* micro label, +0.06em, ONE per screen */
  --type-kpi:     600 26px/1.15 var(--font-sans);  /* tabular-nums */
  --tracking-tight: -0.02em;

  /* ---- elevation: hairline does the work ---- */
  --shadow-none: none;
  --shadow-card: 0 1px 2px rgb(16 24 40 / 4%);
  --shadow-pop:  0 8px 24px -4px rgb(16 24 40 / 12%), 0 2px 6px rgb(16 24 40 / 6%);
  --shadow-focus: 0 0 0 3px var(--accent-ring);

  --motion-fast: 120ms;
  --motion-ease: cubic-bezier(.2, 0, .2, 1);
}

@media (max-width: 767px) { :root { --control-h: 44px; --icon-btn: 44px; } }
```

**Rules that must be enforced in review:**
- No `background-image` on `.app-shell` or any layout container. Geometry comes from CSS, not images.
- No `backdrop-filter` except a sheet/modal scrim.
- No `999px` outside `.badge`/`.tag`/`.chip`.
- No `font-size` literal in feature CSS — only `var(--type-*)`.
- No hardcoded `#hex` outside `:root[data-theme=…]`.
- Delete from `sunrise-glass.css` / `liquid-glass.css` / `pencil.css`: `--aurora-*`, `--glass-*`, `--accent-glow*`, `--accent-grad-*`, all 24 `radial-gradient`/`linear-gradient` paint rules. `modern-ui.css` should end up ~0 lines and be removed once the base layer absorbs it.

### 5.2 Button system

One `.btn` + one variant modifier. Heights are the differentiator, not bespoke CSS per page.

```css
.btn {
  display: inline-flex; align-items: center; justify-content: center; gap: 8px;
  height: var(--control-h); min-width: var(--control-h);
  padding: 0 var(--space-4);
  border: 1px solid transparent; border-radius: var(--radius-md);
  font: 600 14px/1 var(--font-sans);
  white-space: nowrap; cursor: pointer;
  touch-action: manipulation; -webkit-tap-highlight-color: transparent;
  transition: background-color var(--motion-fast) var(--motion-ease),
              border-color var(--motion-fast) var(--motion-ease),
              color var(--motion-fast) var(--motion-ease);
}
.btn:focus-visible { outline: none; box-shadow: var(--shadow-focus); }
.btn:disabled { opacity: .45; pointer-events: none; }

.btn--primary   { background: var(--accent); color: var(--text-on-accent); }
.btn--primary:hover   { background: var(--accent-hover); }
.btn--secondary { background: var(--panel); color: var(--text); border-color: var(--line); }
.btn--secondary:hover { background: var(--panel-hover); border-color: var(--line-strong); }
.btn--ghost     { background: transparent; color: var(--muted); padding: 0 var(--space-3); }
.btn--ghost:hover     { background: var(--panel-hover); color: var(--text); }
.btn--danger    { background: var(--panel); color: var(--danger); border-color: #f0cfca; }
.btn--icon      { width: var(--icon-btn); padding: 0; }        /* square, secondary look */
.btn--block     { width: 100%; }
.btn--sm        { height: var(--control-h-sm); font-size: 13px; padding: 0 var(--space-3); }

/* press feedback without transforms (project bans movement) */
.btn:active:not(:disabled) { background: color-mix(in srgb, var(--accent) 84%, #000); }
.btn--secondary:active, .btn--ghost:active { background: var(--panel-hover); }

/* text link that is still a 44px target */
.btn-link { display: inline-flex; align-items: center; min-height: var(--tap-min); padding: 0 4px;
            color: var(--muted); font: 600 13px/1 var(--font-sans); border: 0; background: none; }
.btn-link:hover { color: var(--text); text-decoration: underline; text-underline-offset: 3px; }
```

**Rules:** exactly **one** `--primary` per screen region; ≥2 actions in a row ⇒ `secondary` + `primary` or `block` stacked (mobile: primary full-width at the bottom of the card, secondary above it, never two half-width buttons of equal weight unless they are equal-weight alternatives); icon-only buttons are always `44×44` on touch and need `aria-label`; migrate `.hd-btn*`, `.news-tabs button`, `.geo-*` pill tabs, `.review-center` filters, `.collections-*` buttons onto these classes and delete their local rules.

### 5.3 Input / select system

```css
.field { display: grid; gap: 6px; }
.field > label { font: 600 13px/1.3 var(--font-sans); color: var(--text-secondary); }
.field > .hint  { font: var(--type-meta); color: var(--muted); }

.input, .select, .textarea {
  width: 100%;                       /* never shrink, never centre */
  height: var(--control-h);
  padding: 0 var(--space-3);
  border: 1px solid var(--line);
  border-radius: var(--radius-md);
  background: var(--panel);
  color: var(--text);
  font: var(--type-body);
  text-align: left;
  appearance: none;
}
.input::placeholder, .textarea::placeholder { color: var(--faint); }
.input:hover, .select:hover { border-color: var(--line-strong); }
.input:focus, .select:focus, .textarea:focus {
  outline: none; border-color: var(--accent); box-shadow: var(--shadow-focus);
}
.input[aria-invalid="true"] { border-color: var(--danger); box-shadow: 0 0 0 3px rgb(180 35 24 / 15%); }
.textarea { height: auto; min-height: 96px; padding: var(--space-3); line-height: 1.5; }
.select { padding-right: 36px; background-image: var(--control-arrow);
          background-repeat: no-repeat; background-position: right 12px center; }
```

Fix the two live offenders: Travel's city input (`width:195px; text-align:center` → `width:100%`), and Settings' 36px fields on mobile (inherits `--control-h: 44px`). Inline add-ons (the News search + refresh) must be one visual control group: `display:flex; gap:8px;` where both children are `--control-h` tall.

### 5.4 Card / panel system

```css
.card {
  background: var(--panel);
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
  box-shadow: none;            /* hairline is the separator */
  padding: var(--space-4);
}
.card--flush { padding: 0; }                       /* rows own their padding */
.card--sunken { background: var(--panel-sunken); box-shadow: none; }
.card-head { display: flex; align-items: center; justify-content: space-between;
             gap: var(--space-3); min-height: 44px; }
.card-head > h2 { font: var(--type-h2); letter-spacing: var(--tracking-tight); color: var(--text); }

/* KPI tile — the reference's signature block */
.kpi {
  display: grid; gap: 6px; padding: var(--space-4);
  border: 1px solid var(--line); border-radius: var(--radius-lg); background: var(--panel);
}
.kpi > .kpi-label { font: 600 13px/1.3 var(--font-sans); color: var(--muted); }
.kpi > .kpi-value { font: var(--type-kpi); font-variant-numeric: tabular-nums;
                    letter-spacing: var(--tracking-tight); color: var(--text); }
.kpi > .kpi-delta { font: 600 12px/1.3 var(--font-sans); color: var(--muted); }
.kpi > .kpi-delta[data-trend="up"]   { color: var(--success); }
.kpi > .kpi-delta[data-trend="down"] { color: var(--danger); }
```

**Nesting rule: max 1 card level.** A card may contain rows, dividers, tabs and buttons; it may **not** contain another bordered card. Geography's 3-deep stack becomes: one `.card` → `.segmented` control → `.list-row`s. Review's tinted tiles become `.kpi` cells separated by `--line-soft` dividers inside **one** card.

### 5.5 List-row system

```css
.list { display: grid; margin: 0; padding: 0; list-style: none; }
.list-row {
  display: grid; grid-template-columns: 40px minmax(0,1fr) auto;
  align-items: center; gap: var(--space-3);
  min-height: var(--row-h); padding: var(--space-3) var(--space-4);
  border: 0; border-bottom: 1px solid var(--line-soft);   /* divider, not a card */
  background: transparent; color: inherit; text-align: left; width: 100%;
  border-radius: 0;
}
.list-row:last-child { border-bottom: 0; }
.list-row:hover, .list-row:active { background: var(--panel-hover); }
.list-row:focus-visible { outline: none; box-shadow: inset 0 0 0 2px var(--accent); }
.list-row > .row-icon { width: 40px; height: 40px; display: grid; place-items: center;
                        border-radius: var(--radius-md); background: var(--panel-hover); color: var(--muted); }
.list-row .row-title { font: 600 15px/1.35 var(--font-sans); color: var(--text);
                       overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.list-row .row-sub   { font: var(--type-meta); color: var(--muted); }
.list-row .row-trail { font: var(--type-meta); color: var(--muted); display: flex; align-items: center; gap: 6px; }
```

Row icons use **one neutral tint**. Module identity comes from the icon glyph + label, never from hue (this removes the red/blue/brown/green KPI numerals and the per-module identity colours).

### 5.6 Navigation

**Mobile bottom bar — 5 destinations + More.**

```css
.app-bottom-nav {
  position: fixed; inset: auto 0 0 0; z-index: 40;
  display: grid; grid-template-columns: repeat(5, 1fr);   /* NEVER overflow-x */
  height: calc(56px + env(safe-area-inset-bottom, 0px));
  padding-bottom: env(safe-area-inset-bottom, 0px);
  background: var(--panel); border-top: 1px solid var(--line);
}
.app-bottom-nav-item {
  display: grid; place-content: center; gap: 2px;
  min-height: 56px; padding: 6px 0;
  border: 0; border-radius: 0; background: transparent;
  font: 600 11px/1 var(--font-sans); color: var(--muted);
}
.app-bottom-nav-item[aria-current="page"] { color: var(--accent); }  /* <= NO pill, NO tint */
.app-bottom-nav-item:active { background: var(--panel-hover); }
```

- Slots: `Home`, `Review`, `News`, `Study`, `More`. `More` opens a bottom sheet listing the remaining routes in **at most 2 columns**, grouped (学习 / 发现 / 创作 / 知识 / 系统), each row `min-height: 48px`.
- The existing "Settings" nav entry becomes a row inside `More` → opens the settings sheet; it is not a route and must not be one of the 5 slots.
- Add `aria-current="page"` (currently missing) and drop the per-item `1px` border + accent tint.

**Top bar**

```css
.app-bar { height: 56px; padding: 0 var(--space-4); border-bottom: 1px solid var(--line); background: var(--panel); }
.app-bar .brand { font: 700 15px/1 var(--font-sans); letter-spacing: var(--tracking-tight); }
.app-bar .actions { display: flex; gap: var(--space-2); }
```

- Mobile: **max 2** icon buttons (search, settings) + optional overflow. Theme + profile move into the `More` sheet. Search becomes an icon that opens a full-height search sheet, not a permanent field.
- Desktop: search is a filled `--control-h`-tall field (`background: var(--panel-sunken); border: 1px solid transparent;`) with a `⌘K` keycap; no centre "AI 助手" pill (move it into the rail footer or the icon row); one cluster on the right.

**Desktop rail** — 240–264px, rows `36px`, group labels `--type-label`, counts as plain 12px text (no `kbd` chips, no `99+` red badge — use a 6px accent dot). Whole rail must fit without scrolling at 900px height: 17 items × 36 + 5 group labels = ~700px ✅ (today it overflows to 1150px because rows are 38px + 12px gaps).

### 5.7 States

```css
/* empty */
.empty { display: grid; justify-items: center; gap: var(--space-3);
         max-width: 320px; margin: var(--space-7) auto; text-align: center; padding: var(--space-5); }
.empty > .empty-icon { width: 40px; height: 40px; display: grid; place-items: center;
                       border-radius: var(--radius-md); background: var(--panel-hover); color: var(--muted); }
.empty > .empty-title { font: 600 15px/1.4 var(--font-sans); color: var(--text); }
.empty > .empty-body  { font: var(--type-meta); color: var(--muted); }
.empty > .btn { margin-top: var(--space-2); }

/* skeleton */
.skeleton { background: var(--panel-hover); border-radius: var(--radius-sm); }
@media (prefers-reduced-motion: no-preference) {
  .skeleton { animation: skeleton-pulse 1.4s var(--motion-ease) infinite; }
  @keyframes skeleton-pulse { 50% { opacity: .55; } }
}
@media (prefers-reduced-motion: reduce) { .skeleton { animation: none; } }

/* inline error banner */
.banner { display: grid; grid-template-columns: 20px 1fr auto; align-items: start;
          gap: var(--space-3); padding: var(--space-3) var(--space-4);
          border: 1px solid var(--line); border-radius: var(--radius-md);
          background: var(--panel); font: var(--type-meta); color: var(--text-secondary); }
.banner[data-tone="danger"]  { background: var(--danger-soft);  border-color: #f0cfca; color: var(--danger); }
.banner[data-tone="warning"] { background: var(--warning-soft); border-color: #efd7a8; color: var(--warning); }
```

Replace every ad-hoc empty state (Collections' 400px void, Study's blank grid, News' dashed reader) with `.empty`. Fish: loading `正在加载…` text → skeleton rows at `--type-meta` size using `--muted` **or better, no text at all**.

### 5.8 Scroll behaviour

**One scroll owner per route. Non-negotiable.**

```css
/* mobile: the pane is the only vertical scroller */
@media (max-width: 767px) {
  .app-content { overflow: hidden; }
  .app-content > .page-pane {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    -webkit-overflow-scrolling: touch;
    overscroll-behavior-y: contain;
    /* clear the 56px bar + 16px breathing room + the AI launcher lane */
    padding-bottom: calc(56px + env(safe-area-inset-bottom, 0px) + 16px);
  }
  .app-content > .page-pane * { max-height: none; }        /* kill page-level fixed heights */
  .app-content > .page-pane .page-scroll { height: auto; overflow: visible; }
}
```

- **Never** put `overflow:auto` on a card, rail, list or reader on mobile. Content grows; the pane scrolls. Delete `.news-list{overflow:auto}`, `.news-reader{overflow:auto}`, `.geo-discovery-card`, `.history-v2-tl-rail`, `.collections` inner list window.
- If a region must scroll independently, it must be a real overlay (sheet/modal) with `overscroll-behavior: contain` and its own focus trap.
- **Horizontal rails** are allowed only with all four: a 24px peek of the next item, `scroll-snap-type: x mandatory` + `scroll-snap-align: start`, edge fade masks, and `-webkit-overflow-scrolling: touch`. Otherwise convert to a wrapped grid or a vertical list. History's 1040px timeline and Geography's 924px rail both need this or a vertical alternative at ≤767px.
- **Fixed overlays must reserve space:** the AI launcher is 48px at `bottom: 72px` today and sits over content. Either place it inside the reserved `padding-bottom` lane (`bottom: calc(56px + 8px)`), or hide it on scroll-down and show on scroll-up. Reserve `padding-right: 56px` on rows that would sit under it.
- `touch-action: manipulation` on all controls; keep `touch-action:none` only on the drawing canvas.
- Add `scroll-behavior: auto` (no smooth scrolling) so tests and back-navigation stay predictable.

### 5.9 Migration map (where to change)

| Concern | File / selector |
|---|---|
| tokens, `.app-shell`, `.app-bar`, `.app-nav`, `.app-bottom-nav`, `.page-pane`, `.page-scroll`, `.news-*`, `.hd-*` | `apps/desktop/ui/src/styles.css` (23k lines — split it: `layout.css`, `nav.css`, `controls.css`, `cards.css`, `type.css`) |
| decorative layers to delete | `theme/sunrise-glass.css` (24 gradients), `theme/liquid-glass.css`, `theme/pencil.css` (paper/sketch borders), `theme/modern-ui.css` (temporary override — fold it in and remove) |
| home button/KPI/row system | `features/home/home.css`, `.hd-btn*`, `.hd-link`, `.hd-row` |
| geography rails + 16 nested cards | `features/geography/geography-explore.css` (`.geo-index-rail`, `.geo-rail-track`, `.geo-place-card`, `.geo-discovery-card`) |
| history timeline on mobile | `features/history/era-timeline.css` (`.history-era-scroll`, `.history-v2-tl-rail`) |
| travel form widths | `features/travel/travel.css` |
| review tinted KPI tiles / filters | `features/learning/reviewCenter.css`, `features/learning/knowledgeReviewCard.css` |
| collections split + buttons | `.collections-pane`, `.page-pane.collections-pane` rules in `styles.css` |
| news list/reader split | `.news-page`, `.news-page-shell`, `.news-body`, `.news-list`, `.news-reader`, `.news-unread-dot` in `styles.css:1468-1700` and the `@media (max-width: 900px)` block at `styles.css:17767` |
| routing | `App.tsx:147` `PAGE_IDS` (add `settings`, or map `study` → `study-board`), `NAV_GROUPS`, bottom-nav `slice` logic |
| icons/labels | bottom nav labels are English while pages are Chinese — choose one convention product-wide |

---

## 6. Top 10 fixes by impact

| # | Fix | Impact | Effort |
|---|---|---|---|
| 1 | **Bottom nav: 17 → 5 + "More" sheet**, remove `overflow-x`, add `aria-current`. | Highest. Unlocks 11 unreachable destinations and removes the biggest "cheap app" signal on every screen. | S |
| 2 | **One scroll owner per route.** News list becomes the page scroller (kill the 181px/14,124px window and the empty 194px reader); Collections collapses to one column; delete the dead scrollers (`geo-discovery-card` 35px, `history-v2-tl-rail` 13px) and the `overflow-x:hidden` clip in `geo-index-rail`. | Fixes the reported "some areas cannot scroll". | M |
| 3 | **Ship the button system** (`.btn` + variants), delete per-page button CSS, enforce `--control-h` and 44px floor. | Kills 8 treatments / 13 heights / 7 radii; fixes `hd-link` 16px and History's 3px bars. | M |
| 4 | **Type scale with a floor**: 7 steps, 15px mobile body, no size < 12px, one eyebrow per screen. | Removes the 35-combination noise; instantly reads as designed. | M |
| 5 | **Contrast**: `--muted` → `#5F6673`, `--warning` → `#8A5A00`, `--danger` → `#B42318`; stop using raw `#ffb020/#f87171` for text. | Turns 7 measured failures into 0. | S |
| 6 | **Radius/spacing unification**: one `:root` block, 4 radii + pill-for-badges-only, 8 space steps; delete hardcoded radii in feature CSS (14 values → 4). | Removes the "each page invented its own" feel. | M |
| 7 | **KPI/list grammar**: numerals near-black `tabular-nums`, colour only on the delta; list rows as dividers not cards; row icons one neutral tint. | Ends per-module hue coding and the double-card look. | M |
| 8 | **Routes fixed**: add `settings`/`study` aliases to `PAGE_IDS` (or redirect), make Settings a real destination rather than a look-alike nav item. | Fixes broken deep links/bookmarks. | S |
| 9 | **Overlay hygiene**: AI launcher inside the reserved bottom lane or hidden on scroll; `.settings-dialog` gets `role="dialog"` + `aria-modal` + 44px controls + a marked selected theme. | Removes content occlusion and the last sub-40px targets. | S |
| 10 | **Empty/loading/error components** (`.empty`, `.skeleton`, `.banner`) and delete the bespoke 400px voids and 3.59:1 loading text. | Makes sparse pages (Study, Collections, Review, System) look intentional. | M |

---

## 7. Re-verification checklist

After the redesign lands, re-run and confirm:

1. `document.querySelector('.app-bottom-nav').scrollWidth === innerWidth` — no horizontal nav overflow, ≤6 items.
2. Every route: exactly **one** element with `scrollHeight > clientHeight` and `overflow-y:auto`; gesture at `(195, 300)`, `(195, 620)` and `(195, 760)` all move that element.
3. Zero elements with `scrollWidth > clientWidth + 2` and `overflow-x:hidden` inside `.app-content`.
4. Zero visible `button, a, input, select, [role=button]` with `height < 44` at 390px.
5. Zero computed `font-size < 12px` inside `.app-content` (except the single `.eyebrow`).
6. Contrast: every text/background pair ≥ 4.5:1 (≥ 3:1 for ≥ 18.66px bold).
7. `.app-shell` and `.app-content` compute `background-image: none`; zero `backdrop-filter` outside scrims.
8. Distinct `border-radius` values across the app ≤ 5 (4 tokens + pill).
9. Last row of every route clears the bottom bar: `scrollerPaddingBottom ≥ navHeight + 16`.
10. `#settings` and `#study` either resolve or 404 visibly; every bottom-nav item changes the hash and sets `aria-current`.
11. AI launcher rect does not intersect any `.list-row`, `.btn` or `.kpi` rect at any scroll position.

---

### Appendix: where the evidence lives

```
/tmp/ui-critique/shots/
  fresh-mobile-*.png        current state, mobile, all routes
  fresh-desktop-*.png       current state, desktop
  mobile-home-vp.png …      pre-modern-ui baseline (07:47, aurora + glass) for before/after
  scroll-*-mobile.png       post-gesture state per route
  settings-mobile.png       settings sheet
  studyboard-mobile.png     study board
/tmp/ui-critique/metrics.json    tokens, radii, fonts, controls per route/viewport
/tmp/ui-critique/scroll2.json    every scroller + touch-gesture deltas
/tmp/ui-critique/visible.json    visible-page control/target counts
```
