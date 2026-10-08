-- Official ObjectGuid dynamic range; 0xFFFFFFFF is reserved invalid.
CREATE SEQUENCE dynamic_object_ids AS bigint MINVALUE 2147483648 MAXVALUE 4294967294 NO CYCLE;
