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

fn format_src(msg: &str, path: &Path) -> String {
	let mut s = String::new();
	s.push_str(msg);
	s.push_str(": `");
	s.push_str(&path.display().to_string());
	s.push('`');
	return s;
}

pub trait ContextCompatExt<T> {
	fn src(self, msg: &str, path: &Path) -> std::result::Result<T, color_eyre::eyre::Report>;
}

impl<T, R: color_eyre::eyre::ContextCompat<T>> ContextCompatExt<T> for R {
	fn src(self, msg: &str, path: &Path) -> std::result::Result<T, color_eyre::eyre::Report> {
		return self.with_context(|| format_src(msg, path));
	}
}

pub trait WrapErrExt<T, E> {
	fn src(self, msg: &str, path: &Path) -> std::result::Result<T, color_eyre::eyre::Report>;
}

impl<T, E, R: color_eyre::eyre::WrapErr<T, E>> WrapErrExt<T, E> for R {
	fn src(self, msg: &str, path: &Path) -> std::result::Result<T, color_eyre::eyre::Report> {
		return self.with_context(|| format_src(msg, path));
		// .with_section(|| path.display().to_string().header("File"));
	}
}

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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_format_src() {
		assert_eq!(format_src("some error", Path::new("/home")), "some error: `/home`");
	}
}
