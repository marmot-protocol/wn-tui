# wn-tui

A terminal user interface for [WhiteNoise](https://github.com/marmot-protocol/whitenoise), a secure group messenger built on MLS and Nostr.

```
+-[Chats]--------+-[Coffee Chat]-------------------------------+
| > Coffee Chat  | [10:31] alice: Hey everyone                 |
|   Work         | [10:32] bob: What's up?                     |
|   DM: eve      | [10:33] alice: Let's discuss the release     |
|                |                👍 2  🎉 1                   |
|                |                                              |
|                +----------------------------------------------+
|                | Type a message...                            |
+----------------+----------------------------------------------+
 j/k Navigate  Enter Open  n New group  g Group info  q Quit
 * alice npub1abc...xyz | 3 chats | 1 pending invite
```

## Prerequisites

wn-tui is a pure presentation layer over the WhiteNoise CLI. It spawns `wn` commands as subprocesses and parses their JSON output. You need both the daemon and the CLI client running.

### 1. WhiteNoise daemon and CLI

Build from the [`feat/cli`](https://github.com/marmot-protocol/whitenoise/pull/537) branch of whitenoise-rs:

```sh
git clone https://github.com/marmot-protocol/whitenoise.git
cd whitenoise
git checkout feat/cli
cargo build --release
```

This produces two binaries in `target/release/`:

- **`wnd`** -- the WhiteNoise daemon. Runs the MLS/Nostr stack and listens on a Unix socket.
- **`wn`** -- the CLI client. Sends commands to the daemon over the socket.

Start the daemon before launching the TUI:

```sh
./target/release/wnd
```

Make sure `wn` is in your `PATH`, or place it alongside the `wn-tui` binary.

### 2. Rust toolchain

Rust 1.79+ (2021 edition). Install via [rustup](https://rustup.rs/) if needed.

## Build & Run

```sh
cd wn-tui
cargo build --release
./target/release/wn-tui
```

Or during development:

```sh
cargo run
```

## Architecture

### Communication model

wn-tui never speaks to the daemon directly. Every interaction goes through the `wn` CLI with `--json` output:

```
+---------+    spawn/exec     +--------+    Unix socket    +---------+
| wn-tui  | ----- stdout --- | wn CLI | ---------------- |   wnd   |
| (TUI)   |    --json         |        |    JSON-line     | (daemon)|
+---------+                   +--------+                   +---------+
```

- **One-shot commands** (`wn --json groups list`, `wn --json messages send ...`): spawn, wait, parse JSON result.
- **Streaming commands** (`wn --json messages subscribe ...`, `wn --json chats subscribe ...`): spawn a long-lived child process, read JSON lines from stdout continuously, kill the child on navigation away.

This keeps the TUI as a thin presentation layer. No protocol types to duplicate, no socket lifecycle to manage. The CLI handles authentication, error formatting, and daemon communication.

### State management (Elm/TEA)

Unidirectional data flow:

```
Terminal events --+
                  |
Stream updates ---+--> mpsc channel --> Event --> Action --> update(state) --> draw(state)
                  |
Tick timer -------+
```

- **`Event`** -- raw inputs (key press, paste, tick, async action result)
- **`Action`** -- all possible state mutations (enum)
- **`App::update(action)`** -- the single place state changes, returns a `Vec<Effect>`
- **`Effect`** -- side effects for the main loop to execute (spawn CLI commands, subscribe to streams)

State transitions are predictable and testable. The `update()` function is pure: given a state and an action, it produces a new state and effects.

### Screens

```
Login --> Main (chat list + messages)
            |---> Group Detail
            |---> Profile
            |---> Settings
            +---> User Search
```

Screens are an enum, not trait objects. Exhaustive matching ensures every screen is handled. `Esc` always navigates back.

## Project structure

```
src/
  main.rs              Entry point, event loop, effect execution
  app.rs               App state, update() dispatcher, key handling
  action.rs            Action enum (state mutations) + Effect enum (side effects)
  event.rs             Event enum, terminal/tick event producers
  tui.rs               Terminal init/restore, panic hook
  wn.rs                CLI wrapper (spawn wn --json, parse responses, stream lines)
  screen/
    mod.rs             Screen enum
    login.rs           Login / create identity
    main_screen.rs     Split panel: chat list + messages + composer
    group_detail.rs    Group info, members, admin actions
    profile.rs         View/edit profile
    settings.rs        Settings display
    user_search.rs     Streaming user search
  widget/
    mod.rs
    chat_list.rs       Sidebar chat list with unread badges
    message_list.rs    Scrollable messages with reactions, wrapping, multi-line
    input.rs           Unicode-safe text input with cursor, auto-growing height
    status_bar.rs      Connection status, account info, unread/invite counts
    popup.rs           Modal dialogs (confirm, input prompt)
```

## Key bindings

### Global

| Key      | Action         |
| -------- | -------------- |
| `Ctrl+C` | Quit           |
| `Esc`    | Back / unfocus |
| `?`      | Help           |

### Chat list

| Key       | Action         |
| --------- | -------------- |
| `j` / `k` | Navigate chats |
| `Enter`   | Open chat      |
| `n`       | New group      |
| `g`       | Group info     |
| `I`       | View invites   |
| `/`       | Search users   |
| `p`       | Profile        |
| `S`       | Settings       |
| `` ` ``   | Toggle logs    |
| `q`       | Quit           |

### Messages

| Key           | Action            |
| ------------- | ----------------- |
| `j` / `k`     | Scroll messages   |
| `G`           | Jump to bottom    |
| `i` / `Enter` | Start composing   |
| `Esc`         | Back to chat list |

### Composer

| Key     | Action         |
| ------- | -------------- |
| `Enter` | Send message   |
| `Esc`   | Stop composing |

## License

TBD
