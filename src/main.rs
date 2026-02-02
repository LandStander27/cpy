use clap::Parser;

use crate::{
	args::Args,
	options::Options,
	util::{prepare::create_directories, progress::ProgressBar},
};

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
mod signal;
mod util;
mod verify;

fn main() -> Result<std::process::ExitCode> {
	let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();
	eyre_hook.install()?;
	std::panic::set_hook(Box::new(move |pi| {
		error!("{}", panic_hook.panic_report(pi));
	}));

	let args = Args::parse();

	let multibar = match util::log::init(args.verbose) {
		Ok(o) => o,
		Err(e) => {
			print_error!(e, args.verbose);
			return Ok(1.into());
		}
	};

	debug!("running with: {args:#?}");

	let (src, dest) = match util::prepare::prepare_paths(&args) {
		Ok(o) => o,
		Err(e) => {
			print_error!(e, args.verbose);
			return Ok(1.into());
		}
	};

	trace!("verifying sources");
	if !verify::verify_sources(&src, &dest, &args) {
		return Ok(1.into());
	}

	trace!("indexing sources");
	let ticker = ProgressBar::new_ticker(&multibar, "Indexing sources", None);
	let abort = match signal::signal_handler() {
		Ok(o) => o,
		Err(e) => {
			print_error!(e, args.verbose);
			return Ok(1.into());
		}
	};

	let mut options = Options::new(&args, &dest, multibar.clone(), ticker.clone(), abort);
	let index = index::index(&src, dest, &mut options);
	ticker.finish(&multibar, None);

	if !index.dirs.is_empty() {
		let ticker = ProgressBar::new_ticker(&multibar, "Creating directories", None);
		if let Err(e) = create_directories(&index.dirs) {
			print_error!(e, args.verbose);
			return Ok(1.into());
		}

		ticker.finish(&multibar, None);
	}

	let pb = ProgressBar::new_bar(&multibar, index.total_size, Some(format!("copying: 0/{} files", index.total_files)));
	options.pb = pb;

	if let Err(e) = copy::copy(&index, &options) {
		print_error!(e, options.verbose);
		return Ok(1.into());
	}

	return Ok(0.into());
}
