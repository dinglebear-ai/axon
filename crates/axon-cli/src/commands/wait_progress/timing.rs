use std::collections::VecDeque;
use std::time::Duration;

const MAX_SAMPLES: usize = 8;
const MIN_SAMPLE_SPAN: Duration = Duration::from_secs(1);
const MIN_REMAINING: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RateEstimate {
    pub per_second: f64,
    pub remaining: Duration,
}

#[derive(Debug, Default)]
pub(crate) struct TimingEstimator {
    samples: VecDeque<(Duration, u64)>,
    denominator: Option<u64>,
}

impl TimingEstimator {
    pub(crate) fn reset(&mut self) {
        self.samples.clear();
        self.denominator = None;
    }

    pub(crate) fn sample(
        &mut self,
        elapsed: Duration,
        done: u64,
        total: Option<u64>,
    ) -> Option<RateEstimate> {
        let total = total.filter(|total| *total > 0)?;
        if self.denominator != Some(total) {
            self.samples.clear();
            self.denominator = Some(total);
        }
        if self
            .samples
            .back()
            .is_some_and(|(_, previous)| done < *previous)
        {
            self.samples.clear();
        }
        self.samples.push_back((elapsed, done));
        while self.samples.len() > MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.estimate(total)
    }

    fn estimate(&self, total: u64) -> Option<RateEstimate> {
        let &(latest_at, latest_done) = self.samples.back()?;
        let &(earliest_at, earliest_done) = self.samples.iter().find(|(at, done)| {
            *done != latest_done && latest_at.saturating_sub(*at) >= MIN_SAMPLE_SPAN
        })?;
        let span = latest_at.saturating_sub(earliest_at);
        let advanced = latest_done.checked_sub(earliest_done)?;
        let per_second = advanced as f64 / span.as_secs_f64();
        if !per_second.is_finite() || per_second <= 0.0 || latest_done >= total {
            return None;
        }
        let remaining = Duration::from_secs_f64((total - latest_done) as f64 / per_second);
        (remaining >= MIN_REMAINING).then_some(RateEstimate {
            per_second,
            remaining,
        })
    }
}

#[cfg(test)]
#[path = "timing_tests.rs"]
mod tests;
