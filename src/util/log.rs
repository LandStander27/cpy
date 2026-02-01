use std::path::Path;

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

#[macro_export]
macro_rules! print_error {
	($err:expr, $verbose:expr) => {{
		if $verbose < 4 {
			error!("{:#}", $err);
		} else {
			error!("{:?}", $err);
		}
	}};
}

pub fn add_err<'a>(s: &'a str, path: &'a Path) -> impl FnOnce() -> String + 'a {
	return move || format!("{s}: {}", path.display());
}

pub fn init(verbose: u8) -> Result<MultiProgress> {
	let mut log_level = LevelFilter::Warn;
	for _ in 0..verbose {
		log_level = log_level.increment_severity();
	}
	let logger = Builder::new()
		.format_timestamp(None)
		.filter_level(log_level)
		.format_target(false)
		.build();
	let multibar = MultiProgress::new();
	LogWrapper::new(multibar.clone(), logger)
		.try_init()
		.with_context(|| "could not init logger")?;
	log::set_max_level(log_level);
	trace!("init logger");

	return Ok(multibar);
}
