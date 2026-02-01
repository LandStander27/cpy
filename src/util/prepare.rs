use std::path::{Path, PathBuf};

use crate::{args::Args, index::DirTask, util::log::add_err};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

pub fn prepare_paths(args: &Args) -> Result<(Vec<PathBuf>, PathBuf)> {
	let src = args
		.src
		.iter()
		.map(Path::new)
		.map(|x| {
			x.canonicalize()
				.with_context(add_err("could not canonicalize path", x))
		})
		.collect::<Result<Vec<PathBuf>>>()?;

	let dest = PathBuf::from(&args.dest);

	return Ok((src, dest));
}

pub fn create_directories(dirs: &[DirTask]) -> Result<()> {
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
