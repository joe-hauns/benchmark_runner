//TODO make Ui a trait

use indicatif::*;


pub struct Ui {
    bar: ProgressBar,
}


impl Ui {
    pub fn new(job: &str, cnt: usize) -> Self {
        let bar = ProgressBar::new(cnt as u64);
        bar.set_style(ProgressStyle::default_bar()
            .template("{spinner} {msg} [{elapsed_precise}] [{wide_bar:.green/fg}] {pos:>7}/{len:7} (left: {eta_precise})")
            .progress_chars("=> "));
        bar.set_message(job);
        bar.enable_steady_tick(100);
        Ui { bar }
    }

    pub fn println(&self, m: impl std::fmt::Display) {
        self.bar.println(m.to_string());
    }

    pub fn progress(&self) {
        self.bar.inc(1);
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.bar.finish();
    }
}


