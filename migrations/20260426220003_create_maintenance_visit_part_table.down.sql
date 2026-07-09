-- Down: drop maintenance.maintenance_visit_parts table
DROP TABLE IF EXISTS maintenance.maintenance_visit_parts CASCADE;
DROP FUNCTION IF EXISTS maintenance.maintenance_visit_parts_audit_timestamp() CASCADE;
