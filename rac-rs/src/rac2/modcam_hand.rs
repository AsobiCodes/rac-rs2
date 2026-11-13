//! The Modern camera's hook into R&C2's follow camera (lane scratch_rc2_cam, stage B1, 2026-10-31).
//!
//! Retail's follow camera is one class update (class table index 0, `+0xc` = 0x42b8d0), called ONLY through the table (`jalr` in the select
//! function, a0 = the current camera record) [C handoff_stageB.md]. Its span is 48 words (0xc0 bytes from 0x42b8d8) that every one of the 26
//! level overlays holds once, linked at another address and with 10 fields relocated (7 `jal` targets, 3 16-bit immediates; the
//! hand binds at 0x42b8d8, after the 2-word prologue at 0x42b8d0, and the 0xc0-byte span to 0x42b998 includes the 0x42b990 stub):
//! `SPEC` is that template (tools: `mkspec.py`, checked against all 26 RAM dumps, lv6 0x47cfc0 and lv11 0x48dd10 included).
//! A relocatable hand (`ee_oracle::rec::register_reloc_hand`, the pattern of R&C1's prompts_box.rs) is installed by
//! `Table::build` at every recompiled copy whose code equals the template with those fields masked (the span must be 0xc0).
//!
//! The hand WRAPS retail: it runs retail's update first (`c.call(entry)`; the re-entry guard `IN` makes that nested dispatch fall
//! through to the recompiled body), then asks the pure camera (`modcam_pose::Cam`) for the Modern pose and writes it into the
//! record: rows +0x00 (forward), +0x10 (left), +0x20 (up) and the eye +0x30, three f32 each (the 4th words stay). Retail's own
//! camera main copies the record into the published camera afterwards, and the hero reads the published camera for his
//! camera-relative movement, so he runs where the Modern camera faces with no further hook (as in R&C1).
//!
//! State kept between fields (`ST`): the camera, the level key (level word + entry pc: a change resets it), the run-time addresses
//! (`Addrs::find`, retried every 30 calls while the level loads) and the words the hand wrote last. GAP RULE: before retail's
//! update the record is read; if it is bit-equal to what the hand wrote last field, nothing else touched it (pause, menu, a
//! field without the follow update: the camera simply continues, Smooth sees the field gap and re-seeds its velocity); if not
//! (new level, another camera record or class wrote it) `modcam_blend::decide` rules: the follow camera was away for more than
//! `RECENT` fields (another camera class was current) = the Modern pose is stale, the camera ends and seeds from retail's pose; it ran
//! just before (another follow record took over, a script wrote the record) or retail's switch blend runs (the u16 at
//! `basis - BLEND_BELOW_BASIS`) = `PoseIn::retail_drives`: the pose blends from the last Modern pose to retail's over `BLEND` fields
//! (`Cam::hand_back`) instead of snapping; both latch it until `Cam::blending` is over, so a 3-field counter still blends fully. Class (`+0x86`) != 0 (retail's free look and the like) is left to retail. `follow_ran_recently(field)` tells the window whether the follow camera
//! ran in the last fields: the mouse is held only then (host/cursor.rs: menus, pause and cutscenes let it go).
//! The wall probe is `modcam_probe::RetailProbe` (retail's own segment test, run on the CPU and undone bit for bit); `NoWalls` when the
//! finder has no match in the level or the moby table is implausible. The right stick comes from `modcam_ui::rstick` (keys + controller).

use crate::addrs::Addrs;
use crate::modcam_blend::{self as blend, Facts, Why};
use crate::modcam_pose::{Cam, Config, Gains, Input, NoWalls, Pose, PoseIn, Probe, Rows, V3, len3, scale, sub};
use crate::modcam_probe::{RetailProbe, SegCache, Wall};
use crate::modcam_who::Who;
use ee_oracle::cpu::Cpu;
use ee_oracle::hs_reloc::{Reloc, RelocHand};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

/// The follow class update of Aranos from its prologue (0x42b8d8, 0xc0 bytes; the class table points 8 bytes earlier, at `lui v0; lw v0` which the recompiler left out of the function, so the hand binds at the recompiled entry 0x42b8d8 = what the interpreter reaches after those two words; every compiled copy's span is exactly 0xc0 [C rc2gen r2lN]) with the fields that differ between the 26 copies masked: `jal` targets
/// (0x3ffffff) at 0x50, 0x6c, 0x7c, 0x84, 0x8c, 0x94, 0x9c, `lui`/`addiu` immediates (0xffff) at 0x10, 0x30, 0x5c.
const SPEC: &str = "reloc follow entry 0x42b8d8 len 0xc0\nmhash 0x673ee3d8209ab859\nmask 0x8 0xffff\nmask 0x28 0xffff\nmask 0x48 0x3ffffff\nmask 0x54 0xffff\nmask 0x64 0x3ffffff\nmask 0x74 0x3ffffff\nmask 0x7c 0x3ffffff\nmask 0x84 0x3ffffff\nmask 0x8c 0x3ffffff\nmask 0x94 0x3ffffff\n";

