- [x] Apply `BackgroundThrottlingPolicy::Suspend` in `create_main_window()`.
- [x] Remove the unused `destroy_main_window`.
- [ ] Run `cargo check`.

## Review

- Pending verification after `cargo check`.

## 2026-05-21 Background throttling follow-up

- [x] Add `BackgroundThrottlingPolicy::Suspend`.
- [x] Remove the unused `destroy_main_window`.
- [x] Run `rtk cargo check --manifest-path src-tauri/Cargo.toml`.
- [x] Review results.

### Review / Results

- `src-tauri/src/main.rs` already matched the requested Rust changes, so no code edits were needed there.
- Verification passed with `rtk cargo check --manifest-path src-tauri/Cargo.toml`.

## 2026-05-21 Drag after zoom fix

- [x] Diagnose the drag regression after maximize/restore from the overlay titlebar.
- [x] Patch the manual drag handler in `installZlackDragRegions()`.
- Automated GUI testing showed the injected overlay drag regions were unreliable and still did not move the window.
- Restored the native visible macOS titlebar so the OS handles window dragging and double-click zoom behavior.
- Forced the macOS native titlebar appearance to Dark Aqua so it matches Slack dark UI.
- A follow-up screenshot still showed a white native macOS titlebar after the window-only appearance change, so app-wide NSApplication Dark Aqua was added too.
- [x] Run `rtk node --check src-tauri/preload.js`.
- [ ] Launch the app and verify drag/maximize behavior.
- [ ] Review results.

## 2026-05-21 Titlebar-less drag root cause fix

- Root cause: macOS custom titlebar drag regions require `decorations(false)`, so a decorated native titlebar cannot be the drag path for the custom UI.
- Fix: restore the titlebar-less window, re-enable manual JavaScript drag handling, and add explicit Tauri window permissions for `startDragging()` and `toggleMaximize()`.
- Automated test follow-up: the initial `decorations(false)` plus permissions patch still no-oped because the preload script could capture the Tauri window API before it was ready, so the drag path now resolves the window at event time and falls back to Rust commands.

## 2026-05-21 Overlay titlebar final approach

- Final approach: use `TitleBarStyle::Overlay` with `hidden_title(true)` for a titlebar-less feel while keeping macOS native window chrome, and do not auto-install JavaScript drag overlays.
- Reason: native macOS should own dragging and double-click maximize or unmaximize, while the injected JavaScript overlays caused the original no-drag-after-double-click regression and interfered with Slack search interactions.

## 2026-05-21 Overlay drag strip final implementation

- Final approach: keep the overlay titlebar for native rounded corners and traffic lights, and inject a single 28px-tall top drag strip that starts at x=80 so the macOS window buttons remain clickable.
- The preload script now handles dragging and double-click maximize manually through `startDragging()` and `toggleMaximize()` with Rust command fallback, instead of relying on Slack layout-specific drag regions.
- This fixes both the drag path breaking after maximize and restore and the Slack UI overlapping the OS drag region, while keeping the overlay titlebar configuration unchanged.

## 2026-05-21 Slack view reply deeplink fix

- [x] Locate Slack internal link interception path in `src-tauri/preload.js`.
- [x] Normalize Slack workspace permalink/app_redirect URLs to `app.slack.com/client` routes before navigation.
- [x] Run `rtk node --check src-tauri/preload.js`.
- [x] Run `rtk cargo check --manifest-path src-tauri/Cargo.toml`.
- [x] Review results.

### Review / Results

- Fixed Slack workspace permalink/app_redirect links so `View reply` navigates inside Zlack instead of the Slack browser launcher page.
- Verification passed: `rtk node --check src-tauri/preload.js` and `rtk cargo check --manifest-path src-tauri/Cargo.toml`.

## 2026-05-21 Slack deeplink freeze follow-up

- [x] Identify that synthesized `/client/.../thread/...` routes can stall Slack.
- [x] Change normalization to use `https://app.slack.com/app_redirect` instead of undocumented thread routes.
- [x] Run JavaScript and Rust validation.
- [x] Rebuild release and copy to `/Applications`.
- [x] Review results.

