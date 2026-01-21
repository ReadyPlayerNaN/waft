use std::sync::Arc;
use std::time::{Duration, SystemTime};

use relm4::AsyncReducible;

use sacrebleui::features::notifications::dbus::hints::Hints;
use sacrebleui::features::notifications::dbus::ingress::IngressedNotification;
use sacrebleui::features::notifications::store::{ItemLifecycle, NotificationOp, Reducer, State};
use sacrebleui::features::notifications::types::NotificationUrgency;

fn t(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn hints(urgency: NotificationUrgency) -> Hints {
    Hints {
        action_icons: false,
        category: None,
        desktop_entry: None,
        image_data: None,
        image_path: None,
        resident: false,
        sound_file: None,
        sound_name: None,
        suppress_sound: false,
        transient: false,
        urgency,
        x: 0,
        y: 0,
    }
}

fn ingress(
    id: u64,
    app_name: Option<&str>,
    created_at: SystemTime,
    urgency: NotificationUrgency,
    ttl: Option<u64>,
) -> IngressedNotification {
    IngressedNotification {
        app_name: app_name.map(|s| Arc::<str>::from(s)),
        actions: vec![],
        created_at,
        description: Arc::<str>::from("desc"),
        icon: None,
        id,
        hints: hints(urgency),
        replaces_id: None,
        title: Arc::<str>::from("title"),
        ttl,
    }
}

async fn new_reducer() -> Reducer {
    <Reducer as AsyncReducible>::init().await
}

async fn apply(reducer: &mut Reducer, op: NotificationOp) {
    let _changed = <Reducer as AsyncReducible>::reduce(reducer, op).await;
}

fn group_lifecycle(state: &State, group_id: &Arc<str>) -> Option<ItemLifecycle> {
    state
        .get_groups()
        .into_iter()
        .find(|(g, _l)| g.get_id() == group_id)
        .map(|(_g, l)| l.clone())
}

fn group_top_lifecycle(state: &State, group_id: &Arc<str>, id: u64) -> Option<ItemLifecycle> {
    state
        .get_group_top(group_id)
        .into_iter()
        .find(|(n, _l)| n.id == id)
        .map(|(_n, l)| l.clone())
}

fn toast_lifecycle(state: &State, id: u64) -> Option<ItemLifecycle> {
    state
        .get_toasts()
        .into_iter()
        .find(|(n, _l)| n.id == id)
        .map(|(_n, l)| l.clone())
}

#[tokio::test]
async fn ingress_creates_group_and_first_toast_visible_and_derives_toast_ttl() {
    let mut r = new_reducer().await;

    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            1,
            Some("My App"),
            t(10),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;

    let s = r.get_state();

    // Notification is stored.
    let n = s.get_notification(&1).expect("notification should exist");
    assert_eq!(n.app_ident().as_ref(), "my_app"); // normalization: lowercase + spaces -> "_"
    assert_eq!(n.app_title().as_ref(), "My App");
    assert_eq!(n.toast_ttl, Some(10)); // derived from urgency when ttl=None

    // Group exists and is visible in archive list.
    let gid: Arc<str> = Arc::from("my_app");
    assert!(s.get_group(&gid).is_some());
    assert!(matches!(
        group_lifecycle(s, &gid),
        Some(ItemLifecycle::Visible)
    ));

    // Group top contains this notification as Visible.
    assert!(matches!(
        group_top_lifecycle(s, &gid, 1),
        Some(ItemLifecycle::Visible)
    ));

    // Toast list contains it as Appearing (first toast starts with enter animation).
    assert!(matches!(
        toast_lifecycle(s, 1),
        Some(ItemLifecycle::Appearing)
    ));
}

#[tokio::test]
async fn second_ingress_same_group_keeps_hotter_visible_in_group_top_and_hides_older() {
    let mut r = new_reducer().await;

    // Older first.
    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            1,
            Some("My App"),
            t(1),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;
    // Newer second (same urgency => newer is "hotter").
    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            2,
            Some("My App"),
            t(2),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;

    let s = r.get_state();
    let gid: Arc<str> = Arc::from("my_app");

    // Group top can contain > 1 items (e.g. older one can be Hiding).
    let top = s.get_group_top(&gid);
    assert_eq!(top.len(), 2);

    // The newer one should remain Visible, the older should become Hiding.
    assert!(matches!(
        group_top_lifecycle(s, &gid, 2),
        Some(ItemLifecycle::Visible)
    ));
    assert!(matches!(
        group_top_lifecycle(s, &gid, 1),
        Some(ItemLifecycle::Hiding)
    ));
}

#[tokio::test]
async fn dismiss_then_dismissed_removes_notification_and_marks_group_dismissing_when_last() {
    let mut r = new_reducer().await;

    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            10,
            Some("App"),
            t(1),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;

    let gid: Arc<str> = Arc::from("app");

    // Dismiss marks the card as dismissing (does not remove yet).
    apply(&mut r, NotificationOp::NotificationDismiss(10)).await;
    let s = r.get_state();
    assert!(matches!(
        group_top_lifecycle(s, &gid, 10),
        Some(ItemLifecycle::Dismissing)
    ));
    assert!(matches!(
        toast_lifecycle(s, 10),
        Some(ItemLifecycle::Dismissing)
    ));
    assert!(s.get_notification(&10).is_some());

    // Dismissed completes removal.
    apply(&mut r, NotificationOp::NotificationDismissed(10)).await;
    let s = r.get_state();

    assert!(
        s.get_notification(&10).is_none(),
        "notification should be removed"
    );
    assert!(
        toast_lifecycle(s, 10).is_none(),
        "toast entry should be removed"
    );
    assert!(
        group_top_lifecycle(s, &gid, 10).is_none(),
        "group top entry should be removed"
    );

    // If it was the last notification belonging to this group, the group lifecycle becomes Dismissing.
    assert!(matches!(
        group_lifecycle(s, &gid),
        Some(ItemLifecycle::Dismissing)
    ));
}

#[tokio::test]
async fn group_lifecycle_returns_to_visible_on_new_ingress_after_last_removed() {
    let mut r = new_reducer().await;
    let gid: Arc<str> = Arc::from("app");

    // Create, then remove the only notification in group.
    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            1,
            Some("App"),
            t(1),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;
    apply(&mut r, NotificationOp::NotificationDismissed(1)).await;

    assert!(matches!(
        group_lifecycle(r.get_state(), &gid),
        Some(ItemLifecycle::Dismissing)
    ));

    // New ingress should set lifecycle back to Visible.
    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            2,
            Some("App"),
            t(2),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;

    assert!(matches!(
        group_lifecycle(r.get_state(), &gid),
        Some(ItemLifecycle::Visible)
    ));
}

#[tokio::test]
async fn retract_then_retracted_removes_notification_and_marks_group_dismissing_when_last() {
    let mut r = new_reducer().await;

    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            42,
            Some("Svc"),
            t(1),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;

    let gid: Arc<str> = Arc::from("svc");

    // Retract marks the card as retracting.
    apply(&mut r, NotificationOp::NotificationRetract(42)).await;
    let s = r.get_state();
    assert!(matches!(
        group_top_lifecycle(s, &gid, 42),
        Some(ItemLifecycle::Retracting)
    ));
    assert!(matches!(
        toast_lifecycle(s, 42),
        Some(ItemLifecycle::Retracting)
    ));

    // Retracted completes removal (same storage behavior as dismissed).
    apply(&mut r, NotificationOp::NotificationRetracted(42)).await;
    let s = r.get_state();

    assert!(
        s.get_notification(&42).is_none(),
        "notification should be removed"
    );
    assert!(
        toast_lifecycle(s, 42).is_none(),
        "toast entry should be removed"
    );
    assert!(
        matches!(group_lifecycle(s, &gid), Some(ItemLifecycle::Dismissing)),
        "last notification removed => group becomes Dismissing"
    );
}

#[tokio::test]
async fn unknown_ids_are_ignored_and_do_not_panic() {
    let mut r = new_reducer().await;

    // These should be no-ops and must not panic.
    apply(&mut r, NotificationOp::NotificationDismiss(999)).await;
    apply(&mut r, NotificationOp::NotificationRetract(999)).await;
    apply(&mut r, NotificationOp::NotificationDismissed(999)).await;
    apply(&mut r, NotificationOp::NotificationRetracted(999)).await;
    apply(&mut r, NotificationOp::ToastHide(999)).await;
    apply(&mut r, NotificationOp::ToastHidden(999)).await;

    let s = r.get_state();
    assert!(s.get_groups().is_empty());
    assert!(s.get_toasts().is_empty());
}

#[tokio::test]
async fn toast_cutting_keeps_the_hottest_five_and_may_drop_newer_but_less_urgent_toasts() {
    // `cut_notif_ids()` operates on a list that is already sorted by "hotness"
    // (`Notification::cmp`: urgency first, then created_at).
    //
    // This means a newly ingressed notification is *not guaranteed* to appear as a toast:
    // if the toast slots are already occupied by hotter items (e.g. Critical urgency),
    // a new Normal notification can be postponed (i.e. not shown as a toast).
    let mut r = new_reducer().await;

    // Fill all 5 toast slots with Critical notifications.
    for (id, secs) in [(1u64, 1u64), (2, 2), (3, 3), (4, 4), (5, 5)] {
        apply(
            &mut r,
            NotificationOp::Ingress(ingress(
                id,
                Some("ToastApp"),
                t(secs),
                NotificationUrgency::Critical,
                None,
            )),
        )
        .await;
    }

    {
        let s = r.get_state();
        // First toast starts as Appearing, subsequent ones also start as Appearing
        assert!(matches!(
            toast_lifecycle(s, 1),
            Some(ItemLifecycle::Appearing)
        ));
        assert!(matches!(
            toast_lifecycle(s, 2),
            Some(ItemLifecycle::Appearing)
        ));
        assert!(matches!(
            toast_lifecycle(s, 3),
            Some(ItemLifecycle::Appearing)
        ));
        assert!(matches!(
            toast_lifecycle(s, 4),
            Some(ItemLifecycle::Appearing)
        ));
        assert!(matches!(
            toast_lifecycle(s, 5),
            Some(ItemLifecycle::Appearing)
        ));
    }

    // Now a newer (by timestamp) Normal notification arrives.
    // Despite being newer, it's less hot than the Critical ones, so it may be dropped from toasts.
    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            6,
            Some("ToastApp"),
            t(10),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;

    let s = r.get_state();

    // The notification exists in the store...
    assert!(s.get_notification(&6).is_some());

    // ...it is in toasts as Pending, waiting for a slot to open up.
    assert!(matches!(
        toast_lifecycle(s, 6),
        Some(ItemLifecycle::Pending)
    ));

    // The existing critical toasts remain in Appearing state.
    assert!(matches!(
        toast_lifecycle(s, 1),
        Some(ItemLifecycle::Appearing)
    ));
    assert!(matches!(
        toast_lifecycle(s, 2),
        Some(ItemLifecycle::Appearing)
    ));
    assert!(matches!(
        toast_lifecycle(s, 3),
        Some(ItemLifecycle::Appearing)
    ));
    assert!(matches!(
        toast_lifecycle(s, 4),
        Some(ItemLifecycle::Appearing)
    ));
    assert!(matches!(
        toast_lifecycle(s, 5),
        Some(ItemLifecycle::Appearing)
    ));
}

#[tokio::test]
async fn toast_cutting_allows_new_critical_toast_to_displace_existing_normal_appearing_when_full() {
    let mut r = new_reducer().await;

    // Fill all 5 toast slots with Normal notifications.
    for (id, secs) in [(1u64, 1u64), (2, 2), (3, 3), (4, 4), (5, 5)] {
        apply(
            &mut r,
            NotificationOp::Ingress(ingress(
                id,
                Some("ToastApp"),
                t(secs),
                NotificationUrgency::Normal,
                None,
            )),
        )
        .await;
    }

    {
        let s = r.get_state();
        assert!(matches!(
            toast_lifecycle(s, 1),
            Some(ItemLifecycle::Appearing)
        ));
        assert!(matches!(
            toast_lifecycle(s, 2),
            Some(ItemLifecycle::Appearing)
        ));
        assert!(matches!(
            toast_lifecycle(s, 3),
            Some(ItemLifecycle::Appearing)
        ));
        assert!(matches!(
            toast_lifecycle(s, 4),
            Some(ItemLifecycle::Appearing)
        ));
        assert!(matches!(
            toast_lifecycle(s, 5),
            Some(ItemLifecycle::Appearing)
        ));
    }

    // Now a Critical notification arrives. Even though the toast list is full,
    // the Critical should take a slot (it's hotter), pushing out the coldest Normal.
    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            6,
            Some("ToastApp"),
            t(6),
            NotificationUrgency::Critical,
            None,
        )),
    )
    .await;

    let s = r.get_state();
    assert!(s.get_notification(&6).is_some());

    // Critical should be promoted to Appearing.
    assert!(matches!(
        toast_lifecycle(s, 6),
        Some(ItemLifecycle::Appearing)
    ));

    // The four hottest Normal notifications should remain Appearing (created_at 2, 3, 4, 5).
    assert!(matches!(
        toast_lifecycle(s, 2),
        Some(ItemLifecycle::Appearing)
    ));
    assert!(matches!(
        toast_lifecycle(s, 3),
        Some(ItemLifecycle::Appearing)
    ));
    assert!(matches!(
        toast_lifecycle(s, 4),
        Some(ItemLifecycle::Appearing)
    ));
    assert!(matches!(
        toast_lifecycle(s, 5),
        Some(ItemLifecycle::Appearing)
    ));

    // The coldest Normal (created_at 1) should be pushed above the limit and start hiding.
    assert!(matches!(toast_lifecycle(s, 1), Some(ItemLifecycle::Hiding)));
}

