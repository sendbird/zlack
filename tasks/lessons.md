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
