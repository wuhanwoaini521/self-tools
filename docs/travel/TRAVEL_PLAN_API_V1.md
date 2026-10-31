# Travel Plan Persistence API — v1

This contract stores user itinerary state independently from generated city guides. Desktop Tauri commands and HTTP clients use the same `TravelStore` and `TravelPlan` domain DTO. The backend owns persistence and optimistic concurrency; clients own presentation and local recovery journals.

## Read one plan

`GET /api/v1/travel/plans?city={city}&days={days}`

For a date-specific plan, include both `date_start=YYYY-MM-DD` and `date_end=YYYY-MM-DD`. Include neither for an undated plan. Supplying only one date is invalid. The response is a `TravelPlan` object or JSON `null` when no plan exists.

## List recent plans

`GET /api/v1/travel/plans/recent?limit=20`

Returns an array of saved plans ordered from most recently updated to least recently updated. `limit` defaults to 20 and is bounded to 1–100. An empty list means no saved plans are available.

## Save a plan

`POST /api/v1/travel/plans`

```json
{
  "plan": {
    "context": {
      "city": "杭州",
      "days": 2,
      "date_range": { "start": "2026-10-01", "end": "2026-10-02" }
    },
    "title": "西湖周末行",
    "departure": "杭州东站",
    "notes": "预留午休",
    "custom_places": {
      "custom:walk": { "name": "河坊街", "note": "晚餐后步行" }
    },
    "selected_keys": ["poi-west-lake", "custom:walk"],
    "assigned_days": { "poi-west-lake": 1, "custom:walk": 2 },
    "must_visit_keys": ["poi-west-lake"],
    "revision": 0,
    "updated_at": 0
  }
}
```

The response contains the stored plan. New plans use `revision: 0`; the backend increments and returns the revision after each accepted save. Clients must send the last revision they read as the expected revision. `updated_at` is assigned by the backend; client-supplied timestamps are ignored.

A stale revision is rejected with HTTP 409 and code `travel_plan_conflict`. The server does not replace the newer plan. Clients should load the current version and either discard their draft or explicitly retry against the new revision after the user chooses to overwrite.

Invalid writes return HTTP 400 with code `travel_plan_invalid`. Context is validated and city names are normalized. Days must be 1–30; dates must be valid ISO calendar dates; assignments must fit the trip; keys and text are bounded; custom places are limited to 100; selected and must-visit collections are bounded and deduplicated.

`must_visit_keys` is a wish list and may include a place not currently selected for the itinerary. `assigned_days` may retain a place's last day after it is removed, so re-adding it can preserve the user's choice. `selected_keys` order is the itinerary order.

## Persistence, migration, and recovery

Plans are stored in backend `travel.db`, keyed by normalized city, day count, and optional date range. The `revision` column is added additively with default `0`; existing plan rows are retained. Older JSON records deserialize with defaults for newly added fields.

The UI reads the backend first. If no backend plan exists, a matching legacy `localStorage` entry is imported and removed only after the backend save succeeds. New edits are debounced writes. A per-tab local-storage journal is an outbox for failed-save recovery, not the source of truth. On a version conflict, the local draft remains available until the user chooses the server version or explicitly overwrites it.

Tauri clients use `travel_plan_get`, `travel_plan_save`, and `travel_plan_recent` with the same plan DTO and revision semantics.

## Deployment boundary

The server refuses non-loopback bind addresses until an authenticated administration boundary exists. Requests must use a loopback `Host`; unsafe methods additionally require a loopback `Origin`. These checks mitigate browser CSRF and DNS rebinding but are **not authentication** and do not make the API safe for remote exposure. CORS remains disabled, and request bodies are capped at 1 MiB.

This is a persistence slice, not the complete Travel API: trip CRUD, optimization and validation, budgets, notebook, versions/undo, and AI propose/apply semantics remain follow-up work.
