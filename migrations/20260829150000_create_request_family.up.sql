-- Migration: create the maintenance request family
-- (maintenance.maintenance_stages + maintenance.maintenance_requests)
--
-- Hand-written: the fence postures, the guard CHECKs, and the two consistency triggers are
-- part of the family's contract (schema/hooks/convergence_guards.hook.yaml, ADR-0014/0015)
-- and land with the tables in ONE migration.
--
-- Fence posture map (schema/models/index.model.yaml, company_fence: shared_blank):
--   maintenance_stages   -> genuinely shared_blank: nullable company_id, NULL rows are the
--                           one shared stage set every company session sees (the Odoo global
--                           stages; the manufacturing master-data pattern)
--   maintenance_requests -> strict-realized: NOT NULL company_id, equality policy with no
--                           NULL arm (the IS NULL arm of the shared_blank shape is dead here)

-- ==============================================================================
-- Enum types (guarded so the migration is order-independent against the older
-- enum-creation migrations; maintenance_type already exists from 20260426220000)
-- ==============================================================================

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'request_kanban_state') THEN
        CREATE TYPE request_kanban_state AS ENUM ('normal', 'blocked', 'done');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'request_priority') THEN
        CREATE TYPE request_priority AS ENUM ('very_low', 'low', 'normal', 'high');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'maintenance_type') THEN
        CREATE TYPE maintenance_type AS ENUM ('preventive', 'corrective');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'repeat_unit') THEN
        CREATE TYPE repeat_unit AS ENUM ('day', 'week', 'month', 'year');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'repeat_type') THEN
        CREATE TYPE repeat_type AS ENUM ('forever', 'until');
    END IF;
END
$$;

CREATE SCHEMA IF NOT EXISTS maintenance;

-- ==============================================================================
-- Table: maintenance.maintenance_stages (shared master data)
-- ==============================================================================

CREATE TABLE IF NOT EXISTS maintenance.maintenance_stages (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    company_id UUID,
    name TEXT NOT NULL,
    sequence INTEGER NOT NULL DEFAULT 20,
    fold BOOLEAN NOT NULL DEFAULT FALSE,
    done BOOLEAN NOT NULL DEFAULT FALSE,
    metadata JSONB NOT NULL DEFAULT '{"created_at":null,"updated_at":null,"deleted_at":null,"created_by":null,"updated_by":null,"deleted_by":null}'::jsonb,
    PRIMARY KEY (id)
);

CREATE INDEX IF NOT EXISTS idx_maintenance_stages_sequence_id ON maintenance.maintenance_stages (sequence, id);

-- GIN index for audit metadata JSONB queries
CREATE INDEX IF NOT EXISTS idx_maintenance_stages_metadata_gin ON maintenance.maintenance_stages USING GIN (metadata);
CREATE INDEX IF NOT EXISTS idx_maintenance_stages_metadata_deleted_at ON maintenance.maintenance_stages ((metadata->>'deleted_at'));
CREATE INDEX IF NOT EXISTS idx_maintenance_stages_metadata_created_at ON maintenance.maintenance_stages ((metadata->>'created_at'));
CREATE INDEX IF NOT EXISTS idx_maintenance_stages_metadata_updated_at ON maintenance.maintenance_stages ((metadata->>'updated_at'));

-- ==============================================================================
-- Table: maintenance.maintenance_requests (strict transaction table)
-- ==============================================================================

CREATE TABLE IF NOT EXISTS maintenance.maintenance_requests (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    company_id UUID NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    request_date DATE NOT NULL DEFAULT CURRENT_DATE,
    schedule_date TIMESTAMPTZ,
    schedule_end TIMESTAMPTZ,
    close_date DATE,
    duration NUMERIC(18, 2) NOT NULL DEFAULT 0 CHECK (duration >= 0),
    owner_user_id UUID,
    user_id UUID,
    asset_id UUID,
    stage_id UUID NOT NULL,
    kanban_state request_kanban_state NOT NULL DEFAULT 'normal',
    priority request_priority NOT NULL DEFAULT 'low',
    maintenance_type maintenance_type NOT NULL DEFAULT 'corrective',
    recurring BOOLEAN NOT NULL DEFAULT FALSE,
    repeat_interval INTEGER NOT NULL DEFAULT 1,
    repeat_unit repeat_unit NOT NULL DEFAULT 'week',
    repeat_type repeat_type NOT NULL DEFAULT 'forever',
    repeat_until DATE,
    successor_request_id UUID,
    successor_of_request_id UUID,
    metadata JSONB NOT NULL DEFAULT '{"created_at":null,"updated_at":null,"deleted_at":null,"created_by":null,"updated_by":null,"deleted_by":null}'::jsonb,
    PRIMARY KEY (id),
    -- The lifecycle reference: a REAL in-module FK. RESTRICT so a stage row cannot be
    -- hard-deleted out from under live or archived requests (G-MT_STAGE_FK).
    CONSTRAINT fk_maintenance_requests_stage_id
        FOREIGN KEY (stage_id) REFERENCES maintenance.maintenance_stages (id)
        ON DELETE RESTRICT,
    -- G-MT2 (widened fire-rule: a CHECK fires on every INSERT/UPDATE regardless of which
    -- column changed — updating schedule_date past an existing schedule_end is caught too).
    CONSTRAINT chk_maintenance_requests_schedule_order
        CHECK (schedule_end IS NULL OR schedule_date IS NULL OR schedule_end >= schedule_date),
    -- G-MT3
    CONSTRAINT chk_maintenance_requests_repeat_interval_positive
        CHECK (repeat_interval >= 1),
    -- G-MT7 (the citation fence made structural: an until-recurrence with no repeat_until
    -- is REJECTED, never silently non-spawning).
    CONSTRAINT chk_maintenance_requests_repeat_until_required
        CHECK (repeat_type <> 'until' OR repeat_until IS NOT NULL),
    -- G-MT8 (only preventive may recur)
    CONSTRAINT chk_maintenance_requests_only_preventive_recur
        CHECK (NOT (maintenance_type = 'corrective' AND recurring))
);

