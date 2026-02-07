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
