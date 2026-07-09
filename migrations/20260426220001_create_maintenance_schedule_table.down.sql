-- Down: drop maintenance.maintenance_schedules table
DROP TABLE IF EXISTS maintenance.maintenance_schedules CASCADE;
DROP FUNCTION IF EXISTS maintenance.maintenance_schedules_audit_timestamp() CASCADE;
