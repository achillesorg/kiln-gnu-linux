//! The tray/notification **status feed**.
//!
//! The shell's tray zone and notification badge must reflect what the service
//! really knows — the StatusNotifier items the watcher registered and the
//! notification history the server stored — never invented content. This module
//! turns the two IPC replies into the shell's display models, and [`poll_status`]
//! does the round trip over any [`ControlClient`].
//!
//! Because [`crate::socket_client::InProcessClient`] runs the *real*
//! [`glaze_service::ipc_server::IpcServer::handle`] in-process, the round trip is
//! fully unit-tested on any host: seed a tray item and a notification into the
//! service state, poll, and assert the badge and tray model.

use glaze_common::ipc::{Request, Response};
use glaze_common::model::{NotificationInfo, TrayCategoryInfo, TrayItemInfo, TrayStatusInfo};
use glaze_service::tray::{TrayCategory, TrayStatus};

use crate::notifications::NotificationBadge;
use crate::socket_client::{ClientError, ControlClient};
use crate::tray::{TrayItem, TrayModel};

/// The shell's view of the service's status state.
///
/// (`TrayModel` and `NotificationBadge` are `Debug + Clone`, not `PartialEq`;
/// assert on the fields directly in tests.)
#[derive(Debug, Clone)]
pub struct StatusSnapshot {
    /// Real StatusNotifier items, in the service's stable order.
    pub tray: TrayModel,
    /// Unread-notification badge state.
    pub badge: NotificationBadge,
    /// The notification history, newest last, for the shell's list UI.
    pub notifications: Vec<NotificationInfo>,
    /// Whether the notification server is in Do-Not-Disturb mode.
    pub dnd: bool,
}

impl StatusSnapshot {
    /// An empty snapshot (what the shell shows when no service is reachable).
    pub fn empty() -> Self {
        StatusSnapshot {
            tray: TrayModel::new(),
            badge: NotificationBadge::new(),
            notifications: Vec::new(),
            dnd: false,
        }
    }

    /// True when `notification` is still unread according to the badge state.
    pub fn is_unread(&self, notification: &NotificationInfo) -> bool {
        self.badge
            .unread_ids()
            .iter()
            .any(|id| id == &notification.id.to_string())
    }
}

/// Map the wire tray status onto the service's display enum.
fn tray_status(status: TrayStatusInfo) -> TrayStatus {
    match status {
        TrayStatusInfo::Passive => TrayStatus::Passive,
        TrayStatusInfo::Active => TrayStatus::Active,
        TrayStatusInfo::NeedsAttention => TrayStatus::NeedsAttention,
    }
}

/// Map the wire tray category onto the service's display enum.
fn tray_category(category: TrayCategoryInfo) -> TrayCategory {
    match category {
        TrayCategoryInfo::ApplicationStatus => TrayCategory::ApplicationStatus,
        TrayCategoryInfo::Communications => TrayCategory::Communications,
        TrayCategoryInfo::SystemServices => TrayCategory::SystemServices,
        TrayCategoryInfo::Hardware => TrayCategory::Hardware,
        TrayCategoryInfo::Other => TrayCategory::Other,
    }
}

/// Convert one wire item into the shell's tray item.
fn to_tray_item(info: &TrayItemInfo) -> TrayItem {
    let mut item = TrayItem::new(info.id.clone(), info.title.clone());
    item.icon = info.icon.clone();
    item.status = tray_status(info.status);
    item.category = tray_category(info.category);
    item
}

/// Build a snapshot from the two replies, or report why they did not fit.
pub fn snapshot_from(
    tray: &Response,
    notifications: &Response,
) -> Result<StatusSnapshot, ClientError> {
    let items = match tray {
        Response::Tray { items } => items,
        Response::Error { code, message } => {
            return Err(ClientError::Service {
                code: *code,
                message: message.clone(),
            })
        }
        other => {
            return Err(ClientError::Protocol(format!(
                "expected Tray, got {other:?}"
            )))
        }
    };
    // Bound with a distinct name: `items` above already holds the tray items.
    let (history, unread_ids, dnd) = match notifications {
        Response::Notifications {
            items,
            unread_ids,
            dnd,
        } => (items, unread_ids, *dnd),
        Response::Error { code, message } => {
            return Err(ClientError::Service {
                code: *code,
                message: message.clone(),
            })
        }
        other => {
            return Err(ClientError::Protocol(format!(
                "expected Notifications, got {other:?}"
            )))
        }
    };

    let mut tray_model = TrayModel::new();
    for info in items {
        tray_model.upsert(to_tray_item(info));
    }

    let mut badge = NotificationBadge::new();
    if dnd {
        badge.set_dnd(true);
    }
    for id in unread_ids {
        badge.push(id.to_string());
    }

    Ok(StatusSnapshot {
        tray: tray_model,
        badge,
        notifications: history.to_vec(),
        dnd,
    })
}

