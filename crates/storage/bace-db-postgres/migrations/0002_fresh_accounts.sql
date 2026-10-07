-- Names are canonicalized by ace-auth using Unicode lowercase, without trimming/NFC.
-- The adapter checks canonical identity on both writes and reads; SQL LOWER is locale-dependent.
CREATE TABLE accounts (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY CHECK(id > 0),
    canonical_name text COLLATE "C" NOT NULL UNIQUE CHECK(octet_length(canonical_name) BETWEEN 1 AND 200),
    password_phc text NOT NULL CHECK(octet_length(password_phc) BETWEEN 1 AND 512),
    access_level smallint NOT NULL DEFAULT 0 CHECK(access_level BETWEEN 0 AND 5),
    disabled boolean NOT NULL DEFAULT false
);
