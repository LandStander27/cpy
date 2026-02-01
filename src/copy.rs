use std::{
	fs::File,
	path::Path,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
};

use nix::fcntl::copy_file_range;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

use crate::{
	index::Index,
	options::Options,
	print_error,
	util::{attr::copy_attributes, log::add_err},
};

pub fn copy(index: &Index, options: &Options) -> Result<()> {
	let completed_files = Arc::new(AtomicUsize::new(0));

	let pool = rayon::ThreadPoolBuilder::new()
		.num_threads(options.threads)
		.build()
		.wrap_err_with(|| "could not create thread pool")?;

	let results: Vec<Result<()>> = pool.install(|| {
		index
			.files
			.par_iter()
			.map(|task| copy_inner(&task.src, &task.dest, task.size, &completed_files, index.total_files, options))
			.collect()
	});

	for res in results.into_iter() {
		if let Err(e) = res {
			print_error!(e, options.verbose);
		}
	}

	info!("copied {} files", completed_files.load(Ordering::Relaxed));

	return Ok(());
}

fn copy_inner(src: &Path, dest: &Path, file_size: u64, completed_files: &Arc<AtomicUsize>, total_files: u64, options: &Options) -> Result<()> {
	trace!("{} -> {}", src.display(), dest.display());

	let src_file = File::open(src).with_context(add_err("could not open file read-only", src))?;
	let dest_file = File::create(dest).with_context(add_err("could not open file write-only", dest))?;

	const TARGET_UPDATES: u64 = 128;
	const MIN_CHUNK: usize = 4 * 1024 * 1024;
	let chunk_size = std::cmp::max(MIN_CHUNK, (file_size / TARGET_UPDATES) as usize);
	let mut total_copied = 0u64;

	loop {
		if options.abort.load(Ordering::Relaxed) {
			drop(dest_file);

			if !options.pb.is_finished() {
				options
					.pb
					.finish(&options.multibar, Some("Aborted".to_string()));
			}

			if let Err(e) = std::fs::remove_file(dest).with_context(add_err("could not remove incomplete file", dest)) {
				print_error!(e, options.verbose);
			}

			info!("cleaned up incomplete file: `{}`", dest.display());

			return Ok(());
		}

		let to_copy = std::cmp::min(chunk_size, (file_size - total_copied) as usize);
		if to_copy == 0 {
			break;
		}

		let copied = copy_file_range(&src_file, None, &dest_file, None, to_copy).with_context(add_err("could not copy file", src))?;

		if copied == 0 {
			break;
		}

		total_copied += copied as u64;
		options.pb.inc(copied as u64);
	}

	if options.archive {
		copy_attributes(src, dest).with_context(add_err("could not copy file attributes", src))?;
	}

	let completed = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
	options
		.pb
		.set_message(format!("copying: {}/{} files", completed, total_files));

	return Ok(());
}
