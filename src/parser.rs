use std::{collections::HashMap, path::{absolute, PathBuf}};
use clap::{arg, builder::ValueParser, command, error::Result, value_parser, Parser, ValueHint};
use glob::{glob, GlobError};

// let r = r#"^((\w+)|(\w+=\w+)(,\w+=\w+)*)$"#;
static EXTENSION_MAP_REGEX: &str = r#"^((\w+)(,\w+=\w+)*|(\w+=\w+)(,\w+=\w+)*(,\w+)?(,\w+=\w+)*)$"#;

pub type ExtensionMap = HashMap<String, String>;

pub fn parser_input_files() -> ValueParser {
    ValueParser::from(move |s: &str| -> std::result::Result<PathBuf, String> { 
        // TODO: Currently expanding glob two times: one for validation and another for actual values
        let path: PathBuf;

        match absolute(s) {
            Ok(p) => path = p,
            Err(err) => return Err(err.to_string()),
        }

        if path.exists() {
            return Ok(path);
        }

        match glob(path.to_str().unwrap()) {
            Ok(expanded) => {
                let mut is_empty = true;

                for path in expanded {
                    if let Err(err) = path {
                        return Err(err.to_string());
                    }
                    is_empty = false;
                }

                if is_empty {
                    return Err(format!("File Not Found: '{s}'"))
                } else {
                    return Ok(path);
                }
            },
            Err(err) => Err(err.to_string()),
        }
    })
}

pub fn parser_extension_map() -> ValueParser {
    ValueParser::from(move |s: &str| -> std::result::Result<ExtensionMap, String> {
        let reg = regex::Regex::new(EXTENSION_MAP_REGEX).unwrap();

        match reg.is_match(s) {
            true => Ok(s.split(',').map(|x| {
                let (key, value) = x.split_once('=').unwrap_or(("*", x));
                (key.to_owned(), value.to_owned())
            }).collect()),
            false => Err(format!("Does not match expression: {EXTENSION_MAP_REGEX}")),
        }
    })
}

#[derive(Parser, Debug)]
#[command(version, about = "Convert large amounts of files", long_about = None)]
pub struct Arguments {
    /// Any file with an extension, directory, or a glob pattern
    #[arg(
        required = true,
        value_parser = parser_input_files(),
        value_hint = ValueHint::AnyPath, 
    )]
    input_files: Vec<PathBuf>,

    /// Maps input extension to output extension (see examples with '--help')
    #[arg(
        short = 'm', 
        long, 
        required = true,
        long_help = "Maps input file extension to desired output file extension\n\n\
                     Examples:\
                     \n* 'jpeg=png' will convert any input file with .jpeg extension to .png\
                     \n* 'jpeg=png,mp4=avi' will convert .jpeg to .png and .mp4 to .avi\
                     \n* 'jpeg' is a wildcard and will try to convert all input files to .jpeg\
                     \n* 'mp3=ogg,jpeg,mp4=avi' there may be exactly one wildcard",
        value_name = "IN-EXT=OUT-EXT",
        value_parser = parser_extension_map(), 
    )]
    pub extension_map: ExtensionMap,

    /// Output directory (creates 'lconvert_output' if not specified)
    #[arg(
        short = 'd',
        long,
        value_name = "PATH",
        value_hint = ValueHint::DirPath,
    )]
    pub output_directory: Option<PathBuf>,

    /// Max number of concurent ffmpeg processes
    #[arg(
        short,
        long,
        value_parser = value_parser!(u32).range(1..),
        value_name = "N",
        default_value = "4",
    )]
    pub n_subprocesses: u32,

    /// Make extension mapping case sensetive
    #[arg(
        short,
        long,
    )]
    pub case_sensitive: bool,

    /// Allow ffmpeg to override files
    #[arg(
        short = 'y',
        long,
    )]
    pub allow_override: bool,

    /// Custom ffmpeg options to apply to every file (see example with '--help')
    #[arg(
        last = true,
        num_args = 0..,
        name = "FFMPEG_OPTIONS",
        long_help = "Custom ffmpeg options to apply to every file\n\n\
                     Warning! Some options (such specifing output pipe) may result in upredictable beviour\n\n\
                     Example:\
                     \n* 'lconvert -d out_dir -m wav=mp3 input_file.wav -- -ab 128KB'\
                     \n                                      Custom option ^^^^^^^^^\
                     \n  Expands to:\n\
                     \n  'ffmpeg -hide_banner -n -loglevel error -progress - -nostats -i input_file.wav -ab 128KB out_dir/input_file.mp3'\
                     \n                                                                   Custom option ^^^^^^^^^"
    )]
    pub ffmpeg_str_options: Vec<String>,
}

impl Arguments {
    pub fn get_glob_expanded_input_files(&self) -> Vec<PathBuf> {
        let mut results: Vec<PathBuf> = Vec::new();

        for input_file in self.input_files.iter() {
            if input_file.exists() {
                results.push(input_file.clone());
            } else {
                results.extend(glob(input_file.to_str().unwrap())
                    .expect("Validated this during parsing")
                    .into_iter()
                    .collect::<Result<Vec<PathBuf>, GlobError>>()
                    .expect("Validated this during parsing")
                );
            }
        }
        results
    }
}
