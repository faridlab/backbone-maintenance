-- Down: drop enum types for maintenance module
DROP TYPE IF EXISTS visit_status CASCADE;
DROP TYPE IF EXISTS maintenance_type CASCADE;
