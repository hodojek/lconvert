use std::fmt::Write;
use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use crate::ffmpeg::{FFmpegError, FFmpegOptions};

// FIXME: This work fine but requires use of unsafe.
// Idealy I want theese to be members of OverallProgress 
// however the with_key functions will need to hold a reference to the object 
// which I cannot figure out how to do so yea. This is stupid maybe idk but it 
// works and that's good to me. whatever.
static mut N_SUCCEEDED: u64 = 0;
static mut N_ERRORED: u64 = 0;
static mut N_TOTAL: u64 = 0;

#[derive(Debug)]
pub struct FFmpegProgress<'a> {
    pub has_duration: bool,
    pub progress_bar: ProgressBar,
    overall_progress: &'a OverallProgress,
}

impl<'a> FFmpegProgress<'a> {
    pub fn new(overall_progress: &'a OverallProgress, options: &FFmpegOptions) -> Self {
        let has_duration = options.duration.is_some();

        let progress_bar = overall_progress.manager.add(
            ProgressBar::new(options.duration.unwrap_or(1.0).floor() as u64)
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
            overall_progress,
        }
    }

    pub fn update(&self, metric_str: Option<&str>) {
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

        if metric.ne("out_time_ms") || value.is_empty() || value.parse::<u64>().is_err() { 
            return; 
        }

        let last_position = self.progress_bar.position();

        let seconds_processed = value.parse::<u64>().unwrap() / 1_000_000;

        self.progress_bar.set_position(seconds_processed.min(self.progress_bar.length().expect("Length set in the constructor")));

        self.overall_progress.update(self.progress_bar.position().saturating_sub(last_position));
    }

    pub fn finish(&self) {
        let last_position = self.progress_bar.position();
        self.progress_bar.set_position(self.progress_bar.length().expect("Length set in the constructor"));
        self.overall_progress.update(self.progress_bar.position().saturating_sub(last_position));
        self.progress_bar.finish_and_clear();
    }
}

#[derive(Debug)]
pub struct OverallProgress {
    pub manager: MultiProgress,
    pub progress_bar: ProgressBar,

}

impl OverallProgress {
    pub fn new(total_duration: u64, n_items: u64) -> Self {
        let manager = MultiProgress::new();
        let progress_bar = manager.add(ProgressBar::new(total_duration));

        unsafe { 
            N_TOTAL = n_items; 
        }

        let style = ProgressStyle::with_template(
            "  [{bar:50.green}] {percent:>3}% (eta: {eta_precise}) [{err:.red}/{ok:.green}/{all}]")
            .unwrap()
            // FIXME: I cannot figure out how to do this without shared state but it works so it's fine
            .with_key("err", |_: &ProgressState, w: &mut dyn Write| write!(w, "{:^1$}", unsafe { N_ERRORED }, unsafe { N_ERRORED }.to_string().len() + 2).unwrap())
            .with_key("ok", |_: &ProgressState, w: &mut dyn Write| write!(w, "{:^1$}", unsafe { N_SUCCEEDED }, unsafe { N_SUCCEEDED }.to_string().len() + 2).unwrap())
            .with_key("all", |_: &ProgressState, w: &mut dyn Write| write!(w, "{:^1$}", unsafe { N_TOTAL }, unsafe { N_TOTAL }.to_string().len() + 2).unwrap())
            .progress_chars("#> ");

        progress_bar.set_message(" 0 ");
        progress_bar.set_style(style);
        progress_bar.tick();

        Self {
            manager,
            progress_bar,
        }
    }

    pub fn update(&self, increase: u64) {
        self.progress_bar.inc(increase);
    }

    pub fn update_completed(&self, error: &Option<FFmpegError>) {
        // FIXME: unsafe here too
        unsafe {
            if error.is_some() {
                N_ERRORED += 1;
            } else {
                N_SUCCEEDED += 1;
            }
        }
    }

    pub fn finish(&self) {
        self.progress_bar.finish_and_clear();
        self.manager.clear().unwrap();
    }
}
