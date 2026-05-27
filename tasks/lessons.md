# Lessons

## 2026-05-21 Slack deeplink correction

- Do not synthesize undocumented Slack web-client thread routes from permalinks unless verified in the running app.
- Prefer preserving Slack-owned redirect semantics by moving workspace-hosted `/app_redirect` and `/archives/...` links onto `https://app.slack.com/app_redirect` so Slack's router resolves the final route.
- After a link-handling fix, verify both "does not open the browser" and "lands on a usable Slack screen"; avoiding the external browser alone is insufficient.

## 2026-05-21 Slack permalink native redirect fix

- For Slack workspace permalinks, do not synthesize undocumented `app.slack.com/client/.../thread/...` routes; the web client can show a blank shell.
- Preserve the workspace permalink and add `no_native_redirect=1` so Slack's supported web permalink flow handles the route without launching the native app.

## 2026-05-22 Slack app card rendering correction

- If a Slack app card originally rendered correctly, do not add global browser API hooks such as `window.open` wrappers or broad DOM observers to fix click behavior; those can perturb Slack's render lifecycle.
- Prefer click-time interception of concrete anchors/events, and preserve Slack's DOM/rendering path unless the render bug is independently proven.
- When a visual regression appears after a link-handling patch, first remove the invasive patch and restore original rendering before adding fallback UI.

## 2026-05-22 Zoom card diagnosis

- Do not assume a broken-looking third-party app card is caused by local rendering code; first compare an active vs ended item and verify whether the third-party service intentionally removed actions.
- For Slack app cards, avoid render-time DOM mutation as a first fix. Prefer click-time interception of anchors, `window.open`, or narrowly matched buttons so Slack's layout remains untouched.

## 2026-05-26 Slack attachment failure stage

- Do not infer that "file attachment fails" means the attach button or menu path is missing; first identify the failing stage: picker open, upload preview, send API, or final message post.
- If the user provides Slack's "Couldn't send message" modal, treat it as a send/upload pipeline failure after file selection, not a UI entry-point problem.
- For Slack Web wrapper bugs, avoid adding shortcut/menu workarounds until the normal in-app UI path has been verified.

## 2026-05-27 Slack file download overlays

- Slack file download overlay buttons may use programmatic `window.open`, blank popup assignment, or workspace `/files/...` permalinks rather than direct anchor downloads.
- A WebView download handler alone is not enough if the app lets Slack-owned file popups escape the configured main webview. Route Slack file URLs back through the main webview download flow.
- When the user shows Slack Desktop behavior, do not infer Zlack showed the same UI; ask/confirm whether the screenshot is reference behavior or current app behavior before diagnosing.

## 2026-05-27 Download toast sizing and action correction

- Match Slack desktop toast proportions, not just its visual style; oversized fallback UI can be functionally correct but still feel broken.
- If a toast action says `View all downloads`, open the OS downloads location when the user expects Finder, not Slack's internal downloads route.
- Do not override Slack file-card body clicks when the cursor/label indicates preview or viewer; only explicit download controls should download.
- Do not treat Finder as verified merely because a Finder window exists; verify that Finder is frontmost and the intended folder is shown.

## 2026-05-27 Hidden WebView memory policy correction

- Do not destroy/recreate Slack WebView as a memory saver unless the user explicitly accepts reload cost; hidden/occluded unload causes expensive Slack reload and state loss.
- Treat WebView lifecycle and WebView background throttling as separate controls: app-owned `destroy()` unloads the view, `Suspend` fully pauses background tasks, and `Throttle` only slows processing.
