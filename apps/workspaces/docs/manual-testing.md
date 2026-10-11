# Manual test procedures

Automated: `scripts/smoke-test.ps1` (an item tied to another desktop is hidden, one tied to the
current desktop stays visible, a new file joins the current desktop, `--quit` restores, kill
recovery). It needs the module lock: quit a running Desqu or Workspaces first. Everything below
needs a human. Record result + date and the Windows build for each case; the project has only been
exercised on build 26300.

Prepare two or more virtual desktops (Win+Tab, New desktop). Use spare files on the desktop.

0. First run (delete the Workspaces config.json first, with the app closed): expect the question
   "Where do your desktop items belong?". Pick desktop 1. Expect every item visible on desktop 1
   and hidden on desktop 2. Skip variant: "Leave them on every desktop" keeps all visible.
1. Start Workspaces. Expect one card per virtual desktop, the current one marked "You are here".
2. Items tab: tie a spare file to desktop 1. Expect it still visible on desktop 1.
3. Switch to desktop 2 (Win+Ctrl+Right). Also switch to a desktop with no open windows and by
   clicking the desktop in Win+Tab. Expect the file hidden; back on desktop 1 it is visible
   again. Record how long each change takes.
4. Tie another file to desktop 2. Expect it visible only there.
5. Create a new file on desktop 2 while Workspaces runs. Expect it to appear under desktop 2 on the
   Desktops tab and to be hidden on desktop 1. Turn off "New files join the desktop I am on" and
   repeat: the file shows on every desktop.
6. Choose "Every desktop" for a tied file. Expect it visible on all desktops.
7. Rename a desktop in Windows; reopen the window. Expect the new name.
8. Remove a desktop that owns items. Expect "Desktop removed" and its items visible everywhere.
9. End the app in Task Manager while a file is hidden; start it again. Expect the file back.
10. Restart Explorer while a file is hidden. Record what happens.
11. With only one desktop: expect the "No virtual desktops yet" hint and nothing hidden.
12. In Desqu with Vials on and the same file in a vial and tied to another desktop: take it out of
    the vial; expect it to stay hidden on the other desktops only.
13. Tie a large number of items (hundreds) to desktop 2, then switch to desktop 1. Expect all of them
    hidden within a few seconds, none left on screen. Check the Settings tab for a problem message.
14. Windows 10 22H2 / 11 23H2 / 24H2: run cases 1-4 and note which fail.
