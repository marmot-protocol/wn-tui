use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use serde_json::Value;

use crate::action::{Action, Effect};
use crate::screen::login::LoginMode;
use crate::screen::Screen;
use crate::widget::chat_list;
use crate::widget::input::Input;

/// Which panel has focus in the main screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    ChatList,
    Messages,
    Composer,
}

/// Active popup overlay.
#[derive(Debug, Clone)]
pub enum Popup {
    TextInput {
        title: String,
        input: Input,
        purpose: InputPurpose,
    },
    Confirm {
        title: String,
        message: String,
        purpose: ConfirmPurpose,
    },
    Invites {
        items: Vec<Value>,
        selected: usize,
    },
    Help {
        screen: Screen,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputPurpose {
    CreateGroup,
    AddMember,
    RenameGroup,
    EditProfileName,
    EditProfileAbout,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmPurpose {
    LeaveGroup,
    RemoveMember { npub: String },
}

/// Why the user search screen was opened.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchPurpose {
    Browse,
    AddMember { group_id: String },
}

/// Which log tab is active.
#[derive(Debug, Clone, PartialEq)]
pub enum LogTab {
    Activity,
    Daemon,
}

/// Root application state.
pub struct App {
    pub running: bool,
    pub screen: Screen,
    pub account: Option<String>,

    // Login
    pub login_mode: LoginMode,
    pub nsec_input: Input,
    pub status_message: Option<String>,

    // Main screen
    pub focus: Panel,
    pub chats: Vec<Value>,
    pub selected_chat: usize,
    pub active_group_id: Option<String>,
    pub messages: Vec<Value>,
    pub message_scroll: usize,
    pub composer: Input,

    // Notifications
    pub unread_counts: HashMap<String, usize>,

    // Connection state
    pub connected: bool,

    // Group detail
    pub viewing_group_id: Option<String>,
    pub group_detail: Option<Value>,
    pub group_members: Vec<Value>,
    pub group_admins: Vec<Value>,
    pub selected_member: usize,

    // Popup
    pub popup: Option<Popup>,

    // Profile
    pub profile: Option<Value>,

    // Settings
    pub settings_data: Option<Value>,
    #[allow(dead_code)]
    pub selected_setting: usize,

    // Follows
    pub follows: Vec<Value>,
    pub selected_follow: usize,

    // User search
    pub search_input: Input,
    pub search_results: Vec<Value>,
    pub selected_result: usize,
    pub search_purpose: SearchPurpose,

    // Log panel
    pub show_logs: bool,
    pub logs: Vec<String>,
    pub daemon_logs: Vec<String>,
    pub log_tab: LogTab,
    pub log_scroll: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            screen: Screen::Login,
            account: None,
            login_mode: LoginMode::Loading("Checking accounts...".into()),
            nsec_input: {
                let mut input = Input::new();
                input.set_masked(true);
                input
            },
            status_message: None,
            focus: Panel::ChatList,
            chats: Vec::new(),
            selected_chat: 0,
            active_group_id: None,
            messages: Vec::new(),
            message_scroll: 0,
            composer: Input::new(),
            unread_counts: HashMap::new(),
            connected: false,
            viewing_group_id: None,
            group_detail: None,
            group_members: Vec::new(),
            group_admins: Vec::new(),
            selected_member: 0,
            popup: None,
            profile: None,
            settings_data: None,
            selected_setting: 0,
            follows: Vec::new(),
            selected_follow: 0,
            search_input: Input::new(),
            search_results: Vec::new(),
            selected_result: 0,
            search_purpose: SearchPurpose::Browse,
            show_logs: false,
            logs: Vec::new(),
            daemon_logs: Vec::new(),
            log_tab: LogTab::Activity,
            log_scroll: 0,
        }
    }

    /// Initial effects to run on startup.
    pub fn startup_effects(&self) -> Vec<Effect> {
        vec![Effect::CheckAccounts]
    }

