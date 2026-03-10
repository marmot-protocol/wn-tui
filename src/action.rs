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

    // Reactions
    ReactionSuccess,
    ReactionError(String),
    MessagesLoaded(Vec<Value>),

    // Message deletion
    MessageDeleted {
        message_id: String,
    },
    MessageDeleteError(String),

    // Media upload
    MediaUploaded,
    MediaUploadError(String),

    // Media
    MediaDownloaded {
        file_hash: String,
        file_path: String,
    },
    MediaDownloadFailed {
        file_hash: String,
        error: String,
    },
    MediaImageLoaded {
        file_hash: String,
        bytes: Vec<u8>,
    },
    MediaPopupReady {
        bytes: Vec<u8>,
        img_width: u32,
        img_height: u32,
    },

    // Notifications (streaming)
    NotificationUpdate(Value),
    NotificationStreamEnded,

    // Group management
    GroupDetailLoaded(Value),
    GroupMembersLoaded {
        members: Vec<Value>,
        admins: Vec<Value>,
    },
    GroupRelaysLoaded(Vec<String>),
    AccountRelaysLoaded(Vec<Value>),
    InvitesLoaded(Vec<Value>),
    GroupActionSuccess(String),
    GroupActionError(String),

    // Profile
    ProfileLoaded(Value),
    ProfileImageFetched(Vec<u8>),
    ProfileUpdateSuccess(String),
    ProfileUpdateError(String),
    NsecExported(String),
    NsecExportError(String),

    // Settings
    SettingsLoaded(Value),
    SettingsUpdateError(String),

    // Follows
    FollowsLoaded(Vec<Value>),
    FollowSuccess(String),
    FollowError(String),
    FollowCheckResult {
        pubkey: String,
        following: bool,
    },

    // User search
    SearchResult(Value),
    SearchStreamEnded,
    UserProfileLoaded(Value),
    UserProfileError(String),

    // Account management
    LogoutSuccess,
    LogoutError(String),

    // Relay health
    RelayHealthLoaded(Value),
    RelayHealthError(String),

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
        reply_to: Option<String>,
    },

    LoadMessages {
        account: String,
        group_id: String,
    },

    // Reactions
    ReactToMessage {
        account: String,
        group_id: String,
        message_id: String,
        emoji: String,
    },
    UnreactToMessage {
        account: String,
        group_id: String,
        message_id: String,
    },
    DeleteMessage {
        account: String,
        group_id: String,
        message_id: String,
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
    LoadGroupRelays {
        account: String,
        group_id: String,
    },
    LoadAccountRelays {
        account: String,
    },
    LoadInvites {
        account: String,
    },
    CreateGroup {
        account: String,
        name: String,
        members: Vec<String>,
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
        display_name: Option<String>,
        about: Option<String>,
        picture: Option<String>,
        nip05: Option<String>,
        lud16: Option<String>,
    },
    ExportNsec {
        account: String,
    },
    FetchProfileImage {
        url: String,
    },

    // Settings
    LoadSettings {
        account: String,
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
    ShowUserProfile {
        account: String,
        pubkey: String,
    },

    // Account management
    Logout {
        account: String,
    },

    // Media
    DownloadMedia {
        account: String,
        group_id: String,
        file_hash: String,
    },
    LoadMediaImage {
        file_hash: String,
        file_path: String,
    },
    LoadMediaPopup {
        file_path: String,
    },
    UploadMedia {
        account: String,
        group_id: String,
        file_path: String,
    },

    // Relay health
    LoadRelayHealth,

    // Daemon logs
    TailDaemonLog,
}
