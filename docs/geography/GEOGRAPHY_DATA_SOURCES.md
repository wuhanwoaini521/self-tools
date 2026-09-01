# Geography 数据源调查

状态：V1 选型记录（2026-09）。首页地图已接入 MapLibre GL JS + Mapterhorn 运行时地形瓦片；正式导入边界或大规模几何前，仍必须为具体下载文件补充精确版本、checksum、坐标系和再分发审核记录。

| Dataset | Purpose | Official Source | Format | License | Commercial Use | Redistribution | Update Frequency | Fields Used | Fields Excluded | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Natural Earth | 全球小比例尺国家/海岸线/自然地理底图 | [naturalearthdata.com](https://www.naturalearthdata.com/about/terms-of-use/) | Shapefile / raster | Public domain（官网条款） | Allowed | Allowed | 版本发布制；导入时锁定版本 | 仅规划中的国界、海岸线、河流和自然地理几何 | 不导入未经审计的详细属性 | 适合作为离线概览底图；V1 仅记录候选，未提交原始文件 |
| Mapterhorn | 运行时 DEM、hillshade 和 3D terrain | [github.com/mapterhorn/mapterhorn](https://github.com/mapterhorn/mapterhorn) / [tiles.mapterhorn.com](https://tiles.mapterhorn.com/) | TileJSON / WebP Terrarium tiles | 代码 BSD-3-Clause；地形数据按 attribution 页面逐项核验 | 按数据源条款 | 按数据源条款 | 运行时服务；以 TileJSON 当前元数据为准 | raster-dem elevation tiles | 不下载、不打包整套地形瓦片 | 与 MapLibre 的 raster-dem、hillshade 和 terrain 能力直接兼容 |
| GeoNames | 地名、别名、坐标、人口候选字段 | [geonames.org/export](https://www.geonames.org/export/) | TSV / API | CC BY 4.0 | Allowed with attribution | Allowed with attribution | 持续更新；下载时记录日期 | name、alternate names、coordinates、feature class | 不直接采用其不明来源的边界/解释 | 适合搜索和地名补全，但质量按记录复核 |
| OpenStreetMap | 详细道路、水系、居民地和 POI | [openstreetmap.org/copyright](https://www.openstreetmap.org/copyright) | PBF / OSM XML / vector tiles | ODbL | Allowed with obligations | Share-alike database obligations | 持续更新 | 只有未来确需的水系/城市几何 | V1 不导入道路、POI、整库 | 许可和归属义务较重，V1 不依赖 |
| Wikidata | 跨语言名称、稳定实体关系和补充属性 | [Wikidata licensing](https://www.wikidata.org/wiki/Wikidata:Licensing) | JSON / RDF / SPARQL | Structured data CC0 | Allowed | Allowed | 持续更新；快照需记录日期 | labels、aliases、external ids、references | 不把百科正文当事实数据库 | 适合实体对齐和关系补充，事实仍需逐项来源审计 |
| World Bank Open Data | 人口、GDP、城市化等动态指标 | [World Bank data licenses](https://datacatalog.worldbank.org/public-licenses) | API / CSV | 默认 CC BY 4.0；逐数据集复核 | Usually allowed with attribution | Usually allowed with attribution | 指标各自更新 | indicator、country、year、value、unit | 不写入无年份人口数字 | 只在需要比较时导入带年份的指标 |
| NASA Earthdata | 未来地形、遥感、地球科学专题 | [NASA Earthdata data-use policy](https://www.earthdata.nasa.gov/engage/open-data-services-software/data-use-policy) | HDF / GeoTIFF / NetCDF 等 | 通常开放；非 NASA 数据逐项核验 | 需按产品条款 | 需按产品条款 | 产品各自更新 | 未来专题数据和元数据 | V1 不引入栅格处理 | 数据体量和处理复杂度超出 V1 |
| NOAA | 气候、海洋和环境时间序列 | [NOAA National Centers for Environmental Information](https://www.ncei.noaa.gov/) | NetCDF / CSV / API | 按具体产品条款 | 按具体产品条款 | 按具体产品条款 | 产品各自更新 | 未来气候统计 | V1 不接实时天气 | 与“地理为什么”有关，但不是 V1 的离线基础依赖 |
| 天地图 / 国家地理信息公共服务平台 | 中国权威在线地图、地名、水系和政区候选 | [国家基础地理信息中心](https://www.ngcc.cn/zdchgc/tdtjs/) / [标准地图服务](https://bzdt.tianditu.gov.cn/) | 在线服务 / 栅格 / 矢量接口（以授权为准） | 以具体服务条款、授权和地图审核要求为准 | 未确认前不假设允许 | 未确认前不假设允许 | 平台持续更新 | 未来仅在明确授权后使用 | V1 不抓取、不镜像、不提交服务数据 | 中国地图必须先核验来源、坐标系、精度和公开使用限制 |

## V1 实际使用

应用不随包携带上述在线瓦片数据，也不在 CI 访问网络。内置 `geography.db` 只包含少量可审计的探索实体、关系和属性，来源字段指向上述官方数据源或人工审核的专题来源。首页地图运行时通过 MapLibre 请求 OpenStreetMap 栅格底图和 Mapterhorn DEM；该数据库版本标记为 `geography-seed-v1`，不是任何第三方数据集的完整镜像。

## 坐标与中国地图记录

- 内置探索点统一使用 `WGS84`，数据库模型保留 `GCJ02` 与 `BD09` 枚举，但 V1 不做自动转换。
- 目前不内置中国正式行政边界，不把 MapLibre 地图当作法定地图或测绘成果。
- 未来若接入中国边界，必须在同一条数据记录中补充：官方来源、具体版本/发布日期、许可或授权、坐标系统、精度/比例尺、公开使用限制和 checksum。
