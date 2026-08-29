//! Request-family probes — the clone-on-done recurrence engine, its DB guards, and the fence.
//!
//! Runs against a DEDICATED scratch database (export `DATABASE_URL`; there is deliberately no
//! fallback — these probes must never run against a shared DB). The non-owner fence legs connect
//! as the NOBYPASSRLS `sv5_mx_app` role (see `APP_DSN`); everything else runs on the superuser
//! pool, where RLS is bypassed and the trigger/CHECK layers are what these legs exercise.
//!
//! Legs (numbered, one per probe):
//!  1  closing a preventive recurring request spawns exactly one successor — calendar-exact
//!     dates advanced, back at the first stage, nothing scheduled anywhere else (zero crons);
//!  2  the successor self-perpetuates (closing IT spawns the next link — no scheduler involved);
//!  3  closing onto the OTHER done stage (Scrap) spawns too;
//!  4  at-most-once: same-stage re-transition is an `already` no-op, done→done reclassification
//!     spawns nothing, and a close→reopen→close cycle cannot claim a second successor slot;
//!  5  termination: `until` past repeat_until spawns nothing; `until` with no repeat_until is
//!     rejected at BOTH layers (service guard + table CHECK);
//!  6  kanban: a stage move resets kanban to normal unless the caller set it in the same verb;
//!  7  close_date: stamped entering done, cleared leaving; a RAW stage change on a non-recurring
//!     request still lands the reset + stamp (trigger arms work without the managed marker);
//!  8  G-MT5: a RAW close of a preventive recurring request raises
//!     maintenance_recurring_close_requires_service_verb;
//!  9  G-MT2 widened: a raw UPDATE that moves schedule_date past schedule_end raises;
//! 10  G-MT3 both layers: repeat_interval 0 rejected by the service guard and by the CHECK;
//! 11  stage FK RESTRICT on hard delete + maintenance_stage_in_use on soft delete of a stage
//!     still referenced by a live request;
//! 12  fences as the NOBYPASSRLS app role: two companies isolated on requests, shared stages
//!     visible to both, a forged cross-company insert rejected by WITH CHECK;
//! 13  the visit engine still completes + posts beside the request family (coexistence); the
//!     pre-existing golden/GL suites are additionally run unchanged in CI.

mod common;

use chrono::{Duration, NaiveDate, TimeZone, Utc};
use rust_decimal::Decimal;
use sqlx::{Connection, PgConnection, PgPool, Row};
use uuid::Uuid;

use backbone_maintenance::application::service::maintenance_events::MaintenanceEvent;
use backbone_maintenance::application::service::maintenance_request_write_service::{
    MaintenanceRequestUpdate, MaintenanceRequestWriteService, NewMaintenanceRequest,
    RequestWriteError,
};
use backbone_maintenance::application::service::maintenance_write_service::NewVisit;
use backbone_maintenance::domain::entity::{
    MaintenanceType, RequestKanbanState, RequestPriority, RepeatType, RepeatUnit,
};

use common::{CapturingSink, CountingGl, FakeInventory};

// ── wiring ────────────────────────────────────────────────────────────────────

fn db() -> String {
    std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be exported, pointing at the dedicated request-family scratch DB")
}

fn app_dsn() -> String {
    std::env::var("SV5_APP_DSN").unwrap_or_else(|_| {
        "postgres://sv5_mx_app:sv5_mx_app@127.0.0.1:5433/sv5_maintenance_req".into()
    })
}

async fn pool() -> PgPool {
    PgPool::connect(&db()).await.expect("connect scratch DB")
}

fn svc(pool: &PgPool) -> MaintenanceRequestWriteService {
    MaintenanceRequestWriteService::new(pool.clone())
}

// The seeded shared stage set (fixed ids — see migrations/seeds/maintenance_stage_seed.sql).
fn stage_new() -> Uuid {
    Uuid::parse_str("00000000-0000-0000-0000-00000000b001").unwrap()
}
fn stage_progress() -> Uuid {
    Uuid::parse_str("00000000-0000-0000-0000-00000000b002").unwrap()
}
fn stage_repaired() -> Uuid {
    Uuid::parse_str("00000000-0000-0000-0000-00000000b003").unwrap()
}
fn stage_scrap() -> Uuid {
    Uuid::parse_str("00000000-0000-0000-0000-00000000b004").unwrap()
}

