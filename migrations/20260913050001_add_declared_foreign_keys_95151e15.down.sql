-- Down: drop the declared foreign keys for maintenance module
ALTER TABLE IF EXISTS maintenance.maintenance_visit_parts DROP CONSTRAINT IF EXISTS fk_maintenance_visit_parts_visit_id;
ALTER TABLE IF EXISTS maintenance.maintenance_visits DROP CONSTRAINT IF EXISTS fk_maintenance_visits_schedule_id;
ALTER TABLE IF EXISTS maintenance.maintenance_requests DROP CONSTRAINT IF EXISTS fk_maintenance_requests_stage_id;
