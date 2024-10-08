# lconvert [![Crates.io Version](https://img.shields.io/crates/v/lconvert)](https://crates.io/crates/lconvert) [![Crates.io Total Downloads](https://img.shields.io/crates/d/lconvert)](https://crates.io/crates/lconvert)

A cli tool that simplifies usage of FFmpeg for multiple files
## Features
### Multiple file conversion
- files with an extension to another extension
- files with different extensions to another extension
- files with different extensions to different extensions
- files in directories

### Custom FFmpeg options
Allows you to apply FFmpeg options (such as changing bitrate, resolution, etc...) to multiple files at once 
### glob expansion
Expands glob expressions
### Parallel execution
Runs multiple FFmpeg instances at once for fast conversion time 
### Progress bar
And it has a progress bar, yes
## Requirements
You will need ffmpeg and ffprobe executables [downloaded](https://www.ffmpeg.org/) and avalable through the PATH variable

You will need [cargo](https://www.rust-lang.org/tools/install) if you want to install lconvert from source (not needed for binary releases)
## Installation
### Binary
Download binary release for your os from [releases](https://github.com/hodojek/lconvert/releases)
### Install with cargo
```
cargo install lconvert
```
### Build yourself
```
git clone https://github.com/hodojek/lconvert.git 
```
```
cd lconvert
```
```
cargo build --release
```
You will find lconvert executable in ./target/release directory
## Example 
<img src="https://github.com/hodojek/lconvert/blob/master/example.gif?raw=true">
