//! The CD 1.01 videos decode: every clip in a real `FMVs` folder (set
//! `FFE_FMV_ROOT` to the folder that contains `FMVs`). Local-only: the
//! videos are the player's own files and are never committed.

use ffe_rs::fmv::{Library, CD_PROFILE};
use std::path::PathBuf;

#[test]
#[ignore = "Local-only: needs the CD videos (FFE_FMV_ROOT)"]
fn every_cd_video_decodes_as_8_bit_dib_with_pcm_audio() {
    let root = PathBuf::from(std::env::var_os("FFE_FMV_ROOT").expect("FFE_FMV_ROOT"));
    let library = Library::find(&root, CD_PROFILE)
        .expect("the FMVs folder matches its manifest")
        .expect("an FMVs folder with videos");
    assert_eq!(library.len(), 323, "the CD's DATA folder has 323 clips");
    let mut seconds = 0.0;
    for name in library.names() {
        let video = library.load(name).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!((video.width, video.height), (120, 120), "{name}");
        assert_eq!(video.frame_micros, 66_666, "{name}: 15 frames a second");
        assert!(!video.frames.is_empty(), "{name}");
        let audio = video
            .audio
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: no audio"));
        assert_eq!(
            (audio.channels, audio.rate, audio.bits),
            (1, 22_050, 16),
            "{name}"
        );
        assert!(!audio.pcm.is_empty(), "{name}");
        seconds += video.seconds();
    }
    println!("{} clips, {seconds:.0} s of video", library.len());
}

/// With `FFE_FMV_DUMP` set to a folder, write a mid-clip frame of `G1`
/// through [`ffe_rs::fmv::Player::draw`] as a 320x200 PPM, to check the
/// picture's orientation and colours by eye.
#[test]
#[ignore = "Local-only: needs the CD videos (FFE_FMV_ROOT) and FFE_FMV_DUMP"]
fn dump_a_mid_clip_frame() {
    let root = PathBuf::from(std::env::var_os("FFE_FMV_ROOT").expect("FFE_FMV_ROOT"));
    let out = PathBuf::from(std::env::var_os("FFE_FMV_DUMP").expect("FFE_FMV_DUMP"));
    let library = Library::find(&root, CD_PROFILE).unwrap().unwrap();
    let mut player = ffe_rs::fmv::Player::new("G1", library.load("G1").unwrap());
    player.advance(4.0);
    let mut screen = vec![0u32; 320 * 200];
    player.draw(&mut screen, 320);
    let mut ppm = b"P6\n320 200\n255\n".to_vec();
    for pixel in screen {
        ppm.extend([(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8]);
    }
    std::fs::write(out.join("g1-frame.ppm"), ppm).unwrap();
}
