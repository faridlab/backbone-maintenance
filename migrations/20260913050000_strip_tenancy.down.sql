-- Hand-authored (user-owned). Not regenerated.
--
-- Reverse the tenancy strip: restore the module-native company fence the module
-- declared before ADR-0029 (strict NOT NULL company_id on schedules, visits,
-- visit_parts and requests; nullable shared_blank company_id on stages). Rows
-- the decorator moved to org_unit_id keep their org anchor — this down file
-- only re-adds the columns and the policies; it does not move data back.

ALTER TABLE maintenance.maintenance_schedules   ADD COLUMN IF NOT EXISTS company_id UUID NOT NULL DEFAULT gen_random_uuid();
ALTER TABLE maintenance.maintenance_visits      ADD COLUMN IF NOT EXISTS company_id UUID NOT NULL DEFAULT gen_random_uuid();
ALTER TABLE maintenance.maintenance_visit_parts ADD COLUMN IF NOT EXISTS company_id UUID;
ALTER TABLE maintenance.maintenance_requests    ADD COLUMN IF NOT EXISTS company_id UUID NOT NULL DEFAULT gen_random_uuid();
ALTER TABLE maintenance.maintenance_stages      ADD COLUMN IF NOT EXISTS company_id UUID;

-- Re-tighten the child table the historical chain tightened (NOT NULL was
-- guaranteed by its parent-visit backfill, not by a default).
ALTER TABLE maintenance.maintenance_visit_parts ALTER COLUMN company_id SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_maintenance_schedules_company_id_asset_id
    ON maintenance.maintenance_schedules (company_id, asset_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_visits_company_id_asset_id
    ON maintenance.maintenance_visits (company_id, asset_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_company_id_stage_id
    ON maintenance.maintenance_requests (company_id, stage_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_company_id_asset_id
    ON maintenance.maintenance_requests (company_id, asset_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_company_id_schedule_date
    ON maintenance.maintenance_requests (company_id, schedule_date);

DROP POLICY IF EXISTS maintenance_schedules_company_isolation ON maintenance.maintenance_schedules;
CREATE POLICY maintenance_schedules_company_isolation ON maintenance.maintenance_schedules
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);

DROP POLICY IF EXISTS maintenance_visits_company_isolation ON maintenance.maintenance_visits;
CREATE POLICY maintenance_visits_company_isolation ON maintenance.maintenance_visits
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);

DROP POLICY IF EXISTS maintenance_visit_parts_company_isolation ON maintenance.maintenance_visit_parts;
CREATE POLICY maintenance_visit_parts_company_isolation ON maintenance.maintenance_visit_parts
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);

DROP POLICY IF EXISTS maintenance_requests_company_isolation ON maintenance.maintenance_requests;
CREATE POLICY maintenance_requests_company_isolation ON maintenance.maintenance_requests
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);

-- The stage master: the shared_blank shape (NULL = the one shared set).
DROP POLICY IF EXISTS maintenance_stages_company_isolation ON maintenance.maintenance_stages;
CREATE POLICY maintenance_stages_company_isolation ON maintenance.maintenance_stages
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid OR company_id IS NULL)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid OR company_id IS NULL);
