use chrono::Duration as ChronoDuration;
use chrono::Local;
use codex_app_server_protocol::ConsumeAccountRateLimitResetCreditOutcome;
use codex_app_server_protocol::RateLimitResetCreditsSummary;
use uuid::Uuid;

use super::*;
use crate::bottom_pane::ResetUsageConfirmParams;
use crate::bottom_pane::ResetUsageConfirmView;
use crate::status::RATE_LIMIT_STALE_THRESHOLD_MINUTES;
use crate::status::RESET_USAGE_MAX_REMAINING_PERCENT;
use crate::status::StatusResetUsageState;
use crate::status::format_reset_credit_count;

#[derive(Debug, Clone)]
pub(super) struct PendingResetUsageRefresh {
    result: Result<ConsumeAccountRateLimitResetCreditOutcome, String>,
}

impl ChatWidget {
    pub(crate) fn on_rate_limit_reset_credits(
        &mut self,
        reset_credits: Option<RateLimitResetCreditsSummary>,
    ) {
        self.rate_limit_reset_credits = reset_credits;
        self.refresh_status_line();
    }

    pub(super) fn status_reset_usage_state_at(
        &self,
        now: chrono::DateTime<Local>,
    ) -> StatusResetUsageState {
        if !self.should_prefetch_rate_limits() {
            return StatusResetUsageState::Hidden;
        }
        if self.reset_usage_request_in_flight.is_some()
            || self.pending_reset_usage_refresh.is_some()
        {
            return StatusResetUsageState::Loading;
        }
        let Some(summary) = self.rate_limit_reset_credits.as_ref() else {
            return StatusResetUsageState::Missing;
        };
        if summary.available_count <= 0 {
            return StatusResetUsageState::NoCredits;
        }
        let Some(snapshot) = self.rate_limit_snapshots_by_limit_id.get("codex") else {
            return StatusResetUsageState::Unavailable;
        };
        if now.signed_duration_since(snapshot.captured_at)
            > ChronoDuration::minutes(RATE_LIMIT_STALE_THRESHOLD_MINUTES)
        {
            return StatusResetUsageState::Stale {
                available_count: summary.available_count,
            };
        }
        let Some(primary) = snapshot.primary.as_ref() else {
            return StatusResetUsageState::Unavailable;
        };
        let remaining_percent = (100.0 - primary.used_percent).round().clamp(0.0, 100.0) as i64;
        if remaining_percent > RESET_USAGE_MAX_REMAINING_PERCENT {
            StatusResetUsageState::Locked {
                available_count: summary.available_count,
                remaining_percent,
            }
        } else {
            StatusResetUsageState::Eligible {
                available_count: summary.available_count,
                remaining_percent,
            }
        }
    }

    pub(super) fn open_reset_usage_prompt(&mut self) {
        if !self.should_prefetch_rate_limits() {
            self.add_error_message(
                "Sign in with ChatGPT to use earned rate-limit resets.".to_string(),
            );
            return;
        }

        match self.status_reset_usage_state_at(Local::now()) {
            StatusResetUsageState::Eligible {
                available_count,
                remaining_percent,
            } => {
                let attempt = self
                    .reset_usage_retry
                    .take()
                    .unwrap_or_else(|| ResetUsageAttempt {
                        idempotency_key: Uuid::new_v4().to_string(),
                        available_count,
                        remaining_percent,
                    });
                let attempt = ResetUsageAttempt {
                    available_count,
                    remaining_percent,
                    ..attempt
                };
                let view = ResetUsageConfirmView::new(
                    ResetUsageConfirmParams { attempt },
                    self.app_event_tx.clone(),
                    self.bottom_pane.list_keymap(),
                );
                self.bottom_pane.show_view(Box::new(view));
            }
            StatusResetUsageState::Locked {
                available_count,
                remaining_percent,
            } => {
                self.reset_usage_retry = None;
                self.add_info_message(
                    format!(
                        "{} available, but reset is locked until {RESET_USAGE_MAX_REMAINING_PERCENT}% or less is left ({remaining_percent}% left).",
                        format_reset_credit_count(available_count)
                    ),
                    /*hint*/ None,
                );
            }
            StatusResetUsageState::NoCredits => {
                self.reset_usage_retry = None;
                self.add_info_message("No rate-limit resets are available.".to_string(), None);
            }
            StatusResetUsageState::Hidden | StatusResetUsageState::Unavailable => {
                self.refresh_reset_usage_data(
                    "Usage data is not available yet. Refreshing; run /reset-usage again shortly.",
                );
            }
            StatusResetUsageState::Loading => {
                self.add_info_message(
                    "A usage reset is already in progress. Run /status shortly.".to_string(),
                    /*hint*/ None,
                );
            }
            StatusResetUsageState::Missing | StatusResetUsageState::Stale { .. } => {
                self.refresh_reset_usage_data(
                    "Usage data is stale or still loading. Refreshing; run /reset-usage again shortly.",
                );
            }
        }
    }

