use super::*;
use assert_matches::assert_matches;
use codex_app_server_protocol::ConsumeAccountRateLimitResetCreditOutcome;
use codex_app_server_protocol::RateLimitResetCreditsSummary;
use codex_utils_path_uri::PathUri;

fn reset_attempt(idempotency_key: &str) -> ResetUsageAttempt {
    ResetUsageAttempt {
        idempotency_key: idempotency_key.to_string(),
        available_count: 1,
        remaining_percent: 10,
    }
}

fn snapshot_with_reset_windows(
    five_hour_used_percent: f64,
    weekly_used_percent: f64,
) -> RateLimitSnapshot {
    RateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: Some(RateLimitWindow {
            used_percent: five_hour_used_percent.round() as i32,
            window_duration_mins: Some(5 * 60),
            resets_at: None,
        }),
        secondary: Some(RateLimitWindow {
            used_percent: weekly_used_percent.round() as i32,
            window_duration_mins: Some(7 * 24 * 60),
            resets_at: None,
        }),
        credits: None,
        individual_limit: None,
        plan_type: None,
        rate_limit_reached_type: None,
    }
}

#[tokio::test]
async fn status_command_renders_immediately_and_refreshes_rate_limits_for_chatgpt_auth() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected status output before refresh request, got {other:?}"),
    };
    assert!(
        !rendered.contains("refreshing limits"),
        "expected /status to avoid transient refresh text in terminal history, got: {rendered}"
    );
    let request_id = match rx.try_recv() {
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        }) => request_id,
        other => panic!("expected rate-limit refresh request, got {other:?}"),
    };
    pretty_assertions::assert_eq!(request_id, 0);
}

#[tokio::test]
async fn status_command_shows_reset_row_for_chatgpt_auth_with_provider_auth_disabled() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.config.model_provider.requires_openai_auth = false;

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected status output before refresh request, got {other:?}"),
    };
    assert!(
        rendered.contains("Resets") && rendered.contains("refresh requested"),
        "expected /status to render reset state before refreshed data arrives, got: {rendered}"
    );
    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { .. },
        })
    );
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::ResetUsageConfirmed { .. })),
        "/status must never send a reset consume event"
    );
}

#[tokio::test]
async fn status_command_refresh_updates_cached_limits_for_future_status_outputs() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);

    chat.dispatch_command(SlashCommand::Status);

    match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(_)) => {}
        other => panic!("expected status output before refresh request, got {other:?}"),
    }
    let first_request_id = match rx.try_recv() {
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        }) => request_id,
        other => panic!("expected rate-limit refresh request, got {other:?}"),
    };

    chat.finish_status_rate_limit_refresh(first_request_id, vec![snapshot(/*percent*/ 92.0)]);
    drain_insert_history(&mut rx);

    chat.dispatch_command(SlashCommand::Status);
    let refreshed = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected refreshed status output, got {other:?}"),
    };
    assert!(
        refreshed.contains("8% left"),
        "expected a future /status output to use refreshed cached limits, got: {refreshed}"
    );
}

#[tokio::test]
async fn status_command_shows_reset_guidance_without_consuming_reset() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 2.0, /*weekly_used_percent*/ 90.0,
    )));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 2 }));
    drain_insert_history(&mut rx);

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 120))
        }
        other => panic!("expected status output, got {other:?}"),
    };
    assert!(
        rendered.contains("Resets")
            && rendered.contains("2 resets available; run /reset-usage")
            && rendered.contains("weekly 10% left"),
        "expected /status to render reset guidance, got: {rendered}"
    );
    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { .. },
        })
    );
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::ResetUsageConfirmed { .. })),
        "/status must never send a reset consume event"
    );
}

#[tokio::test]
async fn status_command_uses_weekly_limit_for_reset_lock() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 2.0, /*weekly_used_percent*/ 9.0,
    )));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 2 }));
    drain_insert_history(&mut rx);

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 120))
        }
        other => panic!("expected status output, got {other:?}"),
    };
    assert!(
        rendered.contains("Resets")
            && rendered.contains("locked until 35% or less is left")
            && rendered.contains("weekly 91% left"),
        "expected /status to use weekly remaining usage for reset lock, got: {rendered}"
    );
    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { .. },
        })
    );
}

#[tokio::test]
async fn reset_usage_blocks_when_more_than_35_percent_remains() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 98.0, /*weekly_used_percent*/ 64.0,
    )));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 1 }));

    chat.dispatch_command(SlashCommand::ResetUsage);

    assert_eq!(chat.bottom_pane.active_view_id(), None);
    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("locked until 35% or less is left (weekly 36% left)"),
        "expected /reset-usage to refuse while more than 35% remains, got: {rendered}"
    );
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::ResetUsageConfirmed { .. })),
        "blocked /reset-usage must not consume a reset"
    );
}

