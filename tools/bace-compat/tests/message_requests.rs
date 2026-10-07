//! Differential input parsing against pinned official C# action/entity decoders.
use bace_wire::*;
use serde_json::Value;
fn fixtures() -> Value {
    serde_json::from_str(include_str!("../fixtures/messages.json")).unwrap()
}
fn hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn progression_requests_preserve_signed_train_credits_and_official_ignored_suffix() {
    for vector in fixtures()["vectors"]["progression_requests"]
        .as_array()
        .unwrap()
    {
        let bytes = hex(vector["bytes"].as_str().unwrap());
        let envelope = GameActionEnvelope::decode(&bytes, 16).unwrap();
        let request = ProgressionRequest::decode(envelope.action, envelope.payload, 16).unwrap();
        let (target, amount) = match request.action {
            ProgressionAction::RaiseAttribute {
                attribute,
                experience_spent,
            } => (attribute, i64::from(experience_spent)),
            ProgressionAction::RaiseVital {
                vital,
                experience_spent,
            } => (vital, i64::from(experience_spent)),
            ProgressionAction::RaiseSkill {
                skill,
                experience_spent,
            } => (skill, i64::from(experience_spent)),
            ProgressionAction::TrainSkill {
                skill,
                credits_spent,
            } => (skill, i64::from(credits_spent)),
        };
        assert_eq!(u64::from(target), vector["target"].as_u64().unwrap());
        assert_eq!(amount, vector["amount"].as_i64().unwrap());
        assert_eq!(
            request.trailing_bytes as u64,
            vector["trailing_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            bytes.len() as u64 - request.trailing_bytes as u64,
            vector["consumed"].as_u64().unwrap()
        );
    }
}

#[test]
fn full_character_creation_matches_official_entity_and_appearance_decoders() {
    for vector in fixtures()["vectors"]["creation_requests"]
        .as_array()
        .unwrap()
    {
        let bytes = hex(vector["bytes"].as_str().unwrap());
        let request = CharacterCreateRequest::decode(&bytes, 4096, 128, 64).unwrap();
        let decoded = &vector["decoded"];
        assert_eq!(request.account, vector["account"].as_str().unwrap());
        assert_eq!(request.unknown_constant, 99); // upstream skips this, not a required 1
        assert_eq!(
            u64::from(request.heritage),
            decoded["Heritage"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(request.gender),
            decoded["Gender"].as_u64().unwrap()
        );
        assert_eq!(
            i64::from(request.template_option),
            decoded["TemplateOption"].as_i64().unwrap()
        );
        for (name, value) in [
            ("StrengthAbility", request.abilities.strength),
            ("EnduranceAbility", request.abilities.endurance),
            ("CoordinationAbility", request.abilities.coordination),
            ("QuicknessAbility", request.abilities.quickness),
            ("FocusAbility", request.abilities.focus),
            ("SelfAbility", request.abilities.self_ability),
            ("CharacterSlot", request.character_slot),
            ("ClassId", request.class_id),
            ("StartArea", request.start_area),
        ] {
            assert_eq!(u64::from(value), decoded[name].as_u64().unwrap(), "{name}");
        }
        assert_eq!(
            request.skill_advancement_classes,
            vector["skills"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect::<Vec<_>>()
        );
        assert_eq!(request.name, decoded["Name"].as_str().unwrap());
        assert_eq!(
            request.requested_admin,
            decoded["IsAdmin"].as_bool().unwrap()
        );
        assert_eq!(
            request.requested_sentinel,
            decoded["IsSentinel"].as_bool().unwrap()
        );
        assert_eq!(
            request.trailing_bytes as u64,
            vector["trailing_bytes"].as_u64().unwrap()
        );
        assert_eq!(
            bytes.len() as u64 - request.trailing_bytes as u64,
            vector["consumed"].as_u64().unwrap()
        );
        let appearance = request.appearance;
        for (name, value) in [
            ("Eyes", appearance.eyes),
            ("Nose", appearance.nose),
            ("Mouth", appearance.mouth),
            ("HairColor", appearance.hair_color),
            ("EyeColor", appearance.eye_color),
            ("HairStyle", appearance.hair_style),
            ("HeadgearStyle", appearance.headgear_style),
            ("HeadgearColor", appearance.headgear_color),
            ("ShirtStyle", appearance.shirt_style),
            ("ShirtColor", appearance.shirt_color),
            ("PantsStyle", appearance.pants_style),
            ("PantsColor", appearance.pants_color),
            ("FootwearStyle", appearance.footwear_style),
            ("FootwearColor", appearance.footwear_color),
        ] {
            assert_eq!(
                u64::from(value),
                decoded["Appearance"][name].as_u64().unwrap(),
                "{name}"
            );
        }
        for (name, value) in [
            ("SkinHue", appearance.skin_hue),
            ("HairHue", appearance.hair_hue),
            ("HeadgearHue", appearance.headgear_hue),
            ("ShirtHue", appearance.shirt_hue),
            ("PantsHue", appearance.pants_hue),
            ("FootwearHue", appearance.footwear_hue),
        ] {
            assert_eq!(
                value,
                decoded["Appearance"][name].as_f64().unwrap(),
                "{name}"
            );
        }
        let consumed = bytes.len() - request.trailing_bytes;
        for length in 0..consumed {
            assert!(
                CharacterCreateRequest::decode(&bytes[..length], 4096, 128, 64).is_err(),
                "length {length}"
            );
        }
        assert!(CharacterCreateRequest::decode(&bytes, bytes.len() - 1, 128, 64).is_err());
        assert!(CharacterCreateRequest::decode(&bytes, 4096, 0, 64).is_err());
        if !request.skill_advancement_classes.is_empty() {
            assert!(CharacterCreateRequest::decode(&bytes, 4096, 128, 0).is_err());
        }
    }
}
