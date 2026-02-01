use std::{os::unix::fs::MetadataExt, path::PathBuf};

use color_eyre::eyre::ContextCompat;
use jwalk::WalkDir;

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

use crate::*;

#[derive(Debug)]
pub struct Index {
	pub tasks: Vec<Task>,
	pub total_size: u64,
	pub total_files: u64,
}

impl Default for Index {
	fn default() -> Self {
		return Self {
			total_files: 0,
			total_size: 0,
			tasks: Vec::with_capacity(100000),
		};
	}
}

impl Index {
	pub fn add_task(&mut self, task: impl Into<Task>) {
		self.tasks.push(task.into());
	}

	// fn add_file(&mut self, task: FileTask) {
	// 	self.tasks.push(Task::File(task));
	// }

	// fn add_directory(&mut self, task: DirTask) {
	// 	self.tasks.push(Task::Dir(task));
	// }
}

#[derive(Debug)]
pub struct FileTask {
	pub src: PathBuf,
	pub dest: PathBuf,
	pub size: u64,
}

#[derive(Debug)]
pub struct DirTask {
	pub src: PathBuf,
	pub dest: PathBuf,
}

#[derive(Debug)]
pub enum Task {
	File(FileTask),
	Dir(DirTask),
}

impl From<FileTask> for Task {
	fn from(f: FileTask) -> Self {
		return Task::File(f);
	}
}

impl From<DirTask> for Task {
	fn from(d: DirTask) -> Self {
		return Task::Dir(d);
	}
}

struct DebounceTicker<'a> {
	ticker: &'a ProgressBar,
	last_update: std::time::Instant,
}

impl<'a> DebounceTicker<'a> {
	pub fn should_change(&self) -> bool {
		return self.last_update.elapsed().as_millis() >= 750;
	}

	pub fn change(&mut self, s: impl Into<std::borrow::Cow<'static, str>>) {
		self.ticker.set_message(s);
		self.last_update = std::time::Instant::now();
	}
}

pub fn index(src: &[&Path], dest: PathBuf, options: &Options) -> Index {
	let mut index = Index::default();
	let mut debounce = DebounceTicker {
		ticker: &options.pb,
		last_update: std::time::Instant::now(),
	};

	for src in src {
		if debounce.should_change() {
			debounce.change(src.display().to_string());
		}

		if let Err(e) = index_entry(src, &dest, &mut index, options, &mut debounce) {
			print_error!(e, options.verbose);
		}

		if options.abort.load(Ordering::Relaxed) {
			info!("operation aborted");
			break;
		}
	}

	return index;
}

fn index_entry(src: &Path, dest: &Path, index: &mut Index, options: &Options, pb: &mut DebounceTicker) -> Result<()> {
	if src.is_dir() {
		index_directory(src, dest, index, options, pb)?;
	} else {
		index_file(src, dest, index, options, pb)?;
	}

	return Ok(());
}

fn index_file(src: &Path, dest: &Path, index: &mut Index, options: &Options, pb: &mut DebounceTicker) -> Result<()> {
	if pb.should_change() {
		pb.change(src.display().to_string());
	}

	let metadata = src
		.metadata()
		.with_context(add_err("could not get file metadata", src))?;

	let dest_path = if options.dest_is_dir {
		dest.join(
			src.file_name()
				.with_context(add_err("could not get filename", src))?,
		)
	} else {
		dest.to_path_buf()
	};

	index.add_task(Task::File(FileTask {
		src: src.to_path_buf(),
		dest: dest_path,
		size: metadata.size(),
	}));
	index.total_files += 1;
	index.total_size += metadata.size();

	return Ok(());
}

fn index_directory(src: &Path, dest: &Path, index: &mut Index, options: &Options, pb: &mut DebounceTicker) -> Result<()> {
	let threads = std::thread::available_parallelism().wrap_err_with(|| "could not get num_threads")?;

	let root_dest = dest.join(
		src.file_name()
			.with_context(add_err("could not get filename", src))?,
	);

	index.add_task(Task::Dir(DirTask {
		src: src.to_path_buf(),
		dest: root_dest.clone(),
	}));

	if !options.recursive {
		return Ok(());
	}

	for entry in WalkDir::new(src)
		.skip_hidden(false)
		.parallelism(jwalk::Parallelism::RayonNewPool(threads.get()))
		.follow_links(false)
	{
		if options.abort.load(Ordering::Relaxed) {
			info!("operation aborted");
			break;
		}

		let entry = match entry.with_context(add_err("could not read directory", src)) {
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
			.with_context(add_err("could not get relative path", &abs))
		{
			Ok(o) => o,
			Err(e) => {
				print_error!(e, options.verbose);
				continue;
			}
		};

		let dest_path = root_dest.join(relative);
		let metadata = match entry
			.metadata()
			.with_context(add_err("could not stat directory", &abs))
		{
			Ok(o) => o,
			Err(e) => {
				print_error!(e, options.verbose);
				continue;
			}
		};
		if metadata.is_dir() {
			index.add_task(Task::Dir(DirTask { src: abs, dest: dest_path }));
		} else if let Err(e) = index_file(&abs, &dest_path, index, options, pb) {
			print_error!(e, options.verbose);
			continue;
		}
	}

	return Ok(());
}
