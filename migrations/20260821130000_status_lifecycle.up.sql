-- Migration: replace the maintenance-schedule active boolean with a status enum
-- maintenance.maintenance_schedules carried `is_active BOOLEAN NOT NULL DEFAULT TRUE`; the
-- tree-wide convention is one `status` enum field per lifecycle (see docs/refactoring-schema in
-- the serpa workspace). FALSE rows are written to 'inactive'; TRUE rows ride the new column's
-- DEFAULT 'active' (no UPDATE needed). The enum type is created unqualified so it lands beside
-- the module's other enum types (public), where the generated sqlx type_name resolves.

DO $$ BEGIN
    CREATE TYPE maintenance_schedule_status AS ENUM ('active', 'inactive');
EXCEPTION WHEN duplicate_object THEN NULL; END $$;

ALTER TABLE maintenance.maintenance_schedules ADD COLUMN status maintenance_schedule_status NOT NULL DEFAULT 'active';
UPDATE maintenance.maintenance_schedules SET status = 'inactive' WHERE NOT is_active;
ALTER TABLE maintenance.maintenance_schedules DROP COLUMN is_active;

DROP INDEX IF EXISTS maintenance.idx_maintenance_schedules_is_active_next_due_date;
CREATE INDEX IF NOT EXISTS idx_maintenance_schedules_status_next_due_date ON maintenance.maintenance_schedules (status, next_due_date);
