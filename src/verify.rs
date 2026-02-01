#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

use crate::*;

pub fn verify_sources(src: &Vec<PathBuf>, dest: &Path, args: &Args) -> Result<bool> {
	let mut stop = false;

	for src in src {
		if !src.exists() {
			error!("`{}` does not exist", src.display());
			stop = true
		}

		if src.is_dir() && !args.recursive {
			warn!("`{}` is a directory, but --recursive was not supplied", src.display());
		}
	}

	if src.len() > 1 && !dest.is_dir() {
		error!("multiple sources were supplied, but `{}` is not a directory", dest.display());
		stop = true;
	}

	return Ok(!stop);
}