/// Instructions the hand adds to the count (a normal hand: the wrapped retail update counts its own).
const EST: u64 = 200;
/// Fields the follow camera may have been silent and still count as running (`follow_ran_recently`).
const RECENT: u32 = 2;
/// The hero moves more than this (m) up or down in one field (6 m/s): in a jump or a fall.
const AIR_STEP: f32 = 0.12;
/// The boom the player starts with (the seed takes retail's own boom). Seconds per field: `rac_engine::field::field_dt()` (PAL 0.02,
/// NTSC 1001/60000), as R&C1's `dt()` (sweep C4: the per-field turn rates are R&C1's at the same field rate, only the springs' time
/// base had to follow the disc).
const DIST: f32 = 7.0;
/// Record offsets: the three rows and the eye (each f32 x 3), and the camera class (u16).
const OFFS: [u32; 4] = [0x00, 0x10, 0x20, 0x30];
const CLASS: u32 = 0x86;

/// The 12 words the hand reads and writes: rows 0..2 then the eye, three f32 bits each.
type Words = [u32; 12];

fn reloc() -> &'static Reloc {
	static R: OnceLock<Reloc> = OnceLock::new();
	R.get_or_init(|| Reloc::parse(SPEC).expect("the follow camera spec parses"))
}

/// Registers the hand (before the recompiled table is built: `lib.rs` `register`, only with the Modern camera on).
pub fn register() {
	static HAND: RelocHand = RelocHand { name: "modcam_follow", reloc, est: EST, f: follow };
	ee_oracle::rec::register_reloc_hand(&HAND);
}

static IN: AtomicBool = AtomicBool::new(false);
struct Guard;
impl Drop for Guard {
	fn drop(&mut self) {
		IN.store(false, Ordering::SeqCst);
	}
}

/// The field + 1 of the newest run of the hand (0 = never).
static LAST: AtomicU32 = AtomicU32::new(0);
static ST: Mutex<Option<State>> = Mutex::new(None);

/// The Modern camera a quick save keeps (`states::save`): all of its state as words (`modcam_pose/snap.rs`), None while it is not running.
pub fn export() -> Option<Vec<u32>> {
	ST.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|s| s.cam.snapshot())
}

/// A quick load hands the saved camera back: the first seed of the new game starts from it, so the view does not snap behind him.
pub fn import(v: &[u32]) {
	ST.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::new).cam.restore(v);
}

/// Whether the follow camera ran in `field` or the `RECENT` fields before (`modcam_ui::current().0` is the newest field): the
/// cursor is held while it does.
pub fn follow_ran_recently(field: u32) -> bool {
	recent(LAST.load(Ordering::Relaxed), field)
}

fn recent(last: u32, field: u32) -> bool {
	last != 0 && field.wrapping_sub(last - 1) <= RECENT
}

/// `RAC2_CAM_LOG=1`: gaps, resets and a status line every 250 steps.
fn log_on() -> bool {
	static ON: OnceLock<bool> = OnceLock::new();
	*ON.get_or_init(|| std::env::var_os("RAC2_CAM_LOG").is_some_and(|v| v != "0"))
}

/// `RAC2_CAM_LOG=2`: also one `modcam boom` line per step (the boom length, the wall test's reach, what it hit).
fn log_all() -> bool {
	static ON: OnceLock<bool> = OnceLock::new();
	*ON.get_or_init(|| std::env::var("RAC2_CAM_LOG").is_ok_and(|v| v == "2"))
}

// ---- the record --------------------------------------------------------------------------------------------------------------

fn rd_words(c: &mut Cpu, rec: u32) -> Words {
	let mut w = [0u32; 12];
	for (i, x) in w.iter_mut().enumerate() {
		*x = c.mem.rd32(rec + OFFS[i / 3] + 4 * (i as u32 % 3));
	}
	w
}

fn v3_of(w: &[u32]) -> V3 {
	[f32::from_bits(w[0]), f32::from_bits(w[1]), f32::from_bits(w[2])]
}

fn pose_of(w: &Words) -> Pose {
	Pose { eye: v3_of(&w[9..12]), rows: [v3_of(&w[0..3]), v3_of(&w[3..6]), v3_of(&w[6..9])] }
}

fn write_pose(c: &mut Cpu, rec: u32, p: &Pose) {
	let rows: Rows = p.rows;
	for (r, v) in [rows[0], rows[1], rows[2], p.eye].iter().enumerate() {
		for (k, x) in v.iter().enumerate() {
			c.mem.wr32(rec + OFFS[r] + 4 * k as u32, x.to_bits());
		}
	}
}

/// The character the player controls (`modcam_who`: the hero moby pointer's record, Clank's in his sections; else the first moby
/// record), slot byte 0, position f32 x 3 at +0x10 in metres [C: Aranos field 2000 247.0, 190.7, 49.9, the camera eye 254.3, 196.4, 54.2].
fn hero_pos(c: &mut Cpu, a: &Addrs) -> Option<Who> {
	crate::modcam_who::find(&c.mem.ram, a.moby)
}

/// In a jump or a fall: the hero moved more than `AIR_STEP` up or down since the field before (a field gap says nothing).
fn airborne(last: Option<(u32, V3)>, field: u32, hero: V3) -> bool {
	last.is_some_and(|(n, h)| field == n.wrapping_add(1) && (hero[2] - h[2]).abs() > AIR_STEP)
}

// ---- the state ---------------------------------------------------------------------------------------------------------------

/// What `State::run` needs of one field (everything read from RAM and the options).
struct In {
	field: u32,
	hero: V3,
	input: Input,
	/// The right stick bytes as the turn maths gets them (`modcam_ui::rstick`: keys + controller, `camera_sticks`).
	rstick: [u8; 2],
	smooth: bool,
	gains: Gains,
	retail: Pose,
	/// Retail owns the pose this field (`State::judge`): the Modern pose blends back to `retail`.
	retail_drives: bool,
	/// The hero's smoothed up (`modcam_frame`): world z on flat ground.
	up: V3,
}

