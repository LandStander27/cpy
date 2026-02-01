use std::{
	path::{Path, PathBuf},
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
};

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use signal_hook::consts::signal::*;
use signal_hook::iterator::Signals;

use crate::{options::Options, util::log::add_err};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

mod args;
mod copy;
mod index;
mod options;
mod util;
mod verify;

fn main() -> Result<()> {
	color_eyre::install().with_context(|| "could not install eyre")?;
	let args = args::Args::parse();

	let multibar = util::log::init(args.verbose)?;

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
	let mut signals = Signals::new([SIGINT, SIGTERM]).with_context(|| "could not setup signal handler")?;
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

	// debug!("index: {index:#?}");

	return Ok(());
}
