-- Down: drop the declared foreign keys for project module
ALTER TABLE IF EXISTS project.tasks DROP CONSTRAINT IF EXISTS fk_tasks_parent_task_id;
ALTER TABLE IF EXISTS project.tasks DROP CONSTRAINT IF EXISTS fk_tasks_project_id;
ALTER TABLE IF EXISTS project.project_template_tasks DROP CONSTRAINT IF EXISTS fk_project_template_tasks_template_id;
