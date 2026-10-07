use crate::simulation::{SimulationConfig, SimulationWorker};

pub fn run(ticks: u32, players: u32, bodies: u32) -> Result<(), Box<dyn std::error::Error>> {
    if ticks == 0 || ticks > 1_000_000 {
        return Err("ticks must be between 1 and 1000000".into());
    }
    let kernel = bace_simulation::synthetic_scenario(players, bodies)?;
    let report = SimulationWorker::spawn(
        kernel,
        SimulationConfig {
            tick_limit: Some(u64::from(ticks)),
            real_time: false,
            ..SimulationConfig::default()
        },
    )?
    .wait()?;
    if report.rejected_commands != 0 || report.discarded_commands != 0 {
        return Err(format!(
            "{} fixture commands rejected; {} discarded on exit",
            report.rejected_commands, report.discarded_commands
        )
        .into());
    }
    println!(
        "Dedicated synthetic worker: {ticks} fixed steps, {players} moving actors, {bodies} other bodies; p99 bucket upper bound {:?}, max {:?}.",
        report.p99_upper_bound, report.max_tick
    );
    println!(
        "No AC network, DAT collision, stock client, persistence latency, or real-time capacity claim."
    );
    Ok(())
}
