# backbone-maintenance — Extension Guide

## Public surface (stable)
- **GL port** (`application::service::maintenance_gl`): `GlPostSink` + the contract envelope
  (`AccountingPostEnvelope`, `GlPostLine`, `GlPostAck`, `GlPostRejected`) — the cost-posting seam a
  composing service implements over accounting's `PostingService`. Zero normal Cargo edge.
- **Inventory port** (`application::service::maintenance_ports`): `InventoryPort` + DTOs (`PartsIssue`,
  `IssueLine`, `IssueAck`, `IssuedLineValue`, `InventoryRejected`) — the parts-issue seam a composing
  service implements over backbone-inventory. Idempotent per `idempotency_key` (`maintenance:{visit}`).
- **Events** (`application::service::maintenance_events`): `MaintenanceCompleted`, the `MaintenanceEvent`
  union, and `MaintenanceEventSink`.
- **Write path** (`application::service::maintenance_write_service::MaintenanceWriteService`):
  `create_schedule`, `plan_visit`, `add_part`, `start_visit`, `complete_visit`, `cancel_visit`.

## How a consuming service uses maintenance
Implement `InventoryPort::issue_parts` over backbone-inventory (move + value the stock) and `GlPostSink`
over accounting. Plan visits against an asset, add parts, and `complete_visit` — it issues the parts,
rolls parts + labor into the cost, and posts the balanced journal (the 9th producer). Subscribe to
`MaintenanceCompleted` for asset/reporting reactions.

## Not a contract
- The 12 generated CRUD endpoints per entity are convenience scaffolding. Do **not** flip a visit's status
  or write a cost through the generic PATCH surface — it bypasses the parts issue, the balanced-post build,
  and the post-once gate. Use `MaintenanceWriteService`.
- `// <<< CUSTOM` blocks preserve local edits only; not a cross-module extension point.

## Invariants a consumer must not break
- `total_cost = labor_cost + parts_cost`; the cost journal balances; a visit posts at most once
  (idempotent on `source_id = visit_id`).
- Parts are issued out of inventory at most once per visit (idempotent on the issue key).
