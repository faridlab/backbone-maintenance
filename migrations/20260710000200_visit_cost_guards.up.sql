-- Storage-layer backstop for the visit cost invariants. The schema's @non_negative/@precision never
-- emitted CHECKs, so the money identity lived only in the Rust complete path — the generic CRUD/PATCH
-- surface could set an unbalanced or negative cost (maturity council 2026-07-10). The identity
-- (total = labor + parts) is enforced for COMPLETED visits (pre-completion total_cost is a placeholder).
ALTER TABLE maintenance.maintenance_visits
  ADD CONSTRAINT visit_labor_non_negative CHECK (labor_cost >= 0),
  ADD CONSTRAINT visit_parts_non_negative CHECK (parts_cost >= 0),
  ADD CONSTRAINT visit_total_non_negative CHECK (total_cost >= 0),
  ADD CONSTRAINT visit_cost_identity_when_completed
    CHECK (status <> 'completed'::visit_status OR total_cost = labor_cost + parts_cost);

ALTER TABLE maintenance.maintenance_visit_parts
  ADD CONSTRAINT visit_part_quantity_non_negative CHECK (quantity >= 0),
  ADD CONSTRAINT visit_part_amount_non_negative   CHECK (amount >= 0);
