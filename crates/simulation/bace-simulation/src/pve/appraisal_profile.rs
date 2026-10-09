//! Source [AssessmentProperty] allowlists from pinned ACE.Entity.Enum.Properties.
//! These select immutable tables only; dynamic appraisal modifications and live
//! creature state must be applied before claiming a complete Identify response.
/// PropertyInt.cs SHA-256 fcef930ae21fa7b1828190949dc96f420a13e64913034ce5895a98334e04940e
const INT_ASSESSMENT: &[u16] = &[
    2, 5, 17, 19, 25, 26, 28, 30, 33, 35, 36, 38, 43, 45, 47, 86, 87, 89, 90, 91, 92, 98, 105, 106,
    107, 108, 109, 110, 111, 113, 114, 115, 117, 125, 131, 134, 158, 159, 160, 166, 170, 171, 172,
    173, 174, 175, 176, 177, 178, 179, 181, 188, 192, 193, 204, 257, 258, 259, 260, 261, 262, 263,
    265, 267, 268, 270, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280, 281, 287, 288, 289, 292,
    303, 304, 305, 306, 307, 308, 313, 314, 315, 316, 319, 320, 323, 324, 350, 351, 352, 353, 366,
    367, 368, 369, 370, 371, 372, 373, 374, 375, 376, 377, 378, 379, 381, 382, 383, 384, 386, 387,
    388, 389, 390,
];
/// PropertyInt64.cs SHA-256 995914e04b6d46779abbe684fc514c23e8e9240ca71c741c22e8190538207d7c
const INT64_ASSESSMENT: &[u16] = &[3, 4, 5];
/// PropertyBool.cs SHA-256 4f3842a1ab4c7ac0e6bd333b587362d690e7c8f169c4f6603f1eb778b1edf6fd
const BOOL_ASSESSMENT: &[u16] = &[2, 3, 63, 69, 85, 91, 94, 99, 100, 108, 130];
/// PropertyFloat.cs SHA-256 5cce6d9400ea30a4007dc61b70698e61a271a69857ba82c6a52da54b71d11714
const FLOAT_ASSESSMENT: &[u16] = &[
    5, 29, 87, 100, 136, 137, 144, 147, 149, 150, 152, 155, 157, 159, 167,
];
/// PropertyString.cs SHA-256 122e6b8de991dfc393ad6bfe895438f2d392751ee0e8ea6cbff860ebf7f83418
const STRING_ASSESSMENT: &[u16] = &[
    5, 7, 8, 10, 14, 15, 16, 21, 23, 25, 35, 38, 39, 40, 43, 47, 52,
];
/// PropertyDataId.cs SHA-256 dceb74593ebc07619a18a4d9cdd272f732403c20ee5291c17ca72800db4fa518
const DATAID_ASSESSMENT: &[u16] = &[9, 10, 11, 15, 16, 17, 41, 55];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AppraisalSourceTables {
    pub integers: Vec<(u16, i32)>,
    pub integers64: Vec<(u16, i64)>,
    pub booleans: Vec<(u16, bool)>,
    pub doubles: Vec<(u16, f64)>,
    pub strings: Vec<(u16, String)>,
    pub data_ids: Vec<(u16, u32)>,
}
impl AppraisalSourceTables {
    /// ACE AppraiseInfo.BuildProperties first copies only annotated properties.
    /// This is a pre-modification snapshot, never a complete live profile.
    pub fn select(source: &bace_content::WeenieV1) -> Self {
        let p = &source.properties;
        Self {
            integers: p
                .ints
                .iter()
                .filter_map(|row| assessed(row.id, INT_ASSESSMENT).map(|id| (id, row.value)))
                .collect(),
            integers64: p
                .int64s
                .iter()
                .filter_map(|row| assessed(row.id, INT64_ASSESSMENT).map(|id| (id, row.value)))
                .collect(),
            booleans: p
                .bools
                .iter()
                .filter_map(|row| assessed(row.id, BOOL_ASSESSMENT).map(|id| (id, row.value)))
                .collect(),
            doubles: p
                .floats
                .iter()
                .filter_map(|row| assessed(row.id, FLOAT_ASSESSMENT).map(|id| (id, row.value)))
                .collect(),
            strings: p
                .strings
                .iter()
                .filter_map(|row| {
                    assessed(row.id, STRING_ASSESSMENT).map(|id| (id, row.value.clone()))
                })
                .collect(),
            data_ids: p
                .data_ids
                .iter()
                .filter_map(|row| assessed(row.id, DATAID_ASSESSMENT).map(|id| (id, row.value)))
                .collect(),
        }
    }
}
fn assessed(id: u32, allow: &[u16]) -> Option<u16> {
    let id = u16::try_from(id).ok()?;
    allow.binary_search(&id).ok().map(|_| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bace_content::{Property, SparseProperties, WeenieV1};

    #[test]
    fn pinned_assessment_annotations_select_only_client_properties() {
        // ACE Level=25 and LongDesc=16 are annotated. Tolerance=67 and
        // ResistItemAppraisal=37 remain private source properties.
        let source = WeenieV1 {
            schema_version: 1,
            weenie_id: 100,
            class_name: "assessment_fixture".into(),
            weenie_type: 10,
            last_modified: None,
            properties: SparseProperties {
                ints: vec![
                    Property { id: 25, value: 10 },
                    Property { id: 37, value: 999 },
                    Property { id: 67, value: 2 },
                ],
                bools: vec![
                    Property { id: 2, value: true },
                    Property {
                        id: 19,
                        value: false,
                    },
                ],
                strings: vec![Property {
                    id: 16,
                    value: "source long description".into(),
                }],
                ..Default::default()
            },
        };
        let selected = AppraisalSourceTables::select(&source);
        assert_eq!(selected.integers, vec![(25, 10)]);
        assert_eq!(selected.booleans, vec![(2, true)]);
        assert_eq!(
            selected.strings,
            vec![(16, "source long description".into())]
        );
        assert!(selected.integers64.is_empty());
        assert!(selected.doubles.is_empty());
        assert!(selected.data_ids.is_empty());
    }

    #[test]
    fn assessment_ids_match_pinned_ace_enum_extraction() {
        let fixture = include_str!("../../tests/fixtures/appraisal_assessment.tsv");
        for (family, allowed) in [
            ("PropertyInt", INT_ASSESSMENT),
            ("PropertyInt64", INT64_ASSESSMENT),
            ("PropertyBool", BOOL_ASSESSMENT),
            ("PropertyFloat", FLOAT_ASSESSMENT),
            ("PropertyString", STRING_ASSESSMENT),
            ("PropertyDataId", DATAID_ASSESSMENT),
        ] {
            let source: Vec<u16> = fixture
                .lines()
                .filter(|line| !line.starts_with('#'))
                .filter_map(|line| {
                    let (name, id) = line.split_once('\t')?;
                    (name == family).then(|| id.parse().expect("pinned ACE enum ID"))
                })
                .collect();
            assert_eq!(allowed, source.as_slice(), "{family}");
        }
    }
}
