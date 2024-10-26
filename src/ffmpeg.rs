use std::fmt::Display;
use std::io::Error;
use std::path::{PathBuf, Path};
use std::process::{Child, Output, Stdio};
use std::str::from_utf8;
use anyhow::Context;
use which::which;
use std::sync::OnceLock;

pub static FFMPEG_PATH: OnceLock<&Path> = OnceLock::new();
pub static FFPROBE_PATH: OnceLock<&Path> = OnceLock::new();

#[derive(Debug)]
pub enum FFmpegError<'a> {
    ChildError(&'a Error),
    OutputError(&'a str),
}

impl<'a> Display for FFmpegError<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { match self {
        FFmpegError::ChildError(child_err) => {
            writeln!(f, "Failed to execute ffmpeg: {child_err}") 
        },
        FFmpegError::OutputError(output_err) => {
            let width = output_err.split('\n')
                .reduce(|acc, x| if x.len() > acc.len() { x } else { acc })
                .unwrap_or("")
                .len()
                .saturating_sub(3);

            writeln!(f, "+{:-^1$}+", " Begin ffmpeg stderr ", width)?;
            writeln!(f, "{}", output_err.trim_end())?;
            write!(f, "+{:-^1$}+", " End ffmpeg stderr ", width)
        },
    }}
}

#[derive(Debug)]
pub struct FFmpegOptions {
    pub input_file: PathBuf,
    pub output_file: PathBuf,
    pub allow_override: bool,
    pub duration: Option<f64>,
    pub str_options: Vec<String>,
}

impl FFmpegOptions {
    pub fn new(input_file: PathBuf, output_file: PathBuf, allow_override: bool, options: Vec<String>) -> Self {
        let duration = get_duration(&input_file).unwrap_or(None); 

        Self { 
            input_file, 
            output_file, 
            allow_override, 
            duration,
            str_options: options, 
        }
    }

    pub fn start(self) -> FFmpegProcessStarted {
        FFmpegProcessStarted {
            child: spawn_ffmpeg(&self),
            options: self,
        }
    }
}

pub struct FFmpegProcessStarted {
    pub child: Result<Child, Error>,
    pub options: FFmpegOptions,
}

impl FFmpegProcessStarted {
    pub fn finish(self) -> FFmpegProcessCompleted {
        FFmpegProcessCompleted {
            output: match self.child {
                Ok(child) => child.wait_with_output(),
                Err(err) => Err(err),
            },
            options: self.options,
        }
    }
}

pub struct FFmpegProcessCompleted {
    pub output: Result<Output, Error>,
    pub options: FFmpegOptions,
}

impl FFmpegProcessCompleted {
    pub fn get_error(&self) -> Option<FFmpegError> {
        if let Err(err) = &self.output {
            return Some(FFmpegError::ChildError(err));
        }

        let error_message = from_utf8(
            &self.output.as_ref().ok().unwrap().stderr
        ).expect("Non utf-8 characters in output");

        if error_message.is_empty() {
            return None;
        } else {
            return Some(FFmpegError::OutputError(error_message.trim_end()));
        }
    }
}

pub fn get_duration(file_path: &PathBuf) -> Result<Option<f64>, anyhow::Error> {
    let child = std::process::Command::new(FFPROBE_PATH.get().expect("Initialized this in main"))
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(file_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output()?;
    let output_str = from_utf8(&output.stdout)?.trim();

    match output_str.parse::<f64>() {
        Ok(duration) => { Ok(Some(duration))},
        Err(_) => { Ok(None) },
    }
}

pub fn spawn_ffmpeg(options: &FFmpegOptions) -> Result<Child, Error> {
    let child = std::process::Command::new(FFMPEG_PATH.get().expect("Initialized this in main"))
        .arg("-hide_banner")
        .arg(if options.allow_override {"-y"} else {"-n"})
        .args(["-loglevel", "error", "-progress", "-", "-nostats"])
        .arg("-i")
        .arg(&options.input_file)
        .args(options.str_options.iter())
        .arg(&options.output_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    Ok(child)
}

pub fn assert_exists(executable: &Path) -> Result<PathBuf, anyhow::Error> {
    Ok(which(executable)
        .with_context(|| format!("'{}' could not be found! Make sure to add '/path/to/ffmpeg/bin' to the PATH variable", executable.display()))?
    )
}
