mod action;
mod app;
mod event;
mod screen;
mod tui;
mod widget;
mod wn;

use anyhow::Result;
use tokio::process::Child;
use tokio::sync::mpsc;

use action::{Action, Effect};
use app::App;
use event::{map_event, Event, EventLoop};

/// Tracks long-lived child processes for streaming commands.
struct StreamHandles {
    chats: Option<Child>,
    messages: Option<Child>,
    notifications: Option<Child>,
    search: Option<Child>,
}

impl StreamHandles {
    fn new() -> Self {
        Self {
            chats: None,
            messages: None,
            notifications: None,
            search: None,
        }
    }

    fn kill_messages(&mut self) {
        if let Some(mut child) = self.messages.take() {
            tokio::spawn(async move {
                let _ = child.kill().await;
            });
        }
    }

    fn kill_chats(&mut self) {
        if let Some(mut child) = self.chats.take() {
            tokio::spawn(async move {
                let _ = child.kill().await;
            });
        }
    }

    fn kill_notifications(&mut self) {
        if let Some(mut child) = self.notifications.take() {
            tokio::spawn(async move {
                let _ = child.kill().await;
            });
        }
    }

    fn kill_search(&mut self) {
        if let Some(mut child) = self.search.take() {
            tokio::spawn(async move {
                let _ = child.kill().await;
            });
        }
    }

