use std::time::Duration;
use std::time::Instant;

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;

use super::CancellationEvent;
use super::ListSelectionView;
use super::SelectionAction;
use super::SelectionItem;
use super::SelectionViewParams;
use super::bottom_pane_view::BottomPaneView;
use super::bottom_pane_view::ViewCompletion;
use super::popup_consts::standard_popup_hint_line_for_keymap;
use crate::app_event::AppEvent;
use crate::app_event::ResetUsageAttempt;
use crate::app_event_sender::AppEventSender;
use crate::keymap::ListKeymap;
use crate::render::renderable::Renderable;
use crate::status::format_reset_credit_count;

const RESET_USAGE_CONTROL_GUARD: Duration = Duration::from_millis(500);

#[derive(Clone, Debug)]
pub(crate) struct ResetUsageConfirmParams {
    pub(crate) attempt: ResetUsageAttempt,
}

pub(crate) struct ResetUsageConfirmView {
    inner: ListSelectionView,
    opened_at: Instant,
}

impl ResetUsageConfirmView {
    pub(crate) fn new(
        params: ResetUsageConfirmParams,
        app_event_tx: AppEventSender,
        list_keymap: ListKeymap,
    ) -> Self {
        Self::new_at(params, app_event_tx, list_keymap, Instant::now())
    }

    #[cfg(test)]
    pub(crate) fn new_at(
        params: ResetUsageConfirmParams,
        app_event_tx: AppEventSender,
        list_keymap: ListKeymap,
        opened_at: Instant,
    ) -> Self {
        Self::new_at_inner(params, app_event_tx, list_keymap, opened_at)
    }

    #[cfg(not(test))]
    fn new_at(
        params: ResetUsageConfirmParams,
        app_event_tx: AppEventSender,
        list_keymap: ListKeymap,
        opened_at: Instant,
    ) -> Self {
        Self::new_at_inner(params, app_event_tx, list_keymap, opened_at)
    }

    fn new_at_inner(
        params: ResetUsageConfirmParams,
        app_event_tx: AppEventSender,
        list_keymap: ListKeymap,
        opened_at: Instant,
    ) -> Self {
        let ResetUsageConfirmParams { attempt } = params;
        let available_count = attempt.available_count;
        let remaining_percent = attempt.remaining_percent;
        let reset_attempt = attempt;
        let reset_actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
            tx.send(AppEvent::ResetUsageConfirmed {
                attempt: reset_attempt.clone(),
            });
        })];

        let items = vec![
            SelectionItem {
                name: "Cancel".to_string(),
                description: Some("Keep current usage and do not spend a reset.".to_string()),
                is_default: true,
                dismiss_on_select: true,
                ..Default::default()
            },
            SelectionItem {
                name: "Reset usage".to_string(),
                description: Some(format!(
                    "Spend 1 of {}. This cannot be undone.",
                    format_reset_credit_count(available_count)
                )),
                actions: reset_actions,
                dismiss_on_select: true,
                ..Default::default()
            },
        ];

        let prompt = if remaining_percent <= 1 {
            "Reset your current usage window?".to_string()
        } else {
            format!("You still have {remaining_percent}% of your weekly limit left. Reset anyway?")
        };

        let inner = ListSelectionView::new(
            SelectionViewParams {
                view_id: Some("reset-usage-confirm"),
                title: Some("Reset usage".to_string()),
                subtitle: Some(prompt),
                footer_note: Some(Line::from(
                    "Controls are briefly locked after opening to avoid accidental key repeats.",
                )),
                footer_hint: Some(standard_popup_hint_line_for_keymap(&list_keymap)),
                items,
                initial_selected_idx: Some(0),
                ..Default::default()
            },
            app_event_tx,
            list_keymap,
        );

        Self { inner, opened_at }
    }

    fn is_armed_at(&self, now: Instant) -> bool {
        now.duration_since(self.opened_at) >= RESET_USAGE_CONTROL_GUARD
    }

    #[cfg(test)]
    pub(crate) fn handle_key_event_at(&mut self, key_event: KeyEvent, now: Instant) {
        if !self.is_armed_at(now) {
            return;
        }
        self.inner.handle_key_event(key_event);
    }
}