CREATE INDEX IF NOT EXISTS idx_maintenance_requests_company_id_stage_id ON maintenance.maintenance_requests (company_id, stage_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_company_id_asset_id ON maintenance.maintenance_requests (company_id, asset_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_stage_id ON maintenance.maintenance_requests (stage_id) WHERE (metadata->>'deleted_at') IS NULL;
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_company_id_schedule_date ON maintenance.maintenance_requests (company_id, schedule_date);

-- At most one spawned successor per source (the partial-unique backstop behind the
-- claim-marker CAS in the transition verb).
CREATE UNIQUE INDEX IF NOT EXISTS idx_maintenance_requests_successor_of_request_id
    ON maintenance.maintenance_requests (successor_of_request_id)
    WHERE successor_of_request_id IS NOT NULL;

-- GIN index for audit metadata JSONB queries
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_metadata_gin ON maintenance.maintenance_requests USING GIN (metadata);
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_metadata_deleted_at ON maintenance.maintenance_requests ((metadata->>'deleted_at'));
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_metadata_created_at ON maintenance.maintenance_requests ((metadata->>'created_at'));
CREATE INDEX IF NOT EXISTS idx_maintenance_requests_metadata_updated_at ON maintenance.maintenance_requests ((metadata->>'updated_at'));

-- ==============================================================================
-- Company fence (RLS). company_id is scoped per request via
-- `set_config('app.company_id', <uuid>, true)`; an unset var sees zero rows
-- (NULLIF yields NULL, matches nothing). FORCE covers the table owner too.
-- ==============================================================================

ALTER TABLE maintenance.maintenance_stages ENABLE ROW LEVEL SECURITY;
ALTER TABLE maintenance.maintenance_stages FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS maintenance_stages_company_isolation ON maintenance.maintenance_stages;
CREATE POLICY maintenance_stages_company_isolation ON maintenance.maintenance_stages
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid OR company_id IS NULL)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid OR company_id IS NULL);

ALTER TABLE maintenance.maintenance_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE maintenance.maintenance_requests FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS maintenance_requests_company_isolation ON maintenance.maintenance_requests;
CREATE POLICY maintenance_requests_company_isolation ON maintenance.maintenance_requests
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);

