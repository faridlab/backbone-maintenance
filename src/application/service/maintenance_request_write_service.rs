//! The hand-authored maintenance request write path (user-owned; survives regen).
//!
//! Three verbs own every write a request's lifecycle allows:
//!   - `create_request`  — file a request (stage defaults to the first visible stage);
//!   - `update_request`  — change descriptive/recurrence fields (schedule, duration, repeat set);
//!   - `transition_request` — move a request to another stage. This is the ONE door for stage
//!     changes: it sets the transaction-local `app.maintenance_managed_transition` marker, so the
//!     G-MT5 trigger lets a preventive recurring request close — and closing is when the
//!     clone-on-done engine fires: the successor is cloned INSIDE the same transaction, at most
//!     once (claim-marker CAS, mirroring the visit engine's completion CAS). ZERO schedulers —
//!     the recurrence is the transition itself; the successor inherits preventive+recurring+the
//!     repeat set, so it self-perpetuates by construction.
//!
//! The DB backstops raw writers: a raw close of a preventive recurring request RAISES
//! `maintenance_recurring_close_requires_service_verb` (it would skip the spawn), while a raw stage
//! change of any non-recurring request still lands the correct kanban reset + close_date via the
//! trigger. Outbox events (`MaintenanceRequestStageChanged`, `SuccessorSpawned`) are the named seam
//! for the deferred activity-family feedbacks, consumed through the host relay.

use backbone_orm::company_scope;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entity::{MaintenanceType, RequestKanbanState, RequestPriority, RepeatType, RepeatUnit};
use crate::infrastructure::persistence::{
    MaintenanceRequestRepository, NewRequestRow, RequestFieldUpdates, RequestTransitionRow,
};

use super::maintenance_events::*;

