use std::collections::HashSet;
use std::io;
use std::time::{Duration, Instant};

use axon_core::config::Config;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::format::FormattedWaitView;

mod batch;
mod extract;
mod session;

pub(crate) use batch::{BatchProgressForwarder, BatchProgressSession, batch_progress_channel};
pub(crate) use extract::ExtractProgressSession;
pub(crate) use session::WaitProgressSession;

const RENDER_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgressMode {
    Interactive,
    Plain,
    Silent,
}

impl ProgressMode {
    pub(crate) fn for_config(cfg: &Config, stderr_is_tty: bool) -> Self {
        if cfg.json_output || cfg.quiet {
            Self::Silent
        } else if stderr_is_tty {
            Self::Interactive
        } else {
            Self::Plain
        }
    }
}

pub(crate) struct WaitRenderer {
    mode: ProgressMode,
    multi: Option<MultiProgress>,
    header: Option<ProgressBar>,
    active: Option<ProgressBar>,
    last_view: Option<FormattedWaitView>,
    last_render: Option<Instant>,
    printed_milestones: HashSet<String>,
    printed_plain_notices: usize,
    finished: bool,
}

impl WaitRenderer {
    pub(crate) fn new(mode: ProgressMode) -> Self {
        let target =
            (mode == ProgressMode::Interactive).then(|| ProgressDrawTarget::stderr_with_hz(4));
        Self::with_target(mode, target)
    }

    #[cfg(test)]
    pub(crate) fn for_test(term: indicatif::InMemoryTerm, mode: ProgressMode) -> Self {
        let target = (mode == ProgressMode::Interactive)
            .then(|| ProgressDrawTarget::term_like_with_hz(Box::new(term), 30));
        Self::with_target(mode, target)
    }

    fn with_target(mode: ProgressMode, target: Option<ProgressDrawTarget>) -> Self {
        let (multi, header, active) = target.map_or((None, None, None), |target| {
            let multi = MultiProgress::with_draw_target(target);
            multi.set_move_cursor(true);
            let style = ProgressStyle::with_template("{msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner());
            let header = multi.add(ProgressBar::new_spinner());
            header.set_style(style.clone());
            let active = multi.add(ProgressBar::new_spinner());
            active.set_style(style);
            (Some(multi), Some(header), Some(active))
        });
        Self {
            mode,
            multi,
            header,
            active,
            last_view: None,
            last_render: None,
            printed_milestones: HashSet::new(),
            printed_plain_notices: 0,
            finished: false,
        }
    }

    pub(crate) fn render(&mut self, view: &FormattedWaitView) -> io::Result<()> {
        if self.finished || self.mode == ProgressMode::Silent {
            return Ok(());
        }
        if self.last_view.as_ref() == Some(view) {
            return Ok(());
        }
        let important = self.last_view.as_ref().is_none_or(|previous| {
            previous.notices != view.notices
                || previous.milestones != view.milestones
                || previous.terminal != view.terminal
        });
        if !important
            && self
                .last_render
                .is_some_and(|last| last.elapsed() < RENDER_INTERVAL)
        {
            return Ok(());
        }
        self.render_now(view)
    }

    pub(crate) fn render_now(&mut self, view: &FormattedWaitView) -> io::Result<()> {
        if self.finished || self.mode == ProgressMode::Silent {
            return Ok(());
        }
        match self.mode {
            ProgressMode::Interactive => self.render_interactive(view)?,
            ProgressMode::Plain => self.render_plain(view),
            ProgressMode::Silent => {}
        }
        self.last_render = Some(Instant::now());
        self.last_view = Some(view.clone());
        Ok(())
    }

    pub(crate) fn diagnostic(&self, message: &str) {
        match self.mode {
            ProgressMode::Interactive => {
                if let Some(multi) = &self.multi {
                    let _ = multi.println(message);
                }
            }
            ProgressMode::Plain => eprintln!("{message}"),
            ProgressMode::Silent => {}
        }
    }

    pub(crate) fn finish(&mut self, view: &FormattedWaitView) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        self.clear_live();
        match self.mode {
            ProgressMode::Interactive => {
                if let Some(multi) = &self.multi {
                    for notice in &view.notices {
                        if self.printed_milestones.insert(notice.clone()) {
                            multi.println(notice)?;
                        }
                    }
                    if let Some(terminal) = &view.terminal
                        && self.printed_milestones.insert(terminal.clone())
                    {
                        multi.println(terminal)?;
                    }
                }
            }
            ProgressMode::Plain => {
                for notice in view.notices.iter().skip(self.printed_plain_notices) {
                    eprintln!("{notice}");
                }
                if let Some(terminal) = &view.terminal {
                    eprintln!("{terminal}");
                }
            }
            ProgressMode::Silent => {}
        }
        Ok(())
    }

    fn render_interactive(&mut self, view: &FormattedWaitView) -> io::Result<()> {
        if let Some(multi) = &self.multi {
            for milestone in &view.milestones {
                if self.printed_milestones.insert(milestone.clone()) {
                    multi.println(milestone)?;
                }
            }
        }
        if let Some(header) = &self.header {
            let mut header_lines = vec![view.heading.clone()];
            header_lines.extend(view.notices.iter().cloned());
            header.set_message(header_lines.join("\n"));
            header.tick();
        }
        if let Some(active) = &self.active {
            active.set_message(view.active.join("\n"));
            active.tick();
        }
        Ok(())
    }

    fn render_plain(&mut self, view: &FormattedWaitView) {
        for milestone in &view.milestones {
            if self.printed_milestones.insert(milestone.clone()) {
                eprintln!("{milestone}");
            }
        }
        for notice in view.notices.iter().skip(self.printed_plain_notices) {
            eprintln!("{notice}");
        }
        self.printed_plain_notices = self.printed_plain_notices.max(view.notices.len());
    }

    fn clear_live(&self) {
        if let Some(multi) = &self.multi {
            if let Some(active) = &self.active {
                multi.remove(active);
            }
            if let Some(header) = &self.header {
                multi.remove(header);
            }
            let _ = multi.clear();
        }
    }
}

impl Drop for WaitRenderer {
    fn drop(&mut self) {
        self.clear_live();
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
