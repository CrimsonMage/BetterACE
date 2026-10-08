-- Pinned ACE account bans retain start, expiry, issuer and reason. Expired bans
-- remain recorded until an exact expiry-clear operation commits at login.
ALTER TABLE accounts
    ADD COLUMN ban_started_unix_millis bigint,
    ADD COLUMN ban_expires_unix_millis bigint,
    ADD COLUMN ban_issuer_account_id bigint,
    ADD COLUMN ban_reason text,
    ADD CONSTRAINT account_ban_complete CHECK (
        (ban_started_unix_millis IS NULL
            AND ban_expires_unix_millis IS NULL
            AND ban_issuer_account_id IS NULL
            AND ban_reason IS NULL)
        OR (ban_started_unix_millis IS NOT NULL
            AND ban_expires_unix_millis IS NOT NULL
            AND ban_started_unix_millis >= 0
            AND ban_expires_unix_millis >= ban_started_unix_millis
            AND (ban_issuer_account_id IS NULL OR ban_issuer_account_id > 0)
            AND (ban_reason IS NULL OR octet_length(ban_reason) BETWEEN 1 AND 2048))
    );

CREATE INDEX accounts_banned_name_idx ON accounts(canonical_name)
    WHERE ban_expires_unix_millis IS NOT NULL;

CREATE TABLE staff_account_ban_operations (
    operation_id bytea PRIMARY KEY CHECK (octet_length(operation_id) = 16),
    fingerprint bytea NOT NULL CHECK (octet_length(fingerprint) = 32),
    account_id bigint NOT NULL REFERENCES accounts(id),
    result_revision bigint NOT NULL CHECK (result_revision > 0),
    result_started_unix_millis bigint,
    result_expires_unix_millis bigint,
    result_issuer_account_id bigint,
    result_reason text,
    CONSTRAINT account_ban_receipt_complete CHECK (
        (result_started_unix_millis IS NULL
            AND result_expires_unix_millis IS NULL
            AND result_issuer_account_id IS NULL
            AND result_reason IS NULL)
        OR (result_started_unix_millis IS NOT NULL
            AND result_expires_unix_millis IS NOT NULL
            AND result_started_unix_millis >= 0
            AND result_expires_unix_millis >= result_started_unix_millis
            AND (result_issuer_account_id IS NULL OR result_issuer_account_id > 0)
            AND (result_reason IS NULL OR octet_length(result_reason) BETWEEN 1 AND 2048))
    )
);
