//! [PC] Dev only: scripted keyboard, left stick, raw mouse motion and Controls-tab captures for hidden test runs (no real input can be
//! sent to a hidden window, and `--pad` bypasses the window's input path), `RAC2_INPUT_FEED` = `;`-separated `FROM-TO:what:args`
//! (game fields; `A:what` = one field): `key:NAME` holds a key (an options-file name, `kbm::names`: `KeyN`, `Space`, `MouseLeft`),
//! `xl:LX,LY` puts the XInput left stick at those raw counts (-32767..32767; it goes through the stick dead zone row like a real
//! pad), `mouse:DX,DY` is raw mouse motion per poll, `cap:NAME` gives a waiting Controls-tab capture that key (the same call a real
//! press makes), `pad` marks a pad-input pulse (hides the cursor). host.rs `poll_input` reads it; unset = nothing happens.
//! [PC] nav lane: `ui:X,Y` moves the window cursor to window pixel (X, Y) (one CursorMoved per field in range) and `click:left|right`
//! clicks there (Pressed + Released once, at the first field in range); host.rs feeds both through the real window-event path.
//! [b48 Y5] FIELD-LOCKED feeds: a kind with an `f` prefix (`fkey`, `fxl`, `fmouse`, `fui`, `fclick`, `fpad`, `fcap`) switches the run
//! to a lock-step input: the game thread (host/game.rs) names the field it is about to run (`Gate::ask`) and waits until the window
//! thread has answered that field with ONE `poll_input` (`Gate::answer`), so every entry is applied at exactly the field its range says,
//! whatever the display rate, and the pad the game reads in that field holds it. Without any `f` entry nothing changes (the entries
//! count display polls' view of the newest field, as before). In a locked run ALL entries (also the unprefixed ones) use the locked
//! field. `cap` ranges stay counts of polls (one poll per field in a locked run). RAC2_MOUSE_SCRIPT is read by the game thread per
//! field already (pc_input.rs, pc_menu_hover.rs): it needs no lock.
use rac_engine::input::kbm::{self, Input};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};

enum What {
	Key(Input),
	Stick(i16, i16),
	Mouse(f64, f64),
	Cap(Input),
	Pad,
	Ui(f64, f64),
	Click(MouseButton),
}

/// (from, to, what, locked): `locked` = written with the `f` prefix [b48 Y5].
type Entry = (u64, u64, What, bool);

fn script() -> &'static Vec<Entry> {
	static S: OnceLock<Vec<Entry>> = OnceLock::new();
	S.get_or_init(|| std::env::var("RAC2_INPUT_FEED").map(|v| parse(&v)).unwrap_or_default())
}

fn parse(v: &str) -> Vec<Entry> {
	v.split(';')
		.filter_map(|e| {
			let mut it = e.splitn(3, ':');
			let (r, what, args) = (it.next()?, it.next()?, it.next().unwrap_or(""));
			let (a, z) = r.split_once('-').unwrap_or((r, r));
			let nums = |s: &str| -> Option<(f64, f64)> {
				let (x, y) = s.split_once(',')?;
				Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
			};
			// [b48 Y5] `fkey`: the same kind, field-locked (no kind name starts with `f`).
			let (what, locked) = what.trim().strip_prefix('f').map_or((what.trim(), false), |w| (w, true));
			let w = match what {
				"key" => What::Key(kbm::names::from_name(args)?),
				"cap" => What::Cap(kbm::names::from_name(args)?),
				"xl" => nums(args).map(|(x, y)| What::Stick(x as i16, y as i16))?,
				"mouse" => nums(args).map(|(x, y)| What::Mouse(x, y))?,
				"pad" => What::Pad,
				"ui" => nums(args).map(|(x, y)| What::Ui(x, y))?,
				"click" => What::Click(match args.trim() {
					"left" => MouseButton::Left,
					"right" => MouseButton::Right,
					_ => return None,
				}),
				_ => return None,
			};
			Some((a.trim().parse().ok()?, z.trim().parse().ok()?, w, locked))
		})
		.collect()
}

fn live(field: u64) -> impl Iterator<Item = &'static What> {
	script().iter().filter(move |e| (e.0..=e.1).contains(&field)).map(|e| &e.2)
}

/// [b46 U6] A feed script is set (RAC2_INPUT_FEED): the run's input is scripted.
pub fn scripted() -> bool {
	!script().is_empty()
}

/// [b48 Y5] The run has field-locked entries (an `f`-prefixed kind): host.rs / game.rs run the lock-step.
pub fn locked() -> bool {
	static L: OnceLock<bool> = OnceLock::new();
	*L.get_or_init(|| script().iter().any(|e| e.3))
}

