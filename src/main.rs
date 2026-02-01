use std::{
	path::{Path, PathBuf},
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
};

use clap::Parser;
use env_logger::Builder;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use indicatif_log_bridge::LogWrapper;
use log::LevelFilter;
use signal_hook::consts::signal::*;
use signal_hook::iterator::Signals;

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

mod copy;
mod index;
mod verify;

pub trait AddError {
	fn add_err(self, src: &Path) -> String;
}

impl AddError for &str {
	fn add_err(self, src: &Path) -> String {
		return format!("{self}: `{}`", src.display());
	}
}

fn add_err<'a>(s: &'a str, path: &'a Path) -> impl FnOnce() -> String + 'a {
	return move || format!("{s}: {}", path.display());
}

// #[cfg(debug_assertions)]
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

#[derive(Parser, Debug, Clone)]
#[command(name = "cpy", disable_help_flag = true, disable_version_flag = true, version = version::version)]
#[command(about = "cp but better (hopefully)", long_about = None)]
pub struct Args {
	#[arg(short, long, help = "display help", action = clap::builder::ArgAction::Help)]
	help: (),

	#[arg(long, help = "print version", action = clap::builder::ArgAction::Version)]
	version: (),

	#[arg(short, long, help = "increase verbosity (-v: info, -vv: debug, -vvv: trace, -vvvv: trace, more detailed errors)", action = clap::ArgAction::Count)]
	verbose: u8,

	#[arg(short, visible_short_alias = 'R', long, help = "copy directories recursively")]
	recursive: bool,

	#[arg(short, long, help = "preserves all file attributes")]
	archive: bool,

	#[arg(help = "sources to copy", required = true)]
	src: Vec<String>,

	#[arg(help = "destination", required = true)]
	dest: String,
}

pub struct Options {
	pub verbose: u8,
	pub recursive: bool,
	pub archive: bool,
	pub dest_is_dir: bool,
	pub pb: ProgressBar,
	pub abort: Arc<AtomicBool>,
}

impl Options {
	pub fn new(args: &Args, dest: &Path, pb: ProgressBar, abort: Arc<AtomicBool>) -> Self {
		return Self {
			verbose: args.verbose,
			recursive: args.recursive,
			archive: args.archive,
			dest_is_dir: dest.exists() && dest.is_dir(),
			pb,
			abort,
		};
	}
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
		.format_target(false)
		.build();
	let multibar = MultiProgress::new();
	LogWrapper::new(multibar.clone(), logger)
		.try_init()
		.context("could not init logger")?;
	log::set_max_level(log_level);
	trace!("init logger");

	debug!("running with: {args:#?}");

	let mut src: Vec<PathBuf> = Vec::new();
	for i in args.src.iter().map(Path::new) {
		let can = match i
			.canonicalize()
			.with_context(add_err("could not canonicalize path", i))
		{
			Ok(o) => o,
			Err(e) => {
				print_error!(e, args.verbose);
				return Ok(());
			}
		};
		src.push(can);
	}
	let dest = PathBuf::from(&args.dest);

	trace!("verifying sources");
	if !verify::verify_sources(&src, &dest, &args)? {
		return Ok(());
	}

	let abort = Arc::new(AtomicBool::new(false));
	let mut signals = Signals::new([SIGINT, SIGTERM]).context("could not setup signal handler")?;
	std::thread::spawn({
		let abort = abort.clone();
		move || {
			for sig in signals.forever() {
				match sig {
					SIGINT | SIGTERM => {
						abort.store(true, Ordering::Relaxed);
					}
					_ => unreachable!(),
				}
			}
		}
	});

	trace!("indexing sources");
	let ticker = multibar.add(ProgressBar::new_spinner().with_style(ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.blue} {msg}").unwrap()));
	ticker.enable_steady_tick(std::time::Duration::from_millis(100));
	ticker.set_prefix("Indexing sources");

	let mut options = Options::new(&args, &dest, ticker.clone(), abort);
	let index = index::index(&src, dest, &options);
	ticker.disable_steady_tick();
	ticker.finish_and_clear();
	multibar.remove(&ticker);

	let pb = multibar.add(
		ProgressBar::new(index.total_size).with_style(
			ProgressStyle::with_template(
				"{msg:.bold} {wide_bar:.blue} {percent:>3}% • {binary_bytes}/{binary_total_bytes} • {binary_bytes_per_sec} • Elapsed: {elapsed_precise} • ETA:{eta_precise}",
			)
			.unwrap()
			.progress_chars("█▓▒░  "),
		),
	);
	options.pb = pb;
	options
		.pb
		.set_message(format!("Copying: 0/{} files", index.total_files));

	if let Err(e) = copy::copy(&index, &options) {
		print_error!(e, options.verbose);
	}

	debug!("index: {index:#?}");

	return Ok(());
}