/// Re-assert the stage seed (idempotent, fixed ids) so the file also works on a freshly
/// migrated DB without the seed step.
async fn ensure_stages(pool: &PgPool) {
    let seed = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/migrations/seeds/maintenance_stage_seed.sql"
    ));
    sqlx::query(seed).execute(pool).await.expect("stage seed");
}

/// A preventive, recurring request starting at the given occurrence date.
fn preventive(company: Uuid, schedule_date: chrono::DateTime<Utc>) -> NewMaintenanceRequest {
    NewMaintenanceRequest {
        company_id: company,
        name: "pump inspection".into(),
        description: None,
        schedule_date: Some(schedule_date),
        schedule_end: None,
        duration: Decimal::TWO, // hours
        owner_user_id: None,
        user_id: None,
        asset_id: None,
        stage_id: None, // first visible stage
        kanban_state: RequestKanbanState::Normal,
        priority: RequestPriority::Low,
        maintenance_type: MaintenanceType::Preventive,
        recurring: true,
        repeat_interval: 1,
        repeat_unit: RepeatUnit::Month,
        repeat_type: RepeatType::Forever,
        repeat_until: None,
    }
}

async fn field_text(pool: &PgPool, id: Uuid, expr: &str) -> String {
    let q = format!(
        "SELECT COALESCE(({})::text, '<null>') FROM maintenance.maintenance_requests WHERE id = $1",
        expr
    );
    sqlx::query_scalar(&q)
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("field read")
}

async fn field_ts(pool: &PgPool, id: Uuid, expr: &str) -> Option<chrono::DateTime<Utc>> {
    let q = format!(
        "SELECT {} FROM maintenance.maintenance_requests WHERE id = $1",
        expr
    );
    sqlx::query_scalar(&q)
        .bind(id)
        .fetch_optional(pool)
        .await
        .expect("timestamp read")
        .flatten()
}

async fn successor_count(pool: &PgPool, source: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM maintenance.maintenance_requests WHERE successor_of_request_id = $1",
    )
    .bind(source)
    .fetch_one(pool)
    .await
    .expect("successor count")
}

async fn outbox_count(pool: &PgPool, event_type: &str, aggregate_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM maintenance.outbox_events WHERE event_type = $1 AND aggregate_id = $2",
    )
    .bind(event_type)
    .bind(aggregate_id.to_string())
    .fetch_one(pool)
    .await
    .expect("outbox count")
}

fn err_text(e: &sqlx::Error) -> String {
    match e {
        sqlx::Error::Database(d) => d.message().to_string(),
        other => other.to_string(),
    }
}

// ── 1 — the spawn ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn p1_close_spawns_one_successor_with_advanced_dates() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let start = Utc.with_ymd_and_hms(2026, 1, 15, 8, 0, 0).unwrap();
    let id = svc(&pool)
        .create_request(preventive(company, start))
        .await
        .expect("create");

    let sink = CapturingSink::new();
    let out = svc(&pool)
        .transition_request(id, stage_repaired(), None, &sink)
        .await
        .expect("transition");

    let successor = out.spawned_successor_id.expect("one successor spawned");
    assert_eq!(successor_count(&pool, id).await, 1, "exactly one successor");

    // The successor is back at the first stage, kanban normal, dates advanced one calendar month.
    assert_eq!(field_text(&pool, successor, "stage_id").await, stage_new().to_string());
    assert_eq!(field_text(&pool, successor, "kanban_state").await, "normal");
    assert_eq!(
        field_ts(&pool, successor, "schedule_date").await,
        Some(Utc.with_ymd_and_hms(2026, 2, 15, 8, 0, 0).unwrap())
    );
    assert_eq!(
        field_ts(&pool, successor, "schedule_end").await,
        Some(Utc.with_ymd_and_hms(2026, 2, 15, 10, 0, 0).unwrap()),
        "schedule_end = next + duration hours"
    );
    // The chain markers point both ways.
    assert_eq!(field_text(&pool, id, "successor_request_id").await, successor.to_string());
    assert_eq!(field_text(&pool, successor, "successor_of_request_id").await, id.to_string());
    // Recurrence set inherited — the chain can continue.
    assert_eq!(field_text(&pool, successor, "maintenance_type").await, "preventive");
    assert_eq!(field_text(&pool, successor, "recurring").await, "true");

    // Nothing is scheduled anywhere else: no scheduler artifact exists, and the continuation is
    // only the durable outbox pair staged in the same transaction.
    let no_scheduler: Option<String> = sqlx::query_scalar(
        "SELECT to_regclass('maintenance.scheduled_jobs')::text",
    )
    .fetch_one(&pool)
    .await
    .expect("regclass probe");
    assert_eq!(no_scheduler, None, "zero crons: no scheduled-jobs artifact at all");
    assert_eq!(outbox_count(&pool, "MaintenanceRequestStageChanged", id).await, 1);
    assert_eq!(outbox_count(&pool, "SuccessorSpawned", successor).await, 1);

    // Both events also reached the live sink.
    let events = sink.events.lock().unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, MaintenanceEvent::MaintenanceRequestStageChanged(_))));
    assert!(events.iter().any(|e| matches!(e, MaintenanceEvent::SuccessorSpawned(_))));
}

