use std::fmt::Write;
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use crate::ffmpeg::{FFmpegError, FFmpegOptions};

#[derive(Debug)]
pub struct FFmpegProgress {
    pub has_duration: bool,
    progress_bar: ProgressBar,
}

impl FFmpegProgress {
    pub fn new(overall_progress: &OverallProgress, options: &FFmpegOptions) -> Self {
        let has_duration = options.duration.is_some();

        let progress_bar = overall_progress.manager.add(
            ProgressBar::new(options.duration.unwrap_or(1.0) as u64)
        );

        let style = ProgressStyle::with_template(
            "{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue}| {percent:>3}% (eta: {eta_precise}) \"{msg}\"")
            .unwrap()
            .progress_chars("##-");

        progress_bar.set_style(style);
        progress_bar.set_message(format!("{}", options.input_file.file_name().unwrap().to_str().unwrap()));

        Self {
            has_duration,
            progress_bar,
        }
    }

    pub fn update(&mut self, metric_str: Option<&str>) {
        self.progress_bar.tick();

        if !self.has_duration || metric_str.is_none() {
            return;
        }

        let metric: &str;
        let value: &str;

        if let Some((m, v)) = metric_str.unwrap().trim().split_once('=') {
            metric = m.trim();
            value = v.trim();
        } else { 
            return; 
        }

        if metric.ne("out_time_ms") || value.is_empty() || value.eq("N/A") { 
            return; 
        }

        let seconds_processed = value.parse::<u64>().unwrap() / 1_000_000;
        self.progress_bar.set_position(seconds_processed);
    }

    pub fn finish(&self) {
        self.progress_bar.finish_and_clear();
    }
}

#[derive(Debug)]
pub struct OverallProgress {
    pub manager: MultiProgress,
    pub progress_bar: ProgressBar,
}

impl OverallProgress {
    pub fn new(n_items: u64) -> Self {
        let manager = MultiProgress::new();
        let progress_bar = manager.add(ProgressBar::new(n_items));

        let style = ProgressStyle::with_template(
            // TODO: Need to implement [ err / ok / total ] correctly
            // Currently ok counts ok and err, but should count only ok
            "  [{bar:50.green}] {percent:>3}% (eta: {eta_precise}) [{msg:.red}/{pos:.green}/{len}]")
            .unwrap()
            .with_key("len", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:^1$}", state.len().unwrap(), state.len().unwrap().to_string().len() + 2).unwrap())
            .with_key("pos", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:^1$}", state.pos(), state.pos().to_string().len() + 2).unwrap())
            .progress_chars("#> ");

        progress_bar.set_message(" 0 ");
        progress_bar.set_style(style);
        progress_bar.tick();

        Self {
            manager,
            progress_bar,
        }
    }

    pub fn update(&self, error: &Option<FFmpegError>) {
        if error.is_some() {
            self.progress_bar.set_message({
                let errored = self.progress_bar.message().trim().parse::<u32>().unwrap();
                let width = errored.to_string().len();
                format!("{:^1$}", errored + 1, width + 2)
            });
        }
        self.progress_bar.inc(1);
    }

    pub fn finish(&self) {
        self.progress_bar.finish_and_clear();
        self.manager.clear().unwrap();
    }
}