struct State {
	cam: Cam,
	key: Option<(u32, u32)>,
	addrs: Option<Addrs>,
	/// The smoothed hero up word (`modcam_frame::find_up`), found with `addrs`.
	up_addr: Option<u32>,
	/// Calls since the level key changed (the address scan is retried every 30 while it finds nothing).
	tries: u32,
	/// The record words after the hand's last write.
	last: Option<Words>,
	hero_last: Option<(u32, V3)>,
	/// The controlled moby of the last field (`modcam_who`): a change is a switch of character.
	who_last: Option<u32>,
	/// A controlled moby seen for one field only (`steady`).
	pending: Option<u32>,
	logged: bool,
	gaps: u32,
	resets: u32,
	steps: u32,
	/// The one-shot hand-back that a rewritten record starts (`modcam_blend::decide`), who owns the pose now, and how often retail took it.
	latch: bool,
	why: Option<Why>,
	handbacks: u32,
	/// Fields the retail switch blend counter has run in a row (logged with RAC2_CAM_LOG when it ends: how long it really runs).
	sw_run: u32,
	/// The wall probe: the finder's cache, the restore buffers, and what the probes did (calls, hits) for the status line.
	seg: SegCache,
	wall: Wall,
	probes: (u64, u64),
	/// Lines logged for steps in which a probe hit a wall (capped).
	wall_lines: u32,
	/// The field of the level's first seed (`cold`).
	first: Option<u32>,
	/// The hero-state gate of the last field (`modcam_yield`), for the log.
	gate: Option<crate::modcam_yield::Gate>,
}

/// The level's first seed and any seed within this many fields after it are a cold start (a load, the end of a level's intro;
/// a retail placement right after it re-seeds, `modcam_blend::Why::Placed`): build 43's seed, no look fade (`Cam::set_cold`).
/// Later take-overs (respawns, mid-level cutscene ends, deaths) keep the fade.
const COLD: u32 = 4;

impl State {
	fn new() -> State {
		State { cam: Cam::new(Config::new(rac_engine::field::field_dt(), DIST)), key: None, addrs: None, up_addr: None, tries: 0, last: None, hero_last: None, who_last: None, pending: None, logged: false, gaps: 0, resets: 0, steps: 0, latch: false, why: None, handbacks: 0, sw_run: 0, seg: SegCache::default(), wall: Wall::default(), probes: (0, 0), wall_lines: 0, first: None, gate: None }
	}

	/// Every call, after `level` and before the camera runs: tells it whether a seed now is a cold start (`COLD`).
	fn cold(&mut self, field: u32) {
		if self.first.is_none() {
			self.first = self.cam.seeded();
		}
		self.cam.set_cold(self.first.map_or(true, |s| field.wrapping_sub(s) < COLD));
	}

	/// A new level / overlay (or the first call): forget the camera, the addresses and the record.
	fn level(&mut self, key: (u32, u32)) {
		if self.key == Some(key) {
			return;
		}
		self.cam.reset();
		(self.key, self.addrs, self.up_addr, self.tries, self.last, self.hero_last, self.logged) = (Some(key), None, None, 0, None, None, false);
		(self.latch, self.why, self.sw_run, self.who_last, self.pending, self.first) = (false, None, 0, None, None, None);
		self.resets += 1;
		if log_on() {
			eprintln!("rac2: modcam level key {key:x?}, reset #{}", self.resets);
		}
	}

	/// The gap rule: whether the record as retail's update found it differs from the words written last field (counted and logged);
	/// `judge` decides what that means.
	fn gap(&mut self, before: &Words) -> bool {
		let changed = self.last.as_ref() != Some(before);
		if changed {
			self.gaps += 1;
			if log_on() {
				eprintln!("rac2: modcam gap #{} (the record changed since the last write; step {})", self.gaps, self.steps);
			}
		}
		changed
	}

	/// Who owns the pose this field (`modcam_blend::decide`): ends a stale camera, counts and logs every change of owner.
	fn judge(&mut self, changed: bool, silent: bool, blend_word: u16) -> blend::Verdict {
		let v = blend::decide(Facts { blend: blend_word, changed, silent, live: self.cam.live(), fresh: self.cam.fresh() }, &mut self.latch);
		if blend::switch_blend(blend_word) {
			self.sw_run += 1;
		} else if self.sw_run > 0 {
			if log_on() {
				eprintln!("rac2: modcam retail switch blend counter ran {} fields (step {})", self.sw_run, self.steps);
			}
			self.sw_run = 0;
		}
		if v.end {
			self.cam.end();
		}
		// Every change of reason is logged (the respawn goes Away -> Placed in a row: run u_new5 fields 208/209); a hand-back counts once.
		if v.why != self.why {
			self.handbacks += u32::from(v.why.is_some() && self.why.is_none());
			if log_on() {
				match v.why {
					Some(w) => eprintln!("rac2: modcam hands back to retail: {} (step {})", w.text(), self.steps),
					None => eprintln!("rac2: modcam drives the follow camera again (step {})", self.steps),
				}
			}
		}
		self.why = v.why;
		v
	}