    /// Process an action and mutate state. Returns side effects for the main loop.
    pub fn update(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Quit => {
                self.running = false;
            }
            Action::Tick | Action::Render => {}
            Action::Key(key) => return self.handle_key(key),
            Action::Paste(text) => {
                self.handle_paste(&text);
            }

            // Login
            Action::AccountsLoaded(accounts) => {
                return self.handle_accounts_loaded(accounts);
            }
            Action::LoginSuccess(npub) => {
                return self.enter_main_screen(npub);
            }
            Action::LoginError(msg) => {
                self.login_mode = LoginMode::Menu;
                self.status_message = Some(format!("Error: {msg}"));
            }

            // Chat streaming
            Action::ChatUpdate(val) => {
                self.connected = true;
                self.handle_chat_update(val);
            }
            Action::ChatStreamEnded => {
                self.connected = false;
                // Auto-reconnect if we have an account
                if let Some(account) = &self.account {
                    return vec![Effect::SubscribeChats {
                        account: account.clone(),
                    }];
                }
            }

            // Message streaming
            Action::MessageUpdate { group_id, message } => {
                if self.active_group_id.as_deref() == Some(&group_id) {
                    self.handle_message_update(message);
                }
            }
            Action::MessageStreamEnded => {}

            // Send
            Action::MessageSent => {}
            Action::MessageSendError(msg) => {
                self.popup = Some(Popup::Error {
                    message: format!("Send failed: {msg}"),
                });
            }

            // Notifications
            Action::NotificationUpdate(val) => {
                self.handle_notification(val);
            }
            Action::NotificationStreamEnded => {}

            // Group management
            Action::GroupDetailLoaded(val) => {
                self.group_detail = Some(val);
            }
            Action::GroupMembersLoaded { members, admins } => {
                self.group_members = members;
                self.group_admins = admins;
                self.selected_member = 0;
            }
            Action::InvitesLoaded(invites) => {
                if invites.is_empty() {
                    self.status_message = Some("No pending invites".into());
                } else {
                    self.popup = Some(Popup::Invites {
                        items: invites,
                        selected: 0,
                    });
                }
            }
            Action::GroupActionSuccess(msg) => {
                self.status_message = Some(msg.clone());
                self.popup = None;
                // If we left a group, go back to main
                if msg.contains("Left group") {
                    self.screen = Screen::Main;
                    self.viewing_group_id = None;
                }
                // Reload group detail if still on that screen
                if self.screen == Screen::GroupDetail {
                    return self.reload_group_detail();
                }
                // Re-subscribe to chats to pick up changes
                if let Some(account) = &self.account {
                    return vec![Effect::SubscribeChats {
                        account: account.clone(),
                    }];
                }
            }
            Action::GroupActionError(msg) => {
                self.popup = Some(Popup::Error {
                    message: format!("Error: {msg}"),
                });
            }

            // Profile
            Action::ProfileLoaded(val) => {
                self.profile = Some(val);
            }
            Action::ProfileUpdateSuccess(msg) => {
                self.status_message = Some(msg);
                if let Some(account) = &self.account {
                    return vec![Effect::LoadProfile {
                        account: account.clone(),
                    }];
                }
            }
            Action::ProfileUpdateError(msg) => {
                self.popup = Some(Popup::Error {
                    message: format!("Error: {msg}"),
                });
            }

            // Settings
            Action::SettingsLoaded(val) => {
                self.settings_data = Some(val);
            }
            Action::SettingsUpdateSuccess(msg) => {
                self.status_message = Some(msg);
                if let Some(account) = &self.account {
                    return vec![Effect::LoadSettings {
                        account: account.clone(),
                    }];
                }
            }
            Action::SettingsUpdateError(msg) => {
                self.popup = Some(Popup::Error {
                    message: format!("Error: {msg}"),
                });
            }

            // Follows
            Action::FollowsLoaded(list) => {
                self.follows = list;
                if self.selected_follow >= self.follows.len() {
                    self.selected_follow = self.follows.len().saturating_sub(1);
                }
            }
            Action::FollowSuccess(_msg) => {
                // Reload follows list to stay in sync
                if let Some(account) = &self.account {
                    return vec![Effect::LoadFollows {
                        account: account.clone(),
                    }];
                }
            }
            Action::FollowError(msg) => {
                self.popup = Some(Popup::Error { message: msg });
            }

            // User search
            Action::SearchResult(val) => {
                self.search_results.push(val);
            }
            Action::SearchStreamEnded => {}

            // Logs
            Action::Log(msg) => {
                self.logs.push(msg);
                if self.logs.len() > 1000 {
                    self.logs.drain(..500);
                }
            }
            Action::DaemonLog(msg) => {
                self.daemon_logs.push(msg);
                if self.daemon_logs.len() > 2000 {
                    self.daemon_logs.drain(..1000);
                }
            }
        }
        vec![]
    }

    fn reload_group_detail(&self) -> Vec<Effect> {
        if let (Some(account), Some(group_id)) = (&self.account, &self.viewing_group_id) {
            vec![
                Effect::LoadGroupDetail {
                    account: account.clone(),
                    group_id: group_id.clone(),
                },
                Effect::LoadGroupMembers {
                    account: account.clone(),
                    group_id: group_id.clone(),
                },
            ]
        } else {
            vec![]
        }
    }

    fn handle_notification(&mut self, val: Value) {
        let group_id = val
            .get("mls_group_id")
            .or_else(|| val.get("group_id"))
            .and_then(|v| v.as_str());

        if let Some(gid) = group_id {
            if self.active_group_id.as_deref() == Some(gid) {
                return;
            }
            *self.unread_counts.entry(gid.to_string()).or_insert(0) += 1;
        }
    }

    fn enter_main_screen(&mut self, npub: String) -> Vec<Effect> {
        let account = npub.clone();
        self.account = Some(npub);
        self.screen = Screen::Main;
        self.status_message = None;
        self.login_mode = LoginMode::Menu;
        self.focus = Panel::ChatList;
        self.chats.clear();
        self.messages.clear();
        self.active_group_id = None;
        self.unread_counts.clear();
        self.connected = false;
        self.popup = None;
        let acct = account.clone();
        let acct2 = acct.clone();
        vec![
            Effect::SubscribeNotifications,
            Effect::SubscribeChats { account },
            Effect::LoadProfile { account: acct },
            Effect::LoadFollows { account: acct2 },
            Effect::TailDaemonLog,
        ]
    }

    /// Total unread messages across all chats.
    pub fn total_unread(&self) -> usize {
        self.unread_counts.values().sum()
    }

    /// Count of chats with pending invitations (pending_confirmation == true).
    pub fn pending_invites(&self) -> usize {
        self.chats
            .iter()
            .filter(|c| c.get("pending_confirmation").and_then(|v| v.as_bool()) == Some(true))
            .count()
    }

    /// Check if a pubkey is in the follows list.
    pub fn is_following(&self, pubkey: &str) -> bool {
        self.follows
            .iter()
            .any(|f| f.get("pubkey").and_then(|v| v.as_str()) == Some(pubkey))
    }

    fn handle_accounts_loaded(&mut self, accounts: Vec<Value>) -> Vec<Effect> {
        if accounts.len() == 1 {
            if let Some(id) = extract_account_id(&accounts[0]) {
                return self.enter_main_screen(id);
            }
        }
        if accounts.len() > 1 {
            self.login_mode = LoginMode::AccountSelect {
                accounts,
                selected: 0,
            };
            self.status_message = None;
            return vec![];
        }
        self.login_mode = LoginMode::Menu;
        self.status_message = None;
        vec![]
    }

    fn handle_chat_update(&mut self, val: Value) {
        if let Some(new_gid) = chat_list::group_id(&val) {
            if let Some(pos) = self
                .chats
                .iter()
                .position(|c| chat_list::group_id(c).as_ref() == Some(&new_gid))
            {
                self.chats[pos] = val;
                return;
            }
        }
        self.chats.push(val);
    }

    fn handle_paste(&mut self, text: &str) {
        let input = match self.screen {
            Screen::Login => Some(&mut self.nsec_input),
            Screen::UserSearch => Some(&mut self.search_input),
            Screen::Main if self.focus == Panel::Composer => Some(&mut self.composer),
            _ => None,
        };
        if let Some(input) = input {
            for ch in text.chars() {
                input.insert(ch);
            }
        }
    }

    fn handle_message_update(&mut self, val: Value) {
        // Deduplicate by message id
        if let Some(id) = val.get("id").and_then(|v| v.as_str()) {
            if self
                .messages
                .iter()
                .any(|m| m.get("id").and_then(|v| v.as_str()) == Some(id))
            {
                return;
            }
        }
        let was_at_bottom = self.message_scroll == 0;
        self.messages.push(val);
        if !was_at_bottom {
            self.message_scroll += 1;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        // Popup takes priority
        if self.popup.is_some() {
            return self.handle_popup_key(key);
        }

        // Global: `?` opens help overlay (except when typing)
        if key.code == KeyCode::Char('?')
            && !matches!(self.screen, Screen::UserSearch)
            && self.focus != Panel::Composer
        {
            self.popup = Some(Popup::Help {
                screen: self.screen.clone(),
            });
            return vec![];
        }

        // Global: backtick toggles log panel (except in text-input screens)
        if key.code == KeyCode::Char('`') && !matches!(self.screen, Screen::UserSearch) {
            self.show_logs = !self.show_logs;
            self.log_scroll = 0;
            return vec![];
        }

        // Tab switches log tabs when log panel is visible
        if key.code == KeyCode::Tab && self.show_logs && self.screen == Screen::Main {
            self.log_tab = match self.log_tab {
                LogTab::Activity => LogTab::Daemon,
                LogTab::Daemon => LogTab::Activity,
            };
            self.log_scroll = 0;
            return vec![];
        }

        match &self.screen {
            Screen::Login => self.handle_login_key(key),
            Screen::Main => self.handle_main_key(key),
            Screen::GroupDetail => self.handle_group_detail_key(key),
            Screen::Profile => self.handle_profile_key(key),
            Screen::Settings => self.handle_settings_key(key),
            Screen::UserSearch => self.handle_search_key(key),
        }
    }

    // ── Popup key handling ───────────────────────────────────────────

    fn handle_popup_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let popup = self.popup.as_mut().unwrap();
        match popup {
            Popup::TextInput { input, purpose, .. } => match key.code {
                KeyCode::Enter => {
                    if !input.is_empty() {
                        let value = input.value.clone();
                        let purpose = purpose.clone();
                        self.popup = None;
                        return self.submit_text_input(purpose, value);
                    }
                    vec![]
                }
                KeyCode::Esc => {
                    self.popup = None;
                    vec![]
                }
                KeyCode::Char(ch) => {
                    input.insert(ch);
                    vec![]
                }
                KeyCode::Backspace => {
                    input.backspace();
                    vec![]
                }
                KeyCode::Delete => {
                    input.delete();
                    vec![]
                }
                KeyCode::Left => {
                    input.move_left();
                    vec![]
                }
                KeyCode::Right => {
                    input.move_right();
                    vec![]
                }
                KeyCode::Home => {
                    input.home();
                    vec![]
                }
                KeyCode::End => {
                    input.end();
                    vec![]
                }
                _ => vec![],
            },
            Popup::Confirm { purpose, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let purpose = purpose.clone();
                    self.popup = None;
                    self.submit_confirm(purpose)
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.popup = None;
                    vec![]
                }
                _ => vec![],
            },
            Popup::Invites { items, selected } => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if !items.is_empty() {
                        *selected = (*selected + 1).min(items.len() - 1);
                    }
                    vec![]
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                    vec![]
                }
                KeyCode::Char('a') | KeyCode::Enter => {
                    let idx = *selected;
                    self.accept_invite(idx)
                }
                KeyCode::Char('d') => {
                    let idx = *selected;
                    self.decline_invite(idx)
                }
                KeyCode::Esc => {
                    self.popup = None;
                    vec![]
                }
                _ => vec![],
            },
            Popup::Help { .. } | Popup::Error { .. } => {
                // Any key dismisses help/error popups
                self.popup = None;
                vec![]
            }
        }
    }

    fn submit_text_input(&mut self, purpose: InputPurpose, value: String) -> Vec<Effect> {
        let account = match &self.account {
            Some(a) => a.clone(),
            None => return vec![],
        };

        match purpose {
            InputPurpose::CreateGroup => {
                vec![Effect::CreateGroup {
                    account,
                    name: value,
                }]
            }
            InputPurpose::AddMember => {
                let group_id = match &self.viewing_group_id {
                    Some(g) => g.clone(),
                    None => return vec![],
                };
                vec![Effect::AddMember {
                    account,
                    group_id,
                    npub: value,
                }]
            }
            InputPurpose::RenameGroup => {
                let group_id = match &self.viewing_group_id {
                    Some(g) => g.clone(),
                    None => return vec![],
                };
                vec![Effect::RenameGroup {
                    account,
                    group_id,
                    name: value,
                }]
            }
            InputPurpose::EditProfileName => {
                vec![Effect::UpdateProfile {
                    account,
                    name: Some(value),
                    about: None,
                }]
            }
            InputPurpose::EditProfileAbout => {
                vec![Effect::UpdateProfile {
                    account,
                    name: None,
                    about: Some(value),
                }]
            }
        }
    }

    fn submit_confirm(&mut self, purpose: ConfirmPurpose) -> Vec<Effect> {
        let account = match &self.account {
            Some(a) => a.clone(),
            None => return vec![],
        };
        let group_id = match &self.viewing_group_id {
            Some(g) => g.clone(),
            None => return vec![],
        };

        match purpose {
            ConfirmPurpose::LeaveGroup => {
                vec![Effect::LeaveGroup { account, group_id }]
            }
            ConfirmPurpose::RemoveMember { npub } => {
                vec![Effect::RemoveMember {
                    account,
                    group_id,
                    npub,
                }]
            }
        }
    }

    fn accept_invite(&mut self, idx: usize) -> Vec<Effect> {
        let account = match &self.account {
            Some(a) => a.clone(),
            None => return vec![],
        };
        let group_id = self.invite_group_id(idx);
        if let Some(gid) = group_id {
            self.popup = None;
            vec![Effect::AcceptInvite {
                account,
                group_id: gid,
            }]
        } else {
            vec![]
        }
    }

    fn decline_invite(&mut self, idx: usize) -> Vec<Effect> {
        let account = match &self.account {
            Some(a) => a.clone(),
            None => return vec![],
        };
        let group_id = self.invite_group_id(idx);
        if let Some(gid) = group_id {
            // Remove from popup list
            if let Some(Popup::Invites { items, selected }) = &mut self.popup {
                items.remove(idx);
                if items.is_empty() {
                    self.popup = None;
                } else {
                    *selected = (*selected).min(items.len().saturating_sub(1));
                }
            }
            vec![Effect::DeclineInvite {
                account,
                group_id: gid,
            }]
        } else {
            vec![]
        }
    }

    fn invite_group_id(&self, idx: usize) -> Option<String> {
        if let Some(Popup::Invites { items, .. }) = &self.popup {
            items.get(idx).and_then(|inv| {
                // Try nested group object first (actual CLI format)
                let group = inv.get("group").unwrap_or(inv);
                chat_list::group_id(group)
            })
        } else {
            None
        }
    }

    // ── Main screen key handling ─────────────────────────────────────

    fn handle_main_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match self.focus {
            Panel::ChatList => self.handle_chat_list_key(key),
            Panel::Messages => self.handle_messages_key(key),
            Panel::Composer => self.handle_composer_key(key),
        }
    }

    fn handle_chat_list_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.chats.is_empty() {
                    self.selected_chat = (self.selected_chat + 1).min(self.chats.len() - 1);
                }
                let effects = self.select_chat();
                self.focus = Panel::ChatList; // stay in chat list while browsing
                effects
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_chat = self.selected_chat.saturating_sub(1);
                let effects = self.select_chat();
                self.focus = Panel::ChatList;
                effects
            }
            KeyCode::Enter => self.select_chat(),
            KeyCode::Tab => {
                if self.active_group_id.is_some() {
                    self.focus = Panel::Messages;
                }
                vec![]
            }
            KeyCode::Char('g') => self.open_group_detail(),
            KeyCode::Char('n') => {
                self.popup = Some(Popup::TextInput {
                    title: "Create Group".into(),
                    input: Input::new(),
                    purpose: InputPurpose::CreateGroup,
                });
                vec![]
            }
            KeyCode::Char('I') => {
                let account = match &self.account {
                    Some(a) => a.clone(),
                    None => return vec![],
                };
                vec![Effect::LoadInvites { account }]
            }
            KeyCode::Char('p') => {
                let account = match &self.account {
                    Some(a) => a.clone(),
                    None => return vec![],
                };
                self.screen = Screen::Profile;
                self.profile = None;
                self.selected_follow = 0;
                let acct = account.clone();
                vec![
                    Effect::LoadProfile { account },
                    Effect::LoadFollows { account: acct },
                ]
            }
            KeyCode::Char('S') => {
                let account = match &self.account {
                    Some(a) => a.clone(),
                    None => return vec![],
                };
                self.screen = Screen::Settings;
                self.settings_data = None;
                vec![Effect::LoadSettings { account }]
            }
            KeyCode::Char('/') => {
                self.screen = Screen::UserSearch;
                self.search_input.clear();
                self.search_results.clear();
                self.selected_result = 0;
                self.search_purpose = SearchPurpose::Browse;
                vec![]
            }
            KeyCode::Char('q') => {
                self.running = false;
                vec![]
            }
            _ => vec![],
        }
    }

    fn open_group_detail(&mut self) -> Vec<Effect> {
        let chat = match self.chats.get(self.selected_chat) {
            Some(c) => c,
            None => return vec![],
        };
        let group_id = match chat_list::group_id(chat) {
            Some(gid) => gid,
            None => return vec![],
        };
        let account = match &self.account {
            Some(a) => a.clone(),
            None => return vec![],
        };

        self.screen = Screen::GroupDetail;
        self.viewing_group_id = Some(group_id.clone());
        self.group_detail = None;
        self.group_members.clear();
        self.group_admins.clear();
        self.selected_member = 0;
        self.status_message = None;

        vec![
            Effect::LoadGroupDetail {
                account: account.clone(),
                group_id: group_id.clone(),
            },
            Effect::LoadGroupMembers { account, group_id },
        ]
    }

    fn handle_messages_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Char('k') | KeyCode::Up => {
                let max = self.messages.len().saturating_sub(1);
                self.message_scroll = (self.message_scroll + 1).min(max);
                vec![]
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.message_scroll = self.message_scroll.saturating_sub(1);
                vec![]
            }
            KeyCode::Char('G') => {
                self.message_scroll = 0;
                vec![]
            }
            KeyCode::Char('i') | KeyCode::Enter => {
                self.focus = Panel::Composer;
                vec![]
            }
            KeyCode::Tab => {
                self.focus = Panel::ChatList;
                vec![]
            }
            KeyCode::Esc => {
                self.focus = Panel::ChatList;
                vec![]
            }
            _ => vec![],
        }
    }

    fn handle_composer_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Enter => {
                if !self.composer.is_empty() {
                    if let (Some(account), Some(group_id)) = (&self.account, &self.active_group_id)
                    {
                        let text = self.composer.value.clone();
                        let effect = Effect::SendMessage {
                            account: account.clone(),
                            group_id: group_id.clone(),
                            text,
                        };
                        self.composer.clear();
                        return vec![effect];
                    }
                }
                vec![]
            }
            KeyCode::Esc => {
                self.focus = Panel::Messages;
                vec![]
            }
            KeyCode::Char(ch) => {
                self.composer.insert(ch);
                vec![]
            }
            KeyCode::Backspace => {
                self.composer.backspace();
                vec![]
            }
            KeyCode::Delete => {
                self.composer.delete();
                vec![]
            }
            KeyCode::Left => {
                self.composer.move_left();
                vec![]
            }
            KeyCode::Right => {
                self.composer.move_right();
                vec![]
            }
            KeyCode::Home => {
                self.composer.home();
                vec![]
            }
            KeyCode::End => {
                self.composer.end();
                vec![]
            }
            _ => vec![],
        }
    }

    fn select_chat(&mut self) -> Vec<Effect> {
        let chat = match self.chats.get(self.selected_chat) {
            Some(c) => c,
            None => return vec![],
        };

        let group_id = match chat_list::group_id(chat) {
            Some(gid) => gid,
            None => return vec![],
        };

        let account = match &self.account {
            Some(a) => a.clone(),
            None => return vec![],
        };

        if self.active_group_id.as_deref() == Some(&group_id) {
            self.focus = Panel::Messages;
            return vec![];
        }

        self.active_group_id = Some(group_id.clone());
        self.messages.clear();
        self.message_scroll = 0;
        self.focus = Panel::Messages;
        self.unread_counts.remove(&group_id);

        vec![
            Effect::UnsubscribeMessages,
            Effect::SubscribeMessages { account, group_id },
        ]
    }

    // ── Group detail key handling ────────────────────────────────────

    fn handle_group_detail_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Main;
                self.viewing_group_id = None;
                self.group_detail = None;
                self.group_members.clear();
                self.group_admins.clear();
                vec![]
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.group_members.is_empty() {
                    self.selected_member =
                        (self.selected_member + 1).min(self.group_members.len() - 1);
                }
                vec![]
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_member = self.selected_member.saturating_sub(1);
                vec![]
            }
            KeyCode::Char('a') => {
                let group_id = match &self.viewing_group_id {
                    Some(gid) => gid.clone(),
                    None => return vec![],
                };
                self.screen = Screen::UserSearch;
                self.search_input.clear();
                self.search_results.clear();
                self.selected_result = 0;
                self.search_purpose = SearchPurpose::AddMember { group_id };
                vec![]
            }
            KeyCode::Char('A') => {
                self.popup = Some(Popup::TextInput {
                    title: "Add Member (paste pubkey/npub)".into(),
                    input: Input::new(),
                    purpose: InputPurpose::AddMember,
                });
                vec![]
            }
            KeyCode::Char('x') => self.confirm_remove_member(),
            KeyCode::Char('R') => {
                let current_name = self
                    .group_detail
                    .as_ref()
                    .and_then(|d| {
                        d.get("name")
                            .or_else(|| d.get("group_name"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("")
                    .to_string();
                let mut input = Input::new();
                // Pre-fill with current name
                for ch in current_name.chars() {
                    input.insert(ch);
                }
                self.popup = Some(Popup::TextInput {
                    title: "Rename Group".into(),
                    input,
                    purpose: InputPurpose::RenameGroup,
                });
                vec![]
            }
            KeyCode::Char('L') => {
                let name = self
                    .group_detail
                    .as_ref()
                    .and_then(|d| {
                        d.get("name")
                            .or_else(|| d.get("group_name"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("this group")
                    .to_string();
                self.popup = Some(Popup::Confirm {
                    title: "Leave Group".into(),
                    message: format!("Leave \"{name}\"? (y/n)"),
                    purpose: ConfirmPurpose::LeaveGroup,
                });
                vec![]
            }
            _ => vec![],
        }
    }

    fn confirm_remove_member(&mut self) -> Vec<Effect> {
        let member = match self.group_members.get(self.selected_member) {
            Some(m) => m,
            None => return vec![],
        };
        let npub = match crate::screen::group_detail::member_npub(member) {
            Some(n) => n.to_string(),
            None => return vec![],
        };
        let name = member
            .get("display_name")
            .or_else(|| member.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&npub)
            .to_string();

        self.popup = Some(Popup::Confirm {
            title: "Remove Member".into(),
            message: format!("Remove {name}? (y/n)"),
            purpose: ConfirmPurpose::RemoveMember { npub },
        });
        vec![]
    }

    // ── Profile key handling ─────────────────────────────────────────

    fn handle_profile_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Main;
                self.profile = None;
                vec![]
            }
            KeyCode::Char('n') => {
                if self.profile.is_some() {
                    let current = self
                        .profile
                        .as_ref()
                        .and_then(|p| p.get("name").or_else(|| p.get("display_name")))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let mut input = Input::new();
                    for ch in current.chars() {
                        input.insert(ch);
                    }
                    self.popup = Some(Popup::TextInput {
                        title: "Edit Name".into(),
                        input,
                        purpose: InputPurpose::EditProfileName,
                    });
                }
                vec![]
            }
            KeyCode::Char('a') => {
                if self.profile.is_some() {
                    let current = self
                        .profile
                        .as_ref()
                        .and_then(|p| p.get("about"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let mut input = Input::new();
                    for ch in current.chars() {
                        input.insert(ch);
                    }
                    self.popup = Some(Popup::TextInput {
                        title: "Edit About".into(),
                        input,
                        purpose: InputPurpose::EditProfileAbout,
                    });
                }
                vec![]
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.follows.is_empty() {
                    self.selected_follow = (self.selected_follow + 1).min(self.follows.len() - 1);
                }
                vec![]
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected_follow = self.selected_follow.saturating_sub(1);
                vec![]
            }
            KeyCode::Char('d') => {
                // Unfollow selected
                let account = match &self.account {
                    Some(a) => a.clone(),
                    None => return vec![],
                };
                let pubkey = self
                    .follows
                    .get(self.selected_follow)
                    .and_then(|f| f.get("pubkey").and_then(|v| v.as_str()))
                    .map(|s| s.to_string());
                match pubkey {
                    Some(pk) => vec![Effect::UnfollowUser {
                        account,
                        pubkey: pk,
                    }],
                    None => vec![],
                }
            }
            _ => vec![],
        }
    }

    // ── Settings key handling ─────────────────────────────────────────

    fn handle_settings_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                self.screen = Screen::Main;
                self.settings_data = None;
                vec![]
            }
            _ => vec![],
        }
    }

    // ── User search key handling ──────────────────────────────────────

    fn handle_search_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        match key.code {
            KeyCode::Esc => {
                let back_screen = match &self.search_purpose {
                    SearchPurpose::AddMember { .. } => Screen::GroupDetail,
                    SearchPurpose::Browse => Screen::Main,
                };
                self.screen = back_screen;
                self.search_input.clear();
                self.search_results.clear();
                self.selected_result = 0;
                vec![Effect::UnsubscribeSearch]
            }
            KeyCode::Enter => {
                // If a result is selected and we have a purpose, act on it
                if let Some(user) = self.search_results.get(self.selected_result).cloned() {
                    if let SearchPurpose::AddMember { ref group_id } = self.search_purpose {
                        let account = match &self.account {
                            Some(a) => a.clone(),
                            None => return vec![],
                        };
                        let npub = user
                            .get("pubkey")
                            .or_else(|| user.get("npub"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if npub.is_empty() {
                            return vec![];
                        }
                        let group_id = group_id.clone();
                        // Go back to group detail
                        self.screen = Screen::GroupDetail;
                        self.search_input.clear();
                        self.search_results.clear();
                        self.selected_result = 0;
                        return vec![
                            Effect::UnsubscribeSearch,
                            Effect::AddMember {
                                account: account.clone(),
                                group_id: group_id.clone(),
                                npub,
                            },
                            Effect::LoadGroupMembers { account, group_id },
                        ];
                    }
                }
                // Otherwise, treat Enter as "submit search query"
                if !self.search_input.is_empty() {
                    let account = match &self.account {
                        Some(a) => a.clone(),
                        None => return vec![],
                    };
                    let query = self.search_input.value.clone();
                    self.search_results.clear();
                    self.selected_result = 0;
                    vec![Effect::SearchUsers { account, query }]
                } else {
                    vec![]
                }
            }
            KeyCode::Down => {
                if !self.search_results.is_empty() {
                    self.selected_result =
                        (self.selected_result + 1).min(self.search_results.len() - 1);
                }
                vec![]
            }
            KeyCode::Up => {
                self.selected_result = self.selected_result.saturating_sub(1);
                vec![]
            }
            KeyCode::Tab => self.toggle_follow_selected(),
            KeyCode::Char(ch) => {
                self.search_input.insert(ch);
                vec![]
            }
            KeyCode::Backspace => {
                self.search_input.backspace();
                vec![]
            }
            KeyCode::Delete => {
                self.search_input.delete();
                vec![]
            }
            KeyCode::Left => {
                self.search_input.move_left();
                vec![]
            }
            KeyCode::Right => {
                self.search_input.move_right();
                vec![]
            }
            KeyCode::Home => {
                self.search_input.home();
                vec![]
            }
            KeyCode::End => {
                self.search_input.end();
                vec![]
            }
            _ => vec![],
        }
    }

    fn toggle_follow_selected(&mut self) -> Vec<Effect> {
        let account = match &self.account {
            Some(a) => a.clone(),
            None => return vec![],
        };
        let pubkey = self
            .search_results
            .get(self.selected_result)
            .and_then(|u| {
                u.get("pubkey")
                    .or_else(|| u.get("npub"))
                    .and_then(|v| v.as_str())
            })
            .map(|s| s.to_string());
        let Some(pubkey) = pubkey else {
            return vec![];
        };

        if self.is_following(&pubkey) {
            vec![Effect::UnfollowUser { account, pubkey }]
        } else {
            vec![Effect::FollowUser { account, pubkey }]
        }
    }

    // ── Login key handling ───────────────────────────────────────────

    fn handle_login_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        // Account select needs mutable access for j/k navigation
        if let LoginMode::AccountSelect { accounts, selected } = &mut self.login_mode {
            return match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if !accounts.is_empty() {
                        *selected = (*selected + 1).min(accounts.len() - 1);
                    }
                    vec![]
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                    vec![]
                }
                KeyCode::Enter => {
                    let idx = *selected;
                    if let Some(id) = accounts.get(idx).and_then(extract_account_id) {
                        self.enter_main_screen(id)
                    } else {
                        vec![]
                    }
                }
                KeyCode::Char('q') => {
                    self.running = false;
                    vec![]
                }
                _ => vec![],
            };
        }

        match &self.login_mode {
            LoginMode::Menu => match key.code {
                KeyCode::Char('c') => {
                    self.login_mode = LoginMode::Loading("Creating identity...".into());
                    self.status_message = None;
                    vec![Effect::CreateIdentity]
                }
                KeyCode::Char('l') => {
                    self.login_mode = LoginMode::NsecInput;
                    self.nsec_input.clear();
                    self.status_message = None;
                    vec![]
                }
                KeyCode::Char('q') => {
                    self.running = false;
                    vec![]
                }
                _ => vec![],
            },
            LoginMode::NsecInput => match key.code {
                KeyCode::Enter => {
                    if !self.nsec_input.is_empty() {
                        let nsec = self.nsec_input.value.clone();
                        self.login_mode = LoginMode::Loading("Logging in...".into());
                        vec![Effect::LoginWithNsec(nsec)]
                    } else {
                        vec![]
                    }
                }
                KeyCode::Esc => {
                    self.login_mode = LoginMode::Menu;
                    self.nsec_input.clear();
                    vec![]
                }
                KeyCode::Char(ch) => {
                    self.nsec_input.insert(ch);
                    vec![]
                }
                KeyCode::Backspace => {
                    self.nsec_input.backspace();
                    vec![]
                }
                KeyCode::Delete => {
                    self.nsec_input.delete();
                    vec![]
                }
                KeyCode::Left => {
                    self.nsec_input.move_left();
                    vec![]
                }
                KeyCode::Right => {
                    self.nsec_input.move_right();
                    vec![]
                }
                KeyCode::Home => {
                    self.nsec_input.home();
                    vec![]
                }
                KeyCode::End => {
                    self.nsec_input.end();
                    vec![]
                }
                _ => vec![],
            },
            LoginMode::Loading(_) | LoginMode::AccountSelect { .. } => vec![],
        }
    }

    // ── Drawing ──────────────────────────────────────────────────────

    pub fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        match &self.screen {
            Screen::Login => crate::screen::login::draw(self, frame, area),
            Screen::Main => crate::screen::main_screen::draw(self, frame, area),
            Screen::GroupDetail => crate::screen::group_detail::draw(self, frame, area),
            Screen::Profile => crate::screen::profile::draw(self, frame, area),
            Screen::Settings => crate::screen::settings::draw(self, frame, area),
            Screen::UserSearch => crate::screen::user_search::draw(self, frame, area),
        }

        // Popup overlay (renders on top of any screen)
        if let Some(popup) = &self.popup {
            self.draw_popup(popup, frame, area);
        }
    }

    fn draw_popup(&self, popup: &Popup, frame: &mut Frame, area: ratatui::layout::Rect) {
        use crate::widget::popup::PopupWidget;
        use ratatui::style::{Color, Style};
        use ratatui::text::{Line, Span};

        match popup {
            Popup::TextInput { title, input, .. } => {
                let widget = PopupWidget::new(title)
                    .input(input)
                    .hints(vec![("Enter", "Submit"), ("Esc", "Cancel")])
                    .size(50, 8);
                frame.render_widget(widget, area);
            }
            Popup::Confirm { title, message, .. } => {
                let widget = PopupWidget::new(title)
                    .body(vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            message.as_str(),
                            Style::default().fg(Color::Yellow),
                        )),
                    ])
                    .hints(vec![("y", "Yes"), ("n", "No")])
                    .size(50, 8);
                frame.render_widget(widget, area);
            }
            Popup::Help { screen } => {
                let body = help_lines(screen);
                let height = (body.len() as u16 + 4).min(22);
                let widget = PopupWidget::new("Help")
                    .body(body)
                    .hints(vec![("Any key", "Close")])
                    .size(55, height);
                frame.render_widget(widget, area);
            }
            Popup::Error { message } => {
                let widget = PopupWidget::new("Error")
                    .body(vec![
                        Line::raw(""),
                        Line::from(Span::styled(
                            message.as_str(),
                            Style::default().fg(Color::Red),
                        )),
                    ])
                    .hints(vec![("Esc", "Dismiss")])
                    .size(55, 8);
                frame.render_widget(widget, area);
            }
            Popup::Invites { items, selected } => {
                let body: Vec<Line> = items
                    .iter()
                    .enumerate()
                    .map(|(i, inv)| {
                        let group = inv.get("group").unwrap_or(inv);
                        let name = group
                            .get("name")
                            .or_else(|| group.get("group_name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown group");
                        let marker = if i == *selected { ">" } else { " " };
                        Line::from(vec![
                            Span::styled(format!("  {marker} "), Style::default().fg(Color::Cyan)),
                            Span::raw(name.to_string()),
                        ])
                    })
                    .collect();
                let height = (body.len() as u16 + 5).min(20);
                let widget = PopupWidget::new("Pending Invites")
                    .body(body)
                    .hints(vec![("a", "Accept"), ("d", "Decline"), ("Esc", "Close")])
                    .size(50, height);
                frame.render_widget(widget, area);
            }
        }
    }
}