#[derive(Debug, thiserror::Error)]
pub enum RequestWriteError {
    #[error("db: {0}")]
    Db(#[from] sqlx::Error),
    #[error("not found: {0}")]
    NotFound(&'static str),
    /// G-MT2 — schedule_end would precede schedule_date (checked on the merged create/update view).
    #[error("schedule_end must not precede schedule_date")]
    ScheduleEndBeforeStart,
    /// G-MT3 — a recurrence step below one unit.
    #[error("repeat_interval must be at least 1")]
    RepeatIntervalBelowOne,
    /// G-MT7 — a recurrence bounded 'until' with no repeat_until date.
    #[error("repeat_type 'until' requires a repeat_until date")]
    RepeatUntilMissing,
    /// G-MT8 — only preventive maintenance may recur.
    #[error("only preventive maintenance may recur")]
    CorrectiveCannotRecur,
    #[error("invalid state: {0}")]
    InvalidState(&'static str),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// A request as filed. The stage defaults to the first visible stage when `stage_id` is `None`.
#[derive(Debug, Clone)]
pub struct NewMaintenanceRequest {
    pub company_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub schedule_date: Option<DateTime<Utc>>,
    pub schedule_end: Option<DateTime<Utc>>,
    pub duration: Decimal,
    pub owner_user_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub asset_id: Option<Uuid>,
    pub stage_id: Option<Uuid>,
    pub kanban_state: RequestKanbanState,
    pub priority: RequestPriority,
    pub maintenance_type: MaintenanceType,
    pub recurring: bool,
    pub repeat_interval: i32,
    pub repeat_unit: RepeatUnit,
    pub repeat_type: RepeatType,
    pub repeat_until: Option<NaiveDate>,
}

impl Default for NewMaintenanceRequest {
    fn default() -> Self {
        Self {
            company_id: Uuid::nil(),
            name: String::new(),
            description: None,
            schedule_date: None,
            schedule_end: None,
            duration: Decimal::ZERO,
            owner_user_id: None,
            user_id: None,
            asset_id: None,
            stage_id: None,
            kanban_state: RequestKanbanState::default(),
            priority: RequestPriority::default(),
            maintenance_type: MaintenanceType::Corrective,
            recurring: false,
            repeat_interval: 1,
            repeat_unit: RepeatUnit::default(),
            repeat_type: RepeatType::default(),
            repeat_until: None,
        }
    }
}

/// A partial request update. `None` leaves the field unchanged. The lifecycle columns (stage,
/// kanban_state, close_date, the successor markers) are not reachable here — they belong to
/// [`MaintenanceRequestWriteService::transition_request`] and the spawn engine.
#[derive(Debug, Clone, Default)]
pub struct MaintenanceRequestUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub schedule_date: Option<DateTime<Utc>>,
    pub schedule_end: Option<DateTime<Utc>>,
    pub duration: Option<Decimal>,
    pub owner_user_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub asset_id: Option<Uuid>,
    pub priority: Option<RequestPriority>,
    pub maintenance_type: Option<MaintenanceType>,
    pub recurring: Option<bool>,
    pub repeat_interval: Option<i32>,
    pub repeat_unit: Option<RepeatUnit>,
    pub repeat_type: Option<RepeatType>,
    pub repeat_until: Option<NaiveDate>,
}

/// The outcome of a stage transition. `already: true` means the request was at the target stage (or a
/// concurrent transition won) — nothing changed and nothing spawned.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionOutcome {
    pub request_id: Uuid,
    pub from_stage_id: Option<Uuid>,
    pub to_stage_id: Uuid,
    /// The request's close_date after the move (`Some(today)` iff the target stage is done).
    pub close_date: Option<NaiveDate>,
    pub spawned_successor_id: Option<Uuid>,
    pub already: bool,
}

pub struct MaintenanceRequestWriteService {
    pool: PgPool,
    requests: MaintenanceRequestRepository,
}

impl MaintenanceRequestWriteService {
    pub fn new(pool: PgPool) -> Self {
        let requests = MaintenanceRequestRepository::new(pool.clone());
        Self { pool, requests }
    }

    /// File a request. The stage defaults to the first visible stage (the shared seed's
    /// "New Request"); entering a done stage stamps `close_date` via the trigger.
    pub async fn create_request(&self, r: NewMaintenanceRequest) -> Result<Uuid, RequestWriteError> {
        Self::guard(
            r.schedule_date,
            r.schedule_end,
            r.repeat_interval,
            r.maintenance_type,
            r.recurring,
            r.repeat_type,
            r.repeat_until,
        )?;
        let stage_id = match r.stage_id {
            Some(s) => {
                let stage = company_scope::with_company_scope(
                    Some(r.company_id),
                    self.requests.fetch_stage(&self.pool, s),
                )
                .await?;
                stage.ok_or(RequestWriteError::NotFound("stage"))?.id
            }
            None => company_scope::with_company_scope(
                Some(r.company_id),
                self.requests.fetch_first_stage_id(&self.pool),
            )
            .await?
            .ok_or(RequestWriteError::NotFound("stage"))?,
        };
        let id = Uuid::new_v4();
        company_scope::with_company_scope(
            Some(r.company_id),
            self.requests.insert_request(&self.pool, &NewRequestRow {
                id,
                company_id: r.company_id,
                name: &r.name,
                description: r.description.as_deref(),
                schedule_date: r.schedule_date,
                schedule_end: r.schedule_end,
                duration: r.duration,
                owner_user_id: r.owner_user_id,
                user_id: r.user_id,
                asset_id: r.asset_id,
                stage_id,
                kanban_state: &r.kanban_state.to_string(),
                priority: &r.priority.to_string(),
                maintenance_type: &r.maintenance_type.to_string(),
                recurring: r.recurring,
                repeat_interval: r.repeat_interval,
                repeat_unit: &r.repeat_unit.to_string(),
                repeat_type: &r.repeat_type.to_string(),
                repeat_until: r.repeat_until,
                successor_of_request_id: None,
            }),
        )
        .await?;
        Ok(id)
    }

    /// Change a request's descriptive/recurrence fields. The guards run on the MERGED view (old row
    /// overlaid with the update), so — the widened G-MT2 fire-rule — moving `schedule_date` past an
    /// existing `schedule_end` is caught here and by the table CHECK.
    pub async fn update_request(
        &self,
        request_id: Uuid,
        u: MaintenanceRequestUpdate,
    ) -> Result<(), RequestWriteError> {
        let old = self
            .fetch(&self.pool, request_id)
            .await?
            .ok_or(RequestWriteError::NotFound("request"))?;

        let schedule_date = u.schedule_date.or(old.schedule_date);
        let schedule_end = u.schedule_end.or(old.schedule_end);
        let repeat_interval = u.repeat_interval.unwrap_or(old.repeat_interval);
        let maintenance_type = u
            .maintenance_type
            .map(|t| t.to_string())
            .unwrap_or_else(|| old.maintenance_type.clone());
        let recurring = u.recurring.unwrap_or(old.recurring);
        let repeat_type = u
            .repeat_type
            .map(|t| t.to_string())
            .unwrap_or_else(|| old.repeat_type.clone());
        let repeat_until = u.repeat_until.or(old.repeat_until);

        Self::guard_schedule(schedule_date, schedule_end)?;
        Self::guard_repeat_interval(repeat_interval)?;
        Self::guard_repeat_until(&repeat_type, repeat_until)?;
        Self::guard_recurring(&maintenance_type, recurring)?;

        if u.name.is_none()
            && u.description.is_none()
            && u.schedule_date.is_none()
            && u.schedule_end.is_none()
            && u.duration.is_none()
            && u.owner_user_id.is_none()
            && u.user_id.is_none()
            && u.asset_id.is_none()
            && u.priority.is_none()
            && u.maintenance_type.is_none()
            && u.recurring.is_none()
            && u.repeat_interval.is_none()
            && u.repeat_unit.is_none()
            && u.repeat_type.is_none()
            && u.repeat_until.is_none()
        {
            return Ok(()); // nothing requested — not an error
        }

        let priority = u.priority.as_ref().map(|p| p.to_string());
        let maintenance_type_str = u.maintenance_type.as_ref().map(|t| t.to_string());
        let repeat_unit_str = u.repeat_unit.as_ref().map(|x| x.to_string());
        let repeat_type_str = u.repeat_type.as_ref().map(|x| x.to_string());

        let moved = company_scope::with_company_scope(
            Some(old.company_id),
            self.requests.update_request_fields(&self.pool, request_id, &RequestFieldUpdates {
                name: u.name.as_deref(),
                description: u.description.as_deref(),
                schedule_date: u.schedule_date,
                schedule_end: u.schedule_end,
                duration: u.duration,
                owner_user_id: u.owner_user_id,
                user_id: u.user_id,
                asset_id: u.asset_id,
                priority: priority.as_deref(),
                maintenance_type: maintenance_type_str.as_deref(),
                recurring: u.recurring,
                repeat_interval: u.repeat_interval,
                repeat_unit: repeat_unit_str.as_deref(),
                repeat_type: repeat_type_str.as_deref(),
                repeat_until: u.repeat_until,
            }),
        )
        .await?;
        if moved != 1 {
            return Err(RequestWriteError::NotFound("request"));
        }
        Ok(())
    }

    /// Move a request to another stage — the only sanctioned way a request's stage changes.
    ///
    /// Inside one transaction: set the managed-transition marker, CAS the stage move (the G-MT5
    /// trigger applies the kanban reset + close_date on the same statement), and — when a
    /// non-done request CLOSES onto a done stage and is preventive+recurring — spawn the successor
    /// (claim the slot first; at most one successor per source). Stages the outbox events and
    /// commits; the successor exists iff the close committed.
    pub async fn transition_request(
        &self,
        request_id: Uuid,
        target_stage_id: Uuid,
        kanban_state_override: Option<RequestKanbanState>,
        events: &dyn MaintenanceEventSink,
    ) -> Result<TransitionOutcome, RequestWriteError> {
        let old = self
            .fetch(&self.pool, request_id)
            .await?
            .ok_or(RequestWriteError::NotFound("request"))?;
        let company_id = old.company_id;

        let target = company_scope::with_company_scope(
            Some(company_id),
            self.requests.fetch_stage(&self.pool, target_stage_id),
        )
        .await?
        .ok_or(RequestWriteError::NotFound("stage"))?;

        // Already there — idempotent no-op (a repeat or concurrent call changed nothing).
        if old.stage_id == target.id {
            return Ok(TransitionOutcome {
                request_id,
                from_stage_id: Some(old.stage_id),
                to_stage_id: target.id,
                close_date: old.close_date,
                spawned_successor_id: None,
                already: true,
            });
        }

        let mut tx = self.pool.begin().await?;
        // The request's own company — the transition tx writes requests/stages/outbox behind the fence.
        company_scope::bind_company_on(&mut tx, company_id).await?;
        // The managed-transition marker: transaction-local (set_config ..., true), so it cannot leak
        // across pool reuse — the same mechanism as app.company_id. This is what authorizes the G-MT5
        // recurrence arm to let a preventive recurring request close (the spawn follows below).
        sqlx::query("SELECT set_config('app.maintenance_managed_transition', '1', true)")
            .execute(&mut *tx)
            .await?;

        let from_stage_id = old.stage_id;
        let moved = self
            .requests
            .transition_cas(
                &mut tx,
                request_id,
                target.id,
                kanban_state_override.as_ref().map(|k| k.to_string()).as_deref(),
            )
            .await?;
        if moved != 1 {
            tx.rollback().await?;
            // Raced (or a concurrent call landed the same move first) — re-read for the outcome.
            let winner = self.fetch(&self.pool, request_id).await?.ok_or(RequestWriteError::NotFound("request"))?;
            return Ok(TransitionOutcome {
                request_id,
                from_stage_id: Some(from_stage_id),
                to_stage_id: winner.stage_id,
                close_date: winner.close_date,
                spawned_successor_id: winner.successor_request_id,
                already: true,
            });
        }

        // ── Clone-on-done: only the CLOSE transition spawns (old stage not done, target done).
        // A done -> done reclassification (e.g. Repaired -> Scrap) does NOT respawn: Odoo's literal
        // fire-rule is any done target, but at-most-once-per-close is the deliberate narrowing this
        // engine commits to — the claim marker makes a second spawn impossible anyway.
        let mut spawned_successor_id: Option<Uuid> = None;
        let mut spawn_window: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
        if !old.stage_done && target.done && old.maintenance_type == "preventive" && old.recurring {
            let base = old.schedule_date.unwrap_or_else(Utc::now);
            let next = advance(base, old.repeat_interval, &old.repeat_unit)
                .ok_or_else(|| RequestWriteError::Invalid("recurrence step overflows the calendar".into()))?;
            // Termination: forever, or the next date still within repeat_until (inclusive).
            let continues = old.repeat_type != "until"
                || old.repeat_until.map(|until| next.date_naive() <= until).unwrap_or(false);
            if continues {
                let hours = if old.duration.is_zero() { Decimal::ONE } else { old.duration };
                let minutes = (hours * Decimal::from(60))
                    .to_i64()
                    .ok_or_else(|| RequestWriteError::Invalid("duration out of range".into()))?;
                let next_end = next + Duration::minutes(minutes);

                let successor_id = Uuid::new_v4();
                // Claim the slot FIRST: 0 rows means a spawn already happened (repeat/race) — skip.
                // Deterministic ordering: claim precedes the INSERT, both precede the commit, all
                // inside the same tx as the stage CAS — the successor exists iff the close committed.
                let claimed = self
                    .requests
                    .claim_successor_slot(&mut tx, request_id, successor_id)
                    .await?;
                if claimed == 1 {
                    let first_stage = self
                        .requests
                        .fetch_first_stage_id_on(&mut tx)
                        .await?
                        .ok_or(RequestWriteError::Invalid("no visible stage to spawn the successor into".into()))?;
                    self.requests
                        .insert_request_on(&mut tx, &NewRequestRow {
                            id: successor_id,
                            company_id,
                            name: &old.name,
                            description: old.description.as_deref(),
                            schedule_date: Some(next),
                            schedule_end: Some(next_end),
                            duration: old.duration,
                            owner_user_id: old.owner_user_id,
                            user_id: old.user_id,
                            asset_id: old.asset_id,
                            stage_id: first_stage,
                            kanban_state: "normal",
                            priority: &old.priority,
                            maintenance_type: &old.maintenance_type,
                            recurring: old.recurring,
                            repeat_interval: old.repeat_interval,
                            repeat_unit: &old.repeat_unit,
                            repeat_type: &old.repeat_type,
                            repeat_until: old.repeat_until,
                            successor_of_request_id: Some(request_id),
                        })
                        .await?;
                    spawned_successor_id = Some(successor_id);
                    spawn_window = Some((next, next_end));
                }
            }
        }

        // ── Outbox: the stage change (and the spawn) land with the same commit.
        let close_date = if target.done { Some(Utc::now().date_naive()) } else { None };
        let stage_event = MaintenanceEvent::MaintenanceRequestStageChanged(MaintenanceRequestStageChanged {
            request_id,
            company_id,
            from_stage_id: Some(from_stage_id),
            to_stage_id: target.id,
            close_date,
            spawned_successor_id,
        });
        let stage_record = backbone_outbox::OutboxRecord::new(
            "MaintenanceRequestStageChanged", "MaintenanceRequest", request_id.to_string(), company_id,
            serde_json::to_value(&stage_event).map_err(|e| RequestWriteError::Invalid(e.to_string()))?,
            Utc::now(),
        );
        backbone_outbox::outbox::stage(&mut *tx, "maintenance", &stage_record)
            .await
            .map_err(|e| RequestWriteError::Invalid(format!("outbox stage: {e}")))?;

        let mut spawn_event_value = None;
        if let Some(successor_id) = spawned_successor_id {
            let (next, next_end) = spawn_window.expect("spawn window set alongside the successor id");
            let spawn_event = MaintenanceEvent::SuccessorSpawned(SuccessorSpawned {
                source_request_id: request_id,
                successor_request_id: successor_id,
                company_id,
                next_schedule_date: Some(next),
                next_schedule_end: Some(next_end),
            });
            let spawn_record = backbone_outbox::OutboxRecord::new(
                "SuccessorSpawned", "MaintenanceRequest", successor_id.to_string(), company_id,
                serde_json::to_value(&spawn_event).map_err(|e| RequestWriteError::Invalid(e.to_string()))?,
                Utc::now(),
            );
            backbone_outbox::outbox::stage(&mut *tx, "maintenance", &spawn_record)
                .await
                .map_err(|e| RequestWriteError::Invalid(format!("outbox stage: {e}")))?;
            spawn_event_value = Some(spawn_event);
        }

        tx.commit().await?;
        events.publish(&stage_event);
        if let Some(ev) = spawn_event_value {
            events.publish(&ev);
        }

        Ok(TransitionOutcome {
            request_id,
            from_stage_id: Some(from_stage_id),
            to_stage_id: target.id,
            close_date,
            spawned_successor_id,
            already: false,
        })
    }

    /// Fetch a request's transition view. Caller-scoped (see the repository).
    async fn fetch(
        &self,
        pool: &PgPool,
        request_id: Uuid,
    ) -> Result<Option<RequestTransitionRow>, RequestWriteError> {
        Ok(self.requests.fetch_for_transition(pool, request_id).await?)
    }

    fn guard_schedule(
        schedule_date: Option<DateTime<Utc>>,
        schedule_end: Option<DateTime<Utc>>,
    ) -> Result<(), RequestWriteError> {
        if let (Some(start), Some(end)) = (schedule_date, schedule_end) {
            if end < start {
                return Err(RequestWriteError::ScheduleEndBeforeStart);
            }
        }
        Ok(())
    }

    fn guard_repeat_interval(repeat_interval: i32) -> Result<(), RequestWriteError> {
        if repeat_interval < 1 {
            return Err(RequestWriteError::RepeatIntervalBelowOne);
        }
        Ok(())
    }

    fn guard_repeat_until(repeat_type: &str, repeat_until: Option<NaiveDate>) -> Result<(), RequestWriteError> {
        if repeat_type == "until" && repeat_until.is_none() {
            return Err(RequestWriteError::RepeatUntilMissing);
        }
        Ok(())
    }

    fn guard_recurring(maintenance_type: &str, recurring: bool) -> Result<(), RequestWriteError> {
        if maintenance_type == "corrective" && recurring {
            return Err(RequestWriteError::CorrectiveCannotRecur);
        }
        Ok(())
    }

    fn guard(
        schedule_date: Option<DateTime<Utc>>,
        schedule_end: Option<DateTime<Utc>>,
        repeat_interval: i32,
        maintenance_type: MaintenanceType,
        recurring: bool,
        repeat_type: RepeatType,
        repeat_until: Option<NaiveDate>,
    ) -> Result<(), RequestWriteError> {
        Self::guard_schedule(schedule_date, schedule_end)?;
        Self::guard_repeat_interval(repeat_interval)?;
        Self::guard_repeat_until(&repeat_type.to_string(), repeat_until)?;
        Self::guard_recurring(&maintenance_type.to_string(), recurring)?;
        Ok(())
    }
}

/// Advance `base` by one repeat step (interval x unit). Months and years are calendar-exact
/// (chrono), matching the recurrence intent rather than a fixed 30-day month.
fn advance(base: DateTime<Utc>, interval: i32, unit: &str) -> Option<DateTime<Utc>> {
    match unit {
        "day" => base.checked_add_signed(Duration::days(interval as i64)),
        "week" => base.checked_add_signed(Duration::weeks(interval as i64)),
        "month" => base
            .checked_add_months(chrono::Months::new(u32::try_from(interval).ok()?)),
        // A year stepped as twelve calendar-exact months (chrono has no Years step).
        "year" => u32::try_from(interval)
            .ok()
            .and_then(|i| i.checked_mul(12))
            .and_then(|m| base.checked_add_months(chrono::Months::new(m))),
        _ => None,
    }
}