	/// The controlled character changed (Ratchet <-> Clank, `modcam_who`) while the Modern camera drives: the boom neither jumps to
	/// the other one nor counts it as a teleport (`Cam::retarget`); retail's pose, which already follows the new character, takes over
	/// through the latched hand-back blend (`BLEND` fields) and the Modern camera seeds again from there. A change while it is not live
	/// (a cutscene in between) needs nothing: it seeds from retail's pose anyway.
	/// A new controlled moby counts from its second field in a row: the respawn names another record for one field [C run u_new,
	/// field 208: the first record, then Clank again]; meanwhile the camera keeps the last character while its record is valid.
	fn steady(&mut self, w: Who, ram: &[u8]) -> Who {
		match self.who_last {
			Some(l) if l != w.moby && self.pending != Some(w.moby) => {
				self.pending = Some(w.moby);
				crate::modcam_who::at(ram, l).unwrap_or(w)
			}
			_ => {
				self.pending = None;
				w
			}
		}
	}

	fn switch(&mut self, who: Who, field: u32) -> bool {
		let swapped = crate::modcam_who::switched(self.who_last, who.moby) && self.cam.live();
		self.who_last = Some(who.moby);
		if swapped {
			self.cam.retarget(who.pos);
			self.hero_last = None;
			self.latch = true;
			if log_on() {
				eprintln!("rac2: modcam follows another character: moby {:#x} {:?} (field {field}, step {})", who.moby, who.body, self.steps);
			}
		}
		swapped
	}

	/// One field: the Modern pose for the input, `None` = leave retail's.
	fn run(&mut self, f: &In, p: &mut dyn Probe) -> Option<Pose> {
		let airborne = airborne(self.hero_last, f.field, f.hero);
		self.hero_last = Some((f.field, f.hero));
		let pin = PoseIn { field: f.field, hero: f.hero, airborne, input: f.input, rstick: f.rstick, gains: f.gains, smooth: f.smooth, retail: f.retail, retail_drives: f.retail_drives, up: f.up };
		self.steps += 1;
		let out = self.cam.step(&pin, p);
		if log_on() && self.steps % 5 == 0 && self.steps < 3000 {
			eprintln!("rac2: modcam track field {} hero {:.2?} yaw {:.3}", f.field, f.hero, self.cam.yaw());
		}
		if log_on() && self.steps % 250 == 0 {
			eprintln!("rac2: modcam step {} gaps {} resets {} handbacks {} live {} yaw {:.3} pitch {:.3} len {:.2} lift {:.2} probes {} hits {}", self.steps, self.gaps, self.resets, self.handbacks, self.cam.live(), self.cam.yaw(), self.cam.pitch(), self.cam.len(), self.cam.lift(), self.probes.0, self.probes.1);
		}
		out
	}

	/// `run` with retail's own wall test as the probe (`modcam_probe`): found by code words in this level's RAM, built once per call
	/// (it reads the moby list once). No match in the level, or an implausible moby table: `NoWalls`, as before the probe.
	/// `who` = the controlled moby: the test ignores it (Clank's own body is no wall; Ratchet's is the table's first record, as before).
	fn walled_run(&mut self, c: &mut Cpu, f: &In, who: u32) -> Option<Pose> {
		let key = c.mem.rd32(ee_oracle::rc2::LEVEL);
		let seg = self.seg.get(&c.mem.ram, key);
		let table = self.addrs.map_or(0, |a| a.moby);
		let mut wall = std::mem::take(&mut self.wall);
		let out = match seg.and_then(|s| RetailProbe::try_new(c, s, &mut wall, table)).map(|p| p.ignoring(who)) {
			Some(mut probe) => {
				let out = self.run(f, &mut probe);
				let hits = probe.hits as u64;
				let probe_markers = probe.markers();
				self.probes = (self.probes.0 + probe.calls as u64, self.probes.1 + hits);
				if log_on() && log_all() {
					// RAC2_CAM_LOG=2: one line per step (the boom, what the tests hit, the hit mobys as addr:class).
					let hm: Vec<String> = probe.hit_mobys.iter().map(|(m, c)| format!("{m:x}:{c:x}")).collect();
					eprintln!("rac2: modcam boom field {} len {:.3} tr {:.2?} hits {hits}/{} rides {} markers {} [{}]", f.field, self.cam.len(), self.cam.trace(), probe.calls, probe.rides, probe_markers, hm.join(" "));
				}
				if log_on() && hits > 0 && self.wall_lines < 400 {
					self.wall_lines += 1;
					eprintln!("rac2: modcam wall field {} yaw {:.3} pitch {:.3} len {:.2} hits {hits}/{} rider skips {}", f.field, self.cam.yaw(), self.cam.pitch(), self.cam.len(), probe.calls, probe.rides);
				}
				out
			}
			None => self.run(f, &mut NoWalls),
		};
		self.wall = wall;
		out
	}

