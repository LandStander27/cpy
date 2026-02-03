use std::path::Path;

use color_eyre::Section;
use regex::{Regex, RegexBuilder};

use crate::print_error;

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

#[derive(Default)]
pub struct ExcludeRules {
	rules: Vec<Regex>,
}

impl ExcludeRules {
	pub fn compile(rules: &[impl AsRef<str>], verbose: u8) -> Result<Self> {
		let mut ret = Vec::new();
		let mut errors = Vec::new();

		for rule in rules {
			let r = match RegexBuilder::new(rule.as_ref())
				.case_insensitive(true)
				.build()
			{
				Ok(o) => o,
				Err(e) => {
					errors.push(eyre!("{e}"));
					continue;
				}
			};

			ret.push(r);
		}

		for e in &errors {
			print_error!(e, verbose);
		}

		if !errors.is_empty() {
			return Err(eyre!("some regex(es) failed to compile").suggestion("check your syntax?"));
		}

		return Ok(ExcludeRules { rules: ret });
	}

	pub fn matches(&self, path: &Path) -> bool {
		return self
			.rules
			.iter()
			.any(|r| r.is_match(&path.display().to_string()));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_excluding() {
		let rules = ExcludeRules::compile(&["\\.png"], 0).unwrap();
		assert!(rules.matches(Path::new("/home/user/picture.png")));
	}
}
