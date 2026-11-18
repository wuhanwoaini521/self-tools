# 八个核心模块的独立边界与重构路线

- 状态：实施基线（2026-11-18）
- 范围：Home、Review、History、Geography、Language、News、RSS、Travel
- 目标：先让模块可以独立理解、实现、替换和验证，再通过明确契约建立跨模块能力。
- 本文是后续代码迁移的边界约束；根目录 `AGENTS.md` 仍是最高优先级的架构规则。

## 1. 目标结构

每个业务模块在现有 Rust workspace 中保持独立的领域边界，不为“看起来独立”而拆成单独 crate 或仓库。一个完整模块应能沿同一条路径找到：

1. **Domain**：模块自己的领域对象、规则和错误，不依赖 UI、数据库或其他模块的内部类型。
2. **Application**：模块自己的用例与端口，负责业务编排；持久化与外部服务由端口表达。
3. **Infrastructure**：只实现该模块的存储、网络和文件适配器；模块数据由模块拥有。
4. **API**：模块自己的版本化 route、请求/响应 DTO、错误映射和契约验证。不得直接把内部数据库模型当作公共契约。
5. **Frontend**：模块自己的页面、组件、类型、API client、样式与 mock/客户端测试。页面不直接调用其他模块的 client、存储或 helper。

部署时仍由 `runtime` / `apps/server` / `apps/desktop` 组合这些模块。组合根负责装配，不承载模块业务规则。

## 2. 八个模块的职责

| 模块 | 独立拥有的职责 | 不拥有的内容 | 对外协作方式 |
|---|---|---|---|
| **History** | 历史时期、人物、事件、作品的只读查询与富化用例 | Geography 的地图实体、Travel 行程状态 | 公开历史查询与富化 API；地点交叉索引由 Exploration 接口协调 |
| **Geography** | 地理实体、地图查询、收藏与位置时间线 | History 事实、Travel 行程编辑 | 公开地理查询/地点校验接口；历史交叉结果经 Exploration |
| **Language** | 语言内容、课程、词典、学习进度和语言专属操作 | 通用 Review Center 页面与跨模块 SRS 规则 | 通过 Learning 事件/SRS 契约登记学习行为与复习来源 |
| **News** | 系统新闻源、发现、分类、聚合与新闻阅读 | 用户 RSS 订阅语义 | 保持自身存储和 API；共享通用抓取/解析基础设施 |
| **RSS** | 用户主动订阅、Feed 与条目阅读状态 | News 推荐源和新闻分类语义 | 保持自身存储和 API；共享通用抓取/解析基础设施 |
| **Travel** | 旅行研究、行程计划、预算、路线、提案与持久化 | Geography/History 的内部存储与 UI 状态 | 经地点查询、历史上下文等稳定端口协作；行程事实由 Travel 持有 |
| **Review** | 通用复习队列、调度规则、评分提交和统计 | Language 专属答题规则、各内容模块的私有存储 | 内容模块注册复习来源；Review 通过学习项 ID 与来源适配器协作 |
| **Home** | 首页所需的聚合查询与概览语义 | 任一业务模块的事实源、写入规则和数据库 | 只消费模块公开的 dashboard/query 契约；不直接访问各模块 client 或 store |

Home 与 Review 是独立的产品能力，但职责不同于叶子业务模块：Home 是只读聚合入口；Review 是通用学习能力。二者都不得成为其他模块的内部类型汇集点。

## 3. 依赖规则

允许：

- 一个模块的 application 通过稳定的 port/use case 请求另一个模块的能力。
- `runtime` 作为组合根连接这些 port，并提供 adapter。
- 真正通用的能力（HTTP 抓取、事件总线、通用学习事件、API transport）放在明确的 shared/platform 层。
- UI 外壳负责导航和跨页意图传递；业务结果仍由所属模块 API 持久化和校验。

禁止新增：

- 模块直接读写另一个模块的 SQLite 表、repository、store 或私有 application service。
- React 页面互相导入对方的 client、页面类型或业务 helper。
- 把一个模块的 DTO 塞进全局 `src/types.ts` 后让所有页面自由引用；公共跨域 DTO 必须有明确 owner 和版本。
- 让后端返回组件、页面、颜色、modal 或 renderer 指令。
- 让 Home 或 Review 成为所有业务操作的万能入口。
- 以“共享”名义把 News/RSS 合回同一个领域、数据库或状态机。

## 4. 当前盘点基线

