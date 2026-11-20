import { LANDFORM_KNOWLEDGE_ENTRIES } from "./landformKnowledgeData";
import { getRealTerrain } from "./realTerrainData";

export interface LandformGlobeMarker {
  id: string;
  title: string;
  subtitle: string;
  example: string;
  /** 观测区地名，与气泡坐标对应。 */
  place: string;
  exampleId: string;
  category: string;
  latitude: number;
  longitude: number;
  featured: boolean;
}

/** 进入地球时优先展开完整气泡的代表性地貌。 */
export const FEATURED_LANDFORM_IDS = [
  "mountains",
  "plateau",
  "basin",
  "volcano",
  "shield-volcano",
  "island-arc",
  "river",
  "canyon",
  "delta",
  "glacier",
  "u-valley-fjord",
  "karst",
  "dunes",
  "coast",
  "mesa",
  "landslide",
] as const;

const FEATURED = new Set<string>(FEATURED_LANDFORM_IDS);

export function landformGlobeMarkers(): LandformGlobeMarker[] {
  return LANDFORM_KNOWLEDGE_ENTRIES.map((entry) => {
    const region = getRealTerrain(entry.id);
    return {
      id: entry.id,
      title: entry.title,
      subtitle: entry.subtitle,
      example: entry.example,
      place: region.place,
      exampleId: entry.exampleId,
      category: entry.category,
      latitude: region.center[1],
      longitude: region.center[0],
      featured: FEATURED.has(entry.id),
    };
  });
}

export function landformCategories(markers = landformGlobeMarkers()): string[] {
  return [...new Set(markers.map((marker) => marker.category))];
}
