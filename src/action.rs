use crossterm::event::KeyEvent;
use serde_json::Value;

/// All possible state mutations in the application.
#[derive(Debug, Clone)]
pub enum Action {
    Tick,
    Quit,
    Render,
    Key(KeyEvent),
    Paste(String),

    // Login flow
    AccountsLoaded(Vec<Value>),
    LoginSuccess(String), // npub
    LoginError(String),

    // Chat list (streaming)
    ChatUpdate(Value),
    ChatStreamEnded,

    // Messages (streaming)
    MessageUpdate {
        group_id: String,
        message: Value,
    },
    MessageStreamEnded,

    // Send message
    MessageSent,
    MessageSendError(String),

    // Notifications (streaming)
    NotificationUpdate(Value),
    NotificationStreamEnded,

    // Group management
    GroupDetailLoaded(Value),
    GroupMembersLoaded {
        members: Vec<Value>,
        admins: Vec<Value>,
    },
    InvitesLoaded(Vec<Value>),
    GroupActionSuccess(String),
    GroupActionError(String),

    // Profile
    ProfileLoaded(Value),
    ProfileUpdateSuccess(String),
    ProfileUpdateError(String),
    NsecExported(String),
    NsecExportError(String),

    // Settings
    SettingsLoaded(Value),
    SettingsUpdateSuccess(String),
    SettingsUpdateError(String),

    // Follows
    FollowsLoaded(Vec<Value>),
    FollowSuccess(String),
    FollowError(String),
    FollowCheckResult { pubkey: String, following: bool },

    // User search
    SearchResult(Value),
    SearchStreamEnded,

    // Logs
    Log(String),
    DaemonLog(String),
}

/// Side effects returned by App::update() for the main loop to execute.
#[derive(Debug, Clone)]
pub enum Effect {
    CheckAccounts,
    CreateIdentity,
    LoginWithNsec(String),

    // Streaming
    SubscribeNotifications,
    SubscribeChats {
        account: String,
    },
    SubscribeMessages {
        account: String,
        group_id: String,
    },
    UnsubscribeMessages,

    // One-shot
    SendMessage {
        account: String,
        group_id: String,
        text: String,
    },

    // Group management
    LoadGroupDetail {
        account: String,
        group_id: String,
    },
    LoadGroupMembers {
        account: String,
        group_id: String,
    },
    LoadInvites {
        account: String,
    },
    CreateGroup {
        account: String,
        name: String,
    },
    AddMember {
        account: String,
        group_id: String,
        npub: String,
    },
    RemoveMember {
        account: String,
        group_id: String,
        npub: String,
    },
    RenameGroup {
        account: String,
        group_id: String,
        name: String,
    },
    LeaveGroup {
        account: String,
        group_id: String,
    },
    AcceptInvite {
        account: String,
        group_id: String,
    },
    DeclineInvite {
        account: String,
        group_id: String,
    },

    // Profile
    LoadProfile {
        account: String,
    },
    UpdateProfile {
        account: String,
        name: Option<String>,
        about: Option<String>,
    },
    ExportNsec {
        account: String,
    },

    // Settings
    LoadSettings {
        account: String,
    },
    #[allow(dead_code)]
    UpdateSetting {
        account: String,
        key: String,
        value: String,
    },

    // Follows
    LoadFollows {
        account: String,
    },
    FollowUser {
        account: String,
        pubkey: String,
    },
    UnfollowUser {
        account: String,
        pubkey: String,
    },
    CheckFollow {
        account: String,
        pubkey: String,
    },

    // User search
    SearchUsers {
        account: String,
        query: String,
    },
    UnsubscribeSearch,

    // Daemon logs
    TailDaemonLog,
}
