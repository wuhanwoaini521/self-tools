import { describe, expect, it } from "vitest";
import { GLOBE_RADIUS, isFacingCamera, latLngToVector3 } from "./globeMath";
import { landformGlobeMarkers } from "./globeLandformMarkers";
import { LANDFORM_KNOWLEDGE_ENTRIES } from "./landformKnowledgeData";
import { LANDFORM_LESSONS } from "./landformLessons";

describe("latLngToVector3", () => {
  it("places the north pole on +Y", () => {
    const [x, y, z] = latLngToVector3(90, 0);
    expect(x).toBeCloseTo(0, 6);
    expect(y).toBeCloseTo(GLOBE_RADIUS, 6);
    expect(z).toBeCloseTo(0, 6);
  });

  it("places longitude 0 on the equator at +X", () => {
    const [x, y, z] = latLngToVector3(0, 0);
    expect(x).toBeCloseTo(GLOBE_RADIUS, 6);
    expect(y).toBeCloseTo(0, 6);
    expect(z).toBeCloseTo(0, 6);
  });

  it("hides points on the far hemisphere", () => {
    const front = latLngToVector3(0, 0);
    expect(isFacingCamera(front, { x: 4, y: 0, z: 0 })).toBe(true);
    expect(isFacingCamera(front, { x: -4, y: 0, z: 0 })).toBe(false);
  });
});

describe("landformGlobeMarkers", () => {
  it("covers every landform knowledge entry with WGS84 coordinates", () => {
    const markers = landformGlobeMarkers();
    expect(markers).toHaveLength(LANDFORM_KNOWLEDGE_ENTRIES.length);
    for (const marker of markers) {
      expect(marker.latitude).toBeGreaterThanOrEqual(-90);
      expect(marker.latitude).toBeLessThanOrEqual(90);
      expect(marker.longitude).toBeGreaterThanOrEqual(-180);
      expect(marker.longitude).toBeLessThanOrEqual(180);
    }
  });

  it("anchors well-known examples to their regional WGS84 centers", () => {
    const byId = Object.fromEntries(
      landformGlobeMarkers().map((marker) => [marker.id, marker]),
    );
    const expected: Array<[string, number, number]> = [
      ["volcano", 35.36, 138.72],
      ["shield-volcano", 19.48, -155.58],
      ["mesa", 37.0, -110.05],
      ["u-valley-fjord", 61.1, 6.5],
      ["barrier-lagoon", 35.2, -75.5],
      ["rift-valley", -1.2, 36.2],
      ["plateau", 33, 88],
      ["basin", 30.5, 105.2],
    ];
    for (const [id, latitude, longitude] of expected) {
      expect(byId[id].latitude).toBeCloseTo(latitude, 1);
      expect(byId[id].longitude).toBeCloseTo(longitude, 1);
    }
  });

  it("has a kid-facing lesson for every landform entry", () => {
    for (const entry of LANDFORM_KNOWLEDGE_ENTRIES) {
      expect(LANDFORM_LESSONS[entry.id], entry.id).toBeTruthy();
    }
  });
});
