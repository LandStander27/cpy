#![cfg_attr(feature = "generators", allow(unreachable_code))]

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;

use clap::Parser;

use crate::{
	args::Args,
	options::Options,
	util::{exclude::ExcludeRules, prepare::create_directories, progress::ProgressBar},
};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, ContextCompat, eyre},
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

struct Reporter<S: AsRef<std::ffi::OsStr>> {
	command: Option<S>,
}

impl<S: AsRef<std::ffi::OsStr>> Reporter<S> {
	pub fn new(command: Option<S>) -> Self {
		return Self { command };
	}

	fn inner(&self, command: S, body: &str) -> Result<()> {
		let mut proc = Command::new("/bin/sh")
			.arg("-c")
			.arg(command)
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.stdin(Stdio::piped())
			.spawn()
			.context("could not spawn command")?;

		let mut stdin = proc.stdin.take().context("expected stdin")?;
		stdin
			.write_all(body.as_bytes())
			.context("could not write body to stdin")?;
		stdin
			.write_all(&[4])
			.context("could not write EOF to stdin")?;
		stdin.flush().context("could not flush stdin")?;

		drop(stdin);
		let status = proc.wait().context("command not running")?;
		if !status.success() {
			error!("--run-when-done command exited with non-zero ({}) exit code", status.code().unwrap_or(-1));
		}

		return Ok(());
	}

	pub fn report_finished(mut self, body: impl AsRef<str>, verbose: u8) {
		if let Some(s) = self.command.take()
			&& let Err(e) = self.inner(s, body.as_ref())
		{
			print_error!(e, verbose);
		}
	}
}

impl<S: AsRef<std::ffi::OsStr>> Drop for Reporter<S> {
	fn drop(&mut self) {
		if let Some(s) = self.command.take()
			&& let Err(e) = self.inner(s, "cpy exited for an unknown reason.")
		{
			print_error!(e, 255);
		}
	}
}

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

	trace!("compiling regexes");
	let rules = match ExcludeRules::compile(&args.exclude, args.verbose) {
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

	let reporter = Reporter::new(args.run_when_done.clone());

	trace!("indexing sources");
	let ticker = if args.quiet {
		ProgressBar::new_dummy()
	} else {
		ProgressBar::new_ticker(&multibar, "indexing sources", None)
	};
	let abort = match signal::signal_handler() {
		Ok(o) => o,
		Err(e) => {
			print_error!(e, args.verbose);
			return Ok(1.into());
		}
	};

	let mut options = Options::new(&args, &dest, rules, multibar.clone(), ticker.clone(), abort);
	let index = index::index(&src, dest, &mut options);
	ticker.finish(&multibar, None);

	if cfg!(debug_assertions) {
		trace!("{index:#?}");
	}

	if options.abort.load(Ordering::Relaxed) {
		return Ok(130.into());
	}

	if index.total_files == 0 && index.total_size == 0 && index.dirs.is_empty() && index.symlinks.is_empty() && index.files.is_empty() {
		warn!("no files to copy, exiting");
		return Ok(0.into());
	}

	if !index.dirs.is_empty() {
		trace!("creating dirs");
		let ticker = if args.quiet {
			ProgressBar::new_dummy()
		} else {
			ProgressBar::new_ticker(&multibar, "creating directories", None)
		};

		if !options.dry_run
			&& let Err(e) = create_directories(&index.dirs, &options)
		{
			reporter.report_finished(format!("could not copy due to error:\n{e:?}"), args.verbose);
			print_error!(e, args.verbose);
			return Ok(1.into());
		}

		ticker.finish(&multibar, None);

		if options.abort.load(Ordering::Relaxed) {
			return Ok(130.into());
		}
	}

	// 5 MB
	let pb = if args.quiet || (index.total_files == 1 && index.total_size <= 1024 * 1024 * 5) {
		ProgressBar::new_dummy()
	} else {
		ProgressBar::new_bar(&multibar, index.total_size, Some(format!("copying: 0/{} files", index.total_files)))
	};
	options.pb = pb;

	trace!("copying");
	match copy::copy(&index, &options) {
		Ok(o) if !o.is_empty() => {
			eprintln!("\n{o}");
			reporter.report_finished(&o, args.verbose);
		}
		Ok(_) => {
			reporter.report_finished(
				if args.src.len() == 1 {
					format!("copied **{}** files successfully.\n\n{} -> {}", index.total_files, args.src[0], args.dest)
				} else {
					format!(
						"copied **{}** files successfully.\n\n# Sources\n- {}\n # Destination\n{}",
						index.total_files,
						args.src.join("\n- "),
						args.dest
					)
				},
				args.verbose,
			);
		}
		Err(e) => {
			reporter.report_finished(format!("could not copy due to error:\n{e:?}"), args.verbose);
			print_error!(e, options.verbose);
			return Ok(1.into());
		}
	}

	if options.abort.load(Ordering::Relaxed) {
		return Ok(130.into());
	}

	return Ok(0.into());
}