-- ==============================================================================
-- G-MT5: the stage-transition consistency trigger on maintenance_requests.
--
-- A CHECK cannot express the close_date arm (it must read the stage table's done flag),
-- so a BEFORE trigger is the mechanism. This is a CONSISTENCY guard, not a security
-- boundary. Arms:
--   1. kanban reset — on a stage change, kanban_state resets to normal unless the caller
--      set it in the same statement (NEW.kanban_state <> OLD.kanban_state) — the Odoo
--      unless-caller-set semantics;
--   2. close_date sync — entering a done stage stamps CURRENT_DATE, leaving clears NULL
--      (INSERT into a done stage stamps too, matching the Odoo create() path);
--   3. recurrence raise — a non-done -> done transition of a preventive recurring request
--      RAISES maintenance_recurring_close_requires_service_verb unless the transaction
--      set app.maintenance_managed_transition = '1' (the service verb's marker): a raw
--      close would skip the successor spawn, so it is refused instead of corrupting.
-- ==============================================================================

CREATE OR REPLACE FUNCTION maintenance.maintenance_requests_stage_transition() RETURNS trigger AS $$
DECLARE
    target_done BOOLEAN;
    source_done BOOLEAN;
BEGIN
    SELECT done INTO target_done FROM maintenance.maintenance_stages WHERE id = NEW.stage_id;

    IF TG_OP = 'UPDATE' THEN
        IF NEW.stage_id IS DISTINCT FROM OLD.stage_id THEN
            -- Arm 1: kanban reset unless the caller set kanban_state in this statement.
            IF NEW.kanban_state = OLD.kanban_state THEN
                NEW.kanban_state := 'normal';
            END IF;

            -- Arm 2: close_date follows the stage's done flag.
            IF target_done THEN
                NEW.close_date := CURRENT_DATE;
            ELSE
                NEW.close_date := NULL;
            END IF;

            -- Arm 3: closing a preventive recurring request requires the managed verb.
            SELECT done INTO source_done FROM maintenance.maintenance_stages WHERE id = OLD.stage_id;
            IF target_done AND NOT COALESCE(source_done, FALSE)
               AND NEW.maintenance_type = 'preventive' AND NEW.recurring
               AND COALESCE(current_setting('app.maintenance_managed_transition', true), '') <> '1' THEN
                RAISE EXCEPTION 'maintenance_recurring_close_requires_service_verb'
                    USING HINT = 'Close recurring preventive requests through the transition verb so the successor is spawned in the same transaction.';
            END IF;
        END IF;
    ELSE
        -- INSERT: entering a done stage stamps close_date up front (Odoo create() parity).
        IF target_done THEN
            NEW.close_date := CURRENT_DATE;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS maintenance_requests_stage_transition ON maintenance.maintenance_requests;
CREATE TRIGGER maintenance_requests_stage_transition
    BEFORE INSERT OR UPDATE ON maintenance.maintenance_requests
    FOR EACH ROW EXECUTE FUNCTION maintenance.maintenance_requests_stage_transition();

-- ==============================================================================
-- G-MT6: soft-delete protection for a referenced stage. Hard DELETE is already blocked
-- by the RESTRICT FK; this blocks the SOFT delete (metadata->>'deleted_at' set) while any
-- live request still references the stage. SECURITY DEFINER so the guard sees referencing
-- rows across the company fence — a shared NULL stage must not become undeletable-by-half
-- (visible to one company, referenced by another). It only reads; it never returns rows.
-- ==============================================================================

CREATE OR REPLACE FUNCTION maintenance.maintenance_stages_soft_delete_guard() RETURNS trigger AS $$
BEGIN
    IF NEW.metadata->>'deleted_at' IS DISTINCT FROM OLD.metadata->>'deleted_at'
       AND NEW.metadata->>'deleted_at' IS NOT NULL THEN
        IF EXISTS (SELECT 1 FROM maintenance.maintenance_requests r
                   WHERE r.stage_id = OLD.id AND (r.metadata->>'deleted_at') IS NULL) THEN
            RAISE EXCEPTION 'maintenance_stage_in_use'
                USING HINT = 'A stage referenced by live requests cannot be deleted.';
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = maintenance, public;

DROP TRIGGER IF EXISTS maintenance_stages_soft_delete_guard ON maintenance.maintenance_stages;
CREATE TRIGGER maintenance_stages_soft_delete_guard
    BEFORE UPDATE ON maintenance.maintenance_stages
    FOR EACH ROW EXECUTE FUNCTION maintenance.maintenance_stages_soft_delete_guard();

-- ==============================================================================
-- Audit-metadata triggers (the module's standard per-table pattern — see
-- 20260426220006_add_audit_triggers.up.sql)
-- ==============================================================================

CREATE OR REPLACE FUNCTION maintenance.maintenance_stages_audit_timestamp() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{created_at}', to_jsonb(NOW()));
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{updated_at}', to_jsonb(NOW()));
    ELSIF TG_OP = 'UPDATE' THEN
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{updated_at}', to_jsonb(NOW()));
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS maintenance_stages_insert_audit ON maintenance.maintenance_stages;
CREATE TRIGGER maintenance_stages_insert_audit BEFORE INSERT ON maintenance.maintenance_stages
    FOR EACH ROW EXECUTE FUNCTION maintenance.maintenance_stages_audit_timestamp();

DROP TRIGGER IF EXISTS maintenance_stages_update_audit ON maintenance.maintenance_stages;
CREATE TRIGGER maintenance_stages_update_audit BEFORE UPDATE ON maintenance.maintenance_stages
    FOR EACH ROW EXECUTE FUNCTION maintenance.maintenance_stages_audit_timestamp();

CREATE OR REPLACE FUNCTION maintenance.maintenance_requests_audit_timestamp() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{created_at}', to_jsonb(NOW()));
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{updated_at}', to_jsonb(NOW()));
    ELSIF TG_OP = 'UPDATE' THEN
        NEW.metadata = jsonb_set(NEW.metadata::jsonb, '{updated_at}', to_jsonb(NOW()));
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS maintenance_requests_insert_audit ON maintenance.maintenance_requests;
CREATE TRIGGER maintenance_requests_insert_audit BEFORE INSERT ON maintenance.maintenance_requests
    FOR EACH ROW EXECUTE FUNCTION maintenance.maintenance_requests_audit_timestamp();

DROP TRIGGER IF EXISTS maintenance_requests_update_audit ON maintenance.maintenance_requests;
CREATE TRIGGER maintenance_requests_update_audit BEFORE UPDATE ON maintenance.maintenance_requests
    FOR EACH ROW EXECUTE FUNCTION maintenance.maintenance_requests_audit_timestamp();
