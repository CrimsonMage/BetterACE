-- V1 aggregates remain unchanged. New schema2 item snapshots own explicit location;
-- V1 placement migration requires existing containment or an admitted world pose.
ALTER TABLE item_ownership ADD COLUMN pack_slot boolean NOT NULL DEFAULT false;
ALTER TABLE item_ownership ADD COLUMN equipped bigint NOT NULL DEFAULT 0 CHECK(equipped BETWEEN 0 AND 4294967295);
ALTER TABLE item_ownership DROP CONSTRAINT item_ownership_container_id_slot_key;
ALTER TABLE item_ownership ADD CONSTRAINT item_slot_unique UNIQUE(container_id,slot,pack_slot,equipped) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE item_ownership ADD CONSTRAINT item_slot_kind CHECK(NOT pack_slot OR equipped=0);
ALTER TABLE house_ownership ALTER COLUMN owner_id DROP NOT NULL;
ALTER TABLE house_ownership ADD COLUMN access_generation bigint NOT NULL DEFAULT 1 CHECK(access_generation>0);
CREATE TABLE item_places(
 item_id bigint PRIMARY KEY REFERENCES entity_snapshots(object_id) DEFERRABLE INITIALLY DEFERRED,
 kind smallint NOT NULL CHECK(kind BETWEEN 0 AND 2),
 cell_id bigint,
 CHECK((kind=1 AND cell_id BETWEEN 1 AND 4294967295) OR (kind<>1 AND cell_id IS NULL))
);
CREATE INDEX world_items_by_cell ON item_places(cell_id,item_id) WHERE kind=1;
-- Deferred consistency permits a transaction to change both relations atomically.
CREATE FUNCTION verify_item_place() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE target bigint; location_kind smallint; contained boolean;
BEGIN
 target := CASE WHEN TG_OP='DELETE' THEN OLD.item_id ELSE NEW.item_id END;
 SELECT kind INTO location_kind FROM item_places WHERE item_id=target;
 IF FOUND THEN
  SELECT EXISTS(SELECT 1 FROM item_ownership WHERE item_id=target) INTO contained;
  IF (location_kind=0)<>contained THEN RAISE EXCEPTION 'item placement/containment mismatch' USING ERRCODE='23514'; END IF;
 END IF;
 RETURN NULL;
END; $$;
CREATE CONSTRAINT TRIGGER item_place_consistent AFTER INSERT OR UPDATE OR DELETE ON item_places DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_item_place();
CREATE CONSTRAINT TRIGGER item_owner_consistent AFTER INSERT OR UPDATE OR DELETE ON item_ownership DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION verify_item_place();
ALTER TABLE item_ownership ALTER CONSTRAINT item_ownership_item_id_fkey DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE item_ownership ALTER CONSTRAINT item_ownership_container_id_fkey DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE house_ownership ALTER CONSTRAINT house_ownership_object_id_fkey DEFERRABLE INITIALLY DEFERRED;
