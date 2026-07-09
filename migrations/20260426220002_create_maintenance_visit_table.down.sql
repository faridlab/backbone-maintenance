-- Down: drop maintenance.maintenance_visits table
DROP TABLE IF EXISTS maintenance.maintenance_visits CASCADE;
DROP FUNCTION IF EXISTS maintenance.maintenance_visits_audit_timestamp() CASCADE;
