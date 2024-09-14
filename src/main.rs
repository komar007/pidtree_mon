use std::{process::ExitCode, time::Duration};

use clap::Parser as _;
use log::error;
use with_daemon::with_daemon;

use config::Config;
use worker::Worker;

mod client;
mod config;
mod worker;

const UPDATE_INTERVAL: Duration = Duration::from_millis(1000);
const SOCKET_FILENAME: &str = "/tmp/pidtree_mon.sock";
const PID_FILENAME: &str = "/tmp/pidtree_mon.pid";

fn main() -> ExitCode {
    match entrypoint() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            println!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn entrypoint() -> Result<(), String> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("none")).init();
    let config = Config::parse();
    let framework_res = with_daemon(
        PID_FILENAME,
        SOCKET_FILENAME,
        |ctrl| Worker::new(UPDATE_INTERVAL, ctrl),
        Worker::handle_client,
        |stream| {
            client::run(
                stream,
                config.pids,
                config.timeout,
                config.fields,
                config.separator,
            )
        },
    );
    let client_res = framework_res.map_err(|e| format!("framework: {e}"))?;
    client_res.map_err(|e| format!("client: {e}"))
}
