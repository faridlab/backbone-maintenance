# backbone-maintenance — FSD

## Entities
MaintenanceSchedule (`company_id`, `asset_id` logical, `name`, `interval_days`, `next_due_date`,
`is_active`) · MaintenanceVisit (`company_id`, `asset_id` logical, `schedule_id?` FK, `maintenance_type`,
`status`, `warehouse_id?` logical, `warranty_claim_id?` logical, `scheduled_date`, `performed_date?`,
`labor_cost`, `parts_cost`, `total_cost`, the three GL account logical FKs, `journal_id?`/
`accounting_post_id?` logical) · MaintenanceVisitPart (`visit_id` FK, `item_id` logical, `quantity`,
`unit_cost`, `amount`). Enums: MaintenanceType {preventive, corrective}, VisitStatus {planned, in_progress,
completed, cancelled}. DB CHECKs: cost non-negativity + `status <> completed OR total = labor + parts`.
Money is IDR, 2dp, half-away.

## Write path (`MaintenanceWriteService`, hand-authored, user-owned)
- `create_schedule(NewSchedule)` → a preventive plan (positive interval)
- `plan_visit(NewVisit)` → a `planned` visit (preventive/corrective) carrying the three GL accounts
- `add_part(visit, item, quantity)` → a part line — **PLANNED visits only** (the set freezes once completion
  begins)
- `start_visit(visit)` → `planned → in_progress`
- `complete_visit(visit, performed_date, &dyn InventoryPort, &dyn GlPostSink, &dyn MaintenanceEventSink)` →
  claim `→ in_progress` (freeze), issue parts (valued by inventory), roll `total = labor + parts`, post ONE
  balanced journal (skip if zero), gate `→ completed` + stage `MaintenanceCompleted`; posts at most once
- `cancel_visit(visit)`

Errors: `MaintenanceError {Db, NotFound, InvalidState, Invalid, Unbalanced, GlRejected, InventoryRejected}`.

## Seams (ports — zero normal Cargo edge)
- **Post → accounting (proven, MSEAM-1/2):** ONE balanced `AccountingPostEnvelope` through `GlPostSink`;
  the 9th producer, `source_type='maintenance'`, idempotent per visit.
- **Issue → inventory:** `InventoryPort::issue_parts` moves + values the parts (moving-average),
  idempotent per `maintenance:{visit}`.
- **Outbound:** `MaintenanceCompleted` for asset/reporting.

## Test oracle
`maintenance_golden_cases` (5: MGC-1 balanced cost post, MGC-2 complete idempotent, MGC-3 zero-cost no post,
MGC-4 parts valued by inventory, MGC-5 preventive completion advances the schedule),
`integrity_probes` (6: MIP-1 interval positive, MIP-2 transition gates, MIP-3 parts only when open, MIP-4
parts issued once, MIP-5 completion event durable, MIP-6 part set frozen after start),
`maintenance_gl_seam` (2: MSEAM-1 balanced journal in REAL accounting, MSEAM-2 re-complete reuses the
journal) + §5 round-trip. **13 tests.**

> The generated `integration_tests.rs` hits an external HTTP server and is environmental scaffolding, not
> part of this module's correctness gate.
