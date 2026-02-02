use std::path::{Path, PathBuf};

use crate::{args::Args, index::DirTask, util::log::WrapErrExt};

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
		.map(|x| x.canonicalize().src("could not canonicalize path", x))
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
				std::fs::create_dir_all(&dir.dest).src("could not create directory", &dir.src)?;
			}
			Err(e) => return Err(e).src("could not create directory", &dir.src)?,
		}
	}

	return Ok(());
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_canonicalize_cwd() {
		let (src, _) = prepare_paths(&Args {
			src: vec![".".to_string()],
			dest: "/".to_string(),
			..Default::default()
		})
		.unwrap();

		assert_eq!(src.first(), Some(&std::env::current_dir().unwrap()));
	}
}
