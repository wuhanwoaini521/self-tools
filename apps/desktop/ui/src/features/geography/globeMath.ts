/** 球面半径，与 GlobeExplorer 中的 mesh 保持一致。 */
export const GLOBE_RADIUS = 2;

const DEG = Math.PI / 180;

/**
 * 经纬度 → Three.js SphereGeometry 顶点约定。
 * 与默认 `phiStart=0`、`thetaStart=0` 的等距圆柱贴图对齐：经度 0° 落在 +X。
 */
export function latLngToVector3(
  latitude: number,
  longitude: number,
  radius = GLOBE_RADIUS,
): [number, number, number] {
  const phi = (longitude + 180) * DEG;
  const theta = (90 - latitude) * DEG;
  const sinTheta = Math.sin(theta);
  return [
    -radius * Math.cos(phi) * sinTheta,
    radius * Math.cos(theta),
    radius * Math.sin(phi) * sinTheta,
  ];
}

/** 点是否朝向相机（用于背面气泡隐藏）。原点在球心。 */
export function isFacingCamera(
  point: [number, number, number],
  camera: { x: number; y: number; z: number },
): boolean {
  return point[0] * camera.x + point[1] * camera.y + point[2] * camera.z > 0;
}
