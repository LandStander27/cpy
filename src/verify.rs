use std::path::{Path, PathBuf};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

use crate::args::Args;

pub fn verify_sources(src: &[PathBuf], dest: &Path, args: &Args) -> bool {
	let mut stop = false;

	for src in src {
		if !src.exists() {
			error!("`{}` does not exist", src.display());
			stop = true
		}

		trace!("{}, {}", src.display(), dest.display());
		if src == dest {
			error!("`{}` and `{}` are the same {}", src.display(), dest.display(), if src.is_dir() { "folder" } else { "file" });
			stop = true;
		}

		if src.is_dir() && !args.recursive {
			warn!("`{}` is a directory, but --recursive was not supplied", src.display());
		}
	}

	if src.len() > 1 && (dest.exists() && !dest.is_dir()) {
		error!("multiple sources were supplied, but `{}` is not a directory", dest.display());
		stop = true;
	}

	return !stop;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::args::Args;
	use tempfile::TempDir;

	fn create_file(path: &Path) {
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).unwrap();
		}
		std::fs::write(path, "test\n").unwrap();
	}

	#[test]
	fn test_verify_multiple_to_file() {
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a.txt");
		let b = temp.path().join("b.txt");
		let c = temp.path().join("c.txt");
		create_file(&a);
		create_file(&b);
		create_file(&c);

		assert!(!verify_sources(&[a.clone(), b.clone()], &c, &Args::default()));
	}

	#[test]
	fn test_verify_correctly() {
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a.txt");
		let b = temp.path().join("b.txt");
		let c = temp.path().join("c.txt");
		let d = temp.path().join("d.txt");
		let e = temp.path().join("e");
		create_file(&a);
		create_file(&b);
		create_file(&c);
		create_file(&d);

		assert!(verify_sources(&[a, b, c, d], &e, &Args::default()));
	}

	#[test]
	fn test_verify_dne() {
		let temp = TempDir::new().unwrap();
		let c = temp.path().join("c.txt");
		let d = temp.path().join("d.txt");
		create_file(&c);

		assert!(!verify_sources(&[d], &c, &Args::default()));
	}

	#[test]
	fn test_verify_same_file() {
		let temp = TempDir::new().unwrap();
		let a = temp.path().join("a.txt");
		let b = temp.path().join("a.txt");
		create_file(&a);

		assert!(!verify_sources(&[a], &b, &Args::default()));
	}
}
