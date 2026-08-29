-- Down migration: drop the maintenance request family.
-- Order: triggers/functions first, then the tables (the FK rests on stages), then enums.

DROP TRIGGER IF EXISTS maintenance_requests_stage_transition ON maintenance.maintenance_requests;
DROP TRIGGER IF EXISTS maintenance_requests_insert_audit ON maintenance.maintenance_requests;
DROP TRIGGER IF EXISTS maintenance_requests_update_audit ON maintenance.maintenance_requests;
DROP TRIGGER IF EXISTS maintenance_stages_soft_delete_guard ON maintenance.maintenance_stages;
DROP TRIGGER IF EXISTS maintenance_stages_insert_audit ON maintenance.maintenance_stages;
DROP TRIGGER IF EXISTS maintenance_stages_update_audit ON maintenance.maintenance_stages;

DROP FUNCTION IF EXISTS maintenance.maintenance_requests_stage_transition();
DROP FUNCTION IF EXISTS maintenance.maintenance_requests_audit_timestamp();
DROP FUNCTION IF EXISTS maintenance.maintenance_stages_soft_delete_guard();
DROP FUNCTION IF EXISTS maintenance.maintenance_stages_audit_timestamp();

DROP TABLE IF EXISTS maintenance.maintenance_requests;
DROP TABLE IF EXISTS maintenance.maintenance_stages;

DROP TYPE IF EXISTS request_kanban_state;
DROP TYPE IF EXISTS request_priority;
DROP TYPE IF EXISTS repeat_unit;
DROP TYPE IF EXISTS repeat_type;
-- maintenance_type stays: the Schedule+Visit engine owns it and predates this family.