// ── 2 — self-perpetuation ────────────────────────────────────────────────────

#[tokio::test]
async fn p2_successor_self_perpetuates() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let start = Utc.with_ymd_and_hms(2026, 1, 15, 8, 0, 0).unwrap();
    let first = svc(&pool)
        .create_request(preventive(company, start))
        .await
        .expect("create");
    let sink = CapturingSink::new();
    let out = svc(&pool)
        .transition_request(first, stage_repaired(), None, &sink)
        .await
        .expect("close first");
    let second = out.spawned_successor_id.expect("successor");

    // Closing the SUCCESSOR spawns the next link — the chain perpetuates itself with no cron.
    let out2 = svc(&pool)
        .transition_request(second, stage_repaired(), None, &sink)
        .await
        .expect("close successor");
    let third = out2.spawned_successor_id.expect("second successor");

    assert_eq!(successor_count(&pool, second).await, 1);
    assert_eq!(
        field_ts(&pool, third, "schedule_date").await,
        Some(Utc.with_ymd_and_hms(2026, 3, 15, 8, 0, 0).unwrap()),
        "dates advance step by step down the chain"
    );
    assert_eq!(field_text(&pool, third, "successor_of_request_id").await, second.to_string());
    assert_eq!(field_text(&pool, third, "stage_id").await, stage_new().to_string());
}

// ── 3 — the other done stage ─────────────────────────────────────────────────

#[tokio::test]
async fn p3_closing_onto_scrap_spawns_too() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let id = svc(&pool)
        .create_request(preventive(company, Utc.with_ymd_and_hms(2026, 4, 1, 6, 0, 0).unwrap()))
        .await
        .expect("create");

    let out = svc(&pool)
        .transition_request(id, stage_scrap(), None, &CapturingSink::new())
        .await
        .expect("close onto Scrap");

    assert!(out.spawned_successor_id.is_some(), "Scrap is a done stage — it spawns");
    assert_eq!(successor_count(&pool, id).await, 1);
}

// ── 4 — at-most-once ─────────────────────────────────────────────────────────

#[tokio::test]
async fn p4_spawn_is_at_most_once_per_source() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let sink = CapturingSink::new();
    let id = svc(&pool)
        .create_request(preventive(company, Utc.with_ymd_and_hms(2026, 2, 10, 9, 0, 0).unwrap()))
        .await
        .expect("create");

    // (a) same-stage transition: an `already` no-op that changes and spawns nothing.
    let out = svc(&pool)
        .transition_request(id, stage_new(), None, &sink)
        .await
        .expect("same-stage transition");
    assert!(out.already, "already at the target stage");
    assert!(out.spawned_successor_id.is_none());
    assert_eq!(successor_count(&pool, id).await, 0);

    // Close for real — spawns exactly one.
    let out = svc(&pool)
        .transition_request(id, stage_repaired(), None, &sink)
        .await
        .expect("close");
    assert!(out.spawned_successor_id.is_some());
    assert_eq!(successor_count(&pool, id).await, 1);

    // (b) done→done reclassification (Repaired → Scrap) does NOT respawn.
    let out = svc(&pool)
        .transition_request(id, stage_scrap(), None, &sink)
        .await
        .expect("reclassify onto the other done stage");
    assert!(!out.already);
    assert!(out.spawned_successor_id.is_none(), "done→done never spawns");

    // (c) close → reopen → close again: the claim slot is taken, so no second successor.
    svc(&pool)
        .transition_request(id, stage_new(), None, &sink)
        .await
        .expect("reopen");
    let out = svc(&pool)
        .transition_request(id, stage_scrap(), None, &sink)
        .await
        .expect("close again");
    assert!(
        out.spawned_successor_id.is_none(),
        "the write-once successor slot refuses a second spawn"
    );
    assert_eq!(successor_count(&pool, id).await, 1, "still exactly one successor");
}

