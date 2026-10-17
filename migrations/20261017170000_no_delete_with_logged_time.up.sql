-- A project or task that live timesheet rows cite is not deleted, soft or hard, by any writer: the
-- generic deletes, permanent delete and empty-trash included. The rows live in the timesheet
-- module's table and survive the delete, so they would point at nothing. An installation without
-- the timesheet module has no such rows, and the check is skipped there.
CREATE OR REPLACE FUNCTION project.refuse_delete_with_logged_time() RETURNS trigger AS $$
DECLARE
    cited boolean;
BEGIN
    IF TG_OP = 'UPDATE'
       AND NOT ((OLD.metadata->>'deleted_at') IS NULL AND (NEW.metadata->>'deleted_at') IS NOT NULL) THEN
        RETURN NEW;
    END IF;
    IF to_regclass('timesheet.timesheets') IS NOT NULL THEN
        IF TG_TABLE_NAME = 'projects' THEN
            EXECUTE 'SELECT EXISTS (SELECT 1 FROM timesheet.timesheets WHERE project_id = $1 AND (metadata->>''deleted_at'') IS NULL)'
                INTO cited USING OLD.id;
            IF cited THEN
                RAISE EXCEPTION 'project_has_timesheet_rows: project % still has logged time', OLD.id
                    USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_projects_g_prj_1__project_has_timesheet_rows';
            END IF;
        ELSE
            EXECUTE 'SELECT EXISTS (SELECT 1 FROM timesheet.timesheets WHERE task_id = $1 AND (metadata->>''deleted_at'') IS NULL)'
                INTO cited USING OLD.id;
            IF cited THEN
                RAISE EXCEPTION 'task_has_timesheet_rows: task % still has logged time', OLD.id
                    USING ERRCODE = 'check_violation', CONSTRAINT = 'ck_tasks_g_prj_2__task_has_timesheet_rows';
            END IF;
        END IF;
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS projects_no_delete_with_logged_time ON project.projects;
CREATE TRIGGER projects_no_delete_with_logged_time
    BEFORE UPDATE OR DELETE ON project.projects
    FOR EACH ROW EXECUTE FUNCTION project.refuse_delete_with_logged_time();

DROP TRIGGER IF EXISTS tasks_no_delete_with_logged_time ON project.tasks;
CREATE TRIGGER tasks_no_delete_with_logged_time
    BEFORE UPDATE OR DELETE ON project.tasks
    FOR EACH ROW EXECUTE FUNCTION project.refuse_delete_with_logged_time();
