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

## 2026-05-22 External HTTP link browser fix

- [x] Locate Slack link interception in `src-tauri/preload.js`.
- [x] Replace string-contains Slack detection with parsed hostname detection.
- [x] Open external http(s) links through Tauri shell invoke so the OS default browser handles them.
- [x] Run JavaScript syntax validation.
- [x] Run Rust build validation.
- [x] Review results.

### Review / Results

- External http(s) links now call Tauri shell `open` via `window.__TAURI_INTERNALS__.invoke('plugin:shell|open', ...)` when the global shell helper is unavailable, so the OS default browser handles them.
- Slack-owned hosts remain inside the Zlack webview, using parsed hostname checks instead of broad substring matching.
- Validation passed: `rtk node --check src-tauri/preload.js` and `rtk cargo check --manifest-path src-tauri/Cargo.toml`.

## 2026-05-22 macOS occlusion memory saver

- [x] Inspect current Rust memory saver scheduling.
- [x] Add short hidden/minimized and occluded destroy delays.
- [x] Gate delayed destroys on final effective visibility using macOS NSWindow occlusion state.
- [x] Run `rtk cargo check --manifest-path src-tauri/Cargo.toml`.
- [x] Review results.

### Review / Results

- Set hidden/minimized and occluded memory-saver delays to 3 minutes with a final effective-visibility check before destroying the Slack webview.
- macOS uses NSWindow `occlusionState`; visibility-check errors are treated conservatively as visible.
- Verification passed: `rtk cargo check --manifest-path src-tauri/Cargo.toml`.


## 2026-05-22 Occlusion memory saver polling

- Event-only occlusion handling missed covered-window cases when no focus, hide, or minimize event armed the destroy timer.
- Added periodic effective-visibility polling so an occluded main window destroys WebContent after 3 continuous minutes.

## 2026-05-22 macOS covered-window visibility

- [x] Confirm `NSWindow.occlusionState` alone did not catch Zlack being covered by another app.
- [x] Add `CGWindowListCopyWindowInfo` coverage sampling so higher z-order layer-0 windows can mark the main window materially covered.
- [x] Run `rtk cargo check --manifest-path /Users/jinku/sendbird/zlack/src-tauri/Cargo.toml`.
- [x] Review results.

### Review / Results

- Added CoreGraphics window-list coverage sampling because `NSWindow.occlusionState` alone did not detect Zlack covered by another app.
- Verification passed: `rtk cargo check --manifest-path /Users/jinku/sendbird/zlack/src-tauri/Cargo.toml`.
## 2026-05-22 macOS relative window coverage detection

- [x] Update CGWindowList coverage detection to query blockers with `kCGWindowListOptionOnScreenAboveWindow` relative to Zlack's window number.
- [x] Run `rtk cargo check --manifest-path /Users/jinku/sendbird/zlack/src-tauri/Cargo.toml`.

### Review / Results

- Coverage detection now uses target-window bounds plus `kCGWindowListOptionOnScreenAboveWindow` blockers relative to Zlack's window number.
- Verification passed with `rtk cargo check --manifest-path /Users/jinku/sendbird/zlack/src-tauri/Cargo.toml`.

## 2026-05-22 macOS covered-window PID fallback

- Added PID-based fallback for CGWindow matching when `NSWindow.windowNumber` does not resolve to a usable CGWindow target or relative blockers are unavailable.

## 2026-05-22 Zoom Join external open fix

- [x] Add Zoom URL/protocol detection helpers in `src-tauri/preload.js`.
- [x] Wrap `window.open` so Zoom and safe external HTTP(S) links open through the OS while Slack internal links stay in-webview.
- [x] Add a focused capture-click fallback for Slack Zoom app card Join buttons that builds a `zoommtg://` join URL from nearby meeting details.
- [x] Validate JavaScript syntax and Rust build.
- [x] Review results.

### Review / Results

- Zoom protocols and Zoom web hosts now route through Tauri shell open, allowing the native Zoom app to claim meeting links.
- Slack internal links continue to use the existing normalization/current-webview behavior.
- Verification passed: `rtk node --check /Users/jinku/sendbird/zlack/src-tauri/preload.js` and `rtk cargo check --manifest-path /Users/jinku/sendbird/zlack/src-tauri/Cargo.toml`.

