use std::{
	path::Path,
	sync::{Arc, atomic::AtomicBool},
};

use indicatif::ProgressBar;

use crate::args::Args;

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
