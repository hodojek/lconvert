use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use assert_fs::prelude::*; 
use std::fs::File;
use std::io::Read;
use std::process::Command;
use std::path::PathBuf;

static BIN_NAME: &str = "lconvert";
static TEST_FILE_MP3: &str = "input.mp3";
static TEST_FILE_OGG: &str = "input.OGG";

macro_rules! get_test_file {($fname:expr) => (
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("resources").join($fname).as_path() 
)}

macro_rules! read_dir {($fname:expr) => (
    $fname.read_dir()?.collect::<Result<Vec<_>, _>>()?.iter().map(|x| x.path().strip_prefix($fname.path()).unwrap().to_owned()).collect::<Vec<_>>()
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

    cmd.args(["-o", output_dir.to_str().unwrap(), "-m", "mp3", input_dir.to_str().unwrap()]);

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
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3,mp3", input_dir.to_str().unwrap()])
        .assert()
        .failure();

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", ",mp3,jpeg=png", input_dir.to_str().unwrap()])
        .assert()
        .failure();

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "png=", input_dir.to_str().unwrap()])
        .assert()
        .failure();

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3=mp4,mp3,jpeg=png", input_dir.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3=mp4,jpeg=png", input_dir.to_str().unwrap()])
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
    input_file.write_file(get_test_file!(TEST_FILE_MP3))?;

    Command::new("ffmpeg")
        .args(["-i", input_file.to_str().unwrap(), "-ss", "5", "-ab", "64KB", output_file.to_str().unwrap()])
        .assert()
        .success();

    let expected_hash = hash!(&output_file);

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3=mp3", "-y", input_file.to_str().unwrap(), "--", "-ab", "64KB", "-ss", "5"])
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
    output_file.write_file(get_test_file!(TEST_FILE_MP3))?;

    let input_file = input_dir.child("input.mp3");
    input_file.write_file(get_test_file!(TEST_FILE_MP3))?;

    let expected_hash = hash!(&output_file);

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3=wav", input_file.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("already exists"));

    assert_eq!(expected_hash, hash!(&output_file));

    let unexpected_hash = hash!(&output_file);

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3=wav", input_file.to_str().unwrap(), "-y"])
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
    input_file.write_file(get_test_file!(TEST_FILE_OGG))?;

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "ogg=MP3", input_file.to_str().unwrap()])
        .assert()
        .success();

    assert!(output_dir.read_dir()?.all(|x| x.unwrap().file_name().eq("input.MP3")));

    let output_dir = assert_fs::TempDir::new()?;

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "ogg=mp3", input_file.to_str().unwrap(), "--case-sensitive"])
        .assert()
        .success();

    assert!(!output_dir.read_dir()?.any(|x| x.unwrap().file_name().eq("input.mp3")));

    let output_dir = assert_fs::TempDir::new()?;

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "OGG=mp3", input_file.to_str().unwrap(), "--case-sensitive"])

        .assert()
        .success();

    assert!(output_dir.read_dir()?.all(|x| x.unwrap().file_name().eq("input.mp3")));

    Ok(())
}

#[test]
fn assert_ffmpeg_exists() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;

    let input_file = input_dir.child("input.mp3");
    input_file.write_file(get_test_file!(TEST_FILE_MP3))?;

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3=mp3", "-y", input_file.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin(BIN_NAME)?
        .env("PATH", ".")
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3=mp3", "-y", input_file.to_str().unwrap()])
        .assert()
        .failure();

    Command::cargo_bin(BIN_NAME)?
        .env_remove("PATH")
        .args(["-o", output_dir.to_str().unwrap(), "-m", "mp3=mp3", "-y", input_file.to_str().unwrap()])
        .assert()
        .failure();

    Ok(())
}