    fn kill_all(&mut self) {
        self.kill_chats();
        self.kill_messages();
        self.kill_notifications();
        self.kill_search();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Auto-start daemon if needed (before terminal setup so errors print cleanly)
    let mut daemon_child = None;
    if !wn::is_daemon_running().await {
        match wn::start_daemon().await {
            Ok(child) => {
                daemon_child = Some(child);
            }
            Err(e) => {
                eprintln!("Warning: Could not start daemon: {e}");
                eprintln!("Some features may not work. Start wnd manually if needed.");
            }
        }
    }

    tui::install_panic_hook();
    let mut terminal = tui::init()?;
    let mut app = App::new();
    let mut events = EventLoop::new(250);
    let action_tx = events.sender();
    let mut streams = StreamHandles::new();

    // Run startup effects
    for effect in app.startup_effects() {
        execute_effect(effect, &action_tx, &mut streams);
    }

    // Initial draw
    terminal.draw(|frame| app.draw(frame))?;

    // Main loop: recv event → handle → update state → execute effects → draw
    while app.running {
        let event = events.next().await?;
        if let Some(action) = map_event(&event) {
            let effects = app.update(action);
            for effect in effects {
                execute_effect(effect, &action_tx, &mut streams);
            }
        }
        terminal.draw(|frame| app.draw(frame))?;
    }

    // Clean up streaming processes
    streams.kill_all();

    // Kill daemon if we started it
    if let Some(mut child) = daemon_child {
        let _ = child.kill().await;
    }

    tui::restore()?;
    Ok(())
}

/// Send a log entry through the event channel.
fn send_log(tx: &mpsc::UnboundedSender<Event>, msg: impl Into<String>) {
    let now = chrono::Local::now().format("%H:%M:%S").to_string();
    let _ = tx.send(Event::Action(Action::Log(format!(
        "[{}] {}",
        now,
        msg.into()
    ))));
}

/// Spawn async tasks for side effects, sending results back through the event channel.
fn execute_effect(effect: Effect, tx: &mpsc::UnboundedSender<Event>, streams: &mut StreamHandles) {
    // Log the effect being executed
    let effect_name = match &effect {
        Effect::CheckAccounts => "CheckAccounts",
        Effect::CreateIdentity => "CreateIdentity",
        Effect::LoginWithNsec(_) => "LoginWithNsec",
        Effect::SubscribeNotifications => "SubscribeNotifications",
        Effect::SubscribeChats { .. } => "SubscribeChats",
        Effect::SubscribeMessages { .. } => "SubscribeMessages",
        Effect::UnsubscribeMessages => "UnsubscribeMessages",
        Effect::SendMessage { .. } => "SendMessage",
        Effect::LoadGroupDetail { .. } => "LoadGroupDetail",
        Effect::LoadGroupMembers { .. } => "LoadGroupMembers",
        Effect::LoadInvites { .. } => "LoadInvites",
        Effect::CreateGroup { .. } => "CreateGroup",
        Effect::AddMember { .. } => "AddMember",
        Effect::RemoveMember { .. } => "RemoveMember",
        Effect::RenameGroup { .. } => "RenameGroup",
        Effect::LeaveGroup { .. } => "LeaveGroup",
        Effect::AcceptInvite { .. } => "AcceptInvite",
        Effect::DeclineInvite { .. } => "DeclineInvite",
        Effect::LoadProfile { .. } => "LoadProfile",
        Effect::UpdateProfile { .. } => "UpdateProfile",
        Effect::LoadSettings { .. } => "LoadSettings",
        Effect::UpdateSetting { .. } => "UpdateSetting",
        Effect::SearchUsers { .. } => "SearchUsers",
        Effect::UnsubscribeSearch => "UnsubscribeSearch",
        Effect::TailDaemonLog => "TailDaemonLog",
    };
    send_log(tx, format!("Effect: {effect_name}"));

    match effect {
        Effect::CheckAccounts => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec(&["whoami"]).await {
                    Ok(val) => {
                        let mut accounts = match val {
                            serde_json::Value::Array(arr) => arr,
                            serde_json::Value::Null => vec![],
                            other => vec![other],
                        };
                        // Enrich accounts with profile names for display
                        for account in &mut accounts {
                            let pubkey = account
                                .get("pubkey")
                                .or_else(|| account.get("npub"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            if let Some(pk) = pubkey {
                                if let Ok(profile) =
                                    wn::exec(&["--account", &pk, "profile", "show"]).await
                                {
                                    if let Some(name) = profile
                                        .get("name")
                                        .or_else(|| profile.get("display_name"))
                                        .and_then(|v| v.as_str())
                                    {
                                        account["display_name"] =
                                            serde_json::Value::String(name.to_string());
                                    }
                                }
                            }
                        }
                        Action::AccountsLoaded(accounts)
                    }
                    Err(_) => Action::AccountsLoaded(vec![]),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::CreateIdentity => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec(&["create-identity"]).await {
                    Ok(val) => extract_npub_login(val),
                    Err(e) => Action::LoginError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::LoginWithNsec(nsec) => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec_with_stdin(&["login"], &nsec).await {
                    Ok(val) => extract_npub_login(val),
                    Err(e) => Action::LoginError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::SubscribeNotifications => {
            streams.kill_notifications();
            let tx = tx.clone();
            tokio::spawn(async move {
                match wn::stream(&["notifications", "subscribe"]).await {
                    Ok((child, mut rx)) => {
                        drop(child);
                        while let Some(val) = rx.recv().await {
                            let notif = val
                                .get("result")
                                .and_then(|r| r.get("item"))
                                .or_else(|| val.get("result"))
                                .cloned();
                            if let Some(notif) = notif {
                                if tx
                                    .send(Event::Action(Action::NotificationUpdate(notif)))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                        let _ = tx.send(Event::Action(Action::NotificationStreamEnded));
                    }
                    Err(_) => {
                        // Notifications are non-critical — silently fail
                    }
                }
            });
        }

        Effect::SubscribeChats { account } => {
            streams.kill_chats();
            let tx = tx.clone();
            tokio::spawn(async move {
                match wn::stream(&["--account", &account, "chats", "subscribe"]).await {
                    Ok((child, mut rx)) => {
                        send_log(&tx, "Chats stream connected");
                        drop(child);
                        while let Some(val) = rx.recv().await {
                            // Stream sends {"result": {"item": {...}, "trigger": "..."}}
                            // Extract the inner item (the actual chat object)
                            let chat = val
                                .get("result")
                                .and_then(|r| r.get("item"))
                                .or_else(|| val.get("result"))
                                .cloned();
                            if let Some(chat) = chat {
                                if tx.send(Event::Action(Action::ChatUpdate(chat))).is_err() {
                                    break;
                                }
                            }
                        }
                        send_log(&tx, "Chats stream disconnected");
                        let _ = tx.send(Event::Action(Action::ChatStreamEnded));
                    }
                    Err(e) => {
                        send_log(&tx, format!("Chats stream error: {e}"));
                        let _ = tx.send(Event::Action(Action::LoginError(format!(
                            "Chat subscribe failed: {e}"
                        ))));
                    }
                }
            });
        }

        Effect::SubscribeMessages { account, group_id } => {
            streams.kill_messages();
            let tx = tx.clone();
            tokio::spawn(async move {
                match wn::stream(&["--account", &account, "messages", "subscribe", &group_id]).await
                {
                    Ok((child, mut rx)) => {
                        send_log(
                            &tx,
                            format!("Messages stream connected (group: {group_id})"),
                        );
                        drop(child);
                        while let Some(val) = rx.recv().await {
                            // Messages come as {"result": {"message": {...}, "trigger": "..."}}
                            let msg = val.get("result").and_then(|r| r.get("message")).cloned();
                            if let Some(msg) = msg {
                                if tx.send(Event::Action(Action::MessageUpdate(msg))).is_err() {
                                    break;
                                }
                            }
                        }
                        send_log(&tx, "Messages stream disconnected");
                        let _ = tx.send(Event::Action(Action::MessageStreamEnded));
                    }
                    Err(e) => {
                        send_log(&tx, format!("Messages stream error: {e}"));
                        let _ = tx.send(Event::Action(Action::MessageSendError(format!(
                            "Message subscribe failed: {e}"
                        ))));
                    }
                }
            });
        }

        Effect::UnsubscribeMessages => {
            streams.kill_messages();
        }

        Effect::SendMessage {
            account,
            group_id,
            text,
        } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action =
                    match wn::exec(&["--account", &account, "messages", "send", &group_id, &text])
                        .await
                    {
                        Ok(_) => Action::MessageSent,
                        Err(e) => Action::MessageSendError(e.to_string()),
                    };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::LoadGroupDetail { account, group_id } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action =
                    match wn::exec(&["--account", &account, "groups", "show", &group_id]).await {
                        Ok(val) => Action::GroupDetailLoaded(val),
                        Err(e) => Action::GroupActionError(e.to_string()),
                    };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::LoadGroupMembers { account, group_id } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let members_result =
                    wn::exec(&["--account", &account, "groups", "members", &group_id]).await;
                let admins_result =
                    wn::exec(&["--account", &account, "groups", "admins", &group_id]).await;

                let members = match members_result {
                    Ok(serde_json::Value::Array(arr)) => arr,
                    Ok(val) => vec![val],
                    Err(_) => vec![],
                };
                let admins = match admins_result {
                    Ok(serde_json::Value::Array(arr)) => arr,
                    Ok(val) => vec![val],
                    Err(_) => vec![],
                };

                let _ = tx.send(Event::Action(Action::GroupMembersLoaded {
                    members,
                    admins,
                }));
            });
        }

        Effect::LoadInvites { account } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec(&["--account", &account, "groups", "invites"]).await {
                    Ok(serde_json::Value::Array(arr)) => Action::InvitesLoaded(arr),
                    Ok(val) => Action::InvitesLoaded(vec![val]),
                    Err(e) => Action::GroupActionError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::CreateGroup { account, name } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action =
                    match wn::exec(&["--account", &account, "groups", "create", &name]).await {
                        Ok(_) => Action::GroupActionSuccess("Group created".into()),
                        Err(e) => Action::GroupActionError(e.to_string()),
                    };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::AddMember {
            account,
            group_id,
            npub,
        } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec(&[
                    "--account",
                    &account,
                    "groups",
                    "add-members",
                    &group_id,
                    &npub,
                ])
                .await
                {
                    Ok(_) => Action::GroupActionSuccess("Member added".into()),
                    Err(e) => Action::GroupActionError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::RemoveMember {
            account,
            group_id,
            npub,
        } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec(&[
                    "--account",
                    &account,
                    "groups",
                    "remove-members",
                    &group_id,
                    &npub,
                ])
                .await
                {
                    Ok(_) => Action::GroupActionSuccess("Member removed".into()),
                    Err(e) => Action::GroupActionError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::RenameGroup {
            account,
            group_id,
            name,
        } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action =
                    match wn::exec(&["--account", &account, "groups", "rename", &group_id, &name])
                        .await
                    {
                        Ok(_) => Action::GroupActionSuccess("Group renamed".into()),
                        Err(e) => Action::GroupActionError(e.to_string()),
                    };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::LeaveGroup { account, group_id } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action =
                    match wn::exec(&["--account", &account, "groups", "leave", &group_id]).await {
                        Ok(_) => Action::GroupActionSuccess("Left group".into()),
                        Err(e) => Action::GroupActionError(e.to_string()),
                    };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::AcceptInvite { account, group_id } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action =
                    match wn::exec(&["--account", &account, "groups", "accept", &group_id]).await {
                        Ok(_) => Action::GroupActionSuccess("Invite accepted".into()),
                        Err(e) => Action::GroupActionError(e.to_string()),
                    };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::DeclineInvite { account, group_id } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec(&[
                    "--account",
                    &account,
                    "groups",
                    "decline",
                    &group_id,
                ])
                .await
                {
                    Ok(_) => Action::GroupActionSuccess("Invite declined".into()),
                    Err(e) => Action::GroupActionError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::LoadProfile { account } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec(&["--account", &account, "profile", "show"]).await {
                    Ok(val) => Action::ProfileLoaded(val),
                    Err(e) => Action::ProfileUpdateError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::UpdateProfile {
            account,
            name,
            about,
        } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut args = vec!["--account", &account, "profile", "update"];
                let name_val;
                let about_val;
                if let Some(ref n) = name {
                    name_val = n.clone();
                    args.push("--name");
                    args.push(&name_val);
                }
                if let Some(ref a) = about {
                    about_val = a.clone();
                    args.push("--about");
                    args.push(&about_val);
                }
                let action = match wn::exec(&args).await {
                    Ok(_) => Action::ProfileUpdateSuccess("Profile updated".into()),
                    Err(e) => Action::ProfileUpdateError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::LoadSettings { account } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action = match wn::exec(&["--account", &account, "settings", "show"]).await {
                    Ok(val) => Action::SettingsLoaded(val),
                    Err(e) => Action::SettingsUpdateError(e.to_string()),
                };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::UpdateSetting {
            account,
            key,
            value,
        } => {
            let tx = tx.clone();
            tokio::spawn(async move {
                let action =
                    match wn::exec(&["--account", &account, "settings", &key, &value]).await {
                        Ok(_) => Action::SettingsUpdateSuccess(format!("{key} updated")),
                        Err(e) => Action::SettingsUpdateError(e.to_string()),
                    };
                let _ = tx.send(Event::Action(action));
            });
        }

        Effect::SearchUsers { account, query } => {
            streams.kill_search();
            let tx = tx.clone();
            tokio::spawn(async move {
                match wn::stream(&["--account", &account, "users", "search", &query]).await {
                    Ok((child, mut rx)) => {
                        drop(child);
                        while let Some(val) = rx.recv().await {
                            // Search results come as:
                            // {"result": {"new_results": [...users...], "trigger": "..."}}
                            let result = val.get("result").cloned().unwrap_or(val.clone());
                            if let Some(users) =
                                result.get("new_results").and_then(|v| v.as_array())
                            {
                                for user in users {
                                    if tx
                                        .send(Event::Action(Action::SearchResult(user.clone())))
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                        let _ = tx.send(Event::Action(Action::SearchStreamEnded));
                    }
                    Err(e) => {
                        let _ = tx.send(Event::Action(Action::SettingsUpdateError(format!(
                            "Search failed: {e}"
                        ))));
                    }
                }
            });
        }

        Effect::UnsubscribeSearch => {
            streams.kill_search();
        }

        Effect::TailDaemonLog => {
            let tx = tx.clone();
            tokio::spawn(async move {
                if let Err(e) = tail_daemon_log(tx).await {
                    // Non-fatal — just log it
                    eprintln!("Daemon log tail failed: {e}");
                }
            });
        }
    }
}

fn extract_npub_login(val: serde_json::Value) -> Action {
    // Try npub first, then pubkey (create-identity returns pubkey)
    let id = val
        .get("npub")
        .or_else(|| val.get("pubkey"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if id.is_empty() {
        Action::LoginError("No account identifier in response".into())
    } else {
        Action::LoginSuccess(id)
    }
}

/// Find the daemon log directory based on platform and build mode.
fn daemon_logs_dir() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    // Check both release and dev dirs — daemon build mode may differ from TUI build mode
    #[cfg(target_os = "macos")]
    let base = home.join("Library").join("Logs").join("whitenoise-cli");
    #[cfg(not(target_os = "macos"))]
    let base = dirs::data_dir()?.join("whitenoise-cli").join("logs");
    Some(base)
}

/// Find the most recent daemon log file.
/// tracing_appender uses UTC dates for rotation, so we check UTC first,
/// then fall back to finding the newest whitenoise.*.log file.
fn daemon_log_path() -> Option<std::path::PathBuf> {
    let base = daemon_logs_dir()?;

    // Try both release and dev directories
    for suffix in &["release", "dev"] {
        let logs_dir = base.join(suffix);
        if !logs_dir.is_dir() {
            continue;
        }

        // Try UTC date first (tracing_appender uses UTC for rotation)
        let utc_today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let utc_path = logs_dir.join(format!("whitenoise.{utc_today}.log"));
        if utc_path.exists() {
            return Some(utc_path);
        }

        // Fall back to most recent log file in the directory
        if let Ok(entries) = std::fs::read_dir(&logs_dir) {
            let mut best: Option<std::path::PathBuf> = None;
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("whitenoise.") && name_str.ends_with(".log") {
                    let path = entry.path();
                    if best.as_ref().is_none_or(|b| path > *b) {
                        best = Some(path);
                    }
                }
            }
            if best.is_some() {
                return best;
            }
        }
    }
    None
}

/// Tail the daemon log file, sending new lines as DaemonLog actions.
async fn tail_daemon_log(tx: mpsc::UnboundedSender<Event>) -> Result<()> {
    use tokio::io::AsyncBufReadExt;

    let path = daemon_log_path().ok_or_else(|| anyhow::anyhow!("Cannot determine log path"))?;

    send_log(&tx, format!("Tailing daemon log: {}", path.display()));

    // Wait for the file to exist (daemon may not have written yet)
    let mut attempts = 0;
    while !path.exists() {
        attempts += 1;
        if attempts > 30 {
            send_log(
                &tx,
                "Daemon log file not found after 30s, giving up".to_string(),
            );
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let file = tokio::fs::File::open(&path).await?;
    let metadata = file.metadata().await?;
    let file_len = metadata.len();

    // Seek to near the end to show recent lines (last ~8KB)
    let seek_pos = file_len.saturating_sub(8192);
    let file = tokio::fs::File::open(&path).await?;
    let mut reader = tokio::io::BufReader::new(file);

    if seek_pos > 0 {
        use tokio::io::AsyncSeekExt;
        reader.seek(std::io::SeekFrom::Start(seek_pos)).await?;
        // Skip partial first line
        let mut discard = String::new();
        let _ = reader.read_line(&mut discard).await;
    }

    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                // EOF — wait a bit and retry (tail -f behavior)
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                // Check if file rotated (date changed)
                if let Some(new_path) = daemon_log_path() {
                    if new_path != path {
                        // Date rotated, restart with new file
                        send_log(&tx, "Daemon log rotated, restarting tail".to_string());
                        return Box::pin(tail_daemon_log(tx)).await;
                    }
                }
            }
            Ok(_) => {
                let trimmed = line.trim_end().to_string();
                if !trimmed.is_empty() {
                    let _ = tx.send(Event::Action(Action::DaemonLog(trimmed)));
                }
            }
            Err(e) => {
                send_log(&tx, format!("Daemon log read error: {e}"));
                break;
            }
        }
    }
    Ok(())
}
