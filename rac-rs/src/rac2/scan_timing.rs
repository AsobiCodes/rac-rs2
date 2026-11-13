//! RAM-scan timers (lane stut3): with `RAC2_SCANLOG=1` every wrapped scan that takes 0.5 ms or more is printed to stderr
//! (`rac2: scan NAME ms`). Off: one relaxed load per call.
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

static ON: AtomicU8 = AtomicU8::new(0);

fn on() -> bool {
	match ON.load(Ordering::Relaxed) {
		0 => {
			let v = if std::env::var_os("RAC2_SCANLOG").is_some() { 2 } else { 1 };
			ON.store(v, Ordering::Relaxed);
			v == 2
		}
		v => v == 2,
	}
}

/// Runs `f`; with RAC2_SCANLOG set, prints its duration when it is 0.5 ms or more.
pub fn timed<R>(name: &str, f: impl FnOnce() -> R) -> R {
	if !on() {
		return f();
	}
	let t = Instant::now();
	let r = f();
	let ms = t.elapsed().as_secs_f64() * 1e3;
	if ms >= 0.5 {
		eprintln!("rac2: scan {name} {ms:.2} ms");
	}
	r
}

/// Page faults of this process so far (Windows; 0 elsewhere): a field that faults in thousands of cold exe pages is slow with no code to blame.
#[cfg(windows)]
fn page_faults() -> u32 {
	#[repr(C)]
	struct Pmc {
		cb: u32,
		faults: u32,
		rest: [usize; 8],
	}
	unsafe extern "system" {
		fn K32GetProcessMemoryInfo(h: isize, p: *mut Pmc, cb: u32) -> i32;
		fn GetCurrentProcess() -> isize;
	}
	let mut p = Pmc { cb: std::mem::size_of::<Pmc>() as u32, faults: 0, rest: [0; 8] };
	// Safety: plain Win32 query into a correctly sized struct.
	unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut p, p.cb) };
	p.faults
}
#[cfg(not(windows))]
fn page_faults() -> u32 {
	0
}

static LAST_IC: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static LAST_FAULTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// With RAC2_SCANLOG set: a field whose `Machine::frame` took 12 ms or more is printed (`rac2: slow field N ms, F page faults`).
pub fn field(field: u64, d: std::time::Duration, icount: u64) {
	if !on() {
		return;
	}
	let f = page_faults();
	let df = f.wrapping_sub(LAST_FAULTS.swap(f, Ordering::Relaxed));
	let di = icount.wrapping_sub(LAST_IC.swap(icount, Ordering::Relaxed));
	if d.as_secs_f64() >= 0.012 {
		eprintln!("rac2: slow field {field} {:.1} ms, {df} page faults, {di} insns", d.as_secs_f64() * 1e3);
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn timed_returns_the_closure_result() {
		assert_eq!(super::timed("t", || 7), 7);
	}
}
