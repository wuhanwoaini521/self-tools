# Geography Explorer 架构基线

状态：V1 实现基线（2026-09）。本文件先于代码落地，所有实现以仓库实际结构为准。

## 1. 当前项目架构

仓库是 Rust workspace + Tauri 2 + React 19：

```text
React features/*
    ↓ @tauri-apps/api/core invoke
Tauri command adapter (apps/desktop/src/lib.rs)
    ↓
application crate（用例编排 / DTO）
    ↓
core crate（无 I/O 的领域模型和纯规则）
    ↓
infrastructure crate（SQLite / 文件 / HTTP / 数据适配器）
```

现有模块是 Home、Markdown、RSS、Travel、History、Language。每个页面在 `App.tsx` 中常驻挂载，通过 `page-hidden` 切换；导航是一个注册表。SQLite 模块使用独立数据库文件和 `Arc<Mutex<Store>>`，Tauri 统一通过 `CommandError` 返回可序列化错误。

## 2. Geography 应该放在哪里

沿用现有四层边界，不新增 crate，不改造共享数据库：

```text
ui/src/features/geography/
    ↓
apps/desktop/src/lib.rs: geography_*
    ↓
crates/application/src/geography/
    ↓
crates/core/src/geography/
    ↓
crates/infrastructure/src/geography/ (geography.db)
```

Geography 是独立 Feature，放在 History 与 Language 附近；Travel 只保留未来通过 deep link 进入 Geography 的接口，不让两个模块互相依赖。

## 3. 可以复用的现有模块

| 能力 | 复用来源 | 用法 |
| --- | --- | --- |
| SQLite 初始化、幂等种子、用户数据隔离 | `history/store.rs` | 新建 `geography.db`，种子只 `INSERT OR IGNORE` |
| `Arc<Mutex<Store>>` 与服务层 | History / Language | 所有查询走 application service |
| `serde` DTO + Tauri `invoke` | 全部已有模块 | 前后端使用 snake_case DTO |
| 确定性推荐 | `HistoryRecommendationService` | 排除最近浏览并轮换推荐类型 |
| CSS Design Tokens | `styles.css` | Geography 只消费已有主题变量 |
| 页面常驻挂载 + 导航注册表 | `App.tsx` | 新增一个导航项和一个 page pane |
| 可追溯来源展示 | History / Language | 每个详情页显示数据集、版本、许可、更新时间 |

## 4. Domain Model

`GeoEntity` 是统一骨架，不把 Country、City、River 拆成互不相关的类型：

- `GeoEntityType`：Country、Region、Province、City、River、Mountain、MountainRange、Plateau、Plain、Basin，以及后续可扩展的水体/气候/板块类型。
- `GeoCoordinate`：坐标系与经纬度绑定，支持 `WGS84 / GCJ02 / BD09`，禁止无语义的 `lat/lng`。
- `GeoEntity`：稳定 id、类型、中英文名、别名、坐标、父级、属性、摘要、来源。
- `GeoRelation`：`LOCATED_IN`、`PART_OF`、`FLOWS_THROUGH`、`SOURCE_OF`、`AFFECTS`、`FORMED_BY` 等带方向关系。
- `GeoSource`：数据集、版本、官方地址、许可、字段范围、更新时间。
- `GeoRecommendation`、`GeoCompareView`：应用层消费的推荐和比较结果，不污染基础事实模型。

V1 的“为什么”使用预置、带来源的解释文本；AI 不参与坐标、边界、人口、面积等事实写入。

## 5. Database Schema

`geography.db`：

```text
geo_entities  (id, type, name, name_en, aliases_json, coordinates_json,
               parent_id, summary, properties_json, source_ids_json)
geo_relations (from_id, to_id, kind, note, source_ids_json)
geo_sources   (id, payload_json)  -- payload 为带版本/许可/字段的 GeoSource
geo_favorites (entity_id, created_at)
geo_views     (entity_id, viewed_at, view_count)
geo_searches  (query, searched_at, search_count)
```

V1 通过稳定 id、JSON 属性和关系表保持演进空间；几何暂不写入复杂多边形，避免在没有正式数据许可和简化策略前提交大型原始数据。地图使用实体坐标和本地关系线，后续可增加 `geo_geometries` 或矢量瓦片索引。

## 6. Map Architecture

