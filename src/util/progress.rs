use std::{borrow::Cow, time::Instant};

use indicatif::{MultiProgress, ProgressStyle};

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

#[derive(Clone)]
pub struct ProgressBar {
	pub pb: indicatif::ProgressBar,
	pub last_update: Instant,
}

impl ProgressBar {
	pub fn new_bar(multibar: &MultiProgress, total: u64, init_msg: Option<String>) -> Self {
		let ticker = multibar.add(
			indicatif::ProgressBar::new(total).with_style(
				ProgressStyle::with_template(
					"{msg:.bold} {wide_bar:.blue} {percent:>3}% • {binary_bytes}/{binary_total_bytes} • {binary_bytes_per_sec} • Elapsed: {elapsed_precise} • ETA:{eta_precise}",
				)
				.unwrap()
				.progress_chars("█▓▒░  "),
			),
		);

		if let Some(init_msg) = init_msg {
			ticker.set_message(init_msg);
		}

		return Self {
			pb: ticker,
			last_update: Instant::now(),
		};
	}

	pub fn new_ticker(multibar: &MultiProgress, desc: impl Into<Cow<'static, str>>, init_msg: Option<String>) -> Self {
		let ticker = multibar.add(indicatif::ProgressBar::new_spinner().with_style(ProgressStyle::with_template("{prefix:.bold.dim} {spinner:.blue} {msg}").unwrap()));
		ticker.enable_steady_tick(std::time::Duration::from_millis(250));
		ticker.set_prefix(desc);

		if let Some(init_msg) = init_msg {
			ticker.set_message(init_msg);
		}

		return Self {
			pb: ticker,
			last_update: Instant::now(),
		};
	}

	#[inline]
	pub fn inc(&self, len: u64) {
		self.pb.inc(len);
	}

	#[inline]
	pub fn set_message(&self, s: String) {
		self.pb.set_message(s);
	}

	#[inline]
	pub fn debounce_set_message(&mut self, f: impl Fn() -> String) {
		if self.last_update.elapsed().as_millis() >= 750 {
			self.pb.set_message(f());
			self.last_update = Instant::now();
		}
	}

	#[inline]
	pub fn is_finished(&self) -> bool {
		return self.pb.is_finished();
	}

	#[inline]
	pub fn finish(&self, multibar: &MultiProgress, msg: Option<String>) {
		self.pb.disable_steady_tick();
		if let Some(msg) = msg {
			self.pb.finish_with_message(msg);
		} else {
			self.pb.finish_and_clear();
		}
		// self.pb.tick();
		multibar.remove(&self.pb);
	}
}
