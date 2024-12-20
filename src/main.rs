mod progress;
mod ffmpeg;
mod parser;

use std::{fs::{create_dir_all, read_dir, DirEntry}, io::{BufRead, BufReader}, path::PathBuf, time::Instant, process::Child};
use anyhow::Context;
use clap::Parser;
use progress::{FFmpegProgress, OverallProgress};
use ffmpeg::{assert_exists, FFmpegOptions, FFmpegProcessCompleted, FFmpegProcessStarted, FFMPEG_PATH, FFPROBE_PATH};
use parser::{Arguments, ExtensionMap, get_longest_common_path, OutputPattern};

struct FFmpegProcessWithProgress<'a> {
    process: FFmpegProcessStarted,
    progress: FFmpegProgress<'a>,
}

impl FFmpegProcessWithProgress<'_> {
    pub fn finish(self) -> FFmpegProcessCompleted {
        self.progress.finish();
        self.process.finish()
    }
}

fn get_ffmpeg_options(
    input_files: Vec<PathBuf>,
    output_pattern: &OutputPattern,
    extension_map: &ExtensionMap,
    ffmpeg_str_options: &Vec<String>,
    case_sensitive: bool,
    allow_override: bool,
    disable_pattern_append: bool,
    tree: Option<PathBuf>
) -> Result<Vec<FFmpegOptions>, anyhow::Error> {
    let mut ffmpeg_options: Vec<FFmpegOptions> = Vec::new();

    for input_file in input_files {
        if input_file.is_dir() {
            ffmpeg_options.extend(get_ffmpeg_options(
                read_dir(&input_file)
                    .with_context(|| format!("Cound not read directory: '{}'", input_file.display()))?
                    .collect::<Result<Vec<DirEntry>, _>>()
                    .with_context(|| format!("Error while reading directory: '{}'", input_file.display()))?
                    .into_iter()
                    .map(|x| input_file.join(x.path()))
                    .collect(),
                output_pattern,
                extension_map,
                ffmpeg_str_options,
                case_sensitive,
                allow_override,
                disable_pattern_append,
                if let Some(t) = &tree { 
                    Some(t.join(input_file.file_name().with_context(|| format!("could not read file_name: {}", input_file.display()))?)) 
                } else { 
                    Some(input_file.file_name().with_context(|| format!("Could not read file_name: '{}'", input_file.display()))?.into()) 
                }
            )?);
            continue;
        }

        let mut input_extension = input_file
            .extension()
            .with_context(|| format!("File has no extension: '{}'", input_file.display()))?
            .to_str()
            .with_context(|| format!("File has non utf-8 fucked up extension: '{}'", input_file.display()))?;

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

        let output_file = output_pattern.fill_blanks(
            &input_file, 
            extension_map, input_extension, 
            &tree,
            &ffmpeg_options,
            allow_override,
            disable_pattern_append,
        )?;

        ffmpeg_options.push(FFmpegOptions::new(
            input_file, 
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

fn update_progress(child: &mut Child, progress: &mut FFmpegProgress) {
    if !progress.has_duration {
        progress.update(None);
        return;
    }

    let output = child.stdout.as_mut().unwrap();
    let reader = BufReader::new(output);

    for result in reader.lines() { 
        if let Ok(line) = result {
            if line.contains("out_time_ms") {
                progress.update(Some(&line));
                break;
            }
        }
    };
}

fn update_process_with_progress<'a>(index: usize, process_with_progress: &'a mut FFmpegProcessWithProgress) -> Option<usize> {
    match &mut process_with_progress.process.child {
        Ok(child) => { 
            match child.try_wait() { // Child exists
                Ok(None) => { // Child is working
                    update_progress(child, &mut process_with_progress.progress);
                    return None;
                },
                _ => { // Child finished or error attemting to acces
                    return Some(index);
                }
        }},
        Err(_) => { // Child did not start
            return Some(index);
        },
    }
}

fn update_processes_until_one_finishes<'a>(processes: &'a mut Vec<FFmpegProcessWithProgress>) -> FFmpegProcessCompleted {
    loop { 
        for (i, process_with_progress) in processes.iter_mut().enumerate() {
            if let Some(finished_process_index) = update_process_with_progress(i, process_with_progress) {
                return processes.remove(finished_process_index).finish();
            };
        }
    }
}

fn run_ffmpeg_concurrent(mut ffmpeg_options: Vec<FFmpegOptions>, n_subprocesses: u32) -> Vec<FFmpegProcessCompleted> {
    let overall_progress = OverallProgress::new(
        ffmpeg_options.iter().map(|x| x.duration.unwrap_or(1.0).floor() as u64).sum(),
        ffmpeg_options.len() as u64
    );

    let mut started_processes: Vec<FFmpegProcessWithProgress> = Vec::new();
    let mut completed_processes: Vec<FFmpegProcessCompleted> = Vec::new();

    while let Some(ffmpeg_options) = ffmpeg_options.pop() {
        // if more processes than limit, wait until one finishes
        if started_processes.len() as u32 >= n_subprocesses { 
            let completed_process = update_processes_until_one_finishes(&mut started_processes);
            overall_progress.update_completed(&completed_process.get_error());
            completed_processes.push(completed_process);
        }

        started_processes.push(FFmpegProcessWithProgress {
            progress: FFmpegProgress::new(&overall_progress, &ffmpeg_options),
            process: ffmpeg_options.start(),
        });
    }

    while !started_processes.is_empty() {
        let completed_process = update_processes_until_one_finishes(&mut started_processes);
        overall_progress.update_completed(&completed_process.get_error());
        completed_processes.push(completed_process);
    }

    overall_progress.finish();
    completed_processes
}

fn print_errors(completed_processes: &Vec<FFmpegProcessCompleted>) {
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
}

fn init_ffmpeg_paths(args: &Arguments) -> Result<(), anyhow::Error> {
    FFMPEG_PATH.get_or_init(|| Box::leak(args.ffmpeg_path.clone().into_boxed_path()));
    FFPROBE_PATH.get_or_init(|| Box::leak(args.ffprobe_path.clone().into_boxed_path()));

    assert_exists(FFMPEG_PATH.get().unwrap())?;
    assert_exists(FFPROBE_PATH.get().unwrap())?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Arguments = Arguments::parse();

    init_ffmpeg_paths(&args)?;

    let start_time = Instant::now();

    let input_files: Vec<PathBuf> = args.get_glob_expanded_input_files();
    let output_pattern = OutputPattern::new(args.output);

    let ffmpeg_options: Vec<FFmpegOptions> = get_ffmpeg_options(
        input_files, 
        &output_pattern,
        &args.extension_map, 
        &args.ffmpeg_str_options,
        args.case_sensitive, 
        args.allow_override, 
        args.disable_pattern_append,
        None
    )?;

    create_hierarchy(&ffmpeg_options)?;

    println!("Total files      :  {}", ffmpeg_options.len());
    println!("Output directory : '{}'", get_longest_common_path(ffmpeg_options.iter().map(|x| x.output_file.as_path()).collect()).unwrap_or_default().display());

    let completed_processes: Vec<FFmpegProcessCompleted> = run_ffmpeg_concurrent(ffmpeg_options, args.n_subprocesses);

    println!("\nDone in {:.1?}!\n", Instant::now().duration_since(start_time));

    print_errors(&completed_processes);

    if cfg!(target_os = "linux") {
        // FIXME: For some reason on linux after the prgram is done, character echo is disabled
        // This fixes it but will need to find why that happens
        std::process::Command::new("stty").arg("echo").spawn()?.wait()?;
    }
    Ok(())
}
