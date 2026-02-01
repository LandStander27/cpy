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

use crate::index::*;
use crate::*;

pub fn copy(index: &Index) -> Result<()> {
	return Ok(());
}
