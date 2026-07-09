# ADR-001 — The maintenance visit engine, parts valuation, and the cost post

Status: accepted · 2026-07-10 · Operations-adjacents (Tier 5; the 9th GL producer)

## Context
An asset-heavy SMB maintains its fixed assets: preventive schedules + corrective visits, consuming spare
parts, with the cost landing against the asset. Maintenance owns the visit lifecycle; it reads the asset
(backbone-asset), consumes parts (backbone-inventory), and posts the cost (backbone-accounting). Promoted
for an asset-heavy customer (tier5-deferred §6).

## Decision
1. **The visit is the unit of costing + posting.** `planned → in_progress → completed | cancelled`. On
   completion the visit issues its parts, rolls `total = labor + parts`, and posts ONE balanced journal.
2. **Parts are valued by inventory, not maintenance.** The `InventoryPort` moves + values the stock at
   moving-average; the parts cost is inventory's number. Issue is idempotent per `maintenance:{visit}`.
3. **The cost post is the 9th GL producer.** ONE balanced `AccountingPostEnvelope` —
   `Dr Maintenance Expense (total) · Cr Inventory Parts (parts) · Cr Labor Payable (labor)` — through a
   `GlPostSink`, `source_type='maintenance'`, idempotent per visit (transition-gated `→ completed`). A
   zero-cost visit completes without a post. Zero Cargo edge — proven vs REAL accounting (MSEAM-1/2).
4. **The part set is FROZEN once completion begins.** `add_part` accepts a planned visit only, and
   `complete_visit` claims the visit to `in_progress` before any external effect — so a crash-and-retry
   re-issues the identical set and never consumes a part it fails to cost/journal (maturity council).
5. **The completion event is durable** — staged in the same tx as the gating UPDATE.
6. **Reads the asset as a logical FK; posts money only on completion.**

## Consequences
- Turn maintenance off and no maintenance cost books; it is the one place service cost enters the ledger
  against an asset. Proven vs REAL accounting; durable across a lost publish; survives regen (§5).

## Parking lot (each with a gate)
- **Preventive recurrence was inert** — FIXED (completeness council 2026-07-10): `complete_visit` never advanced the schedule's `next_due_date`, so a preventive schedule froze overdue forever; now advances it inline in the completion tx by `interval_days` for a schedule-linked preventive visit (MGC-5, proven-by-revert).
- **Part set could widen mid-completion → consumed-but-uncosted part** — FIXED (maturity council
  2026-07-10): froze the set (`add_part` planned-only + claim-to-in_progress before issuing) + DB CHECKs on
  the cost identity/non-negativity (MIP-6, proven-by-revert).
- **Write-back UPDATEs not atomic with the gating tx** — self-healing on retry, but a reader between
  attempts sees valued lines on a non-completed visit. Gate: move the write-back into the gating tx.
- **Adapter idempotency** — recovery rests on the InventoryPort/GlPostSink honoring the `visit_id` key.
  Gate: contract tests on the composing adapters.
- **IoT/condition monitoring, AMC-contract billing, labor scheduling** — deferred (PRD non-goals).