/// [b48 Y5] The lock-step between the game thread (asks for a field) and the window thread (answers it with one poll).
pub struct Gate {
	ask: AtomicU64,
	ack: AtomicU64,
}

/// How long the game thread waits for the window's answer before it runs the field with the pad it has (a stalled window).
pub const GATE_WAIT: Duration = Duration::from_millis(100);

impl Gate {
	pub const fn new() -> Self {
		Gate { ask: AtomicU64::new(u64::MAX), ack: AtomicU64::new(u64::MAX) }
	}

	/// Game thread: names the field it will run next. Returns true when the window answered it; false when `quit` says stop or
	/// `wait` ran out.
	pub fn ask(&self, field: u64, wait: Duration, quit: impl Fn() -> bool) -> bool {
		self.ask.store(field, Ordering::SeqCst);
		let t = Instant::now();
		while self.ack.load(Ordering::SeqCst) != field {
			if quit() || t.elapsed() >= wait {
				return false;
			}
			std::thread::sleep(Duration::from_micros(200));
		}
		true
	}

	/// Window thread: the field the game thread waits for and nobody answered yet.
	pub fn pending(&self) -> Option<u64> {
		let g = self.ask.load(Ordering::SeqCst);
		(g != u64::MAX && self.ack.load(Ordering::SeqCst) != g).then_some(g)
	}

	/// Window thread: the pad of `field` is stored (call it after the poll, so the game's read sees the pad).
	pub fn answer(&self, field: u64) {
		self.ack.store(field, Ordering::SeqCst);
	}
}

static GATE: Gate = Gate::new();

/// [b48 Y5] Game thread, before it reads the pad of `field`: waits for the window's field-locked poll (no `f` entries: no wait).
pub fn gate(field: u64, quit: impl Fn() -> bool) {
	static LATE: AtomicU64 = AtomicU64::new(0);
	if locked() && !GATE.ask(field, GATE_WAIT, quit) && LATE.fetch_add(1, Ordering::Relaxed) < 5 {
		eprintln!("devfeed: field {field} ran without the window's locked poll (window stalled or quitting)");
	}
}

/// [b48 Y5] Window thread: the field to answer now (Some only in a locked run, once per field); None = nothing to do this redraw.
pub fn pending() -> Option<u64> {
	if locked() { GATE.pending() } else { None }
}

/// [b48 Y5] Window thread: `field`'s poll is done.
pub fn answer(field: u64) {
	GATE.answer(field);
}

/// [b50 V6] The run is a window-shots run (the game and the window step in lockstep: no `Gate`, the window's redraw of field F-1 holds
/// the pad that the game reads for F). Before this, a locked feed in such a run was never polled at all (`poll_input` returned early,
/// `Run::pad` never asked): every `f` entry of the tour did nothing, short taps and held keys alike.
static LOCKSTEP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_lockstep(on: bool) {
	LOCKSTEP.store(on, Ordering::Relaxed);
}

/// [b50 V6] A locked feed in a lockstep run: the window polls it once per field (`first_poll`), for the field about to run.
pub fn lockstep_locked() -> bool {
	LOCKSTEP.load(Ordering::Relaxed) && locked()
}

/// [b50 V6] Remembers the last field polled: true only the first time a field is seen, so several redraws of one field poll the feed
/// once (a key tap of 1 field is down for exactly one pad read, `fmouse` moves once per field).
pub struct Once(AtomicU64);

impl Once {
	pub const fn new() -> Self {
		Once(AtomicU64::new(u64::MAX))
	}
	pub fn first(&self, field: u64) -> bool {
		self.0.swap(field, Ordering::Relaxed) != field
	}
}

static POLLED: Once = Once::new();

/// [b50 V6] Window thread, lockstep + locked feed: true when `field` was not polled yet.
pub fn first_poll(field: u64) -> bool {
	POLLED.first(field)
}

/// [b50 V6] `--pad` / `--stick` next to a locked feed: the scripted pad stays, the window's pad (the feed's keys, stick, clicks) is
/// added: buttons OR-ed, sticks the window's only where the script has no stick entry. Only called when `locked()`; a pad-only run
/// never gets here (no `f` entry = the old code path, bit for bit).
pub fn merge_pad(scripted: (u16, [u8; 4]), has_stick: bool, fed: (u16, [u8; 4])) -> (u16, [u8; 4]) {
	// an all-zero stick word is the pad nothing has stored yet (before the first poll): keep the script's neutral sticks
	(scripted.0 | fed.0, if has_stick || fed.1 == [0; 4] { scripted.1 } else { fed.1 })
}