/// Ask the service for its tray and notification state.
pub fn poll_status<C: ControlClient + ?Sized>(
    client: &mut C,
) -> Result<StatusSnapshot, ClientError> {
    let tray = client.request(&Request::ListTray)?;
    let notifications = client.request(&Request::ListNotifications)?;
    snapshot_from(&tray, &notifications)
}

/// Clear the unread badge in the service (the user opened the notifications).
pub fn mark_all_read<C: ControlClient + ?Sized>(client: &mut C) -> Result<(), ClientError> {
    client.request_expect_ok(&Request::MarkNotificationsRead)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use glaze_common::ipc::ErrorCode;
    use glaze_service::notifications::NotifyRequest;
    use glaze_service::registry::AppRegistry;
    use glaze_service::tray::TrayItem as ServiceTrayItem;

    use crate::socket_client::InProcessClient;

    fn volume_item(status: TrayStatus) -> ServiceTrayItem {
        ServiceTrayItem {
            id: "org.glaze.Volume:/StatusNotifierItem".into(),
            service: "org.glaze.Volume".into(),
            path: "/StatusNotifierItem".into(),
            title: "Volume".into(),
            icon: Some("audio-volume-high".into()),
            status,
            category: TrayCategory::Hardware,
        }
    }

    fn notify_request(summary: &str) -> NotifyRequest {
        NotifyRequest {
            app_name: "Test".into(),
            summary: summary.into(),
            body: "body".into(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_service_yields_an_empty_snapshot() {
        let mut client = InProcessClient::new(AppRegistry::default());
        let snapshot = poll_status(&mut client).unwrap();
        assert!(snapshot.tray.is_empty());
        assert_eq!(snapshot.badge.unread_count(), 0);
        assert_eq!(snapshot.badge.badge_text(), None);
        assert!(!snapshot.dnd);
    }

    #[test]
    fn seeded_tray_item_and_notification_round_trip() {
        let mut client = InProcessClient::new(AppRegistry::default());
        {
            let state = client.state();
            let mut guard = state.lock().unwrap();
            guard
                .tray_mut()
                .registry_mut()
                .upsert(volume_item(TrayStatus::NeedsAttention));
            guard
                .notifications_mut()
                .notify(notify_request("Charged"), 1_700_000_000);
            guard
                .notifications_mut()
                .notify(notify_request("Update ready"), 1_700_000_010);
        }

        let snapshot = poll_status(&mut client).unwrap();
        assert_eq!(snapshot.tray.len(), 1);
        let item = snapshot.tray.iter().next().unwrap();
        assert_eq!(item.title, "Volume");
        assert_eq!(item.icon.as_deref(), Some("audio-volume-high"));
        assert!(item.needs_attention(), "status must survive the wire");
        assert_eq!(snapshot.tray.needs_attention(), 1);

        assert_eq!(snapshot.badge.unread_count(), 2);
        assert_eq!(snapshot.badge.badge_text().as_deref(), Some("2"));
    }

    /// The full path: a StatusNotifier watcher signal becomes a tray item in the
    /// shell's model, through the real request handler — no shortcuts.
    #[test]
    fn watcher_signals_reach_the_shell_model() {
        use glaze_service::tray_watcher::SniSignal;

        let mut client = InProcessClient::new(AppRegistry::default());
        let id = "org.glaze.Volume:/StatusNotifierItem".to_string();
        {
            let state = client.state();
            let mut guard = state.lock().unwrap();
            guard.apply_tray_signal(SniSignal::RegisterItem {
                service_or_path: "org.glaze.Volume".into(),
                sender: None,
            });
            guard.apply_tray_signal(SniSignal::NewTitle {
                id: id.clone(),
                title: "Volume".into(),
            });
            guard.apply_tray_signal(SniSignal::NewIcon {
                id: id.clone(),
                icon: Some("audio-volume-high".into()),
            });
            guard.apply_tray_signal(SniSignal::NewStatus {
                id: id.clone(),
                status: TrayStatus::NeedsAttention,
            });
        }

        let snapshot = poll_status(&mut client).unwrap();
        assert_eq!(snapshot.tray.len(), 1);
        let item = snapshot.tray.iter().next().unwrap();
        assert_eq!(item.id, id);
        assert_eq!(item.title, "Volume");
        assert_eq!(item.icon.as_deref(), Some("audio-volume-high"));
        assert!(item.needs_attention());
    }

    #[test]
    fn the_snapshot_carries_the_notification_history() {
        let mut client = InProcessClient::new(AppRegistry::default());
        {
            let state = client.state();
            let mut guard = state.lock().unwrap();
            guard
                .notifications_mut()
                .notify(notify_request("Charged"), 1_700_000_000);
            guard
                .notifications_mut()
                .notify(notify_request("Update ready"), 1_700_000_010);
        }

        let snapshot = poll_status(&mut client).unwrap();
        assert_eq!(snapshot.notifications.len(), 2);
        assert_eq!(snapshot.notifications[0].summary, "Charged");
        assert_eq!(snapshot.notifications[1].summary, "Update ready");
        // Both are unread, so the list UI can mark them.
        assert!(snapshot.notifications.iter().all(|n| snapshot.is_unread(n)));

        mark_all_read(&mut client).unwrap();
        let after = poll_status(&mut client).unwrap();
        assert_eq!(after.notifications.len(), 2, "history is kept");
        assert!(
            after.notifications.iter().all(|n| !after.is_unread(n)),
            "none are unread after marking"
        );
    }

    #[test]
    fn marking_read_clears_the_badge() {
        let mut client = InProcessClient::new(AppRegistry::default());
        {
            let state = client.state();
            let mut guard = state.lock().unwrap();
            guard
                .notifications_mut()
                .notify(notify_request("Hello"), 1_700_000_000);
        }
        assert_eq!(poll_status(&mut client).unwrap().badge.unread_count(), 1);

        mark_all_read(&mut client).unwrap();
        let after = poll_status(&mut client).unwrap();
        assert_eq!(after.badge.unread_count(), 0);
        assert_eq!(after.badge.badge_text(), None);
    }

    #[test]
    fn dnd_suppresses_the_badge_text_but_not_the_count() {
        let mut client = InProcessClient::new(AppRegistry::default());
        {
            let state = client.state();
            let mut guard = state.lock().unwrap();
            guard.dnd = true;
            guard
                .notifications_mut()
                .notify(notify_request("Quiet"), 1_700_000_000);
        }
        let snapshot = poll_status(&mut client).unwrap();
        assert!(snapshot.dnd);
        assert_eq!(snapshot.badge.unread_count(), 1);
        assert_eq!(snapshot.badge.badge_text(), None, "DND hides the badge");
    }

    #[test]
    fn badge_caps_at_99_plus() {
        let mut client = InProcessClient::new(AppRegistry::default());
        {
            let state = client.state();
            let mut guard = state.lock().unwrap();
            // The default history holds 64; give this server room for the cap.
            guard.notifications = glaze_service::notifications::NotificationServer::new(200);
            for i in 0..120 {
                guard
                    .notifications_mut()
                    .notify(notify_request(&format!("n{i}")), 1_700_000_000 + i);
            }
        }
        let snapshot = poll_status(&mut client).unwrap();
        assert_eq!(snapshot.badge.badge_text().as_deref(), Some("99+"));
    }

    #[test]
    fn a_service_error_becomes_a_service_client_error() {
        let err = snapshot_from(
            &Response::error(ErrorCode::Internal, "boom"),
            &Response::Notifications {
                items: vec![],
                unread_ids: vec![],
                dnd: false,
            },
        )
        .unwrap_err();
        match err {
            ClientError::Service { code, .. } => assert_eq!(code, ErrorCode::Internal),
            other => panic!("expected a service error, got {other:?}"),
        }
    }

    #[test]
    fn a_wrong_variant_becomes_a_protocol_error() {
        let err = snapshot_from(
            &Response::Pong,
            &Response::Notifications {
                items: vec![],
                unread_ids: vec![],
                dnd: false,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ClientError::Protocol(_)));
    }
}
