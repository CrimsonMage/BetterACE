//! ACE.DatLoader CharGen, HeritageGroupCG, StarterArea, TemplateCG and SkillCG,
//! pinned 47edade3bd3f6044b676d4eb877c4965c7eda62b. AGPL-3.0-only.
use crate::{
    CreationGender, DatArchive, DatError, DatTableLimits, DatTableVersion,
    table_reader::{TableReader, load_record},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CreationPosition {
    pub cell: u32,
    pub origin: [f32; 3],
    /// Original DAT wire order is W, X, Y, Z.
    pub orientation_wxyz: [f32; 4],
}
#[derive(Clone, Debug, PartialEq)]
pub struct StarterArea {
    pub name: String,
    pub locations: Vec<CreationPosition>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CreationSkill {
    pub skill: u32,
    pub normal_cost: i32,
    pub primary_cost: i32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreationTemplate {
    pub name: String,
    pub icon: u32,
    pub title: u32,
    /// Strength, Endurance, Coordination, Quickness, Focus, Self.
    pub attributes: [u32; 6],
    pub normal_skills: Vec<u32>,
    pub primary_skills: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeritageGroup {
    pub name: String,
    pub icon: u32,
    pub setup: u32,
    pub environment_setup: u32,
    pub attribute_credits: u32,
    pub skill_credits: u32,
    pub primary_start_areas: Vec<i32>,
    pub secondary_start_areas: Vec<i32>,
    pub skills: Vec<CreationSkill>,
    pub templates: Vec<CreationTemplate>,
    /// ACE skips this byte; its meaning is not inferred here.
    pub gender_marker: u8,
    pub genders: BTreeMap<i32, CreationGender>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CharGen {
    pub reserved: u32,
    pub starter_areas: Vec<StarterArea>,
    /// ACE skips this byte; preserve it without inventing semantics.
    pub heritage_marker: u8,
    pub heritage_groups: BTreeMap<u32, HeritageGroup>,
}
impl CharGen {
    pub const RECORD_ID: u32 = 0x0e000002;
    pub fn decode(bytes: &[u8]) -> Result<Self, DatError> {
        Self::decode_with_limits(bytes, DatTableLimits::default())
    }
    pub fn load_verified(
        archive: &mut DatArchive,
        version: DatTableVersion,
    ) -> Result<Self, DatError> {
        Self::decode(&load_record(
            archive,
            Self::RECORD_ID,
            version,
            DatTableLimits::default(),
        )?)
    }
    pub fn decode_with_limits(bytes: &[u8], limits: DatTableLimits) -> Result<Self, DatError> {
        let mut reader = TableReader::new(bytes, Self::RECORD_ID, limits)?;
        let reserved = reader.u32()?;
        let starter_areas = reader.array(2, |r| {
            Ok(StarterArea {
                name: r.dotnet_string()?,
                locations: r.array(32, |r| {
                    Ok(CreationPosition {
                        cell: r.u32()?,
                        origin: [r.f32()?, r.f32()?, r.f32()?],
                        orientation_wxyz: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
                    })
                })?,
            })
        })?;
        let heritage_marker = reader.u8()?;
        let count = reader.smart_count(30)?;
        let mut heritage_groups = BTreeMap::new();
        for _ in 0..count {
            let id = reader.u32()?;
            let group = HeritageGroup::read(&mut reader)?;
            if heritage_groups.insert(id, group).is_some() {
                return Err(DatError::Format("duplicate DAT heritage ID"));
            }
        }
        reader.finish()?;
        Ok(Self {
            reserved,
            starter_areas,
            heritage_marker,
            heritage_groups,
        })
    }
}
impl HeritageGroup {
    fn read(reader: &mut TableReader<'_>) -> Result<Self, DatError> {
        let name = reader.dotnet_string()?;
        let icon = reader.u32()?;
        let setup = reader.u32()?;
        let environment_setup = reader.u32()?;
        let attribute_credits = reader.u32()?;
        let skill_credits = reader.u32()?;
        let primary_start_areas = reader.array(4, |r| Ok(r.u32()? as i32))?;
        let secondary_start_areas = reader.array(4, |r| Ok(r.u32()? as i32))?;
        let skills = reader.array(12, |r| {
            Ok(CreationSkill {
                skill: r.u32()?,
                normal_cost: r.u32()? as i32,
                primary_cost: r.u32()? as i32,
            })
        })?;
        let templates = reader.array(35, |r| {
            Ok(CreationTemplate {
                name: r.dotnet_string()?,
                icon: r.u32()?,
                title: r.u32()?,
                attributes: [r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?, r.u32()?],
                normal_skills: r.array(4, |r| r.u32())?,
                primary_skills: r.array(4, |r| r.u32())?,
            })
        })?;
        let gender_marker = reader.u8()?;
        let count = reader.smart_count(52)?;
        let mut genders = BTreeMap::new();
        for _ in 0..count {
            let id = reader.u32()? as i32;
            if genders.insert(id, CreationGender::read(reader)?).is_some() {
                return Err(DatError::Format("duplicate DAT gender ID"));
            }
        }
        Ok(Self {
            name,
            icon,
            setup,
            environment_setup,
            attribute_credits,
            skill_credits,
            primary_start_areas,
            secondary_start_areas,
            skills,
            templates,
            gender_marker,
            genders,
        })
    }
}
