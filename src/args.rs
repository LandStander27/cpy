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
#[command(about = "cp but better (hopefully)", long_about = None)]
pub struct Args {
	#[arg(short, long, help = "display help", action = clap::builder::ArgAction::Help)]
	pub help: (),

	#[arg(long, help = "print version", action = clap::builder::ArgAction::Version)]
	pub version: (),

	#[arg(short, long, help = "increase verbosity (-v: info, -vv: debug, -vvv: trace, -vvvv: trace, more detailed errors)", action = clap::ArgAction::Count)]
	pub verbose: u8,

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

	#[arg(long, help = "copy files as CoW copies. see https://btrfs.readthedocs.io/en/latest/Reflink.html", default_value = "auto")]
	pub reflink: ReflinkMode,

	#[arg(help = "sources to copy", required = true)]
	pub src: Vec<String>,

	#[arg(help = "destination", required = true)]
	pub dest: String,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum ReflinkMode {
	Never,
	Always,
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
