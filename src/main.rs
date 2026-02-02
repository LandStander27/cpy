#![cfg_attr(feature = "generators", allow(unreachable_code))]

use std::sync::atomic::Ordering;

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
	let args = Args::parse();

	#[cfg(feature = "generators")]
	{
		use clap::CommandFactory;
		let man = clap_mangen::Man::new(Args::command());
		let mut buffer: Vec<u8> = Default::default();
		man.render(&mut buffer).unwrap();
		std::fs::write(args.generate_man, buffer).unwrap();

		#[allow(unused)]
		use clap_complete::{Generator, Shell, generate};
		clap_complete::aot::generate(args.generate_shell, &mut Args::command(), Args::command().get_name().to_string(), &mut std::io::stdout());

		return Ok(0.into());
	}

	let (panic_hook, eyre_hook) = color_eyre::config::HookBuilder::default().into_hooks();
	eyre_hook.install()?;
	std::panic::set_hook(Box::new(move |pi| {
		error!("{}", panic_hook.panic_report(pi));
	}));

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
	let ticker = if args.quiet {
		ProgressBar::new_dummy()
	} else {
		ProgressBar::new_ticker(&multibar, "Indexing sources", None)
	};
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

	if options.abort.load(Ordering::Relaxed) {
		return Ok(130.into());
	}

	if !index.dirs.is_empty() {
		let ticker = if args.quiet {
			ProgressBar::new_dummy()
		} else {
			ProgressBar::new_ticker(&multibar, "Creating directories", None)
		};

		if !options.dry_run
			&& let Err(e) = create_directories(&index.dirs)
		{
			print_error!(e, args.verbose);
			return Ok(1.into());
		}

		ticker.finish(&multibar, None);

		if options.abort.load(Ordering::Relaxed) {
			return Ok(130.into());
		}
	}

	// 5 MB
	let pb = if args.quiet || (index.total_files == 1 && index.total_size <= 1024 * 1024 * 1024 * 5) {
		ProgressBar::new_dummy()
	} else {
		ProgressBar::new_bar(&multibar, index.total_size, Some(format!("copying: 0/{} files", index.total_files)))
	};
	options.pb = pb;

	if let Err(e) = copy::copy(&index, &options) {
		print_error!(e, options.verbose);
		return Ok(1.into());
	}

	if options.abort.load(Ordering::Relaxed) {
		return Ok(130.into());
	}

	return Ok(0.into());
}