// ── 5 — termination + the until citation fence ────────────────────────────────

#[tokio::test]
async fn p5_until_termination_and_both_layer_rejection() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let sink = CapturingSink::new();

    // (a) repeat_until before the next occurrence: the close lands, nothing spawns.
    let mut bounded = preventive(company, Utc.with_ymd_and_hms(2026, 1, 15, 8, 0, 0).unwrap());
    bounded.repeat_type = RepeatType::Until;
    bounded.repeat_until = Some(NaiveDate::from_ymd_opt(2026, 1, 31).unwrap()); // next would be Feb 15
    let id = svc(&pool).create_request(bounded).await.expect("create");
    let out = svc(&pool)
        .transition_request(id, stage_repaired(), None, &sink)
        .await
        .expect("terminal close");
    assert!(out.spawned_successor_id.is_none(), "past repeat_until — chain ends");
    assert_eq!(successor_count(&pool, id).await, 0);

    // (b) `until` with no repeat_until is rejected at BOTH layers — never silently non-spawning.
    let mut bad = preventive(company, Utc.with_ymd_and_hms(2026, 1, 15, 8, 0, 0).unwrap());
    bad.repeat_type = RepeatType::Until;
    bad.repeat_until = None;
    let err = svc(&pool).create_request(bad).await.expect_err("service guard rejects");
    assert!(matches!(err, RequestWriteError::RepeatUntilMissing), "got: {err:?}");

    let raw = sqlx::query(
        r#"INSERT INTO maintenance.maintenance_requests
             (id, company_id, name, stage_id, maintenance_type, recurring, repeat_type, repeat_until)
           VALUES ($1, $2, 'raw until probe', $3, 'preventive', true, 'until', NULL)"#,
    )
    .bind(Uuid::new_v4())
    .bind(company)
    .bind(stage_new())
    .execute(&pool)
    .await;
    match raw {
        Err(e) => assert!(
            err_text(&e).contains("repeat_until") || err_text(&e).contains("chk_maintenance_requests_repeat_until_required"),
            "DB CHECK rejects: {}",
            err_text(&e)
        ),
        Ok(_) => panic!("raw INSERT with repeat_type until and NULL repeat_until must fail"),
    }
}

// ── 6 — kanban reset unless caller-set ───────────────────────────────────────

#[tokio::test]
async fn p6_kanban_resets_on_stage_change_unless_set() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let sink = CapturingSink::new();

    let mut blocked = preventive(company, Utc.with_ymd_and_hms(2026, 5, 1, 7, 0, 0).unwrap());
    blocked.recurring = false; // kanban legs don't need recurrence
    let id = svc(&pool).create_request(blocked).await.expect("create");
    // Park the request in a blocked sub-state via a stage move WITH the override…
    svc(&pool)
        .transition_request(id, stage_progress(), Some(RequestKanbanState::Blocked), &sink)
        .await
        .expect("move with kanban override");
    assert_eq!(field_text(&pool, id, "kanban_state").await, "blocked", "caller-set survives");

    // …then move WITHOUT an override: the sub-state resets to normal for the new stage.
    svc(&pool)
        .transition_request(id, stage_new(), None, &sink)
        .await
        .expect("move without override");
    assert_eq!(field_text(&pool, id, "kanban_state").await, "normal", "reset on bare stage change");
}

// ── 7 — close_date sync ──────────────────────────────────────────────────────