impl BottomPaneView for ResetUsageConfirmView {
    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if !self.is_armed_at(Instant::now()) {
            return;
        }
        self.inner.handle_key_event(key_event);
    }

    fn is_complete(&self) -> bool {
        self.inner.is_complete()
    }

    fn completion(&self) -> Option<ViewCompletion> {
        self.inner.completion()
    }

    fn view_id(&self) -> Option<&'static str> {
        self.inner.view_id()
    }

    fn selected_index(&self) -> Option<usize> {
        self.inner.selected_index()
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        if !self.is_armed_at(Instant::now()) {
            return CancellationEvent::Handled;
        }
        self.inner.on_ctrl_c()
    }

    fn prefer_esc_to_handle_key_event(&self) -> bool {
        true
    }

    fn handle_paste(&mut self, _pasted: String) -> bool {
        false
    }

    fn terminal_title_requires_action(&self) -> bool {
        true
    }

    fn next_frame_delay(&self) -> Option<Duration> {
        let elapsed = Instant::now().duration_since(self.opened_at);
        (elapsed < RESET_USAGE_CONTROL_GUARD).then_some(RESET_USAGE_CONTROL_GUARD - elapsed)
    }
}

impl Renderable for ResetUsageConfirmView {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.inner.render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.inner.desired_height(width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::RuntimeKeymap;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyModifiers;
    use pretty_assertions::assert_eq;
    use tokio::sync::mpsc::unbounded_channel;

    fn attempt() -> ResetUsageAttempt {
        ResetUsageAttempt {
            idempotency_key: "attempt-1".to_string(),
            available_count: 2,
            remaining_percent: 10,
        }
    }

    #[test]
    fn guard_blocks_immediate_enter() {
        let (tx, mut rx) = unbounded_channel();
        let opened_at = Instant::now();
        let mut view = ResetUsageConfirmView::new_at(
            ResetUsageConfirmParams { attempt: attempt() },
            AppEventSender::new(tx),
            RuntimeKeymap::defaults().list,
            opened_at,
        );

        view.handle_key_event_at(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            opened_at + RESET_USAGE_CONTROL_GUARD - Duration::from_millis(1),
        );

        assert!(!view.is_complete());
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn default_enter_after_guard_cancels() {
        let (tx, mut rx) = unbounded_channel();
        let opened_at = Instant::now();
        let mut view = ResetUsageConfirmView::new_at(
            ResetUsageConfirmParams { attempt: attempt() },
            AppEventSender::new(tx),
            RuntimeKeymap::defaults().list,
            opened_at,
        );

        view.handle_key_event_at(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            opened_at + RESET_USAGE_CONTROL_GUARD,
        );

        assert_eq!(view.completion(), Some(ViewCompletion::Accepted));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn reset_row_sends_confirmed_event_after_guard() {
        let (tx, mut rx) = unbounded_channel();
        let opened_at = Instant::now();
        let attempt = attempt();
        let mut view = ResetUsageConfirmView::new_at(
            ResetUsageConfirmParams {
                attempt: attempt.clone(),
            },
            AppEventSender::new(tx),
            RuntimeKeymap::defaults().list,
            opened_at,
        );

        let armed_at = opened_at + RESET_USAGE_CONTROL_GUARD;
        view.handle_key_event_at(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), armed_at);
        view.handle_key_event_at(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), armed_at);

        match rx.try_recv() {
            Ok(AppEvent::ResetUsageConfirmed {
                attempt: actual_attempt,
            }) => assert_eq!(actual_attempt, attempt),
            other => panic!("expected reset confirmation event, got {other:?}"),
        }
    }

    #[test]
    fn pasted_newline_is_inert() {
        let (tx, mut rx) = unbounded_channel();
        let opened_at = Instant::now();
        let mut view = ResetUsageConfirmView::new_at(
            ResetUsageConfirmParams { attempt: attempt() },
            AppEventSender::new(tx),
            RuntimeKeymap::defaults().list,
            opened_at,
        );

        assert!(!view.handle_paste("\n".to_string()));
        assert!(rx.try_recv().is_err());
    }
}
