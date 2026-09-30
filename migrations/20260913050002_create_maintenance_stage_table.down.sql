-- Down: drop maintenance.maintenance_stages table
DROP TABLE IF EXISTS maintenance.maintenance_stages CASCADE;
DROP FUNCTION IF EXISTS maintenance.maintenance_stages_audit_timestamp() CASCADE;