#[tokio::test]
async fn p7_close_date_syncs_and_raw_moves_on_non_recurring_land() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let sink = CapturingSink::new();

    let mut corrective = preventive(company, Utc.with_ymd_and_hms(2026, 6, 1, 7, 0, 0).unwrap());
    corrective.maintenance_type = MaintenanceType::Corrective;
    corrective.recurring = false;
    let id = svc(&pool).create_request(corrective).await.expect("create");

    // Entering done stamps close_date; leaving clears it.
    svc(&pool)
        .transition_request(id, stage_repaired(), None, &sink)
        .await
        .expect("close");
    assert_eq!(
        field_text(&pool, id, "close_date").await,
        Utc::now().date_naive().to_string(),
        "entering done stamps today"
    );
    svc(&pool)
        .transition_request(id, stage_progress(), None, &sink)
        .await
        .expect("reopen");
    assert_eq!(field_text(&pool, id, "close_date").await, "<null>", "leaving done clears");

    // RAW stage change on the non-recurring request: the trigger still lands reset + stamp
    // (no managed marker needed — the raise arm is only for preventive recurring closes).
    sqlx::query("UPDATE maintenance.maintenance_requests SET stage_id = $2 WHERE id = $1")
        .bind(id)
        .bind(stage_repaired())
        .execute(&pool)
        .await
        .expect("raw close of a non-recurring request is allowed");
    assert_eq!(field_text(&pool, id, "close_date").await, Utc::now().date_naive().to_string());
    assert_eq!(field_text(&pool, id, "kanban_state").await, "normal", "raw move resets kanban");
}

// ── 8 — G-MT5 raw-close refusal ──────────────────────────────────────────────

#[tokio::test]
async fn p8_raw_close_of_recurring_preventive_raises() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let id = svc(&pool)
        .create_request(preventive(company, Utc.with_ymd_and_hms(2026, 7, 1, 7, 0, 0).unwrap()))
        .await
        .expect("create");

    let raw = sqlx::query("UPDATE maintenance.maintenance_requests SET stage_id = $2 WHERE id = $1")
        .bind(id)
        .bind(stage_repaired())
        .execute(&pool)
        .await;
    match raw {
        Err(e) => assert!(
            err_text(&e).contains("maintenance_recurring_close_requires_service_verb"),
            "wrong error: {}",
            err_text(&e)
        ),
        Ok(_) => panic!("raw close of a preventive recurring request must raise (it would skip the spawn)"),
    }
    // The refused row is untouched.
    assert_eq!(field_text(&pool, id, "stage_id").await, stage_new().to_string());
}

// ── 9 — G-MT2 widened fire-rule ───────────────────────────────────────────────

#[tokio::test]
async fn p9_widened_schedule_order_check_fires_on_date_move() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let start = Utc.with_ymd_and_hms(2026, 8, 1, 6, 0, 0).unwrap();
    let mut windowed = preventive(company, start);
    windowed.recurring = false;
    windowed.schedule_end = Some(start + Duration::hours(3));
    let id = svc(&pool).create_request(windowed).await.expect("create");

    // Moving ONLY schedule_date past the existing schedule_end must raise (widened fire-rule).
    let raw = sqlx::query("UPDATE maintenance.maintenance_requests SET schedule_date = $2 WHERE id = $1")
        .bind(id)
        .bind(start + Duration::days(1))
        .execute(&pool)
        .await;
    match raw {
        Err(e) => assert!(
            err_text(&e).contains("chk_maintenance_requests_schedule_order")
                || err_text(&e).contains("schedule_end"),
            "wrong error: {}",
            err_text(&e)
        ),
        Ok(_) => panic!("moving schedule_date past schedule_end must raise"),
    }
}

// ── 10 — G-MT3 both layers ───────────────────────────────────────────────────

#[tokio::test]
async fn p10_repeat_interval_floor_both_layers() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let id = svc(&pool)
        .create_request(preventive(company, Utc.with_ymd_and_hms(2026, 8, 2, 6, 0, 0).unwrap()))
        .await
        .expect("create");

    // Service layer: typed rejection.
    let err = svc(&pool)
        .update_request(
            id,
            MaintenanceRequestUpdate { repeat_interval: Some(0), ..Default::default() },
        )
        .await
        .expect_err("service guard rejects");
    assert!(matches!(err, RequestWriteError::RepeatIntervalBelowOne), "got: {err:?}");

    // DB layer: the CHECK refuses a raw writer.
    let raw = sqlx::query("UPDATE maintenance.maintenance_requests SET repeat_interval = 0 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await;
    match raw {
        Err(e) => assert!(
            err_text(&e).contains("chk_maintenance_requests_repeat_interval_positive")
                || err_text(&e).contains("repeat_interval"),
            "wrong error: {}",
            err_text(&e)
        ),
        Ok(_) => panic!("repeat_interval 0 must raise at the DB"),
    }
}

