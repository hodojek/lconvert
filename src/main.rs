mod progress;
mod ffmpeg;
mod parser;

use std::{env::current_dir, fs::{create_dir_all, read_dir, DirEntry}, io::{BufRead, BufReader}, path::{absolute, PathBuf}, time::Instant};
use anyhow::Context;
use clap::Parser;
use progress::{FFmpegProgress, OverallProgress};
use ffmpeg::{assert_exists, FFmpegOptions, FFmpegProcessCompleted, FFmpegProcessStarted};
use parser::{Arguments, ExtensionMap};

struct FFmpegProcessWithProgress {
    process: FFmpegProcessStarted,
    progress: FFmpegProgress,
}

impl FFmpegProcessWithProgress {
    pub fn finish(self) -> FFmpegProcessCompleted {
        self.progress.finish();
        self.process.finish()
    }
}

fn create_output_directory(output_directory: &Option<PathBuf>) -> Result<PathBuf, anyhow::Error> {
    let directory = match output_directory {
        Some(dir) => { 
            absolute(dir).with_context(|| format!("could not get absolute path of directory: '{}'", dir.display()))?
        },
        None => { 
            let cwd = absolute(
                current_dir().with_context(|| "could not get current directory")?
            ).with_context(|| "could not get absolute path of current directory")?;

            let mut name = "lconvert_output".to_string();
            let mut dir = cwd.join(&name);
            let mut i: u32 = 0;

            while dir.exists() {
                i += 1;
                name = format!("lconvert_output_{i}");
                dir = cwd.join(&name);
            }
            dir
        },
    };
    create_dir_all(&directory).with_context(|| format!("could not create directory: '{}'", directory.display()))?;
    Ok(directory)
}

fn get_ffmpeg_options(
    input_files: &Vec<PathBuf>, 
    output_directory: &PathBuf, 
    extension_map: &ExtensionMap, 
    ffmpeg_str_options: &Vec<String>,
    case_sensitive: bool, 
    allow_override: bool,
) -> Result<Vec<FFmpegOptions>, anyhow::Error> {
    let mut ffmpeg_options: Vec<FFmpegOptions> = Vec::new();

    for input_file in input_files {
        if input_file.is_dir() {
            ffmpeg_options.extend(
                get_ffmpeg_options(
                    &read_dir(input_file)
                        .with_context(|| format!("cound not read directory: '{}'", input_file.display()))?
                        .collect::<Result<Vec<DirEntry>, _>>()
                        .with_context(|| format!("error while reading directory: '{}'", input_file.display()))?
                        .into_iter()
                        .map(|x| input_file.join(x.path()))
                        .collect(),
                    &output_directory.join(
                        input_file
                        .file_name()
                        .with_context(|| format!("could not get file_name for some reason!?: '{}'", input_file.display()))?
                    ), 
                    extension_map,
                    ffmpeg_str_options,
                    case_sensitive,
                    allow_override,
                )?
            );
            continue;
        }

        let mut input_extension = input_file
                .extension()
                .with_context(|| format!("file has no extension: '{}'", input_file.display()))?
                .to_str()
                .with_context(|| format!("file has REALLY fucked up extension: '{}'", input_file.display()))?;

        if !case_sensitive { for key in extension_map.keys() { 
            if key.to_lowercase().eq(&input_extension.to_lowercase()) {
                input_extension = key;
                break; 
            }
        }}

        if !extension_map.contains_key(input_extension) {
            if extension_map.contains_key("*") {
                input_extension = "*";
            } else {
                continue;
            }
        }

        let mut output_file = output_directory.join(
            input_file.file_name().with_context(|| format!("could not get file_name for some reason!?: '{}'", input_file.display()))?
        );

        output_file.set_extension(&extension_map[input_extension]);

        ffmpeg_options.push(FFmpegOptions::new(
            input_file.clone(), 
            output_file, 
            allow_override, 
            ffmpeg_str_options.clone()
        ))
    }
    return Ok(ffmpeg_options);
}

fn create_hierarchy(ffmpeg_options: &Vec<FFmpegOptions>) -> Result<(), anyhow::Error>{
    for ffmpeg_option in ffmpeg_options{
        create_dir_all(
            ffmpeg_option.output_file.parent().with_context(|| format!("could not get parent of file: '{}'", ffmpeg_option.output_file.display()))?
        ).with_context(|| format!("could not create directory hierarchy: '{}'", ffmpeg_option.output_file.parent().unwrap().display()))?;
    }
    Ok(())
}

