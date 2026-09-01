# 内置自然地形底图

- 文件：`apps/desktop/ui/public/geography/natural-earth-50m-hypso-mercator.jpg`
- 上游数据：Natural Earth 1:50m Cross-blended Hypsometric Tints with Shaded Relief and Water（`HYP_50M_SR_W`）
- 上游地址：https://www.naturalearthdata.com/downloads/50m-raster-data/50m-cross-blend-hypso/
- 数据许可：Public Domain。

该资源用于 Geography 的世界自然地形视图。生成时将原始 WGS84 地理坐标栅格缩放到 6144×6144，并重投影为 Web Mercator，因此可用高德 JS API 的 `ImageLayer` 在 `[-180, -85.05112878, 180, 85.05112878]` 范围内随地图缩放与平移。

这是中小比例尺的地理图册底图，适合识别高原、盆地、山脉、平原和海陆轮廓；不用于测绘、导航或边界判定。放大到国家/城市级别时，应接入更高分辨率的区域 DEM 或地形瓦片。
