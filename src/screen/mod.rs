pub mod group_detail;
pub mod login;
pub mod main_screen;
pub mod profile;
pub mod settings;
pub mod user_search;

/// All screens in the application. Fixed set, exhaustive matching.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Login,
    Main,
    GroupDetail,
    Profile,
    Settings,
    UserSearch,
}
