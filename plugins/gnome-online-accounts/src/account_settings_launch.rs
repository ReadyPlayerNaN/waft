//! Bounded, observable dispatch to system account settings; no credentials handled.
use crate::lifecycle::{GoaLifecycle, failure, unix_now};
use std::{process::Stdio, time::Duration};
use tokio::process::Command;
use waft_plugin::StateLocker;
use waft_protocol::{
    entity::accounts::AccountSettingsLaunch,
    error::{ProtocolError, ProtocolErrorScope},
};

#[derive(Debug, Clone)]
pub enum SettingsTarget {
    Generic,
    Provider(String),
    Account(String),
}
impl SettingsTarget {
    pub fn arguments(&self) -> Vec<String> {
        let mut args = vec!["online-accounts".into()];
        match self {
            Self::Generic => {}
            Self::Provider(id) => args.extend(["add".into(), id.clone()]),
            Self::Account(id) => args.push(id.clone()),
        }
        args
    }
    fn key(&self) -> String {
        match self {
            Self::Generic => "generic".into(),
            Self::Provider(id) => format!("provider:{id}"),
            Self::Account(id) => format!("account:{id}"),
        }
    }
}
fn desktop_override(wayland: bool, desktop: Option<&str>) -> bool {
    wayland
        && !desktop.is_some_and(|s| {
            s.split(':')
                .any(|token| token.eq_ignore_ascii_case("GNOME"))
        })
}

