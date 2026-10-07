-- Synthetic official-writer-shaped fixture, including session variables,
-- ordered child records and SQL string syntax that must reach MariaDB intact.
DELETE FROM `weenie` WHERE `class_Id` = 900004;
INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (900004, 'complex-sql-fixture', 10, '2026-10-07 00:00:00');
INSERT INTO `weenie_properties_string` (`object_Id`,`type`,`value`)
VALUES (900004,1,'Name with ''quotes'', ; semicolon and 雪');
INSERT INTO `weenie_properties_attribute` (`object_Id`,`type`,`init_Level`,`level_From_C_P`,`c_P_Spent`)
VALUES (900004,1,100,1,50);
INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`,`type`,`init_Level`,`level_From_C_P`,`c_P_Spent`,`current_Level`)
VALUES (900004,1,100,1,50,99);
INSERT INTO `weenie_properties_book` (`object_Id`,`max_Num_Pages`,`max_Num_Chars_Per_Page`)
VALUES (900004,10,1000);
INSERT INTO `weenie_properties_book_page_data` (`object_Id`,`page_Id`,`author_Id`,`ignore_Author`,`page_Text`)
VALUES (900004,20,1,True,'Second page'),(900004,10,1,False,'First page');
INSERT INTO `weenie_properties_emote` (`object_Id`,`category`,`probability`)
VALUES (900004,1,1.0);
SET @parent_id = LAST_INSERT_ID();
INSERT INTO `weenie_properties_emote_action` (`emote_Id`,`order`,`type`,`message`,`display`,`motion`,`hero_X_P_64`)
VALUES (@parent_id,20,1,'Second action',NULL,NULL,9223372036854775807),
       (@parent_id,10,2,'First action',False,0x85000001,NULL);
INSERT INTO `weenie_properties_generator` (`object_Id`,`weenie_Class_Id`,`delay`,`probability`)
VALUES (900004,300,NULL,0.5);
INSERT INTO `weenie_properties_create_list` (`object_Id`,`destination_Type`,`weenie_Class_Id`,`palette`,`shade`,`try_To_Bond`)
VALUES (900004,1,300,-1,0.5,True);
INSERT INTO `weenie_properties_position` (`object_Id`,`position_Type`,`obj_Cell_Id`,`origin_X`,`origin_Y`,`origin_Z`,`angles_W`,`angles_X`,`angles_Y`,`angles_Z`)
VALUES (900004,1,1234,1,2,3,1,0,0,0);
INSERT INTO `weenie_properties_skill` (`object_Id`,`type`,`level_From_P_P`,`s_a_c`,`p_p`,`init_Level`,`resistance_At_Last_Check`,`last_Used_Time`)
VALUES (900004,1,65535,2,4294967295,100,0,1.25);
INSERT INTO `weenie_properties_spell_book` (`object_Id`,`spell`,`probability`)
VALUES (900004,1,0.5);
INSERT INTO `weenie_properties_event_filter` (`object_Id`,`event`)
VALUES (900004,-2147483648);
INSERT INTO `weenie_properties_palette` (`object_Id`,`sub_Palette_Id`,`offset`,`length`)
VALUES (900004,123,65535,1);
INSERT INTO `weenie_properties_anim_part` (`object_Id`,`index`,`animation_Id`)
VALUES (900004,255,4294967295);
INSERT INTO `weenie_properties_texture_map` (`object_Id`,`index`,`old_Id`,`new_Id`)
VALUES (900004,255,123,456);