#[tokio::test]
async fn reset_usage_refuses_when_weekly_window_is_missing() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 98.0)));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 1 }));

    chat.dispatch_command(SlashCommand::ResetUsage);

    assert_eq!(chat.bottom_pane.active_view_id(), None);
    let mut saw_message = false;
    loop {
        match rx.try_recv() {
            Ok(AppEvent::InsertHistoryCell(_)) => saw_message = true,
            Ok(AppEvent::RefreshRateLimits {
                origin: RateLimitRefreshOrigin::StartupPrefetch { .. },
            }) => break,
            other => panic!("expected reset usage refresh request, got {other:?}"),
        }
    }
    assert!(saw_message);
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::ResetUsageConfirmed { .. })),
        "/reset-usage must not consume a reset without weekly usage data"
    );
}

#[tokio::test]
async fn reset_usage_refreshes_and_refuses_when_data_is_missing() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);

    chat.dispatch_command(SlashCommand::ResetUsage);

    assert_eq!(chat.bottom_pane.active_view_id(), None);
    assert_matches!(rx.try_recv(), Ok(AppEvent::InsertHistoryCell(_)));
    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StartupPrefetch { .. },
        })
    );
}

#[tokio::test]
async fn reset_usage_opens_confirmation_at_35_percent_or_less_without_consuming() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 10.0, /*weekly_used_percent*/ 65.0,
    )));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 1 }));

    chat.dispatch_command(SlashCommand::ResetUsage);

    assert_eq!(
        chat.bottom_pane.active_view_id(),
        Some("reset-usage-confirm")
    );
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::ResetUsageConfirmed { .. })),
        "opening confirmation must not consume a reset"
    );
}

#[tokio::test]
async fn reset_usage_refuses_while_consume_request_is_in_flight() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 90.0, /*weekly_used_percent*/ 90.0,
    )));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 1 }));

    assert!(chat.start_reset_usage_request(reset_attempt("attempt-1")));
    chat.dispatch_command(SlashCommand::ResetUsage);

    assert_eq!(chat.bottom_pane.active_view_id(), None);
    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("already in progress"),
        "expected /reset-usage to refuse while consume is in flight, got: {rendered}"
    );
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::ResetUsageConfirmed { .. })),
        "in-flight /reset-usage must not consume another reset"
    );
}

#[tokio::test]
async fn reset_usage_refuses_while_post_consume_refresh_is_pending() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 90.0, /*weekly_used_percent*/ 90.0,
    )));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 1 }));

    let attempt = reset_attempt("attempt-1");
    assert!(chat.start_reset_usage_request(attempt.clone()));
    chat.finish_reset_usage_request(
        attempt,
        Ok(ConsumeAccountRateLimitResetCreditOutcome::Reset),
    );
    chat.dispatch_command(SlashCommand::ResetUsage);

    assert_eq!(chat.bottom_pane.active_view_id(), None);
    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("already in progress"),
        "expected /reset-usage to refuse during post-consume refresh, got: {rendered}"
    );
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::ResetUsageConfirmed { .. })),
        "pending-refresh /reset-usage must not consume another reset"
    );
}

#[tokio::test]
async fn reset_usage_failed_post_consume_refresh_invalidates_cached_eligibility() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 90.0, /*weekly_used_percent*/ 90.0,
    )));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 1 }));

    let attempt = reset_attempt("attempt-1");
    assert!(chat.start_reset_usage_request(attempt.clone()));
    chat.finish_reset_usage_request(
        attempt,
        Ok(ConsumeAccountRateLimitResetCreditOutcome::Reset),
    );
    chat.finish_reset_usage_rate_limit_refresh(/*refresh_succeeded*/ false);
    drain_insert_history(&mut rx);

    chat.dispatch_command(SlashCommand::ResetUsage);

    assert_eq!(chat.bottom_pane.active_view_id(), None);
    assert_matches!(rx.try_recv(), Ok(AppEvent::InsertHistoryCell(_)));
    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StartupPrefetch { .. },
        })
    );
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::ResetUsageConfirmed { .. })),
        "failed-refresh /reset-usage must require fresh data before another consume"
    );
}

