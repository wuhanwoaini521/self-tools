# Exploration API v1 — Geography / History crosswalk

**Status:** additive, backward-compatible HTTP and Tauri commands.

The cross-module boundary uses canonical dataset IDs, never display-name equality.

## `GET /api/v1/exploration/geography/{geography_entity_id}/history`

The Tauri command is `exploration_location_history({ id })`.

Returns `404` when the Geography entity does not exist. For a known Geography entity it returns:

```json
{
  "geography": { "entity": { "id": "chengdu" } },
  "crosswalk": {
    "id": "geography:chengdu|history:cbdb-place-406333",
    "geography_entity_id": "chengdu",
    "history_place_id": "cbdb-place-406333",
    "relation_type": "historical_event_place_anchor",
    "status": "reviewed",
    "confidence_percent": null,
    "evidence_time_scope": { "start_year": 756, "end_year": 756, "precision": "year" },
    "geography_source_ids": ["geography-seed"],
    "history_source_ids": ["source-curated-backbone-v1"],
    "provenance": [
      { "source_id": "source-curated-backbone-v1", "record_id": "event-place-806a475743df317548dd51e2", "uri": "history-data-pipeline/data/curated/history_backbone/events/sui_tang/event-anlu-xuanzong-shu.yml", "note": "Reviewed event-place anchor; not a claim that all historical Shu territory equals modern Chengdu." },
      { "source_id": "source-curated-backbone-v1", "record_id": "event-anlu-xuanzong-shu", "uri": "history-data-pipeline/data/curated/history_backbone/events/sui_tang/event-anlu-xuanzong-shu.yml", "note": "The evidence time scope is the reviewed event year, not the History place validity interval." }
    ]
  },
  "crosswalks": [
    {
      "id": "geography:chengdu|history:cbdb-place-406333",
      "geography_entity_id": "chengdu",
      "history_place_id": "cbdb-place-406333",
      "relation_type": "historical_event_place_anchor",
      "status": "reviewed",
      "confidence_percent": null,
      "evidence_time_scope": { "start_year": 756, "end_year": 756, "precision": "year" },
      "geography_source_ids": ["geography-seed"],
      "history_source_ids": ["source-curated-backbone-v1"],
      "provenance": [
        { "source_id": "source-curated-backbone-v1", "record_id": "event-place-806a475743df317548dd51e2", "uri": "history-data-pipeline/data/curated/history_backbone/events/sui_tang/event-anlu-xuanzong-shu.yml", "note": "Reviewed event-place anchor; not a claim that all historical Shu territory equals modern Chengdu." },
        { "source_id": "source-curated-backbone-v1", "record_id": "event-anlu-xuanzong-shu", "uri": "history-data-pipeline/data/curated/history_backbone/events/sui_tang/event-anlu-xuanzong-shu.yml", "note": "The evidence time scope is the reviewed event year, not the History place validity interval." }
      ]
    }
  ],
  "events": [
    { "event": { "id": "event-anlu-xuanzong-shu", "name_zh_cn": "唐玄宗入蜀", "start_year": 756, "end_year": 756, "quality_status": "reviewed" }, "places": [{ "place_id": "cbdb-place-406333", "link_status": "linked", "link_quality_status": "reviewed" }], "sources": [{ "id": "source-curated-backbone-v1" }] }
  ]
}
```

### Reviewed event-to-story associations

`GET /api/v1/history/events/{event_id}` adds the backward-compatible `related_stories` array to event details:

```json
{
  "event": { "id": "event-anlu-luoyang", "quality_status": "reviewed" },
  "related_stories": [
    {
      "event_id": "event-anlu-luoyang",
      "story_id": "story-an-lushan-rebellion",
      "sequence": 3,
      "link_quality_status": "reviewed",
      "story": {
        "id": "story-an-lushan-rebellion",
        "title_zh_cn": "安史之乱",
        "quality_status": "reviewed",
        "usable": true
      }
    }
  ],
  "sources": [{ "id": "source-curated-backbone-v1" }]
}
```

Only exact event/story IDs with reviewed links, reviewed events/stories, and usable story records are returned. Sources referenced by related stories are merged into the existing event `sources` list. Consumers may fetch the story detail for its reviewed event sequence; story-context events do not establish that an event occurred at a Geography location. Place claims still require the reviewed stable History place ID and its evidence scope described below.

`crosswalks` is the additive multi-result field. The legacy `crosswalk` field remains populated only when exactly one reviewed link exists; with several links it is `null` rather than silently selecting one. For clients that may call an older v1 server, the newer relation, time-scope, and provenance fields are optional on the wire.

`crosswalk: null`, `crosswalks: []`, and `events: []` is the normal, honest result when no reviewed link is available. A known Geography ID with an unavailable History source returns an error, not this empty result. Event results include source metadata and only include event-place rows marked linked and reviewed/verified/curated by History; events from several crosswalks are de-duplicated by stable event ID.

## `GET /api/v1/exploration/history-places/{history_place_id}/geography`

The Tauri command is `exploration_geography_for_history_place({ placeId })`.

Returns a Geography entity detail or JSON `null` when no reviewed reverse link exists.

## Reviewed links

- Geography `chengdu` ↔ History `cbdb-place-406333`: reviewed `historical_event_place_anchor`, backed by History event-place record `event-place-806a475743df317548dd51e2` in `event-anlu-xuanzong-shu.yml`. This is an event-place anchor for the 756 event 唐玄宗入蜀, not an assertion that all historical Shu territory equals the modern city.
- Geography `luoyang` (Wikidata Q187136; modern urban area) ↔ History `cbdb-place-14693`: reviewed `modern_urban_area_anchor`, backed by `event-place-ef7ba1621307cfb31edd377c` in `event-anlu-luoyang.yml`. The reviewed event `event-anlu-luoyang` is 洛阳失守 (756). Its event year is the evidence time scope; it is not a full validity interval for the place ID.
- Luoyang Geography sources are Wikidata Q187136 (city) and Q43684 (Henan). The coordinate is a representative point, not a boundary or route.
- History candidate `cbdb-place-100409` is deliberately not linked to modern Luoyang: the `event-three-dong-zhuo` record describes a 189 CE event whose QA note rejects that candidate due to conflicting effective dates and leaves it unresolved.

The local `history.duckdb` contains the reviewed linked event-place rows. No coordinates, transport times, current operating status, or additional History links are inferred. Search never creates links from label equality; add links only after source review and through stable identifiers.