	/// After retail's update: the Modern pose into the record. `rec_class` = the record's class (`+0x86`).
	fn after(&mut self, c: &mut Cpu, entry: u32, rec: u32, before: Words, rec_class: u16) {
		self.level((c.mem.rd32(ee_oracle::rc2::LEVEL), entry));
		let (field, input) = crate::modcam_ui::current();
		self.cold(field);
		if rec_class != 0 {
			self.cam.end();
			self.last = None;
			return;
		}
		let silent = !recent(LAST.load(Ordering::Relaxed), field);
		let changed = self.gap(&before);
		// Each address retried on its own until found (review A3: the hero up may still be unset when the camera's are found).
		if self.addrs.is_none() || self.up_addr.is_none() {
			self.tries += 1;
			if self.tries % 30 == 1 {
				if self.addrs.is_none() {
					self.addrs = crate::scan_timing::timed("hand_addrs", || Addrs::find(&c.mem.ram));
				}
				if self.up_addr.is_none() {
					self.up_addr = crate::scan_timing::timed("hand_up", || crate::modcam_frame::find_up(&c.mem.ram));
				}
			}
		}
		let who = self.addrs.and_then(|a| hero_pos(c, &a)).map(|w| self.steady(w, &c.mem.ram));
		let Some(who) = who else {
			self.cam.end();
			self.last = None;
			return;
		};
		let hero = who.pos;
		let blend_word = self.addrs.map_or(0, |a| c.mem.rd16(a.basis - blend::BLEND_BELOW_BASIS));
		let v = self.judge(changed, silent, blend_word);
		let swapped = self.switch(who, field);
		// The hero-state gates (modcam_yield, R&C1 `why_retail`): rail, look, swing, script freeze, grind, vendor / pad lock.
		let gate = crate::modcam_yield::hero_gate(&c.mem.ram);
		if gate != self.gate {
			if log_on() {
				eprintln!("rac2: modcam hero gate {} (field {field})", gate.map_or("off".into(), |g| g.text()));
			}
			self.gate = gate;
		}
		// Camera Classic chosen in the menu while the Modern camera runs (A4, R&C1's toggle off): a live camera blends back to retail's
		// like a gate; `follow` declines once nothing is left to blend.
		let gated = gate.is_some() || !crate::modcam_ui::modern();
		let (mut yield_drive, mut yield_out) = crate::modcam_yield::verdict(gated, self.cam.live() || self.cam.blending());
		if !who.body.on_foot() || crate::modcam_death::dying_in(&c.mem.ram) {
			// Giant Clank, a vehicle or the hero's death (`modcam_death`): retail's camera, as R&C1 (a live Modern camera blends back to it
			// first; after the respawn the camera seeds from retail's pose, `modcam_look` keeps that take-over free of a cut).
			let going = self.cam.live() || self.cam.blending();
			(yield_drive, yield_out) = (yield_drive || going, yield_out || !going);
		}
		if yield_out {
			self.cam.end();
			self.last = Some(rd_words(c, rec));
			// [b46 W6] U1: during the vendor / pad lock the follow camera is not "running" for the cursor: it stays free for the vendor's menu
			if !matches!(gate, Some(crate::modcam_yield::Gate::Interaction(_))) {
				LAST.store(field.wrapping_add(1), Ordering::Relaxed);
			}
			return;
		}
		let o = crate::options::get();
		let retail = pose_of(&rd_words(c, rec));
		// The strafe aim (raised pivot, shoulder) is Ratchet's alone, as R&C1's aim mode needs body form 0.
		let input = if who.body.aims() && o.strafe_on() /* [b46 W6] C11 */ { input } else { crate::modcam_pose::Input { aim: false, strafe: false, ..input } };
		let f = In { field, hero, input, rstick: crate::modcam_ui::rstick(), smooth: o.camfeel_smooth(), gains: gains_of(&c.mem.ram, &o), retail, retail_drives: v.retail_drives || yield_drive || swapped, up: self.up_addr.map_or([0.0, 0.0, 1.0], |a| crate::modcam_frame::read_up(&c.mem.ram, a)) };
		if let Some(p) = self.walled_run(c, &f, who.moby) {
			write_pose(c, rec, &p);
		}
		blend::settle(&mut self.latch, self.cam.blending());
		self.last = Some(rd_words(c, rec));
		LAST.store(field.wrapping_add(1), Ordering::Relaxed);
		if !self.logged {
			self.logged = true;
			let from_eye = len3(sub(retail.eye, hero));
			eprintln!("rac2: modcam hand active: level word {} entry {entry:#x} rec {rec:#x} hero {hero:.1?} retail eye {:.1?} ({from_eye:.1} m away)", c.mem.rd32(ee_oracle::rc2::LEVEL), retail.eye);
		}
	}
}

/// The turn gains of this field: the Mouse / Camera speed rows, retail's Reversed options (word 0 = Reversed) and the mouse's own
/// invert rows (`modcam_mouseinv`, R&C1's `mouse_lr` / `mouse_ud`). Shared with the first-person mouse look (`modcam_fp`).
pub fn gains_of(ram: &[u8], o: &rac_engine::pcopts::PcOptions) -> Gains {
	let zero = |a: u32| rac_formats::bytes::u32_at(ram, a as usize).is_ok_and(|w| w == 0);
	let g = Gains::new(scale(o.mousespeed), scale(o.camspeed), zero(crate::addrs::OPT_LR), zero(crate::addrs::OPT_UD));
	g.with_mouse_signs(crate::modcam_mouseinv::signs(g.lr, g.ud))
}

/// Whether the hand has anything to do with Camera Classic: a Modern camera still live or blending back after the switch to Classic.
fn classic_idle() -> bool {
	!crate::modcam_ui::modern() && ST.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_none_or(|s| !(s.cam.live() || s.cam.blending()))
}

