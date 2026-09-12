-- Hand-authored (user-owned). Not regenerated.
--
-- Strip every company-fence artifact from the maintenance tables (ADR-0029): the
-- module is tenant-agnostic; org scoping is installed by the COMPOSING service's
-- tenancy decorator, never by the module. Dropped here, per table: the
-- <table>_company_isolation RLS policy, the company-leading indexes, and the
-- company_id column.
--
-- The module declares no company-leading unique: the request family's only
-- unique is the successor partial (successor_of_request_id), which carries no
-- company scope — nothing is re-declared.
--
-- The composed-world shapes: maintenance_stages (the old shared_blank master)
-- fences with allow_root — rows the old policy's NULL arm admitted become
-- tenant-root-anchored at backfill, and the scope union (subtree ∪ tenant
-- root) carries the one-shared-set semantics; a stage anchored at a regular
-- unit stays that unit's private stage. Schedules, visits, visit_parts and
-- requests are plain org-scoped rows (decorator fill stamps the acting unit).
--
-- Ordering guard (the decorator must run FIRST on any database with data): the
-- module never moves tenancy data. A table is safe to strip when EITHER
--   a) it carries org_unit_id with no NULLs — the decorator backfilled it from
--      company_id — or b) it is empty (a fresh database).
-- Otherwise the strip RAISEs, naming the decorator step, rather than dropping
-- a column that still holds the only tenancy key. The file is re-runnable
-- (every drop is IF EXISTS and the tracker has no checksums), so a failed run
-- retries cleanly after the decorator lands.
--
-- RLS enable/force flags are deliberately NOT touched: the decorator owns those
-- now.

DO $$
DECLARE
    t text;
    has_org boolean;
    org_nulls bigint;
    total bigint;
    offenders text := '';
BEGIN
    FOREACH t IN ARRAY ARRAY['maintenance_schedules', 'maintenance_visits',
                             'maintenance_visit_parts', 'maintenance_requests',
                             'maintenance_stages']
    LOOP
        IF to_regclass(format('maintenance.%I', t)) IS NULL THEN
            CONTINUE; -- chain not fully applied on this database; nothing to strip
        END IF;

        SELECT EXISTS (
                   SELECT 1 FROM information_schema.columns
                   WHERE table_schema = 'maintenance' AND table_name = t AND column_name = 'org_unit_id'
               )
        INTO has_org;

        EXECUTE format('SELECT count(*) FROM maintenance.%I', t) INTO total;

        IF has_org THEN
            EXECUTE format(
                'SELECT count(*) FROM maintenance.%I WHERE org_unit_id IS NULL', t)
            INTO org_nulls;
        ELSE
            org_nulls := total; -- no org column: every row's only tenancy key is company_id
        END IF;

        IF has_org AND org_nulls = 0 THEN
            CONTINUE; -- decorator backfilled: safe
        END IF;
        IF total = 0 THEN
            CONTINUE; -- empty table (fresh database): safe
        END IF;
        offenders := offenders || format(' maintenance.%s (%s rows, %s rows not covered by org_unit_id);', t, total, org_nulls);
    END LOOP;

    IF offenders <> '' THEN
        RAISE EXCEPTION 'refusing to strip company_id — these tables are not yet covered by the tenancy decorator:%. Apply the composing service''s tenancy decorator (it backfills org_unit_id from company_id) and re-run; it is the only step that moves tenancy data.', offenders;
    END IF;
END $$;

DROP POLICY IF EXISTS maintenance_schedules_company_isolation   ON maintenance.maintenance_schedules;
DROP POLICY IF EXISTS maintenance_visits_company_isolation      ON maintenance.maintenance_visits;
DROP POLICY IF EXISTS maintenance_visit_parts_company_isolation ON maintenance.maintenance_visit_parts;
DROP POLICY IF EXISTS maintenance_requests_company_isolation    ON maintenance.maintenance_requests;
DROP POLICY IF EXISTS maintenance_stages_company_isolation      ON maintenance.maintenance_stages;

DROP INDEX IF EXISTS maintenance.idx_maintenance_schedules_company_id_asset_id;
DROP INDEX IF EXISTS maintenance.idx_maintenance_visits_company_id_asset_id;
DROP INDEX IF EXISTS maintenance.idx_maintenance_requests_company_id_stage_id;
DROP INDEX IF EXISTS maintenance.idx_maintenance_requests_company_id_asset_id;
DROP INDEX IF EXISTS maintenance.idx_maintenance_requests_company_id_schedule_date;

ALTER TABLE maintenance.maintenance_schedules   DROP COLUMN IF EXISTS company_id;
ALTER TABLE maintenance.maintenance_visits      DROP COLUMN IF EXISTS company_id;
ALTER TABLE maintenance.maintenance_visit_parts DROP COLUMN IF EXISTS company_id;
ALTER TABLE maintenance.maintenance_requests    DROP COLUMN IF EXISTS company_id;
ALTER TABLE maintenance.maintenance_stages      DROP COLUMN IF EXISTS company_id;