- **History**：后端 `HistoryService` + `HistoryQueryPort` 边界较清晰；HTTP handler/DTO 仍集中在 `apps/server/src/routes.rs`，前端 History 还直接连接 Learning 收藏/事件与 Exploration。
- **Geography**：后端已有 service/port 和独立 store/API；前端仍依赖 History、Learning 和 Exploration 的具体 client/types。
- **Language / Review**：通用 SRS 在 `learning`，语言课程也依赖它；前端 ReviewCenter 仍位于 `features/learning`，并反向依赖 Language 的 `reviewMatch.ts`。当前是共享能力与 UI 归属混在一起。
- **News / RSS**：Rust bounded context 已明确分离，各有领域、application service 和存储；前端仍共享 RSS 的 HTML 清洗 helper 与全局类型。Server 目前缺独立 RSS HTTP API。
- **Travel**：业务域已有后端模型和 API，但路线/行程计算与持久化仍有 UI 职责，Travel 页面还直接连接 Geography、History 和 Learning。这是最需要完整纵向迁移的模块。
- **Home**：前端页面直接调用多个业务 client/helper；后端首页能力目前落在 Learning/Todays dashboard 命名下，没有独立 Home 模块命名。
- **组合与契约**：server 路由大多由 `routes.rs` 统一注册；部分 response 直接序列化 core/application 类型；没有完整 OpenAPI/契约兼容门禁。

## 5. 迁移顺序

迁移按“先模块独立、后跨模块集成”执行。每一阶段只改一个明确边界，保留现有数据和用户行为，并做该模块的局部验证；不以一次全仓重写作为验收条件。

### 阶段 0：固定模块契约

- 维护本文件作为边界清单；逐步建立强类型 API DTO 与版本化契约检查。
- 记录每个模块的输入、输出、事实源、持久化、外部能力和局部验收命令。
- 用架构检查阻止新的跨模块 store/client/import。

### 阶段 1：先收敛前端模块所有权

- 模块 DTO、API client、页面样式和测试迁到所属 feature 目录。
- 从 `App.tsx` 移出各模块刷新/徽标/数据装配；外壳只保留路由、导航和模块间意图。
- 将 RSS/News 共用 HTML 内容处理移至中立 shared 内容层。
- 把 Review UI 提为 `features/review`；抽离通用答案匹配规则，移除 Review 对 Language 私有 helper 的反向依赖。

### 阶段 2：收敛服务端模块入口

- 将各模块 routes、DTO、错误映射从总 `routes.rs` 移至模块 API 文件，由总 router 只 merge。
- 为 RSS 建立独立 `/api/v1/rss/*` 契约；整理 Learning/Review 与 Language review 双入口的 canonical 归属。
- Home API 明确命名为 dashboard/Home query，避免以 Language/Learning 路径承载首页语义。

### 阶段 3：逐个完成业务模块闭环

顺序：History → Geography/Exploration → News 与 RSS → Language/Learning/Review → Travel。

每个闭环需满足：模块 domain/application/adapter/API/client/UI 路径完整；模块数据由自身事实源拥有；不依赖兄弟模块的内部实现；具备模块级单测/API 契约检查/前端 API mock 测试；存在不加载 React/Tauri 的独立 API 冒烟方式。

其中 Travel 作为单独纵向迁移：先把行程规则、编辑状态、持久化和计划操作收进后端，再提供稳定 API/client，最后删除 UI 作为唯一事实源的本地状态。

### 阶段 4：最后接入聚合模块

- Review 通过来源 adapter 注册各业务模块的可复习项，不能直接读语言或其他模块数据库。
- Home 只依赖模块公开的 dashboard query，组合概览数据；失败按模块独立降级。
- 跨模块链接使用 owner 明确的 ID/版本化 DTO/port；UI 的跨页导航由 shell 处理。

## 6. 单模块完成定义

在声明一个模块“独立完成”前，逐条确认：

- [ ] Domain 不包含 UI、HTTP、SQL 或兄弟模块内部类型。
- [ ] Application 用例通过端口访问外部资源，不直接依赖兄弟模块的 store/repository。
- [ ] Infrastructure 只实现本模块端口或真正共享的技术能力。
- [ ] API 有本模块 route、稳定 DTO 和错误语义，不透出内部持久化类型。
- [ ] Frontend 只调用本模块 client，组件不承载业务规则和跨模块数据获取。
- [ ] 数据刷新后可从后端恢复；持久化事实源唯一且归属明确。
- [ ] 模块有独立的 backend tests、API contract checks、frontend client/component tests。
- [ ] 至少一个无 React/Tauri 的薄客户端能消费模块 API。
- [ ] 需要跨模块能力时，依赖方向、port owner、ID 与失败行为均有记录。

## 7. 与当前改动的关系

本路线以当前工作树为基线，不重写或覆盖已有未提交的 Windows、Travel 和其他改动。后续每个模块迁移都必须先读取当时的 diff，限定触及范围，并在完成时明确报告局部验证结果；未执行全量验证时不得宣称全仓通过。