    pub(crate) fn start_reset_usage_request(&mut self, attempt: ResetUsageAttempt) -> bool {
        match self.status_reset_usage_state_at(Local::now()) {
            StatusResetUsageState::Eligible { .. } => {}
            StatusResetUsageState::Loading => {
                self.add_info_message(
                    "A usage reset is already in progress. Run /status shortly.".to_string(),
                    /*hint*/ None,
                );
                return false;
            }
            StatusResetUsageState::Locked {
                available_count,
                remaining_percent,
            } => {
                self.reset_usage_retry = None;
                self.add_info_message(
                    format!(
                        "{} available, but reset is now locked until {RESET_USAGE_MAX_REMAINING_PERCENT}% or less is left ({remaining_percent}% left).",
                        format_reset_credit_count(available_count)
                    ),
                    /*hint*/ None,
                );
                return false;
            }
            StatusResetUsageState::NoCredits => {
                self.reset_usage_retry = None;
                self.add_info_message("No rate-limit resets are available.".to_string(), None);
                return false;
            }
            StatusResetUsageState::Hidden
            | StatusResetUsageState::Missing
            | StatusResetUsageState::Unavailable
            | StatusResetUsageState::Stale { .. } => {
                self.refresh_reset_usage_data(
                    "Usage data changed before reset. Refreshing; run /reset-usage again shortly.",
                );
                return false;
            }
        }
        self.reset_usage_request_in_flight = Some(attempt);
        self.refresh_status_line();
        true
    }

    pub(crate) fn finish_reset_usage_request(
        &mut self,
        attempt: ResetUsageAttempt,
        result: Result<ConsumeAccountRateLimitResetCreditOutcome, String>,
    ) {
        let matches_in_flight = self
            .reset_usage_request_in_flight
            .as_ref()
            .is_some_and(|in_flight| in_flight.idempotency_key == attempt.idempotency_key);
        if matches_in_flight {
            self.reset_usage_request_in_flight = None;
        }
        if result.is_err() {
            self.reset_usage_retry = Some(attempt);
        } else {
            self.reset_usage_retry = None;
        }
        self.pending_reset_usage_refresh = Some(PendingResetUsageRefresh { result });
        self.refresh_status_line();
    }

    pub(crate) fn finish_reset_usage_rate_limit_refresh(&mut self, refresh_succeeded: bool) {
        let Some(pending) = self.pending_reset_usage_refresh.take() else {
            return;
        };
        if !refresh_succeeded {
            self.rate_limit_reset_credits = None;
            self.rate_limit_snapshots_by_limit_id.remove("codex");
            self.refresh_status_line();
        }

        let message = match pending.result {
            Ok(ConsumeAccountRateLimitResetCreditOutcome::Reset) => {
                if refresh_succeeded {
                    "Usage reset. Fresh limits are now loaded.".to_string()
                } else {
                    "Usage reset, but the follow-up refresh failed. Run /status before using another reset.".to_string()
                }
            }
            Ok(ConsumeAccountRateLimitResetCreditOutcome::AlreadyRedeemed) => {
                if refresh_succeeded {
                    "This reset attempt was already redeemed. Fresh limits are now loaded."
                        .to_string()
                } else {
                    "This reset attempt was already redeemed, but the follow-up refresh failed. Run /status before trying again.".to_string()
                }
            }
            Ok(ConsumeAccountRateLimitResetCreditOutcome::NothingToReset) => {
                "No eligible usage window needed a reset.".to_string()
            }
            Ok(ConsumeAccountRateLimitResetCreditOutcome::NoCredit) => {
                "No rate-limit resets are available.".to_string()
            }
            Err(_) => {
                if refresh_succeeded {
                    "Could not reset usage. Fresh limits are loaded; run /reset-usage to retry if still eligible.".to_string()
                } else {
                    "Could not reset usage, and the follow-up refresh failed. Run /status before retrying.".to_string()
                }
            }
        };
        self.add_info_message(message, /*hint*/ None);
    }

    fn refresh_reset_usage_data(&mut self, message: &str) {
        self.add_info_message(message.to_string(), /*hint*/ None);
        self.app_event_tx.send(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StartupPrefetch,
        });
    }
}
