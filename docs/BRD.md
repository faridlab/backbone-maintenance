# backbone-maintenance — BRD

## Documents
MaintenanceSchedule (preventive plan for an asset) · MaintenanceVisit (+ MaintenanceVisitPart). Own
Postgres schema `maintenance`. **Posts GL** (the 9th producer), `source_type='maintenance'`.

## Business rules

**BR-1 (schedule).** `create_schedule` defines a preventive plan for an asset with a positive
`interval_days` and a `next_due_date`.

**BR-2 (plan).** `plan_visit` opens a `planned` visit against an asset — preventive (referencing a
schedule) or corrective (ad-hoc, optionally referencing a support warranty claim). Labor cost is
non-negative. It carries the three GL accounts the cost post will use.

**BR-3 (parts).** `add_part` adds a part line (quantity) to a `planned`/`in_progress` visit; the value is
set on completion from inventory.

**BR-4 (complete — the cost invariant).** `complete_visit` issues the parts out of inventory (valued at
inventory's moving-average, via `InventoryPort`, **idempotent per visit**), rolls `total_cost = labor +
parts`, and posts ONE **balanced** journal — `Dr Maintenance Expense (total) · Cr Inventory Parts (parts) ·
Cr Labor Payable (labor)` — via `GlPostSink`, then transition-gates `→ completed`. Posts **at most once**
(idempotent on `source_id = visit_id`); a zero-cost visit completes without a post. The completion event
is staged in the same tx as the gating UPDATE (durable). Emits `MaintenanceCompleted`. For a preventive, schedule-linked visit, the schedule's `next_due_date` advances by `interval_days` (the recurrence) atomically in the completion tx (completeness council 2026-07-10).

**BR-5 (cancel).** `cancel_visit` cancels a `planned`/`in_progress` visit.

## Events
`MaintenanceCompleted` (visit_id, company_id, asset_id, journal_id?, labor_cost, parts_cost, total_cost).

## Deferred (with reason)
IoT/condition-based monitoring, AMC-contract billing (→ billing subscriptions), labor scheduling, spare-
parts forecasting (tier5-deferred §6).