/// The hand: retail's follow update, then the Modern pose over its record. Declines (`false`) inside the nested call that runs
/// retail's body (`IN`), for a record that cannot be one, and with Camera Classic once no Modern pose is left to blend back (then
/// retail's update runs untouched: Classic writes nothing; the hand is registered always so the menu switch is live, A4).
fn follow(c: &mut Cpu) -> bool {
	if IN.swap(true, Ordering::SeqCst) {
		return false;
	}
	if classic_idle() {
		IN.store(false, Ordering::SeqCst);
		return false;
	}
	let _g = Guard;
	let (entry, rec) = (c.pc, c.g(4) as u32 & 0x01ff_ffff);
	if rec == 0 || rec as usize + 0x90 > c.mem.ram.len() {
		return false;
	}
	let (class, before) = (c.mem.rd16(rec + CLASS), rd_words(c, rec));
	c.call(entry);
	let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
	g.get_or_insert_with(State::new).after(c, entry, rec, before, class);
	true
}

#[cfg(test)]
mod tests {
	use super::*;
	use ee_oracle::mem::Mem;

	fn pose(eye: V3) -> Pose {
		Pose { eye, rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] }
	}

	fn field(n: u32, look: [i16; 2]) -> In {
		let hero = [100.0, 50.0, 10.0];
		In { field: n, hero, input: Input { look, ..Input::default() }, rstick: [0x80; 2], smooth: false, gains: Gains::new(1.0, 1.0, false, false), retail: pose([hero[0] - 7.0, hero[1], hero[2] + 3.0]), retail_drives: false, up: [0.0, 0.0, 1.0] }
	}

	fn words_of(p: &Pose) -> Words {
		let mut w = [0u32; 12];
		for (i, x) in p.rows.iter().flatten().chain(p.eye.iter()).enumerate() {
			w[i] = x.to_bits();
		}
		w
	}

	#[test]
	fn the_spec_is_the_follow_update_and_matches_a_real_level() {
		let r = reloc();
		assert_eq!((r.entry, r.len, r.mhash), (0x42b8d8, 0xc0, 0x673ee3d8209ab859));
		// The Aranos RAM dump of the research (when present): the template matches its copy of the update at 0x42b8d8.
		let Ok(ram) = std::fs::read("E:/ratchet/rac2/runs/ram2000.bin") else {
			eprintln!("research dump E:/ratchet/rac2/runs/ram2000.bin not present: skipped");
			return;
		};
		let mut m = Mem::new();
		let n = ram.len().min(m.ram.len());
		m.ram[..n].copy_from_slice(&ram[..n]);
		assert!(r.matches(&m, 0x42b8d8), "the spec text must match the dump (a typo in a mask moves the masked hash)");
		assert!(!r.matches(&m, 0x42b8dc));
	}

	#[test]
	fn the_record_round_trips_and_the_fourth_words_stay() {
		let mut c = Cpu::new(Mem::new());
		let rec = 0x30_0000;
		c.mem.wr32(rec + 0x0c, 0x1111_1111);
		c.mem.wr32(rec + 0x2c, 0x2222_2222);
		let p = Pose { eye: [1.5, -2.5, 3.5], rows: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]] };
		write_pose(&mut c, rec, &p);
		assert_eq!(pose_of(&rd_words(&mut c, rec)), p);
		assert_eq!((c.mem.rd32(rec + 0x0c), c.mem.rd32(rec + 0x2c)), (0x1111_1111, 0x2222_2222));
		assert_eq!(rd_words(&mut c, rec), words_of(&p));
	}

	#[test]
	fn the_hero_is_the_first_record_and_must_be_slot_zero() {
		let mut c = Cpu::new(Mem::new());
		let (table, s) = (0x20_0000, 0x40_0000);
		let a = Addrs { moby: table, ..Addrs::aranos() };
		c.mem.wr32(table, s);
		for (k, v) in [247.0f32, 190.5, 49.9].iter().enumerate() {
			c.mem.wr32(s + 0x10 + 4 * k as u32, v.to_bits());
		}
		assert_eq!(hero_pos(&mut c, &a).map(|w| w.pos), Some([247.0, 190.5, 49.9]));
		c.mem.wr8(s + 0x20, 0xfd);
		assert_eq!(hero_pos(&mut c, &a), None, "an unused slot is no hero");
		c.mem.wr8(s + 0x20, 0);
		c.mem.wr32(s + 0x14, f32::NAN.to_bits());
		assert_eq!(hero_pos(&mut c, &a), None, "NaN position");
	}

	#[test]
	fn fields_count_as_following_for_two_fields_only() {
		assert!(!recent(0, 5), "never ran");
		assert!(recent(11, 10), "ran in field 10");
		assert!(recent(11, 12) && recent(11, 12 + 0), "two fields later");
		assert!(!recent(11, 13), "three fields of silence: pause, menu or cutscene");
		assert!(!recent(11, 9), "a window older than the hand's last field is not 'recent'");
	}

	#[test]
	fn the_mouse_turns_the_camera_and_an_untouched_record_continues() {
		let mut st = State::new();
		let mut p = NoWalls;
		// Field 0: the first record is nobody's: a gap (the camera seeds from retail).
		st.gap(&[0; 12]);
		assert_eq!(st.gaps, 1);
		let a = st.run(&field(0, [0, 0]), &mut p).expect("a pose");
		let w0 = words_of(&a);
		st.last = Some(w0);
		let yaw0 = st.cam.yaw();
		assert!(st.cam.live());
		// Field 1: the record is what the hand wrote: no gap, the camera continues; 100 counts to the right turn it.
		st.gap(&w0);
		assert_eq!(st.gaps, 1, "bit-equal record: no gap");
		let b = st.run(&field(1, [100, 0]), &mut p).expect("a pose");
		assert_ne!(b.rows[0], a.rows[0], "the mouse turned the view");
		assert!((st.cam.yaw() - yaw0).abs() > 0.05, "yaw {} -> {}", yaw0, st.cam.yaw());
		// The pose stays on the orbit: the eye is the boom away from the hero's pivot.
		assert!(len3(sub(b.eye, [100.0, 50.0, 10.9])) > 1.5);
	}

	#[test]
	fn a_changed_record_ends_the_camera_and_a_new_level_resets_it() {
		let mut st = State::new();
		let mut p = NoWalls;
		st.level((0, 0x42b8d0));
		let w = words_of(&st.run(&field(0, [0, 0]), &mut p).unwrap());
		st.last = Some(w);
		st.gap(&w);
		assert_eq!((st.gaps, st.cam.live()), (0, true));
		let mut other = w;
		other[9] ^= 1;
		let changed = st.gap(&other);
		assert_eq!((st.gaps, changed), (1, true));
		// The follow camera was away (silent) and the record was rewritten: another camera class was current, the pose is stale.
		let v = st.judge(changed, true, 0);
		assert_eq!((st.cam.live(), v.end, v.retail_drives), (false, true, false), "away: end, re-seed");
		st.run(&field(1, [0, 0]), &mut p);
		assert!(st.cam.live(), "seeded again from retail's pose");
		st.level((0, 0x42b8d0));
		assert_eq!(st.resets, 1, "the same key is not a new level");
		st.level((1, 0x42f118));
		assert_eq!((st.resets, st.last), (2, None));
		assert!(!st.cam.live(), "a new level forgets the camera");
	}

	/// `field(n, ..)` with retail's pose moved to `far` and the verdict applied: one field of the hand-back loop the hand runs.
	fn handed(st: &mut State, n: u32, far: Pose, changed: bool, blend_word: u16) -> (bool, Option<Pose>) {
		let v = st.judge(changed, false, blend_word);
		let mut f = field(n, [0, 0]);
		f.retail = far;
		f.retail_drives = v.retail_drives;
		let out = st.run(&f, &mut NoWalls);
		blend::settle(&mut st.latch, st.cam.blending());
		(v.retail_drives, out)
	}

	#[test]
	fn a_rewritten_record_blends_to_retails_pose_without_a_snap_and_modern_continues_from_it() {
		let mut st = State::new();
		st.level((0, 0x42b8d0));
		// Two Modern fields: a Modern pose is on screen (a rewrite of the bare seed seeds again instead, `modcam_blend::Why::Placed`).
		st.run(&field(0, [0, 0]), &mut NoWalls).unwrap();
		let a = st.run(&field(1, [0, 0]), &mut NoWalls).unwrap();
		st.last = Some(words_of(&a));
		// Another follow record is current and retail's pose is 6 m away from the Modern one.
		let far = pose([a.eye[0] + 6.0, a.eye[1], a.eye[2]]);
		let (mut prev, mut worst, mut n) = (a.eye, 0.0f32, 2u32);
		loop {
			let (drives, out) = handed(&mut st, n, far, n == 2, 0);
			if !drives {
				let next = out.expect("Modern seeds from retail's pose").eye;
				assert!(len3(sub(next, far.eye)) < 0.6, "the Modern camera starts where retail's pose is: {next:?} vs {:?}", far.eye);
				assert!(len3(sub(next, prev)) < 0.6, "no snap between the last blended pose and the first Modern one");
				break;
			}
			let e = out.expect("a blended pose").eye;
			worst = worst.max(len3(sub(e, prev)));
			prev = e;
			n += 1;
			assert!(n < 100, "the hand-back never ended");
		}
		assert_eq!(n - 2, crate::modcam_pose::BLEND, "the blend takes BLEND fields");
		assert!(len3(sub(prev, far.eye)) < 1e-3, "it ends on retail's pose: {prev:?}");
		// `Cam::hand_back` blends from the previous blended pose each field (R&C1's way), so the steps exceed a single smoothstep's 0.45.
		assert!(worst < 1.2, "a 6 m offset moves less than 1.2 m a field, was {worst}");
		assert!(!st.latch && st.handbacks == 1);
	}

	#[test]
	fn a_switch_to_clank_blends_to_retails_pose_on_him_and_modern_follows_him_after() {
		use crate::modcam_who::Body;
		let mut st = State::new();
		st.level((0, 0x42b8d0));
		let ratchet = Who { moby: 0x1c2c680, body: Body::Ratchet, pos: [100.0, 50.0, 10.0] };
		assert!(!st.switch(ratchet, 0), "the first field knows nobody before");
		let a = st.run(&field(0, [0, 0]), &mut NoWalls).unwrap();
		st.last = Some(words_of(&a));
		assert!(!st.switch(ratchet, 1), "the same moby is no switch");
		// Clank 12 m away (more than a teleport `JUMP`); retail's camera already looks at him from 3.5 m.
		let clank = Who { moby: 0x1c30580, body: Body::Clank, pos: [112.0, 50.0, 10.0] };
		let near = pose([clank.pos[0] - 3.5, clank.pos[1], clank.pos[2] + 2.0]);
		assert!(st.switch(clank, 1));
		let dist = len3(sub(near.eye, a.eye));
		let (mut prev, mut worst, mut n) = (a.eye, 0.0f32, 1u32);
		loop {
			let v = st.judge(false, false, 0);
			let mut f = field(n, [0, 0]);
			(f.hero, f.retail, f.retail_drives) = (clank.pos, near, v.retail_drives);
			let e = st.run(&f, &mut NoWalls).expect("a pose every field: no frame of retail's raw pose, no gap").eye;
			blend::settle(&mut st.latch, st.cam.blending());
			worst = worst.max(len3(sub(e, prev)));
			prev = e;
			if !f.retail_drives {
				assert!(len3(sub(e, near.eye)) < 0.6, "Modern seeds where retail's pose on Clank is: {e:?} vs {:?}", near.eye);
				break;
			}
			n += 1;
			assert!(n < 100, "the hand-back never ended");
		}
		assert_eq!(n - 1, crate::modcam_pose::BLEND, "the switch blends over BLEND fields");
		assert!(worst < 0.25 * dist, "no snap: the largest step {worst} of {dist} m");
		// From now on the Modern camera orbits Clank.
		assert!(len3(sub(st.cam.pivot(), [112.0, 50.0, 10.9])) < 0.05, "pivot {:?}", st.cam.pivot());
		let b = st.run(&{ let mut f = field(n + 1, [0, 0]); f.hero = [112.5, 50.0, 10.0]; f }, &mut NoWalls).unwrap();
		assert!(len3(sub(b.eye, prev)) < 0.6, "the next field continues smoothly");
	}

	#[test]
	fn a_cold_start_seeds_with_the_orbits_rows_and_a_later_take_over_fades_from_retails_view() {
		// Retail's rows (`field`: level, along +x) look 0.29 rad above the eye -> pivot line.
		let mut st = State::new();
		st.level((0, 0x42b8d0));
		let at = |st: &mut State, n: u32| {
			st.cold(n);
			st.run(&field(n, [0, 0]), &mut NoWalls).unwrap()
		};
		let apart = |x: &Rows, y: &Rows| (0..9).map(|k| (x[k / 3][k % 3] - y[k / 3][k % 3]).abs()).fold(0.0f32, f32::max);
		let shown = field(0, [0, 0]).retail.rows;
		// The level's first seed (a load) and the fields after it: the orbit's rows at once, as build 43.
		let (a, b) = (at(&mut st, 1), at(&mut st, 2));
		assert!(apart(&a.rows, &shown) > 0.1, "a cold start takes the orbit's rows at once: {:?}", a.rows);
		assert!(apart(&a.rows, &b.rows) < 1e-5, "and keeps them: no fade");
		// A re-seed right after it (retail placing its camera) is still cold.
		st.cam.end();
		let p = at(&mut st, 1 + COLD - 1);
		assert!(apart(&p.rows, &a.rows) < 1e-5, "a placement within COLD fields seeds cold too");
		// A later take-over (retail drove meanwhile: a respawn, a cutscene end) shows retail's view first, then fades.
		st.cam.end();
		let (c, d) = (at(&mut st, 40), at(&mut st, 41));
		assert!(apart(&c.rows, &shown) < 1e-4, "a later take-over shows retail's view first: {:?}", c.rows);
		assert!(apart(&d.rows, &c.rows) > 1e-4, "then fades to the orbit's");
		assert!(len3(sub(a.eye, c.eye)) < 1e-5, "the eye is the same either way");
		// A new level starts cold again.
		st.level((1, 0x42b8d0));
		assert!(apart(&at(&mut st, 60).rows, &a.rows) < 1e-5, "a new level's first seed is cold");
	}

	#[test]
	fn retails_switch_blend_keeps_a_dead_camera_out_and_hands_a_live_one_back() {
		let mut st = State::new();
		st.level((0, 0x42b8d0));
		let far = pose([90.0, 50.0, 14.0]);
		// Nothing Modern on screen yet: retail's pose is left alone while its blend runs (no pose, no seed).
		for (n, b) in [(0u32, 1u16), (1, 2), (2, 3)] {
			let (drives, out) = handed(&mut st, n, far, false, b);
			assert!(drives && out.is_none() && !st.cam.live(), "blend {b}");
		}
		// The blend is over: Modern seeds from the record.
		let (drives, out) = handed(&mut st, 3, far, false, 0);
		assert!(!drives && out.is_some() && st.cam.live());
		// Live and a switch blend starts: hand back (a pose that moves toward retail's), one count.
		let (drives, out) = handed(&mut st, 4, far, false, 1);
		assert!(drives && out.is_some() && st.handbacks == 2);
	}

	#[test]
	fn a_pause_with_an_untouched_record_resumes_the_modern_camera() {
		let mut st = State::new();
		st.level((0, 0x42b8d0));
		let a = st.run(&field(0, [0, 0]), &mut NoWalls).unwrap();
		st.last = Some(words_of(&a));
		// 300 silent fields later: the record is bit-equal, no blend: not away, no hand-back, still live.
		let changed = st.gap(&words_of(&a));
		let v = st.judge(changed, true, 0);
		assert_eq!((v.end, v.retail_drives, st.cam.live(), st.handbacks), (false, false, true, 0));
	}

	#[test]
	fn a_jump_or_fall_is_airborne_only_between_consecutive_fields() {
		let (h0, h1) = ([0.0, 0.0, 10.0], [0.0, 0.0, 10.3]);
		assert!(airborne(Some((4, h0)), 5, h1), "0.3 m up in one field = 15 m/s");
		assert!(airborne(Some((4, h1)), 5, h0), "a fall counts too");
		assert!(!airborne(Some((4, h0)), 5, [0.0, 0.0, 10.05]), "a slope is not a jump");
		assert!(!airborne(Some((4, h0)), 9, h1), "a field gap says nothing");
		assert!(!airborne(None, 5, h1));
	}
}
