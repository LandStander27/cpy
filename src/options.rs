use std::{
	path::Path,
	sync::{Arc, atomic::AtomicBool},
};

use indicatif::MultiProgress;

use crate::{
	args::{Args, ReflinkMode},
	util::progress::ProgressBar,
};

pub struct Options {
	pub verbose: u8,
	pub recursive: bool,
	pub archive: bool,
	pub dest_is_dir: bool,
	pub threads: usize,
	pub force: bool,
	pub update: bool,
	pub reflink: ReflinkMode,
	pub pb: ProgressBar,
	pub multibar: MultiProgress,
	pub abort: Arc<AtomicBool>,
}

impl Options {
	pub fn new(args: &Args, dest: &Path, multibar: MultiProgress, pb: ProgressBar, abort: Arc<AtomicBool>) -> Self {
		return Self {
			verbose: args.verbose,
			recursive: args.recursive,
			archive: args.archive,
			dest_is_dir: dest.exists() && dest.is_dir(),
			threads: args.threads,
			force: args.force,
			update: args.update,
			reflink: args.reflink,
			multibar,
			pb,
			abort,
		};
	}
}
