import type { HistoryDetailView, HistoryNode } from "../../../types";
import { historyEras, type HistoryEra } from "../historyEras";
import type { HistoryGeoNavigationRequest } from "../types/history";
import { DetailShell } from "./DetailShell";
import { DynastyDetail } from "./DynastyDetail";
import { PersonDetail } from "./PersonDetail";
import { EventDetail } from "./EventDetail";
import { WarDetail } from "./WarDetail";
import { PlaceDetail } from "./PlaceDetail";
import { InstitutionDetail } from "./InstitutionDetail";
import { ArtifactDetail } from "./ArtifactDetail";
import { CultureDetail } from "./CultureDetail";

function eraForNode(node: HistoryNode): HistoryEra | null {
  return (
    historyEras.find((era) => era.id === node.id || era.id === node.period_id) ??
    null
  );
}

/** 生成「在地图中查看」的低耦合请求（仅当该类型携带可定位信息）。 */
function mapRequestFor(view: HistoryDetailView): HistoryGeoNavigationRequest | null {
  const node = view.document.node;
  const detail = view.document.detail;
  if (node.kind === "place" && detail.detail_type === "place") {
    return {
      mode: "history",
      entityType: "place",
      entityId: node.id,
      entityName: node.title,
      longitude: detail.longitude,
      latitude: detail.latitude,
    };
  }
  if (node.kind === "war") {
    return {
      mode: "history",
      entityType: "war",
      entityId: node.id,
      entityName: node.title,
    };
  }
  return null;
}

/**
 * DetailRouter：按节点类型把 DetailView 分发到专属布局组件。
 */
export function DetailRouter({
  view,
  favorite,
  onBack,
  onToggleFavorite,
  onOpenNode,
  onOpenEra,
  onNavigateToGeography,
}: {
  view: HistoryDetailView;
  favorite: boolean;
  onBack: () => void;
  onToggleFavorite: () => void;
  onOpenNode: (node: HistoryNode) => void;
  onOpenEra: (era: HistoryEra) => void;
  onNavigateToGeography?: (request: HistoryGeoNavigationRequest) => void;
}) {
  const node = view.document.node;
  const era = eraForNode(node);
  const mapRequest = mapRequestFor(view);

  let body: React.ReactNode = null;
  switch (node.kind) {
    case "dynasty":
      body = <DynastyDetail view={view} era={era} onOpenEra={onOpenEra} />;
      break;
    case "person":
      body = <PersonDetail view={view} onOpenNode={onOpenNode} />;
      break;
    case "event":
      body = <EventDetail view={view} onOpenNode={onOpenNode} />;
      break;
    case "war":
      body = <WarDetail view={view} onOpenNode={onOpenNode} />;
      break;
    case "place":
      body = <PlaceDetail view={view} onOpenNode={onOpenNode} />;
      break;
    case "institution":
      body = <InstitutionDetail view={view} onOpenNode={onOpenNode} />;
      break;
    case "artifact":
      body = <ArtifactDetail view={view} onOpenNode={onOpenNode} />;
      break;
    case "culture":
      body = <CultureDetail view={view} onOpenNode={onOpenNode} />;
      break;
  }

  return (
    <DetailShell
      node={node}
      view={view}
      favorite={favorite}
      onBack={onBack}
      onToggleFavorite={onToggleFavorite}
      onOpenNode={onOpenNode}
      onOpenEra={onOpenEra}
      onMap={
        mapRequest && onNavigateToGeography
          ? () => onNavigateToGeography(mapRequest)
          : undefined
      }
    >
      {body}
    </DetailShell>
  );
}