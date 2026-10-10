# Manual test procedures

Automated: `scripts/smoke-test.ps1` (hide on the current desktop, no hide for another desktop,
`--quit` restore, kill recovery) and `cargo test -p workspaces-module -- --ignored real_running`
(find, close and relaunch an app). Everything below needs a human. Record result + date and the
Windows build for each case; the project has only been exercised on build 26300.

Prepare two or more virtual desktops (Win+Tab → New desktop). Use a spare file on the desktop.

1. Start Workspaces. Expect one card per virtual desktop, the current one marked "You are here".
2. On desktop 1 open an app and an Explorer folder; press "Save open windows". Expect both listed.
3. Switch on "Hide some desktop icons", choose the spare file. Expect it hidden on desktop 1.
4. Switch to desktop 2 (Win+Ctrl+Right). Expect the file visible; back on desktop 1 hidden again.
   Record how long the change takes.
5. Close the saved app and folder, press "Launch here". Expect both to open on desktop 1.
6. On desktop 2 the "Launch here" button of desktop 1 is disabled; the message says to switch first.
7. Rename a desktop in Windows; reopen the window. Expect the new name.
8. Remove a desktop that has a workspace. Expect "Desktop removed" with a Delete workspace button.
9. Turn on "Refresh a workspace when I leave its desktop", open another app, switch away and back.
   Expect the saved list to include it.
10. End the app in Task Manager while the file is hidden; start it again. Expect the file back.
11. Restart Explorer while the file is hidden. Record what happens.
12. With only one desktop: expect the "No virtual desktops yet" hint.
13. In Desqu with Vials on and the same file in a vial and hidden by a workspace: take it out of the
    vial; expect it to stay hidden on the workspace's desktop only.
14. Windows 10 22H2 / 11 23H2 / 24H2: run cases 1–5 and note which fail.
