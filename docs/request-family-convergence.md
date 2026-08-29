# Maintenance request family — Odoo convergence notes

Scope of this note: the `maintenance.request` / `maintenance.stage` convergence pass that introduced
the request family to this module (schema models, guards, migration + shared stage seed, the
clone-on-done recurrence engine, and its validated write surface). It records the fence posture map
and the deliberate port decisions, so the next reader can tell divergence from accident.

## Company fence posture map

`schema/models/index.model.yaml` declares this module `company_fence: shared_blank`. That single
switch describes `maintenance_stages` only; every register-bearing table in the module is and stays
**strict** at the row level. The realized posture per table:

| Table | Posture | Policy shape |
|---|---|---|
| `maintenance_stages` | **shared_blank** (genuinely) | nullable `company_id`; NULL rows = the one shared stage set; policy is `company_id = current OR company_id IS NULL` |
| `maintenance_requests` | strict-realized | `company_id NOT NULL`; policy is exact equality |
| `maintenance_schedules` | strict-realized | unchanged from before this pass |
| `maintenance_visits` | strict-realized | unchanged |
| `maintenance_visit_parts` | strict-realized | unchanged |

Why stages are shared: stages are company-agnostic master data in the source model (they carry no
company column there at all), and the seeded set (New Request / In Progress / Repaired / Scrap) is
meant to be visible to every tenant so a request can always resolve "first stage". A per-company
stage override remains possible — insert a stage with a company_id — but none is seeded, and the
module's own write paths only ever resolve stages through the fence, so a tenant-private stage is
invisible to other tenants by construction.

Disclosure (verified by the pass council): the shared NULL-company stage rows are writable by any
tenant session at the database layer. The stages policy's NULL arm admits UPDATE from every
company (master-data parity with the source's global stages); only the in-use soft-delete guard
and the RESTRICT hard-delete guard are blocked cross-fence. The composition layer's module-write
gate is the only write guard for shared stage rows - the RLS policy is a visibility fence, not a
write guard, and must not be described as one.

## Port decisions (kept vs dropped)

Kept, per the register: the stage entity with `sequence`/`fold`/`done` (`done` is THE canonical
closed flag — `close_date` on a request derives from it, nothing reads `fold` for logic); the
request with kanban sub-state, priority, and the full recurrence set
(`recurring`, `repeat_interval`, `repeat_unit`, `repeat_type`, `repeat_until`); the successor
markers (`successor_request_id` on the source, `successor_of_request_id` on the clone).

Deliberately NOT ported:

- **archive** on requests and stages — this module's soft-delete convention (`metadata.deleted_at`)
  replaces it; a hard DELETE of a referenced stage is refused by the FK, and a soft delete is
  refused by the `maintenance_stage_in_use` guard while any live request sits on the stage.
- **equipment / category / team** on the request — the asset link is the module's existing
  `asset_id` (backbone-asset FK vocabulary); maintenance teams are out of scope for this family.
- **instruction fields** (`maintenance_team_id`, request instructions) — activity-family follow-up.
- **MTBF / MTTR computes** — dashboard analytics, not register state; deferred with the activity
  feedback seam (below).

Known label-only divergence: `priority` ports the source labels (`0..3`) as
`very_low / low / normal / high` rather than numeric variants — display ordering only, no logic
keys off it. Default `low` matches the source default.

## The recurrence engine (zero schedulers)

Recurrence is clone-on-done and lives entirely inside `transition_request`: when a preventive,
recurring request moves onto a `done` stage, the same transaction computes the next occurrence
(calendar-exact step by `repeat_interval x repeat_unit`, termination at `repeat_until` inclusive),
claims the source's `successor_request_id` slot (write-once CAS; the partial UNIQUE index is the
backstop), and inserts the successor back at the first stage. The successor inherits
preventive+recurring+the repeat set, so the chain self-perpetuates with no scheduler — **zero crons
is a hard invariant of this family**.

Two transaction-local guards make that safe against raw writers (full statement in
`schema/hooks/convergence_guards.hook.yaml`): G-MT5's trigger resets `kanban_state` and syncs
`close_date` on any stage change, and REFUSES a close of a preventive recurring request that does
not carry the `app.maintenance_managed_transition` marker — only the service verb sets it, because
a raw close would skip the spawn. G-MT6 refuses soft-deleting a stage still in use.

## Where feedback goes

Both transition outcomes stage outbox events (`MaintenanceRequestStageChanged`,
`SuccessorSpawned`) — the named seam for the deferred activity-family feedbacks (MTBF/MTTR,
request->visit scheduling). Consumers attach through the host's outbox relay; nothing in this
module polls.