// ── 11 — stage referential safety ─────────────────────────────────────────────

#[tokio::test]
async fn p11_stage_delete_protections() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();

    // A company-private stage holding one live request.
    let used = Uuid::new_v4();
    let unused = Uuid::new_v4();
    for (id, name) in [(used, "probe used stage"), (unused, "probe unused stage")] {
        sqlx::query(
            r#"INSERT INTO maintenance.maintenance_stages (id, company_id, name, sequence, done)
               VALUES ($1, $2, $3, 90, FALSE)"#,
        )
        .bind(id)
        .bind(company)
        .bind(name)
        .execute(&pool)
        .await
        .expect("stage insert");
    }
    let mut r = preventive(company, Utc.with_ymd_and_hms(2026, 8, 3, 6, 0, 0).unwrap());
    r.stage_id = Some(used);
    r.recurring = false;
    let req = svc(&pool).create_request(r).await.expect("create on the stage");

    // Hard delete: RESTRICT.
    let hard = sqlx::query("DELETE FROM maintenance.maintenance_stages WHERE id = $1")
        .bind(used)
        .execute(&pool)
        .await;
    match hard {
        Err(e) => assert!(
            err_text(&e).contains("fk_maintenance_requests_stage_id")
                || err_text(&e).contains("restrict"),
            "wrong error: {}",
            err_text(&e)
        ),
        Ok(_) => panic!("hard delete of a referenced stage must be restricted"),
    }

    // Soft delete: refused while a live request sits on the stage.
    let soft = sqlx::query(
        r#"UPDATE maintenance.maintenance_stages
           SET metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(NOW())) WHERE id = $1"#,
    )
    .bind(used)
    .execute(&pool)
    .await;
    match soft {
        Err(e) => assert!(
            err_text(&e).contains("maintenance_stage_in_use"),
            "wrong error: {}",
            err_text(&e)
        ),
        Ok(_) => panic!("soft delete of an in-use stage must raise"),
    }

    // Soft delete of an UNREFERENCED stage still works (the guard is about liveness, not stages).
    sqlx::query(
        r#"UPDATE maintenance.maintenance_stages
           SET metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(NOW())) WHERE id = $1"#,
    )
    .bind(unused)
    .execute(&pool)
    .await
    .expect("unused stage soft-deletes");

    // And the live request is still exactly where it was.
    assert_eq!(field_text(&pool, req, "stage_id").await, used.to_string());
}

// ── 12 — the fence, as a non-owner NOBYPASSRLS role ──────────────────────────

