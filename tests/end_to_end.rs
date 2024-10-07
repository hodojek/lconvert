use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use assert_fs::prelude::*; 
use std::fs::File;
use std::io::Read;
use std::process::Command;
use std::path::PathBuf;

static BIN_NAME: &str = "lconvert";

macro_rules! get_test_file {($fname:expr) => (
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("resources").join($fname).as_path() 
)}

macro_rules! hash {($fname:expr) => {{
    let mut f = File::open($fname)?;
    let mut buffer: [u8; 32] = [0; 32];
    let mut hasher = blake3::Hasher::new();

    while f.read(&mut buffer).is_ok_and(|x| x != 0) {
        hasher.update(&buffer);
    }
    hasher.finalize()
}}}

#[test]
fn input_dir_doesnt_exist() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;

    let mut cmd = Command::cargo_bin(BIN_NAME)?;

    cmd.args(["-d", output_dir.to_str().unwrap(), "-m", "mp3", input_dir.to_str().unwrap()]);

    input_dir.close()?;

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("File Not Found"));

    Ok(())
}

#[test]
fn extension_map() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "mp3,mp3", input_dir.to_str().unwrap()])
        .assert()
        .failure();

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", ",mp3,jpeg=png", input_dir.to_str().unwrap()])
        .assert()
        .failure();

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "png=", input_dir.to_str().unwrap()])
        .assert()
        .failure();

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "mp3=mp4,mp3,jpeg=png", input_dir.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "mp3=mp4,jpeg=png", input_dir.to_str().unwrap()])
        .assert()
        .success();

    Ok(())
}

#[test]
fn convert_file() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;

    let output_file = output_dir.child("input.mp3");
    let input_file = input_dir.child("input.mp3");
    input_file.write_file(get_test_file!("input.mp3"))?;

    Command::new("ffmpeg")
        .args(["-i", input_file.to_str().unwrap(), "-ss", "5", "-ab", "64KB", output_file.to_str().unwrap()])
        .assert()
        .success();

    let expected_hash = hash!(&output_file);

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "mp3=mp3", "-y", input_file.to_str().unwrap(), "--", "-ab", "64KB", "-ss", "5"])
        .assert()
        .success();

    assert_eq!(hash!(&output_file), expected_hash);

    Ok(())
}

#[test]
fn override_file() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;

    let output_file = output_dir.child("input.wav");
    output_file.write_file(get_test_file!("input.mp3"))?;

    let input_file = input_dir.child("input.mp3");
    input_file.write_file(get_test_file!("input.mp3"))?;

    let expected_hash = hash!(&output_file);

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "mp3=wav", input_file.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("already exists"));

    assert_eq!(expected_hash, hash!(&output_file));

    let unexpected_hash = hash!(&output_file);

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "mp3=wav", input_file.to_str().unwrap(), "-y"])
        .assert()
        .success();

    assert_ne!(unexpected_hash, hash!(&output_file));

    Ok(())
}


#[test]
fn case_sensitivity() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;

    let input_file = input_dir.child("input.OGG");
    input_file.write_file(get_test_file!("input.OGG"))?;

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "ogg=MP3", input_file.to_str().unwrap()])
        .assert()
        .success();

    assert!(output_dir.read_dir()?.all(|x| x.unwrap().file_name().eq("input.MP3")));

    let output_dir = assert_fs::TempDir::new()?;

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "ogg=mp3", input_file.to_str().unwrap(), "--case-sensitive"])
        .assert()
        .success();

    assert!(!output_dir.read_dir()?.any(|x| x.unwrap().file_name().eq("input.mp3")));

    let output_dir = assert_fs::TempDir::new()?;

    Command::cargo_bin(BIN_NAME)?
        .args(["-d", output_dir.to_str().unwrap(), "-m", "OGG=mp3", input_file.to_str().unwrap(), "--case-sensitive"])
        .assert()
        .success();

    assert!(output_dir.read_dir()?.all(|x| x.unwrap().file_name().eq("input.mp3")));

    Ok(())
}

// TODO: Add more tests
