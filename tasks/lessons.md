# Lessons

## 2026-05-21 Slack deeplink correction

- Do not synthesize undocumented Slack web-client thread routes from permalinks unless verified in the running app.
- Prefer preserving Slack-owned redirect semantics by moving workspace-hosted `/app_redirect` and `/archives/...` links onto `https://app.slack.com/app_redirect` so Slack's router resolves the final route.
- After a link-handling fix, verify both "does not open the browser" and "lands on a usable Slack screen"; avoiding the external browser alone is insufficient.

## 2026-05-21 Slack permalink native redirect fix

- For Slack workspace permalinks, do not synthesize undocumented `app.slack.com/client/.../thread/...` routes; the web client can show a blank shell.
- Preserve the workspace permalink and add `no_native_redirect=1` so Slack's supported web permalink flow handles the route without launching the native app.