#[test]
fn convert_multiple_files() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;

    let output_file_1 = output_dir.child("input1.wav");
    let output_file_2 = output_dir.child("input2.wav");
    let output_file_3 = output_dir.child("input3.wav");
    let input_file_1 = input_dir.child("input1.mp3");
    let input_file_2 = input_dir.child("input2.mp3");
    let input_file_3 = input_dir.child("input3.OGG");
    input_file_1.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_2.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_3.write_file(get_test_file!(TEST_FILE_OGG))?;

    Command::new("ffmpeg")
        .args(["-i", input_file_1.to_str().unwrap(), "-ss", "5", "-ab", "64KB", &output_file_1.to_string_lossy()])
        .assert()
        .success();
    Command::new("ffmpeg")
        .args(["-i", input_file_2.to_str().unwrap(), "-ss", "5", "-ab", "64KB", &output_file_2.to_string_lossy()])
        .assert()
        .success();
    Command::new("ffmpeg")
        .args(["-i", input_file_3.to_str().unwrap(), "-ss", "5", "-ab", "64KB", &output_file_3.to_string_lossy()])
        .assert()
        .success();

    let expected_hash_1 = hash!(&output_file_1);
    let expected_hash_2 = hash!(&output_file_2);
    let expected_hash_3 = hash!(&output_file_3);

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", &output_dir.to_string_lossy(), "-m", "wav", "-y", &input_dir.to_string_lossy(), "--", "-ab", "64KB", "-ss", "5"])
        .assert()
        .success();

    assert_eq!(hash!(&output_file_1), expected_hash_1);
    assert_eq!(hash!(&output_file_2), expected_hash_2);
    assert_eq!(hash!(&output_file_3), expected_hash_3);

    Ok(())
}

#[test]
fn output_pattern() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;
    let correct_dir = assert_fs::TempDir::new()?;

    let input_file_1 = input_dir.child("input1.mp3");
    let input_file_2 = input_dir.child("input2.mp3");
    let input_file_3 = input_dir.child("input3.OGG");
    input_file_1.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_2.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_3.write_file(get_test_file!(TEST_FILE_OGG))?;

    let _ = correct_dir.child("test").child("input1").child("wav").child("input1.wav").touch();
    let _ = correct_dir.child("test").child("input2").child("wav").child("input2.wav").touch();
    let _ = correct_dir.child("test").child("input3").child("wav").child("input3.wav").touch();

    Command::cargo_bin(BIN_NAME)?
        .args(["-o", &dbg!(format!("{}/test/{{{{stem}}}}/{{{{out-ext}}}}/{{{{file}}}}", &output_dir.to_string_lossy())), "-m", "wav", "-y", &input_dir.to_string_lossy()])
        .assert()
        .success();

    let test_output = read_dir!(output_dir);
    let test_correct = read_dir!(correct_dir);
    assert_eq!(dbg!(test_output), dbg!(test_correct));

    let test_output = read_dir!(output_dir.child("test"));
    let test_correct = read_dir!(correct_dir.child("test"));
    assert_eq!(dbg!(test_output), dbg!(test_correct));

    let test_output = read_dir!(output_dir.child("test").child("input1"));
    let test_correct = read_dir!(correct_dir.child("test").child("input1"));
    assert_eq!(dbg!(test_output), dbg!(test_correct));

    let test_output = read_dir!(output_dir.child("test").child("input1").child("wav"));
    let test_correct = read_dir!(correct_dir.child("test").child("input1").child("wav"));
    assert_eq!(dbg!(test_output), dbg!(test_correct));

    Ok(())
}

#[test]
fn output_pattern_with_no_templates() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;
    let correct_dir = assert_fs::TempDir::new()?;

    let input_file_1 = input_dir.child("test").child("input1.mp3");
    let input_file_2 = input_dir.child("test").child("input2.mp3");
    let input_file_3 = input_dir.child("test").child("input3.OGG");
    input_file_1.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_2.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_3.write_file(get_test_file!(TEST_FILE_OGG))?;

    let _ = correct_dir.child("dir").child(input_dir.path()).child("test").child("input1.wav").touch();
    let _ = correct_dir.child("dir").child(input_dir.path()).child("test").child("input2.wav").touch();
    let _ = correct_dir.child("dir").child(input_dir.path()).child("test").child("input3.wav").touch();

    Command::cargo_bin(BIN_NAME)?
        .current_dir(&output_dir)
        .args(["-o", "dir", "-m", "wav", "-y", &input_dir.to_string_lossy()])
        .assert()
        .success();

    let test_output = read_dir!(output_dir.child("dir").child(input_dir.path()).child("test"));
    let test_correct = read_dir!(correct_dir.child("dir").child(input_dir.path()).child("test"));
    assert_eq!(dbg!(test_output), dbg!(test_correct));

    Ok(())
}

