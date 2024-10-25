use std::{collections::HashMap, path::{absolute, PathBuf, Path}};
use clap::{arg, builder::ValueParser, command, error::Result, value_parser, Parser, ValueHint};
use glob::{glob, GlobError};
use anyhow::Context;
use crate::FFmpegOptions;

// let r = r#"^((\w+)|(\w+=\w+)(,\w+=\w+)*)$"#;
static EXTENSION_MAP_REGEX: &str = r#"^((\w+)(,\w+=\w+)*|(\w+=\w+)(,\w+=\w+)*(,\w+)?(,\w+=\w+)*)$"#;
static DEFAULT_PATTERN: &str = "lconvert_output{{unique-suffix}}/{{tree}}/{{file}}";

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
        long_help = 
            "Maps input file extension to desired output file extension\n\n\
             Examples:\
             \n* 'jpeg=png' will convert any input file with .jpeg extension to .png\
             \n* 'jpeg=png,mp4=avi' will convert .jpeg to .png and .mp4 to .avi\
             \n* 'jpeg' is a wildcard and will try to convert all input files to .jpeg\
             \n* 'mp3=ogg,jpeg,mp4=avi' there may be exactly one wildcard",
        value_name = "IN-EXT=OUT-EXT",
        value_parser = parser_extension_map(), 
    )]
    pub extension_map: ExtensionMap,

    /// Output pattern
    #[arg(
        short = 'o',
        long,
        value_name = "PATTERN",
        default_value = DEFAULT_PATTERN,
        long_help = format!(
            "Output pattern\n\n\
             A pattern is a path with optional placeholders that will be filled automaticaly.\n\
             If no pattern is provided, the default is to create a unique 'lconvert_output'\n\
             directory and copy the directory tree and file names.\n\n\
             Placeholders:\n\
             * {} - File name with input extension\n\
             * {} - File name without extension\n\
             * {} - Parent hierarchy of the input file\n\
             * {} - Parent of the input file\n\
             * {} - Input extension\n\
             * {} - Output extension\n\
             * {} - A unique suffix (_<UNIQUE_NUMBER>). Replaced by empty string if directory or file is already unique\n\n\
             Important:\n\
             * The last element of the pattern will always have an output extension.\
             \n  If it did not have an extension, it will be added, if it did, it will be changed.\n\
             * For convinience, any pattern with no placeholders will be appended with {} and {}.\
             \n  So pattern 'outdir' becomes 'outdir/{}/{}'.\
             \n  You can disable this behaviour with '--disable-pattern-append' flag.",
             OutputPattern::FILE,
             OutputPattern::STEM,
             OutputPattern::TREE,
             OutputPattern::PARENT,
             OutputPattern::IN_EXT,
             OutputPattern::OUT_EXT,
             OutputPattern::UNIQUE_SUFFIX,
             OutputPattern::TREE,
             OutputPattern::FILE,
             OutputPattern::TREE,
             OutputPattern::FILE,
        )
    )]
    pub output: PathBuf,

    /// Max number of concurent ffmpeg processes
    #[arg(
        short,
        long,
        value_parser = value_parser!(u32).range(1..),
        value_name = "N",
        default_value = "4",
    )]
    pub n_subprocesses: u32,

    /// Disables appending {{tree}}/{{file}} to output patterns with no placeholders
    #[arg(
        short = 'd',
        long,
    )]
    pub disable_pattern_append: bool,


    /// Make extension map case sensetive
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
        long_help = 
            "Custom ffmpeg options to apply to every file\n\n\
             Warning! Some options (such specifing output pipe) may result in upredictable beviour\n\n\
             Example:\
             \n| 'lconvert -o out_dir -m wav=mp3 input_file.wav -- -ab 128KB'\
             \n|                                     Custom option ^^^^^^^^^\
             \n| Expands to:\n|\
             \n| 'ffmpeg -hide_banner -n -loglevel error -progress - -nostats -i input_file.wav -ab 128KB out_dir/input_file.mp3'\
             \n|                                                                  Custom option ^^^^^^^^^"
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

#[derive(Debug)]
pub struct OutputPattern {
    pattern: PathBuf
}

impl OutputPattern {
    pub const FILE: &'static str = "{{file}}";
    pub const STEM: &'static str = "{{stem}}";
    pub const IN_EXT: &'static str = "{{in-ext}}";
    pub const OUT_EXT: &'static str = "{{out-ext}}";
    pub const TREE: &'static str = "{{tree}}";
    pub const PARENT: &'static str = "{{parent}}";
    pub const UNIQUE_SUFFIX: &'static str = "{{unique-suffix}}";

