//! [debug] Replay tools for the field hunt (lane hang, 2026-10-31): the EE RAM of chosen fields on disk, and the field number for
//! the oracle's thread log.
//!
//! `RAC2_RAMDUMP=f1,f2,..` writes `ramNNNNN.bin` (32 MB, the RAM after the Nth field has run, N counted from 1 as the
//! `ramhash field N` lines) into `RAC2_RAMDUMP_DIR` (default: the current folder). `set_field` tells the oracle's kernel which
//! field the game is in (`ORACLE_THREADLOG=1` prints it with each thread / semaphore syscall).
use ee_oracle::Machine;
use std::sync::OnceLock;

fn fields() -> &'static Vec<u64> {
	static F: OnceLock<Vec<u64>> = OnceLock::new();
	F.get_or_init(|| std::env::var("RAC2_RAMDUMP").map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default())
}

/// Before a field runs: `field` = fields done so far (the oracle's thread log prints it).
pub fn set_field(field: u64) {
	ee_oracle::cold::FIELD.store(field, std::sync::atomic::Ordering::Relaxed);
}

/// After a field ran: `done` = fields done (1 after the first). Writes the dump when it is one of RAC2_RAMDUMP's.
pub fn after_field(m: &Machine, done: u64) {
	if !fields().contains(&done) {
		return;
	}
	let dir = std::env::var("RAC2_RAMDUMP_DIR").unwrap_or_else(|_| ".".into());
	let path = format!("{dir}/ram{done:05}.bin");
	match std::fs::write(&path, &m.cpu.mem.ram[..32 << 20]) {
		Ok(()) => eprintln!("ramdump: {path}"),
		Err(e) => eprintln!("ramdump: {path}: {e}"),
	}
}

/// After a "spin" error (the game thread never reached its vblank wait): the loop's pc samples, registers and the stack's code
/// words on stderr, and the RAM (`hang.bin` in RAC2_RAMDUMP_DIR when that is set). The game thread keeps spinning, so the reads race.
pub fn hang_report(m: &Machine) {
	let mut pcs = std::collections::BTreeMap::<u32, u32>::new();
	for _ in 0..20000 {
		*pcs.entry(m.cpu.pc).or_default() += 1;
		std::hint::spin_loop();
	}
	eprintln!("hang: pc samples {:x?}", pcs);
	let g = |i: usize| m.cpu.g(i) as u32;
	eprintln!("hang: ra {:08x} sp {:08x} a0-a3 {:08x} {:08x} {:08x} {:08x} s0-s3 {:08x} {:08x} {:08x} {:08x} v0 {:08x} v1 {:08x}", g(31), g(29), g(4), g(5), g(6), g(7), g(16), g(17), g(18), g(19), g(2), g(3));
	let sp = g(29);
	let words: Vec<String> = (0..256u32).map(|i| { let a = ((sp + i * 4) & 0x01ff_fffc) as usize; u32::from_le_bytes(m.cpu.mem.ram[a..a + 4].try_into().unwrap()) }).filter(|v| (0x100000..0x500000).contains(v) && v % 4 == 0).map(|v| format!("{v:x}")).collect();
	eprintln!("hang: stack code words: {}", words.join(" "));
	if let Ok(dir) = std::env::var("RAC2_RAMDUMP_DIR") {
		let _ = std::fs::write(format!("{dir}/hang.bin"), &m.cpu.mem.ram[..32 << 20]);
	}
}