fn held_in(s: &[Entry], field: u64) -> Vec<Input> {
	s.iter().filter(|e| (e.0..=e.1).contains(&field)).filter_map(|e| if let What::Key(i) = &e.2 { Some(*i) } else { None }).collect()
}

/// The inputs held in `field`.
pub fn held(field: u64) -> Vec<Input> {
	held_in(script(), field)
}

/// The scripted XInput left stick of `field`.
pub fn stick(field: u64) -> Option<(i16, i16)> {
	live(field).find_map(|w| if let What::Stick(x, y) = w { Some((*x, *y)) } else { None })
}

/// [b46 U1] The scripted raw mouse motion of this poll (the `mouse:` entries live in `field`, summed), None without any.
pub fn motion(field: u64) -> Option<(f64, f64)> {
	live(field).filter_map(|w| if let What::Mouse(x, y) = w { Some((*x, *y)) } else { None }).reduce(|a, b| (a.0 + b.0, a.1 + b.1))
}

/// Scripted effects of this poll: a pad pulse, a capture key (raw mouse motion: `motion`).
/// `cap` ranges count POLLS (the field does not advance in menus), the other kinds count game fields.
pub fn poll(field: u64) {
	static POLLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
	let n = POLLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	if n % 200 == 0 && !script().is_empty() {
		eprintln!("devfeed: poll {n} field {field} entries {} capturing {}", script().len(), kbm::capturing());
	}
	for e in script() {
		match &e.2 {
			// [b46 U1] `mouse:` motion: host.rs `poll_input` routes `motion(field)` like a real raw motion (cursor_vis included).
			What::Pad if (e.0..=e.1).contains(&field) => rac_engine::cursor_vis::pad(),
			What::Cap(i) if (e.0..=e.1).contains(&n) && kbm::capturing() => kbm::capture_poll(&[*i], 0),
			_ => {}
		}
	}
}

