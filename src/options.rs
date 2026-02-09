use std::{
	path::Path,
	sync::{Arc, atomic::AtomicBool},
};

use indicatif::MultiProgress;

use crate::{
	args::{Args, ReflinkMode},
	util::{exclude::ExcludeRules, progress::ProgressBar},
};

pub struct Options {
	pub verbose: u8,
	pub recursive: bool,
	pub archive: bool,
	pub dest_is_dir: bool,
	pub threads: usize,
	pub force: bool,
	pub update: bool,
	pub dry_run: bool,
	pub one_file_system: bool,
	pub verify: bool,
	pub one_source: bool,
	pub exclude_rules: ExcludeRules,
	pub reflink: ReflinkMode,
	pub pb: ProgressBar,
	pub multibar: MultiProgress,
	pub abort: Arc<AtomicBool>,
}

impl Options {
	pub fn new(args: &Args, dest: &Path, exclude_rules: ExcludeRules, multibar: MultiProgress, pb: ProgressBar, abort: Arc<AtomicBool>) -> Self {
		return Self {
			verbose: args.verbose,
			recursive: args.recursive,
			archive: args.archive,
			dest_is_dir: args.src.len() > 1 || dest.is_dir(),
			one_source: args.src.len() == 1,
			threads: args.threads,
			force: args.force,
			update: args.update,
			reflink: args.reflink,
			dry_run: args.dry_run,
			one_file_system: args.one_file_system,
			verify: args.verify,
			exclude_rules,
			multibar,
			pb,
			abort,
		};
	}
}
