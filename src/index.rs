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
	pub symlinks: Vec<SymlinkTask>,
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
			symlinks: Vec::new(),
			dirs: Vec::new(),
		};
	}
}

impl Index {
	fn add_file(&mut self, task: FileTask) {
		self.files.push(task);
	}

	fn add_symlink(&mut self, task: SymlinkTask) {
		self.symlinks.push(task);
	}

	fn add_directory(&mut self, task: DirTask) {
		self.dirs.push(task);
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileTask {
	pub src: PathBuf,
	pub dest: PathBuf,
	pub size: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymlinkTask {
	pub target: PathBuf,
	pub dest: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
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
			// info!("operation aborted");
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

	if options.exclude_rules.matches(src) {
		return Ok(());
	}

	let metadata = if src.is_symlink() {
		std::fs::symlink_metadata(src).src("could not get file metadata", src)?
	} else {
		src.metadata().src("could not get file metadata", src)?
	};

	let dest_path = if is_top_level && options.dest_is_dir {
		dest.join(src.file_name().src("could not get filename", src)?)
	} else {
		dest.to_path_buf()
	};

	if options.update && dest_path.exists() {
		return Ok(());
	}

	if metadata.is_symlink() {
		let target = std::fs::read_link(src).src("could not read symbolic link", src)?;
		index.add_symlink(SymlinkTask { target, dest: dest_path });

		index.total_files += 1;
		return Ok(());
	}

	index.add_file(FileTask {
		src: src.to_path_buf(),
		dest: dest_path,
		size: metadata.size(),
	});
	index.total_size += metadata.size();
	index.total_files += 1;

	return Ok(());
}

fn index_directory(src: &Path, dest: &Path, index: &mut Index, options: &mut Options) -> Result<()> {
	if options.exclude_rules.matches(src) {
		return Ok(());
	}

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

#[cfg(test)]
mod tests {
	use std::sync::{Arc, atomic::AtomicBool};

	use super::*;
	use crate::{args::Args, util::progress::ProgressBar};
	use indicatif::MultiProgress;
	use tempfile::TempDir;

	fn create_file(path: &Path) {
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).unwrap();
		}
		std::fs::write(path, "test\n").unwrap();
	}

	fn create_symlink(dest: &Path, target: &Path) {
		if let Some(parent) = dest.parent() {
			std::fs::create_dir_all(parent).unwrap();
		}
		std::os::unix::fs::symlink(target, dest).unwrap();
	}

	#[test]
	fn test_indexing() {
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a.txt");
		let b = temp.path().join("b.txt");
		let c = temp.path().join("c");
		let a2 = temp.path().join("c/a.txt");
		let d = temp.path().join("c/d");
		let b2 = temp.path().join("c/d/b.txt");
		let dest = temp.path().join("dest");

		create_file(&a);
		create_file(&b);
		std::fs::create_dir_all(&c).unwrap();
		create_file(&a2);
		std::fs::create_dir_all(&d).unwrap();
		create_file(&b2);
		std::fs::create_dir_all(&dest).unwrap();

		let multibar = MultiProgress::new();
		let args = Args {
			recursive: false,
			..Default::default()
		};

		let mut options = Options::new(&args, &dest, Default::default(), multibar, ProgressBar::new_dummy(), Arc::new(AtomicBool::new(false)));

		let index = index(&[a, b, c], dest, &mut options);

		assert_eq!(index.files.len(), 2);
		assert_eq!(index.dirs.len(), 1);
		assert_eq!(index.total_files, 2);
	}

	#[test]
	fn test_symlink_indexing() {
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a.txt");
		let b = temp.path().join("b.txt");
		let c = temp.path().join("c");
		let a2 = temp.path().join("c/a.txt");
		let dest = temp.path().join("dest");

		create_symlink(&a, &a2);
		create_file(&b);
		std::fs::create_dir_all(&c).unwrap();
		create_file(&a2);

		let multibar = MultiProgress::new();
		let args = Args {
			recursive: true,
			..Default::default()
		};

		let mut options = Options::new(&args, &dest, Default::default(), multibar, ProgressBar::new_dummy(), Arc::new(AtomicBool::new(false)));

		let index = index(&[a, b, c], dest.clone(), &mut options);

		assert_eq!(index.files.len(), 2);
		assert_eq!(index.symlinks.len(), 1);
		assert_eq!(index.dirs.len(), 1);
		assert_eq!(index.total_files, 3);

		assert_eq!(
			index.symlinks.first().unwrap(),
			&SymlinkTask {
				dest: dest.join("a.txt"),
				target: a2,
			}
		);
	}

	#[test]
	fn test_recursive_indexing() {
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a.txt");
		let b = temp.path().join("b.txt");
		let c = temp.path().join("c");
		let a2 = temp.path().join("c/a.txt");
		let d = temp.path().join("c/d");
		let b2 = temp.path().join("c/d/b.txt");
		let dest = temp.path().join("dest");

		create_file(&a);
		create_file(&b);
		std::fs::create_dir_all(&c).unwrap();
		create_file(&a2);
		std::fs::create_dir_all(&d).unwrap();
		create_file(&b2);
		std::fs::create_dir_all(&dest).unwrap();

		let multibar = MultiProgress::new();
		let args = Args {
			recursive: true,
			..Default::default()
		};

		let mut options = Options::new(&args, &dest, Default::default(), multibar, ProgressBar::new_dummy(), Arc::new(AtomicBool::new(false)));

		let index = index(&[a, b, c], dest.clone(), &mut options);

		assert_eq!(index.files.len(), 4);
		assert_eq!(index.dirs.len(), 2);
		assert_eq!(index.total_files, 4);

		assert!(index.files.contains(&FileTask {
			src: b2,
			dest: dest.join("c/d/b.txt"),
			size: 5
		}));
	}
}