/// [PC] nav lane: the scripted window events of `field` (`ui` / `click` entries), each sent once: a cursor move once per (entry,
/// field), a click once per entry.
pub fn ui_events(field: u64) -> Vec<WindowEvent> {
	static SENT: Mutex<Vec<u64>> = Mutex::new(Vec::new());
	let s = script();
	if s.is_empty() {
		return Vec::new();
	}
	let mut sent = SENT.lock().unwrap_or_else(|e| e.into_inner());
	sent.resize(s.len(), u64::MAX);
	let mut out = Vec::new();
	let device_id = DeviceId::dummy();
	for (i, e) in s.iter().enumerate() {
		if !(e.0..=e.1).contains(&field) {
			continue;
		}
		match &e.2 {
			What::Ui(x, y) if sent[i] != field => {
				sent[i] = field;
				out.push(WindowEvent::CursorMoved { device_id, position: winit::dpi::PhysicalPosition::new(*x, *y) });
			}
			What::Click(button) if sent[i] == u64::MAX => {
				sent[i] = field;
				for state in [ElementState::Pressed, ElementState::Released] {
					out.push(WindowEvent::MouseInput { device_id, state, button: *button });
				}
			}
			_ => {}
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_script_parses() {
		let s = parse("10-20:key:Space;30:xl:2000,-1000;40-50:mouse:3,4;60:cap:KeyN;70:pad;x:key:Space;5:key:NoSuchKey");
		assert_eq!(s.len(), 5);
		assert!(matches!(s[1].2, What::Stick(2000, -1000)) && s[1].0 == 30 && s[1].1 == 30);
		assert!(matches!(s[2].2, What::Mouse(..)) && matches!(s[3].2, What::Cap(_)) && matches!(s[4].2, What::Pad));
		let u = parse("130:ui:640.5,360;131-140:click:left;150:click:right;160:click:middle");
		assert_eq!(u.len(), 3);
		assert!(matches!(u[0].2, What::Ui(x, y) if x == 640.5 && y == 360.0) && u[0].0 == 130 && u[0].1 == 130);
		assert!(matches!(u[1].2, What::Click(MouseButton::Left)) && u[1].0 == 131 && u[1].1 == 140);
		assert!(matches!(u[2].2, What::Click(MouseButton::Right)));
	}

	/// [b48 Y5] `f` prefixes lock an entry; the kinds and ranges are the unprefixed ones.
	#[test]
	fn f_prefix_locks() {
		let s = parse("10-20:fkey:Space;30:key:Space;40-50:fmouse:3,4;60:fclick:left;70:fxl:1,2;80:fui:5,6;90:fpad;100:fbogus:1");
		assert_eq!(s.len(), 7);
		assert_eq!(s.iter().map(|e| e.3).collect::<Vec<_>>(), [true, false, true, true, true, true, true]);
		assert!(matches!(s[0].2, What::Key(_)) && matches!(s[2].2, What::Mouse(..)) && matches!(s[3].2, What::Click(MouseButton::Left)));
		assert!(matches!(s[4].2, What::Stick(1, 2)) && matches!(s[5].2, What::Ui(..)) && matches!(s[6].2, What::Pad));
		assert!(s[0].0 == 10 && s[0].1 == 20 && s[1].0 == 30 && s[1].1 == 30);
	}

	/// [b48 Y5] The timing rule: whatever the window's poll rate, the game's read of field F sees the pad answered for F, and each
	/// field is answered exactly once, in order.
	#[test]
	fn the_gate_answers_every_field_exactly_once() {
		use std::sync::Arc;
		for poll_us in [50u64, 700, 3000] {
			let g = Arc::new(Gate::new());
			let pad = Arc::new(AtomicU64::new(u64::MAX));
			let (g2, pad2) = (g.clone(), pad.clone());
			let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
			let stop2 = stop.clone();
			let win = std::thread::spawn(move || {
				let mut answered = Vec::new();
				while !stop2.load(Ordering::SeqCst) {
					if let Some(f) = g2.pending() {
						pad2.store(f * 10, Ordering::SeqCst); // the "pad" of field f
						g2.answer(f);
						answered.push(f);
					}
					std::thread::sleep(Duration::from_micros(poll_us));
				}
				answered
			});
			let mut seen = Vec::new();
			for f in 0..40u64 {
				assert!(g.ask(f, Duration::from_secs(5), || false));
				seen.push(pad.load(Ordering::SeqCst));
			}
			stop.store(true, Ordering::SeqCst);
			let answered = win.join().unwrap();
			assert_eq!(seen, (0..40).map(|f| f * 10).collect::<Vec<_>>());
			assert_eq!(answered, (0..40).collect::<Vec<_>>());
			assert_eq!(g.pending(), None);
		}
	}

	/// [b50 V6] A tap of 1-3 fields is held in exactly its fields and polled once each; held keys and `fmouse` are as before.
	#[test]
	fn a_tap_is_seen_once_by_the_reader_of_its_field() {
		let s = parse("60:fkey:Escape;70-72:fkey:Space;80-90:fkey:KeyC;100-103:fmouse:3,4");
		let seen = |k: Input| (50..120u64).filter(|&f| held_in(&s, f).contains(&k)).collect::<Vec<_>>();
		assert_eq!(seen(kbm::names::from_name("Escape").unwrap()), [60]);
		assert_eq!(seen(kbm::names::from_name("Space").unwrap()), [70, 71, 72]);
		assert_eq!(seen(kbm::names::from_name("KeyC").unwrap()), (80..=90).collect::<Vec<_>>());
		// the lockstep window polls once per field however many redraws it makes
		let once = Once::new();
		let mut polls = Vec::new();
		for f in [59, 60, 60, 60, 61, 61, 62] {
			if once.first(f) {
				polls.push(f);
			}
		}
		assert_eq!(polls, [59, 60, 61, 62]);
		// mouse motion: one entry per field of its range (poll-timed reads of `motion` are unchanged: same `live` filter)
		let m = |f: u64| s.iter().filter(|e| (e.0..=e.1).contains(&f)).filter(|e| matches!(e.2, What::Mouse(..))).count();
		assert_eq!((m(99), m(100), m(103), m(104)), (0, 1, 1, 0));
	}

	/// [b50 V6] The scripted pad and the feed's pad together.
	#[test]
	fn a_pad_script_and_a_locked_feed_add_up() {
		assert_eq!(merge_pad((0x0800, [0x80, 0x80, 0x80, 0x80]), false, (0x0040, [0x80, 0x80, 0xFF, 0x80])), (0x0840, [0x80, 0x80, 0xFF, 0x80]));
		assert_eq!(merge_pad((0, [0x80, 0x80, 0x00, 0x80]), true, (0x10, [0x80, 0x80, 0xFF, 0x80])), (0x10, [0x80, 0x80, 0x00, 0x80]));
	}

	#[test]
	fn the_gate_gives_up_and_stops() {
		let g = Gate::new();
		assert_eq!(g.pending(), None);
		assert!(!g.ask(3, Duration::from_millis(5), || false)); // nobody answers: the game runs on
		assert_eq!(g.pending(), Some(3));
		assert!(!g.ask(4, Duration::from_secs(5), || true)); // quit
		g.answer(4);
		assert_eq!(g.pending(), None);
		assert!(g.ask(4, Duration::from_millis(5), || false)); // an answered field returns at once
	}
}
