-- Migration: company row-level-security fence for maintenance.maintenance_visit_parts
-- Hand-written (ADR-0010 Decision A: child-table fence). Mirrors the fence already on the
-- parent maintenance_visits/maintenance_schedules (see 20260426220004_enable_company_rls.up.sql),
-- and follows the same recipe billing/inventory/selling used for their child tables.
--
-- company_id is scoped per request via `set_config('app.company_id', <uuid>, true)`; an unset
-- var sees zero rows. The parent (maintenance.maintenance_visits) is already fenced, so the
-- backfill is deterministic — no fail-loud/ambiguity logic is required. company_id is a LOGICAL
-- FK to organization.companies (no hard SQL FK, per the module convention).

-- 1. Add the column nullable so existing rows survive the backfill.
ALTER TABLE maintenance.maintenance_visit_parts ADD COLUMN IF NOT EXISTS company_id UUID;

-- 2. Backfill FROM PARENT. Every part line inherits its visit's company.
UPDATE maintenance.maintenance_visit_parts AS p
   SET company_id = v.company_id
  FROM maintenance.maintenance_visits AS v
 WHERE p.visit_id = v.id
   AND p.company_id IS NULL;

-- 3. Tighten to NOT NULL now that every row has a company.
ALTER TABLE maintenance.maintenance_visit_parts ALTER COLUMN company_id SET NOT NULL;

-- 4. Enable + FORCE RLS (FORCE covers the table owner too, so only the app role — with the
--    policy predicate — can see rows; seeders/migrations run as owner before this point).
ALTER TABLE maintenance.maintenance_visit_parts ENABLE ROW LEVEL SECURITY;
ALTER TABLE maintenance.maintenance_visit_parts FORCE  ROW LEVEL SECURITY;

-- 5. The isolation policy: a row is visible/writable iff its company_id matches the request's
--    app.company_id. An unset var → NULLIF yields NULL → matches zero rows (fail-closed).
DROP POLICY IF EXISTS maintenance_visit_parts_company_isolation ON maintenance.maintenance_visit_parts;
CREATE POLICY maintenance_visit_parts_company_isolation ON maintenance.maintenance_visit_parts
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