### Review / Results

- Replaced synthesized Slack thread routes with `https://app.slack.com/app_redirect` normalization to avoid the frozen Slack client screen.
- Verification passed: `rtk node --check src-tauri/preload.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk npm run tauri build`.
- Installed rebuilt app to `/Applications/Zlack.app`.

## 2026-05-21 Slack thread permalink fix follow-up

- [x] Reproduced the provided permalink parsing shape.
- [x] Route archive permalinks using query `thread_ts` as the parent thread id and `p...` as the selected reply message.
- [x] Add team id fallbacks from current Slack route, notification telemetry, Slack globals, and web storage.
- [x] Verify `rtk node --check src-tauri/preload.js`.
- [x] Verify `rtk cargo check --manifest-path src-tauri/Cargo.toml`.
- [x] Build release, copy to `/Applications/Zlack.app`, and relaunch installed app.

### Review / Results

- Provided URL now maps to `https://app.slack.com/client/<TEAM_ID>/C0B378QA49F/thread/C0B378QA49F-1778815631.641049?message_ts=1779338098.102489`.
- Installed app process is running from `/Applications/Zlack.app/Contents/MacOS/Zlack`.
- UI automation/screenshot verification was blocked by macOS Screen Recording/Accessibility permissions in this CLI session.

## 2026-05-21 Slack Desktop shortcut parity

- [x] Inspect current menu/shortcut handling.
- [x] Add macOS app menu accelerators for Slack Desktop shortcuts beyond existing Cmd+K.
- [x] Bridge accelerator menu events into the Slack webview.
- [x] Run JS/Rust validation.
- [x] Build/install release if validation passes.

### Review / Results

- Added a native Slack menu with accelerators for common Slack Desktop shortcuts and a preload bridge that re-dispatches them into Slack.
- Validation passed: `rtk node --check src-tauri/preload.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.
- Release build was installed to `/Applications/Zlack.app` and relaunched.

## 2026-05-21 Shortcut interception fix

- [x] Confirm native Slack menu accelerators were intercepting physical shortcuts before Slack webview.
- [x] Remove native Slack accelerator menu and synthetic keyboard bridge.
- [x] Validate JS/Rust builds.
- [x] Rebuild release and reinstall to /Applications.

### Review / Results

- Removed native Slack menu accelerators because they intercepted trusted physical key events before Slack could receive them.
- Removed the synthetic keyboard bridge because Slack ignores untrusted `KeyboardEvent`s for these shortcuts.
- Validation passed: `rtk node --check src-tauri/preload.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.
- Release build installed to `/Applications/Zlack.app` and relaunched.

## 2026-05-21 Slack Desktop shortcut action bridge

- [x] Identify that Slack Web/Chrome does not implement several Slack Desktop-only shortcuts.
- [x] Replace synthetic keyboard event forwarding with native shortcut actions.
- [x] Add Tauri menu accelerators that call Slack route/DOM actions directly instead of re-dispatching key events.
- [x] Validate JavaScript syntax and Rust build.
- [x] Run strict clippy validation.
- [x] Build release, install to `/Applications/Zlack.app`, and relaunch.

### Review / Results

- Physical shortcuts are now handled as native app shortcuts for Desktop-only cases, then mapped to Slack actions/routes/visible controls.
- `Cmd+K` is intentionally left to Slack Web unchanged.
- Added follow-up coverage for `Cmd+O` upload and `Cmd+J` latest unread after checking Slack Desktop-only shortcut classification.
- Validation passed: `rtk node --check src-tauri/preload.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.
- Release build installed and launched from `/Applications/Zlack.app`.

## 2026-05-21 Shortcut bridge direct eval fix

- [x] Confirm the previous native shortcut bridge depended on a preload-owned `window.__zlackRunSlackShortcutAction` function.
- [x] Remove that preload/window bridge dependency.
- [x] Move the Slack action runner into `src-tauri/shortcut_actions.js`.
- [x] Update Rust menu accelerator handling to inject and execute the action runner directly via `window.eval` on each shortcut.
- [x] Validate JavaScript and Rust.
- [x] Build release, install to `/Applications/Zlack.app`, and relaunch.

### Review / Results

- This addresses the likely issue that native shortcut handling was not actually connected to the Slack page/source context through the preload global.
- Validation passed: `rtk node --check src-tauri/preload.js`, `rtk node --check src-tauri/shortcut_actions.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.
- Release build installed and launched from `/Applications/Zlack.app`.

