-- Down: remove the company RLS fence for maintenance module

-- Reverse the company RLS fence for maintenance.maintenance_schedules
DROP POLICY IF EXISTS maintenance_schedules_company_isolation ON maintenance.maintenance_schedules;
ALTER TABLE maintenance.maintenance_schedules NO FORCE ROW LEVEL SECURITY;
ALTER TABLE maintenance.maintenance_schedules DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for maintenance.maintenance_visits
DROP POLICY IF EXISTS maintenance_visits_company_isolation ON maintenance.maintenance_visits;
ALTER TABLE maintenance.maintenance_visits NO FORCE ROW LEVEL SECURITY;
ALTER TABLE maintenance.maintenance_visits DISABLE ROW LEVEL SECURITY;

