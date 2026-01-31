use clap::{Parser, ValueEnum};
use env_logger::Builder;
use indicatif::MultiProgress;
use indicatif_log_bridge::LogWrapper;
use log::LevelFilter;

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

#[derive(Parser, Debug, Clone)]
#[command(name = "cpy", version = version::version)]
#[command(about = "cp but better (hopefully)", long_about = None)]
pub struct Args {
	#[arg(short, long, help = "increase verbosity (-v: info, -vv: debug, -vvv: trace)", action = clap::ArgAction::Count)]
	verbose: u8,

	#[arg(help = "whether to copy or move", required = true)]
	mode: Mode,

	#[arg(help = "sources to copy", required = true)]
	src: Vec<String>,

	#[arg(help = "destination", required = true)]
	dest: String,
}

#[derive(ValueEnum, Debug, Clone)]
pub enum Mode {
	Copy,
	Move,
}

fn main() -> Result<()> {
	color_eyre::install().context("could not install eyre")?;
	let args = Args::parse();

	let mut log_level = LevelFilter::Warn;
	for _ in 0..args.verbose {
		log_level = log_level.increment_severity();
	}
	let logger = Builder::new()
		.format_timestamp(None)
		.filter_level(log_level)
		.build();
	let multibar = MultiProgress::new();
	LogWrapper::new(multibar.clone(), logger)
		.try_init()
		.context("could not init logger")?;
	log::set_max_level(log_level);
	trace!("init logger");

	return Ok(());
}
