use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

use signal_hook::consts::signal::*;
use signal_hook::iterator::Signals;

#[allow(unused)]
use {
	color_eyre::{
		Result,
		eyre::{Context as EyreContext, eyre},
	},
	log::{debug, error, info, trace, warn},
};

pub fn signal_handler() -> Result<Arc<AtomicBool>> {
	let abort = Arc::new(AtomicBool::new(false));
	let mut signals = Signals::new([SIGINT, SIGTERM]).with_context(|| "could not setup signal handler")?;
	std::thread::spawn({
		let abort = abort.clone();
		move || {
			for sig in signals.forever() {
				match sig {
					SIGINT | SIGTERM => {
						abort.store(true, Ordering::Relaxed);
					}
					_ => unreachable!(),
				}
			}
		}
	});

	return Ok(abort);
}