## 2026-05-22 Zoom Slack card visible join button

- [x] Inspect existing Zoom link/button interception in `src-tauri/preload.js`.
- [x] Add a visible fallback Join Zoom button when Slack renders the Zoom app card action as an invisible/skeleton area.
- [x] Reuse existing external-open flow so the fallback opens the native Zoom app.
- [x] Validate JavaScript syntax and Rust build.
- [x] Record review results.
### Review / Results

- Added a Slack DOM observer that detects Zoom cards with hidden Zoom links or meeting text and injects a visible `Join Zoom` button only when no visible join control exists.
- The injected button reuses `openExternalLink`, so Zoom URLs still open through the OS/native Zoom flow instead of inside Slack.
- Verification passed: `rtk node --check src-tauri/preload.js` and `rtk cargo check --manifest-path src-tauri/Cargo.toml`.

## 2026-05-22 Revert intrusive Zoom card rendering hooks

- [x] Remove global `window.open` wrapping because Slack app card rendering previously worked and the hook was too broad.
- [x] Remove DOM observer/fallback button injection that tried to compensate for the broken Zoom card UI instead of preserving Slack's original rendering.
- [x] Keep click-time external URL handling so actual Zoom/http links still open through the OS.
- [x] Validate JavaScript syntax and Rust build.

### Review / Results

- Slack's original Zoom app-card rendering path is no longer modified by Zlack at render time.
- External links are still handled only from the capture-phase click listener, reducing risk to Slack's message/card rendering lifecycle.
- Verification passed: `rtk node --check src-tauri/preload.js` and `rtk cargo check --manifest-path src-tauri/Cargo.toml`.

## 2026-05-22 Restore non-rendering Zoom open fix

- [x] Keep intrusive Zoom card DOM/fallback button injection removed.
- [x] Restore external `window.open` interception so Slack app-card buttons that open Zoom programmatically route through Tauri shell.
- [x] Restore focused click fallback for visible Zoom Join buttons without anchors by constructing `zoommtg://` from nearby meeting text.
- [x] Validate JavaScript syntax, Rust check, and release build.
- [x] Reinstall and restart Zlack.

### Review / Results

- Fixed the current “button visible but click does nothing” case by handling non-anchor Zoom buttons again.
- No render-time DOM observer or injected fallback button remains, so this should not alter Slack's card layout.
- Verification passed: `rtk node --check src-tauri/preload.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk npm run tauri build`.
- Installed rebuilt app to `/Applications/Zlack.app` and relaunched it.

## 2026-05-27 Remove hidden WebView destroy memory saver

- [x] Update requirement: do not destroy/recreate Slack WebView after it is hidden or occluded for ~3 minutes.
- [x] Remove delayed memory-saver destroy timers and occlusion monitor from `src-tauri/src/main.rs`.
- [x] Keep only WebView background throttling by using `BackgroundThrottlingPolicy::Throttle`.
- [x] Validate Rust build and install rebuilt app to `/Applications/Zlack.app`.
- [x] Review results.

### Review / Results

- Removed app-owned hidden/occluded WebView unload behavior: no more 3-minute `window.destroy()` timer, no occlusion polling thread, and no delayed destroy scheduling on close/minimize/focus loss.
- Changed background throttling from `Disabled` to `Throttle`, so a hidden/background WebView is slowed but not fully suspended or destroyed.
- Remaining behavior: closing from the window/menu still hides the window; reopening restores the same WebView instead of reloading Slack.
- Verification passed: `rtk cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk npm run tauri build`.
- Installed rebuilt app to `/Applications/Zlack.app` and relaunched it.

## 2026-05-22 Reliable external URL opener

- [x] Confirm rendered Zoom buttons were not blocked by layout changes; click path was the failure.
- [x] Add Rust `open_external_url` command with scheme allowlist for http(s) and Zoom protocols.
- [x] Update preload external opener to invoke the Rust command before plugin JS fallbacks.
- [x] Validate JavaScript/Rust and rebuild/restart Zlack.

### Review / Results

