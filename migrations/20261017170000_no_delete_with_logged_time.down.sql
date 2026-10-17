DROP TRIGGER IF EXISTS tasks_no_delete_with_logged_time ON project.tasks;
DROP TRIGGER IF EXISTS projects_no_delete_with_logged_time ON project.projects;
DROP FUNCTION IF EXISTS project.refuse_delete_with_logged_time();
