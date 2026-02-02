use std::{
	os::unix::fs::MetadataExt,
	path::{Path, PathBuf},
	sync::atomic::Ordering,
};

use jwalk::WalkDir;

use crate::{
	options::Options,
	print_error,
	util::log::{ContextCompatExt, WrapErrExt},
};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

#[derive(Debug)]
pub struct Index {
	pub files: Vec<FileTask>,
	pub dirs: Vec<DirTask>,
	pub total_size: u64,
	pub total_files: u64,
}

impl Default for Index {
	fn default() -> Self {
		return Self {
			total_files: 0,
			total_size: 0,
			files: Vec::new(),
			dirs: Vec::new(),
		};
	}
}

impl Index {
	fn add_file(&mut self, task: FileTask) {
		self.files.push(task);
	}

	fn add_directory(&mut self, task: DirTask) {
		self.dirs.push(task);
	}
}

#[derive(Debug, Clone)]
pub struct FileTask {
	pub src: PathBuf,
	pub dest: PathBuf,
	pub size: u64,
}

#[derive(Debug, Clone)]
pub struct DirTask {
	pub src: PathBuf,
	pub dest: PathBuf,
}

pub fn index(src: &[PathBuf], dest: PathBuf, options: &mut Options) -> Index {
	let mut index = Index::default();

	for src in src {
		options
			.pb
			.debounce_set_message(|| src.display().to_string());

		if let Err(e) = index_entry(src, &dest, &mut index, options) {
			print_error!(e, options.verbose);
		}

		if options.abort.load(Ordering::Relaxed) {
			info!("operation aborted");
			break;
		}
	}

	return index;
}

fn index_entry(src: &Path, dest: &Path, index: &mut Index, options: &mut Options) -> Result<()> {
	if src.is_dir() {
		index_directory(src, dest, index, options)?;
	} else {
		index_file(src, dest, index, options, true)?;
	}

	return Ok(());
}

fn index_file(src: &Path, dest: &Path, index: &mut Index, options: &mut Options, is_top_level: bool) -> Result<()> {
	options
		.pb
		.debounce_set_message(|| src.display().to_string());

	let metadata = src.metadata().src("could not get file metadata", src)?;

	let dest_path = if is_top_level && options.dest_is_dir {
		dest.join(src.file_name().src("could not get filename", src)?)
	} else {
		dest.to_path_buf()
	};

	index.add_file(FileTask {
		src: src.to_path_buf(),
		dest: dest_path,
		size: metadata.size(),
	});
	index.total_files += 1;
	index.total_size += metadata.size();

	return Ok(());
}

fn index_directory(src: &Path, dest: &Path, index: &mut Index, options: &mut Options) -> Result<()> {
	let root_dest = dest.join(src.file_name().src("could not get filename", src)?);

	index.add_directory(DirTask {
		src: src.to_path_buf(),
		dest: root_dest.clone(),
	});

	if !options.recursive {
		return Ok(());
	}

	for entry in WalkDir::new(src)
		.skip_hidden(false)
		.parallelism(jwalk::Parallelism::RayonNewPool(options.threads))
		.follow_links(false)
	{
		if options.abort.load(Ordering::Relaxed) {
			info!("operation aborted");
			break;
		}

		let entry = match entry.src("could not read directory", src) {
			Ok(o) => o,
			Err(e) => {
				print_error!(e, options.verbose);
				continue;
			}
		};
		let abs = entry.path();
		if abs == src {
			continue;
		}

		let relative = match abs
			.strip_prefix(src)
			.src("could not get relative path", &abs)
		{
			Ok(o) => o,
			Err(e) => {
				print_error!(e, options.verbose);
				continue;
			}
		};

		let dest_path = root_dest.join(relative);
		let metadata = match entry.metadata().src("could not stat directory", &abs) {
			Ok(o) => o,
			Err(e) => {
				print_error!(e, options.verbose);
				continue;
			}
		};
		if metadata.is_dir() {
			index.add_directory(DirTask { src: abs, dest: dest_path });
		} else if let Err(e) = index_file(&abs, &dest_path, index, options, false) {
			print_error!(e, options.verbose);
			continue;
		}
	}

	return Ok(());
}
