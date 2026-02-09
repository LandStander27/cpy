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
	util::{attr::copy_attributes, checksum, log::WrapErrExt},
};

pub fn copy(index: &Index, options: &Options) -> Result<()> {
	let completed_files = Arc::new(AtomicUsize::new(0));
	if !index.symlinks.is_empty() {
		for link in &index.symlinks {
			info!("symlink {} -> {}", link.dest.display(), link.target.display());

			if !options.dry_run {
				match std::os::unix::fs::symlink(&link.target, &link.dest).src("could not create symlink", &link.dest) {
					Ok(_) => (),
					Err(e) if options.force => {
						info!("(from --force) deleting {}", link.dest.display());
						std::fs::remove_file(&link.dest).src("could not delete symlink after fail", &link.dest)?;
						std::os::unix::fs::symlink(&link.target, &link.dest).src("could not create symlink", &link.dest)?;
					}
					Err(e) => return Err(e),
				}
			}

			let completed = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
			options
				.pb
				.set_message(format!("copying: {}/{} files", completed, index.total_files));
		}

		debug!("created {} symlink(s)", index.symlinks.len());
	}

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

	if options.verbose >= 4 {
		errors.iter().for_each(|x| print_error!(x, options.verbose));
	} else if !errors.is_empty() {
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

	// info!("copied {} files", completed_files.load(Ordering::Relaxed));
	options
		.pb
		.finish(&options.multibar, Some(format!("copied {} files successfully", completed_files.load(Ordering::Relaxed))));

	return Ok(());
}

fn copy_inner(src: &Path, dest: &Path, file_size: u64, completed_files: &Arc<AtomicUsize>, total_files: u64, options: &Options) -> Result<()> {
	if options.abort.load(Ordering::Relaxed) {
		return Ok(());
	}

	info!("{} -> {}", src.display(), dest.display());

	if options.dry_run {
		options.pb.inc(file_size);

		let completed = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
		options
			.pb
			.set_message(format!("copying: {}/{} files", completed, total_files));

		return Ok(());
	}

	if options.reflink != ReflinkMode::Never && reflink(src, dest, file_size, completed_files, total_files, options)? {
		return Ok(());
	}

	#[cfg(target_os = "linux")]
	{
		if !kernel_copy(src, dest, file_size, options).unwrap_or(false) {
			debug!("copy_file_range failed... retrying with userspace copy...");
			copy_core(src, dest, file_size, options)?;
		}
	}

	#[cfg(not(target_os = "linux"))]
	copy_core(src, dest, file_size, options)?;

	if options.archive {
		copy_attributes(src, dest).src("could not copy file attributes", src)?;
	}

	if options.verify {
		let res = checksum::is_same_file(src, dest)?;
		if !res.is_same {
			warn!("{} was corrupted during copy", dest.display());
			debug!("checksums src: {}, dest: {}", res.src, res.dest);

			info!("deleted {}", dest.display());
			std::fs::remove_file(dest).src("could not delete", dest)?;

			return Err(eyre!("{} -> {}: was corrupted on copy (checksum check failed)", src.display(), dest.display()));
		}
	}

	let completed = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
	options
		.pb
		.set_message(format!("copying: {}/{} files", completed, total_files));

	return Ok(());
}

#[cfg(target_os = "linux")]
fn kernel_copy(src: &Path, dest: &Path, file_size: u64, options: &Options) -> Result<bool> {
	let src_file = File::open(src).src("could not open file read-only", src)?;
	let dest_file = match File::create_new(dest).src("could not open file write-only", dest) {
		Ok(o) => o,
		Err(e) if options.force => {
			info!("(from --force) deleting {}", dest.display());
			std::fs::remove_file(dest).src("could not delete file after open fail", dest)?;
			File::create_new(dest).src("could not open file write-only", dest)?
		}
		Err(e) => return Err(e),
	};

	const TARGET_UPDATES: u64 = 128;
	const MIN_CHUNK: usize = 4 * 1024 * 1024;
	let chunk_size = std::cmp::max(MIN_CHUNK, (file_size / TARGET_UPDATES) as usize);
	let mut total_copied = 0_u64;

	loop {
		use nix::fcntl::copy_file_range;

		if options.abort.load(Ordering::Relaxed) {
			drop(dest_file);

			if !options.pb.is_finished() {
				options.pb.finish(&options.multibar, None);
			}

			if let Err(e) = std::fs::remove_file(dest).src("could not remove incomplete file", dest) {
				print_error!(e, options.verbose);
			}

			info!("cleaned up incomplete file: `{}`", dest.display());

			return Ok(true);
		}

		let to_copy = chunk_size.min((file_size - total_copied) as usize);
		if to_copy == 0 {
			break;
		}

		match copy_file_range(&src_file, None, &dest_file, None, to_copy) {
			Ok(0) => break,
			Ok(copied) => {
				total_copied += copied as u64;
				options.pb.inc(copied as u64);
			}
			Err(_) => {
				return Ok(false);
			}
		}
	}

	return Ok(true);
}

fn copy_core(src: &Path, dest: &Path, file_size: u64, options: &Options) -> Result<()> {
	let mut src_file = File::open(src).src("could not open file read-only", src)?;
	let dest_file = match File::create_new(dest).src("could not open file write-only", dest) {
		Ok(o) => o,
		Err(e) if options.force => {
			info!("(from --force) deleting {}", dest.display());
			std::fs::remove_file(dest).src("could not delete file after open fail", dest)?;
			File::create_new(dest).src("could not open file write-only", dest)?
		}
		Err(e) => return Err(e),
	};

	let mut accumulated_bytes = 0_u64;

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

#[cfg(test)]
mod tests {
	use std::sync::atomic::AtomicBool;

	use crate::{
		args::Args,
		index::index,
		util::{exclude::ExcludeRules, progress::ProgressBar},
	};

	use super::*;

	use indicatif::MultiProgress;
	use tempfile::TempDir;

	fn create_file(path: &Path) {
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).unwrap();
		}
		std::fs::write(path, "test\n").unwrap();
	}

	#[test]
	fn test_copy_single() {
		let args = Args::default();
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a.txt");
		let b = temp.path().join("b.txt");

		create_file(&a);

		let mut options = Options::new(
			&args,
			&b,
			ExcludeRules::default(),
			MultiProgress::new(),
			ProgressBar::new_dummy(),
			Arc::new(AtomicBool::new(false)),
		);

		let index = index(&[a], b.clone(), &mut options);
		copy(&index, &options).unwrap();

		assert_eq!(std::fs::read_to_string(&b).unwrap(), "test\n");
	}

	#[test]
	fn test_copy_multi() {
		let args = Args {
			src: vec![String::new(), String::new()],
			..Default::default()
		};
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a.txt");
		let b = temp.path().join("b.txt");
		let c = temp.path().join("c");

		create_file(&a);
		create_file(&b);
		std::fs::create_dir_all(&c).unwrap();

		let mut options = Options::new(
			&args,
			&b,
			ExcludeRules::default(),
			MultiProgress::new(),
			ProgressBar::new_dummy(),
			Arc::new(AtomicBool::new(false)),
		);

		let index = index(&[a, b], c.clone(), &mut options);
		copy(&index, &options).unwrap();

		assert_eq!(std::fs::read_to_string(c.join("a.txt")).unwrap(), "test\n");
		assert_eq!(std::fs::read_to_string(c.join("b.txt")).unwrap(), "test\n");
	}

	#[test]
	fn test_copy_recursive() {
		let args = Args {
			src: vec![String::new()],
			recursive: true,
			..Default::default()
		};
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a");
		let b = a.join("b.txt");
		let c = a.join("c.txt");
		let d = temp.path().join("d");

		std::fs::create_dir_all(&a).unwrap();
		create_file(&b);
		create_file(&c);
		std::fs::create_dir_all(&d).unwrap();

		let mut options = Options::new(
			&args,
			&b,
			ExcludeRules::default(),
			MultiProgress::new(),
			ProgressBar::new_dummy(),
			Arc::new(AtomicBool::new(false)),
		);

		let index = index(&[a], d.clone(), &mut options);
		copy(&index, &options).unwrap();

		assert_eq!(std::fs::read_to_string(d.join("b.txt")).unwrap(), "test\n");
		assert_eq!(std::fs::read_to_string(d.join("c.txt")).unwrap(), "test\n");
	}
}
