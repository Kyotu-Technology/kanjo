-- 1 when the title was chosen by the user (/rename), which an ai-title does not replace.
ALTER TABLE sessions ADD COLUMN title_user INTEGER NOT NULL DEFAULT 0;
