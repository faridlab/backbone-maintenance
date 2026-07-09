# backbone-maintenance — business flows & golden cases

## Flow: schedule → plan → parts → complete (cost post)
```
create_schedule (preventive plan, interval_days, next_due_date)
   │
   ▼  plan_visit → planned (preventive from a schedule, or corrective ad-hoc)
   │
   ▼  add_part (PLANNED only — the set freezes once completion begins) · start_visit (planned→in_progress)
   │
   ▼  complete_visit → claim in_progress (freeze) → issue parts via InventoryPort (valued moving-average)
   │      → total = labor + parts → post ONE balanced journal (skip if zero) → gate → completed
   │      Dr Maintenance Expense (total) · Cr Inventory Parts (parts) · Cr Labor Payable (labor)
   │      └─ re-complete → reaches the ledger 0 more times, returns the same journal (posts at most once)
   │
   └▶ MaintenanceCompleted (durable, staged in the completion tx)
```
Posts to the GL as the 9th producer (`source_type='maintenance'`). Zero-cost visits complete without a post.

## Golden cases (`tests/maintenance_golden_cases.rs`)
- **MGC-1 — balanced cost post.** Labor 200,000 + parts (3 × 5,000 = 15,000) → Dr Expense 215,000 · Cr
  Parts 15,000 · Cr Labor 200,000; balances, `source_type='maintenance'`.
- **MGC-2 — complete idempotent.** Re-completing a posted visit hits the ledger once.
- **MGC-3 — zero-cost no post.** A visit with no parts + no labor completes without touching the ledger.
- **MGC-4 — parts valued by inventory.** `parts_cost` = inventory's rate × quantity; the event carries it.
- **MGC-5 — preventive completion advances the schedule.** A 90-day schedule due 2026-07-01, completed that
  day → `next_due_date` → 2026-09-29 (the recurrence). Proven-by-revert.

## Integrity probes (`tests/integrity_probes.rs`)
- **MIP-1 — interval positive.**
- **MIP-2 — transition gates.** A cancelled visit cannot complete.
- **MIP-3 — parts only when open.** No parts on a completed visit.
- **MIP-4 — parts issued once.** Stock is issued once even across a re-complete.
- **MIP-5 — completion event durable.** With the in-proc publish lost, `MaintenanceCompleted` is staged in
  the outbox.
- **MIP-6 — part set frozen after start.** Once `in_progress`, `add_part` is refused — so a crash-and-retry
  can't widen the set and consume a part it fails to cost. Proven-by-revert.

## Seam (`tests/maintenance_gl_seam.rs`)
- **MSEAM-1 — cost journal lands balanced in REAL accounting.** Dr Expense 320,000 · Cr Parts 20,000 · Cr
  Labor 300,000; Σ = 0; accepted as `source_type='maintenance'`.
- **MSEAM-2 — re-complete reuses the journal.** The expense is not doubled.

## §5 round-trip (`scripts/maintenance_gl_seam_roundtrip.sh`)
Regen (`--force`) leaves the seam files byte-identical; the oracle + seam re-run green.
