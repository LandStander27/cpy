use clap::{Parser, ValueEnum};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

#[derive(Parser, Debug, Clone)]
#[command(name = "cpy", disable_help_flag = true, disable_version_flag = true, version = version::version)]
#[command(about = "cp but better (hopefully)\nrepository: https://codeberg.org/Land/cpy", long_about = None)]
pub struct Args {
	#[arg(short, long, help = "display help", action = clap::builder::ArgAction::Help)]
	pub help: (),

	#[arg(long, help = "print version", action = clap::builder::ArgAction::Version)]
	pub version: (),

	#[arg(short, long, help = "increase verbosity, which can slow down copying significantly if --quiet is not supplied (-v: info, -vv: debug, -vvv: trace, -vvvv: trace, more detailed errors)", action = clap::ArgAction::Count)]
	pub verbose: u8,

	#[arg(short, long, help = "hide progress bar (recommended with --verbose)")]
	pub quiet: bool,

	#[arg(short = 'c', long, help = "perform checksum verification (not applicable to --reflink)")]
	pub verify: bool,

	#[arg(long, help = "do not perform any copy operations")]
	pub dry_run: bool,

	#[arg(short, visible_short_alias = 'R', long, help = "copy directories recursively")]
	pub recursive: bool,

	#[arg(short, long, help = "preserves all file attributes")]
	pub archive: bool,

	#[arg(short = 'j', long, help = "threads to use for copying", value_parser = over_0, default_value_t = 4)]
	pub threads: usize,

	#[arg(short, long, help = "if an existing destination file cannot be created, remove it and try again")]
	pub force: bool,

	#[arg(short, long, help = "ignore files with destinations that already exist")]
	pub update: bool,

	#[arg(short, long, help = "exclude files with an absolute file path matching REGEX", value_name = "REGEX")]
	pub exclude: Vec<String>,

	#[arg(
		long,
		help = "copy files as CoW copies. see https://btrfs.readthedocs.io/en/latest/Reflink.html",
		default_value = "auto",
		value_name = "MODE"
	)]
	pub reflink: ReflinkMode,

	#[arg(short = 'x', long, help = "stay on the same file system per SOURCE")]
	pub one_file_system: bool,

	#[arg(help = "sources to copy", required = true)]
	pub src: Vec<String>,

	#[arg(help = "destination", required = true)]
	pub dest: String,

	#[cfg(feature = "generators")]
	#[arg(long = "generate-man", help = "do not use")]
	pub generate_man: String,

	#[cfg(feature = "generators")]
	#[arg(value_enum, long = "generate-shell", help = "do not use")]
	pub generate_shell: clap_complete::Shell,
}

impl Default for Args {
	fn default() -> Self {
		#[cfg(not(feature = "generators"))]
		return Self {
			archive: false,
			dest: "".to_string(),
			force: false,
			help: (),
			version: (),
			recursive: false,
			reflink: ReflinkMode::default(),
			src: Vec::new(),
			exclude: Vec::new(),
			one_file_system: false,
			quiet: false,
			threads: 4,
			update: false,
			verbose: 0,
			dry_run: false,
			verify: false,
		};

		#[cfg(feature = "generators")]
		unreachable!();
	}
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Default)]
pub enum ReflinkMode {
	Never,
	Always,

	#[default]
	Auto,
}

fn over_0(s: &str) -> Result<usize, String> {
	let num: usize = s
		.parse()
		.map_err(|_| format!("`{s}` is not a valid usize"))?;
	if num > 0 {
		Ok(num)
	} else {
		Err("--threads must be >0".to_string())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_over_0() {
		assert_eq!(over_0("0"), Err("--threads must be >0".to_string()));
		assert_eq!(over_0("1"), Ok(1));
		assert_eq!(over_0("-1"), Err("`-1` is not a valid usize".to_string()));
		assert_eq!(over_0("awd"), Err("`awd` is not a valid usize".to_string()));
	}
}
