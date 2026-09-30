-- Down: drop maintenance.maintenance_requests table
DROP TABLE IF EXISTS maintenance.maintenance_requests CASCADE;
DROP FUNCTION IF EXISTS maintenance.maintenance_requests_audit_timestamp() CASCADE;
