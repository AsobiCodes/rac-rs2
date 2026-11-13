//! Disc test of the engine's movie decoder on R&C2 movies (stage E6): movie 0 and the intro cutscene (table entry 21) are read
//! through RC2.HDR's MPEG table, 30 video frames and some English audio are decoded, PNGs go to runs/movie_test/.
use rac_engine::fmv::adpcm_stream::{parse_header, AdpcmDecoder};
use rac_engine::fmv::mpeg2::Decoder;
use rac_engine::fmv::pss::{Packet, PssDemuxer};
use rac_formats::iso::Disc;

const ISO: &str = "E:/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It)/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It).iso";
const OUT: &str = "E:/ratchet/scratch_perf_engine/runs/movie_test";

/// (absolute sector, bytes) of table entry `i`: RC2.HDR (sector 0x3e9) MPEG section: word 1 = base sector, 16-byte records from
/// word 2 = {sector, bytes} of a subtitle script, {sector, bytes} of the PSS.
fn entry(disc: &mut Disc, i: usize) -> (u32, u32) {
	let h = disc.read_bytes(0x3e9, 11 * 2048).unwrap();
	let w = |k: usize| u32::from_le_bytes(h[k * 4..k * 4 + 4].try_into().unwrap());
	(w(1) + w(2 + i * 4 + 2), w(2 + i * 4 + 3))
}

fn png(f: &rac_engine::fmv::mpeg2::Frame) -> Vec<u8> {
	let (w, h, cw) = (f.width as usize, f.height as usize, (f.width as usize).div_ceil(2));
	let mut rgb = Vec::with_capacity(w * h * 3);
	for y in 0..h {
		for x in 0..w {
			let yy = (f.y[y * w + x] as f32 - 16.0) * 1.164;
			let (cb, cr) = (f.cb[(y / 2) * cw + x / 2] as f32 - 128.0, f.cr[(y / 2) * cw + x / 2] as f32 - 128.0);
			rgb.extend([(yy + 1.596 * cr).clamp(0.0, 255.0) as u8, (yy - 0.392 * cb - 0.813 * cr).clamp(0.0, 255.0) as u8, (yy + 2.017 * cb).clamp(0.0, 255.0) as u8]);
		}
	}
	crate::render::png(&crate::render::Image { w, h, rgb })
}

/// Run: cargo test --release --lib -p rac2 -p rac-engine decode_r2_movies -- --ignored --nocapture
#[test]
#[ignore]
fn decode_r2_movies() {
	let mut disc = Disc::open(std::path::Path::new(&std::env::var("RAC2_ISO").unwrap_or(ISO.into()))).unwrap();
	std::fs::create_dir_all(OUT).unwrap();
	for (idx, want_bytes) in [(0usize, 24_674_308u32), (21, 120_504_324)] {
		let (lba, bytes) = entry(&mut disc, idx);
		assert_eq!(bytes, want_bytes, "table entry {idx}");
		let data = disc.read_bytes(lba as u64, 8 << 20).unwrap();
		let mut dm = PssDemuxer::new(std::io::Cursor::new(data));
		let (mut dec, mut frames, mut pcm, mut ad) = (Decoder::new(), Vec::new(), Vec::new(), None);
		while frames.len() < 30 {
			match dm.next_packet().unwrap() {
				Some(Packet::Video { pts, data, .. }) => {
					dec.push(&data, pts);
					while let Some(f) = dec.next_frame() {
						frames.push(f);
					}
				}
				Some(Packet::Audio { track: 0, data, .. }) => {
					let mut b = &data[..];
					if ad.is_none() {
						let (h, n) = parse_header(b).expect("SShd header");
						assert_eq!((h.codec, h.rate, h.channels, h.interleave), (0x10, 48000, 2, 32));
						b = &b[n..];
						ad = Some(AdpcmDecoder::new(h.channels as usize, h.interleave));
					}
					ad.as_mut().unwrap().push(b, &mut pcm);
				}
				Some(_) => {}
				None => break,
			}
		}
		assert!(frames.len() >= 30, "movie {idx}: {} frames", frames.len());
		assert_eq!((frames[0].width, frames[0].height, frames[0].frame_rate_code), (512, 416, 3));
		assert!(pcm.iter().any(|&s| s.unsigned_abs() > 500), "movie {idx}: audio is not silent");
		for n in [0, 15, 29] {
			std::fs::write(format!("{OUT}/movie{idx}_f{n:02}.png"), png(&frames[n])).unwrap();
		}
		println!("movie {idx}: lba {lba} bytes {bytes}: {} frames, {} pcm samples, errors {}", frames.len(), pcm.len(), dec.errors);
	}
}
