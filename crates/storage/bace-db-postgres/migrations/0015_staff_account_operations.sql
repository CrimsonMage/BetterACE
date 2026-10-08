-- Explicit staff account mutations are CAS-fenced and replayable. Normal login
-- auto-creation remains Player-only; this journal never stores password plaintext.
ALTER TABLE accounts ADD COLUMN revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0);
CREATE TABLE staff_account_operations (
 operation_id bytea PRIMARY KEY CHECK(octet_length(operation_id)=16),
 fingerprint bytea NOT NULL CHECK(octet_length(fingerprint)=32),
 issuer bigint REFERENCES accounts(id),
 account_id bigint NOT NULL REFERENCES accounts(id),
 canonical_name text NOT NULL CHECK(octet_length(canonical_name) BETWEEN 1 AND 200),
 result_revision bigint NOT NULL CHECK(result_revision>0),
 result_access smallint NOT NULL CHECK(result_access BETWEEN 0 AND 5),
 result_disabled boolean NOT NULL
);
