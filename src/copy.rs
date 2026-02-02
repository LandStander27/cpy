use std::{
	fs::File,
	io::{BufWriter, Read, Write},
	path::Path,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
};

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
	args::ReflinkMode,
	index::Index,
	options::Options,
	print_error,
	util::{attr::copy_attributes, log::WrapErrExt},
};

pub fn copy(index: &Index, options: &Options) -> Result<()> {
	let completed_files = Arc::new(AtomicUsize::new(0));

	let pool = rayon::ThreadPoolBuilder::new()
		.num_threads(options.threads)
		.build()
		.with_context(|| "could not create thread pool")?;

	let results: Vec<Result<()>> = pool.install(|| {
		index
			.files
			.par_iter()
			.map(|task| copy_inner(&task.src, &task.dest, task.size, &completed_files, index.total_files, options))
			.collect()
	});

	let mut errors: Vec<color_eyre::Report> = Vec::new();

	for res in results.into_iter() {
		if let Err(e) = res {
			errors.push(e);
		}
	}

	if options.abort.load(Ordering::Relaxed) {
		let completed = completed_files.load(Ordering::Relaxed);

		options.pb.finish(&options.multibar, None);
		eprintln!("\ncompleted:  {} files", completed);
		eprintln!("remaining:  {} files", index.total_files as usize - completed);

		return Ok(());
	}

	if !errors.is_empty() {
		options
			.pb
			.finish(&options.multibar, Some("completed with errors".to_string()));
		eprintln!("\nfailed to copy {} file{}:", errors.len(), if errors.len() == 1 { "" } else { "s" });
		for err in errors.iter().take(5) {
			eprintln!("    {err:#}");
		}
		if errors.len() > 5 {
			eprintln!("    ... and {} more", errors.len() - 5);
		}
	}

	info!("copied {} files", completed_files.load(Ordering::Relaxed));
	options
		.pb
		.finish(&options.multibar, Some(format!("copied {} files successfully", completed_files.load(Ordering::Relaxed))));

	return Ok(());
}

fn copy_inner(src: &Path, dest: &Path, file_size: u64, completed_files: &Arc<AtomicUsize>, total_files: u64, options: &Options) -> Result<()> {
	if options.abort.load(Ordering::Relaxed) {
		return Ok(());
	}

	trace!("{} -> {}", src.display(), dest.display());

	if options.reflink != ReflinkMode::Never && reflink(src, dest, file_size, completed_files, total_files, options)? {
		return Ok(());
	}

	let mut src_file = File::open(src).src("could not open file read-only", src)?;
	let dest_file = match File::create_new(dest).src("could not open file write-only", dest) {
		Ok(o) => o,
		Err(e) if options.force => {
			std::fs::remove_file(dest).src("could not delete file after open fail", dest)?;
			File::create_new(dest).src("could not open file write-only", dest)?
		}
		Err(e) => return Err(e),
	};

	let mut accumulated_bytes = 0u64;

	let buffer_size: usize = if file_size < 1024 * 1024 {
		64 * 1024
	} else if file_size < 8 * 1024 * 1024 {
		256 * 1024
	} else if file_size < 64 * 1024 * 1024 {
		512 * 1024
	} else if file_size < 512 * 1024 * 1024 {
		1024 * 1024
	} else {
		2 * 1024 * 1024
	};

	let mut dest_file = BufWriter::with_capacity(buffer_size, dest_file);
	let mut buffer = vec![0_u8; buffer_size];

	const MAX_UPDATES: u64 = 128;
	let update_threshold = if file_size > MAX_UPDATES * buffer_size as u64 {
		file_size / MAX_UPDATES
	} else {
		buffer_size as u64
	};

	loop {
		if options.abort.load(Ordering::Relaxed) {
			dest_file.flush().src("could not flush data", dest)?;
			drop(dest_file);

			if !options.pb.is_finished() {
				options.pb.finish(&options.multibar, None);
			}

			if let Err(e) = std::fs::remove_file(dest).src("could not remove incomplete file", dest) {
				print_error!(e, options.verbose);
			}

			info!("cleaned up incomplete file: `{}`", dest.display());

			return Ok(());
		}

		let bytes_read = src_file.read(&mut buffer).src("could not read file", src)?;
		if bytes_read == 0 {
			break;
		}
		dest_file
			.write_all(&buffer[..bytes_read])
			.src("could not write to file", dest)?;

		accumulated_bytes += bytes_read as u64;
		if accumulated_bytes >= update_threshold {
			options.pb.inc(accumulated_bytes);
			accumulated_bytes = 0;
		}
	}

	if accumulated_bytes > 0 {
		options.pb.inc(accumulated_bytes);
	}

	dest_file.flush().src("could not flush data", dest)?;

	if options.archive {
		copy_attributes(src, dest).src("could not copy file attributes", src)?;
	}

	let completed = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
	options
		.pb
		.set_message(format!("copying: {}/{} files", completed, total_files));

	return Ok(());
}

fn reflink(src: &Path, dest: &Path, file_size: u64, completed_files: &Arc<AtomicUsize>, total_files: u64, options: &Options) -> Result<bool> {
	if dest.try_exists().unwrap_or(false) {
		if options.force && options.reflink == ReflinkMode::Always {
			std::fs::remove_file(dest).src("could not delete file", dest)?;
		} else {
			return Ok(false);
		}
	}

	match reflink_copy::reflink(src, dest) {
		Ok(()) => {
			options.pb.inc(file_size);
			if options.archive {
				copy_attributes(src, dest).src("could not copy file attributes", src)?;
			}

			let completed = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
			options
				.pb
				.set_message(format!("copying: {}/{} files", completed, total_files));
		}
		Err(e) if options.reflink == ReflinkMode::Always => {
			return Err(e).src("reflink failed", src)?;
		}
		Err(_) => {
			trace!("auto reflink failed");
			return Ok(false);
		}
	}

	return Ok(true);
}
