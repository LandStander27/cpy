use std::{
	fs::{Metadata, Permissions},
	os::unix::fs::{MetadataExt, PermissionsExt},
	path::Path,
};

use filetime::{FileTime, set_file_mtime};
use nix::libc;

use crate::util::log::add_err;

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

pub fn copy_attributes(src: &Path, dest: &Path) -> Result<()> {
	let metadata = src
		.metadata()
		.with_context(add_err("could not stat file", src))?;

	copy_mtime(&metadata, src, dest)?;
	copy_ownership(&metadata, dest)?;
	copy_xattr(src, dest)?;

	return Ok(());
}

fn copy_mtime(metadata: &Metadata, src: &Path, dest: &Path) -> Result<()> {
	let modified_time = metadata
		.modified()
		.with_context(add_err("could not get mtime", src))?;
	let system_modified_time = FileTime::from_system_time(modified_time);
	set_file_mtime(dest, system_modified_time).with_context(add_err("could not set mtime", dest))?;

	return Ok(());
}

fn copy_ownership(metadata: &Metadata, dest: &Path) -> Result<()> {
	let mode = metadata.permissions().mode();
	let permissions = Permissions::from_mode(mode);
	std::fs::set_permissions(dest, permissions).with_context(add_err("could not set permissions", dest))?;

	let uid = metadata.uid();
	let gid = metadata.gid();

	// Note: This requires elevated privileges (root) to work in most cases
	// We'll attempt it but won't fail if it doesn't work
	let dest_cstring = std::ffi::CString::new(dest.to_string_lossy().as_bytes()).with_context(add_err("invalid string", dest))?;

	unsafe {
		let result = libc::chown(dest_cstring.as_ptr(), uid, gid);
		if result != 0 {
			let err = std::io::Error::last_os_error();
			// Only return error if it's not a permission issue
			// (EPERM = 1, EACCES = 13)
			if err.raw_os_error() != Some(1) && err.raw_os_error() != Some(13) {
				return Err(err).with_context(add_err("could not set ownership", dest))?;
			}
		}
	}

	return Ok(());
}

fn copy_xattr(src: &Path, dest: &Path) -> Result<()> {
	let xattrs = match xattr::list(src) {
		Ok(attrs) => attrs,
		Err(e) => {
			if e.kind() != std::io::ErrorKind::Unsupported {
				return Err(e).with_context(add_err("could not set xattr", dest))?;
			}

			return Ok(());
		}
	};

	for attr_name in xattrs {
		if let Some(value) = xattr::get(src, &attr_name)? {
			let _ = xattr::set(dest, &attr_name, &value);
		}
	}

	return Ok(());
}
