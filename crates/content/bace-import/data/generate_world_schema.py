"""Generate frozen typed DTOs from the checked-in, pinned schema *definition*.
Never reads or parses SQL input/data: runtime extraction uses MariaDB's parser.
AGPL-3.0-only, source ACEmulator/ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b.
"""
from pathlib import Path
import re, hashlib
base=Path(__file__).resolve().parents[1]
content=base.parent/'bace-content'/'src'
tools=base.parent/'bace-content-tools'/'src'
schema=(base/'data/world-base.sql').read_text()
tables=[]
for name,body in re.findall(r'CREATE TABLE `([^`]+)` \((.*?)\n\) ENGINE',schema,re.S):
 if name=='weenie' or name.startswith('weenie_properties_'): continue
 fields=[]
 for line in body.splitlines():
  m=re.match(r'  `([^`]+)` ([a-z]+)(.*)',line)
  if not m: continue
  col,sql,tail=m.groups()
  ty={'int':'u32' if 'unsigned' in tail else 'i32','smallint':'u16' if 'unsigned' in tail else 'i16','tinyint':'u8' if 'unsigned' in tail else 'i8','bigint':'u64' if 'unsigned' in tail else 'i64','float':'f32','double':'f64','bit':'bool','datetime':'String','varchar':'String','text':'String'}[sql]
  nullable='NOT NULL' not in tail and 'GENERATED ALWAYS' not in tail
  rust=col.lower()
  if rust in ('type','use','mod','match','enum'): rust='r#'+rust
  fields.append((col,rust,ty,nullable))
 tables.append((name,''.join(p.title() for p in name.split('_'))+'RowV1',fields))
header='//! Frozen world rows from official ACE 47edade3bd3f6044b676d4eb877c4965c7eda62b.\n//! AGPL-3.0-only. Generated from data/world-base.sql; field order is schema 1.\n'
rows=header+'use serde::{Deserialize, Serialize};\n'
for name,ty,fields in tables:
 rows+='#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]\n#[serde(deny_unknown_fields)]\npub struct '+ty+' {\n'
 for col,rust,t,n in fields:
  rows+=f'    pub {rust}: '+(f'Option<{t}>' if n else t)+',\n'
 rows+='}\n'
(content/'world_rows.rs').write_text(rows)
rec=header+'use crate::world_rows::*;\nuse serde::{Deserialize, Serialize};\n#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]\npub enum WorldRecordV1 {\n'
for name,ty,fields in tables: rec+=f'    {ty[:-5]}('+(f'Box<{ty}>' if name=='spell' else ty)+'),\n'
rec+='}\nimpl WorldRecordV1 {\n    pub fn table_name(&self) -> &\'static str { match self {\n'
for name,ty,fields in tables: rec+=f'        Self::{ty[:-5]}(_) => "{name}",\n'
rec+='    }}\n    pub fn namespace(&self) -> u16 { match self {\n'
for i,(name,ty,fields) in enumerate(tables,16): rec+=f'        Self::{ty[:-5]}(_) => {i},\n'
rec+='    }}\n    pub fn id(&self) -> u64 { match self {\n'
for name,ty,fields in tables: rec+=f'        Self::{ty[:-5]}(r) => u64::from(r.{"guid" if name=="landblock_instance" else "id"}),\n'
rec+='    }}\n    pub fn validate(&self) -> Result<(), String> { match self {\n'
for name,ty,fields in tables:
 checks=[]
 for col,rust,t,n in fields:
  if t in ('f32','f64'): checks.append(f'r.{rust}.is_none_or(|v| v.is_finite())' if n else f'r.{rust}.is_finite()')
  if t=='String': checks.append(f'r.{rust}.as_ref().is_none_or(|v| v.len() <= 1048576)' if n else f'r.{rust}.len() <= 1048576')
 rec+=f'        Self::{ty[:-5]}({"r" if checks else "_"}) => '+ ('if '+ ' && '.join(checks)+' { Ok(()) } else { Err("nonfinite or oversized '+name+' field".into()) },\n' if checks else 'Ok(()),\n')
rec+='    }}\n}\n'
(content/'world_record.rs').write_text(rec)
ext=header+'use crate::{mariadb::IsolatedMariaDb, SqlStagingError};\nuse bace_content::WorldRecordV1;\nuse crate::sql_specs::TableSpec;\npub(crate) const WORLD_TABLES: &[TableSpec] = &[\n'
for name,ty,fields in tables:
 ext+=f'    TableSpec {{ name: "{name}", native: "", key: "", order: "'+('guid' if name=='landblock_instance' else 'id')+'", fields: &[\n'
 for col,rust,t,n in fields: ext+=f'        ("{col}", "{rust.removeprefix("r#")}"),\n'
 ext+='    ]},\n'
ext+='];\npub(crate) fn extract(db: &IsolatedMariaDb) -> Result<Vec<WorldRecordV1>, SqlStagingError> {\n let mut result = Vec::new();\n for spec in WORLD_TABLES {\n  for mut row in crate::sql_extract::rows(db,spec)? {\n'
# all bit fields cast via rows expression; normalize generic known keys
bits=sorted({rust.removeprefix('r#') for _,_,fs in tables for _,rust,t,_ in fs if t=='bool'})
for name,ty,fields in tables:
 for col,rust,t,n in fields:
  if t=='bool':
   field=rust.removeprefix('r#')
   ext+=f'   if spec.name == "{name}" && let Some(v) = row.get_mut("{field}") && !v.is_null() {{ *v = match v.as_u64() {{ Some(0)=>false.into(), Some(1)=>true.into(), _=>return Err(SqlStagingError::Extraction("invalid world boolean".into())) }}; }}\n'
ext+='   let value = serde_json::Value::Object(row);\n   let record = match spec.name {\n'
for name,ty,fields in tables: ext+=f'    "{name}" => WorldRecordV1::{ty[:-5]}(serde_json::from_value(value).map_err(|e| SqlStagingError::Extraction(e.to_string()))?),\n'
ext+='    _ => return Err(SqlStagingError::Extraction("unmapped world table".into())),\n   };\n   record.validate().map_err(SqlStagingError::Extraction)?;\n   if result.len() >= 3000000 { return Err(SqlStagingError::Extraction("world row limit".into())); }\n   result.push(record);\n  }\n }\n Ok(result)\n}\n'
(base/'src/world_extract.rs').write_text(ext)
print('tables',len(tables),'rows',len(rows.splitlines()),'record',len(rec.splitlines()),'extract',len(ext.splitlines()),'bits',bits)