    pub fn new(pattern: PathBuf) -> Self {
        Self { pattern }
    }

    pub fn has_blanks(&self) -> bool {
        let string = self.pattern.to_string_lossy();
        string.contains(Self::FILE) ||
        string.contains(Self::STEM) ||
        string.contains(Self::IN_EXT) ||
        string.contains(Self::OUT_EXT) ||
        string.contains(Self::TREE) ||
        string.contains(Self::PARENT) ||
        string.contains(Self::UNIQUE_SUFFIX)
    }

    pub fn fill_blanks(&self, 
                       input_file: &Path, 
                       extension_map: &ExtensionMap, 
                       input_extension: &str, 
                       tree: &Option<PathBuf>,
                       ffmpeg_options: &Vec<FFmpegOptions>,
                       allow_override: bool,
                       disable_pattern_append: bool) -> Result<PathBuf, anyhow::Error> 
    {
        let output_pattern = if !self.has_blanks() && !disable_pattern_append {
            self.pattern.join(Self::TREE).join(Self::FILE)
        } else {
            self.pattern.to_owned()
        };

        let output_pattern = output_pattern.to_string_lossy()
            .replace(
                Self::STEM, 
                input_file.file_stem().with_context(|| format!("Could not get file_name: '{}'", input_file.display()))?.to_str().unwrap()
            ).replace(
                Self::FILE, 
                input_file.file_name().with_context(|| format!("Could not get file_name: '{}'", input_file.display()))?.to_str().unwrap()
            ).replace(
                Self::IN_EXT,
                input_file.extension().with_context(|| format!("Could not get extension: '{}'", input_file.display()))?.to_str().unwrap()
            ).replace(
                Self::OUT_EXT,
                &extension_map[input_extension]
            ).replace(
                Self::PARENT,
                input_file.parent().with_context(|| format!("Could not get parent: '{}'", input_file.display()))?
                          .file_name().with_context(|| format!("Could not get file_name: '{}'", input_file.display()))?.to_str().unwrap()
            ).replace(
                Self::TREE, 
                if let Some(t) = &tree { t.to_str().unwrap() } else { "" }
            );

        let mut output_file = absolute(Path::new(&output_pattern))?;
        output_file.set_extension(&extension_map[input_extension]);
        output_file = Self::replace_uniques(output_file, &ffmpeg_options, allow_override);

        Ok(output_file)
    }

    fn replace_uniques(mut output_file: PathBuf, ffmpeg_options: &Vec<FFmpegOptions>, _allow_override: bool) -> PathBuf {
        let mut flag = true;

        while flag { for (i, (first, second)) in get_components(&output_file).iter().enumerate() {
            flag = false;

            if first.to_string_lossy().contains(Self::UNIQUE_SUFFIX) {
                let mut o = first.to_string_lossy().replace(Self::UNIQUE_SUFFIX, "");
                let mut num = 1;

                while 
                    (second.is_some() && Path::new(&o).exists()) ||
                    (second.is_none() && (
                        ffmpeg_options.iter().any(|x| get_components(&x.output_file)[i].0.eq(Path::new(&o))) || 
                        Path::new(&o).exists() 
                        // (!allow_override && Path::new(&o).exists()) // Don't know about the allow_override, feels like it should not be able to override anyway
                    ))
                {
                    o = first.to_string_lossy().replace(Self::UNIQUE_SUFFIX, &format!("_{}", num));
                    num += 1;
                };

                output_file = if let Some(s) = second { Path::new(&o).join(s) } else { PathBuf::from(&o) };

                flag = true;
                break;
            }
        };};
        output_file
    }
}

pub fn get_longest_common_path(mut paths: Vec<PathBuf>) -> Option<PathBuf> {
    if paths.is_empty() {
        return None;
    }

    paths.sort();

    let first: Vec<_> = paths[0].iter().collect(); 
    let last: Vec<_> = paths[paths.len()-1].iter().collect();

    let min_length = first.len().min(last.len());
    let mut i = 0;

    while i < min_length && first[i] == last[i] {
        i += 1;
    };

    if i == 0 { 
        return None;
    };

    Some(first[..i].iter().collect())
}

pub fn get_components(path: &Path) -> Vec<(PathBuf, Option<PathBuf>)> {
    let path_components = path.iter().collect::<Vec<_>>();
    let mut ret = Vec::new();

    for i in 1..path.iter().count() {
        ret.push((
            path_components[0..i].iter().collect(),
            Some(path_components[i..].iter().collect())
        ))
    };
    ret.push((path_components.iter().collect(), None));
    ret
}