- Replaced the silent JS shell-plugin dependency with an app-owned Tauri command that launches URLs via the OS opener.
- Kept render-time DOM mutation disabled; only click-time open handling changed.
- Verification passed: `rtk node --check src-tauri/preload.js`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk npm run tauri build`.
- Installed rebuilt app to `/Applications/Zlack.app` and relaunched it.

## 2026-05-22 WebView-level Zoom URL interception

- [x] Add Tauri WebView `on_navigation` hook to catch Zoom protocol/http URLs even if Slack bypasses preload click handlers.
- [x] Add `on_new_window` hook to route non-Slack new-window requests through the OS opener.
- [x] Reuse the Rust allowlisted external URL opener.
- [x] Validate Rust check and install generated app bundle.

### Review / Results

- Zoom navigation now gets intercepted below JavaScript/preload, at the Tauri WebView level.
- Non-Slack `window.open`/new-window requests are denied in-webview after being handed to the OS opener.
- `rtk cargo check --manifest-path src-tauri/Cargo.toml` passed; release binary/app bundle built, but DMG bundling failed after app generation, so the generated `.app` was installed directly for testing.
- Installed rebuilt app to `/Applications/Zlack.app` and relaunched it.

## 2026-05-22 Codex review: Windows Zoom opener escaping

- [x] Reproduce/identify the Codex review finding for external URL opening.
- [x] Replace Windows `cmd /C start` URL launch path with a shell-free opener so Zoom query strings keep `&` parameters intact.
- [x] Clean up clippy warnings introduced in the branch diff.
- [x] Validate Rust formatting, check, clippy, and tests.

### Review / Results

- Fixed Windows external URL launches by using `rundll32 url.dll,FileProtocolHandler <url>` instead of `cmd /C start`, avoiding shell parsing of Zoom URLs containing `&confno=...&pwd=...`.
- Verification passed: `rtk cargo check --manifest-path src-tauri/Cargo.toml`, `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`, and `rtk cargo test --manifest-path src-tauri/Cargo.toml`.

## 2026-05-23 Shortcut diagnostic popup removal

- [x] Locate shortcut diagnostic popup/notification paths.
- [x] Remove injected DOM overlay from native shortcut execution.
- [x] Remove physical shortcut fallback overlay while preserving fallback navigation.
- [x] Validate JavaScript and Rust build.
- [ ] Commit, open PR to main, and merge.

### Review / Results

- Removed shortcut diagnostic DOM overlays and the native shortcut-fired notification while keeping shortcut action execution and history fallback behavior intact.
- Verification passed: `rtk node --check src-tauri/shortcut_actions.js` and `rtk cargo check --manifest-path src-tauri/Cargo.toml`.

## 2026-05-26 Slack attachment send failure fix

- [x] Correct diagnosis: failure occurs after Slack UI file selection, not at the picker/menu entry point.
- [x] Remove the temporary native Attach File menu change.
- [x] Remove the unconditional fake `navigator.serviceWorker` implementation so Slack can use native Service Worker behavior or detect real absence.
- [x] Validate JavaScript syntax and Rust build.
- [x] Review results.

### Review / Results

- Removed the unconditional `navigator.serviceWorker` mock from `src-tauri/preload.js`. Slack now sees the real WebView Service Worker support instead of a fake object whose `ready` promise never resolves and whose `register()` always rejects.
- Reverted the temporary native `File > Attach File…` menu change because the failing path is Slack's normal in-app `+` attachment flow.
- Verification passed: `rtk node --check src-tauri/preload.js`, `rtk node --check src-tauri/shortcut_actions.js`, `rtk cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`, and `rtk npm run tauri build`.

## 2026-05-27 Slack file download button fix

- [x] Investigate why uploaded file download buttons appear to do nothing.
- [x] Confirm Tauri/Wry cancels WebKit download navigations on macOS when no download handler is registered.
- [x] Add a main webview `on_download` handler that allows downloads and logs requested/finished events.
- [x] Add overlay-button handling for Slack file URLs such as `https://sendbird.slack.com/files/...` opened through `window.open` or blank popup assignment.
- [x] Add direct click fallback for non-anchor Slack download overlay buttons.
- [x] Add Slack Desktop-like translucent Zlack download toast for fallback clicks and native download events.
- [x] Validate JavaScript, Rust build, and release bundle.
- [x] Review results.

### Review / Results

