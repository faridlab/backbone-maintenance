-- Down: restore the maintenance-schedule active boolean from the status enum
-- Only 'inactive' rows are written back to FALSE; 'active' rows ride the boolean DEFAULT TRUE.

ALTER TABLE maintenance.maintenance_schedules ADD COLUMN is_active BOOLEAN NOT NULL DEFAULT TRUE;
UPDATE maintenance.maintenance_schedules SET is_active = FALSE WHERE status = 'inactive';
ALTER TABLE maintenance.maintenance_schedules DROP COLUMN status;

DROP INDEX IF EXISTS idx_maintenance_schedules_status_next_due_date;
CREATE INDEX IF NOT EXISTS idx_maintenance_schedules_is_active_next_due_date ON maintenance.maintenance_schedules (is_active, next_due_date);

DROP TYPE IF EXISTS maintenance_schedule_status;
