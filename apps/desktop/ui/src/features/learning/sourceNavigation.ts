/** Stable API-source references never depend on a collection row or browser cache. */
export function sourceRoute(
  module: string,
  entityType: string,
  entityId: string,
): string | null {
  const id = encodeURIComponent(entityId);
  if (module === "geography") return `#geography?id=${id}`;
  if (module !== "history") return null;

  if (entityType === "event") return `#history?event=${id}`;
  if (entityType === "person") return `#history?person=${id}`;
  if (entityType === "story") return `#history?story=${id}`;
  return null;
}