#[test]
fn output_pattern_default() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;
    let output_dir = assert_fs::TempDir::new()?;
    let correct_dir = assert_fs::TempDir::new()?;

    let input_file_1 = input_dir.child("test").child("input1.mp3");
    let input_file_2 = input_dir.child("test").child("input2.mp3");
    let input_file_3 = input_dir.child("test").child("input3.OGG");
    input_file_1.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_2.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_3.write_file(get_test_file!(TEST_FILE_OGG))?;

    let _ = correct_dir.child("lconvert_output").child(output_dir.path()).child("test").child("input1.wav").touch()?;
    let _ = correct_dir.child("lconvert_output").child(output_dir.path()).child("test").child("input2.wav").touch()?;
    let _ = correct_dir.child("lconvert_output").child(output_dir.path()).child("test").child("input3.wav").touch()?;

    Command::cargo_bin(BIN_NAME)?
        .current_dir(&output_dir)
        .args(["-m", "wav", "-y", &dbg!(input_dir.to_string_lossy())])
        .assert()
        .success();

    let test_correct = read_dir!(correct_dir.child("lconvert_output").child(output_dir.path()).child("test"));
    let test_output = read_dir!(output_dir.child("lconvert_output").child(output_dir.path()).child("test"));
    assert_eq!(dbg!(test_output), dbg!(test_correct));

    Ok(())
}

#[test]
fn output_pattern_unique() -> Result<(), Box<dyn std::error::Error>> {
    let input_dir = assert_fs::TempDir::new()?;

    let input_file_1 = input_dir.child("input1.mp3");
    let input_file_2 = input_dir.child("input2.mp3");
    let input_file_3 = input_dir.child("input3.OGG");
    input_file_1.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_2.write_file(get_test_file!(TEST_FILE_MP3))?;
    input_file_3.write_file(get_test_file!(TEST_FILE_OGG))?;

    // test directory does not exist
    let output_dir = assert_fs::TempDir::new()?;
    let correct_dir = assert_fs::TempDir::new()?;
    let _ = correct_dir.child("test").child("wav.wav").touch()?;
    let _ = correct_dir.child("test").child("wav_1.wav").touch()?;
    let _ = correct_dir.child("test").child("wav_2.wav").touch()?;

    Command::cargo_bin(BIN_NAME)?
        .current_dir(&output_dir)
        .args(["-o", "test{{unique-suffix}}/{{out-ext}}{{unique-suffix}}", "-m", "wav", "-y", &dbg!(input_dir.to_string_lossy())])
        .assert()
        .success();

    let test_correct = read_dir!(correct_dir.child("test"));
    let test_output = read_dir!(output_dir.child("test"));
    assert_eq!(dbg!(test_output), dbg!(test_correct));

    // test directory does exist
    let output_dir = assert_fs::TempDir::new()?;
    let _ = output_dir.child("test").create_dir_all()?;

    let correct_dir = assert_fs::TempDir::new()?;
    let _ = correct_dir.child("test").child("wav.wav").touch()?;
    let _ = correct_dir.child("test").child("wav_1.wav").touch()?;
    let _ = correct_dir.child("test").child("wav_2.wav").touch()?;

    Command::cargo_bin(BIN_NAME)?
        .current_dir(&output_dir)
        .args(["-o", "test{{unique-suffix}}/{{out-ext}}{{unique-suffix}}", "-m", "wav", "-y", &dbg!(input_dir.to_string_lossy())])
        .assert()
        .success();

    let test_correct = read_dir!(correct_dir.child("test"));
    let test_output = read_dir!(output_dir.child("test_1"));
    assert_eq!(dbg!(test_output), dbg!(test_correct));

    Ok(())
}

// TODO: Add more tests