impl GoaLifecycle {
    pub async fn launch_settings(
        &self,
        target: SettingsTarget,
    ) -> anyhow::Result<serde_json::Value> {
        self.launch_program(target, "gnome-control-center").await
    }
    async fn launch_program(
        &self,
        target: SettingsTarget,
        program: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let request = uuid::Uuid::new_v4();
        let key = target.key();
        {
            let mut st = self.state.lock_or_recover();
            if !st.launch_targets.insert(key.clone()) {
                return Err(failure("goa.busy", "Account settings is already opening"));
            }
            st.status.launch_request_id = Some(request);
            st.status.last_launch = Some(unix_now());
            st.status.launch_error = None;
            st.status.launch = AccountSettingsLaunch::Idle;
        }
        self.notifier.notify();
        let mut command = Command::new(program);
        command
            .args(target.arguments())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(false);
        if desktop_override(
            std::env::var_os("WAYLAND_DISPLAY").is_some()
                || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland"),
            std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
        ) {
            command.env("XDG_CURRENT_DESKTOP", "GNOME");
        }
        let child = command.spawn();
        let Ok(mut child) = child else {
            self.state.lock_or_recover().launch_targets.remove(&key);
            self.launch_result(request, false, "goa.launch-unavailable");
            return Err(failure(
                "goa.launch-unavailable",
                "Install GNOME Settings to manage accounts",
            ));
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let lifecycle = self.clone();
        // This task deliberately outlives cancellation of the requesting action.
        waft_plugin::spawn_monitored("goa/account-settings-child", async move {
            let initial = tokio::select! {
                status = child.wait() => Some(status),
                _ = tokio::time::sleep(Duration::from_millis(250)) => None,
            };
            let accepted = initial
                .as_ref()
                .is_none_or(|result| result.as_ref().is_ok_and(std::process::ExitStatus::success));
            lifecycle.launch_result(request, accepted, "goa.launch-failed");
            if tx.send(accepted).is_err() {
                log::debug!("[goa] settings launch requester cancelled; child still supervised");
            }
            if initial.is_none() {
                match child.wait().await {
                    Ok(status) if status.success() => {}
                    Ok(_) | Err(_) => lifecycle.launch_result(request, false, "goa.launch-failed"),
                }
            }
            lifecycle
                .state
                .lock_or_recover()
                .launch_targets
                .remove(&key);
            log::debug!("[goa] account-settings child reaped");
            lifecycle.notifier.notify();
            Ok(())
        });
        if rx.await.unwrap_or(false) {
            Ok(serde_json::json!({"outcome":"launch-accepted", "launch_request_id":request}))
        } else {
            Err(failure(
                "goa.launch-failed",
                "Account settings could not be opened",
            ))
        }
    }
    fn launch_result(&self, request: uuid::Uuid, accepted: bool, code: &str) {
        {
            let mut st = self.state.lock_or_recover();
            if st.status.launch_request_id != Some(request) {
                return;
            }
            st.status.launch = if accepted {
                AccountSettingsLaunch::Accepted
            } else {
                AccountSettingsLaunch::Failed
            };
            st.status.launch_error = (!accepted).then(|| {
                ProtocolError::new(
                    code,
                    "Account settings could not be opened",
                    ProtocolErrorScope::Action,
                    false,
                )
            });
        }
        self.notifier.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_target_arguments_and_child_only_desktop_rule() {
        assert_eq!(SettingsTarget::Generic.arguments(), ["online-accounts"]);
        assert_eq!(
            SettingsTarget::Provider("ms_graph".into()).arguments(),
            ["online-accounts", "add", "ms_graph"]
        );
        assert_eq!(
            SettingsTarget::Account("fixture".into()).arguments(),
            ["online-accounts", "fixture"]
        );
        assert!(desktop_override(true, Some("niri")));
        assert!(!desktop_override(true, Some("niri:GNOME")));
        assert!(!desktop_override(false, Some("niri")));
    }
    fn script(directory: &tempfile::TempDir, contents: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.path().join("settings-fixture");
        std::fs::write(&path, contents).expect("fixture script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
            .expect("fixture executable");
        path.to_str().expect("fixture path").into()
    }
    async fn reaped(lifecycle: &GoaLifecycle) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !lifecycle.state.lock_or_recover().launch_targets.is_empty() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child reaping deadline");
    }
    #[tokio::test]
    async fn immediate_failure_and_accepted_dispatch_are_distinct() {
        let (notifier, _receiver) = waft_plugin::EntityNotifier::new_pair();
        let lifecycle = GoaLifecycle::new(notifier);
        assert!(
            lifecycle
                .launch_program(SettingsTarget::Generic, "/bin/false")
                .await
                .is_err()
        );
        assert_eq!(
            lifecycle.state.lock_or_recover().status.launch,
            AccountSettingsLaunch::Failed
        );
        let accepted = lifecycle
            .launch_program(SettingsTarget::Generic, "/bin/true")
            .await
            .expect("dispatch");
        assert_eq!(accepted["outcome"], "launch-accepted");
        reaped(&lifecycle).await;
    }
    #[tokio::test]
    async fn cancelled_request_keeps_child_supervised_and_older_failure_cannot_overwrite_latest() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let program = script(&dir, "#!/bin/sh\nsleep 0.5\nexit 1\n");
        let (notifier, _receiver) = waft_plugin::EntityNotifier::new_pair();
        let lifecycle = GoaLifecycle::new(notifier);
        let parent_desktop = std::env::var_os("XDG_CURRENT_DESKTOP");
        let first = lifecycle
            .launch_program(SettingsTarget::Generic, &program)
            .await
            .expect("initial dispatch");
        assert!(
            lifecycle
                .launch_program(SettingsTarget::Generic, &program)
                .await
                .is_err(),
            "duplicate active target"
        );
        let latest = lifecycle
            .launch_program(SettingsTarget::Account("fixture".into()), "/bin/true")
            .await
            .expect("newer dispatch");
        assert_ne!(first["launch_request_id"], latest["launch_request_id"]);
        reaped(&lifecycle).await;
        assert_eq!(
            lifecycle.state.lock_or_recover().status.launch,
            AccountSettingsLaunch::Accepted
        );
        let cloned = lifecycle.clone();
        let task = tokio::spawn(async move {
            cloned
                .launch_program(SettingsTarget::Generic, &program)
                .await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while lifecycle.state.lock_or_recover().launch_targets.is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("child spawned");
        task.abort();
        task.await.expect_err("cancel requester");
        reaped(&lifecycle).await;
        assert_eq!(
            lifecycle.state.lock_or_recover().status.launch,
            AccountSettingsLaunch::Failed
        );
        assert_eq!(
            std::env::var_os("XDG_CURRENT_DESKTOP"),
            parent_desktop,
            "parent environment unchanged"
        );
    }
    #[tokio::test]
    async fn hundred_children_are_reaped_and_duplicate_slots_return_to_baseline() {
        tokio::time::timeout(Duration::from_secs(60), async {
            let (notifier, _receiver) = waft_plugin::EntityNotifier::new_pair();
            let lifecycle = GoaLifecycle::new(notifier);
            for _ in 0..100 {
                assert_eq!(
                    lifecycle
                        .launch_program(SettingsTarget::Generic, "/bin/true")
                        .await
                        .expect("launch")["outcome"],
                    "launch-accepted"
                );
                reaped(&lifecycle).await;
                assert!(lifecycle.state.lock_or_recover().launch_targets.is_empty());
            }
        })
        .await
        .expect("child churn deadline");
    }
    #[tokio::test]
    async fn missing_launcher_is_not_success() {
        let (notifier, _receiver) = waft_plugin::EntityNotifier::new_pair();
        let lifecycle = GoaLifecycle::new(notifier);
        assert!(
            lifecycle
                .launch_program(
                    SettingsTarget::Generic,
                    "/nonexistent/waft-fixture-launcher"
                )
                .await
                .is_err()
        );
        let st = lifecycle.state.lock_or_recover();
        assert_eq!(st.status.launch, AccountSettingsLaunch::Failed);
        assert!(st.launch_targets.is_empty());
    }
}