## 2026-05-21 Shortcut diagnostic instrumentation

- [x] Ask advisor and Codex rescue for independent shortcut diagnosis.
- [x] Add Rust-side native shortcut fired logging/notification.
- [x] Add Slack-page visible diagnostic overlay showing eval reached, action result, URL, and exceptions.
- [x] Move shortcut context access from preload lexical `lastEventContext` to `window.__zlackLastEventContext`.
- [x] Validate JS/Rust and clippy.
- [x] Build release, install to `/Applications/Zlack.app`, and relaunch.

### Review / Results

- Both advisors agreed the missing piece was instrumentation to separate native accelerator, eval, and Slack action failures.
- Installed diagnostic build: pressing mapped shortcuts should show a temporary top-right `Zlack shortcut` overlay in Slack and a native notification.
- Validation passed: `rtk node --check src-tauri/preload.js`, `rtk node --check src-tauri/shortcut_actions.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.

## 2026-05-21 Shortcut action refinement from diagnostics

- [x] Use diagnostic feedback showing `Cmd+F` pipeline worked but search click was a false-positive.
- [x] After opening search, attempt to click/select the first visible search suggestion row.
- [x] Use attribute/text matching for Activity and Threads controls before route fallback.
- [x] Keep hard route navigation skipped when no existing Slack link is present, avoiding reload/freeze.
- [x] Validate and reinstall release build.

### Review / Results

- `Cmd+[` is confirmed fixed by user.
- `Cmd+F` now schedules first search suggestion selection after opening search.
- `Cmd+Shift+M` and `Cmd+Shift+T` now try visible Slack controls by activity/thread-related attributes instead of immediately relying on unstable route navigation.
- Validation passed: `rtk node --check src-tauri/shortcut_actions.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.

## 2026-05-21 File and Go menu shortcut scope

- [x] Limit custom native shortcuts to the visible Slack-style File and Go menu items from the screenshots.
- [x] Keep `Cmd+F` out of the custom bridge so the existing/default behavior remains untouched.
- [x] Restore `Cmd+[` and `Cmd+]` history handling because the user explicitly wanted to keep them.
- [x] Add File menu items: New Message, New Canvas, Close Window, Show Main Window.
- [x] Validate JS/Rust and clippy.
- [x] Build release, install to `/Applications/Zlack.app`, and relaunch.

### Review / Results

- Installed build exposes native `File` and `Go` menus rather than broad Slack Desktop parity shortcuts.
- Custom shortcut actions now cover the screenshot menu scope: New Message/New Canvas plus Go destinations like All Unreads, Threads, All DMs, Activity, Channel Browser, People, Downloads, and history back/forward.
- Validation passed: `rtk node --check src-tauri/shortcut_actions.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.
## 2026-05-21 Cmd+F and Threads refinement

- [x] Restore `Cmd+F` custom search opening.
- [x] Remove automatic Enter/search-submit behavior after opening search.
- [x] Change Threads action to only click visible Slack sidebar/menu elements.
- [x] Disable Threads hard navigation fallback to avoid refresh/freeze behavior.
- [x] Validate Rust/JS and reinstall release build.

### Review / Results

- `Cmd+F` now opens search but does not auto-submit.
- `Cmd+Shift+T` no longer calls hard navigation; if the Threads sidebar/menu item is not found, it returns false instead of refreshing Slack.
- Validation passed: `rtk cargo check --manifest-path src-tauri/Cargo.toml` and `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`.
- Release build installed to `/Applications/Zlack.app` and relaunched.