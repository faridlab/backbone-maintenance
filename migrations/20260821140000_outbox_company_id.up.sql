-- Repair: carry company_id on maintenance.outbox_events
-- The outbox fence was moved into backbone-outbox's multi_tenant ensure (which creates the column
-- only when IT creates the table), and the module's own outbox DDL migration never gained the
-- column. On any database where the module's migration ran first, `CREATE TABLE IF NOT EXISTS`
-- makes the ensure a no-op and every `stage()` insert fails with "column company_id does not
-- exist". Guarded so databases where the ensure (or the retired hand-authored fence) already
-- added the column apply cleanly.

ALTER TABLE maintenance.outbox_events ADD COLUMN IF NOT EXISTS company_id UUID;
UPDATE maintenance.outbox_events SET company_id = (payload ->> 'company_id')::uuid WHERE company_id IS NULL;
ALTER TABLE maintenance.outbox_events ALTER COLUMN company_id SET NOT NULL;
CREATE INDEX IF NOT EXISTS idx_maintenance_outbox_company_id ON maintenance.outbox_events (company_id);
