-- Existing account ages are unknown; do not treat this migration as their birthday.
ALTER TABLE accounts ADD COLUMN created_unix bigint CHECK(created_unix IS NULL OR created_unix >= 0);
-- Defaults apply only to future native/account-admin inserts.
ALTER TABLE accounts ALTER COLUMN created_unix SET DEFAULT floor(extract(epoch FROM CURRENT_TIMESTAMP))::bigint;
