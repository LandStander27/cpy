use std::{
	fs::{File, Metadata, Permissions},
	os::unix::fs::{MetadataExt, PermissionsExt},
	sync::atomic::AtomicUsize,
};

use filetime::{FileTime, set_file_mtime};
use nix::{fcntl::copy_file_range, libc};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

use crate::index::*;
use crate::*;

pub fn copy(index: &Index, options: &Options) -> Result<()> {
	create_directories(&index.dirs)?;

	let completed_files = Arc::new(AtomicUsize::new(0));

	let threads = std::thread::available_parallelism().with_context(|| "could not get num_threads")?;
	let pool = rayon::ThreadPoolBuilder::new()
		.num_threads(threads.into())
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
				options.pb.finish_with_message("Aborted");
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

	copy_attributes(src, dest).with_context(add_err("could not copy file attributes", src))?;
	let completed = completed_files.fetch_add(1, Ordering::Relaxed) + 1;
	options
		.pb
		.set_message(format!("Copying: {}/{} files", completed, total_files));

	return Ok(());
}

fn create_directories(dirs: &[DirTask]) -> Result<()> {
	let mut dirs: Vec<&DirTask> = dirs.iter().collect();
	dirs.sort_unstable_by_key(|d| d.dest.components().count());
	dirs.dedup_by_key(|d| &d.dest);

	for dir in &dirs {
		match std::fs::create_dir(&dir.dest) {
			Ok(()) => {}
			Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
				std::fs::create_dir_all(&dir.dest).with_context(add_err("could not create directory", &dir.src))?;
			}
			Err(e) => return Err(e).with_context(add_err("could not create directory", &dir.src))?,
		}
	}

	return Ok(());
}

fn copy_attributes(src: &Path, dest: &Path) -> Result<()> {
	let metadata = src
		.metadata()
		.with_context(add_err("could not stat file", src))?;

	copy_mtime(&metadata, src, dest)?;
	copy_ownership(&metadata, dest)?;
	copy_xattr(src, dest)?;

	return Ok(());
}

fn copy_mtime(metadata: &Metadata, src: &Path, dest: &Path) -> Result<()> {
	let modified_time = metadata
		.modified()
		.with_context(add_err("could not get mtime", src))?;
	let system_modified_time = FileTime::from_system_time(modified_time);
	set_file_mtime(dest, system_modified_time).with_context(add_err("could not set mtime", dest))?;

	return Ok(());
}

fn copy_ownership(metadata: &Metadata, dest: &Path) -> Result<()> {
	let mode = metadata.permissions().mode();
	let permissions = Permissions::from_mode(mode);
	std::fs::set_permissions(dest, permissions).with_context(add_err("could not set permissions", dest))?;

	let uid = metadata.uid();
	let gid = metadata.gid();

	// Note: This requires elevated privileges (root) to work in most cases
	// We'll attempt it but won't fail if it doesn't work
	let dest_cstring = std::ffi::CString::new(dest.to_string_lossy().as_bytes()).with_context(add_err("invalid string", dest))?;

	unsafe {
		let result = libc::chown(dest_cstring.as_ptr(), uid, gid);
		if result != 0 {
			let err = std::io::Error::last_os_error();
			// Only return error if it's not a permission issue
			// (EPERM = 1, EACCES = 13)
			if err.raw_os_error() != Some(1) && err.raw_os_error() != Some(13) {
				return Err(err).with_context(add_err("could not set ownership", dest))?;
			}
		}
	}

	return Ok(());
}

fn copy_xattr(src: &Path, dest: &Path) -> Result<()> {
	let xattrs = match xattr::list(src) {
		Ok(attrs) => attrs,
		Err(e) => {
			if e.kind() != std::io::ErrorKind::Unsupported {
				return Err(e).with_context(add_err("could not set xattr", dest))?;
			}

			return Ok(());
		}
	};

	for attr_name in xattrs {
		if let Some(value) = xattr::get(src, &attr_name)? {
			let _ = xattr::set(dest, &attr_name, &value);
		}
	}

	return Ok(());
}