#[tokio::test]
async fn reset_usage_revalidates_eligibility_when_confirmation_is_accepted() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 90.0, /*weekly_used_percent*/ 90.0,
    )));
    chat.on_rate_limit_reset_credits(Some(RateLimitResetCreditsSummary { available_count: 1 }));
    chat.on_rate_limit_snapshot(Some(snapshot_with_reset_windows(
        /*five_hour_used_percent*/ 90.0, /*weekly_used_percent*/ 63.0,
    )));

    assert!(!chat.start_reset_usage_request(reset_attempt("attempt-1")));

    let rendered = drain_insert_history(&mut rx)
        .into_iter()
        .map(|lines| lines_to_single_string(&lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("reset is now locked until 35% or less is left (weekly 37% left)"),
        "expected confirmation-time revalidation to block changed eligibility, got: {rendered}"
    );
    assert_eq!(chat.reset_usage_request_in_flight, None);
}

#[tokio::test]
async fn status_command_renders_immediately_without_rate_limit_refresh() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Status);

    assert_matches!(rx.try_recv(), Ok(AppEvent::InsertHistoryCell(_)));
    assert!(
        !std::iter::from_fn(|| rx.try_recv().ok())
            .any(|event| matches!(event, AppEvent::RefreshRateLimits { .. })),
        "non-ChatGPT sessions should not request a rate-limit refresh for /status"
    );
}

#[tokio::test]
async fn status_command_uses_catalog_default_reasoning_when_config_empty() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.4")).await;
    chat.config.model_reasoning_effort = None;

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected status output, got {other:?}"),
    };
    assert!(
        rendered.contains("gpt-5.4 (reasoning medium, summaries auto)"),
        "expected /status to render the catalog default reasoning effort, got: {rendered}"
    );
}

#[tokio::test]
async fn status_command_renders_native_and_foreign_instruction_sources() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let (foreign_source, foreign_display) = if cfg!(windows) {
        (
            PathUri::parse("file:///remote/AGENTS.md").expect("POSIX instruction source"),
            "/remote/AGENTS.md",
        )
    } else {
        (
            PathUri::parse("file:///C:/remote/AGENTS.md").expect("Windows instruction source"),
            r"C:\remote\AGENTS.md",
        )
    };
    chat.instruction_source_paths = vec![
        PathUri::from_abs_path(&chat.config.cwd.join("AGENTS.md")),
        foreign_source,
    ];

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected status output, got {other:?}"),
    };
    assert!(
        rendered.contains(&format!("AGENTS.md, {foreign_display}")),
        "expected /status to show native-relative and environment-native foreign paths, got: {rendered}"
    );
    assert!(
        !rendered.contains("Agents.md  <none>"),
        "expected /status to avoid stale <none> when app-server provided instruction sources, got: {rendered}"
    );
}

#[tokio::test]
async fn status_command_overlapping_refreshes_update_matching_cells_only() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);

    chat.dispatch_command(SlashCommand::Status);
    match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(_)) => {}
        other => panic!("expected first status output, got {other:?}"),
    }
    let first_request_id = match rx.try_recv() {
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        }) => request_id,
        other => panic!("expected first refresh request, got {other:?}"),
    };

    chat.dispatch_command(SlashCommand::Status);
    let second_rendered = match rx.try_recv() {
        Ok(AppEvent::InsertHistoryCell(cell)) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected second status output, got {other:?}"),
    };
    let second_request_id = match rx.try_recv() {
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        }) => request_id,
        other => panic!("expected second refresh request, got {other:?}"),
    };

    assert_ne!(first_request_id, second_request_id);
    assert!(
        !second_rendered.contains("refreshing limits"),
        "expected /status to avoid transient refresh text in terminal history, got: {second_rendered}"
    );

    chat.finish_status_rate_limit_refresh(first_request_id, Vec::new());
    pretty_assertions::assert_eq!(chat.refreshing_status_outputs.len(), 1);

    chat.finish_status_rate_limit_refresh(second_request_id, vec![snapshot(/*percent*/ 92.0)]);
    assert!(chat.refreshing_status_outputs.is_empty());
}

#[tokio::test]
async fn account_update_rejects_stale_status_rate_limit_snapshots() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);
    chat.dispatch_command(SlashCommand::Status);
    assert_matches!(rx.try_recv(), Ok(AppEvent::InsertHistoryCell(_)));
    let request_id = match rx.try_recv() {
        Ok(AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        }) => request_id,
        other => panic!("expected status refresh request, got {other:?}"),
    };

    chat.update_account_state(
        /*status_account_display*/ None, /*plan_type*/ None,
        /*has_chatgpt_account*/ true, /*has_codex_backend_auth*/ true,
    );
    chat.finish_status_rate_limit_refresh(request_id, vec![snapshot(/*percent*/ 92.0)]);

    assert!(chat.rate_limit_snapshots_by_limit_id.is_empty());
}
