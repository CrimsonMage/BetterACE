//! Explicit source implementation exclusions. A native-owner candidate is not
//! an execution/parity claim; its concrete owner and source evidence are separate.
use crate::command_catalog::CommandSpec;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCompatibility {
    SourceTodo,
    Incompatible(&'static str),
    NativeOwnerRequired,
}
pub fn command_compatibility(spec: &CommandSpec) -> CommandCompatibility {
    if spec.source_stub {
        return CommandCompatibility::SourceTodo;
    }
    let reason = match spec.name {
        "allstats" => {
            "The source aggregates GCStatus and ACE database queue diagnostics with server statistics; BetterACE has no corresponding managed collector or mutable Biota queue."
        }
        "forcegc" | "forcegc2" => {
            "The source operates the .NET collector/large-object heap; BetterACE has no corresponding managed runtime."
        }
        "gcstatus" => {
            "The source reports .NET GC heap, generations, fragmentation and pause data; BetterACE has no corresponding managed collector."
        }
        "clearphysicscaches" => {
            "The source clears ACE static BSP/GfxObj/Polygon/Vertex caches; BetterACE retains immutable verified assets owned by active generations."
        }
        "clearcache" => {
            "The source invalidates live SQL content caches; BetterACE admits content through validated immutable .bace publication."
        }
        "database-shard-cache-pbrt" | "database-shard-cache-npbrt" => {
            "The source configures ACE mutable Biota cache retention; BetterACE uses frozen aggregates and explicit lease ownership."
        }
        "databaseperftest" => {
            "The source runs destructive synthetic Biota database benchmarking; use isolated native persistence qualification tooling."
        }
        "import-json" | "import-sql" | "import-sql-folders" => {
            "The source imports legacy authoring directly into live ACE SQL; native TOML compilation and the durable candidate-publication journal are required."
        }
        "export-json" | "export-json-folders" | "export-sql" | "export-sql-folders" => {
            "The source exports ACE database authoring formats; native content export belongs to bounded TOML tooling, not this live command handler."
        }
        "fix-allegiances"
        | "fix-biota-emote-delay"
        | "fix-gear-plating"
        | "fix-shortcut-bars"
        | "fix-spell-bars" => {
            "The source performs an ACE schema-specific historical row repair; frozen native data requires a separately reviewed versioned migration."
        }
        _ => return CommandCompatibility::NativeOwnerRequired,
    };
    CommandCompatibility::Incompatible(reason)
}