/// Extract account identifier (pubkey or npub) from a JSON value.
fn extract_account_id(val: &Value) -> Option<String> {
    // Direct string
    if let Some(s) = val.as_str() {
        return Some(s.to_string());
    }
    // Object with npub or pubkey
    val.get("npub")
        .or_else(|| val.get("pubkey"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Convert a hex pubkey to bech32 npub format. Returns the input unchanged on failure.
pub fn hex_to_npub(hex: &str) -> String {
    if hex.starts_with("npub") {
        return hex.to_string();
    }
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect();
    if bytes.len() != 32 {
        return hex.to_string();
    }
    bech32::encode::<bech32::Bech32>(bech32::Hrp::parse("npub").unwrap(), &bytes)
        .unwrap_or_else(|_| hex.to_string())
}

/// Generate help text lines for the given screen.
fn help_lines(screen: &Screen) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Color, Style};
    use ratatui::text::{Line, Span};

    let hint = |key: &'static str, desc: &'static str| -> Line<'static> {
        Line::from(vec![
            Span::styled(format!("  {key:<12}"), Style::default().fg(Color::Cyan)),
            Span::raw(desc),
        ])
    };

    let mut lines = vec![Line::raw("")];

    match screen {
        Screen::Main => {
            lines.push(hint("j / k", "Navigate chats"));
            lines.push(hint("Enter", "Select chat"));
            lines.push(hint("Tab", "Switch focus"));
            lines.push(hint("i / Enter", "Start typing (messages)"));
            lines.push(hint("Esc", "Unfocus / back"));
            lines.push(hint("n", "New group"));
            lines.push(hint("g", "Group info"));
            lines.push(hint("I", "View invites"));
            lines.push(hint("p", "Profile"));
            lines.push(hint("S", "Settings"));
            lines.push(hint("/", "Search users"));
            lines.push(hint("`", "Toggle logs"));
            lines.push(hint("?", "This help"));
            lines.push(hint("q", "Quit"));
        }
        Screen::GroupDetail => {
            lines.push(hint("j / k", "Navigate members"));
            lines.push(hint("a", "Search & add member"));
            lines.push(hint("A", "Add by pubkey/npub"));
            lines.push(hint("x", "Remove member"));
            lines.push(hint("R", "Rename group"));
            lines.push(hint("L", "Leave group"));
            lines.push(hint("Esc", "Back"));
        }
        Screen::Profile => {
            lines.push(hint("n", "Edit name"));
            lines.push(hint("a", "Edit about"));
            lines.push(hint("Esc", "Back"));
        }
        Screen::Settings => {
            lines.push(hint("Esc", "Back"));
        }
        Screen::Login => {
            lines.push(hint("c", "Create identity"));
            lines.push(hint("l", "Login with nsec"));
            lines.push(hint("q", "Quit"));
        }
        Screen::UserSearch => {
            lines.push(hint("Enter", "Search"));
            lines.push(hint("j / k", "Navigate results"));
            lines.push(hint("Esc", "Back"));
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};
    use serde_json::json;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn app_on_main() -> App {
        let mut app = App::new();
        app.screen = Screen::Main;
        app.account = Some("npub1test".into());
        app.focus = Panel::ChatList;
        app
    }

    fn app_with_chats() -> App {
        let mut app = app_on_main();
        app.chats = vec![
            json!({"mls_group_id": "g1", "name": "Coffee Chat"}),
            json!({"mls_group_id": "g2", "name": "Work"}),
            json!({"mls_group_id": "g3", "name": "DM: Eve"}),
        ];
        app
    }

    fn app_on_group_detail() -> App {
        let mut app = app_on_main();
        app.screen = Screen::GroupDetail;
        app.viewing_group_id = Some("g1".into());
        app.group_detail = Some(json!({"name": "Coffee Chat", "description": "Daily standup"}));
        app.group_members = vec![
            json!({"npub": "npub1alice", "display_name": "Alice"}),
            json!({"npub": "npub1bob", "display_name": "Bob"}),
        ];
        app.group_admins = vec![json!("npub1alice")];
        app.selected_member = 0;
        app
    }

    // ── Login tests ──────────────────────────────────────────────────

    #[test]
    fn starts_in_loading_state() {
        let app = App::new();
        assert_eq!(app.screen, Screen::Login);
        assert!(matches!(app.login_mode, LoginMode::Loading(_)));
    }

    #[test]
    fn startup_checks_accounts() {
        let app = App::new();
        let effects = app.startup_effects();
        assert!(matches!(effects[0], Effect::CheckAccounts));
    }

    #[test]
    fn single_account_auto_logins_and_subscribes() {
        let mut app = App::new();
        let effects = app.update(Action::AccountsLoaded(vec![Value::String(
            "npub1abc".into(),
        )]));
        assert_eq!(app.screen, Screen::Main);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::SubscribeChats { ref account } if account == "npub1abc")));
    }

    #[test]
    fn single_account_with_pubkey_auto_logins() {
        let mut app = App::new();
        let effects = app.update(Action::AccountsLoaded(vec![json!({
            "pubkey": "d42ca434",
            "account_type": "Local"
        })]));
        assert_eq!(app.screen, Screen::Main);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::SubscribeChats { ref account } if account == "d42ca434")));
    }

    #[test]
    fn multiple_accounts_shows_selector() {
        let mut app = App::new();
        app.update(Action::AccountsLoaded(vec![
            json!({"pubkey": "aaa"}),
            json!({"pubkey": "bbb"}),
        ]));
        assert!(matches!(app.login_mode, LoginMode::AccountSelect { .. }));
        assert_eq!(app.screen, Screen::Login);
    }

    #[test]
    fn account_select_enter_picks_account() {
        let mut app = App::new();
        app.login_mode = LoginMode::AccountSelect {
            accounts: vec![json!({"pubkey": "aaa"}), json!({"pubkey": "bbb"})],
            selected: 1,
        };
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert_eq!(app.screen, Screen::Main);
        assert_eq!(app.account.as_deref(), Some("bbb"));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::SubscribeChats { ref account } if account == "bbb")));
    }

    #[test]
    fn account_select_j_k_navigates() {
        let mut app = App::new();
        app.login_mode = LoginMode::AccountSelect {
            accounts: vec![json!({"pubkey": "aaa"}), json!({"pubkey": "bbb"})],
            selected: 0,
        };
        app.update(Action::Key(key(KeyCode::Char('j'))));
        if let LoginMode::AccountSelect { selected, .. } = &app.login_mode {
            assert_eq!(*selected, 1);
        } else {
            panic!("Expected AccountSelect");
        }
    }

    #[test]
    fn login_success_enters_main_and_subscribes() {
        let mut app = App::new();
        let effects = app.update(Action::LoginSuccess("npub1xyz".into()));
        assert_eq!(app.screen, Screen::Main);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::SubscribeNotifications)));
    }

    #[test]
    fn login_error_shows_message() {
        let mut app = App::new();
        app.update(Action::LoginError("bad nsec".into()));
        assert!(app.status_message.as_ref().unwrap().contains("bad nsec"));
    }

    // ── Chat list navigation ─────────────────────────────────────────

    #[test]
    fn chat_list_j_moves_down_and_subscribes() {
        let mut app = app_with_chats();
        let effects = app.update(Action::Key(key(KeyCode::Char('j'))));
        assert_eq!(app.selected_chat, 1);
        assert_eq!(app.active_group_id.as_deref(), Some("g2"));
        assert!(effects.iter().any(
            |e| matches!(e, Effect::SubscribeMessages { ref group_id, .. } if group_id == "g2")
        ));
        assert_eq!(app.focus, Panel::ChatList, "focus stays in chat list");
    }

    #[test]
    fn chat_list_enter_selects_and_focuses_messages() {
        let mut app = app_with_chats();
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert_eq!(app.active_group_id.as_deref(), Some("g1"));
        assert!(effects.iter().any(
            |e| matches!(e, Effect::SubscribeMessages { ref group_id, .. } if group_id == "g1")
        ));
        assert_eq!(app.focus, Panel::Messages, "Enter moves focus to messages");
    }

    #[test]
    fn selecting_same_chat_only_focuses() {
        let mut app = app_with_chats();
        app.active_group_id = Some("g1".into());
        app.focus = Panel::ChatList;
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert_eq!(app.focus, Panel::Messages);
        assert!(effects.is_empty());
    }

    // ── Focus management ─────────────────────────────────────────────

    #[test]
    fn tab_cycles_focus() {
        let mut app = app_with_chats();
        app.active_group_id = Some("g1".into());
        app.update(Action::Key(key(KeyCode::Tab)));
        assert_eq!(app.focus, Panel::Messages);
        app.update(Action::Key(key(KeyCode::Tab)));
        assert_eq!(app.focus, Panel::ChatList);
    }

    #[test]
    fn esc_from_composer_goes_to_messages() {
        let mut app = app_with_chats();
        app.focus = Panel::Composer;
        app.update(Action::Key(key(KeyCode::Esc)));
        assert_eq!(app.focus, Panel::Messages);
    }

    // ── Composer ─────────────────────────────────────────────────────

    #[test]
    fn composer_enter_sends_message() {
        let mut app = app_on_main();
        app.focus = Panel::Composer;
        app.active_group_id = Some("g1".into());
        app.composer.insert('h');
        app.composer.insert('i');
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert!(matches!(
            effects[0],
            Effect::SendMessage { ref text, .. } if text == "hi"
        ));
    }

    // ── Chat/Message updates ─────────────────────────────────────────

    #[test]
    fn chat_update_adds_new_chat() {
        let mut app = app_on_main();
        app.update(Action::ChatUpdate(
            json!({"mls_group_id": "g1", "name": "New"}),
        ));
        assert_eq!(app.chats.len(), 1);
    }

    #[test]
    fn chat_update_replaces_existing() {
        let mut app = app_on_main();
        app.chats = vec![json!({"mls_group_id": "g1", "name": "Old"})];
        app.update(Action::ChatUpdate(
            json!({"mls_group_id": "g1", "name": "Updated"}),
        ));
        assert_eq!(app.chats.len(), 1);
        assert_eq!(app.chats[0]["name"], "Updated");
    }

    fn msg_update(message: Value) -> Action {
        Action::MessageUpdate {
            group_id: "g1".into(),
            message,
        }
    }

    #[test]
    fn message_update_auto_scrolls_at_bottom() {
        let mut app = app_on_main();
        app.active_group_id = Some("g1".into());
        app.update(msg_update(json!({"id": "1", "content": "msg"})));
        assert_eq!(app.message_scroll, 0);
    }

    #[test]
    fn message_update_deduplicates_by_id() {
        let mut app = app_on_main();
        app.active_group_id = Some("g1".into());
        app.update(msg_update(json!({"id": "msg1", "content": "hello"})));
        app.update(msg_update(json!({"id": "msg1", "content": "hello"})));
        app.update(msg_update(json!({"id": "msg2", "content": "world"})));
        assert_eq!(app.messages.len(), 2);
    }

    #[test]
    fn message_update_without_id_always_appends() {
        let mut app = app_on_main();
        app.active_group_id = Some("g1".into());
        app.update(msg_update(json!({"content": "no id"})));
        app.update(msg_update(json!({"content": "no id"})));
        assert_eq!(app.messages.len(), 2);
    }

    #[test]
    fn message_update_ignored_for_wrong_group() {
        let mut app = app_on_main();
        app.active_group_id = Some("g1".into());
        app.update(Action::MessageUpdate {
            group_id: "g2".into(),
            message: json!({"id": "1", "content": "wrong group"}),
        });
        assert_eq!(app.messages.len(), 0);
    }

    // ── Notifications ────────────────────────────────────────────────

    #[test]
    fn notification_increments_unread() {
        let mut app = app_on_main();
        app.update(Action::NotificationUpdate(json!({"mls_group_id": "g1"})));
        assert_eq!(app.unread_counts.get("g1"), Some(&1));
    }

    #[test]
    fn notification_for_active_chat_ignored() {
        let mut app = app_on_main();
        app.active_group_id = Some("g1".into());
        app.update(Action::NotificationUpdate(json!({"mls_group_id": "g1"})));
        assert_eq!(app.unread_counts.get("g1"), None);
    }

    #[test]
    fn total_unread_sums_all() {
        let mut app = app_on_main();
        app.unread_counts.insert("g1".into(), 3);
        app.unread_counts.insert("g2".into(), 7);
        assert_eq!(app.total_unread(), 10);
    }

    // ── Group detail navigation ──────────────────────────────────────

    #[test]
    fn g_opens_group_detail() {
        let mut app = app_with_chats();
        let effects = app.update(Action::Key(key(KeyCode::Char('g'))));
        assert_eq!(app.screen, Screen::GroupDetail);
        assert_eq!(app.viewing_group_id.as_deref(), Some("g1"));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadGroupDetail { .. })));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadGroupMembers { .. })));
    }

    #[test]
    fn g_with_no_chats_does_nothing() {
        let mut app = app_on_main();
        app.update(Action::Key(key(KeyCode::Char('g'))));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn esc_from_group_detail_returns_to_main() {
        let mut app = app_on_group_detail();
        app.update(Action::Key(key(KeyCode::Esc)));
        assert_eq!(app.screen, Screen::Main);
        assert!(app.viewing_group_id.is_none());
    }

    #[test]
    fn group_detail_loads_data() {
        let mut app = app_on_main();
        app.screen = Screen::GroupDetail;
        app.update(Action::GroupDetailLoaded(json!({"name": "Test"})));
        assert!(app.group_detail.is_some());
    }

    #[test]
    fn group_members_loaded() {
        let mut app = app_on_main();
        app.update(Action::GroupMembersLoaded {
            members: vec![json!({"npub": "a"}), json!({"npub": "b"})],
            admins: vec![json!("a")],
        });
        assert_eq!(app.group_members.len(), 2);
        assert_eq!(app.group_admins.len(), 1);
    }

    #[test]
    fn group_detail_j_k_navigates_members() {
        let mut app = app_on_group_detail();
        assert_eq!(app.selected_member, 0);
        app.update(Action::Key(key(KeyCode::Char('j'))));
        assert_eq!(app.selected_member, 1);
        app.update(Action::Key(key(KeyCode::Char('k'))));
        assert_eq!(app.selected_member, 0);
    }

    // ── Popups ───────────────────────────────────────────────────────

    #[test]
    fn n_opens_create_group_popup() {
        let mut app = app_with_chats();
        app.update(Action::Key(key(KeyCode::Char('n'))));
        assert!(matches!(
            app.popup,
            Some(Popup::TextInput {
                purpose: InputPurpose::CreateGroup,
                ..
            })
        ));
    }

    #[test]
    fn popup_esc_closes() {
        let mut app = app_with_chats();
        app.popup = Some(Popup::TextInput {
            title: "Test".into(),
            input: Input::new(),
            purpose: InputPurpose::CreateGroup,
        });
        app.update(Action::Key(key(KeyCode::Esc)));
        assert!(app.popup.is_none());
    }

    #[test]
    fn popup_enter_submits_create_group() {
        let mut app = app_with_chats();
        let mut input = Input::new();
        input.insert('T');
        input.insert('e');
        input.insert('s');
        input.insert('t');
        app.popup = Some(Popup::TextInput {
            title: "Create Group".into(),
            input,
            purpose: InputPurpose::CreateGroup,
        });
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert!(app.popup.is_none());
        assert!(matches!(
            effects[0],
            Effect::CreateGroup { ref name, .. } if name == "Test"
        ));
    }

    #[test]
    fn a_opens_user_search_for_add_member() {
        let mut app = app_on_group_detail();
        app.update(Action::Key(key(KeyCode::Char('a'))));
        assert_eq!(app.screen, Screen::UserSearch);
        assert!(matches!(
            app.search_purpose,
            SearchPurpose::AddMember { .. }
        ));
    }

    #[test]
    fn x_confirms_remove_member() {
        let mut app = app_on_group_detail();
        app.update(Action::Key(key(KeyCode::Char('x'))));
        assert!(matches!(
            app.popup,
            Some(Popup::Confirm {
                purpose: ConfirmPurpose::RemoveMember { .. },
                ..
            })
        ));
    }

    #[test]
    fn r_uppercase_opens_rename() {
        let mut app = app_on_group_detail();
        app.update(Action::Key(key(KeyCode::Char('R'))));
        assert!(matches!(
            app.popup,
            Some(Popup::TextInput {
                purpose: InputPurpose::RenameGroup,
                ..
            })
        ));
    }

    #[test]
    fn l_uppercase_confirms_leave() {
        let mut app = app_on_group_detail();
        app.update(Action::Key(key(KeyCode::Char('L'))));
        assert!(matches!(
            app.popup,
            Some(Popup::Confirm {
                purpose: ConfirmPurpose::LeaveGroup,
                ..
            })
        ));
    }

    #[test]
    fn confirm_leave_emits_effect() {
        let mut app = app_on_group_detail();
        app.popup = Some(Popup::Confirm {
            title: "Leave".into(),
            message: "Leave?".into(),
            purpose: ConfirmPurpose::LeaveGroup,
        });
        let effects = app.update(Action::Key(key(KeyCode::Char('y'))));
        assert!(app.popup.is_none());
        assert!(matches!(effects[0], Effect::LeaveGroup { .. }));
    }

    #[test]
    fn group_action_success_after_leave_goes_to_main() {
        let mut app = app_on_group_detail();
        app.update(Action::GroupActionSuccess("Left group".into()));
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn group_action_success_reloads_detail() {
        let mut app = app_on_group_detail();
        let effects = app.update(Action::GroupActionSuccess("Member added".into()));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadGroupDetail { .. })));
    }

    #[test]
    fn group_action_success_resubscribes_chats() {
        let mut app = app_on_main();
        app.account = Some("abc123".into());
        let effects = app.update(Action::GroupActionSuccess("Invite accepted".into()));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::SubscribeChats { .. })));
    }

    #[test]
    fn pending_invites_counts_from_chats() {
        let mut app = app_on_main();
        app.chats = vec![
            json!({"name": "Active", "pending_confirmation": false}),
            json!({"name": "Invite1", "pending_confirmation": true}),
            json!({"name": "Invite2", "pending_confirmation": true}),
            json!({"name": "NoField"}),
        ];
        assert_eq!(app.pending_invites(), 2);
    }

    #[test]
    fn pending_invites_zero_when_no_pending() {
        let mut app = app_on_main();
        app.chats = vec![json!({"name": "Active", "pending_confirmation": false})];
        assert_eq!(app.pending_invites(), 0);
    }

    #[test]
    fn login_loads_profile() {
        let mut app = App::new();
        let effects = app.update(Action::LoginSuccess("npub1xyz".into()));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadProfile { .. })));
    }

    #[test]
    fn i_uppercase_loads_invites() {
        let mut app = app_with_chats();
        let effects = app.update(Action::Key(key(KeyCode::Char('I'))));
        assert!(matches!(effects[0], Effect::LoadInvites { .. }));
    }

    #[test]
    fn invites_loaded_opens_popup() {
        let mut app = app_on_main();
        app.update(Action::InvitesLoaded(vec![
            json!({"mls_group_id": "inv1", "name": "Invite 1"}),
        ]));
        assert!(matches!(app.popup, Some(Popup::Invites { .. })));
    }

    #[test]
    fn empty_invites_shows_message() {
        let mut app = app_on_main();
        app.update(Action::InvitesLoaded(vec![]));
        assert!(app.popup.is_none());
        assert!(app.status_message.as_ref().unwrap().contains("No pending"));
    }

    #[test]
    fn popup_takes_key_priority() {
        let mut app = app_with_chats();
        app.popup = Some(Popup::TextInput {
            title: "Test".into(),
            input: Input::new(),
            purpose: InputPurpose::CreateGroup,
        });
        // 'q' should type in popup, not quit
        app.update(Action::Key(key(KeyCode::Char('q'))));
        assert!(app.running);
        if let Some(Popup::TextInput { input, .. }) = &app.popup {
            assert_eq!(input.value, "q");
        }
    }

    #[test]
    fn q_quits_from_chat_list() {
        let mut app = app_on_main();
        app.update(Action::Key(key(KeyCode::Char('q'))));
        assert!(!app.running);
    }

    // ── Profile ───────────────────────────────────────────────────────

    #[test]
    fn p_opens_profile_screen() {
        let mut app = app_on_main();
        let effects = app.update(Action::Key(key(KeyCode::Char('p'))));
        assert_eq!(app.screen, Screen::Profile);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadProfile { .. })));
    }

    #[test]
    fn profile_loaded_stores_data() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        app.update(Action::ProfileLoaded(
            json!({"name": "Alice", "about": "Hi"}),
        ));
        assert!(app.profile.is_some());
        assert_eq!(app.profile.as_ref().unwrap()["name"], "Alice");
    }

    #[test]
    fn profile_n_opens_edit_name() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        app.profile = Some(json!({"name": "Alice", "about": "Hi"}));
        app.update(Action::Key(key(KeyCode::Char('n'))));
        assert!(matches!(
            app.popup,
            Some(Popup::TextInput {
                purpose: InputPurpose::EditProfileName,
                ..
            })
        ));
    }

    #[test]
    fn profile_a_opens_edit_about() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        app.profile = Some(json!({"name": "Alice", "about": "Hi"}));
        app.update(Action::Key(key(KeyCode::Char('a'))));
        assert!(matches!(
            app.popup,
            Some(Popup::TextInput {
                purpose: InputPurpose::EditProfileAbout,
                ..
            })
        ));
    }

    #[test]
    fn profile_edit_name_submits_update() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        app.profile = Some(json!({"name": "Alice"}));
        let mut input = Input::new();
        input.insert('B');
        input.insert('o');
        input.insert('b');
        app.popup = Some(Popup::TextInput {
            title: "Edit Name".into(),
            input,
            purpose: InputPurpose::EditProfileName,
        });
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::UpdateProfile { name: Some(ref n), .. } if n == "Bob")));
    }

    #[test]
    fn profile_update_success_reloads() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        let effects = app.update(Action::ProfileUpdateSuccess("Updated".into()));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadProfile { .. })));
    }

    #[test]
    fn profile_esc_returns_to_main() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        app.update(Action::Key(key(KeyCode::Esc)));
        assert_eq!(app.screen, Screen::Main);
    }

    // ── Settings ──────────────────────────────────────────────────────

    #[test]
    fn s_uppercase_opens_settings() {
        let mut app = app_on_main();
        let effects = app.update(Action::Key(key(KeyCode::Char('S'))));
        assert_eq!(app.screen, Screen::Settings);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadSettings { .. })));
    }

    #[test]
    fn settings_loaded_stores_data() {
        let mut app = app_on_main();
        app.screen = Screen::Settings;
        app.update(Action::SettingsLoaded(json!({"theme": "dark"})));
        assert!(app.settings_data.is_some());
    }

    #[test]
    fn settings_esc_returns_to_main() {
        let mut app = app_on_main();
        app.screen = Screen::Settings;
        app.update(Action::Key(key(KeyCode::Esc)));
        assert_eq!(app.screen, Screen::Main);
    }

    // ── User search ───────────────────────────────────────────────────

    #[test]
    fn slash_opens_search_screen() {
        let mut app = app_on_main();
        app.update(Action::Key(key(KeyCode::Char('/'))));
        assert_eq!(app.screen, Screen::UserSearch);
    }

    #[test]
    fn search_enter_emits_search() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.search_input.insert('a');
        app.search_input.insert('l');
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::SearchUsers { ref query, .. } if query == "al")));
    }

    #[test]
    fn search_result_adds_to_list() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.update(Action::SearchResult(
            json!({"npub": "npub1x", "name": "Alice"}),
        ));
        assert_eq!(app.search_results.len(), 1);
    }

    #[test]
    fn search_arrow_keys_navigate_results() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.search_results = vec![json!({"npub": "a"}), json!({"npub": "b"})];
        app.update(Action::Key(key(KeyCode::Down)));
        assert_eq!(app.selected_result, 1);
        app.update(Action::Key(key(KeyCode::Up)));
        assert_eq!(app.selected_result, 0);
    }

    #[test]
    fn search_j_types_into_input() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.update(Action::Key(key(KeyCode::Char('j'))));
        assert_eq!(app.search_input.value, "j");
    }

    #[test]
    fn search_esc_returns_to_main() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        let effects = app.update(Action::Key(key(KeyCode::Esc)));
        assert_eq!(app.screen, Screen::Main);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::UnsubscribeSearch)));
    }

    // ── Follows ───────────────────────────────────────────────────────

    #[test]
    fn follows_loaded_populates_list() {
        let mut app = app_on_main();
        app.update(Action::FollowsLoaded(vec![
            json!({"pubkey": "abc"}),
            json!({"pubkey": "def"}),
        ]));
        assert_eq!(app.follows.len(), 2);
        assert!(app.is_following("abc"));
        assert!(app.is_following("def"));
    }

    #[test]
    fn follows_loaded_replaces_previous() {
        let mut app = app_on_main();
        app.follows = vec![json!({"pubkey": "old"})];
        app.update(Action::FollowsLoaded(vec![json!({"pubkey": "new"})]));
        assert_eq!(app.follows.len(), 1);
        assert!(app.is_following("new"));
        assert!(!app.is_following("old"));
    }

    #[test]
    fn follow_success_reloads_follows() {
        let mut app = app_on_main();
        let effects = app.update(Action::FollowSuccess("Followed abc".into()));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadFollows { .. })));
    }

    #[test]
    fn follow_error_shows_popup() {
        let mut app = app_on_main();
        app.update(Action::FollowError("Network error".into()));
        assert!(matches!(app.popup, Some(Popup::Error { .. })));
    }

    #[test]
    fn login_loads_follows() {
        let mut app = App::new();
        let effects = app.update(Action::LoginSuccess("npub1xyz".into()));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadFollows { .. })));
    }

    #[test]
    fn profile_loads_follows() {
        let mut app = app_on_main();
        let effects = app.update(Action::Key(key(KeyCode::Char('p'))));
        assert_eq!(app.screen, Screen::Profile);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::LoadFollows { .. })));
    }

    #[test]
    fn profile_j_navigates_follows() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        app.follows = vec![
            json!({"pubkey": "a", "name": "Alice"}),
            json!({"pubkey": "b", "name": "Bob"}),
        ];
        app.update(Action::Key(key(KeyCode::Char('j'))));
        assert_eq!(app.selected_follow, 1);
    }

    #[test]
    fn profile_d_unfollows_selected() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        app.follows = vec![
            json!({"pubkey": "abc", "name": "Alice"}),
            json!({"pubkey": "def", "name": "Bob"}),
        ];
        app.selected_follow = 1;
        let effects = app.update(Action::Key(key(KeyCode::Char('d'))));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::UnfollowUser { ref pubkey, .. } if pubkey == "def")));
    }

    #[test]
    fn follows_selected_clamped_after_reload() {
        let mut app = app_on_main();
        app.selected_follow = 5;
        app.update(Action::FollowsLoaded(vec![json!({"pubkey": "a"})]));
        assert_eq!(app.selected_follow, 0);
    }

    #[test]
    fn search_tab_follows_user() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.search_results = vec![json!({"pubkey": "abc123", "name": "Alice"})];
        app.selected_result = 0;
        let effects = app.update(Action::Key(key(KeyCode::Tab)));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::FollowUser { ref pubkey, .. } if pubkey == "abc123")));
    }

    #[test]
    fn search_tab_unfollows_if_already_following() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.search_results = vec![json!({"pubkey": "abc123", "name": "Alice"})];
        app.selected_result = 0;
        app.follows = vec![json!({"pubkey": "abc123"})];
        let effects = app.update(Action::Key(key(KeyCode::Tab)));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::UnfollowUser { ref pubkey, .. } if pubkey == "abc123")));
    }

    // ── Help overlay ──────────────────────────────────────────────────

    #[test]
    fn question_mark_opens_help() {
        let mut app = app_on_main();
        app.update(Action::Key(key(KeyCode::Char('?'))));
        assert!(matches!(
            app.popup,
            Some(Popup::Help {
                screen: Screen::Main
            })
        ));
    }

    #[test]
    fn help_esc_closes() {
        let mut app = app_on_main();
        app.popup = Some(Popup::Help {
            screen: Screen::Main,
        });
        app.update(Action::Key(key(KeyCode::Esc)));
        assert!(app.popup.is_none());
    }

    #[test]
    fn help_any_key_closes() {
        let mut app = app_on_main();
        app.popup = Some(Popup::Help {
            screen: Screen::Main,
        });
        app.update(Action::Key(key(KeyCode::Char('x'))));
        assert!(app.popup.is_none());
    }

    #[test]
    fn help_on_group_detail_shows_group_detail() {
        let mut app = app_on_group_detail();
        app.update(Action::Key(key(KeyCode::Char('?'))));
        assert!(matches!(
            app.popup,
            Some(Popup::Help {
                screen: Screen::GroupDetail
            })
        ));
    }

    // ── Error popup ───────────────────────────────────────────────────

    #[test]
    fn group_action_error_shows_error_popup() {
        let mut app = app_on_group_detail();
        app.update(Action::GroupActionError("Network error".into()));
        assert!(matches!(app.popup, Some(Popup::Error { .. })));
    }

    #[test]
    fn error_popup_esc_closes() {
        let mut app = app_on_main();
        app.popup = Some(Popup::Error {
            message: "Something broke".into(),
        });
        app.update(Action::Key(key(KeyCode::Esc)));
        assert!(app.popup.is_none());
    }

    #[test]
    fn error_popup_enter_closes() {
        let mut app = app_on_main();
        app.popup = Some(Popup::Error {
            message: "Something broke".into(),
        });
        app.update(Action::Key(key(KeyCode::Enter)));
        assert!(app.popup.is_none());
    }

    #[test]
    fn send_error_shows_error_popup() {
        let mut app = app_on_main();
        app.update(Action::MessageSendError("timeout".into()));
        assert!(matches!(app.popup, Some(Popup::Error { .. })));
    }

    #[test]
    fn profile_update_error_shows_error_popup() {
        let mut app = app_on_main();
        app.screen = Screen::Profile;
        app.update(Action::ProfileUpdateError("failed".into()));
        assert!(matches!(app.popup, Some(Popup::Error { .. })));
    }

    // ── Connection state ──────────────────────────────────────────────

    #[test]
    fn chat_update_sets_connected() {
        let mut app = app_on_main();
        assert!(!app.connected);
        app.update(Action::ChatUpdate(json!({"mls_group_id": "g1"})));
        assert!(app.connected);
    }

    #[test]
    fn chat_stream_ended_sets_disconnected() {
        let mut app = app_on_main();
        app.connected = true;
        app.update(Action::ChatStreamEnded);
        assert!(!app.connected);
    }

    #[test]
    fn chat_stream_ended_emits_reconnect() {
        let mut app = app_on_main();
        app.connected = true;
        let effects = app.update(Action::ChatStreamEnded);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::SubscribeChats { .. })));
    }

    // ── Log panel ────────────────────────────────────────────────────

    #[test]
    fn backtick_toggles_log_panel() {
        let mut app = app_on_main();
        assert!(!app.show_logs);

        app.update(Action::Key(key(KeyCode::Char('`'))));
        assert!(app.show_logs);

        app.update(Action::Key(key(KeyCode::Char('`'))));
        assert!(!app.show_logs);
    }

    #[test]
    fn tab_switches_log_tabs_when_panel_visible() {
        let mut app = app_on_main();
        app.show_logs = true;

        assert_eq!(app.log_tab, LogTab::Activity);
        app.update(Action::Key(key(KeyCode::Tab)));
        assert_eq!(app.log_tab, LogTab::Daemon);
        app.update(Action::Key(key(KeyCode::Tab)));
        assert_eq!(app.log_tab, LogTab::Activity);
    }

    #[test]
    fn tab_does_not_switch_logs_when_panel_hidden() {
        let mut app = app_on_main();
        assert!(!app.show_logs);

        // Tab should cycle panel focus, not switch log tabs
        app.update(Action::Key(key(KeyCode::Tab)));
        assert_eq!(app.log_tab, LogTab::Activity);
    }

    #[test]
    fn log_action_appends_to_buffer() {
        let mut app = App::new();
        app.update(Action::Log("test entry".into()));
        assert_eq!(app.logs.len(), 1);
        assert_eq!(app.logs[0], "test entry");
    }

    #[test]
    fn daemon_log_action_appends_to_daemon_buffer() {
        let mut app = App::new();
        app.update(Action::DaemonLog("daemon entry".into()));
        assert_eq!(app.daemon_logs.len(), 1);
        assert_eq!(app.daemon_logs[0], "daemon entry");
    }

    #[test]
    fn log_buffer_trims_when_over_limit() {
        let mut app = App::new();
        for i in 0..1001 {
            app.update(Action::Log(format!("entry {i}")));
        }
        assert!(app.logs.len() <= 501);
        // Oldest entries should be gone
        assert!(!app.logs[0].contains("entry 0"));
    }

    // ── Scroll preservation ──────────────────────────────────────────

    #[test]
    fn message_scroll_preserved_when_not_at_bottom() {
        let mut app = app_on_main();
        app.active_group_id = Some("g1".into());
        app.messages = vec![json!({"content": "old"})];
        app.message_scroll = 5; // Scrolled up

        app.update(msg_update(json!({"content": "new"})));
        assert_eq!(app.messages.len(), 2);
        assert_eq!(
            app.message_scroll, 6,
            "scroll should increase to keep position"
        );
    }

    #[test]
    fn message_scroll_stays_at_bottom() {
        let mut app = app_on_main();
        app.active_group_id = Some("g1".into());
        app.messages = vec![json!({"content": "old"})];
        app.message_scroll = 0; // At bottom

        app.update(msg_update(json!({"content": "new"})));
        assert_eq!(app.message_scroll, 0, "should auto-scroll");
    }

    // ── Invite popup navigation ──────────────────────────────────────

    #[test]
    fn invite_popup_j_k_navigates() {
        let mut app = app_on_main();
        app.popup = Some(Popup::Invites {
            items: vec![
                json!({"group_id": "g1", "name": "Group A"}),
                json!({"group_id": "g2", "name": "Group B"}),
            ],
            selected: 0,
        });
        app.update(Action::Key(key(KeyCode::Char('j'))));
        if let Some(Popup::Invites { selected, .. }) = &app.popup {
            assert_eq!(*selected, 1);
        } else {
            panic!("Expected Invites popup");
        }

        app.update(Action::Key(key(KeyCode::Char('k'))));
        if let Some(Popup::Invites { selected, .. }) = &app.popup {
            assert_eq!(*selected, 0);
        } else {
            panic!("Expected Invites popup");
        }
    }

    #[test]
    fn invite_popup_clamps_at_bounds() {
        let mut app = app_on_main();
        app.popup = Some(Popup::Invites {
            items: vec![json!({"group_id": "g1"})],
            selected: 0,
        });
        // Can't go below 0
        app.update(Action::Key(key(KeyCode::Char('k'))));
        if let Some(Popup::Invites { selected, .. }) = &app.popup {
            assert_eq!(*selected, 0);
        } else {
            panic!("Expected Invites popup");
        }
    }

    #[test]
    fn invite_accept_emits_effect() {
        let mut app = app_on_main();
        app.popup = Some(Popup::Invites {
            items: vec![json!({
                "group": {"mls_group_id": "abc123", "name": "Test Group"},
                "membership": {}
            })],
            selected: 0,
        });
        let effects = app.update(Action::Key(key(KeyCode::Char('a'))));
        assert!(effects.iter().any(
            |e| matches!(e, Effect::AcceptInvite { ref group_id, .. } if group_id == "abc123")
        ));
    }

    #[test]
    fn invite_group_id_extracts_from_nested_group() {
        let mut app = app_on_main();
        app.popup = Some(Popup::Invites {
            items: vec![json!({
                "group": {
                    "mls_group_id": {"value": {"vec": [174, 153, 3]}},
                    "name": "Test"
                }
            })],
            selected: 0,
        });
        let effects = app.update(Action::Key(key(KeyCode::Char('a'))));
        assert!(effects.iter().any(
            |e| matches!(e, Effect::AcceptInvite { ref group_id, .. } if group_id == "ae9903")
        ));
    }

    // ── Empty input rejection ────────────────────────────────────────

    #[test]
    fn empty_text_input_not_submitted() {
        let mut app = app_on_main();
        app.popup = Some(Popup::TextInput {
            title: "Name".into(),
            input: Input::new(),
            purpose: InputPurpose::CreateGroup,
        });
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert!(effects.is_empty(), "empty input should not emit effects");
        assert!(app.popup.is_some(), "popup should remain open");
    }

    #[test]
    fn empty_composer_not_sent() {
        let mut app = app_on_main();
        app.focus = Panel::Composer;
        app.active_group_id = Some("g1".into());
        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::SendMessage { .. })),
            "empty message should not be sent"
        );
    }

    // ── Login emits TailDaemonLog ────────────────────────────────────

    #[test]
    fn login_success_starts_daemon_log_tail() {
        let mut app = App::new();
        let effects = app.update(Action::LoginSuccess("npub1xyz".into()));
        assert!(
            effects.iter().any(|e| matches!(e, Effect::TailDaemonLog)),
            "login should start tailing daemon logs"
        );
    }

    // ── Search with AddMember purpose ────────────────────────────────

    #[test]
    fn search_select_with_add_member_purpose_emits_add_member() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.search_purpose = SearchPurpose::AddMember {
            group_id: "g1".into(),
        };
        app.search_results = vec![json!({"npub": "npub1alice", "name": "Alice"})];
        app.selected_result = 0;

        let effects = app.update(Action::Key(key(KeyCode::Enter)));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::AddMember { ref npub, .. } if npub == "npub1alice")));
    }

    // ── Backtick ignored in search screen ────────────────────────────

    #[test]
    fn backtick_types_in_search_instead_of_toggling_logs() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.update(Action::Key(key(KeyCode::Char('`'))));
        assert!(
            !app.show_logs,
            "backtick should not toggle logs on search screen"
        );
    }

    #[test]
    fn question_mark_not_available_on_search_screen() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.update(Action::Key(key(KeyCode::Char('?'))));
        assert!(
            app.popup.is_none(),
            "? should type into input, not open help"
        );
    }

    #[test]
    fn question_mark_not_available_in_composer() {
        let mut app = app_on_main();
        app.focus = Panel::Composer;
        app.update(Action::Key(key(KeyCode::Char('?'))));
        assert!(
            app.popup.is_none(),
            "? should type into composer, not open help"
        );
    }

    // ── Paste ───────────────────────────────────────────────────────

    #[test]
    fn paste_multiline_into_composer() {
        let mut app = app_on_main();
        app.focus = Panel::Composer;
        app.update(Action::Paste("line1\nline2\nline3".into()));
        assert_eq!(app.composer.value, "line1\nline2\nline3");
    }

    #[test]
    fn paste_into_search() {
        let mut app = app_on_main();
        app.screen = Screen::UserSearch;
        app.update(Action::Paste("npub1abc".into()));
        assert_eq!(app.search_input.value, "npub1abc");
    }

    #[test]
    fn paste_ignored_when_not_in_input() {
        let mut app = app_on_main();
        app.focus = Panel::ChatList;
        app.update(Action::Paste("should be ignored".into()));
        assert!(app.composer.is_empty());
    }
}
