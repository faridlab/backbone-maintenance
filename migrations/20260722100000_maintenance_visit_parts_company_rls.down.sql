-- Rollback: drop the company isolation fence and the column.
-- Reverses 20260722100000_maintenance_visit_parts_company_rls.up.sql exactly.

-- 5. Drop the policy.
DROP POLICY IF EXISTS maintenance_visit_parts_company_isolation ON maintenance.maintenance_visit_parts;

-- 4. Disable RLS (drop FORCE implicitly — disabling also disengages FORCE).
ALTER TABLE maintenance.maintenance_visit_parts NO FORCE ROW LEVEL SECURITY;
ALTER TABLE maintenance.maintenance_visit_parts DISABLE ROW LEVEL SECURITY;

-- 3. Allow NULL again (the column came in nullable, NOT NULL was step 3 of the up).
ALTER TABLE maintenance.maintenance_visit_parts ALTER COLUMN company_id DROP NOT NULL;

-- 2. No row-level undo for the backfill — company_id was newly populated by the up migration,
--    and dropping the column next erases it entirely.

-- 1. Drop the column.
ALTER TABLE maintenance.maintenance_visit_parts DROP COLUMN IF EXISTS company_id;
