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
2. On desktop 1 select a spare icon and press Ctrl+X, then Ctrl+V on the desktop (or let a new file
   join). Expect it tied to desktop 1 (the card count goes up) and still visible there.
3. Switch to desktop 2 (Win+Ctrl+Right). Also switch to a desktop with no open windows and by
   clicking the desktop in Win+Tab. Expect the file hidden; back on desktop 1 it is visible
   again. Record how long each change takes.
4. On desktop 2 select an icon that belongs to desktop 1 (it is hidden there: show desktop 1, press
   Ctrl+X on the icon, switch to desktop 2, press Ctrl+V on the desktop). Expect it visible only on
   desktop 2 afterwards, with no Explorer dialog or error. Also check: cut then Esc / copy (Ctrl+C)
   instead of cut does not move anything; Ctrl+V with the Desqu window focused does nothing.
5. Create a new file on desktop 2 while Workspaces runs. Expect it to appear under desktop 2 on the
   Desktops tab and to be hidden on desktop 1. Turn off "New files join the desktop I am on" and
   repeat: the file shows on every desktop.
6. Press "Show on every desktop" on a card with items. Expect them visible on all desktops.
7. Rename a desktop in Windows; reopen the window. Expect the new name.
8. Remove a desktop that owns items. Expect "Desktop removed" and its items visible everywhere.
9. End the app in Task Manager while a file is hidden; start it again. Expect the file back.
10. Restart Explorer while a file is hidden. Record what happens.
11. With only one desktop: expect the "No virtual desktops yet" hint and nothing hidden.
12. In Desqu with Vials on and the same file in a vial and tied to another desktop: take it out of
    the vial; expect it to stay hidden on the other desktops only.
13. Tie a large number of items (hundreds) to desktop 2, then switch to desktop 1. Expect all of them
    hidden within a few seconds, none left on screen. Check the Settings tab for a problem message.
14. If anything stays visible: Settings tab, "Last check" line (how many items are hidden and how many
    could not be), and the log file %APPDATA%\io.github.wolfskii.<desqu|workspaces>\logspp.log.
15. Windows 10 22H2 / 11 23H2 / 24H2: run cases 1-4 and note which fail.
