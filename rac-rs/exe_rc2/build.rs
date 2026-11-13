// [PC] The segment heap manifest, as exe/build.rs (the same file, ../exe/rac-rs.manifest).
fn main() {
	println!("cargo:rerun-if-changed=../exe/rac-rs.manifest");
	println!("cargo:rerun-if-changed=build.rs");
	if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
		let m = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../exe/rac-rs.manifest");
		embed_manifest::embed_manifest_file(&m).expect("embedding rac-rs.manifest");
	}
}
