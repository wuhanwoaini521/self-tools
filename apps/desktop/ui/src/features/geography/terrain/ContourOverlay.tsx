import { useEffect, useMemo } from "react";
import * as THREE from "three";
import { buildTopSurfaceGeometry } from "./geometry";
import type { Heightfield } from "./heightfield";
import type { TerrainPreset } from "./presets";

const VERT = /* glsl */ `
varying vec3 vWorld;
void main() {
  vWorld = position;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}
`;

const FRAG = /* glsl */ `
uniform float uInterval;
uniform float uBase;
uniform float uMajorEvery;
uniform float uOpacity;
varying vec3 vWorld;
void main() {
  float band = (vWorld.y - uBase) / uInterval;
  float k = floor(band + 0.5);
  float f = abs(band - k);
  float fw = max(fwidth(band), 0.0006);
  float line = 1.0 - smoothstep(0.0, fw * 1.15, f);
  float isIndex = mod(k + 0.5, uMajorEvery) < 0.5 ? 1.0 : 0.0;
  float alpha = line * mix(0.16, 0.42, isIndex) * uOpacity;
  vec3 col = mix(vec3(0.55, 0.5, 0.44), vec3(0.42, 0.38, 0.33), isIndex);
  gl_FragColor = vec4(col, alpha);
}
`;

const MATERIAL_POOL = new WeakMap<THREE.BufferGeometry, THREE.ShaderMaterial>();

/**
 * Contour lines drawn directly on the terrain surface via a height-derived fragment
 * shader — thin, low-contrast, never occluding the hillshaded relief underneath.
 */
export function ContourOverlay({
  heightfield,
  preset,
}: {
  heightfield: Heightfield;
  preset: TerrainPreset;
}) {
  const geometry = useMemo(
    () => buildTopSurfaceGeometry(heightfield),
    [heightfield],
  );
  useEffect(() => () => geometry.dispose(), [geometry]);

  const material = useMemo(() => {
    const cached = MATERIAL_POOL.get(geometry);
    if (cached) return cached;
    const mat = new THREE.ShaderMaterial({
      vertexShader: VERT,
      fragmentShader: FRAG,
      uniforms: {
        uInterval: { value: preset.contourInterval },
        uBase: { value: preset.contourBase },
        uMajorEvery: { value: 5 },
        uOpacity: { value: 1 },
      },
      transparent: true,
      depthWrite: false,
      polygonOffset: true,
      polygonOffsetFactor: -1,
      polygonOffsetUnits: -1,
    });
    MATERIAL_POOL.set(geometry, mat);
    return mat;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [geometry]);

  useEffect(() => {
    material.uniforms.uInterval.value = preset.contourInterval;
    material.uniforms.uBase.value = preset.contourBase;
  }, [material, preset]);

  return <mesh geometry={geometry} material={material} renderOrder={2} />;
}