#[tokio::test]
async fn pending_toast_is_promoted_when_slot_opens_via_dismiss() {
    let mut r = new_reducer().await;

    // Fill all 5 toast slots with Critical notifications.
    for (id, secs) in [(1u64, 1u64), (2, 2), (3, 3), (4, 4), (5, 5)] {
        apply(
            &mut r,
            NotificationOp::Ingress(ingress(
                id,
                Some("ToastApp"),
                t(secs),
                NotificationUrgency::Critical,
                None,
            )),
        )
        .await;
    }

    // Add a Normal notification that will be pending (less hot than Critical).
    apply(
        &mut r,
        NotificationOp::Ingress(ingress(
            6,
            Some("ToastApp"),
            t(10),
            NotificationUrgency::Normal,
            None,
        )),
    )
    .await;

    // Verify notification 6 is pending.
    {
        let s = r.get_state();
        assert!(matches!(
            toast_lifecycle(s, 6),
            Some(ItemLifecycle::Pending)
        ));
    }

    // Dismiss the hottest Critical notification (id=5).
    apply(&mut r, NotificationOp::NotificationDismiss(5)).await;
    apply(&mut r, NotificationOp::NotificationDismissed(5)).await;

    // Now notification 6 should be promoted to Appearing.
    let s = r.get_state();
    assert!(
        toast_lifecycle(s, 5).is_none(),
        "dismissed notification should be removed"
    );
    assert!(
        matches!(toast_lifecycle(s, 6), Some(ItemLifecycle::Appearing)),
        "pending notification should be promoted to Appearing when a slot opens"
    );
}
