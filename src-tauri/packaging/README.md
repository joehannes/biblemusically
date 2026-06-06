Playwright-driven Midjourney integration

The legacy `midjourney-proxy` packaging and autostart helpers have been removed.
Use the visible Playwright-based workflow to perform interactive login and capture a
`mj_cookie` via the Settings → Capture session button in the application.

Files in this folder now include a Playwright generator helper and simple installers
that inform about the new flow.
