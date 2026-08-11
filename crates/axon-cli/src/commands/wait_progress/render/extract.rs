use std::future::Future;
use std::io::IsTerminal;

use axon_core::{config::Config, ui};
use axon_services::extract::ExtractProgress;
use tokio::sync::watch;

use super::{ProgressMode, WaitRenderer};
use crate::commands::wait_progress::format::{FormattedWaitView, format_wait_view};
use crate::commands::wait_progress::model::{OperatorPhase, RenderedMilestone, WaitViewModel};

pub(crate) struct ExtractProgressSession {
    receiver: watch::Receiver<ExtractProgress>,
    model: WaitViewModel,
    renderer: WaitRenderer,
    latest: ExtractProgress,
    dirty: bool,
}

impl ExtractProgressSession {
    pub(crate) fn new(
        cfg: &Config,
        receiver: watch::Receiver<ExtractProgress>,
        urls_total: usize,
    ) -> Self {
        let mode = ProgressMode::for_config(cfg, std::io::stderr().is_terminal());
        let latest = receiver.borrow().clone();
        let noun = if urls_total == 1 { "URL" } else { "URLs" };
        let mut model = WaitViewModel::source(format!("{urls_total} {noun}"), None);
        model.apply_extract_progress(&latest);
        Self {
            receiver,
            model,
            renderer: WaitRenderer::new(mode),
            latest,
            dirty: true,
        }
    }

    pub(crate) async fn run_until<T, E>(
        &mut self,
        work: impl Future<Output = Result<T, E>>,
    ) -> Result<T, E> {
        tokio::pin!(work);
        let mut cadence = tokio::time::interval(std::time::Duration::from_millis(250));
        let mut updates_open = true;
        loop {
            tokio::select! {
                result = &mut work => {
                    self.apply_latest();
                    self.finish(result.is_err());
                    return result;
                }
                changed = self.receiver.changed(), if updates_open => {
                    if changed.is_ok() {
                        self.apply_latest();
                    } else {
                        updates_open = false;
                    }
                }
                _ = cadence.tick() => self.render_if_dirty(),
            }
        }
    }

    fn apply_latest(&mut self) {
        self.latest = self.receiver.borrow_and_update().clone();
        self.dirty |= self.model.apply_extract_progress(&self.latest);
    }

    fn render_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let view = self.formatted();
        let _ = self.renderer.render(&view);
        self.dirty = false;
    }

    fn finish(&mut self, failed: bool) {
        self.model.active = None;
        self.model.terminal = Some(RenderedMilestone {
            phase: OperatorPhase::Extract,
            summary: format!(
                "{}/{} URLs · {} items",
                self.latest.urls_done, self.latest.urls_total, self.latest.items_done
            ),
            elapsed: std::time::Duration::ZERO,
            degraded: failed,
        });
        let view = self.formatted();
        let _ = self.renderer.finish(&view);
    }

    fn formatted(&self) -> FormattedWaitView {
        let width = std::env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(80);
        format_wait_view(&self.model, width, None, ui::stderr_color_enabled())
    }
}
