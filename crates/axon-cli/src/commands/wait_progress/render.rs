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
    pub(crate) fn new(mode: ProgressMode, motion: bool, color: bool) -> Self {
        let target =
            (mode == ProgressMode::Interactive).then(|| ProgressDrawTarget::stderr_with_hz(4));
        Self::with_target(mode, target, motion, color)
    }

    #[cfg(test)]
    pub(crate) fn for_test(term: indicatif::InMemoryTerm, mode: ProgressMode) -> Self {
        Self::for_test_with_motion(term, mode, true)
    }

    #[cfg(test)]
    pub(crate) fn for_test_with_motion(
        term: indicatif::InMemoryTerm,
        mode: ProgressMode,
        motion: bool,
    ) -> Self {
        let target = (mode != ProgressMode::Silent)
            .then(|| ProgressDrawTarget::term_like_with_hz(Box::new(term), 30));
        Self::with_target(mode, target, motion, false)
    }

    fn with_target(
        mode: ProgressMode,
        target: Option<ProgressDrawTarget>,
        motion: bool,
        color: bool,
    ) -> Self {
        let (multi, header, active) = target.map_or((None, None, None), |target| {
            let multi = MultiProgress::with_draw_target(target);
            if mode != ProgressMode::Interactive {
                return (Some(multi), None, None);
            }
            multi.set_move_cursor(true);
            let frames: Vec<String> = if motion {
                ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, frame)| {
                        if index % 3 == 1 {
                            axon_core::ui::shimmer_when(color, frame)
                        } else {
                            axon_core::ui::accent_when(color, frame)
                        }
                    })
                    .collect()
            } else {
                let marker = axon_core::ui::accent_when(color, "•");
                vec![marker.clone(), marker]
            };
            let frame_refs: Vec<&str> = frames.iter().map(String::as_str).collect();
            let header_style = ProgressStyle::with_template("{spinner} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner());
            let header_style = header_style.tick_strings(&frame_refs);
            let active_style = ProgressStyle::with_template("{msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner());
            let header = multi.add(ProgressBar::new_spinner());
            header.set_style(header_style);
            let active = multi.add(ProgressBar::new_spinner());
            active.set_style(active_style);
            if motion {
                header.enable_steady_tick(Duration::from_millis(80));
            } else {
                header.tick();
            }
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
            ProgressMode::Plain => self.render_plain(view)?,
            ProgressMode::Silent => {}
        }
        self.last_render = Some(Instant::now());
        self.last_view = Some(view.clone());
        Ok(())
    }

    pub(crate) fn diagnostic(&self, message: &str) -> io::Result<()> {
        match self.mode {
            ProgressMode::Interactive => {
                if let Some(multi) = &self.multi {
                    multi.println(message)?;
                }
            }
            ProgressMode::Plain => eprintln!("{message}"),
            ProgressMode::Silent => {}
        }
        Ok(())
    }

    pub(crate) fn finish(&mut self, view: &FormattedWaitView) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        self.clear_live()?;
        match self.mode {
            ProgressMode::Interactive => {
                if let Some(multi) = &self.multi {
                    for notice in &view.notices {
                        if !self.printed_milestones.contains(notice) {
                            multi.println(notice)?;
                            self.printed_milestones.insert(notice.clone());
                        }
                    }
                    if let Some(terminal) = &view.terminal
                        && !self.printed_milestones.contains(terminal)
                    {
                        multi.println(terminal)?;
                        self.printed_milestones.insert(terminal.clone());
                    }
                }
            }
            ProgressMode::Plain => {
                for notice in view.notices.iter().skip(self.printed_plain_notices) {
                    if let Some(multi) = &self.multi {
                        multi.println(notice)?;
                    } else {
                        eprintln!("{notice}");
                    }
                }
                // Redirected stderr is diagnostic-only: no success receipt or
                // progress transcript. The command's result remains stdout.
            }
            ProgressMode::Silent => {}
        }
        self.finished = true;
        Ok(())
    }

    fn render_interactive(&mut self, view: &FormattedWaitView) -> io::Result<()> {
        if let Some(multi) = &self.multi {
            for milestone in &view.milestones {
                if !self.printed_milestones.contains(milestone) {
                    multi.println(milestone)?;
                    self.printed_milestones.insert(milestone.clone());
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

    fn render_plain(&mut self, view: &FormattedWaitView) -> io::Result<()> {
        for notice in view.notices.iter().skip(self.printed_plain_notices) {
            if let Some(multi) = &self.multi {
                multi.println(notice)?;
            } else {
                eprintln!("{notice}");
            }
        }
        self.printed_plain_notices = self.printed_plain_notices.max(view.notices.len());
        Ok(())
    }

    fn clear_live(&self) -> io::Result<()> {
        if let Some(multi) = &self.multi {
            if let Some(active) = &self.active {
                multi.remove(active);
            }
            if let Some(header) = &self.header {
                multi.remove(header);
            }
            multi.clear()?;
        }
        Ok(())
    }
}

impl Drop for WaitRenderer {
    fn drop(&mut self) {
        let _ = self.clear_live();
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