当前首页地图使用 MapLibre GL JS：由 WebGL 负责地图渲染、缩放、旋转和图层管理；实体点与关系线由应用转换为 GeoJSON source/layer。Mapterhorn 提供 `raster-dem` 地形瓦片，分别作为 3D terrain 和 hillshade 的数据源，地形模式使用两个独立 DEM source，避免地形与阴影争抢瓦片缓存。

地图适配器保持如下边界：

```text
GeoMapAdapter
└── MapLibreTerrainAdapter
```

MapLibre 本身是渲染库，不提供底图或地形数据；当前运行时使用 OpenStreetMap 栅格底图与 Mapterhorn DEM，均通过网络请求加载。MapLibre 与 Mapterhorn 的代码按各自 BSD-3-Clause 许可使用，地形数据仍需按 Mapterhorn attribution 页面标注；底图保留 OpenStreetMap 归属。MapLibre 模块采用动态导入，地图 chunk 只在 Geography 页面需要时加载。网络或瓦片初始化失败时，只展示清晰的加载失败提示，不再渲染手绘或静态占位图。

## 7. Geographic Data Strategy

V1 只内置少量人工审核的实体事实和关系，所有事实绑定 `GeoSource`，不提交大数据集。候选数据源、许可和字段边界见 `GEOGRAPHY_DATA_SOURCES.md`。

- 全球小比例尺边界/自然地理：优先评估 Natural Earth（公有领域）。
- 地名与别名：评估 GeoNames（CC BY 4.0）和 Wikidata 结构化数据（CC0）。
- 人口/GDP：需要时再按指标版本导入 World Bank（默认 CC BY 4.0，但逐数据集复核）。
- 中国地图：不从随机 GeoJSON 直接使用；先核验自然资源部标准地图/天地图的授权、坐标、精度和公开使用限制。
- OSM：只在能承担 ODbL 归属和派生数据库义务时使用，不作为当前 V1 的基础数据。
- Mapterhorn：运行时地形 DEM 瓦片，使用 `https://tiles.mapterhorn.com/tilejson.json` 的 Terrarium 编码；不把第三方地形数据打包进应用，按其 attribution 页面展示数据来源。

## 8. Search Strategy

搜索是 `GeoEntity Search`，不是网页全文搜索：

1. 精确 id / 名称优先；
2. 中文名、英文名、别名统一归一化匹配；
3. 再匹配摘要、标签和属性；
4. 结果按实体类型分组，支持类型过滤和 limit；
5. 未来可以把相同查询下沉到 SQLite FTS5，但不改变 application API。

## 9. UI Information Architecture

```text
Geography
├── Explore（首页、Daily Discovery、搜索、继续探索）
├── Map（首页内的探索画布）
├── China（通过中国实体和相关实体进入）
└── Knowledge（地貌知识与阅读入口）
```

V1 用一个一级导航项承载这些子体验，避免占满全局 Sidebar。实体详情在同一页面的侧栏/面板展开，保持用户的探索上下文。

## 10. V1 范围

实现：问题式 Daily Discovery、实体搜索、实体详情、基础关系、世界/中国探索画布、Administrative/Terrain/River 图层语义、Country/City/River/Mountain/Region 详情、地貌知识阅读、收藏、最近探索、Sources。

不实现：在线地图服务、导航、实时天气/交通、专业 GIS、3D Globe、复杂投影转换、Graph DB、Vector DB、RAG 和多 Agent。

## 11. 新增文件

- `docs/geography/architecture.md`
- `docs/geography/GEOGRAPHY_DATA_SOURCES.md`
- `crates/core/src/geography/{mod.rs,model.rs,recommendation.rs,compare.rs}`
- `crates/application/src/geography/{mod.rs,service.rs}`
- `crates/infrastructure/src/geography/{mod.rs,store.rs}`
- `apps/desktop/ui/src/features/geography/{GeographyPage.tsx,GeoMap.tsx}`

## 12. 修改文件

- `crates/core/src/lib.rs`
- `crates/application/src/lib.rs`
- `crates/infrastructure/src/lib.rs`
- `apps/desktop/src/lib.rs`
- `apps/desktop/ui/src/{App.tsx,types.ts,styles.css}`

不修改现有 History、Travel、Language 的业务逻辑；当前工作区已经存在的 History 与全局样式改动保留。
