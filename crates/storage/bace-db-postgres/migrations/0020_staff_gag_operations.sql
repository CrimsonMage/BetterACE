-- Source gag properties remain inside the frozen player aggregate. This journal
-- fences exact retries against character login/logout and preserves the receipt.
CREATE TABLE staff_gag_operations (
 operation_id bytea PRIMARY KEY CHECK(octet_length(operation_id)=16),
 fingerprint bytea NOT NULL CHECK(octet_length(fingerprint)=32),
 issuer_account bigint NOT NULL REFERENCES accounts(id),
 character_id bigint NOT NULL REFERENCES character_ownership(character_id),
 mutation_revision numeric(20,0) NOT NULL CHECK(mutation_revision>=0),
 persisted_version bigint NOT NULL CHECK(persisted_version>0)
);
