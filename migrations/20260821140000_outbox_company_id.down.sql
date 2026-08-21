-- Down: remove the outbox company_id column restored by the paired up migration.

DROP INDEX IF EXISTS idx_maintenance_outbox_company_id;
ALTER TABLE maintenance.outbox_events DROP COLUMN IF EXISTS company_id;
