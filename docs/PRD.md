# backbone-maintenance — PRD

Operations-adjacents (Tier 5) · asset maintenance · the **9th GL producer**.

## Why
An asset-heavy SMB (fleet, equipment rental, workshop) needs to **maintain its fixed assets** — schedule
preventive service, run corrective visits on faults, consume spare parts, and see the **cost** land in the
books against the asset. This is the lean maintenance core: schedule → visit → complete, with parts issued
out of inventory and a maintenance-cost journal posted. Promoted for an asset-heavy customer
(tier5-deferred §6). Reads the asset (backbone-asset) and consumes parts (backbone-inventory).

## Scope (KEEP — tier5-deferred.md §6)
- **MaintenanceSchedule** — a preventive plan for an asset, recurring every `interval_days`; `next_due_date`
  tracks when the next visit is due.
- **MaintenanceVisit (+ Parts)** — one visit (preventive from a schedule, or corrective ad-hoc) on an
  asset: `planned → in_progress → completed | cancelled`, with part lines + labor.
- **Parts consumption** — on completion the visit's parts are **issued out of inventory**, valued at
  inventory's moving-average (the parts cost is inventory's number, not a made-up one), via an
  `InventoryPort`.
- **The cost post** — completion posts ONE balanced journal — the **9th GL producer** — `Dr Maintenance
  Expense (total) · Cr Inventory Parts (parts) · Cr Labor Payable (labor)`, `total = parts + labor`,
  idempotent per visit. A zero-cost visit completes without a post.
- **Warranty link** — a corrective visit may reference the support warranty claim it services.

## Non-goals (CUT / DEFER — tier5-deferred.md §6)
- **IoT / condition-based monitoring** (sensor-triggered maintenance).
- **AMC-contract billing** (annual maintenance contracts) — that's `backbone-billing` subscriptions.
- Labor scheduling / technician rostering, spare-parts forecasting.

## Success criteria
- The cost journal always balances (`total = parts + labor`) and posts **at most once** per visit
  (idempotent); parts are issued out of inventory **exactly once**.
- The journal lands in the REAL backbone-accounting ledger, accepted as `source_type='maintenance'`.
- Zero normal Cargo edge; survives a full codegen regen (§5).