fn update_processes_until_one_finishes<'a>(processes: &'a mut Vec<FFmpegProcessWithProgress>) -> FFmpegProcessCompleted {
    loop { for (i, process_with_progress) in processes.iter_mut().enumerate() {
        match &mut process_with_progress.process.child {
            Ok(child) => { match child.try_wait() { // Child exists
                Ok(None) => { // Child is working
                    let progress = &mut process_with_progress.progress;

                    if !progress.has_duration {
                        progress.update(None);
                        continue;
                    }

                    let output = child.stdout.as_mut().unwrap();
                    let reader = BufReader::new(output);

                    for result in reader.lines() { if let Ok(line) = result {
                        if line.contains("out_time_ms") {
                            progress.update(Some(&line));
                            break;
                        }
                    }}
                },
                _ => { // Child finished or error attemting to acces
                    return processes.remove(i).finish();
                }
            }},
            Err(_) => { // Child did not start
                return processes.remove(i).finish(); 
            },
        }
    }}
}

fn run_ffmpeg_concurrent(mut ffmpeg_options: Vec<FFmpegOptions>, n_subprocesses: u32) -> Vec<FFmpegProcessCompleted> {
    let overall_progress = OverallProgress::new(ffmpeg_options.len() as u64);

    let mut started_processes: Vec<FFmpegProcessWithProgress> = Vec::new();
    let mut completed_processes: Vec<FFmpegProcessCompleted> = Vec::new();

    while let Some(ffmpeg_options) = ffmpeg_options.pop() {
        // if more processes than limit, wait until one finishes
        if started_processes.len() as u32 >= n_subprocesses { 
            let completed_process = update_processes_until_one_finishes(&mut started_processes);
            overall_progress.update(&completed_process.get_error());
            completed_processes.push(completed_process);
        }

        started_processes.push(FFmpegProcessWithProgress {
            progress: FFmpegProgress::new(&overall_progress, &ffmpeg_options),
            process: ffmpeg_options.start(),
        });
    }

    while !started_processes.is_empty() {
        let completed_process = update_processes_until_one_finishes(&mut started_processes);
        overall_progress.update(&completed_process.get_error());
        completed_processes.push(completed_process);
    }

    overall_progress.finish();
    completed_processes
}

fn main() -> anyhow::Result<()> {
    let args: Arguments = Arguments::parse();

    assert_exists("ffmpeg")?;
    assert_exists("ffprobe")?;

    let start_time = Instant::now();

    let input_files: Vec<PathBuf> = args.get_glob_expanded_input_files();

    let output_directory: PathBuf = create_output_directory(
        &args.output_directory
    )?;

    let ffmpeg_options: Vec<FFmpegOptions> = get_ffmpeg_options(
        &input_files, 
        &output_directory, 
        &args.extension_map, 
        &args.ffmpeg_str_options,
        args.case_sensitive, 
        args.allow_override, 
    )?;

    create_hierarchy(
        &ffmpeg_options
    )?;

    println!("Total files      :  {}", ffmpeg_options.len());
    println!("Output directory : '{}'", output_directory.display());

    let completed_processes: Vec<FFmpegProcessCompleted> = run_ffmpeg_concurrent(ffmpeg_options, args.n_subprocesses);

    println!("\nDone in {:.1?}!\n", Instant::now().duration_since(start_time));

    for completed_process in completed_processes.iter() { 
        if let Some(err) = completed_process.get_error() { 
            eprintln!("┌ Error while trying to process input file: '{}'", completed_process.options.input_file.display());
            eprintln!("{err}");
            eprintln!();
        }
    }

    if completed_processes.iter().any(|x| x.get_error().is_some()) {
        eprintln!(
            "{}/{} files finished with errors!", 
            completed_processes.iter().filter(|x| x.get_error().is_some()).count(),
            completed_processes.len(), 
        );
    }

    if cfg!(target_os = "linux") {
        // FIXME: For some reason on linux after the prgram is done character echo is disabled
        // This fixes it but will need to find why it happens
        std::process::Command::new("stty").arg("echo").spawn()?.wait()?;
    }
    Ok(())
}
