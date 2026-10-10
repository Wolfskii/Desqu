# Manual test procedures

The automated smoke test (`pwsh apps/vials/scripts/smoke-test.ps1`) covers parking, restore, the
vial window styles, `--quit` and crash recovery with one temporary file. The cases below need a
human and a real desktop. Record result + date next to each case.

Use a spare file or shortcut on the desktop, not something you cannot afford to lose track of:
Vials never moves or deletes files, but items are hidden while a vial holds them.

## Basics

1. Start Vials, **Vials** tab → add a vial. Expect a window on the desktop, below other windows.
2. Drag an item from the **Items** tab into the vial. Expect it gone from the loose desktop and
   shown in the vial with its icon.
3. Double click it in the vial. Expect it to open.
4. Press × on it. Expect it back on the desktop where it was.
5. Drag the vial by its title bar; restart Vials. Expect the same place and size.
6. Collapse the vial. Expect only the title bar to remain; expand restores it.
7. Alt+Tab and the taskbar must not list the vial windows; clicking one must not steal focus.

## Safety

8. With an item hidden, end Vials from Task Manager. Expect the item to stay hidden. Start Vials
   again (item removed from every vial first). Expect it back on the desktop.
9. Quit Vials from the tray. Expect all hidden items back, exactly where they were.
10. Restart Explorer (Task Manager → Windows Explorer → Restart) while a vial holds items.
    Record what happens to the hidden items and the vial windows.
11. Win+D (show desktop). Record whether the vials stay visible.
12. Change display scaling (125% / 150%) and move a vial to a second monitor if you have one.
    Expect the vial to keep a sensible size and position.

## Rules

13. Add a rule "extension .txt → vial X", turn auto-sort on, create a new .txt on the desktop.
    Expect it in vial X within a few seconds (it may show on the desktop for ~4 s first).
14. Take that file out of the vial by hand, create another .txt. Expect only the new one sorted.

## Inside Desqu

15. Turn Vials on in Desqu with the standalone app running. Expect an offer to take over; accept.
    Expect vials and settings unchanged and the standalone autostart entry disabled.
16. Turn Vials off in Desqu. Expect vial windows closed and every hidden item restored.
