use std::{fs::File, io::Read, path::Path};

use xxhash_rust::xxh3::Xxh3;

use crate::util::log::WrapErrExt;

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

pub struct CheckResults {
	pub is_same: bool,
	pub src: u64,
	pub dest: u64,
}

pub fn is_same_file(src: &Path, dest: &Path) -> Result<CheckResults> {
	let mut src_file = File::open(src).src("could not open file", src)?;
	let mut dest_file = File::open(dest).src("could not open file", dest)?;

	let mut hasher = Xxh3::new();
	let mut buffer = vec![0u8; 128 * 1024];
	loop {
		let bytes_read = src_file.read(&mut buffer)?;
		if bytes_read == 0 {
			break;
		}
		hasher.update(&buffer[..bytes_read]);
	}

	let src_hash = hasher.digest();

	hasher.reset();
	loop {
		let bytes_read = dest_file.read(&mut buffer)?;
		if bytes_read == 0 {
			break;
		}
		hasher.update(&buffer[..bytes_read]);
	}

	let dest_hash = hasher.digest();

	return Ok(CheckResults {
		is_same: src_hash == dest_hash,
		src: src_hash,
		dest: dest_hash,
	});
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn create_file(path: &Path, contents: &str) {
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent).unwrap();
		}
		std::fs::write(path, contents).unwrap();
	}

	#[test]
	fn test_verify_same() {
		let temp = TempDir::new().unwrap();
		let src = temp.path().join("a.txt");
		let dest = temp.path().join("b.txt");

		create_file(&src, "test\n");
		create_file(&dest, "test\n");

		let res = is_same_file(&src, &dest).unwrap();
		assert!(res.is_same);
		assert_eq!(res.src, res.dest);
		assert_eq!(res.src, 7147276431135252565);
		assert_eq!(res.dest, 7147276431135252565);
	}

	#[test]
	fn test_verify_different() {
		let temp = TempDir::new().unwrap();
		let src = temp.path().join("a.txt");
		let dest = temp.path().join("b.txt");

		create_file(&src, "test\n");
		create_file(&dest, "another test\n");

		let res = is_same_file(&src, &dest).unwrap();
		assert!(!res.is_same);
		assert_ne!(res.src, res.dest);
		assert_eq!(res.src, 7147276431135252565);
		assert_eq!(res.dest, 11589011247844422973);
	}
}
