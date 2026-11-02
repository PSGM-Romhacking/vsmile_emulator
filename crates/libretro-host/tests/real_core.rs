//! Runs a real libretro core through the host. Skipped unless `LIBRETRO_TEST_CORE` points
//! to a core (e.g. `libmil_capivaras_libretro.so`); `LIBRETRO_TEST_ROM` is optional (default:
//! 16 KiB of zeros). One core per process, hence a single test.

use std::path::PathBuf;

use libretro_host::HostConfig;

#[test]
fn plays_a_real_core() {
    let Some(core_path) = std::env::var_os("LIBRETRO_TEST_CORE") else {
        eprintln!("LIBRETRO_TEST_CORE not set: skipped");
        return;
    };
    let rom = std::env::var_os("LIBRETRO_TEST_ROM").map_or_else(|| vec![0u8; 0x4000], |p| std::fs::read(PathBuf::from(p)).unwrap());
    let tmp = std::env::temp_dir().join("libretro-host-test");
    let info = libretro_host::open(&PathBuf::from(core_path), HostConfig { system_dir: tmp.join("system"), save_dir: tmp.join("saves"), thumbnails: String::new() }).unwrap();
    assert!(!info.name.is_empty());
    assert!(!info.extensions.is_empty());
    eprintln!("{} {:?}", info.name, info.extensions);

    let mut core = (info.create)(48000);
    core.load(&rom).unwrap();
    for _ in 0..30 {
        core.run_frame(&[0, 0]);
    }
    let (vw, vh, aspect) = {
        let v = core.video();
        assert!(v.width > 0 && v.height > 0 && v.pixels.len() >= v.width * v.height, "{}x{}", v.width, v.height);
        (v.width, v.height, v.aspect)
    };
    assert!(aspect > 0.0);
    let (n, d) = core.fps();
    let fps = n as f64 / d as f64;
    assert!((20.0..200.0).contains(&fps), "fps {fps}");

    let mut audio = Vec::new();
    core.take_audio(&mut audio);
    eprintln!("{}x{} aspect {:.3} fps {fps:.3}, {} audio frames in the last take, {} options", vw, vh, aspect, audio.len() / 2, core.options().len());

    // save state round trip
    let state = core.save_state();
    assert!(!state.is_empty());
    core.run_frame(&[0, 0]);
    core.load_state(&state).unwrap();
    assert_eq!(core.save_state(), state);

    // a second game: the old core is dropped (deinit) and a new one initialised
    drop(core);
    let mut core = (info.create)(44100);
    core.load(&rom).unwrap();
    core.run_frame(&[0xFFFF, 0]);
    core.reset();
}