- Added `DownloadEvent` handling in `src-tauri/src/main.rs` so WebKit download requests are allowed instead of cancelled.
- Added Slack file URL handling in `src-tauri/preload.js` for workspace file links like `https://sendbird.slack.com/files/...`, direct Slack file/CDN hosts, and blank-popup `window.open` flows used by overlay buttons.
- Added a direct capture-phase fallback for non-anchor download overlay buttons: when a control labeled download or the leftmost file action is clicked, Zlack searches the nearby Slack file card for a file URL and starts the WebView download itself.
- Added a Slack Desktop-like translucent in-app download toast in `src-tauri/preload.js`, plus a native `DownloadEvent` bridge from `src-tauri/src/main.rs` so Zlack visibly responds on both fallback-click and WebKit download paths.
- Logs requested and finished download URLs to help distinguish future routing problems from successful downloads.
- Verification passed: `rtk node --check src-tauri/preload.js`, `rtk cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`, and `rtk npm run tauri build`.

## 2026-05-27 Slack typing pane black render fix

- [x] Investigate intermittent black Slack message pane where only the typing indicator remains visible.
- [x] Identify WebView compositor suspension as the most likely cause for a virtualized Slack message list black layer.
- [x] Change Slack WebView background throttling from `Suspend` to `Disabled`.
- [x] Validate Rust build and release bundle.
- [x] Review results.

### Review / Results

- Changed `create_main_window()` from `BackgroundThrottlingPolicy::Suspend` to `BackgroundThrottlingPolicy::Disabled` so Slack's virtualized/composited message pane is not suspended while the window is backgrounded or covered.
- This trades a bit more background activity for avoiding stale black compositor layers where only the typing indicator repaints.
- Verification passed: `rtk node --check src-tauri/preload.js`, `rtk cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, `rtk cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`, and `rtk npm run tauri build`.

## 2026-05-27 PDF download/toast correction

- [x] Correct the requirement: PDF file-card body clicks should keep Slack's viewer behavior; only explicit download controls should download.
- [x] Remove the targeted PDF file-card body click interceptor from `src-tauri/preload.js`.
- [x] Keep the smaller Slack-style toast.
- [x] Prevent the toast's `View all downloads` button from being captured by the generic download-control interceptor.
- [x] Change download kickoff from `_self` navigation to a hidden iframe target so viewer downloads do not show a redirect screen in the main WebView.
- [x] Change `View all downloads` to open the OS Downloads folder in Finder and force Finder activation with `open -a Finder`.
- [x] Build/relaunch/install Zlack from `/Applications/Zlack.app`.
- [x] Review results.

### Review / Results

- Removed the PDF card/body click path so Slack can open the viewer from normal body clicks.
- Reduced the toast from a large 704px-wide card to a compact 380px-wide translucent notification with smaller icon, copy, and link sizing.
- Added a Tauri `open_downloads_folder` command and wired `View all downloads` to the local Downloads folder so the action opens Finder on macOS; the button is excluded from download-control interception and also stops propagation before calling the command.
- Download kickoff now targets a hidden iframe instead of `_self`, avoiding visible redirect screens in the main Slack viewer route.
- Verification passed: `rtk node --check src-tauri/preload.js`, `rtk cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk npm run tauri build`.
- Installed rebuilt app to `/Applications/Zlack.app` and relaunched it.

## 2026-05-27 Reveal downloaded file in Finder

- [x] Update requirement: `View all downloads` should reveal and highlight the downloaded file, not only open the Downloads folder.
- [x] Store the most recent download destination/path from Tauri download events.
- [x] Change Finder opening to reveal the downloaded file when it exists, falling back to the Downloads folder otherwise.
- [x] Build and install to `/Applications/Zlack.app`.
- [x] Review results.

### Review / Results

- Added native tracking of the latest Tauri download path from `DownloadEvent::Requested` / `DownloadEvent::Finished`.
- `View all downloads` now opens the latest completed download path through Finder `reveal`, which selects/highlights the file; if the file is missing or outside Downloads, it falls back to the Downloads folder.
- Verification passed: `rtk cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `rtk cargo check --manifest-path src-tauri/Cargo.toml`, and `rtk npm run tauri build`.
- Installed rebuilt app to `/Applications/Zlack.app` and relaunched it.
- Finder reveal command verified with a concrete file: Finder selection resolved to `/Users/jinku/Downloads/07242024-FSA.pdf`.