#[tokio::test]
async fn p12_company_fence_and_shared_stages() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let co_a = Uuid::new_v4();
    let co_b = Uuid::new_v4();

    // Sanity: both fences are ENABLEd and FORCEd (the owner is fenced too).
    let row = sqlx::query(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE oid = 'maintenance.maintenance_requests'::regclass",
    )
    .fetch_one(&pool)
    .await
    .expect("pg_class");
    assert_eq!((row.get::<bool, _>(0), row.get::<bool, _>(1)), (true, true), "RLS enabled + forced on requests");
    let row = sqlx::query(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class WHERE oid = 'maintenance.maintenance_stages'::regclass",
    )
    .fetch_one(&pool)
    .await
    .expect("pg_class");
    assert_eq!((row.get::<bool, _>(0), row.get::<bool, _>(1)), (true, true), "RLS enabled + forced on stages");

    // A non-owner session: the role cannot bypass RLS, so the policies are the only view.
    let mut app = PgConnection::connect(&app_dsn()).await.expect("app-role connect");
    async fn set_scope(conn: &mut PgConnection, company: Uuid) {
        sqlx::query("SELECT set_config('app.company_id', $1, false)")
            .bind(company.to_string())
            .execute(conn)
            .await
            .expect("set scope");
    }

    // Company A files a request through the fence.
    set_scope(&mut app, co_a).await;
    let req_a = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO maintenance.maintenance_requests
             (id, company_id, name, stage_id, maintenance_type, recurring)
           VALUES ($1, $2, 'company A request', $3, 'corrective', false)"#,
    )
    .bind(req_a)
    .bind(co_a)
    .bind(stage_new())
    .execute(&mut app)
    .await
    .expect("scoped insert passes WITH CHECK");

    // A sees its own request AND the shared stage set.
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM maintenance.maintenance_requests WHERE id = $1")
        .bind(req_a)
        .fetch_one(&mut app)
        .await
        .expect("count as A");
    assert_eq!(seen, 1, "own rows visible");
    let stages: i64 = sqlx::query_scalar("SELECT count(*) FROM maintenance.maintenance_stages")
        .fetch_one(&mut app)
        .await
        .expect("stage count as A");
    assert!(stages >= 4, "the shared NULL-company stages are visible (got {stages})");

    // Company B sees none of A's requests, but the same shared stages.
    set_scope(&mut app, co_b).await;
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM maintenance.maintenance_requests WHERE id = $1")
        .bind(req_a)
        .fetch_one(&mut app)
        .await
        .expect("count as B");
    assert_eq!(seen, 0, "cross-company request invisible");
    let stages_b: i64 = sqlx::query_scalar("SELECT count(*) FROM maintenance.maintenance_stages")
        .fetch_one(&mut app)
        .await
        .expect("stage count as B");
    assert_eq!(stages_b, stages, "both companies see the same shared stages");

    // A forged cross-company write is refused by WITH CHECK.
    let forged = sqlx::query(
        r#"INSERT INTO maintenance.maintenance_requests
             (id, company_id, name, stage_id, maintenance_type, recurring)
           VALUES ($1, $2, 'forged row', $3, 'corrective', false)"#,
    )
    .bind(Uuid::new_v4())
    .bind(co_a) // row claims A…
    .bind(stage_new())
    .execute(&mut app) // …while scoped as B
    .await;
    match forged {
        Err(e) => assert!(
            err_text(&e).contains("row-level security") || err_text(&e).contains("policy"),
            "wrong error: {}",
            err_text(&e)
        ),
        Ok(_) => panic!("a B-scoped session must not insert an A-owned row"),
    }
}

// ── 13 — coexistence with the visit engine ────────────────────────────────────

#[tokio::test]
async fn p13_visit_engine_still_completes_beside_the_request_family() {
    let pool = pool().await;
    ensure_stages(&pool).await;
    let company = Uuid::new_v4();
    let accounts = common::mx_accounts(&pool, company).await;

    // One request-family row in place while the visit engine runs — both engines, one DB.
    let req = svc(&pool)
        .create_request(preventive(company, Utc.with_ymd_and_hms(2026, 9, 1, 6, 0, 0).unwrap()))
        .await
        .expect("create");

    let write = backbone_maintenance::application::service::MaintenanceWriteService::new(pool.clone());
    let inventory = FakeInventory::new("2500");
    let gl = CountingGl::new();
    let visit = write
        .plan_visit(NewVisit {
            company_id: company,
            asset_id: Uuid::new_v4(),
            schedule_id: None,
            maintenance_type: "corrective".into(),
            scheduled_date: common::today(),
            warehouse_id: Some(Uuid::new_v4()),
            warranty_claim_id: None,
            labor_cost: common::dec("500"),
            maintenance_expense_account_id: accounts.expense,
            parts_inventory_account_id: accounts.parts_inventory,
            labor_payable_account_id: accounts.labor_payable,
        })
        .await
        .expect("plan visit");
    write
        .add_part(visit, Uuid::new_v4(), common::dec("4"))
        .await
        .expect("add part");
    write
        .complete_visit(visit, common::today(), &inventory, &gl, &CapturingSink::new())
        .await
        .expect("complete visit");

    assert_eq!(gl.count(), 1, "the cost journal still posts");
    // And the request family row is untouched by the visit engine.
    assert_eq!(field_text(&pool, req, "stage_id").await, stage_new().to_string());
    assert_eq!(field_text(&pool, req, "successor_request_id").await, "<null>");
}
