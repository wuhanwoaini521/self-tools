#![expect(
    unused_crate_dependencies,
    reason = "workspace dev-dependencies are shared across infrastructure integration targets"
)]

use std::sync::Arc;

use devtoolbox_application::backup::BackupService;
use devtoolbox_core::learning::{
    CollectionItemRef, ReviewCardType, ReviewRating, UniversalReviewCard,
};
use devtoolbox_core::travel::{TravelPlan, TravelPlanContext, TravelPlanPlace};
use devtoolbox_infrastructure::backup::SqliteBackupSource;
use devtoolbox_infrastructure::{LearningStore, TravelStore};
use rusqlite::Connection;
use tempfile::TempDir;

fn journal_mode(path: &std::path::Path) -> String {
    let connection = Connection::open(path).expect("open database for journal mode");
    connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("read journal mode")
}

fn integrity_ok(path: &std::path::Path) -> bool {
    let connection = Connection::open(path).expect("open database for integrity check");
    let verdict: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("run integrity check");
    verdict == "ok"
}

#[test]
fn release_recovery_round_trips_user_learning_and_travel_data() {
    let fixture = TempDir::new().expect("isolated fixture");
    let live = fixture.path().join("app-config");
    let backup_dir = fixture.path().join("backup");
    let restored_dir = fixture.path().join("restored-app-config");
    std::fs::create_dir_all(&live).expect("create isolated app config");

    let learning_path = live.join("learning.db");
    let travel_path = live.join("travel.db");
    let now = 1_800_000_000_i64;

    let learning = LearningStore::open(&learning_path).expect("open isolated Learning store");
    let collection = learning
        .create_collection("Release restore fixture", None, &[], now)
        .expect("create collection");
    let item = learning
        .add_collection_item(
            &collection.id,
            CollectionItemRef {
                module: "history".into(),
                entity_type: "event".into(),
                entity_id: "release-restore-event".into(),
                title: "Reviewed History source".into(),
                note: Some("Personal note preserved by backup".into()),
            },
            now,
        )
        .expect("save collection item and note");
    let card = UniversalReviewCard {
        id: "knowledge:history:event:release-restore-event".into(),
        module: "history".into(),
        entity_id: "release-restore-event".into(),
        entity_type: "event".into(),
        card_type: ReviewCardType::Qa,
        prompt: "What does the restore drill prove?".into(),
        answer: "Application-owned data survives a verified restore.".into(),
        options: None,
        hint: None,
        context: None,
        source_title: Some("Reviewed History source".into()),
        user_note: None,
        due_at: now - 10,
        interval_days: 0.0,
        ease: 2.5,
        mastery_score: 0.0,
        repetition_count: 0,
        lapses: 0,
        last_reviewed_at: None,
        created_at: now - 100,
    };
    learning
        .upsert_review_card(&card)
        .expect("create review card");
    let outcome = learning
        .record_review_outcome(&card.id, ReviewRating::Good, now)
        .expect("record SRS review");
    assert_eq!(outcome.repetition_count, 1);

    let travel = TravelStore::open(&travel_path).expect("open isolated Travel store");
    let context = TravelPlanContext {
        city: "杭州".into(),
        days: 2,
        date_range: None,
    };
    let plan = TravelPlan {
        context: context.clone(),
        title: "Release restore trip".into(),
        departure: "Hangzhou East".into(),
        notes: "Keep this user-authored itinerary note".into(),
        custom_places: [(
            "custom:tea-house".into(),
            TravelPlanPlace {
                name: "West Lake Tea House".into(),
                note: "Personal stop note".into(),
            },
        )]
        .into(),
        selected_keys: vec!["custom:tea-house".into()],
        assigned_days: [("custom:tea-house".into(), 2)].into(),
        must_visit_keys: vec!["custom:tea-house".into()],
        ..TravelPlan::default()
    };
    let saved_plan = travel.upsert_plan(&plan, now).expect("save TravelPlan");

    assert_eq!(journal_mode(&learning_path).to_ascii_lowercase(), "wal");
    assert_eq!(
        journal_mode(&travel_path).to_ascii_lowercase(),
        "delete",
        "record TravelStore's actual journal mode rather than assuming WAL"
    );

    let mut backup = BackupService::new();
    backup
        .register(Arc::new(SqliteBackupSource::new(
            "learning",
            &learning_path,
            true,
            false,
            None,
        )))
        .expect("register Learning backup source");
    backup
        .register(Arc::new(SqliteBackupSource::new(
            "travel",
            &travel_path,
            true,
            false,
            None,
        )))
        .expect("register Travel backup source");
    let manifest = backup
        .backup(&backup_dir, "0.1.0", "isolated release recovery drill")
        .expect("snapshot active application stores");
    assert_eq!(manifest.entries.len(), 2);
    assert!(manifest.has_sensitive_entries());
    assert!(manifest.entry("learning.db.bak").is_some());
    assert!(manifest.entry("travel.db.bak").is_some());
    assert!(manifest.entry("settings.json").is_none());
    assert!(integrity_ok(&backup_dir.join("learning.db.bak")));
    assert!(integrity_ok(&backup_dir.join("travel.db.bak")));

    // Destroy only the disposable fixture after closing its active stores.
    drop((learning, travel));
    Connection::open(&learning_path)
        .expect("open isolated source for destructive drill")
        .execute_batch(
            "DELETE FROM collection_items;
             DELETE FROM collections;
             DELETE FROM review_cards;
             DELETE FROM review_submission_receipts;
             DELETE FROM learning_events;
             DELETE FROM learning_progress;",
        )
        .expect("clear isolated Learning data");
    Connection::open(&travel_path)
        .expect("open isolated Travel source for destructive drill")
        .execute("DELETE FROM travel_plans", [])
        .expect("clear isolated TravelPlan");

    let restored = backup
        .restore(&backup_dir, &restored_dir)
        .expect("restore into a separate isolated directory");
    assert!(restored.is_ok(), "{}", restored.summary());
    assert_eq!(restored.restored.len(), 2);
    assert_eq!(restored.verified, restored.restored);

    let restored_learning = LearningStore::open(restored_dir.join("learning.db.bak"))
        .expect("open restored Learning DB");
    let restored_collections = restored_learning
        .list_collections()
        .expect("list restored collections");
    assert_eq!(restored_collections.len(), 1);
    assert_eq!(restored_collections[0].id, collection.id);
    let restored_items = restored_learning
        .list_collection_items(&collection.id)
        .expect("read restored Knowledge source");
    assert_eq!(restored_items.len(), 1);
    assert_eq!(restored_items[0].id, item.id);
    assert_eq!(restored_items[0].entity_id, "release-restore-event");
    assert_eq!(
        restored_items[0].note.as_deref(),
        Some("Personal note preserved by backup")
    );
    let restored_card = restored_learning
        .get_review_card(&card.id)
        .expect("read restored review card")
        .expect("review card exists after restore");
    assert_eq!(restored_card.entity_id, "release-restore-event");
    assert_eq!(restored_card.repetition_count, 1);
    assert_eq!(restored_card.last_reviewed_at, Some(now));
    let progress = restored_learning
        .get_progress("history:event:release-restore-event")
        .expect("read restored SRS progress")
        .expect("SRS progress exists after restore");
    assert_eq!(progress.review_count, 1);
    assert_eq!(progress.correct_count, 1);
    assert_eq!(progress.next_review_at, Some(outcome.due_at));

    let restored_travel =
        TravelStore::open(restored_dir.join("travel.db.bak")).expect("open restored Travel DB");
    let reopened_plan = restored_travel
        .get_plan(&context)
        .expect("read restored TravelPlan")
        .expect("TravelPlan exists after restore");
    assert_eq!(reopened_plan, saved_plan);
    assert_eq!(
        reopened_plan.custom_places["custom:tea-house"].note,
        "Personal stop note"
    );
}
