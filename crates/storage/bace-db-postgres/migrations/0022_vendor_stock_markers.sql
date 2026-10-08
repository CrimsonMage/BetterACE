-- Vendor stock has its own unplaced durable owner. The vendor aggregate must
-- already be durable at the version fenced by the operation; transient-only
-- vendors cannot receive stock under item_ownership's container FK.
CREATE TABLE vendor_stock_markers (
    vendor_id bigint PRIMARY KEY CHECK (vendor_id BETWEEN 1 AND 4294967295),
    marker_id bigint NOT NULL UNIQUE CHECK (marker_id BETWEEN 2147483648 AND 4294967294),
    version bigint NOT NULL CHECK (version > 0),
    stock_revision bigint NOT NULL CHECK (stock_revision > 0),
    source_revision bigint NOT NULL CHECK (source_revision > 0),
    source_hash bytea NOT NULL CHECK (octet_length(source_hash) = 32),
    payload bytea NOT NULL CHECK (octet_length(payload) BETWEEN 53 AND 131124)
);
